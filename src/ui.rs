use eframe::egui::{self, Color32, RichText};
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

const ACCENT: Color32 = Color32::from_rgb(20, 116, 106);
const MUTED: Color32 = Color32::from_rgb(101, 116, 126);

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
    worker: Worker,
    updates: SyncSender<UpdateCommand>,
    update_events: Receiver<UpdateEvent>,
    config: Config,
    loaded: bool,
    dirty: bool,
    busy: bool,
    watching: bool,
    automatic: bool,
    auto_approved: bool,
    page: Page,
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
    pub fn new(cc: &eframe::CreationContext<'_>, directory: PathBuf) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        cc.egui_ctx.style_mut_of(egui::Theme::Light, |s| {
            s.spacing.item_spacing = egui::vec2(10.0, 9.0);
            s.spacing.button_padding = egui::vec2(12.0, 7.0);
            s.visuals.selection.bg_fill = ACCENT;
            s.visuals.panel_fill = Color32::from_rgb(247, 249, 249);
            s.visuals.hyperlink_color = ACCENT;
        });
        let repaint = cc.egui_ctx.clone();
        let worker = Worker::start(directory.clone(), move || repaint.request_repaint());
        let (updates, commands) = mpsc::sync_channel(2);
        let (events, update_events) = mpsc::sync_channel(32);
        let update_directory = directory.join("updates");
        let ctx = cc.egui_ctx.clone();
        thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                let event = match command {
                    UpdateCommand::Check => {
                        UpdateEvent::Checked(updater::check().map_err(|e| format!("{e:#}")))
                    }
                    UpdateCommand::Download(update) => UpdateEvent::Downloaded(
                        updater::download(&update, &update_directory, |received, total| {
                            let _ = events
                                .try_send(UpdateEvent::Progress(received as f32 / total as f32));
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
        Self {
            directory,
            worker,
            updates,
            update_events,
            config: Config::default(),
            loaded: false,
            dirty: false,
            busy: false,
            watching: false,
            automatic: false,
            auto_approved: false,
            page: Page::Rules,
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
        match self.worker.commands.try_send(command) {
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

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.worker.events.try_recv() {
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
                Event::Error(error) => self.message(true, error),
                Event::Busy(busy) => self.busy = busy,
                Event::Watching(watching, automatic) => {
                    self.watching = watching;
                    self.automatic = automatic;
                    self.busy = false;
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
                            self.worker = Worker::start(self.directory.clone(), move || {
                                repaint.request_repaint()
                            });
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
        self.worker
            .shutdown(self.dirty.then(|| self.config.clone()));
        self.dirty = false;
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("FileOrganizer").color(ACCENT).size(25.0));
            ui.label(RichText::new("A place for every file.").color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let status = if self.shutting_down {
                    "Finishing safely…"
                } else if self.automatic {
                    "Automatic · running"
                } else if self.watching {
                    "Watching · review only"
                } else {
                    "Paused"
                };
                ui.label(RichText::new(status).color(if self.watching { ACCENT } else { MUTED }));
                if self.busy {
                    ui.spinner();
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (page, name) in [
                (Page::Rules, "Rules"),
                (Page::Review, "Review"),
                (Page::Activity, "Activity"),
                (Page::Settings, "Folders & settings"),
            ] {
                if ui.selectable_label(self.page == page, name).clicked() {
                    self.page = page;
                }
            }
            ui.separator();
            ui.label(format!(
                "{} rules · {} planned · {} unfinished",
                self.config.rules.len(),
                self.plan.operations.len(),
                self.pending
            ));
        });
        ui.separator();
        ui.add_enabled_ui(
            self.loaded && !self.stopped && !self.busy && !self.shutting_down,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(if self.dirty {
                            "Save configuration *"
                        } else {
                            "Save configuration"
                        })
                        .clicked()
                    {
                        self.send(Command::Save(self.config.clone()));
                        self.dirty = false;
                    }
                    if ui.button("Preview now").clicked() {
                        self.send(Command::Preview(self.config.clone()));
                        self.dirty = false;
                        self.page = Page::Review;
                    }
                    if ui.button("Watch for review").clicked() {
                        self.send(Command::Watch {
                            config: self.config.clone(),
                            automatic: false,
                        });
                        self.dirty = false;
                        self.page = Page::Review;
                    }
                    if ui
                        .add_enabled(self.watching, egui::Button::new("Pause"))
                        .clicked()
                    {
                        self.send(Command::Pause);
                    }
                });
            },
        );
        if self.pending > 0 {
            ui.colored_label(Color32::from_rgb(155, 65, 30), "Unfinished journal records need recovery. Organization is blocked until they are resolved.");
        }
        if let Some((error, text)) = self.messages.last() {
            ui.colored_label(
                if *error {
                    Color32::from_rgb(170, 50, 40)
                } else {
                    MUTED
                },
                text,
            );
        }
        if self.available_update.is_some()
            && self.page != Page::Settings
            && ui
                .link(format!("{} — open Updates", self.update_status))
                .clicked()
        {
            self.page = Page::Settings;
        }
        ui.add_space(10.0);
    }

    fn rules(&mut self, ui: &mut egui::Ui) {
        ui.heading("Build your filing routine");
        ui.label("Rules run from top to bottom. The first matching enabled rule wins.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Blank rule").clicked() {
                self.config.rules.push(Rule::default());
                self.selected_rule = self.config.rules.len() - 1;
            }
            for kind in ["Downloads", "Photos", "Documents"] {
                if ui.button(format!("+ {kind} template")).clicked() {
                    self.config.rules.push(Rule::template(kind));
                    self.selected_rule = self.config.rules.len() - 1;
                }
            }
        });
        ui.separator();
        if self.config.rules.is_empty() {
            ui.add_space(40.0);
            ui.heading("Start with one small rule");
            ui.label("Add a template, choose your approved folders in Folders & settings, then preview the result.");
            return;
        }
        self.selected_rule = self.selected_rule.min(self.config.rules.len() - 1);
        ui.columns(2, |columns| {
            let list = &mut columns[0];
            egui::ScrollArea::vertical().id_salt("rule-list").show(list, |ui| {
                for (index, rule) in self.config.rules.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut rule.enabled, "");
                        if ui.selectable_label(self.selected_rule == index, format!("{:02}  {}", index+1, rule.name)).clicked() { self.selected_rule = index; }
                    });
                    ui.label(RichText::new(format!("      {:?} · {}", rule.action, if rule.extensions.is_empty() { "all types" } else { &rule.extensions })).small().color(MUTED));
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.selected_rule > 0, egui::Button::new("Move up")).clicked() {
                        self.config.rules.swap(self.selected_rule, self.selected_rule-1); self.selected_rule -= 1;
                    }
                    if ui.add_enabled(self.selected_rule+1 < self.config.rules.len(), egui::Button::new("Move down")).clicked() {
                        self.config.rules.swap(self.selected_rule, self.selected_rule+1); self.selected_rule += 1;
                    }
                });
                if ui.button("Remove selected rule").clicked() { self.config.rules.remove(self.selected_rule); }
            });
            let Some(rule) = self.config.rules.get_mut(self.selected_rule) else { return };
            egui::ScrollArea::vertical().id_salt("rule-editor").show(&mut columns[1], |ui| {
                ui.heading("Rule editor");
                field(ui, "Rule name", &mut rule.name);
                ui.separator();
                ui.strong("When all conditions match");
                field(ui, "Filename pattern (* and ?)", &mut rule.pattern);
                field(ui, "Extensions, comma separated (empty = any)", &mut rule.extensions);
                field(ui, "Source subfolder pattern (* = any, empty = root)", &mut rule.folder_pattern);
                ui.horizontal(|ui| { ui.label("At least bytes"); ui.add(egui::DragValue::new(&mut rule.min_bytes).range(0..=u64::MAX)); });
                ui.horizontal(|ui| { ui.label("At most bytes (0 = unlimited)"); ui.add(egui::DragValue::new(&mut rule.max_bytes).range(0..=u64::MAX)); });
                ui.horizontal(|ui| { ui.label("Modified at least minutes ago"); ui.add(egui::DragValue::new(&mut rule.min_age_minutes).range(0..=525600)); });
                ui.separator(); ui.strong("Then");
                egui::ComboBox::from_id_salt("action").selected_text(format!("{:?}", rule.action)).show_ui(ui, |ui| {
                    for action in [Action::Move, Action::Copy, Action::Rename] { ui.selectable_value(&mut rule.action, action, format!("{action:?}")); }
                });
                if rule.action != Action::Rename { field(ui, "Destination folder (inside an approved output root)", &mut rule.destination); }
                field(ui, "Filename template", &mut rule.name_template);
                ui.label(RichText::new("{name} = full name · {stem} = name without extension · {ext} = extension. Example: filed-{stem}.{ext}").small().color(MUTED));
                egui::ComboBox::from_id_salt("collision").selected_text(format!("Collision: {:?}", rule.conflict)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut rule.conflict, Conflict::Skip, "Skip existing name");
                    ui.selectable_value(&mut rule.conflict, Conflict::KeepBoth, "Keep both with a numbered suffix");
                });
            });
        });
    }

    fn review(&mut self, ui: &mut egui::Ui) {
        ui.heading("Review before anything changes");
        ui.label(format!(
            "{} files scanned · {} waiting for stability · {} proposed actions",
            self.plan.scanned,
            self.plan.waiting,
            self.plan.operations.len()
        ));
        if self.plan.waiting > 0 {
            ui.label("The worker will sample these files again after the settling interval.");
        }
        let actionable =
            !self.busy && !self.dirty && !self.watching && self.pending == 0 && !self.stopped;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Select all").clicked() {
                self.selected.fill(true);
            }
            if ui.button("Clear selection").clicked() {
                self.selected.fill(false);
            }
            let count = self.selected.iter().filter(|s| **s).count();
            if ui
                .add_enabled(
                    actionable && count > 0,
                    egui::Button::new(format!("Apply {count} selected actions")),
                )
                .clicked()
            {
                let indices = self
                    .selected
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| s.then_some(i))
                    .collect();
                self.send(Command::Apply {
                    generation: self.generation,
                    indices,
                });
            }
        });
        if self.watching {
            ui.label(
                "Pause monitoring to freeze this plan and select actions, or use Preview now.",
            );
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("preview")
            .max_height((ui.available_height() - 80.0).max(120.0))
            .show_rows(ui, 100.0, self.plan.operations.len(), |ui, range| {
                for index in range {
                    let op = &self.plan.operations[index];
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.selected[index], "");
                        ui.vertical(|ui| {
                            ui.strong(format!(
                                "{:?}  ·  {}  ·  {} bytes",
                                op.action, op.rule_name, op.stamp.bytes
                            ));
                            ui.label(op.source.display().to_string());
                            ui.colored_label(ACCENT, format!("→ {}", op.destination.display()));
                        });
                    });
                    ui.separator();
                }
            });
        if !self.plan.notices.is_empty() {
            egui::CollapsingHeader::new(format!(
                "{} scan and collision notices",
                self.plan.notices.len()
            ))
            .show(ui, |ui| {
                for notice in &self.plan.notices {
                    ui.label(notice);
                }
            });
        }
    }

    fn activity(&mut self, ui: &mut egui::Ui) {
        ui.heading("Activity & recovery");
        ui.label("Undo checks the destination's SHA-256 before changing it. Changed files and occupied restore paths are preserved.");
        if ui
            .add_enabled(
                self.pending > 0 && !self.busy,
                egui::Button::new("Recover unfinished operations (roll back)"),
            )
            .clicked()
        {
            self.send(Command::Recover);
        }
        ui.separator();
        let mut undo = None;
        egui::ScrollArea::vertical()
            .id_salt("history")
            .show(ui, |ui| {
                for entry in &self.history {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(format!(
                                "{} · {:?} · {}",
                                entry.phase, entry.operation.action, entry.operation.rule_name
                            ));
                            ui.label(RichText::new(&entry.created).small().color(MUTED));
                            if ui
                                .add_enabled(
                                    entry.phase == "Done" && !self.busy && self.pending == 0,
                                    egui::Button::new("Undo"),
                                )
                                .clicked()
                            {
                                undo = Some(entry.id.clone());
                            }
                        });
                        ui.label(entry.operation.source.display().to_string());
                        ui.label(format!("→ {}", entry.operation.destination.display()));
                        if !entry.error.is_empty() {
                            ui.colored_label(Color32::from_rgb(170, 50, 40), &entry.error);
                        }
                    });
                }
                if self.history.is_empty() {
                    ui.label("No file operations yet. A dry run does not change files.");
                }
                egui::CollapsingHeader::new("Session messages").show(ui, |ui| {
                    for (_, text) in self.messages.iter().rev() {
                        ui.label(text);
                    }
                });
            });
        if let Some(id) = undo {
            self.send(Command::Undo(id));
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().id_salt("settings").show(ui, |ui| {
            ui.heading("Approved folders");
            ui.label("Enter existing absolute local folder paths. No folders are selected by default. Links and reparse points are refused.");
            roots(ui, "Source folders", &mut self.config.source_roots);
            roots(ui, "Output roots", &mut self.config.destination_roots);
            ui.label(RichText::new("Move/copy destinations may be new subfolders inside an output root. Output subtrees are excluded from source scans. Rename stays in the source folder.").small().color(MUTED));
            ui.checkbox(&mut self.config.recursive, "Include source subfolders");
            ui.horizontal(|ui| { ui.label("File settling interval (seconds)"); ui.add(egui::DragValue::new(&mut self.config.settle_seconds).range(2..=60)); });
            ui.separator();
            ui.heading("Automatic organization");
            ui.label("Monitoring runs only while this application is open. Every launch starts paused.");
            ui.checkbox(&mut self.auto_approved, "I reviewed these rules and want matching files applied automatically");
            if ui.add_enabled(self.auto_approved && !self.busy && self.loaded && !self.stopped && self.pending == 0, egui::Button::new("Start automatic organization")).clicked() {
                self.send(Command::Watch { config: self.config.clone(), automatic: true }); self.dirty = false;
            }
            ui.separator();
            ui.heading("Updates");
            ui.label(format!("Installed version: {} · Windows x64 portable", updater::VERSION));
            ui.checkbox(&mut self.config.startup_updates, "Check for updates on startup");
            ui.label("Save configuration to persist this choice. Disabling startup checks leaves manual checks available.");
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.update_busy, egui::Button::new("Check for updates")).clicked() {
                    self.update_status = "Checking for updates…".into(); self.update_command(UpdateCommand::Check);
                }
                if let Some(update) = self.available_update.clone()
                    && ui.add_enabled(!self.update_busy && !self.busy && !self.watching, egui::Button::new("Download and install (restarts app)")).clicked() {
                        self.update_status = "Downloading and verifying…".into(); self.download_progress = 0.0;
                        self.update_command(UpdateCommand::Download(update));
                }
            });
            if self.update_busy { ui.add(egui::ProgressBar::new(self.download_progress).show_percentage()); }
            ui.label(&self.update_status);
            ui.label(RichText::new("The planned public repository may not be published yet. Unavailable releases are reported as errors, never as an up-to-date result.").small().color(MUTED));
            ui.hyperlink_to("Canonical repository", updater::REPOSITORY);
            ui.separator();
            ui.heading("Local data");
            ui.label(self.directory.display().to_string());
            ui.label("Rules, settings, operation paths, hashes and recovery state stay in organizer.db. File contents are never sent to a service. Update checks contact GitHub.");
            ui.label("Portable updates replace only the executable and preserve this data folder.");
        });
    }
}

