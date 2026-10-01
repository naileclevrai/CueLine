//! Small custom widgets: vector transport icons, meters, status chips.

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, Response, Sense, Shape, Stroke, StrokeKind, Ui};

use super::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    ToStart,
    ToEnd,
    Follow,
}

/// A square button with a crisp vector icon (no font glyph dependency).
pub fn icon_button(ui: &mut Ui, icon: Icon, active: bool, tint: Color32) -> Response {
    let size = vec2(34.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let bg = if resp.is_pointer_button_down_on() {
        theme::BG_WIDGET_HOVER.gamma_multiply(1.3)
    } else if resp.hovered() {
        theme::BG_WIDGET_HOVER
    } else {
        theme::BG_WIDGET
    };
    p.rect_filled(rect, CornerRadius::same(3), bg);
    let border = if active { tint } else { theme::BORDER };
    p.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0, border), StrokeKind::Inside);
    let c = rect.center();
    let fg = if active { tint } else { theme::TEXT };
    let s = 6.5;
    match icon {
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![pos2(c.x - s * 0.8, c.y - s), pos2(c.x + s, c.y), pos2(c.x - s * 0.8, c.y + s)],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for dx in [-3.5, 3.5] {
                let r = egui::Rect::from_center_size(pos2(c.x + dx, c.y), vec2(3.5, s * 2.0));
                p.rect_filled(r, CornerRadius::ZERO, fg);
            }
        }
        Icon::Stop => {
            p.rect_filled(egui::Rect::from_center_size(c, vec2(s * 1.7, s * 1.7)), CornerRadius::same(1), fg);
        }
        Icon::ToStart | Icon::ToEnd => {
            let dir = if icon == Icon::ToStart { -1.0 } else { 1.0 };
            let bar_x = c.x + dir * s;
            p.rect_filled(egui::Rect::from_center_size(pos2(bar_x, c.y), vec2(2.5, s * 2.0)), CornerRadius::ZERO, fg);
            p.add(Shape::convex_polygon(
                vec![pos2(c.x + dir * s * 0.7, c.y), pos2(c.x - dir * s * 0.9, c.y - s), pos2(c.x - dir * s * 0.9, c.y + s)],
                fg,
                Stroke::NONE,
            ));
        }
        Icon::Follow => {
            p.line_segment([pos2(c.x, c.y - s), pos2(c.x, c.y + s)], Stroke::new(2.0, fg));
            p.add(Shape::convex_polygon(
                vec![pos2(c.x + 2.0, c.y - 4.0), pos2(c.x + s + 1.0, c.y), pos2(c.x + 2.0, c.y + 4.0)],
                fg,
                Stroke::NONE,
            ));
        }
    }
    resp
}

/// Converts a linear peak to a 0..1 meter position (-60..0 dBFS).
pub fn meter_pos(peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Horizontal peak meter with green/yellow/red zones.
pub fn hmeter(ui: &mut Ui, width: f32, height: f32, levels: &[f32]) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    paint_hmeter(ui.painter(), rect, levels);
}

pub fn paint_hmeter(p: &egui::Painter, rect: egui::Rect, levels: &[f32]) {
    p.rect_filled(rect, CornerRadius::same(1), theme::BG_DEEP);
    let n = levels.len().max(1) as f32;
    let h = (rect.height() - (n - 1.0)) / n;
    for (i, &lv) in levels.iter().enumerate() {
        let y = rect.top() + i as f32 * (h + 1.0);
        let w = rect.width() * meter_pos(lv);
        let bar = egui::Rect::from_min_size(pos2(rect.left(), y), vec2(w, h));
        let color = if lv >= 0.99 {
            theme::ERROR
        } else if lv > 0.5 {
            theme::WARN
        } else {
            theme::PLAYING
        };
        p.rect_filled(bar, CornerRadius::ZERO, color.gamma_multiply(0.9));
    }
}

/// Small rounded label with a coloured status dot.
pub fn status_chip(ui: &mut Ui, label: &str, detail: &str, on: bool, color: Color32) -> Response {
    let frame = egui::Frame::new()
        .fill(theme::BG_WIDGET)
        .stroke(Stroke::new(1.0, if on { color.gamma_multiply(0.6) } else { theme::BORDER }))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(egui::Margin::symmetric(8, 4));
    frame
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                let dot = if on { color } else { theme::TEXT_FAINT };
                ui.painter().circle_filled(r.center(), 4.0, dot);
                ui.label(egui::RichText::new(label).strong().color(if on { theme::TEXT } else { theme::TEXT_DIM }));
                if !detail.is_empty() {
                    ui.label(egui::RichText::new(detail).small().color(theme::TEXT_DIM));
                }
            });
        })
        .response
        .interact(Sense::click())
}
