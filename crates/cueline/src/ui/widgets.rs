//! Hand-drawn controls with a macOS look: tabular timecode text, traffic
//! lights, toolbar buttons, switches, segmented controls, faders, knobs,
//! meters and vector icons.

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui,
};

use super::{fonts, theme};

// ----- text -----------------------------------------------------------------

/// Width of the widest digit, so numbers never jitter while they change.
fn digit_width(p: &Painter, font: &FontId) -> f32 {
    (0..10).map(|d| p.layout_no_wrap(d.to_string(), font.clone(), Color32::WHITE).size().x).fold(0.0, f32::max)
}

/// Measures `text` drawn with tabular digits.
pub fn tabular_width(p: &Painter, text: &str, font: &FontId) -> f32 {
    let dw = digit_width(p, font);
    text.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                dw
            } else {
                p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x
            }
        })
        .sum()
}

/// Draws `text` with every digit centred in a fixed-width cell (true tabular
/// figures, which egui cannot request from the font itself).
pub fn tabular(p: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) -> Rect {
    let dw = digit_width(p, &font);
    let width = tabular_width(p, text, &font);
    let height = p.layout_no_wrap("0".into(), font.clone(), color).size().y;
    let rect = align.anchor_size(pos, vec2(width, height));
    let mut x = rect.left();
    for c in text.chars() {
        let g = p.layout_no_wrap(c.to_string(), font.clone(), color);
        let w = if c.is_ascii_digit() { dw } else { g.size().x };
        p.galley(pos2(x + (w - g.size().x) * 0.5, rect.top()), g, color);
        x += w;
    }
    rect
}

// ----- window chrome ----------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Close,
    Minimize,
    Zoom,
}

/// The three macOS window buttons. Glyphs appear when the group is hovered.
pub fn traffic_lights(ui: &mut Ui) -> Option<WindowAction> {
    let (rect, _) = ui.allocate_exact_size(vec2(56.0, 14.0), Sense::hover());
    let group_hovered = ui.rect_contains_pointer(rect.expand(4.0));
    let focused = ui.ctx().input(|i| i.viewport().focused.unwrap_or(true));
    let specs = [
        (WindowAction::Close, Color32::from_rgb(0xff, 0x5f, 0x57), Color32::from_rgb(0xe0, 0x44, 0x3e)),
        (WindowAction::Minimize, Color32::from_rgb(0xfe, 0xbc, 0x2e), Color32::from_rgb(0xd8, 0xa0, 0x24)),
        (WindowAction::Zoom, Color32::from_rgb(0x28, 0xc8, 0x40), Color32::from_rgb(0x1a, 0xab, 0x29)),
    ];
    let mut action = None;
    for (i, (kind, fill, edge)) in specs.into_iter().enumerate() {
        let c = pos2(rect.left() + 6.0 + i as f32 * 20.0, rect.center().y);
        let r = Rect::from_center_size(c, vec2(12.0, 12.0));
        let resp = ui.interact(r, ui.id().with(("traffic", i)), Sense::click());
        let p = ui.painter();
        let (fill, edge) =
            if focused || group_hovered { (fill, edge) } else { (theme::BG_WIDGET_HOVER, theme::BG_WIDGET) };
        let fill = if resp.is_pointer_button_down_on() { theme::mix(fill, Color32::BLACK, 0.25) } else { fill };
        p.circle_filled(c, 6.0, fill);
        p.circle_stroke(c, 5.75, Stroke::new(0.5, edge));
        if group_hovered {
            let g = Color32::from_rgba_unmultiplied(0, 0, 0, 150);
            match kind {
                WindowAction::Close => {
                    p.line_segment([c + vec2(-2.5, -2.5), c + vec2(2.5, 2.5)], Stroke::new(1.2, g));
                    p.line_segment([c + vec2(-2.5, 2.5), c + vec2(2.5, -2.5)], Stroke::new(1.2, g));
                }
                WindowAction::Minimize => {
                    p.line_segment([c + vec2(-3.0, 0.0), c + vec2(3.0, 0.0)], Stroke::new(1.3, g));
                }
                WindowAction::Zoom => {
                    p.add(Shape::convex_polygon(
                        vec![c + vec2(-3.0, -3.0), c + vec2(1.5, -3.0), c + vec2(-3.0, 1.5)],
                        g,
                        Stroke::NONE,
                    ));
                    p.add(Shape::convex_polygon(
                        vec![c + vec2(3.0, 3.0), c + vec2(-1.5, 3.0), c + vec2(3.0, -1.5)],
                        g,
                        Stroke::NONE,
                    ));
                }
            }
        }
        if resp.clicked() {
            action = Some(kind);
        }
    }
    action
}

