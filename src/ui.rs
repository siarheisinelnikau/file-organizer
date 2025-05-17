use crate::design::*;
use egui::{self, RichText};
mod ai_view;
mod capture;
mod folders;
mod views;
pub use capture::capture_screens;
use file_organizer::{
    journal::Entry,
    model::*,
    updater::{self, Downloaded, Update},
    worker::{Command, Event, Worker},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Rules,
    Review,
    Activity,
    Settings,
}

enum UpdateCommand {
    Check,
    Download(Update),
    Install(Downloaded),
}
enum UpdateEvent {
    Checked(Result<Option<Update>, String>),
    Progress(f32),
    Downloaded(Result<Downloaded, String>),
    Installed(Result<(), String>),
}

pub struct Organizer {
    directory: PathBuf,
    worker: Option<Worker>,
    folders: folders::FolderPicker,
    updates: SyncSender<UpdateCommand>,
    update_events: Receiver<UpdateEvent>,
    config: Config,
    loaded: bool,
    dirty: bool,
    busy: bool,
    ai_busy: bool,
    ai_done: usize,
    ai_total: usize,
    ai_status: String,
    token_input: String,
    watching: bool,
    automatic: bool,
    auto_approved: bool,
    page: Page,
    settings_tab: usize,
    selected_rule: usize,
    plan: Plan,
    generation: u64,
    selected: Vec<bool>,
    history: Vec<Entry>,
    pending: usize,
    messages: Vec<(bool, String)>,
    update_status: String,
    available_update: Option<Update>,
    update_busy: bool,
    download_progress: f32,
    downloaded: Option<Downloaded>,
    shutting_down: bool,
    stopped: bool,
    may_close: bool,
    install_sent: bool,
}