impl eframe::App for Organizer {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.may_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.begin_shutdown();
        }
        let before = serde_json::to_string(&self.config).unwrap_or_default();
        egui::CentralPanel::default().show(ui, |ui| {
            self.header(ui);
            ui.add_enabled_ui(
                self.loaded && !self.stopped && !self.busy && !self.shutting_down,
                |ui| match self.page {
                    Page::Rules => self.rules(ui),
                    Page::Review => self.review(ui),
                    Page::Activity => self.activity(ui),
                    Page::Settings => self.settings(ui),
                },
            );
            if !self.loaded {
                ui.label("Loading local settings and recovery journal…");
            }
        });
        if before != serde_json::to_string(&self.config).unwrap_or_default() {
            self.dirty = true;
            self.auto_approved = false;
            self.plan = Plan::default();
            self.selected.clear();
            if self.watching {
                self.send(Command::Pause);
            }
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
}

fn roots(ui: &mut egui::Ui, label: &str, roots: &mut Vec<String>) {
    ui.strong(label);
    let mut remove = None;
    for (index, root) in roots.iter_mut().enumerate() {
        ui.push_id((label, index), |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(root).desired_width(ui.available_width() - 100.0),
                );
                if ui.button("Remove").clicked() {
                    remove = Some(index);
                }
            });
        });
    }
    if let Some(index) = remove {
        roots.remove(index);
    }
    if ui
        .button(format!("+ Add {}", label.to_lowercase()))
        .clicked()
    {
        roots.push(String::new());
    }
}