// ----- icons ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    ToStart,
    ToEnd,
    Follow,
    Gear,
    Sidebar,
    Plus,
    Clock,
    Folder,
}

pub fn paint_icon(p: &Painter, icon: Icon, c: Pos2, s: f32, color: Color32) {
    let stroke = Stroke::new((s * 0.17).max(1.2), color);
    match icon {
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![c + vec2(-s * 0.42, -s * 0.55), c + vec2(s * 0.58, 0.0), c + vec2(-s * 0.42, s * 0.55)],
                color,
                Stroke::new(1.0, color),
            ));
        }
        Icon::Pause => {
            for dx in [-0.3, 0.3] {
                let r = Rect::from_center_size(c + vec2(dx * s, 0.0), vec2(s * 0.27, s * 1.1));
                p.rect_filled(r, CornerRadius::same(1), color);
            }
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(s, s)), CornerRadius::same(2), color);
        }
        Icon::ToStart | Icon::ToEnd => {
            let d = if icon == Icon::ToStart { -1.0 } else { 1.0 };
            p.rect_filled(
                Rect::from_center_size(c + vec2(d * s * 0.5, 0.0), vec2(s * 0.16, s * 1.0)),
                CornerRadius::same(1),
                color,
            );
            for k in [0.0, 1.0] {
                let tip = c + vec2(d * (s * 0.38 - k * s * 0.5), 0.0);
                let back = tip.x - d * s * 0.5;
                p.add(Shape::convex_polygon(
                    vec![tip, pos2(back, c.y - s * 0.45), pos2(back, c.y + s * 0.45)],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::Follow => {
            p.line_segment([c + vec2(-s * 0.15, -s * 0.6), c + vec2(-s * 0.15, s * 0.6)], stroke);
            p.add(Shape::convex_polygon(
                vec![c + vec2(s * 0.1, -s * 0.35), c + vec2(s * 0.6, 0.0), c + vec2(s * 0.1, s * 0.35)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Gear => {
            let teeth = 8;
            let mut pts = Vec::new();
            for i in 0..teeth * 2 {
                let a = i as f32 / (teeth * 2) as f32 * std::f32::consts::TAU;
                let r = if i % 2 == 0 { s * 0.62 } else { s * 0.48 };
                pts.push(c + vec2(a.cos() * r, a.sin() * r));
            }
            pts.push(pts[0]);
            p.add(Shape::line(pts, stroke));
            p.circle_stroke(c, s * 0.2, stroke);
        }
        Icon::Sidebar => {
            let r = Rect::from_center_size(c, vec2(s * 1.4, s * 1.1));
            p.rect_stroke(r, CornerRadius::same(2), stroke, StrokeKind::Middle);
            let x = r.right() - s * 0.45;
            p.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], stroke);
        }
        Icon::Plus => {
            p.line_segment([c + vec2(-s * 0.5, 0.0), c + vec2(s * 0.5, 0.0)], stroke);
            p.line_segment([c + vec2(0.0, -s * 0.5), c + vec2(0.0, s * 0.5)], stroke);
        }
        Icon::Folder => {
            let body = Rect::from_center_size(c + vec2(0.0, s * 0.1), vec2(s * 1.4, s * 0.95));
            let tab = Rect::from_min_size(body.left_top() - vec2(0.0, s * 0.22), vec2(s * 0.6, s * 0.3));
            p.rect_filled(tab, CornerRadius::same(2), color);
            p.rect_filled(body, CornerRadius::same(2), color);
        }
        Icon::Clock => {
            p.circle_stroke(c, s * 0.6, stroke);
            p.line_segment([c, c + vec2(0.0, -s * 0.38)], stroke);
            p.line_segment([c, c + vec2(s * 0.28, s * 0.12)], stroke);
        }
    }
}

/// Borderless toolbar button that lights up on hover, like NSToolbar items.
pub fn tool_button(ui: &mut Ui, icon: Icon, active: bool, tint: Color32, size: egui::Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let fill = if resp.is_pointer_button_down_on() {
        theme::BG_WIDGET_ACTIVE
    } else if active {
        tint.gamma_multiply(0.22)
    } else if resp.hovered() {
        theme::BG_WIDGET
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(rect, CornerRadius::same(6), fill);
    let fg = if active { tint } else { theme::TEXT };
    paint_icon(p, icon, rect.center(), 11.0, fg);
    resp
}

/// Rounded container that groups related toolbar buttons.
pub fn button_group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x3a, 0x3a, 0x3d))
        .stroke(Stroke::new(0.5, Color32::from_white_alpha(14)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add).inner
        })
        .inner
}

// ----- controls -----------------------------------------------------------------

/// macOS switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let size = vec2(32.0, 18.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let p = ui.painter();
    let track = theme::mix(Color32::from_rgb(0x48, 0x48, 0x4c), theme::GREEN, t);
    p.rect_filled(rect, CornerRadius::same(9), track);
    let knob_x = egui::lerp((rect.left() + 9.0)..=(rect.right() - 9.0), t);
    p.circle_filled(pos2(knob_x, rect.center().y + 0.5), 7.5, Color32::from_black_alpha(60));
    p.circle_filled(pos2(knob_x, rect.center().y), 7.5, Color32::from_rgb(0xf5, 0xf5, 0xf7));
    resp
}

