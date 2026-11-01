use crate::{
    ai, credentials, engine, filesystem as disk,
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
    Appearance {
        language: crate::localization::Language,
        theme: crate::localization::Theme,
    },
    Preview(Config),
    Reclassify(Config),
    CheckAi(crate::model::AiSettings),
    SaveAiToken(AiSettings, zeroize::Zeroizing<String>),
    RemoveAiToken(AiSettings),
    CancelAi,
    AiProgress {
        id: u64,
        done: usize,
        total: usize,
    },
    AiFinished {
        id: u64,
        result: std::result::Result<AiCompletion, String>,
    },
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
pub enum AiCompletion {
    Plan(ai::Report),
    Connection(String),
}

pub enum Event {
    Loaded(Config),
    AppearanceSaved {
        language: crate::localization::Language,
        theme: crate::localization::Theme,
    },
    Plan {
        generation: u64,
        plan: Plan,
    },
    History(Vec<Entry>, usize),
    Message(String),
    Error(String),
    Busy(bool),
    Watching(bool, bool),
    Stopped,
    AiProgress {
        active: bool,
        done: usize,
        total: usize,
    },
    AiConnection(String),
}

pub struct Worker {
    pub commands: SyncSender<Command>,
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    shutdown_config: Arc<Mutex<Option<Config>>>,
}

impl Worker {
    pub fn start(directory: PathBuf, repaint: impl Fn() + Send + Sync + 'static) -> Self {
        Self::start_with_ai(directory, repaint, Arc::new(ai::HubClient))
    }
    #[doc(hidden)]
    pub fn start_with_ai(
        directory: PathBuf,
        repaint: impl Fn() + Send + Sync + 'static,
        client: Arc<dyn ai::CompletionClient>,
    ) -> Self {
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
                client,
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
    ai_client: Arc<dyn ai::CompletionClient>,
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
    let mut journal = Journal::open(&directory.join("organizer.db"))?;
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
    let mut ai_task: Option<(u64, Arc<AtomicBool>)> = None;
    let ai_running = Arc::new(AtomicBool::new(false));
    let mut task_sequence = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            cancel_ai(&mut ai_task, emit);
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
            cancel_ai(&mut ai_task, emit);
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
            cancel_ai(&mut ai_task, emit);
            watcher = None;
            automatic = false;
            due = None;
            stability = Stability::new();
            emit(Event::Watching(false, false));
            emit(Event::Busy(false));
            continue;
        }
        emit(Event::Busy(true));
        let mut request_plan = false;
        let outcome = (|| -> Result<()> {
            match next {
                Some(Command::Appearance { language, theme }) => {
                    config.language = language;
                    config.theme = theme;
                    journal.save_config(&config)?;
                    emit(Event::AppearanceSaved { language, theme });
                }
                Some(Command::Save(value)) => {
                    cancel_ai(&mut ai_task, emit);
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
                    cancel_ai(&mut ai_task, emit);
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
                Some(Command::Reclassify(value)) => {
                    cancel_ai(&mut ai_task, emit);
                    journal.clear_ai_decisions()?;
                    watcher = None;
                    automatic = false;
                    due = None;
                    emit(Event::Watching(false, false));
                    journal.save_config(&value)?;
                    config = value;
                    request_plan = true;
                }
                Some(Command::Preview(value)) => {
                    cancel_ai(&mut ai_task, emit);
                    watcher = None;
                    automatic = false;
                    emit(Event::Watching(false, false));
                    journal.save_config(&value)?;
                    config = value;
                    request_plan = true;
                }
                Some(Command::CancelAi) => {
                    cancel_ai(&mut ai_task, emit);
                    watcher = None;
                    automatic = false;
                    due = None;
                    emit(Event::Watching(false, false));
                    emit(Event::Message("AI analysis cancelled. No AI file actions will be applied. An in-flight hub request may take up to 45 seconds to finish.".into()));
                }
                Some(Command::SaveAiToken(settings, token)) => {
                    credentials::store(&ai::endpoint(&settings)?, &token)?;
                    emit(Event::AiConnection(
                        "Token saved in Windows Credential Manager.".into(),
                    ));
                }
                Some(Command::RemoveAiToken(settings)) => {
                    credentials::remove(&ai::endpoint(&settings)?)?;
                    emit(Event::AiConnection("Saved token removed. An explicitly configured environment token may still be used.".into()));
                }
                Some(Command::CheckAi(settings)) => {
                    watcher = None;
                    automatic = false;
                    due = None;
                    emit(Event::Watching(false, false));
                    ensure!(ai_task.is_none(), "Cancel the current AI analysis first");
                    let cancelled = Arc::new(AtomicBool::new(false));
                    task_sequence += 1;
                    let id = task_sequence;
                    spawn_ai(
                        id,
                        None,
                        Some(settings),
                        cancelled.clone(),
                        ai_running.clone(),
                        commands.clone(),
                        ai_client.clone(),
                    )?;
                    ai_task = Some((id, cancelled));
                    emit(Event::AiProgress {
                        active: true,
                        done: 0,
                        total: 0,
                    });
                }
                Some(Command::AiProgress { id, done, total }) => {
                    if ai_task.as_ref().is_some_and(|t| t.0 == id) {
                        emit(Event::AiProgress {
                            active: true,
                            done,
                            total,
                        });
                    }
                }
                Some(Command::AiFinished { id, result }) => {
                    if ai_task.as_ref().is_some_and(|t| t.0 == id) {
                        ai_task = None;
                        emit(Event::AiProgress {
                            active: false,
                            done: 0,
                            total: 0,
                        });
                        match result.map_err(anyhow::Error::msg)? {
                            AiCompletion::Connection(status) => emit(Event::AiConnection(status)),
                            AiCompletion::Plan(report) => {
                                journal.save_ai_decisions(&report.cache)?;
                                current = report.plan;
                                generation += 1;
                                if automatic {
                                    let approved: Vec<_> = current
                                        .operations
                                        .iter()
                                        .filter(|o| ai::recommended(o, &config.ai))
                                        .cloned()
                                        .collect();
                                    if !approved.is_empty() {
                                        apply(&journal, &config, approved, emit, cancel)?;
                                    }
                                    current
                                        .operations
                                        .retain(|o| !ai::recommended(o, &config.ai));
                                }
                                emit(Event::Plan {
                                    generation,
                                    plan: current.clone(),
                                });
                                due = if current.waiting > 0 {
                                    Some(
                                        Instant::now()
                                            + Duration::from_secs(
                                                config.settle_seconds.clamp(2, 60),
                                            ),
                                    )
                                } else if watcher.is_some() {
                                    Some(Instant::now() + Duration::from_secs(60))
                                } else {
                                    None
                                };
                            }
                        }
                    }
                }
                Some(Command::Apply {
                    generation: requested,
                    indices,
                }) => {
                    ensure!(
                        ai_task.is_none(),
                        "Wait for AI analysis or cancel it before applying changes"
                    );
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
                    watcher = None;
                    automatic = false;
                    due = None;
                    emit(Event::Watching(false, false));
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
                    cancel_ai(&mut ai_task, emit);
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
                    cancel_ai(&mut ai_task, emit);
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
                    if ai_task.is_none() {
                        request_plan = true;
                    } else {
                        due = None;
                    }
                }
                _ => {}
            }
            if request_plan {
                if config.mode == PlanningMode::AiHub {
                    let job = prepare_ai(&config, &journal, &mut stability)?;
                    current = job.base.clone();
                    generation += 1;
                    emit(Event::Plan {
                        generation,
                        plan: current.clone(),
                    });
                    if job.facts.is_empty() {
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
                    } else {
                        let cancelled = Arc::new(AtomicBool::new(false));
                        task_sequence += 1;
                        let id = task_sequence;
                        spawn_ai(
                            id,
                            Some(job),
                            None,
                            cancelled.clone(),
                            ai_running.clone(),
                            commands.clone(),
                            ai_client.clone(),
                        )?;
                        ai_task = Some((id, cancelled));
                        due = None;
                        emit(Event::AiProgress {
                            active: true,
                            done: 0,
                            total: 0,
                        });
                    }
                } else {
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
            }
            Ok(())
        })();
        if let Err(e) = outcome {
            cancel_ai(&mut ai_task, emit);
            watcher = None;
            automatic = false;
            due = None;
            emit(Event::Watching(false, false));
            emit(Event::Error(format!("{e:#}")));
            let _ = history(&journal, emit);
        }
        emit(Event::Busy(false));
    }
    cancel_ai(&mut ai_task, emit);
    drop(watcher);
    drop(journal);
    drop(lock);
    Ok(())
}

fn cancel_ai(task: &mut Option<(u64, Arc<AtomicBool>)>, emit: &impl Fn(Event)) {
    if let Some((_, flag)) = task.take() {
        flag.store(true, Ordering::Relaxed);
        emit(Event::AiProgress {
            active: false,
            done: 0,
            total: 0,
        });
    }
}
fn prepare_ai(config: &Config, journal: &Journal, stability: &mut Stability) -> Result<ai::Job> {
    ai::endpoint(&config.ai)?;
    ensure!(
        config.ai.minimum_confidence <= 100 && config.ai.minimum_confidence >= 1,
        "AI confidence threshold must be between 1 and 100"
    );
    let scan = disk::scan(config)?;
    let scanned = scan.files.len();
    let eligible = journal.unsuppressed(scan.files)?;
    let (facts, waiting) = stability.ready(
        eligible,
        Instant::now(),
        Duration::from_secs(config.settle_seconds.clamp(2, 60)),
    );
    let mut base = Plan {
        scanned,
        waiting,
        notices: scan.notices,
        ..Plan::default()
    };
    let mut candidates = Vec::new();
    for fact in facts {
        let cached = journal.ai_decision(&ai::cache_key(config, &fact)?)?;
        if cached.as_ref().is_some_and(|d| d.category.is_none()) {
            if base.notices.len() < 200 {
                base.notices.push(format!(
                    "AI left {} unclassified. Reclassify to ask again.",
                    fact.path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            continue;
        }
        candidates.push((fact, cached));
    }
    candidates
        .sort_by_key(|(fact, cached)| (if cached.is_none() { 0 } else { 1 }, path_key(&fact.path)));
    if candidates.len() > ai::RUN_LIMIT {
        candidates.truncate(ai::RUN_LIMIT);
        base.notices.push(
            "AI plan limited to 100 files. Organize or review this batch, then preview again."
                .into(),
        );
    }
    let (facts, cached) = candidates.into_iter().unzip();
    Ok(ai::Job {
        config: config.clone(),
        facts,
        cached,
        base,
    })
}
fn spawn_ai(
    id: u64,
    job: Option<ai::Job>,
    check: Option<AiSettings>,
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    commands: SyncSender<Command>,
    client: Arc<dyn ai::CompletionClient>,
) -> Result<()> {
    ensure!(
        running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok(),
        "A previous AI request is still finishing. Try again in up to 45 seconds."
    );
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(job) = job {
                ai::run(job, client.as_ref(), &cancel, |done, total| {
                    let _ = commands.send(Command::AiProgress { id, done, total });
                })
                .map(AiCompletion::Plan)
            } else {
                client
                    .models(check.as_ref().expect("AI connection settings"))
                    .map(AiCompletion::Connection)
            }
        }))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("AI analysis stopped unexpectedly")))
        .map_err(|e| format!("{e:#}"));
        running.store(false, Ordering::SeqCst);
        let _ = commands.send(Command::AiFinished { id, result });
    });
    Ok(())
}
