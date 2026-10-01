//! Separate, resizable window with a huge timecode readout, for a second
//! screen at front of house or on stage.

use eframe::egui::{self, Align2};

use super::widgets::{tabular, tabular_width};
use super::{fonts, theme};
use crate::app::CueLineApp;

pub fn window(app: &mut CueLineApp, ctx: &egui::Context) {
    if !app.ui.show_big_clock {
        return;
    }
    let pos = app.position_secs();
    let playing = app.is_playing();
    let rate = app.project.frame_rate;
    let tc = app.timecode_at(pos).display(rate).to_string();
    let next =
        app.project.markers.iter().find(|m| m.time_secs > pos + 1e-4).map(|m| (m.name.clone(), m.time_secs - pos));
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
            let base = 100.0;
            let w = tabular_width(p, &tc, &fonts::display_light(base)).max(1.0);
            let size = (rect.width() * 0.88 / w * base).min(rect.height() * 0.5);
            let color = if playing { theme::GREEN } else { theme::TEXT };
            tabular(
                p,
                rect.center() - egui::vec2(0.0, size * 0.18),
                Align2::CENTER_CENTER,
                &tc,
                fonts::display_light(size),
                color,
            );
            let info = match &next {
                Some((name, left)) => {
                    format!("NEXT  ·  {}  ·  {:.1} s", if name.is_empty() { "Cue" } else { name }, left)
                }
                None => format!("{} FPS", rate.label()),
            };
            p.text(
                egui::pos2(rect.center().x, rect.center().y + size * 0.48),
                Align2::CENTER_CENTER,
                info,
                fonts::medium((size * 0.15).max(12.0)),
                theme::ORANGE,
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