impl Organizer {
    pub fn new(ctx: &egui::Context, directory: PathBuf, live: bool) -> Self {
        crate::design::configure(ctx);
        let repaint = ctx.clone();
        let worker =
            live.then(|| Worker::start(directory.clone(), move || repaint.request_repaint()));
        let (updates, commands) = mpsc::sync_channel(2);
        let (events, update_events) = mpsc::sync_channel(32);
        let update_directory = directory.join("updates");
        let ctx = ctx.clone();
        if live {
            thread::spawn(move || {
                while let Ok(command) = commands.recv() {
                    let event = match command {
                        UpdateCommand::Check => {
                            UpdateEvent::Checked(updater::check().map_err(|e| format!("{e:#}")))
                        }
                        UpdateCommand::Download(update) => UpdateEvent::Downloaded(
                            updater::download(&update, &update_directory, |received, total| {
                                let _ = events.try_send(UpdateEvent::Progress(
                                    received as f32 / total as f32,
                                ));
                                ctx.request_repaint();
                            })
                            .map_err(|e| format!("{e:#}")),
                        ),
                        UpdateCommand::Install(download) => UpdateEvent::Installed(
                            updater::launch_helper(&download).map_err(|e| format!("{e:#}")),
                        ),
                    };
                    if events.send(event).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Self {
            directory,
            worker,
            folders: folders::FolderPicker::new(live),
            updates,
            update_events,
            config: Config::default(),
            loaded: !live,
            dirty: false,
            busy: false,
            ai_busy: false,
            ai_done: 0,
            ai_total: 0,
            ai_status: "Connect to your running AI Hub. No model is started automatically.".into(),
            token_input: String::new(),
            watching: false,
            automatic: false,
            auto_approved: false,
            page: Page::Review,
            settings_tab: 0,
            selected_rule: 0,
            plan: Plan::default(),
            generation: 0,
            selected: Vec::new(),
            history: Vec::new(),
            pending: 0,
            messages: Vec::new(),
            update_status: "Not checked yet".into(),
            available_update: None,
            update_busy: false,
            download_progress: 0.0,
            downloaded: None,
            shutting_down: false,
            stopped: false,
            may_close: false,
            install_sent: false,
        }
    }

    fn message(&mut self, error: bool, text: String) {
        self.messages.push((error, text));
        if self.messages.len() > 100 {
            self.messages.remove(0);
        }
    }

    fn send(&mut self, command: Command) {
        let Some(worker) = &self.worker else { return };
        match worker.commands.try_send(command) {
            Ok(()) => self.busy = true,
            Err(e) => self.message(true, format!("Background worker unavailable: {e}")),
        }
    }

    fn update_command(&mut self, command: UpdateCommand) {
        match self.updates.try_send(command) {
            Ok(()) => self.update_busy = true,
            Err(e) => self.update_status = format!("Update worker unavailable: {e}"),
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.poll_folders();
        while let Some(event) = self.worker.as_ref().and_then(|w| w.events.try_recv().ok()) {
            match event {
                Event::Loaded(config) => {
                    self.config = config;
                    self.loaded = true;
                    if self.config.startup_updates {
                        self.update_status = "Checking for updates…".into();
                        self.update_command(UpdateCommand::Check);
                    }
                }
                Event::Plan { generation, plan } => {
                    self.generation = generation;
                    self.selected = vec![false; plan.operations.len()];
                    self.plan = plan;
                }
                Event::History(history, pending) => {
                    self.history = history;
                    self.pending = pending;
                }
                Event::Message(message) => self.message(false, message),
                Event::Error(error) => {
                    if self.ai_status.starts_with("Checking the universal route") {
                        self.ai_status = error.clone();
                    }
                    self.message(true, error);
                }
                Event::Busy(busy) => self.busy = busy,
                Event::AiProgress {
                    active,
                    done,
                    total,
                } => {
                    self.ai_busy = active;
                    self.ai_done = done;
                    self.ai_total = total;
                }
                Event::AiConnection(status) => self.ai_status = status,
                Event::Watching(watching, automatic) => {
                    self.watching = watching;
                    self.automatic = automatic;
                }
                Event::Stopped => {
                    self.stopped = true;
                    self.busy = false;
                    if self.shutting_down && self.downloaded.is_none() {
                        self.may_close = true;
                    }
                }
            }
        }
        while let Ok(event) = self.update_events.try_recv() {
            match event {
                UpdateEvent::Checked(result) => {
                    self.update_busy = false;
                    match result {
                        Ok(Some(update)) => {
                            self.update_status = format!("Version {} is available", update.version);
                            self.available_update = Some(update);
                        }
                        Ok(None) => {
                            self.update_status = "You are up to date".into();
                            self.available_update = None;
                        }
                        Err(error) => self.update_status = error,
                    }
                }
                UpdateEvent::Progress(fraction) => self.download_progress = fraction,
                UpdateEvent::Downloaded(result) => {
                    self.update_busy = false;
                    match result {
                        Ok(download) => {
                            self.update_status = format!(
                                "Version {} verified. Finishing current work before restart…",
                                download.version
                            );
                            self.downloaded = Some(download);
                            self.begin_shutdown();
                        }
                        Err(e) => self.update_status = e,
                    }
                }
                UpdateEvent::Installed(result) => {
                    self.update_busy = false;
                    match result {
                        Ok(()) => self.may_close = true,
                        Err(e) => {
                            self.update_status = e;
                            self.shutting_down = false;
                            self.downloaded = None;
                            self.install_sent = false;
                            let repaint = ctx.clone();
                            self.worker = Some(Worker::start(self.directory.clone(), move || {
                                repaint.request_repaint()
                            }));
                            self.loaded = false;
                            self.stopped = false;
                        }
                    }
                }
            }
        }
        if self.shutting_down
            && self.stopped
            && !self.install_sent
            && let Some(download) = self.downloaded.clone()
        {
            self.install_sent = true;
            self.update_command(UpdateCommand::Install(download));
        }
        if self.may_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn begin_shutdown(&mut self) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        if self.stopped {
            if self.downloaded.is_none() {
                self.may_close = true;
            }
            return;
        }
        if let Some(worker) = &self.worker {
            worker.shutdown(self.dirty.then(|| self.config.clone()));
        } else {
            self.may_close = true;
        }
        self.dirty = false;
    }

    pub fn paint(&mut self, ui: &mut egui::Ui) {
        let before = self.config.clone();
        self.shell(ui);
        if before != self.config {
            self.configuration_changed();
        }
    }
    fn configuration_changed(&mut self) {
        self.dirty = true;
        self.auto_approved = false;
        self.plan = Plan::default();
        self.selected.clear();
        if self.watching || self.ai_busy {
            self.send(Command::Pause);
        }
    }
    pub fn close_requested(&mut self) {
        self.begin_shutdown();
    }
    pub fn can_close(&self) -> bool {
        self.may_close
    }
}
