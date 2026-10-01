//! Top transport bar: big timecode display, transport controls, I/O status.

use std::sync::atomic::Ordering;

use cueline_core::Timecode;
use eframe::egui::{self, CornerRadius, RichText, Stroke};

use super::theme;
use super::widgets::{hmeter, icon_button, status_chip, Icon};
use super::{METER_LTC, METER_MASTER_L, METER_MASTER_R};
use crate::app::CueLineApp;

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let playing = app.is_playing();
    let pos = app.position_secs();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        timecode_display(app, ui, pos, playing);
        ui.add_space(8.0);

        if icon_button(ui, Icon::ToStart, false, theme::TEXT).on_hover_text("Go to start (Home)").clicked() {
            app.seek(0.0);
        }
        let (icon, tip) = if playing { (Icon::Pause, "Pause (Shift+Space)") } else { (Icon::Play, "Play (Space)") };
        if icon_button(ui, icon, playing, theme::PLAYING).on_hover_text(tip).clicked() {
            if playing {
                app.pause();
            } else {
                app.play();
            }
        }
        if icon_button(ui, Icon::Stop, false, theme::TEXT).on_hover_text("Stop and return (Space)").clicked() {
            if playing {
                app.stop();
            } else {
                app.seek(0.0);
            }
        }
        if icon_button(ui, Icon::ToEnd, false, theme::TEXT).on_hover_text("Go to end (End)").clicked() {
            app.seek(app.project_end_secs());
        }
        let follow = app.settings.follow_playhead;
        if icon_button(ui, Icon::Follow, follow, theme::ACCENT).on_hover_text("Follow playhead (F)").clicked() {
            app.settings.follow_playhead = !follow;
        }

        ui.add_space(12.0);
        io_status(app, ui, playing);
    });
}

fn timecode_display(app: &mut CueLineApp, ui: &mut egui::Ui, pos: f64, playing: bool) {
    let rate = app.project.frame_rate;
    let tc = app.timecode_at(pos);
    let frame = egui::Frame::new()
        .fill(theme::BG_DEEP)
        .stroke(Stroke::new(1.0, if playing { theme::PLAYING.gamma_multiply(0.5) } else { theme::BORDER }))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(egui::Margin { left: 12, right: 12, top: 2, bottom: 3 });
    frame.show(ui, |ui| {
        ui.set_min_width(250.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(text) = &mut app.ui.goto_text {
                let edit = ui.add(
                    egui::TextEdit::singleline(text)
                        .font(theme::mono(28.0))
                        .desired_width(230.0)
                        .frame(egui::Frame::NONE)
                        .hint_text("HH:MM:SS:FF"),
                );
                edit.request_focus();
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if enter {
                    let text = text.clone();
                    match Timecode::parse(&text, rate) {
                        Some(tc) => {
                            app.ui.goto_text = None;
                            app.seek_timecode(tc);
                        }
                        None => app.ui.toast_error(format!("\"{text}\" is not a valid {rate} timecode")),
                    }
                } else if escape || edit.lost_focus() {
                    app.ui.goto_text = None;
                }
            } else {
                let color = if playing { theme::PLAYING } else { theme::TEXT };
                let resp = ui
                    .add(
                        egui::Label::new(
                            RichText::new(tc.display(rate).to_string()).font(theme::mono(28.0)).color(color),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_text("Click to go to a timecode (G)");
                if resp.clicked() {
                    app.ui.goto_text = Some(tc.to_string());
                }
            }
            ui.horizontal(|ui| {
                let secs = pos.max(0.0);
                let sign = if pos < 0.0 { "-" } else { "" };
                let m = (secs / 60.0).floor();
                let s = secs - m * 60.0;
                ui.label(
                    RichText::new(format!("{sign}{m:02}:{s:06.3}")).font(theme::mono(11.5)).color(theme::TEXT_DIM),
                );
                ui.add_space(6.0);
                ui.label(RichText::new(format!("{} fps", rate.label())).font(theme::mono(11.5)).color(theme::LTC));
                if rate.is_drop() {
                    ui.label(RichText::new("DF").font(theme::mono(11.5)).color(theme::WARN));
                }
            });
        });
    });
}

fn io_status(app: &mut CueLineApp, ui: &mut egui::Ui, playing: bool) {
    let sh = app.shared.clone();
    let channels = app.output_channels() as i32;

    let ltc_on = app.project.ltc.enabled && app.project.ltc.channel >= 0 && app.project.ltc.channel < channels;
    let ltc_detail = if ltc_on {
        format!("out {} · {:.0} dB", app.project.ltc.channel + 1, app.project.ltc.level_db)
    } else if app.project.ltc.enabled {
        "not routed".into()
    } else {
        "off".into()
    };
    let ltc_active = ltc_on && playing;
    if status_chip(ui, "LTC", &ltc_detail, ltc_active, theme::LTC).on_hover_text("Click to toggle LTC output").clicked()
    {
        app.project.ltc.enabled = !app.project.ltc.enabled;
        app.dirty = true;
        app.apply_project_to_engine();
    }
    let ltc = app.ui.meter(METER_LTC, sh.ltc_peak.take());
    hmeter(ui, 46.0, 8.0, &[ltc]);

    let mtc = &app.mtc.status;
    let connected = mtc.connected.load(Ordering::Relaxed);
    let sending = mtc.sending.load(Ordering::Relaxed);
    let mtc_detail = match (&app.project.mtc.port, app.project.mtc.enabled) {
        (_, false) => "off".to_string(),
        (None, true) => "no port".to_string(),
        (Some(p), true) if connected => short(p, 18),
        (Some(_), true) => "port error".to_string(),
    };
    let mtc_on = app.project.mtc.enabled && connected;
    let hover = mtc.error.lock().unwrap().clone().unwrap_or_else(|| "Click to toggle MIDI Timecode output".into());
    if status_chip(ui, "MTC", &mtc_detail, mtc_on && (sending || !playing), theme::MTC).on_hover_text(hover).clicked() {
        app.project.mtc.enabled = !app.project.mtc.enabled;
        app.dirty = true;
        app.apply_project_to_engine();
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let l = app.ui.meter(METER_MASTER_L, sh.master_peak[0].take());
        let r = app.ui.meter(METER_MASTER_R, sh.master_peak[1].take());
        hmeter(ui, 90.0, 14.0, &[l, r]);
        match &app.engine {
            Some(e) => {
                let load = sh.dsp_load.load() * 100.0;
                let buf = sh.buffer_frames.load(Ordering::Relaxed);
                let lat = sh.latency_ns.load(Ordering::Relaxed) as f32 / 1e6;
                ui.label(
                    RichText::new(format!("{} Hz · {buf} smp · {lat:.1} ms · DSP {load:.0}%", e.sample_rate))
                        .font(theme::mono(11.0))
                        .color(if load > 70.0 { theme::WARN } else { theme::TEXT_DIM }),
                );
                ui.label(RichText::new(short(&e.device_name, 28)).color(theme::TEXT_DIM))
                    .on_hover_text(format!("{} — {}", e.host_name, e.device_name));
            }
            None => {
                ui.label(RichText::new("No audio device").color(theme::ERROR))
                    .on_hover_text(app.engine_error.clone().unwrap_or_default());
            }
        }
    });
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
