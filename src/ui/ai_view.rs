use super::views::field;
use super::*;

impl Organizer {
    pub(super) fn ai_settings(&mut self, ui: &mut egui::Ui) {
        section(ui, "AI Hub · Universal");
        caption(
            ui,
            "Universal uses the most recently started local model in AI Hub. Start that model in the hub first.",
        );
        section(ui, "Where files belong");
        if super::folders::folder_field(
            ui,
            "ai-folder",
            "Organized folder",
            &mut self.config.ai.destination,
            &self.config.destination_roots,
        ) {
            self.choose_folder(
                ui.ctx(),
                super::folders::Target::Ai,
                self.config.ai.destination.clone(),
            );
        }
        caption(
            ui,
            "Choosing a folder also allows AI to create its categories there. Pasted paths must be inside a saved destination.",
        );
        ui.strong("How would you like files organized?");
        ui.add(
            egui::TextEdit::multiline(&mut self.config.ai.goal)
                .desired_rows(2)
                .char_limit(1000)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            ui.label("Action");
            ui.selectable_value(&mut self.config.ai.action, Action::Move, "Move");
            ui.selectable_value(&mut self.config.ai.action, Action::Copy, "Copy");
        });
        ui.checkbox(
            &mut self.config.ai.include_text,
            "Include short text excerpts for better context",
        );
        caption(
            ui,
            "Optional: first 1 KB of TXT, MD, CSV, JSON, XML, YAML and LOG files up to 1 MB. No PDF, image or OCR reading.",
        );
        ui.checkbox(
            &mut self.config.ai.allow_rename,
            "Let AI suggest clearer filenames",
        );
        ui.horizontal(|ui| {
            ui.label("Recommended confidence");
            ui.add(
                egui::DragValue::new(&mut self.config.ai.minimum_confidence)
                    .range(1..=100)
                    .suffix("%"),
            );
        });
        caption(
            ui,
            "Lower estimates stay for manual review in automatic mode. Confidence is a model estimate, not a guarantee.",
        );
        ui.add_space(12.0);
        egui::CollapsingHeader::new("Hub connection & access token").show(ui, |ui| {
        field(ui, "AI Hub base URL", &mut self.config.ai.base_url);
        ui.add_space(8.0);
        ui.strong("Access token");
        ui.add(
            egui::TextEdit::singleline(&mut self.token_input)
                .password(true)
                .hint_text("Your AI Hub token")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.token_input.is_empty(),
                    egui::Button::new("Save token"),
                )
                .clicked()
            {
                let token = zeroize::Zeroizing::new(std::mem::take(&mut self.token_input));
                self.send(Command::SaveAiToken(self.config.ai.clone(), token));
            }
            if ui.button("Remove saved token").clicked() {
                self.send(Command::RemoveAiToken(self.config.ai.clone()));
            }
            if ui.button("Test connection").clicked() {
                self.ai_status = "Checking the universal route…".into();
                self.send(Command::CheckAi(self.config.ai.clone()));
            }
        });
        caption(
            ui,
            "Tokens are stored in Windows Credential Manager for this hub address. Leave blank if your hub allows unauthenticated access.",
        );
        caption(ui, &self.ai_status);
        ui.add_space(12.0);
        });
        egui::CollapsingHeader::new("Advanced connection & filename conflicts").show(ui, |ui| {
            ui.checkbox(
                &mut self.config.ai.allow_http_lan,
                "Allow unencrypted HTTP to a trusted remote hub",
            );
            caption(
                ui,
                "Localhost HTTP works by default. Use HTTPS for remote connections.",
            );
            ui.horizontal(|ui| {
                ui.label("Existing filename");
                ui.selectable_value(
                    &mut self.config.ai.conflict,
                    Conflict::KeepBoth,
                    "Keep both",
                );
                ui.selectable_value(&mut self.config.ai.conflict, Conflict::Skip, "Skip");
            });
        });
    }
}
