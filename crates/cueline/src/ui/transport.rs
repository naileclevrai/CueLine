//! Unified toolbar: transport controls, the central LCD and I/O status.

use std::sync::atomic::Ordering;

use cueline_core::Timecode;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind};

use super::widgets::{button_group, paint_icon, paint_vmeter, pill, tabular, tool_button, Icon};
use super::{fonts, theme};
use super::{METER_MASTER_L, METER_MASTER_R};
use crate::app::CueLineApp;

pub const HEIGHT: f32 = 58.0;
const LCD_W: f32 = 520.0;
const LCD_H: f32 = 46.0;

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let playing = app.is_playing();
    let pos = app.position_secs();
    let rect = ui.max_rect();

    // Centre the LCD in the window when there is room, otherwise after the
    // transport group.
    let lcd_x = (rect.center().x - LCD_W / 2.0).max(rect.left() + 236.0);
    let lcd = Rect::from_min_size(pos2(lcd_x, rect.center().y - LCD_H / 2.0), vec2(LCD_W, LCD_H));
    lcd_display(app, ui, lcd, pos, playing);

    ui.horizontal_centered(|ui| {
        ui.add_space(14.0);
        let size = vec2(34.0, 28.0);
        button_group(ui, |ui| {
            if tool_button(ui, Icon::ToStart, false, theme::TEXT, size).on_hover_text("Go to start (Home)").clicked() {
                app.seek(0.0);
            }
            if tool_button(ui, Icon::Stop, false, theme::TEXT, size).on_hover_text("Stop (Space)").clicked() {
                if playing {
                    app.stop();
                } else {
                    app.seek(0.0);
                }
            }
            let (icon, tip) = if playing { (Icon::Pause, "Pause (Shift+Space)") } else { (Icon::Play, "Play (Space)") };
            if tool_button(ui, icon, playing, theme::GREEN, size).on_hover_text(tip).clicked() {
                if playing {
                    app.pause();
                } else {
                    app.play();
                }
            }
            if tool_button(ui, Icon::ToEnd, false, theme::TEXT, size).on_hover_text("Go to end (End)").clicked() {
                app.seek(app.project_end_secs());
            }
        });
        ui.add_space(6.0);
        let follow = app.settings.follow_playhead;
        if tool_button(ui, Icon::Follow, follow, theme::BLUE, vec2(32.0, 28.0))
            .on_hover_text("Follow playhead (F)")
            .clicked()
        {
            app.settings.follow_playhead = !follow;
        }
    });

    // Right side, laid out from the right edge.
    let right = Rect::from_min_max(pos2(lcd.right() + 12.0, rect.top()), pos2(rect.right() - 12.0, rect.bottom()));
    if right.width() > 40.0 {
        let mut child = ui
            .new_child(egui::UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)));
        io_status(app, &mut child);
    }
}

