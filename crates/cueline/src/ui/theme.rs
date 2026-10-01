//! A dense, dark, low-glare theme in the spirit of Reaper's default.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG_DEEP: Color32 = Color32::from_rgb(0x12, 0x13, 0x16);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x1f);
pub const BG_LANE: Color32 = Color32::from_rgb(0x20, 0x21, 0x26);
pub const BG_LANE_ALT: Color32 = Color32::from_rgb(0x1d, 0x1e, 0x23);
pub const BG_HEADER: Color32 = Color32::from_rgb(0x26, 0x28, 0x2d);
pub const BG_WIDGET: Color32 = Color32::from_rgb(0x2e, 0x30, 0x36);
pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(0x38, 0x3a, 0x42);
pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x35, 0x3c);
pub const GRID: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x32);
pub const GRID_STRONG: Color32 = Color32::from_rgb(0x3a, 0x3d, 0x45);

pub const TEXT: Color32 = Color32::from_rgb(0xd8, 0xda, 0xde);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8b, 0x90, 0x99);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x5c, 0x61, 0x6a);

pub const ACCENT: Color32 = Color32::from_rgb(0x4a, 0x9e, 0xf0);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xff, 0x55, 0x4a);
pub const PLAYING: Color32 = Color32::from_rgb(0x3f, 0xcf, 0x6b);
pub const LTC: Color32 = Color32::from_rgb(0xf0, 0xa8, 0x30);
pub const MTC: Color32 = Color32::from_rgb(0xa8, 0x8c, 0xff);
pub const WARN: Color32 = Color32::from_rgb(0xf0, 0xc0, 0x40);
pub const ERROR: Color32 = Color32::from_rgb(0xf0, 0x5a, 0x5a);
pub const MUTE: Color32 = Color32::from_rgb(0xe0, 0x5a, 0x4a);
pub const SOLO: Color32 = Color32::from_rgb(0xe8, 0xc8, 0x3a);

/// Track colours offered for new tracks, cycled in order.
pub const TRACK_COLORS: [[u8; 3]; 8] = [
    [0x4f, 0xa8, 0x8e],
    [0x5b, 0x8f, 0xd6],
    [0xc0, 0x7a, 0xd0],
    [0xd6, 0x8a, 0x4f],
    [0x8f, 0xb8, 0x4f],
    [0xd0, 0x5f, 0x7a],
    [0x4f, 0xb8, 0xc8],
    [0xb8, 0xa0, 0x5f],
];

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(3);
    v.extreme_bg_color = BG_DEEP;
    v.faint_bg_color = BG_LANE_ALT;
    v.code_bg_color = BG_DEEP;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARN;
    v.error_fg_color = ERROR;
    v.selection.bg_fill = ACCENT.linear_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.slider_trailing_fill = true;

    let radius = CornerRadius::same(3);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_PANEL;
    w.noninteractive.weak_bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.noninteractive.corner_radius = radius;
    for (state, fill) in [
        (&mut w.inactive, BG_WIDGET),
        (&mut w.hovered, BG_WIDGET_HOVER),
        (&mut w.active, BG_WIDGET_HOVER),
        (&mut w.open, BG_WIDGET),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.bg_stroke = Stroke::new(1.0, GRID_STRONG);
    w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    ctx.set_visuals(v);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 22.0;
        s.text_styles.insert(TextStyle::Body, FontId::proportional(13.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
        s.text_styles.insert(TextStyle::Monospace, mono(12.5));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(16.0));
    });
}
