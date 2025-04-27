use super::*;
use egui::{Color32, Frame, Margin, Stroke};

impl Organizer {
    pub(super) fn shell(&mut self, root: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .exact_size(196.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(Margin::same(18))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(root, |ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("FileOrganizer")
                        .size(21.0)
                        .strong()
                        .color(ACCENT),
                );
                caption(ui, "Less clutter. More clarity.");
                ui.add_space(32.0);
                for (page, label, hint) in [
                    (Page::Review, "Review", "Preview & organize"),
                    (Page::Rules, "Rules", "Your filing routine"),
                    (Page::Activity, "Activity", "History & undo"),
                    (Page::Settings, "Settings", "Folders & preferences"),
                ] {
                    let selected = self.page == page;
                    Frame::new()
                        .fill(if selected { TINT } else { Color32::TRANSPARENT })
                        .corner_radius(8)
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            if ui
                                .add_sized(
                                    [ui.available_width(), 28.0],
                                    egui::Button::new(RichText::new(label).strong()).frame(false),
                                )
                                .clicked()
                            {
                                self.page = page;
                            }
                            ui.label(RichText::new(hint).small().color(MUTED));
                        });
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    caption(ui, format!("Version {}", updater::VERSION));
                    if self.available_update.is_some() && ui.link("Update available").clicked() {
                        self.page = Page::Settings;
                        self.settings_tab = 3;
                    }
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(if self.shutting_down {
                            "Finishing safely"
                        } else if self.automatic {
                            "Automatic filing"
                        } else if self.watching {
                            "Watching for review"
                        } else {
                            "Organization paused"
                        })
                        .strong()
                        .color(ACCENT),
                    );
                    if self.watching
                        && ui
                            .add_enabled(!self.busy, egui::Button::new("Pause monitoring"))
                            .clicked()
                    {
                        self.send(Command::Pause);
                    }
                    if !self.watching {
                        caption(ui, "Your files stay as they are.");
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::new().fill(CANVAS).inner_margin(Margin::same(26)))
            .show(root, |ui| {
                let (title, subtitle) = match self.page {
                    Page::Review => (
                        "Review your files",
                        "Preview the changes, then choose what to organize.",
                    ),
                    Page::Rules => (
                        "Your filing routine",
                        "The first matching rule decides where each file belongs.",
                    ),
                    Page::Activity => (
                        "Activity & undo",
                        "See what changed and put files back when you need to.",
                    ),
                    Page::Settings => (
                        "Settings",
                        "Choose your folders and how FileOrganizer works.",
                    ),
                };
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(title);
                        caption(ui, subtitle);
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.dirty
                            && primary(
                                ui,
                                "Save changes",
                                !self.busy && !self.ai_busy && !self.stopped,
                            )
                            .clicked()
                        {
                            self.send(Command::Save(self.config.clone()));
                            self.dirty = false;
                        }
                        if self.ai_busy {
                            if ui.button("Cancel AI").clicked() {
                                self.send(Command::CancelAi);
                            }
                            caption(
                                ui,
                                if self.ai_total > 0 {
                                    format!("AI · {} / {} files", self.ai_done, self.ai_total)
                                } else {
                                    "AI · connecting".into()
                                },
                            );
                        } else if self.busy {
                            ui.label("Working…");
                        }
                    });
                });
                ui.add_space(20.0);
                if self.pending > 0 {
                    notice(
                        ui,
                        WARNING,
                        "Some changes need recovery before you continue.",
                    );
                    if self.page != Page::Activity && ui.link("Open recovery").clicked() {
                        self.page = Page::Activity;
                    }
                } else if let Some((error, text)) = self.messages.last() {
                    notice(ui, if *error { ERROR } else { ACCENT }, text);
                }
                if !self.loaded {
                    ui.label("Loading your folders and history…");
                    return;
                }
                if self.shutting_down {
                    notice(
                        ui,
                        MUTED,
                        "Finishing the current file safely before closing…",
                    );
                }
                ui.add_enabled_ui(
                    !self.busy && !self.ai_busy && !self.stopped && !self.shutting_down,
                    |ui| match self.page {
                        Page::Review => self.review_view(ui),
                        Page::Rules => self.rules_view(ui),
                        Page::Activity => self.activity_view(ui),
                        Page::Settings => self.settings_view(ui),
                    },
                );
            });
    }

    fn review_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.config.mode, PlanningMode::AiHub, "AI Hub");
            ui.selectable_value(&mut self.config.mode, PlanningMode::Rules, "Manual rules");
        });
        ui.add_space(12.0);
        let ai = self.config.mode == PlanningMode::AiHub;
        if ai {
            caption(
                ui,
                if self.config.ai.include_text {
                    "Universal reads filenames, relative folders and short text excerpts. Review its proposed folders below."
                } else {
                    "Universal categorizes filenames and relative folders. File contents are excluded."
                },
            );
            if setup_target(&self.config).is_some() {
                surface(ui, |ui| {
                    section(ui, "Let AI find the right place");
                    caption(
                        ui,
                        "Choose your source folders and an allowed destination, then describe how you want files organized. No rules to write.",
                    );
                    ui.add_space(12.0);
                    if primary(ui, "Set up AI organization", true).clicked() {
                        self.page = Page::Settings;
                        self.settings_tab = if self.config.source_roots.is_empty()
                            || self.config.destination_roots.is_empty()
                        {
                            0
                        } else {
                            1
                        };
                    }
                });
                return;
            }
            ui.add_space(10.0);
        }
        if let Some(target) = setup_target(&self.config) {
            let folders_missing = target == Page::Settings;
            surface(ui, |ui| {
                ui.add_space(20.0);
                ui.label(
                    RichText::new("Give your files a place to go")
                        .size(23.0)
                        .strong(),
                );
                caption(
                    ui,
                    "Choose folders, add a rule, and see exactly what will change.",
                );
                ui.add_space(20.0);
                for (n, title, detail, ready) in [
                    (
                        "1",
                        "Choose your folders",
                        "Select what to organize and where it can go.",
                        !folders_missing,
                    ),
                    (
                        "2",
                        "Add your first rule",
                        "Start with documents, photos, or downloads.",
                        !self.config.rules.is_empty(),
                    ),
                    (
                        "3",
                        "Preview & approve",
                        "Review each change before applying it.",
                        false,
                    ),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if ready { "Ready" } else { n })
                                .color(ACCENT)
                                .strong(),
                        );
                        ui.vertical(|ui| {
                            ui.strong(title);
                            caption(ui, detail);
                        });
                    });
                    ui.add_space(10.0);
                }
                if primary(
                    ui,
                    if folders_missing {
                        "Choose folders"
                    } else {
                        "Create a rule"
                    },
                    true,
                )
                .clicked()
                {
                    self.page = if folders_missing {
                        Page::Settings
                    } else {
                        Page::Rules
                    };
                    self.settings_tab = 0;
                }
                ui.add_space(16.0);
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if primary(ui, if ai { "Create AI plan" } else { "Preview changes" }, true).clicked() {
                self.send(Command::Preview(self.config.clone()));
                self.dirty = false;
            }
            if ai && ui.button("Reclassify").on_hover_text("Discard cached AI categories and ask Universal again. This sends another request.").clicked() { self.send(Command::Reclassify(self.config.clone())); self.dirty = false; }
            if ui
                .button("Watch for changes")
                .on_hover_text("Keep this preview updated. Files only change after approval.")
                .clicked()
            {
                self.send(Command::Watch {
                    config: self.config.clone(),
                    automatic: false,
                });
                self.dirty = false;
            }
        });
        ui.add_space(12.0);
        if self.plan.waiting > 0 {
            caption(
                ui,
                format!(
                    "Waiting for {} files to finish changing. The preview refreshes automatically.",
                    self.plan.waiting
                ),
            );
        }
        let count = self.selected.iter().filter(|s| **s).count();
        if self.plan.operations.is_empty() {
            surface(ui, |ui| {
                ui.strong(if self.plan.scanned == 0 {
                    "See the plan before you commit"
                } else {
                    "No changes to review"
                });
                caption(
                    ui,
                    if ai {
                        "Create an AI plan to propose categories for settled files. Review the result before organizing."
                    } else if self.plan.scanned == 0 {
                        "Preview your folders to find files that match your rules."
                    } else {
                        "No settled files currently match an enabled rule. You can adjust your rules or check again later."
                    },
                );
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("{} proposed changes", self.plan.operations.len()));
                if ui
                    .small_button(if ai {
                        "Select recommended"
                    } else {
                        "Select all"
                    })
                    .clicked()
                {
                    for (selected, op) in self.selected.iter_mut().zip(&self.plan.operations) {
                        *selected = !ai || file_organizer::ai::recommended(op, &self.config.ai);
                    }
                }
                if ui.small_button("Clear").clicked() {
                    self.selected.fill(false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary(
                        ui,
                        format!("Organize {count} selected"),
                        count > 0 && !self.dirty && !self.watching && self.pending == 0,
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
            });
            if self.watching {
                caption(
                    ui,
                    "Pause monitoring to keep this preview fixed and approve changes.",
                );
            }
            caption(
                ui,
                "File dates and basic attributes are preserved. Find undo in Activity.",
            );
            if ai {
                caption(
                    ui,
                    format!(
                        "Recommended: model confidence at least {}%. This estimate is not a guarantee; check the destinations.",
                        self.config.ai.minimum_confidence
                    ),
                );
            }
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("preview")
                .auto_shrink([false, true])
                .max_height((ui.available_height() - 68.0).max(100.0))
                .show_rows(
                    ui,
                    if ai { 164.0 } else { 134.0 },
                    self.plan.operations.len(),
                    |ui, range| {
                        for i in range {
                            let op = &self.plan.operations[i];
                            Frame::new()
                                .fill(Color32::WHITE)
                                .inner_margin(Margin::same(12))
                                .corner_radius(8)
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.set_min_height(if ai { 134.0 } else { 104.0 });
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut self.selected[i], "").on_hover_text(
                                            "Include this file in the approved changes",
                                        );
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                let name = op
                                                    .source
                                                    .file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy();
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(
                                                        (ui.available_width() - 160.0).max(120.0),
                                                        34.0,
                                                    ),
                                                    egui::Layout::left_to_right(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(name.as_ref())
                                                                    .strong(),
                                                            )
                                                            .truncate(),
                                                        )
                                                        .on_hover_text(name.as_ref());
                                                    },
                                                );
                                                ui.label(
                                                    RichText::new(format!("{:?}", op.action))
                                                        .color(ACCENT),
                                                );
                                                ui.label(
                                                    RichText::new(size_label(op.stamp.bytes))
                                                        .color(MUTED)
                                                        .small(),
                                                );
                                            });
                                            if let Some(decision) = &op.ai {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(format!(
                                                            "{}% · {}",
                                                            decision.confidence, decision.reason
                                                        ))
                                                        .color(
                                                            if file_organizer::ai::recommended(
                                                                op,
                                                                &self.config.ai,
                                                            ) {
                                                                ACCENT
                                                            } else {
                                                                WARNING
                                                            },
                                                        ),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(&decision.reason);
                                            }
                                            path_label(ui, &op.source.display().to_string());
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new("to").color(ACCENT));
                                                path_label(
                                                    ui,
                                                    &op.destination.display().to_string(),
                                                );
                                            });
                                        });
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    },
                );
        }
        if !self.plan.notices.is_empty() {
            egui::CollapsingHeader::new(format!("{} items to know about", self.plan.notices.len()))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for notice in &self.plan.notices {
                                ui.label(notice);
                            }
                        });
                });
        }
    }

    fn rules_view(&mut self, ui: &mut egui::Ui) {
        if self.config.mode == PlanningMode::AiHub {
            caption(
                ui,
                "AI Hub is active. These rules are used only when Manual rules is selected on Review.",
            );
        }
        ui.horizontal(|ui| {
            for label in ["Documents", "Photos", "Downloads", "Custom"] {
                if ui.button(format!("+ {label}")).clicked() {
                    self.config.rules.push(if label == "Custom" {
                        Rule::default()
                    } else {
                        Rule::template(label)
                    });
                    self.selected_rule = self.config.rules.len() - 1;
                }
            }
        });
        ui.add_space(12.0);
        if self.config.rules.is_empty() {
            surface(ui, |ui| {
                section(ui, "Start with a simple rule");
                caption(
                    ui,
                    "For example: move PDF documents into your Documents folder. Choose a template above to begin.",
                );
            });
            return;
        }
        self.selected_rule = self.selected_rule.min(self.config.rules.len() - 1);
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(228.0, height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                caption(ui, "RULE ORDER");
                egui::ScrollArea::vertical().id_salt("rule-list").auto_shrink([false,true]).max_height(height-110.0).show(ui, |ui| {
                    for (index, rule) in self.config.rules.iter_mut().enumerate() {
                        Frame::new().fill(if index == self.selected_rule { TINT } else { Color32::WHITE }).corner_radius(8).inner_margin(10).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut rule.enabled, "").on_hover_text("Enable this rule");
                                if ui.selectable_label(index == self.selected_rule, format!("{}  {}", index+1, rule.name)).clicked() { self.selected_rule = index; }
                            });
                            caption(ui, format!("{:?} · {}", rule.action, if rule.extensions.is_empty() { "all file types" } else { &rule.extensions }));
                        }); ui.add_space(6.0);
                    }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.selected_rule > 0, egui::Button::new("Move up")).clicked() { self.config.rules.swap(self.selected_rule, self.selected_rule-1); self.selected_rule-=1; }
                    if ui.add_enabled(self.selected_rule+1 < self.config.rules.len(), egui::Button::new("Move down")).clicked() { self.config.rules.swap(self.selected_rule, self.selected_rule+1); self.selected_rule+=1; }
                });
                if ui.button(RichText::new("Remove rule").color(ERROR)).clicked() { self.config.rules.remove(self.selected_rule); }
            });
            ui.add_space(12.0);
            let Some(rule) = self.config.rules.get_mut(self.selected_rule) else { return };
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            egui::ScrollArea::vertical().id_salt("rule-editor").auto_shrink([false,true]).show(ui, |ui| {
                surface(ui, |ui| {
                    field(ui, "Rule name", &mut rule.name);
                    section(ui, "Match these files");
                    field(ui, "File extensions", &mut rule.extensions);
                    caption(ui, "Separate with commas, for example pdf, docx. Leave blank for all types.");
                    field(ui, "Filename pattern", &mut rule.pattern);
                    ui.horizontal(|ui| { ui.label("Modified at least"); ui.add(egui::DragValue::new(&mut rule.min_age_minutes).range(0..=525600).suffix(" min ago")); });
                    egui::CollapsingHeader::new("Size & source subfolders").show(ui, |ui| {
                        field(ui, "Source subfolder pattern", &mut rule.folder_pattern);
                        caption(ui, "* includes all subfolders; blank matches the source folder itself.");
                        ui.horizontal(|ui| { ui.label("Minimum bytes"); ui.add(egui::DragValue::new(&mut rule.min_bytes)); });
                        ui.horizontal(|ui| { ui.label("Maximum bytes (0 = any)"); ui.add(egui::DragValue::new(&mut rule.max_bytes)); });
                    });
                    section(ui, "Put them in place");
                    ui.horizontal(|ui| {
                        for action in [Action::Move, Action::Copy, Action::Rename] { ui.selectable_value(&mut rule.action, action, format!("{action:?}")); }
                    });
                    if rule.action != Action::Rename { field(ui, "Destination folder", &mut rule.destination); caption(ui, "Use a folder inside an allowed destination from Settings."); }
                    if rule.action == Action::Rename { rename_field(ui, rule); }
                    else { egui::CollapsingHeader::new("Rename files too (optional)").default_open(rule.name_template != "{name}").show(ui, |ui| rename_field(ui, rule)); }
                    section(ui, "If a filename already exists");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut rule.conflict, Conflict::Skip, "Skip this file");
                        ui.selectable_value(&mut rule.conflict, Conflict::KeepBoth, "Keep both");
                    });
                    caption(ui, "Existing files are never overwritten.");
                });
            });
            });
        });
    }

    fn activity_view(&mut self, ui: &mut egui::Ui) {
        if self.pending > 0 && primary(ui, "Recover unfinished changes", true).clicked() {
            self.send(Command::Recover);
        }
        if self.history.is_empty() {
            surface(ui, |ui| {
                section(ui, "Your history starts with your first change");
                caption(
                    ui,
                    "Previewing never moves files. Applied changes appear here with an Undo action.",
                );
            });
        }
        let mut undo = None;
        egui::ScrollArea::vertical()
            .id_salt("history")
            .show(ui, |ui| {
                for entry in &self.history {
                    surface(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong(
                                entry
                                    .operation
                                    .source
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            );
                            ui.label(
                                RichText::new(match entry.phase.as_str() {
                                    "Done" => "Completed",
                                    "Undone" => "Undone",
                                    "Recovered" => "Recovered",
                                    _ => "Needs attention",
                                })
                                .color(if entry.phase == "Done" {
                                    ACCENT
                                } else {
                                    MUTED
                                }),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_enabled(
                                            entry.phase == "Done" && self.pending == 0,
                                            egui::Button::new("Undo"),
                                        )
                                        .clicked()
                                    {
                                        undo = Some(entry.id.clone());
                                    }
                                },
                            );
                        });
                        caption(
                            ui,
                            format!(
                                "{:?} · {} · {}",
                                entry.operation.action, entry.operation.rule_name, entry.created
                            ),
                        );
                        path_label(ui, &entry.operation.source.display().to_string());
                        path_label(ui, &format!("to {}", entry.operation.destination.display()));
                        if !entry.error.is_empty() {
                            ui.colored_label(ERROR, &entry.error);
                        }
                    });
                    ui.add_space(8.0);
                }
                egui::CollapsingHeader::new("Session details").show(ui, |ui| {
                    for (_, text) in self.messages.iter().rev() {
                        ui.label(text);
                    }
                });
            });
        if let Some(id) = undo {
            self.send(Command::Undo(id));
        }
    }

    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, label) in ["Folders", "AI Hub", "Automation", "Updates & about"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut self.settings_tab, i, *label);
            }
        });
        ui.add_space(16.0);
        egui::ScrollArea::vertical().id_salt("settings").show(ui, |ui| {
            surface(ui, |ui| match self.settings_tab {
                0 => {
                    section(ui, "Folders to organize"); caption(ui, "Only these folders are scanned. Choose existing local folders.");
                    roots(ui, "source", &mut self.config.source_roots);
                    ui.checkbox(&mut self.config.recursive, "Include subfolders");
                    section(ui, "Allowed destinations"); caption(ui, "AI and rules may create subfolders here. Other locations stay out of reach.");
                    roots(ui, "destination", &mut self.config.destination_roots);
                    egui::CollapsingHeader::new("File readiness").show(ui, |ui| {
                        ui.horizontal(|ui| { ui.label("Wait for a file to stop changing"); ui.add(egui::DragValue::new(&mut self.config.settle_seconds).range(2..=60).suffix(" seconds")); });
                        caption(ui, "Downloads still in progress are skipped. Output folders are not scanned again.");
                    });
                    if self.dirty { caption(ui, "Save changes above when your folders are ready."); }
                }
                1 => self.ai_settings(ui),
                2 => {
                    section(ui, "Review first. Automate when ready.");
                    caption(ui, "Every launch starts paused. Monitoring continues only while FileOrganizer is open.");
                    ui.add_space(12.0);
                    ui.checkbox(&mut self.auto_approved, "I reviewed my setup and want files organized automatically");
                    ui.add_space(8.0);
                    if primary(ui, "Start automatic organization", self.auto_approved && self.pending == 0 && setup_target(&self.config).is_none()).clicked() {
                        self.send(Command::Watch { config: self.config.clone(), automatic: true }); self.dirty = false;
                    }
                    caption(ui, "For approval on every batch, use Watch for changes on the Review screen.");
                    if self.config.mode == PlanningMode::AiHub { caption(ui, format!("AI proposals below {}% confidence stay in Review. Confidence is the model’s estimate. Up to 100 files per batch; requests are made only after monitoring starts.", self.config.ai.minimum_confidence)); }
                    section(ui, "Undo stays available"); caption(ui, "Automatic changes use the same history and recovery checks as approved batches.");
                }
                _ => {
                    section(ui, "FileOrganizer"); caption(ui, format!("Installed version {} · Windows x64", updater::VERSION));
                    ui.checkbox(&mut self.config.startup_updates, "Check for updates on startup");
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(!self.update_busy, egui::Button::new("Check for updates")).clicked() { self.update_status = "Checking for updates…".into(); self.update_command(UpdateCommand::Check); }
                        if let Some(update) = self.available_update.clone()
                            && primary(ui, "Download and install", !self.update_busy && !self.watching).on_hover_text("Downloads a verified update and restarts FileOrganizer.").clicked() {
                                self.update_status = "Downloading and verifying…".into(); self.download_progress = 0.0; self.update_command(UpdateCommand::Download(update));
                            }
                    });
                    if self.update_busy { ui.add(egui::ProgressBar::new(self.download_progress).show_percentage()); }
                    caption(ui, &self.update_status);
                    ui.hyperlink_to("Project & releases", updater::REPOSITORY);
                    section(ui, "Your data"); caption(ui, "Settings and undo history stay on this computer. AI analysis sends filenames and relative folders to your configured AI Hub; short text excerpts are optional. Update checks contact GitHub.");
                    egui::CollapsingHeader::new("Data location & support").show(ui, |ui| {
                        path_label(ui, &self.directory.display().to_string()); caption(ui, "Updates preserve your settings and undo history.");
                    });
                }
            });
        });
    }
}

