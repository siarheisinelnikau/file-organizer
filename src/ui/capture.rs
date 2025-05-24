//! Finite, side-effect-free screen fixtures through the production UI and CPU renderer.
use super::*;
use anyhow::{Result, ensure};
use std::{
    fs,
    time::{Duration, Instant},
};

pub fn capture_screens(args: &[String]) -> Result<()> {
    ensure!(
        args.len() == 1 || args.len() == 3 || args.len() == 4,
        "Usage: --capture ABSOLUTE_OUTPUT_DIRECTORY [WIDTH HEIGHT [SCALE]]"
    );
    let directory = PathBuf::from(&args[0]);
    file_organizer::filesystem::check_path(&directory)?;
    let width: u32 = args.get(1).map_or(Ok(1160), |v| v.parse())?;
    let height: u32 = args.get(2).map_or(Ok(820), |v| v.parse())?;
    ensure!(
        (960..=1920).contains(&width) && (680..=1200).contains(&height),
        "Capture size outside supported bounds"
    );
    let scale: f32 = args.get(3).map_or(Ok(1.0), |v| v.parse())?;
    ensure!(
        scale.is_finite()
            && (1.0..=2.0).contains(&scale)
            && width as f32 / scale >= 960.0
            && height as f32 / scale >= 680.0,
        "Logical viewport is too small"
    );
    fs::create_dir_all(&directory)?;
    let start = Instant::now();
    let mut records = Vec::new();
    for scene in [
        "review",
        "rules",
        "settings",
        "updates",
        "activity",
        "welcome",
        "recovery",
        "automation",
        "ai-review",
        "ai-settings",
        "ai-working",
        "folder-setup",
        "rule-details",
        "guide-ai",
        "guide-rules",
        "guide-move",
        "guide-undo",
        "guide-copy",
        "guide-copy-undo",
    ] {
        ensure!(
            start.elapsed() < Duration::from_secs(20),
            "Capture deadline exceeded"
        );
        let ctx = egui::Context::default();
        let mut app = Organizer::new(&ctx, PathBuf::from(r"C:\Demo\FileOrganizer"), false);
        app.fixture(scene);
        let mut renderer = crate::renderer::Renderer::new();
        let mut last = Vec::new();
        let before = Instant::now();
        // Font initialization and egui's size/layout settling are finite; no event loop or window exists.
        for frame in 0..3 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width as f32 / scale, height as f32 / scale),
                )),
                time: Some(frame as f64 / 60.0),
                ..Default::default()
            };
            raw.viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .native_pixels_per_point = Some(scale);
            let output = ctx.run_ui(raw, |ui| app.paint(ui));
            last = renderer.draw(&ctx, output, width, height)?;
        }
        let path = directory.join(format!("{scene}.png"));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let rgba: Vec<u8> = last.into_iter().flatten().collect();
        let mut writer = std::io::BufWriter::new(file);
        image::write_buffer_with_format(
            &mut writer,
            &rgba,
            width,
            height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )?;
        std::io::Write::flush(&mut writer)?;
        records.push(
            serde_json::json!({ "scene": scene, "width": width, "height": height, "frames": 3, "scale": scale,
            "capture_ms": before.elapsed().as_millis() }),
        );
    }
    let record = serde_json::json!({ "backend": "egui 0.37 meshes -> explicit 0.33 raster adapter -> egui_software_backend 0.0.2 CPU",
        "window_created": false, "workers_started": false, "network_enabled": false, "frames": records.len()*3,
        "total_ms": start.elapsed().as_millis(), "captures": records });
    let manifest = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("capture.json"))?;
    serde_json::to_writer_pretty(manifest, &record)?;
    println!("Captured screens and exited: {}", directory.display());
    Ok(())
}

