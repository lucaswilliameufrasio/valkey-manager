use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Stroke, Vec2};

pub const CANVAS: Color32 = Color32::from_rgb(10, 23, 28);
pub const SURFACE: Color32 = Color32::from_rgb(16, 34, 40);
pub const SURFACE_RAISED: Color32 = Color32::from_rgb(23, 46, 52);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(31, 62, 67);
pub const SURFACE_INPUT: Color32 = Color32::from_rgb(8, 21, 26);
pub const BORDER: Color32 = Color32::from_rgb(42, 69, 73);
pub const TEXT: Color32 = Color32::from_rgb(242, 250, 247);
pub const MUTED: Color32 = Color32::from_rgb(151, 174, 172);
pub const MINT: Color32 = Color32::from_rgb(54, 222, 197);
pub const MINT_DARK: Color32 = Color32::from_rgb(14, 94, 84);
pub const LIME: Color32 = Color32::from_rgb(185, 239, 97);
pub const AMBER: Color32 = Color32::from_rgb(255, 200, 90);
pub const RED: Color32 = Color32::from_rgb(255, 127, 127);
pub const RED_DARK: Color32 = Color32::from_rgb(105, 39, 46);

pub fn apply_theme(context: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(MUTED);
    visuals.hyperlink_color = MINT;
    visuals.faint_bg_color = SURFACE;
    visuals.extreme_bg_color = SURFACE_INPUT;
    visuals.text_edit_bg_color = Some(SURFACE_INPUT);
    visuals.code_bg_color = SURFACE_INPUT;
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = RED;
    visuals.window_fill = SURFACE;
    visuals.panel_fill = CANVAS;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.selection.bg_fill = MINT_DARK;
    visuals.selection.stroke = Stroke::new(1.0, MINT);

    set_widget(&mut visuals.widgets.noninteractive, SURFACE, BORDER, MUTED);
    set_widget(&mut visuals.widgets.inactive, SURFACE_RAISED, BORDER, TEXT);
    set_widget(&mut visuals.widgets.hovered, SURFACE_HOVER, MINT, TEXT);
    set_widget(&mut visuals.widgets.active, MINT_DARK, MINT, TEXT);
    set_widget(&mut visuals.widgets.open, SURFACE_RAISED, MINT, TEXT);

    let mut style = (*context.style_of(egui::Theme::Dark)).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.interact_size = Vec2::new(40.0, 36.0);
    style.spacing.window_margin = Margin::same(16);
    style.visuals = visuals;
    context.set_style_of(egui::Theme::Dark, style);
    context.set_theme(egui::Theme::Dark);
}

fn set_widget(
    widget: &mut egui::style::WidgetVisuals,
    fill: Color32,
    border: Color32,
    text: Color32,
) {
    widget.bg_fill = fill;
    widget.weak_bg_fill = fill;
    widget.bg_stroke = Stroke::new(1.0, border);
    widget.corner_radius = CornerRadius::same(9);
    widget.fg_stroke = Stroke::new(1.0, text);
    widget.expansion = 0.0;
}

pub fn card_frame() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(14)
        .inner_margin(16)
}

pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_ascii_uppercase())
            .size(11.0)
            .strong()
            .color(MUTED),
    );
}

pub fn metric_card(ui: &mut egui::Ui, label: &str, value: &str, accent: Color32) {
    Frame::new()
        .fill(SURFACE_RAISED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(174.0, 76.0));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").size(11.0).color(accent));
                ui.label(egui::RichText::new(label).size(12.0).color(MUTED));
            });
            ui.add_space(5.0);
            ui.label(egui::RichText::new(value).size(21.0).strong().color(TEXT));
        });
}