fn surface<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(Margin::same(18))
        .corner_radius(10)
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 2.0).max(0.0));
            body(ui)
        })
        .inner
}
pub(super) fn field(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(RichText::new(label).strong());
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}
fn rename_field(ui: &mut egui::Ui, rule: &mut Rule) {
    field(ui, "New filename pattern", &mut rule.name_template);
    caption(ui, "{name} = full name · {stem} = name · {ext} = extension");
}
fn path_label(ui: &mut egui::Ui, path: &str) {
    ui.add(egui::Label::new(RichText::new(path).small().color(MUTED)).truncate())
        .on_hover_text(path);
}
fn notice(ui: &mut egui::Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(color, "•");
        ui.add(egui::Label::new(text).truncate())
            .on_hover_text(text);
    });
    ui.add_space(8.0);
}
fn roots(ui: &mut egui::Ui, kind: &str, paths: &mut Vec<String>) {
    let mut remove = None;
    for (i, path) in paths.iter_mut().enumerate() {
        ui.push_id((kind, i), |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 94.0).max(120.0);
                ui.add(
                    egui::TextEdit::singleline(path)
                        .hint_text("C:\\Your folder")
                        .desired_width(width),
                );
                if ui.button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        paths.remove(i);
    }
    if ui.button(format!("+ Add {kind} folder")).clicked() {
        paths.push(String::new());
    }
}
fn size_label(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn setup_target(config: &Config) -> Option<Page> {
    if config.source_roots.iter().all(|p| p.trim().is_empty()) {
        return Some(Page::Settings);
    }
    if config.mode == PlanningMode::AiHub {
        return (config.destination_roots.iter().all(|p| p.trim().is_empty())
            || config.ai.destination.trim().is_empty())
        .then_some(Page::Settings);
    }
    if config.rules.is_empty() {
        return Some(Page::Rules);
    }
    if config
        .rules
        .iter()
        .any(|r| r.enabled && r.action != Action::Rename)
        && config.destination_roots.iter().all(|p| p.trim().is_empty())
    {
        return Some(Page::Settings);
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_only_workflow_does_not_require_an_output_folder() {
        let mut config = Config {
            source_roots: vec![r"C:\Fixture".into()],
            rules: vec![Rule {
                action: Action::Rename,
                ..Rule::default()
            }],
            ..Config::default()
        };
        assert!(setup_target(&config).is_none());
        config.rules[0].action = Action::Move;
        assert!(setup_target(&config) == Some(Page::Settings));
        config.destination_roots.push(r"C:\Output".into());
        assert!(setup_target(&config).is_none());
    }
}
