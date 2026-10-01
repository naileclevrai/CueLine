//! Separate, resizable window with a huge timecode readout, for a second
//! screen at front of house or on stage.

use eframe::egui::{self, Align2};

use super::theme;
use crate::app::CueLineApp;

pub fn window(app: &mut CueLineApp, ctx: &egui::Context) {
    if !app.ui.show_big_clock {
        return;
    }
    let pos = app.position_secs();
    let playing = app.is_playing();
    let rate = app.project.frame_rate;
    let tc = app.timecode_at(pos).display(rate).to_string();
    let next = app.project.markers.iter().find(|m| m.time_secs > pos + 1e-4).map(|m| (m.name.clone(), m.time_secs - pos));
    let mut close = false;

    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("big_clock"),
        egui::ViewportBuilder::default().with_title("CueLine — Timecode").with_inner_size([760.0, 300.0]),
        |ui, _class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                close = true;
            }
            let rect = ui.max_rect();
            let p = ui.painter();
            p.rect_filled(rect, 0.0, theme::BG_DEEP);
            // 11 monospace glyphs; Hack's advance is ~0.6 em.
            let size = (rect.width() / (11.0 * 0.62)).min(rect.height() * 0.55);
            let color = if playing { theme::PLAYING } else { theme::TEXT };
            p.text(rect.center() - egui::vec2(0.0, size * 0.15), Align2::CENTER_CENTER, &tc, theme::mono(size), color);
            let info = match &next {
                Some((name, left)) => format!("NEXT  {}  in {:.1}s", if name.is_empty() { "marker" } else { name }, left),
                None => format!("{} fps", rate.label()),
            };
            p.text(
                egui::pos2(rect.center().x, rect.bottom() - size * 0.28),
                Align2::CENTER_CENTER,
                info,
                theme::mono((size * 0.18).max(12.0)),
                theme::LTC,
            );
        },
    );
    if close {
        app.ui.show_big_clock = false;
    }
    if playing {
        ctx.request_repaint();
    }
}
