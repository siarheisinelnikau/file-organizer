use crate::design::*;
use egui::{self, RichText};
use file_organizer::localization::{self, Language, Theme, formatted, tr};
mod ai_view;
mod capture;
mod folders;
mod guide;
mod views;
pub use capture::capture_screens;
use file_organizer::{
    journal::Entry,
    model::*,
    updater::{self, Downloaded, Update},
    worker::{Command, Event, Worker},
};
use std::path::PathBuf;

fn action_name(action: Action) -> &'static str {
    match action {
        Action::Move => "Move",
        Action::Copy => "Copy",
        Action::Rename => "Rename",
    }
}

/// Authored status messages keep canonical English values in state so switching languages
/// also updates an already displayed result. Technical diagnostic payloads stay intact.
pub(crate) fn translate_status(ui: &egui::Ui, status: &str) -> String {
    let translated = tr(ui, status);
    if translated != status {
        return translated.to_owned();
    }
    for template in [
        "Version {} is available",
        "Completed {completed} of {total} actions. Review Activity for details.",
        "Universal is connected to {}.",
        "Collision skipped: {}",
        "Link skipped: {}",
        "No available name for {name}",
        "{}: invalid filename template",
        "{}: destination must be absolute",
        "AI left {} for review: {}",
        "Recovered {}",
        "Recovery needs attention: {e:#}",
        "Cannot read {}: {e}",
        "AI left {} unclassified. Reclassify to ask again.",
        "AI Hub returned HTTP {status}. Its active model must support non-streaming JSON chat completions.",
    ] {
        let result = formatted(ui, template, status);
        if result != status {
            return result;
        }
    }
    for context in [
        "Enter the AI Hub base URL, for example http://127.0.0.1:8080/v1",
        "AI Hub did not return a JSON response",
        "AI Hub did not return a model list",
        "AI Hub returned invalid classification JSON. No changes were planned.",
    ] {
        if let Some(details) = status
            .strip_prefix(context)
            .and_then(|rest| rest.strip_prefix(": "))
        {
            return format!("{}: {details}", tr(ui, context));
        }
    }
    status.to_owned()
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Guide,
    Rules,
    Review,
    Activity,
    Settings,
}

enum UpdateCommand {
    Check,
    Download(Box<Update>),
}

pub struct Organizer {
    directory: PathBuf,
    worker: Option<Worker>,
    folders: folders::FolderPicker,
    updates: Option<updater::Service>,
    guide: guide::Guide,
    config: Config,
    loaded: bool,
    dirty: bool,
    appearance_dirty: bool,
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
    available_update: Option<Box<Update>>,
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
        let repaint = ctx.clone();
        let updates = if live {
            updater::config()
                .and_then(|config| updater::Service::new(config, move || repaint.request_repaint()))
        } else {
            Err("Update checks are disabled in preview mode.".into())
        };
        let update_error = updates.as_ref().err().cloned();
        Self {
            directory,
            worker,
            folders: folders::FolderPicker::new(live),
            updates: updates.ok(),
            guide: guide::Guide::default(),
            config: Config::default(),
            loaded: !live,
            dirty: false,
            appearance_dirty: false,
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
            update_status: update_error.unwrap_or_else(|| "Not checked yet".into()),
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
        let Some(service) = &self.updates else { return };
        let result = match command {
            UpdateCommand::Check => service.check(),
            UpdateCommand::Download(update) => service.download(update),
        };
        match result {
            Ok(()) => self.update_busy = true,
            Err(error) => self.update_status = error,
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        self.poll_folders();
        while let Some(event) = self.worker.as_ref().and_then(|w| w.events.try_recv().ok()) {
            match event {
                Event::Loaded(config) => {
                    self.config = config;
                    localization::set_language(ctx, self.config.language);
                    crate::design::apply_theme(ctx, self.config.theme);
                    self.loaded = true;
                    if self.config.startup_updates {
                        self.update_status = "Checking for updates…".into();
                        self.update_command(UpdateCommand::Check);
                    }
                }
                Event::AppearanceSaved { language, theme } => {
                    if self.config.language == language && self.config.theme == theme {
                        self.appearance_dirty = false;
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
        while let Some(event) = self
            .updates
            .as_ref()
            .and_then(|service| service.events.try_recv().ok())
        {
            match event {
                updater::Event::Checked(result) => {
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
                updater::Event::Progress { received, total } => {
                    self.download_progress = if total > 0 {
                        received as f32 / total as f32
                    } else {
                        0.0
                    }
                }
                updater::Event::Ready(result) => {
                    self.update_busy = false;
                    match result {
                        Ok(download) => {
                            self.update_status =
                                "Update verified. Finishing current work before restart…".into();
                            self.downloaded = Some(download);
                            self.begin_shutdown();
                        }
                        Err(e) => self.update_status = e,
                    }
                }
            }
        }
        if self.shutting_down
            && self.stopped
            && !self.install_sent
            && let Some(mut download) = self.downloaded.take()
        {
            self.install_sent = true;
            match updater::launch_helper(&mut download) {
                Ok(()) => self.may_close = true,
                Err(error) => {
                    self.update_status = error;
                    self.shutting_down = false;
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
            worker.shutdown((self.dirty || self.appearance_dirty).then(|| self.config.clone()));
        } else {
            self.may_close = true;
        }
        self.dirty = false;
    }

    pub fn paint(&mut self, ui: &mut egui::Ui) {
        localization::set_language(ui.ctx(), self.config.language);
        let before = self.config.clone();
        self.shell(ui);
        if self.page != Page::Guide {
            self.pause_guide();
        }
        if before != self.config {
            if before.language != self.config.language || before.theme != self.config.theme {
                localization::set_language(ui.ctx(), self.config.language);
                crate::design::apply_theme(ui.ctx(), self.config.theme);
                ui.ctx().request_repaint();
                let mut workflow = self.config.clone();
                workflow.language = before.language;
                workflow.theme = before.theme;
                // Appearance does not invalidate a reviewed plan or pause monitoring.
                self.appearance_dirty = true;
                self.send(Command::Appearance {
                    language: self.config.language,
                    theme: self.config.theme,
                });
                if workflow != before {
                    self.configuration_changed();
                }
            } else {
                self.configuration_changed();
            }
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
    pub fn pause_guide(&mut self) {
        self.guide.pause();
    }
    pub fn close_requested(&mut self) {
        self.pause_guide();
        self.begin_shutdown();
    }
    pub fn can_close(&self) -> bool {
        self.may_close
    }
}