/// Segmented control; returns true when the selection changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    let font = fonts::medium(12.0);
    let widths: Vec<f32> = options
        .iter()
        .map(|(_, l)| ui.painter().layout_no_wrap(l.to_string(), font.clone(), Color32::WHITE).size().x + 22.0)
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 26.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(7), Color32::from_rgb(0x1f, 0x1f, 0x21));
    let mut x = rect.left() + 2.0;
    for (i, ((v, label), w)) in options.iter().zip(widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(w, rect.height() - 4.0));
        let resp = ui.interact(r, ui.id().with(("seg", i)), Sense::click());
        let selected = *value == *v;
        let p = ui.painter();
        if selected {
            p.rect_filled(r.translate(vec2(0.0, 0.5)), CornerRadius::same(5), Color32::from_black_alpha(70));
            p.rect_filled(r, CornerRadius::same(5), Color32::from_rgb(0x5a, 0x5a, 0x5e));
        } else if resp.hovered() {
            p.rect_filled(r, CornerRadius::same(5), Color32::from_white_alpha(8));
        }
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            *label,
            font.clone(),
            if selected { theme::TEXT } else { theme::TEXT_DIM },
        );
        if resp.clicked() && !selected {
            *value = *v;
            changed = true;
        }
        x += w;
    }
    changed
}

/// Small rounded toggle (mute / solo), Logic style.
pub fn chip(ui: &mut Ui, text: &str, on: bool, color: Color32, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
    let p = ui.painter();
    let fill = if on {
        color
    } else if resp.hovered() {
        theme::BG_WIDGET_HOVER
    } else {
        theme::BG_WIDGET
    };
    p.rect_filled(rect, CornerRadius::same(4), fill);
    let fg = if on { Color32::from_rgb(0x12, 0x12, 0x14) } else { theme::TEXT_DIM };
    p.text(rect.center(), Align2::CENTER_CENTER, text, fonts::semibold(10.5), fg);
    resp.on_hover_text(tip)
}

