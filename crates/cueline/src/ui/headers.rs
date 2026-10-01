//! Left column of the arrange view: LTC and track headers.

use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder};

use super::theme;
use super::widgets::paint_hmeter;
use super::METER_TRACK;
use crate::app::{CueLineApp, TrackState};

/// A compact square toggle (mute / solo / on).
pub fn toggle_chip(ui: &mut Ui, text: &str, on: bool, color: Color32, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::click());
    let p = ui.painter();
    let fill = if on {
        color
    } else if resp.hovered() {
        theme::BG_WIDGET_HOVER
    } else {
        theme::BG_WIDGET
    };
    p.rect_filled(rect, CornerRadius::same(2), fill);
    p.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, if on { color } else { theme::BORDER }),
        StrokeKind::Inside,
    );
    let fg = if on { Color32::from_rgb(0x14, 0x14, 0x16) } else { theme::TEXT_DIM };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.0), fg);
    resp.on_hover_text(tip)
}

pub fn channel_label(ch: i32) -> String {
    if ch < 0 {
        "None".into()
    } else {
        format!("Out {}", ch + 1)
    }
}

pub fn channel_combo(ui: &mut Ui, id: &str, value: &mut i32, channels: u16, allow_none: bool) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(64.0).selected_text(channel_label(*value)).show_ui(ui, |ui| {
        if allow_none {
            changed |= ui.selectable_value(value, -1, "None").changed();
        }
        for c in 0..channels as i32 {
            changed |= ui.selectable_value(value, c, channel_label(c)).changed();
        }
    });
    changed
}

pub fn corner(app: &mut CueLineApp, ui: &mut Ui, rect: Rect) {
    ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::BG_HEADER);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.label(RichText::new("TRACKS").small().strong().color(theme::TEXT_DIM));
    child.label(RichText::new(format!("{}", app.tracks.len())).small().color(theme::TEXT_FAINT));
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.small_button("+ Import").on_hover_text("Import audio files (Ctrl+I)").clicked() {
            super::menus::import_dialog(app);
        }
    });
}

pub fn ltc_header(app: &mut CueLineApp, ui: &mut Ui, rect: Rect) {
    ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::BG_HEADER);
    ui.painter().rect_filled(
        Rect::from_min_size(rect.left_top(), vec2(4.0, rect.height())),
        CornerRadius::ZERO,
        theme::LTC,
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 4.0)).translate(vec2(2.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 5.0;
    let mut changed = false;
    let on = app.project.ltc.enabled;
    if toggle_chip(ui, "ON", on, theme::LTC, "Enable LTC output").clicked() {
        app.project.ltc.enabled = !on;
        changed = true;
    }
    ui.label(RichText::new("LTC").strong().color(if on { theme::LTC } else { theme::TEXT_DIM }));
    let channels = app.output_channels();
    changed |= channel_combo(ui, "ltc_out", &mut app.project.ltc.channel, channels, true);
    changed |= ui
        .add(
            egui::DragValue::new(&mut app.project.ltc.level_db)
                .range(-40.0..=0.0)
                .speed(0.2)
                .suffix(" dB")
                .fixed_decimals(1),
        )
        .on_hover_text("LTC level (dBFS peak)")
        .changed();
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}

pub fn track_headers(app: &mut CueLineApp, ui: &mut Ui, area: Rect) {
    ui.painter().rect_filled(area, CornerRadius::ZERO, theme::BG_PANEL);
    let h = app.view.track_height;
    let mut remove = None;
    let mut select = None;
    let ids: Vec<u64> = app.tracks.iter().map(|t| t.id).collect();
    for (i, id) in ids.into_iter().enumerate() {
        let top = area.top() + i as f32 * h - app.view.scroll_y;
        let row = Rect::from_min_size(pos2(area.left(), top), vec2(area.width(), h));
        if !row.intersects(area) {
            continue;
        }
        let selected = app.view.selected_track == Some(id);
        let painter = ui.painter_at(area);
        painter.rect_filled(row, CornerRadius::ZERO, if selected { theme::BG_WIDGET } else { theme::BG_HEADER });
        painter.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, theme::BG_DEEP));

        let bg = ui.interact(row.intersect(area), ui.id().with(("track_row", id)), Sense::click());
        if bg.clicked() {
            select = Some(id);
        }

        let mut child = ui.new_child(UiBuilder::new().max_rect(row.shrink2(vec2(10.0, 6.0)).translate(vec2(2.0, 0.0))));
        child.set_clip_rect(area);
        let peak = app.tracks[i].params.peak.take();
        let shown = app.ui.meter(METER_TRACK + id, peak);
        let action = header_contents(app, &mut child, i, shown);
        match action {
            HeaderAction::Remove => remove = Some(id),
            HeaderAction::Select => select = Some(id),
            HeaderAction::None => {}
        }
        let t = &app.tracks[i];
        painter.rect_filled(
            Rect::from_min_size(row.left_top(), vec2(4.0, h - 1.0)),
            CornerRadius::ZERO,
            theme::rgb(t.def.color),
        );
    }
    if let Some(id) = select {
        app.view.selected_track = Some(id);
    }
    if let Some(id) = remove {
        app.remove_track(id);
    }
}

