use egui::{Color32, FontFamily, FontId, Stroke, TextStyle};
const INK: Color32 = Color32::from_rgb(29, 48, 51);
pub const MUTED: Color32 = Color32::from_rgb(86, 105, 109);
pub const ACCENT: Color32 = Color32::from_rgb(17, 104, 92);
pub const TINT: Color32 = Color32::from_rgb(226, 240, 234);
pub const CANVAS: Color32 = Color32::from_rgb(246, 248, 247);
pub const BORDER: Color32 = Color32::from_rgb(217, 226, 222);
pub const WARNING: Color32 = Color32::from_rgb(144, 75, 19);
pub const ERROR: Color32 = Color32::from_rgb(161, 49, 48);

#[derive(Clone, Copy)]
pub struct Palette {
    pub ink: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub tint: Color32,
    pub canvas: Color32,
    pub border: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub surface: Color32,
}
pub fn colors(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}
fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            ink: Color32::from_rgb(230, 239, 237),
            muted: Color32::from_rgb(164, 184, 180),
            accent: Color32::from_rgb(106, 214, 183),
            tint: Color32::from_rgb(37, 69, 61),
            canvas: Color32::from_rgb(20, 29, 28),
            surface: Color32::from_rgb(28, 40, 37),
            border: Color32::from_rgb(60, 79, 72),
            warning: Color32::from_rgb(242, 184, 112),
            error: Color32::from_rgb(255, 153, 149),
        }
    } else {
        Palette {
            ink: INK,
            muted: MUTED,
            accent: ACCENT,
            tint: TINT,
            canvas: CANVAS,
            border: BORDER,
            warning: WARNING,
            error: ERROR,
            surface: Color32::WHITE,
        }
    }
}
pub fn configure(ctx: &egui::Context) {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let c = palette(theme == egui::Theme::Dark);
        ctx.style_mut_of(theme, |s| {
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
            s.visuals = if theme == egui::Theme::Dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            s.visuals.override_text_color = Some(c.ink);
            s.visuals.panel_fill = c.canvas;
            s.visuals.window_fill = c.surface;
            s.visuals.extreme_bg_color = c.surface;
            s.visuals.faint_bg_color = c.tint;
            s.visuals.selection.bg_fill = c.tint;
            s.visuals.selection.stroke = Stroke::new(1.0, c.accent);
            s.visuals.hyperlink_color = c.accent;
            s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, c.border);
            s.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, c.accent);
            s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, c.muted);
            s.visuals.warn_fg_color = c.warning;
            s.visuals.error_fg_color = c.error;
            s.animation_time = 0.12;
        });
    }
    apply_theme(ctx, file_organizer::localization::Theme::Light);
}
pub fn apply_theme(ctx: &egui::Context, theme: file_organizer::localization::Theme) {
    ctx.set_theme(match theme {
        file_organizer::localization::Theme::Light => egui::Theme::Light,
        file_organizer::localization::Theme::Dark => egui::Theme::Dark,
    });
}
pub fn primary(ui: &mut egui::Ui, label: impl Into<String>, enabled: bool) -> egui::Response {
    let label = label.into();
    let label = file_organizer::localization::tr(ui, &label).to_owned();
    ui.scope(|ui| {
        ui.visuals_mut().override_text_color = Some(if enabled {
            Color32::WHITE
        } else {
            colors(ui).ink
        });
        ui.add_enabled(
            enabled,
            egui::Button::new(label)
                .fill(if enabled {
                    Color32::from_rgb(17, 104, 92)
                } else {
                    colors(ui).border
                })
                .stroke(Stroke::NONE),
        )
    })
    .inner
}
pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(
        egui::RichText::new(crate::ui::translate_status(ui, &text.into())).color(colors(ui).muted),
    );
}
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(file_organizer::localization::tr(ui, title))
            .size(17.0)
            .strong(),
    );
}