/// Horizontal fader mapping -60..+6 dB with an audio taper.
pub fn fader(ui: &mut Ui, db: &mut f32, width: f32, color: Color32) -> Response {
    const MIN: f32 = -60.0;
    const MAX: f32 = 6.0;
    let to_pos = |db: f32| ((db.clamp(MIN, MAX) - MIN) / (MAX - MIN)).powf(2.2);
    let from_pos = |t: f32| MIN + t.clamp(0.0, 1.0).powf(1.0 / 2.2) * (MAX - MIN);
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 16.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 12.0, 4.0));
    if resp.double_clicked() {
        *db = 0.0;
        resp.mark_changed();
    } else if let (true, Some(pos)) = (resp.dragged() || resp.clicked(), resp.interact_pointer_pos()) {
        let fine = ui.input(|i| i.modifiers.shift);
        let new = if fine && resp.dragged() {
            *db + resp.drag_delta().x * 0.05
        } else {
            from_pos((pos.x - track.left()) / track.width())
        };
        let new = (new * 10.0).round() / 10.0;
        if new != *db {
            *db = new.clamp(MIN, MAX);
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(2), Color32::from_rgb(0x18, 0x18, 0x1a));
    let x = track.left() + to_pos(*db) * track.width();
    p.rect_filled(
        Rect::from_min_max(track.left_top(), pos2(x, track.bottom())),
        CornerRadius::same(2),
        color.gamma_multiply(0.75),
    );
    let unity = track.left() + to_pos(0.0) * track.width();
    p.vline(unity, (track.top() - 3.0)..=(track.bottom() + 3.0), Stroke::new(1.0, theme::TEXT_FAINT));
    let knob = Rect::from_center_size(pos2(x, rect.center().y), vec2(10.0, 14.0));
    p.rect_filled(knob.translate(vec2(0.0, 1.0)), CornerRadius::same(3), Color32::from_black_alpha(90));
    let fill = if resp.hovered() || resp.dragged() { Color32::WHITE } else { Color32::from_rgb(0xe5, 0xe5, 0xea) };
    p.rect_filled(knob, CornerRadius::same(3), fill);
    resp.on_hover_text(format!("{:+.1} dB — double-click for 0 dB, Shift for fine", *db))
}

/// Rotary pan knob (-1..1); drag vertically, double-click to centre.
pub fn knob(ui: &mut Ui, value: &mut f32, color: Color32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click_and_drag());
    if resp.double_clicked() {
        *value = 0.0;
        resp.mark_changed();
    } else if resp.dragged() {
        let d = resp.drag_delta();
        let new = (*value + (d.x - d.y) * 0.01).clamp(-1.0, 1.0);
        if new != *value {
            *value = new;
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    let c = rect.center();
    let r = 8.0;
    p.circle_filled(c, r, Color32::from_rgb(0x2e, 0x2e, 0x31));
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let arc = |a0: f32, a1: f32| -> Vec<Pos2> {
        (0..=16)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 16.0;
                c + vec2(a.cos(), a.sin()) * (r + 1.5)
            })
            .collect()
    };
    p.add(Shape::line(arc(start, start + sweep), Stroke::new(2.0, Color32::from_rgb(0x18, 0x18, 0x1a))));
    let mid = start + sweep * 0.5;
    let at = start + sweep * (*value + 1.0) * 0.5;
    if value.abs() > 0.005 {
        p.add(Shape::line(arc(mid.min(at), mid.max(at)), Stroke::new(2.0, color)));
    }
    p.circle_filled(
        c,
        r - 2.0,
        if resp.hovered() { Color32::from_rgb(0x58, 0x58, 0x5c) } else { Color32::from_rgb(0x4a, 0x4a, 0x4e) },
    );
    p.line_segment(
        [c + vec2(at.cos(), at.sin()) * 2.0, c + vec2(at.cos(), at.sin()) * (r - 2.5)],
        Stroke::new(1.6, theme::TEXT),
    );
    let label = match (*value * 100.0).round() as i32 {
        0 => "Center".to_string(),
        v if v < 0 => format!("{} L", -v),
        v => format!("{v} R"),
    };
    resp.on_hover_text(format!("Pan {label} — double-click to centre"))
}

