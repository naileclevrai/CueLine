//! Bottom status bar: audio engine and timecode output details.

use std::sync::atomic::Ordering;

use eframe::egui::{self, pos2, vec2, Align2, CornerRadius, Rect, Sense};

use super::widgets::{meter_pos, paint_hmeter};
use super::{fonts, theme, METER_LTC};
use crate::app::CueLineApp;

pub const HEIGHT: f32 = 26.0;

fn item(ui: &mut egui::Ui, dot: Option<egui::Color32>, text: &str, color: egui::Color32) -> egui::Response {
    let font = fonts::text(11.5);
    let w = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color).size().x + if dot.is_some() { 14.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, HEIGHT), Sense::hover());
    let p = ui.painter();
    let mut x = rect.left();
    if let Some(c) = dot {
        p.circle_filled(pos2(x + 4.0, rect.center().y), 3.5, c);
        x += 14.0;
    }
    p.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, text, font, color);
    resp
}

fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 12.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, theme::SEPARATOR);
}

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let sh = app.shared.clone();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.add_space(4.0);

        let channels = app.output_channels() as i32;
        let ltc = &app.project.ltc;
        let routed = ltc.enabled && ltc.channel >= 0 && ltc.channel < channels;
        let ltc_text = if routed {
            format!("LTC  {} fps  →  Out {}  ·  {:.1} dBFS", app.project.frame_rate.label(), ltc.channel + 1, ltc.level_db)
        } else if ltc.enabled {
            "LTC not routed — choose an output".to_string()
        } else {
            "LTC off".to_string()
        };
        item(ui, Some(if routed { theme::LTC } else { theme::TEXT_FAINT }), &ltc_text, theme::TEXT_DIM);
        let lv = app.ui.meter(METER_LTC, sh.ltc_peak.take());
        if meter_pos(lv) > 0.0 || routed {
            let (r, _) = ui.allocate_exact_size(vec2(54.0, 6.0), Sense::hover());
            paint_hmeter(ui.painter(), r, &[lv]);
        }
        divider(ui);

        let mtc = &app.project.mtc;
        let connected = app.mtc.status.connected.load(Ordering::Relaxed);
        let mtc_text = match (&mtc.port, mtc.enabled) {
            (Some(p), true) if connected => format!("MTC  →  {p}"),
            (Some(p), true) => format!("MTC  {p} unavailable"),
            (None, true) => "MTC  no port selected".to_string(),
            _ => "MTC off".to_string(),
        };
        let mtc_color = if mtc.enabled && connected { theme::MTC } else { theme::TEXT_FAINT };
        item(ui, Some(mtc_color), &mtc_text, theme::TEXT_DIM);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(4.0);
            match &app.engine {
                Some(e) => {
                    let load = sh.dsp_load.load();
                    let (r, _) = ui.allocate_exact_size(vec2(40.0, 6.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, CornerRadius::same(3), theme::BG_DEEP);
                    let fill = Rect::from_min_size(r.min, vec2(r.width() * load.clamp(0.0, 1.0), r.height()));
                    let c = if load > 0.7 { theme::ORANGE } else { theme::TEXT_DIM };
                    p.rect_filled(fill, CornerRadius::same(3), c);
                    item(ui, None, &format!("DSP {:.0}%", load * 100.0), theme::TEXT_DIM);
                    divider(ui);
                    let buf = sh.buffer_frames.load(Ordering::Relaxed);
                    let lat = sh.latency_ns.load(Ordering::Relaxed) as f32 / 1e6;
                    item(ui, None, &format!("{:.1} kHz  ·  {buf} samples  ·  {lat:.1} ms", e.sample_rate as f32 / 1000.0), theme::TEXT_DIM);
                    divider(ui);
                    if ui.available_width() > 120.0 {
                        item(ui, Some(theme::GREEN), &format!("{} · {}", e.host_name, e.device_name), theme::TEXT_DIM);
                    }
                }
                None => {
                    item(ui, Some(theme::RED), "No audio device — open Settings", theme::TEXT)
                        .on_hover_text(app.engine_error.clone().unwrap_or_default());
                }
            }
        });
    });
}
