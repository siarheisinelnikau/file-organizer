use crate::{
    engine, filesystem as disk,
    journal::{Entry, Journal},
    model::*,
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use notify::{RecursiveMode, Watcher};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

pub enum Command {
    Save(Config),
    Preview(Config),
    Apply {
        generation: u64,
        indices: Vec<usize>,
    },
    Watch {
        config: Config,
        automatic: bool,
    },
    Pause,
    Changed,
    WatchError(String),
    Undo(String),
    Recover,
    Shutdown,
}

pub enum Event {
    Loaded(Config),
    Plan { generation: u64, plan: Plan },
    History(Vec<Entry>, usize),
    Message(String),
    Error(String),
    Busy(bool),
    Watching(bool, bool),
    Stopped,
}

pub struct Worker {
    pub commands: SyncSender<Command>,
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    shutdown_config: Arc<Mutex<Option<Config>>>,
}

impl Worker {
    pub fn start(directory: PathBuf, repaint: impl Fn() + Send + Sync + 'static) -> Self {
        let (commands, receiver) = mpsc::sync_channel(8);
        let (sender, events) = mpsc::sync_channel(32);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let shutdown_config = Arc::new(Mutex::new(None));
        let worker_shutdown_config = shutdown_config.clone();
        let callback_commands = commands.clone();
        thread::spawn(move || {
            let emit = |event| {
                let _ = sender.send(event);
                repaint();
            };
            if let Err(e) = run(
                directory,
                receiver,
                callback_commands,
                &emit,
                &worker_cancel,
                &worker_shutdown_config,
            ) {
                emit(Event::Error(format!("{e:#}")));
            }
            emit(Event::Stopped);
        });
        Self {
            commands,
            events,
            cancel,
            shutdown_config,
        }
    }

    /// Shutdown cannot be lost when notification traffic fills the bounded command queue.
    pub fn shutdown(&self, save: Option<Config>) {
        if let Ok(mut slot) = self.shutdown_config.lock() {
            *slot = save;
        }
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.commands.try_send(Command::Shutdown);
    }
}

pub struct Stability {
    samples: HashMap<String, (Stamp, Instant)>,
}

impl Default for Stability {
    fn default() -> Self {
        Self::new()
    }
}
impl Stability {
    pub fn new() -> Self {
        Self {
            samples: HashMap::new(),
        }
    }

    pub fn ready(
        &mut self,
        files: Vec<FileFact>,
        now: Instant,
        settle: Duration,
    ) -> (Vec<FileFact>, usize) {
        let mut next = HashMap::new();
        let mut ready = Vec::new();
        let mut waiting = 0;
        for fact in files {
            let key = path_key(&fact.path);
            let since = self
                .samples
                .get(&key)
                .filter(|(stamp, _)| stamp == &fact.stamp)
                .map(|(_, t)| *t)
                .unwrap_or(now);
            next.insert(key, (fact.stamp.clone(), since));
            let old_enough =
                disk::now_ns().saturating_sub(fact.stamp.modified_ns) >= settle.as_nanos();
            if now.duration_since(since) >= settle && old_enough {
                ready.push(fact);
            } else {
                waiting += 1;
            }
        }
        self.samples = next;
        (ready, waiting)
    }
}

fn make_plan(config: &Config, journal: &Journal, stability: &mut Stability) -> Result<Plan> {
    let scanned = disk::scan(config)?;
    let count = scanned.files.len();
    let eligible = journal.unsuppressed(scanned.files)?;
    let (ready, waiting) = stability.ready(
        eligible,
        Instant::now(),
        Duration::from_secs(config.settle_seconds.clamp(2, 60)),
    );
    // Use the same canonical path representation as scanned files, notably \\?\ paths on Windows.
    let mut normalized = config.clone();
    for rule in normalized
        .rules
        .iter_mut()
        .filter(|r| r.enabled && r.action != Action::Rename)
    {
        let checked = disk::inside(
            &PathBuf::from(&rule.destination).join(".validation"),
            &config.destination_roots,
        )?;
        rule.destination = checked
            .parent()
            .context("Missing output directory")?
            .to_string_lossy()
            .into_owned();
    }
    let mut plan = engine::plan(&normalized, &ready, &scanned.occupied, disk::now_ns());
    plan.scanned = count;
    plan.waiting = waiting;
    plan.notices.extend(scanned.notices);
    if plan.operations.len() > 2_000 {
        plan.operations.truncate(2_000);
        plan.notices
            .push("Plan limited to 2000 actions. Preview again after applying this batch.".into());
    }
    plan.notices.truncate(200);
    Ok(plan)
}

fn history(journal: &Journal, emit: &impl Fn(Event)) -> Result<()> {
    emit(Event::History(
        journal.entries(false)?,
        journal.entries(true)?.len(),
    ));
    Ok(())
}

fn apply(
    journal: &Journal,
    config: &Config,
    operations: Vec<Operation>,
    emit: &impl Fn(Event),
    cancel: &AtomicBool,
) -> Result<()> {
    let total = operations.len();
    let mut completed = 0;
    let mut failure = None;
    for operation in operations {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match journal.execute(&operation, config) {
            Ok(()) => completed += 1,
            Err(e) => {
                failure = Some(e.context(format!("Stopped at {}", operation.source.display())));
                break;
            }
        }
    }
    emit(Event::Message(format!(
        "Completed {completed} of {total} actions. Review Activity for details."
    )));
    history(journal, emit)?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

fn run(
    directory: PathBuf,
    receiver: Receiver<Command>,
    commands: SyncSender<Command>,
    emit: &impl Fn(Event),
    cancel: &AtomicBool,
    shutdown_config: &Mutex<Option<Config>>,
) -> Result<()> {
    disk::check_path(&directory)?;
    fs::create_dir_all(&directory)?;
    disk::check_path(&directory.join("instance.lock-file"))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("instance.lock-file"))?;
    lock.try_lock_exclusive()
        .context("FileOrganizer is already using this data directory")?;
    let journal = Journal::open(&directory.join("organizer.db"))?;
    let mut config = journal.config()?;
    emit(Event::Loaded(config.clone()));
    history(&journal, emit)?;
    let error_path = directory.join("update-error.txt");
    if error_path.try_exists()? {
        disk::check_path(&error_path)?;
        if fs::metadata(&error_path)?.len() <= 64 * 1024 {
            emit(Event::Error(format!(
                "Previous update failed: {}",
                fs::read_to_string(error_path)?
            )));
        }
    }
    let mut watcher = None;
    let mut automatic = false;
    let mut due: Option<Instant> = None;
    let mut stability = Stability::new();
    let mut current = Plan::default();
    let mut generation = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            if let Some(save) = shutdown_config
                .lock()
                .map_err(|_| anyhow::anyhow!("Shutdown settings lock failed"))?
                .take()
            {
                journal.save_config(&save)?;
            }
            break;
        }
        let next = if let Some(deadline) = due {
            match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        };
        if matches!(next, Some(Command::Shutdown)) {
            if let Some(save) = shutdown_config
                .lock()
                .map_err(|_| anyhow::anyhow!("Shutdown settings lock failed"))?
                .take()
            {
                journal.save_config(&save)?;
            }
            break;
        }
        if matches!(next, Some(Command::Changed)) {
            if watcher.is_some() {
                due =
                    Some(Instant::now() + Duration::from_secs(config.settle_seconds.clamp(2, 60)));
            }
            continue;
        }
        if matches!(next, Some(Command::Pause)) {
            watcher = None;
            automatic = false;
            due = None;
            stability = Stability::new();
            emit(Event::Watching(false, false));
            continue;
        }
        emit(Event::Busy(true));
        let outcome = (|| -> Result<()> {
            match next {
                Some(Command::Save(value)) => {
                    watcher = None;
                    automatic = false;
                    due = None;
                    journal.save_config(&value)?;
                    config = value;
                    current = Plan::default();
                    generation += 1;
                    emit(Event::Plan {
                        generation,
                        plan: current.clone(),
                    });
                    emit(Event::Watching(false, false));
                    emit(Event::Message(
                        "Configuration saved. Monitoring is paused.".into(),
                    ));
                }
                Some(Command::Watch {
                    config: value,
                    automatic: auto,
                }) => {
                    ensure!(
                        journal.entries(true)?.is_empty(),
                        "Recover unfinished operations first"
                    );
                    disk::scan(&value)?;
                    watcher = None;
                    let callback = commands.clone();
                    let mut new_watcher = notify::recommended_watcher(
                        move |event: notify::Result<notify::Event>| {
                            let command = match event {
                                Ok(event) if matches!(event.kind, notify::EventKind::Access(_)) => {
                                    return;
                                }
                                Ok(_) => Command::Changed,
                                Err(e) => Command::WatchError(e.to_string()),
                            };
                            // A full queue already wakes the worker; a periodic reconciliation scan covers coalesced notifications.
                            let _ = callback.try_send(command);
                        },
                    )?;
                    for source in &value.source_roots {
                        new_watcher.watch(
                            &disk::root(source)?,
                            if value.recursive {
                                RecursiveMode::Recursive
                            } else {
                                RecursiveMode::NonRecursive
                            },
                        )?;
                    }
                    journal.save_config(&value)?;
                    config = value;
                    automatic = auto;
                    watcher = Some(new_watcher);
                    stability = Stability::new();
                    due = Some(Instant::now());
                    emit(Event::Watching(true, automatic));
                }
                Some(Command::Preview(value)) => {
                    watcher = None;
                    automatic = false;
                    emit(Event::Watching(false, false));
                    journal.save_config(&value)?;
                    config = value;
                    current = make_plan(&config, &journal, &mut stability)?;
                    generation += 1;
                    due = (current.waiting > 0).then(|| {
                        Instant::now() + Duration::from_secs(config.settle_seconds.clamp(2, 60))
                    });
                    emit(Event::Plan {
                        generation,
                        plan: current.clone(),
                    });
                }
                Some(Command::Apply {
                    generation: requested,
                    indices,
                }) => {
                    ensure!(
                        requested == generation,
                        "Preview changed; review the current plan"
                    );
                    let operations = indices
                        .into_iter()
                        .map(|i| {
                            current
                                .operations
                                .get(i)
                                .cloned()
                                .context("Invalid preview selection")
                        })
                        .collect::<Result<Vec<_>>>()?;
                    apply(&journal, &config, operations, emit, cancel)?;
                    current = Plan::default();
                    generation += 1;
                    due = None;
                    emit(Event::Plan {
                        generation,
                        plan: current.clone(),
                    });
                }
                Some(Command::Undo(id)) => {
                    watcher = None;
                    automatic = false;
                    due = None;
                    emit(Event::Watching(false, false));
                    let result = journal.undo(&id);
                    history(&journal, emit)?;
                    result?;
                    emit(Event::Message("Operation undone. It will not be automatically repeated for the same file version.".into()));
                }
                Some(Command::Recover) => {
                    watcher = None;
                    automatic = false;
                    due = None;
                    emit(Event::Watching(false, false));
                    for message in journal.recover() {
                        emit(Event::Message(message));
                    }
                    history(&journal, emit)?;
                }
                Some(Command::WatchError(error)) => anyhow::bail!("Folder watcher failed: {error}"),
                None => {
                    current = make_plan(&config, &journal, &mut stability)?;
                    generation += 1;
                    emit(Event::Plan {
                        generation,
                        plan: current.clone(),
                    });
                    if automatic && !current.operations.is_empty() {
                        apply(&journal, &config, current.operations.clone(), emit, cancel)?;
                        current = Plan::default();
                        generation += 1;
                        emit(Event::Plan {
                            generation,
                            plan: current.clone(),
                        });
                        ensure!(
                            journal.entries(true)?.is_empty(),
                            "Automatic organization paused after an incomplete action"
                        );
                    }
                    due = if current.waiting > 0 {
                        Some(
                            Instant::now()
                                + Duration::from_secs(config.settle_seconds.clamp(2, 60)),
                        )
                    } else if watcher.is_some() {
                        Some(Instant::now() + Duration::from_secs(60))
                    } else {
                        None
                    };
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = outcome {
            watcher = None;
            automatic = false;
            due = None;
            emit(Event::Watching(false, false));
            emit(Event::Error(format!("{e:#}")));
            let _ = history(&journal, emit);
        }
        emit(Event::Busy(false));
    }
    drop(watcher);
    drop(journal);
    drop(lock);
    Ok(())
}
