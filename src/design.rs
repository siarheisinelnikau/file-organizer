use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};
pub const INK: Color32 = Color32::from_rgb(29, 48, 51);
pub const MUTED: Color32 = Color32::from_rgb(86, 105, 109);
pub const ACCENT: Color32 = Color32::from_rgb(17, 104, 92);
pub const TINT: Color32 = Color32::from_rgb(226, 240, 234);
pub const CANVAS: Color32 = Color32::from_rgb(246, 248, 247);
pub const BORDER: Color32 = Color32::from_rgb(217, 226, 222);
pub const WARNING: Color32 = Color32::from_rgb(144, 75, 19);
pub const ERROR: Color32 = Color32::from_rgb(161, 49, 48);

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    ctx.set_visuals(egui::Visuals::light());
    ctx.style_mut_of(egui::Theme::Light, |s| {
        s.text_styles.insert(
            TextStyle::Heading,
            FontId::new(26.0, FontFamily::Proportional),
        );
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(13.0));
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 9.0);
        s.spacing.interact_size.y = 34.0;
        s.visuals.override_text_color = Some(INK);
        s.visuals.panel_fill = CANVAS;
        s.visuals.window_fill = Color32::WHITE;
        s.visuals.extreme_bg_color = Color32::WHITE;
        s.visuals.faint_bg_color = TINT;
        s.visuals.selection.bg_fill = TINT;
        s.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        s.visuals.hyperlink_color = ACCENT;
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        s.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
        s.animation_time = 0.12;
    });
}
pub fn primary(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    enabled: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.visuals_mut().override_text_color = Some(if enabled { Color32::WHITE } else { INK });
        ui.add_enabled(
            enabled,
            egui::Button::new(label)
                .fill(if enabled { ACCENT } else { BORDER })
                .stroke(Stroke::NONE),
        )
    })
    .inner
}
pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).color(MUTED));
}
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(title).size(17.0).strong());
}