fn lcd_display(app: &mut CueLineApp, ui: &mut egui::Ui, lcd: Rect, pos: f64, playing: bool) {
    let rate = app.project.frame_rate;
    let p = ui.painter().clone();
    // Recessed glass: dark body, faint bottom highlight, hairline edge.
    p.rect_filled(lcd.translate(vec2(0.0, 1.0)), CornerRadius::same(9), Color32::from_white_alpha(10));
    p.rect_filled(lcd, CornerRadius::same(9), theme::LCD);
    p.rect_stroke(lcd, CornerRadius::same(9), Stroke::new(1.0, Color32::from_black_alpha(160)), StrokeKind::Inside);

    let label_font = fonts::semibold(8.5);
    // Two bands: values centred in the upper one, captions in the lower one.
    let value_y = lcd.top() + 19.0;
    let caption_y = lcd.bottom() - 9.0;
    let sections = [lcd.left() + 236.0, lcd.left() + 352.0];
    for x in sections {
        p.vline(x, (lcd.top() + 9.0)..=(lcd.bottom() - 9.0), Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2b, 0x2f)));
    }

    // 1. Timecode.
    let tc_area = Rect::from_min_max(lcd.left_top(), pos2(sections[0], lcd.bottom()));
    let state_c = pos2(tc_area.left() + 16.0, value_y);
    if playing {
        paint_icon(&p, Icon::Play, state_c, 7.0, theme::GREEN);
    } else {
        paint_icon(&p, Icon::Stop, state_c, 6.0, theme::TEXT_FAINT);
    }
    let tc = app.timecode_at(pos);
    let tc_pos = pos2(tc_area.left() + 30.0, value_y);
    if app.ui.goto_text.is_none() {
        tabular(
            &p,
            tc_pos,
            Align2::LEFT_CENTER,
            &tc.display(rate).to_string(),
            fonts::display_light(25.0),
            theme::TEXT,
        );
    }
    let caption = if rate.is_drop() {
        format!("TIMECODE  ·  {} DF", rate.label())
    } else {
        format!("TIMECODE  ·  {} FPS", rate.label())
    };
    p.text(pos2(tc_area.left() + 31.0, caption_y), Align2::LEFT_CENTER, caption, label_font.clone(), theme::TEXT_FAINT);

    let resp = ui.interact(tc_area, ui.id().with("lcd_tc"), Sense::click());
    if resp.clicked() && app.ui.goto_text.is_none() {
        app.ui.goto_text = Some(tc.to_string());
    }
    resp.on_hover_text("Click to go to a timecode (G)");
    if app.ui.goto_text.is_some() {
        goto_editor(app, ui, Rect::from_min_size(tc_pos - vec2(0.0, 15.0), vec2(196.0, 30.0)), rate);
    }

    // 2. Elapsed time since project start.
    let secs = pos.max(0.0);
    let m = (secs / 60.0).floor();
    let s = secs - m * 60.0;
    let elapsed = format!("{}{m:02}:{s:05.2}", if pos < 0.0 { "-" } else { "" });
    let ex = sections[0] + 14.0;
    tabular(&p, pos2(ex, value_y), Align2::LEFT_CENTER, &elapsed, fonts::display(16.0), theme::TEXT_DIM);
    p.text(pos2(ex + 1.0, caption_y), Align2::LEFT_CENTER, "ELAPSED", label_font.clone(), theme::TEXT_FAINT);

    // 3. Next cue.
    let nx = sections[1] + 14.0;
    match app.project.markers.iter().enumerate().find(|(_, m)| m.time_secs > pos + 1e-4) {
        Some((i, mk)) => {
            let left = mk.time_secs - pos;
            let m = (left / 60.0).floor();
            let s = left - m * 60.0;
            let soon = left < 5.0 && playing;
            let color = if soon { theme::ORANGE } else { theme::TEXT };
            tabular(
                &p,
                pos2(nx, value_y),
                Align2::LEFT_CENTER,
                &format!("−{m:02}:{s:04.1}"),
                fonts::display(16.0),
                color,
            );
            let name = if mk.name.is_empty() { format!("Marker {}", i + 1) } else { mk.name.clone() };
            let dot = pos2(nx + 3.0, caption_y);
            p.circle_filled(dot, 3.0, theme::rgb(mk.color));
            let galley = p.layout_no_wrap(name.to_uppercase(), label_font.clone(), theme::TEXT_DIM);
            let clip = Rect::from_min_max(pos2(nx + 10.0, lcd.top()), pos2(lcd.right() - 10.0, lcd.bottom()));
            p.with_clip_rect(clip).galley(pos2(nx + 10.0, caption_y - galley.size().y / 2.0), galley, theme::TEXT_DIM);
        }
        None => {
            tabular(
                &p,
                pos2(nx, value_y),
                Align2::LEFT_CENTER,
                "−−:−−.−",
                fonts::display(16.0),
                theme::TEXT_QUATERNARY,
            );
            p.text(pos2(nx + 1.0, caption_y), Align2::LEFT_CENTER, "NEXT CUE", label_font, theme::TEXT_FAINT);
        }
    }
}

