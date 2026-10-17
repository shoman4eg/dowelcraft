//! Тёмная тема окна — цвета и отступы cadcraft (`crates/ui-egui/src/theme.rs`, MIT OR Apache-2.0).

use eframe::egui::{self, Color32};

/// Цвета оболочки.
pub struct Tokens {
    pub chrome: Color32,
    pub chrome_dark: Color32,
    pub panel: Color32,
    pub tab_active: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub border: Color32,
    pub canvas: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub field: Color32,
}

pub const T: Tokens = Tokens {
    chrome: Color32::from_rgb(0x3b, 0x44, 0x53),
    chrome_dark: Color32::from_rgb(0x31, 0x38, 0x44),
    panel: Color32::from_rgb(0x3c, 0x44, 0x52),
    tab_active: Color32::from_rgb(0x4e, 0x5a, 0x6e),
    control: Color32::from_rgb(0x50, 0x5a, 0x6d),
    control_hover: Color32::from_rgb(0x5c, 0x67, 0x7c),
    border: Color32::from_rgb(0x23, 0x29, 0x30),
    canvas: Color32::from_rgb(0x22, 0x28, 0x2f),
    text: Color32::from_rgb(0xd7, 0xdb, 0xe0),
    text_dim: Color32::from_rgb(0xa3, 0xaa, 0xb4),
    text_faint: Color32::from_rgb(0x87, 0x8d, 0x96),
    accent: Color32::from_rgb(0x3d, 0x8b, 0xfd),
    field: Color32::from_rgb(0x31, 0x38, 0x44),
};

/// Поставить тему: цвета виджетов, окон и меню, отступы.
pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = T.panel;
    v.window_fill = T.chrome;
    v.extreme_bg_color = T.field;
    v.faint_bg_color = T.chrome_dark;
    v.override_text_color = Some(T.text);
    v.widgets.noninteractive.bg_fill = T.panel;
    v.widgets.noninteractive.fg_stroke.color = T.text;
    v.widgets.noninteractive.bg_stroke.color = T.border;
    v.widgets.inactive.bg_fill = T.control;
    v.widgets.inactive.weak_bg_fill = T.control;
    v.widgets.inactive.fg_stroke.color = T.text;
    v.widgets.hovered.bg_fill = T.control_hover;
    v.widgets.hovered.weak_bg_fill = T.control_hover;
    v.widgets.active.bg_fill = T.accent;
    v.widgets.active.weak_bg_fill = T.accent;
    v.selection.bg_fill = T.accent;
    v.window_stroke = egui::Stroke::new(1.0, T.border);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(90) };
    v.window_corner_radius = egui::CornerRadius::same(6);
    v.menu_corner_radius = egui::CornerRadius::same(5);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.spacing.interact_size.y = 20.0;
        s.interaction.tooltip_delay = 0.15;
    });
}
