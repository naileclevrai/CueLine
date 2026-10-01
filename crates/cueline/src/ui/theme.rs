//! macOS-style dark appearance: system colours, label hierarchy, hairline
//! separators and generous corner radii, in the spirit of Logic Pro.

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke, TextStyle, Visuals};

use super::fonts;

// Surfaces, from back to front.
pub const BG_DEEP: Color32 = Color32::from_rgb(0x16, 0x16, 0x18);
pub const BG_CONTENT: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1e);
pub const BG_LANE: Color32 = Color32::from_rgb(0x21, 0x21, 0x23);
pub const BG_LANE_ALT: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x20);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x24, 0x24, 0x26);
pub const BG_HEADER: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2c);
pub const BG_TOOLBAR: Color32 = Color32::from_rgb(0x2d, 0x2d, 0x30);
pub const BG_TOOLBAR_TOP: Color32 = Color32::from_rgb(0x34, 0x34, 0x37);
pub const BG_SELECTED: Color32 = Color32::from_rgb(0x35, 0x37, 0x3d);
pub const BG_WIDGET: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3c);
pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(0x46, 0x46, 0x49);
pub const BG_WIDGET_ACTIVE: Color32 = Color32::from_rgb(0x52, 0x52, 0x56);
pub const LCD: Color32 = Color32::from_rgb(0x10, 0x11, 0x13);

// Lines.
pub const SEPARATOR: Color32 = Color32::from_rgb(0x38, 0x38, 0x3b);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0c);
pub const BORDER: Color32 = Color32::from_rgb(0x44, 0x44, 0x48);
pub const GRID: Color32 = Color32::from_rgb(0x28, 0x28, 0x2b);
pub const GRID_STRONG: Color32 = Color32::from_rgb(0x33, 0x33, 0x37);

// Label hierarchy (Apple: label / secondary / tertiary / quaternary).
pub const TEXT: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf7);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x98, 0x98, 0x9f);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x63, 0x63, 0x69);
pub const TEXT_QUATERNARY: Color32 = Color32::from_rgb(0x48, 0x48, 0x4c);

// System colours (dark variants).
pub const BLUE: Color32 = Color32::from_rgb(0x0a, 0x84, 0xff);
pub const GREEN: Color32 = Color32::from_rgb(0x30, 0xd1, 0x58);
pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x9f, 0x0a);
pub const RED: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);
pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xd6, 0x0a);
pub const PURPLE: Color32 = Color32::from_rgb(0xbf, 0x5a, 0xf2);
pub const TEAL: Color32 = Color32::from_rgb(0x40, 0xc8, 0xe0);

// Semantic aliases.
pub const ACCENT: Color32 = BLUE;
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub const PLAYING: Color32 = GREEN;
pub const LTC: Color32 = ORANGE;
pub const MTC: Color32 = PURPLE;
pub const WARN: Color32 = YELLOW;
pub const ERROR: Color32 = RED;
pub const MUTE: Color32 = Color32::from_rgb(0x5a, 0xa9, 0xff);
pub const SOLO: Color32 = YELLOW;

/// Region colours offered for new tracks, cycled in order.
pub const TRACK_COLORS: [[u8; 3]; 8] = [
    [0x5e, 0x9e, 0xff],
    [0x3d, 0xc8, 0x8a],
    [0xff, 0xa4, 0x3d],
    [0xbf, 0x7a, 0xff],
    [0xff, 0x5e, 0x7a],
    [0x4c, 0xd3, 0xe0],
    [0xff, 0xd6, 0x4a],
    [0x8e, 0x8e, 0xff],
];

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Linear blend between two colours (`t` = 0 → `a`, 1 → `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Kept for call sites that want fixed-width digits in small labels.
pub fn mono(size: f32) -> egui::FontId {
    fonts::mono(size)
}

pub fn apply(ctx: &egui::Context) {
    fonts::install(ctx);

    let mut v = Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = BG_PANEL;
    v.window_fill = Color32::from_rgb(0x2a, 0x2a, 0x2d);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(0x4a, 0x4a, 0x4e));
    v.window_corner_radius = CornerRadius::same(12);
    v.window_shadow = Shadow { offset: [0, 18], blur: 48, spread: 0, color: Color32::from_black_alpha(150) };
    v.popup_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(130) };
    v.menu_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = Color32::from_rgb(0x1a, 0x1a, 0x1c);
    v.faint_bg_color = Color32::from_rgb(0x2e, 0x2e, 0x31);
    v.code_bg_color = BG_DEEP;
    v.hyperlink_color = BLUE;
    v.warn_fg_color = YELLOW;
    v.error_fg_color = RED;
    v.selection.bg_fill = BLUE.linear_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.slider_trailing_fill = true;
    v.striped = false;
    v.button_frame = true;

    let r6 = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_PANEL;
    w.noninteractive.weak_bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, SEPARATOR);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    w.noninteractive.corner_radius = r6;
    for (state, fill, fg) in [
        (&mut w.inactive, BG_WIDGET, TEXT),
        (&mut w.hovered, BG_WIDGET_HOVER, TEXT),
        (&mut w.active, BG_WIDGET_ACTIVE, TEXT),
        (&mut w.open, BG_WIDGET_HOVER, TEXT),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = r6;
        state.expansion = 0.0;
        state.bg_stroke = Stroke::new(0.5, Color32::from_white_alpha(18));
        state.fg_stroke = Stroke::new(1.0, fg);
    }
    ctx.set_visuals(v);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size = egui::vec2(28.0, 24.0);
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.window_margin = egui::Margin::same(18);
        s.spacing.combo_width = 160.0;
        s.spacing.slider_rail_height = 4.0;
        s.interaction.selectable_labels = false;
        s.text_styles.insert(TextStyle::Body, fonts::text(13.0));
        s.text_styles.insert(TextStyle::Button, fonts::text(13.0));
        s.text_styles.insert(TextStyle::Small, fonts::text(11.0));
        s.text_styles.insert(TextStyle::Monospace, fonts::mono(12.0));
        s.text_styles.insert(TextStyle::Heading, fonts::semibold(15.0));
    });
}