fn goto_editor(app: &mut CueLineApp, ui: &mut egui::Ui, rect: Rect, rate: cueline_core::FrameRate) {
    let Some(text) = &mut app.ui.goto_text else { return };
    let edit = ui.put(
        rect,
        egui::TextEdit::singleline(text)
            .font(fonts::display_light(24.0))
            .text_color(theme::ORANGE)
            .frame(egui::Frame::NONE)
            .hint_text("HH:MM:SS:FF"),
    );
    edit.request_focus();
    let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
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
}

fn io_status(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let sh = app.shared.clone();
    ui.spacing_mut().item_spacing.x = 8.0;

    if tool_button(ui, Icon::Sidebar, app.ui.show_markers, theme::BLUE, vec2(32.0, 28.0))
        .on_hover_text("Show cue list")
        .clicked()
    {
        app.ui.show_markers = !app.ui.show_markers;
    }
    if tool_button(ui, Icon::Gear, false, theme::TEXT, vec2(32.0, 28.0)).on_hover_text("Settings (Ctrl+,)").clicked() {
        app.ui.prefs.open = true;
    }
    if tool_button(ui, Icon::Clock, app.ui.show_big_clock, theme::ORANGE, vec2(32.0, 28.0))
        .on_hover_text("Big timecode window (B)")
        .clicked()
    {
        app.ui.show_big_clock = !app.ui.show_big_clock;
    }

    // Master meter.
    let l = app.ui.meter(METER_MASTER_L, sh.master_peak[0].take());
    let r = app.ui.meter(METER_MASTER_R, sh.master_peak[1].take());
    let (m, _) = ui.allocate_exact_size(vec2(9.0, 34.0), Sense::hover());
    paint_vmeter(ui.painter(), m, &[l, r]);

    if ui.available_width() < 120.0 {
        return;
    }
    let mtc = &app.mtc.status;
    let connected = mtc.connected.load(Ordering::Relaxed);
    let mtc_on = app.project.mtc.enabled && connected;
    let mtc_detail = match (&app.project.mtc.port, app.project.mtc.enabled) {
        (_, false) => "Off".to_string(),
        (None, true) => "No port".to_string(),
        (Some(_), true) if connected => "Sending".to_string(),
        (Some(_), true) => "Port error".to_string(),
    };
    let hover = mtc.error.lock().unwrap().clone().unwrap_or_else(|| {
        format!("MIDI Timecode — {}\nClick to toggle", app.project.mtc.port.clone().unwrap_or_else(|| "no port".into()))
    });
    let detail = if ui.available_width() > 260.0 { mtc_detail.as_str() } else { "" };
    if pill(ui, "MTC", detail, mtc_on, theme::MTC).on_hover_text(hover).clicked() {
        app.project.mtc.enabled = !app.project.mtc.enabled;
        app.dirty = true;
        app.apply_project_to_engine();
    }

    if ui.available_width() < 80.0 {
        return;
    }
    let channels = app.output_channels() as i32;
    let ltc = &app.project.ltc;
    let routed = ltc.enabled && ltc.channel >= 0 && ltc.channel < channels;
    let detail = if !ltc.enabled {
        "Off".to_string()
    } else if routed {
        format!("Out {}", ltc.channel + 1)
    } else {
        "Not routed".to_string()
    };
    let detail = if ui.available_width() > 170.0 { detail.as_str() } else { "" };
    if pill(ui, "LTC", detail, routed, theme::LTC).on_hover_text("Linear timecode output — click to toggle").clicked()
    {
        app.project.ltc.enabled = !app.project.ltc.enabled;
        app.dirty = true;
        app.apply_project_to_engine();
    }
}
