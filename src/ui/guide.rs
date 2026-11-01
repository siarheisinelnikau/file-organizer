//! A finite, scrubbable illustration. Only pure planners see these example facts.
use super::*;
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use file_organizer::{ai, engine};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

const SECONDS: f32 = 16.0;
const STAGES: [&str; 5] = ["Files", "Settings", "Review", "Organize", "Undo"];

pub(super) struct Guide {
    pub mode: PlanningMode,
    pub action: Action,
    kind: String,
    threshold: u8,
    pub position: f32,
    playing: bool,
    last_tick: Option<Instant>,
    example: Option<Example>,
}
impl Default for Guide {
    fn default() -> Self {
        Self {
            mode: PlanningMode::AiHub,
            action: Action::Move,
            kind: "Documents".into(),
            threshold: 80,
            position: 0.0,
            playing: false,
            last_tick: None,
            example: None,
        }
    }
}
struct ExampleFile {
    name: String,
    destination: Option<String>,
    confidence: Option<u8>,
    selected: bool,
}
struct Example {
    files: Vec<ExampleFile>,
    selected: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
enum FileStatus {
    Review,
    Suggested,
    Working,
    Done,
    Undoing,
    Undone,
}

impl Guide {
    pub fn pause(&mut self) {
        self.playing = false;
        self.last_tick = None;
    }
    fn tick(&mut self, now: Instant) {
        if !self.playing {
            self.last_tick = None;
            return;
        }
        if let Some(previous) = self.last_tick {
            self.position = (self.position
                + now.saturating_duration_since(previous).as_secs_f32() / SECONDS)
                .min(1.0);
        }
        self.last_tick = Some(now);
        if self.position >= 1.0 {
            self.pause();
        }
    }
    fn scrub(&mut self, position: f32) {
        self.position = position.clamp(0.0, 1.0);
        self.pause();
    }
    fn stage(&self) -> (usize, f32) {
        let scaled = self.position.clamp(0.0, 1.0) * 5.0;
        let stage = (scaled.floor() as usize).min(4);
        (stage, (scaled - stage as f32).min(1.0))
    }
    fn transfer(&self, file: usize, selected: &[usize]) -> f32 {
        let Some(order) = selected.iter().position(|i| *i == file) else {
            return 0.0;
        };
        let (stage, fraction) = self.stage();
        // Leave the end of each action chapter still so its result can be inspected.
        let fraction = (fraction / 0.75).min(1.0);
        match stage {
            3 => (fraction * selected.len() as f32 - order as f32).clamp(0.0, 1.0),
            4 => {
                1.0 - (fraction * selected.len() as f32 - (selected.len() - 1 - order) as f32)
                    .clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }
    fn destination_status(&self, file: &ExampleFile, amount: f32) -> FileStatus {
        if !file.selected {
            return FileStatus::Review;
        }
        let stage = self.stage().0;
        if stage == 4 && amount <= 0.0 {
            FileStatus::Undone
        } else if stage == 4 && amount < 1.0 {
            FileStatus::Undoing
        } else if amount >= 1.0 {
            FileStatus::Done
        } else if amount > 0.0 {
            FileStatus::Working
        } else {
            FileStatus::Suggested
        }
    }
    fn build_example(&self) -> anyhow::Result<Example> {
        let base = if cfg!(windows) {
            PathBuf::from(r"C:\Example")
        } else {
            PathBuf::from("/example")
        };
        let source = base.join("Downloads");
        let output = base.join("Organized");
        let names = ["Invoice-April.pdf", "Seaside-trip.jpg", "Project-notes.zip"];
        let facts: Vec<_> = names
            .iter()
            .map(|name| FileFact {
                path: source.join(name),
                source_root: source.clone(),
                stamp: Stamp {
                    bytes: 480_000,
                    modified_ns: 0,
                },
            })
            .collect();
        let mut config = Config {
            mode: self.mode,
            ..Config::default()
        };
        config.ai.action = self.action;
        config.ai.minimum_confidence = self.threshold;
        let plan = if self.mode == PlanningMode::Rules {
            let mut rule = Rule::template(&self.kind);
            rule.action = self.action;
            rule.min_age_minutes = 0;
            rule.destination = output
                .join(if self.kind == "Downloads" {
                    "Archives"
                } else {
                    &self.kind
                })
                .to_string_lossy()
                .into_owned();
            config.rules.push(rule);
            engine::plan(&config, &facts, &HashSet::new(), 0)
        } else {
            let decisions = [
                ("Finance", 94, "Invoice filename"),
                ("Photos/Travel", 88, "Travel photo filename"),
                ("Projects/Archives", 55, "Project subject is unclear"),
            ]
            .map(|(category, confidence, reason)| AiDecision {
                category: Some(category.into()),
                name: None,
                confidence,
                reason: reason.into(),
            });
            ai::plan_decisions(&config, &facts, &decisions, &HashSet::new(), &output)?
        };
        let files: Vec<_> = facts
            .iter()
            .zip(names)
            .map(|(fact, name)| {
                let operation = plan.operations.iter().find(|o| o.source == fact.path);
                ExampleFile {
                    name: name.into(),
                    destination: operation.map(|o| {
                        o.destination
                            .strip_prefix(&output)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/")
                    }),
                    confidence: operation.and_then(|o| o.ai.as_ref().map(|d| d.confidence)),
                    selected: operation.is_some_and(|o| {
                        self.mode == PlanningMode::Rules || ai::recommended(o, &config.ai)
                    }),
                }
            })
            .collect();
        let selected = files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.selected.then_some(i))
            .collect();
        Ok(Example { files, selected })
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 8.0;
        self.tick(Instant::now());
        let before = (self.mode, self.action, self.kind.clone(), self.threshold);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.mode, PlanningMode::Rules, tr(ui, "Manual rules"));
            ui.selectable_value(&mut self.mode, PlanningMode::AiHub, "AI Hub");
            caption(ui, "Demo only · no file actions");
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr(ui, "Action"));
            ui.selectable_value(&mut self.action, Action::Move, tr(ui, "Move"));
            ui.selectable_value(&mut self.action, Action::Copy, tr(ui, "Copy"));
            if self.mode == PlanningMode::Rules {
                ui.label(tr(ui, "File type"));
                egui::ComboBox::from_id_salt("guide-kind")
                    .width(135.0)
                    .selected_text(tr(
                        ui,
                        if self.kind == "Downloads" {
                            "Archives"
                        } else {
                            &self.kind
                        },
                    ))
                    .show_ui(ui, |ui| {
                        for kind in ["Documents", "Photos", "Downloads"] {
                            ui.selectable_value(
                                &mut self.kind,
                                kind.into(),
                                tr(
                                    ui,
                                    if kind == "Downloads" {
                                        "Archives"
                                    } else {
                                        kind
                                    },
                                ),
                            );
                        }
                    });
            } else {
                ui.label(tr(ui, "Recommend at"));
                ui.add(
                    egui::DragValue::new(&mut self.threshold)
                        .range(1..=100)
                        .suffix(tr(ui, "% confidence")),
                );
            }
        });
        if before != (self.mode, self.action, self.kind.clone(), self.threshold) {
            self.example = None;
            self.pause();
        }
        if self.example.is_none() {
            match self.build_example() {
                Ok(example) => self.example = Some(example),
                Err(e) => {
                    ui.colored_label(
                        colors(ui).error,
                        formatted(
                            ui,
                            "Example unavailable: {e}",
                            &format!("Example unavailable: {e}"),
                        ),
                    );
                    return;
                }
            }
        }
        caption(
            ui,
            if self.mode == PlanningMode::AiHub {
                "Example settings: group by subject · Universal · filenames only"
            } else {
                "Example settings: matching file type to Organized folder"
            },
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if primary(ui, if self.playing { "Pause" } else { "Play" }, true).clicked() {
                if self.playing {
                    self.pause();
                } else {
                    if self.position >= 1.0 {
                        self.position = 0.0;
                    }
                    self.playing = true;
                    self.last_tick = Some(Instant::now());
                }
            }
            let width = (ui.available_width() - 190.0).max(160.0);
            let response = ui
                .scope(|ui| {
                    ui.spacing_mut().slider_width = (width - 70.0).max(130.0);
                    ui.add(
                        egui::Slider::new(&mut self.position, 0.0..=1.0)
                            .show_value(false)
                            .text(tr(ui, "Timeline")),
                    )
                })
                .inner;
            if response.dragged() || response.changed() {
                self.pause();
            }
            caption(
                ui,
                format!("{:02} / 16 s", (self.position * SECONDS).round() as u32),
            );
            if ui.button(tr(ui, "Reset")).clicked() {
                self.scrub(0.0);
            }
        });
        let (stage, _) = self.stage();
        ui.horizontal_wrapped(|ui| {
            for (index, name) in STAGES.iter().enumerate() {
                if ui
                    .selectable_label(stage == index, format!("{}  {}", index + 1, tr(ui, name)))
                    .clicked()
                {
                    self.scrub(index as f32 / 5.0);
                }
            }
        });
        ui.add_space(8.0);
        let (stage, _) = self.stage();
        let (title, detail) = match stage {
            0 => (
                "Start with your files",
                "Choose a source folder and a destination. Previewing leaves the originals in place.",
            ),
            1 if self.mode == PlanningMode::Rules => (
                "Choose the matching rule",
                "The selected file type goes to its destination. Files without a matching rule stay where they are.",
            ),
            1 => (
                "Tell AI how you want things organized",
                "Universal proposes subject folders. These are illustrated suggestions; a real plan uses your running model.",
            ),
            2 if self.mode == PlanningMode::Rules => (
                "Review before applying",
                "The plan shows the exact destination. You select the changes to approve, and existing files are never overwritten.",
            ),
            2 => (
                "Review the AI suggestions",
                "Use Select recommended, then check the destinations. Lower confidence stays for review. The score is a model estimate.",
            ),
            3 if self.action == Action::Copy => (
                "Create the selected copies",
                "The originals stay in Downloads. The destination gets copies with the original dates and attributes.",
            ),
            3 => (
                "Move the selected files",
                "After approval, selected files reach their planned folders. Other files stay in Downloads.",
            ),
            _ if self.action == Action::Copy => (
                "Undo removes the unchanged copies",
                "Activity keeps a record of each change. Copy undo removes the verified copy and leaves the original alone.",
            ),
            _ => (
                "Undo puts the files back",
                "Activity keeps a record of each change. Undo restores the original path, dates and attributes after checking for conflicts.",
            ),
        };
        ui.strong(tr(ui, title));
        caption(ui, detail);
        ui.add_space(4.0);
        self.scene(ui);
        let count = self.example.as_ref().unwrap().selected.len();
        caption(
            ui,
            formatted(
                ui,
                "{count} selected of 3 · Drag the timeline or change the example settings.",
                &format!(
                    "{count} selected of 3 · Drag the timeline or change the example settings."
                ),
            ),
        );
        if self.playing {
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }
    }
    fn scene(&self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 220.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect(
            rect,
            10.0,
            colors(ui).surface,
            Stroke::new(1.0, colors(ui).border),
            StrokeKind::Inside,
        );
        let width = (rect.width() - 88.0) / 2.0;
        let left = Rect::from_min_size(rect.min + vec2(12.0, 12.0), vec2(width, 196.0));
        let right = Rect::from_min_size(
            pos2(rect.max.x - 12.0 - width, rect.min.y + 12.0),
            vec2(width, 196.0),
        );
        for panel in [left, right] {
            painter.rect(
                panel,
                8.0,
                colors(ui).canvas,
                Stroke::new(1.0, colors(ui).border),
                StrokeKind::Inside,
            );
        }
        let (stage, _) = self.stage();
        painter.text(
            left.min + vec2(12.0, 12.0),
            Align2::LEFT_TOP,
            tr(ui, "Downloads"),
            FontId::proportional(16.0),
            colors(ui).ink,
        );
        painter.text(
            right.min + vec2(12.0, 12.0),
            Align2::LEFT_TOP,
            tr(
                ui,
                if stage < 3 {
                    "Proposed folders"
                } else {
                    "Organized"
                },
            ),
            FontId::proportional(16.0),
            colors(ui).ink,
        );
        let example = self.example.as_ref().unwrap();
        for (index, file) in example.files.iter().enumerate() {
            let y = 46.0 + index as f32 * 50.0;
            let amount = self.transfer(index, &example.selected);
            let left_visible = self.action == Action::Copy || amount < 1.0;
            let left_row = Rect::from_min_size(left.min + vec2(8.0, y), vec2(width - 16.0, 42.0));
            let right_row = Rect::from_min_size(right.min + vec2(8.0, y), vec2(width - 16.0, 42.0));
            painter.rect_filled(
                left_row,
                5.0,
                if amount > 0.0 && amount < 1.0 {
                    colors(ui).tint
                } else {
                    colors(ui).surface
                },
            );
            painter.text(
                left_row.min + vec2(8.0, 5.0),
                Align2::LEFT_TOP,
                &file.name,
                FontId::proportional(14.0),
                if left_visible {
                    colors(ui).ink
                } else {
                    colors(ui).muted
                },
            );
            let status = if !file.selected && stage >= 2 {
                if file.destination.is_some() {
                    "Needs your review"
                } else {
                    "No matching rule"
                }
            } else if self.action == Action::Copy && stage >= 3 {
                "Original kept"
            } else if amount >= 1.0 {
                "Moved"
            } else {
                "Original file"
            };
            painter.text(
                left_row.min + vec2(8.0, 25.0),
                Align2::LEFT_TOP,
                tr(ui, status),
                FontId::proportional(12.0),
                colors(ui).muted,
            );
            if stage > 0
                && (stage < 3 || file.selected)
                && let Some(destination) = &file.destination
            {
                let color = if file.selected {
                    colors(ui).accent
                } else {
                    colors(ui).warning
                };
                painter.rect_filled(
                    right_row,
                    5.0,
                    if amount >= 1.0 {
                        colors(ui).tint
                    } else {
                        colors(ui).surface
                    },
                );
                let row = painter.with_clip_rect(right_row.shrink(5.0));
                row.text(
                    right_row.min + vec2(8.0, 5.0),
                    Align2::LEFT_TOP,
                    destination,
                    FontId::proportional(12.0),
                    if stage < 3 || amount > 0.0 {
                        colors(ui).ink
                    } else {
                        colors(ui).muted
                    },
                );
                let status = match self.destination_status(file, amount) {
                    FileStatus::Review => {
                        file.confidence.map_or("Not selected".into(), |confidence| {
                            formatted(
                                ui,
                                "{confidence}% · review first",
                                &format!("{confidence}% · review first"),
                            )
                        })
                    }
                    FileStatus::Done => "Done".into(),
                    FileStatus::Undoing => "Undo in progress".into(),
                    FileStatus::Undone => "Undone".into(),
                    FileStatus::Working => if self.action == Action::Copy {
                        "Copying…"
                    } else {
                        "Moving…"
                    }
                    .into(),
                    FileStatus::Suggested => {
                        file.confidence
                            .map_or("Selected for review".into(), |confidence| {
                                formatted(
                                    ui,
                                    "{confidence}% · recommended",
                                    &format!("{confidence}% · recommended"),
                                )
                            })
                    }
                };
                row.text(
                    right_row.min + vec2(8.0, 25.0),
                    Align2::LEFT_TOP,
                    translate_status(ui, &status),
                    FontId::proportional(12.0),
                    color,
                );
            }
            if amount > 0.0 && amount < 1.0 {
                let ease = amount * amount * (3.0 - 2.0 * amount);
                let removing_copy = stage == 4 && self.action == Action::Copy;
                let center = if removing_copy {
                    right_row.center()
                } else {
                    left_row.center().lerp(right_row.center(), ease)
                };
                let flying = Rect::from_center_size(center, vec2(38.0, 30.0));
                painter.rect(
                    flying,
                    5.0,
                    if removing_copy {
                        colors(ui).tint.gamma_multiply(amount)
                    } else {
                        colors(ui).tint
                    },
                    Stroke::new(
                        1.0,
                        if removing_copy {
                            colors(ui).accent.gamma_multiply(amount)
                        } else {
                            colors(ui).accent
                        },
                    ),
                    StrokeKind::Inside,
                );
                painter.text(
                    center,
                    Align2::CENTER_CENTER,
                    file.name.rsplit('.').next().unwrap(),
                    FontId::proportional(11.0),
                    colors(ui).accent,
                );
            }
        }
        let middle = rect.center();
        let direction = if stage == 4 { -1.0 } else { 1.0 };
        if stage == 4 && self.action == Action::Copy {
            painter.text(
                middle,
                Align2::CENTER_CENTER,
                tr(ui, "Undo"),
                FontId::proportional(12.0),
                colors(ui).muted,
            );
        } else {
            painter.arrow(
                middle - vec2(18.0 * direction, 0.0),
                vec2(36.0 * direction, 0.0),
                Stroke::new(2.0, colors(ui).accent),
            );
        }
        if stage == 0 {
            painter.text(
                right.center(),
                Align2::CENTER_CENTER,
                tr(ui, "Choose a destination"),
                FontId::proportional(14.0),
                colors(ui).muted,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendering_guide_keeps_configuration_intact_and_navigation_stops_playback() {
        let ctx = egui::Context::default();
        let mut app = Organizer::new(&ctx, PathBuf::from("unused-example-data"), false);
        let before = app.config.clone();
        app.page = Page::Guide;
        app.guide.playing = true;
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(960.0, 680.0))),
            ..Default::default()
        };
        let mut frame = ctx.run_ui(input(), |ui| app.paint(ui));
        // This state-only test intentionally has no rasterizer consuming texture updates.
        frame.textures_delta.clear();
        assert_eq!(app.config, before);
        assert!(!app.dirty);
        app.page = Page::Review;
        let mut frame = ctx.run_ui(input(), |ui| app.paint(ui));
        frame.textures_delta.clear();
        assert!(!app.guide.playing);
        app.guide.playing = true;
        app.pause_guide();
        assert!(!app.guide.playing);
    }
    #[test]
    fn example_plans_use_real_rule_matching_and_ai_recommendations() {
        let mut guide = Guide::default();
        let demo = guide.build_example().unwrap();
        assert_eq!(demo.selected, vec![0, 1]);
        guide.threshold = 40;
        assert_eq!(guide.build_example().unwrap().selected.len(), 3);
        guide.threshold = 95;
        assert!(guide.build_example().unwrap().selected.is_empty());
        guide.mode = PlanningMode::Rules;
        assert_eq!(guide.build_example().unwrap().selected, vec![0]);
        guide.kind = "Photos".into();
        assert_eq!(guide.build_example().unwrap().selected, vec![1]);
        guide.kind = "Downloads".into();
        assert_eq!(guide.build_example().unwrap().selected, vec![2]);
    }
    #[test]
    fn playback_finishes_and_pausing_prevents_background_advancement() {
        let mut guide = Guide::default();
        let now = Instant::now();
        guide.playing = true;
        guide.last_tick = Some(now);
        guide.tick(now + Duration::from_secs(8));
        assert_eq!(guide.position, 0.5);
        guide.pause();
        guide.tick(now + Duration::from_secs(30));
        assert_eq!(guide.position, 0.5);
        guide.playing = true;
        guide.last_tick = Some(now);
        guide.tick(now + Duration::from_secs(16));
        assert_eq!(guide.position, 1.0);
        assert!(!guide.playing);
        guide.scrub(0.4);
        assert!(!guide.playing);
        assert_eq!(guide.stage().0, 2);
    }
    #[test]
    fn transfers_and_undo_follow_selected_files_at_scrubbed_times() {
        let mut guide = Guide::default();
        let selected = guide.build_example().unwrap().selected;
        guide.scrub(0.7);
        assert!(guide.transfer(0, &selected) > 0.99);
        assert_eq!(guide.transfer(2, &selected), 0.0);
        guide.scrub(0.8);
        assert_eq!(guide.transfer(0, &selected), 1.0);
        guide.scrub(1.0);
        let example = guide.build_example().unwrap();
        assert_eq!(
            guide.destination_status(&example.files[0], 0.0),
            FileStatus::Undone
        );
        assert_eq!(
            guide.destination_status(&example.files[2], 0.0),
            FileStatus::Review
        );
        for i in 0..3 {
            assert_eq!(guide.transfer(i, &selected), 0.0);
        }
        guide.action = Action::Copy;
        assert_eq!(guide.build_example().unwrap().selected, selected);
    }
}