impl Organizer {
    fn fixture(&mut self, scene: &str) {
        if scene.starts_with("guide-") {
            self.page = Page::Guide;
            self.guide.mode = if scene == "guide-rules" {
                PlanningMode::Rules
            } else {
                PlanningMode::AiHub
            };
            if scene.starts_with("guide-copy") {
                self.guide.action = Action::Copy;
            }
            self.guide.position = match scene {
                "guide-copy" => 0.79,
                "guide-copy-undo" => 0.92,
                "guide-move" => 0.66,
                "guide-undo" => 0.92,
                _ => 0.45,
            };
            return;
        }
        if scene == "folder-setup" {
            self.page = Page::Settings;
            self.config.mode = PlanningMode::AiHub;
            return;
        }
        if scene == "welcome" {
            self.config.mode = PlanningMode::AiHub;
            return;
        }
        self.config.source_roots = vec![r"C:\Demo\Downloads".into()];
        self.config.destination_roots = vec![r"C:\Demo\Organized".into()];
        for (kind, destination) in [
            ("Documents", r"C:\Demo\Organized\Documents"),
            ("Photos", r"C:\Demo\Organized\Photos"),
            ("Downloads", r"C:\Demo\Organized\Archives"),
        ] {
            let mut rule = Rule::template(kind);
            rule.destination = destination.into();
            self.config.rules.push(rule);
        }
        let facts = [
            ("Quarterly report.pdf", "Documents", "Documents", 482_304),
            ("Weekend by the sea.jpg", "Photos", "Photos", 3_921_024),
            (
                "Project handoff.zip",
                "Downloads: archives",
                "Archives",
                16_482_304,
            ),
        ];
        if scene == "rule-details" {
            self.config.rules[0].min_bytes = 20 * 1024 * 1024;
            self.config.rules[0].max_bytes = 100 * 1024 * 1024;
        }
        self.plan.scanned = 24;
        for (name, rule_name, folder, bytes) in facts {
            self.plan.operations.push(Operation {
                ai: None,
                source: PathBuf::from(format!(r"C:\Demo\Downloads\{name}")),
                destination: PathBuf::from(format!(r"C:\Demo\Organized\{folder}\{name}")),
                action: Action::Move,
                stamp: Stamp {
                    bytes,
                    modified_ns: 0,
                },
                rule_id: "capture".into(),
                rule_name: rule_name.into(),
            });
        }
        if scene.starts_with("ai-") {
            self.config.mode = PlanningMode::AiHub;
            self.config.ai.destination = r"C:\Demo\Organized".into();
            for (i, op) in self.plan.operations.iter_mut().enumerate() {
                op.ai = Some(AiDecision {
                    category: Some(
                        if i == 0 {
                            "Documents/Finance"
                        } else if i == 1 {
                            "Photos/Travel"
                        } else {
                            "Projects/Archives"
                        }
                        .into(),
                    ),
                    name: None,
                    confidence: if i == 2 { 55 } else { 94 },
                    reason: if i == 0 {
                        "Report name suggests a financial document."
                    } else if i == 1 {
                        "Filename describes a seaside trip."
                    } else {
                        "Project archive; subject is unclear."
                    }
                    .into(),
                });
                op.destination = PathBuf::from(r"C:\Demo\Organized")
                    .join(op.ai.as_ref().unwrap().category.as_ref().unwrap())
                    .join(op.source.file_name().unwrap());
            }
        }
        if scene == "ai-working" {
            self.ai_busy = true;
            self.ai_done = 10;
            self.ai_total = 24;
            self.plan = Plan::default();
        }
        self.selected = vec![true, true, false];
        self.page = match scene {
            "rules" | "rule-details" => Page::Rules,
            "settings" | "updates" | "automation" | "ai-settings" => Page::Settings,
            "activity" | "recovery" => Page::Activity,
            _ => Page::Review,
        };
        self.dirty = scene == "rules" || scene == "settings";
        if scene == "automation" {
            self.settings_tab = 2;
        }
        if scene == "ai-settings" {
            self.settings_tab = 1;
        }
        if scene == "updates" {
            self.settings_tab = 3;
            self.update_status = "Check for a new version whenever you need one.".into();
        }
        if scene == "activity" || scene == "recovery" {
            self.history = self
                .plan
                .operations
                .iter()
                .enumerate()
                .map(|(i, op)| Entry {
                    id: format!("fixture-{i}"),
                    operation: op.clone(),
                    sha256: "fixture".into(),
                    metadata: None,
                    source_roots: Vec::new(),
                    destination_roots: Vec::new(),
                    stage: PathBuf::new(),
                    phase: if i == 1 { "Undone" } else { "Done" }.into(),
                    error: String::new(),
                    created: "2026-10-31 10:30 UTC".into(),
                })
                .collect();
            if scene == "recovery" {
                self.pending = 1;
                self.history[0].phase = "Published".into();
                self.history[0].error =
                    "Could not remove the original file. Both copies have been preserved.".into();
            }
        }
    }
}