enum HeaderAction {
    None,
    Select,
    Remove,
}

fn header_contents(app: &mut CueLineApp, ui: &mut Ui, i: usize, meter: f32) -> HeaderAction {
    let mut action = HeaderAction::None;
    let id = app.tracks[i].id;
    let mut changed = false;
    let mut checkpoint = false;
    let before = app.tracks[i].def.clone();
    ui.spacing_mut().item_spacing = vec2(4.0, 5.0);

    ui.horizontal(|ui| {
        let editing = app.ui.rename_track.as_ref().is_some_and(|(rid, _)| *rid == id);
        if editing {
            let (_, text) = app.ui.rename_track.as_mut().unwrap();
            let r = ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
            r.request_focus();
            if r.lost_focus() {
                let (_, name) = app.ui.rename_track.take().unwrap();
                if !ui.input(|i| i.key_pressed(egui::Key::Escape)) && !name.trim().is_empty() {
                    app.tracks[i].def.name = name.trim().to_string();
                    checkpoint = true;
                    changed = true;
                }
            }
        } else {
            let t = &app.tracks[i];
            let mut name = RichText::new(&t.def.name).color(theme::TEXT);
            if let TrackState::Failed(_) = t.state {
                name = name.color(theme::ERROR);
            }
            let r = ui.add(egui::Label::new(name).truncate().sense(Sense::click())).on_hover_text(match &t.state {
                TrackState::Failed(e) => format!("{}\n{e}", t.def.path.display()),
                _ => format!("{}\nDouble-click to rename", t.def.path.display()),
            });
            if r.double_clicked() {
                app.ui.rename_track = Some((id, app.tracks[i].def.name.clone()));
            } else if r.clicked() {
                action = HeaderAction::Select;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(RichText::new("×").size(14.0).color(theme::TEXT_DIM)).frame(false))
                .on_hover_text("Remove track")
                .clicked()
            {
                action = HeaderAction::Remove;
            }
            let def = &mut app.tracks[i].def;
            if toggle_chip(ui, "S", def.solo, theme::SOLO, "Solo").clicked() {
                checkpoint = true;
                def.solo = !def.solo;
                changed = true;
            }
            if toggle_chip(ui, "M", def.mute, theme::MUTE, "Mute").clicked() {
                checkpoint = true;
                def.mute = !def.mute;
                changed = true;
            }
        });
    });

    ui.horizontal(|ui| {
        let def = &mut app.tracks[i].def;
        let vol = ui
            .add(
                egui::DragValue::new(&mut def.gain_db)
                    .range(-60.0..=12.0)
                    .speed(0.1)
                    .fixed_decimals(1)
                    .prefix("Vol ")
                    .suffix(" dB"),
            )
            .on_hover_text("Volume (double-click to type, Ctrl+click: 0 dB)");
        if vol.drag_started() {
            checkpoint = true;
        }
        if vol.clicked() && ui.input(|i| i.modifiers.command) {
            def.gain_db = 0.0;
            changed = true;
        }
        changed |= vol.changed();
        let mut pan = def.pan * 100.0;
        let pan_resp = ui
            .add(egui::DragValue::new(&mut pan).range(-100.0..=100.0).speed(0.5).fixed_decimals(0).custom_formatter(
                |v, _| match v.round() as i32 {
                    0 => "C".into(),
                    v if v < 0 => format!("{}L", -v),
                    v => format!("{v}R"),
                },
            ))
            .on_hover_text("Pan");
        if pan_resp.drag_started() {
            checkpoint = true;
        }
        if pan_resp.changed() {
            def.pan = pan / 100.0;
            changed = true;
        }
    });

    let rect = ui.available_rect_before_wrap();
    let meter_rect = Rect::from_min_size(pos2(rect.left(), rect.top() + 2.0), vec2(rect.width(), 5.0));
    if meter_rect.bottom() < ui.clip_rect().bottom() + 6.0 {
        paint_hmeter(ui.painter(), meter_rect, &[meter]);
    }

    if checkpoint {
        // Record the state as it was before this frame's edit.
        let after = std::mem::replace(&mut app.tracks[i].def, before);
        app.checkpoint();
        app.tracks[i].def = after;
    }
    if changed {
        app.track_changed(id);
    }
    action
}