// ----- meters -------------------------------------------------------------------

/// Converts a linear peak to a 0..1 meter position (-60..0 dBFS).
pub fn meter_pos(peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn meter_color(pos: f32) -> Color32 {
    if pos > 0.985 {
        theme::RED
    } else if pos > 0.85 {
        theme::YELLOW
    } else {
        theme::GREEN
    }
}

/// Horizontal meter, one bar per channel.
pub fn paint_hmeter(p: &Painter, rect: Rect, levels: &[f32]) {
    p.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(0x14, 0x14, 0x16));
    let n = levels.len().max(1) as f32;
    let gap = 1.0;
    let h = (rect.height() - 2.0 - (n - 1.0) * gap) / n;
    for (i, &lv) in levels.iter().enumerate() {
        let y = rect.top() + 1.0 + i as f32 * (h + gap);
        let pos = meter_pos(lv);
        let bar = Rect::from_min_size(pos2(rect.left() + 1.0, y), vec2((rect.width() - 2.0) * pos, h));
        p.rect_filled(bar, CornerRadius::same(1), meter_color(pos));
    }
}

/// Vertical meter, one bar per channel.
pub fn paint_vmeter(p: &Painter, rect: Rect, levels: &[f32]) {
    p.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(0x14, 0x14, 0x16));
    let n = levels.len().max(1) as f32;
    let w = (rect.width() - 2.0 - (n - 1.0)) / n;
    for (i, &lv) in levels.iter().enumerate() {
        let x = rect.left() + 1.0 + i as f32 * (w + 1.0);
        let pos = meter_pos(lv);
        let h = (rect.height() - 2.0) * pos;
        let bar = Rect::from_min_max(pos2(x, rect.bottom() - 1.0 - h), pos2(x + w, rect.bottom() - 1.0));
        p.rect_filled(bar, CornerRadius::same(1), meter_color(pos));
    }
}

/// Toolbar status capsule with a coloured dot, e.g. "● LTC  Out 2".
pub fn pill(ui: &mut Ui, label: &str, detail: &str, on: bool, color: Color32) -> Response {
    let font = fonts::semibold(11.5);
    let dfont = fonts::text(11.5);
    let p = ui.painter();
    let lw = p.layout_no_wrap(label.into(), font.clone(), Color32::WHITE).size().x;
    let dw = if detail.is_empty() {
        0.0
    } else {
        p.layout_no_wrap(detail.into(), dfont.clone(), Color32::WHITE).size().x + 6.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0 + lw + dw + 10.0, 24.0), Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() { theme::BG_WIDGET_HOVER } else { theme::BG_WIDGET };
    p.rect_filled(rect, CornerRadius::same(12), if on { theme::mix(fill, color, 0.16) } else { fill });
    let dot = pos2(rect.left() + 13.0, rect.center().y);
    if on {
        p.circle_filled(dot, 6.5, color.gamma_multiply(0.25));
    }
    p.circle_filled(dot, 3.5, if on { color } else { theme::TEXT_FAINT });
    p.text(
        pos2(rect.left() + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font,
        if on { theme::TEXT } else { theme::TEXT_DIM },
    );
    if !detail.is_empty() {
        p.text(pos2(rect.left() + 30.0 + lw, rect.center().y), Align2::LEFT_CENTER, detail, dfont, theme::TEXT_DIM);
    }
    resp
}
