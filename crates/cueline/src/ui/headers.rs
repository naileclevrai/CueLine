//! Left column of the arrange view: section header, LTC header and track
//! headers in the style of Logic Pro.

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, UiBuilder};

use super::widgets::{chip, fader, knob, paint_vmeter, switch, tabular, tool_button, Icon};
use super::{fonts, theme, METER_TRACK};
use crate::app::{CueLineApp, TrackState};

pub fn channel_label(ch: i32) -> String {
    if ch < 0 {
        "None".into()
    } else {
        format!("Out {}", ch + 1)
    }
}

pub fn channel_combo(ui: &mut Ui, id: &str, value: &mut i32, channels: u16, allow_none: bool) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).width(70.0).selected_text(channel_label(*value)).show_ui(ui, |ui| {
        if allow_none {
            changed |= ui.selectable_value(value, -1, "None").changed();
        }
        for c in 0..channels as i32 {
            changed |= ui.selectable_value(value, c, channel_label(c)).changed();
        }
    });
    changed
}

fn child(ui: &mut Ui, rect: Rect) -> Ui {
    ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)))
}

pub fn corner(app: &mut CueLineApp, ui: &mut Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, theme::BG_PANEL);
    p.text(
        pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Tracks",
        fonts::semibold(13.0),
        theme::TEXT,
    );
    let count_x =
        rect.left() + 14.0 + p.layout_no_wrap("Tracks".into(), fonts::semibold(13.0), theme::TEXT).size().x + 7.0;
    p.text(
        pos2(count_x, rect.center().y),
        Align2::LEFT_CENTER,
        app.tracks.len().to_string(),
        fonts::text(12.0),
        theme::TEXT_FAINT,
    );
    let mut c = child(ui, rect.shrink2(vec2(10.0, 0.0)));
    c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if tool_button(ui, Icon::Plus, false, theme::TEXT, vec2(28.0, 26.0))
            .on_hover_text("Import audio (Ctrl+I)")
            .clicked()
        {
            super::menus::import_dialog(app);
        }
    });
}

pub fn ltc_header(app: &mut CueLineApp, ui: &mut Ui, rect: Rect) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, theme::BG_HEADER);
    let on = app.project.ltc.enabled;
    // Orange "TC" badge.
    let badge = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(26.0, 20.0));
    p.rect_filled(badge, CornerRadius::same(5), if on { theme::LTC } else { theme::BG_WIDGET });
    p.text(
        badge.center(),
        Align2::CENTER_CENTER,
        "TC",
        fonts::semibold(10.5),
        if on { Color32::from_rgb(0x1a, 0x12, 0x02) } else { theme::TEXT_DIM },
    );
    p.text(
        pos2(badge.right() + 10.0, rect.center().y - 7.0),
        Align2::LEFT_CENTER,
        "Timecode",
        fonts::medium(13.0),
        theme::TEXT,
    );
    p.text(
        pos2(badge.right() + 10.0, rect.center().y + 8.0),
        Align2::LEFT_CENTER,
        format!("LTC · {} fps", app.project.frame_rate.label()),
        fonts::text(11.0),
        theme::TEXT_DIM,
    );

    let mut c = child(ui, rect.shrink2(vec2(12.0, 0.0)));
    let mut changed = false;
    c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let mut enabled = app.project.ltc.enabled;
        if switch(ui, &mut enabled).on_hover_text("LTC output").changed() {
            app.project.ltc.enabled = enabled;
            changed = true;
        }
        let channels = app.output_channels();
        changed |= channel_combo(ui, "ltc_out", &mut app.project.ltc.channel, channels, true);
    });
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
        painter.rect_filled(row, CornerRadius::ZERO, if selected { theme::BG_SELECTED } else { theme::BG_HEADER });
        painter.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, theme::HAIRLINE));
        let color = theme::rgb(app.tracks[i].def.color);
        let bar =
            Rect::from_min_max(pos2(row.left() + 7.0, row.top() + 9.0), pos2(row.left() + 10.0, row.bottom() - 9.0));
        painter.rect_filled(bar, CornerRadius::same(2), color);

        let bg = ui.interact(row.intersect(area), ui.id().with(("track_row", id)), Sense::click());
        if bg.clicked() {
            select = Some(id);
        }
        let hovered = ui.rect_contains_pointer(row.intersect(area));

        // Vertical meter on the right edge.
        let peak = app.tracks[i].params.peak.take();
        let shown = app.ui.meter(METER_TRACK + id, peak);
        let meter =
            Rect::from_min_max(pos2(row.right() - 14.0, row.top() + 9.0), pos2(row.right() - 8.0, row.bottom() - 9.0));
        paint_vmeter(&painter, meter, &[shown]);

        let content =
            Rect::from_min_max(pos2(row.left() + 18.0, row.top() + 7.0), pos2(meter.left() - 8.0, row.bottom() - 7.0));
        let mut c = ui.new_child(UiBuilder::new().max_rect(content));
        c.set_clip_rect(area.intersect(row));
        match header_contents(app, &mut c, i, hovered) {
            HeaderAction::Remove => remove = Some(id),
            HeaderAction::Select => select = Some(id),
            HeaderAction::None => {}
        }
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

fn header_contents(app: &mut CueLineApp, ui: &mut Ui, i: usize, hovered: bool) -> HeaderAction {
    let mut action = HeaderAction::None;
    let id = app.tracks[i].id;
    let mut changed = false;
    let mut checkpoint = false;
    let before = app.tracks[i].def.clone();
    ui.spacing_mut().item_spacing = vec2(6.0, 8.0);

    // Row 1: number, name, mute / solo.
    ui.horizontal(|ui| {
        ui.set_height(20.0);
        let n = RichText::new(format!("{}", i + 1)).font(fonts::semibold(11.0)).color(theme::TEXT_FAINT);
        ui.allocate_ui_with_layout(vec2(14.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_width(14.0);
            ui.label(n)
        });
        let editing = app.ui.rename_track.as_ref().is_some_and(|(rid, _)| *rid == id);
        let name_w = (ui.available_width() - 54.0).max(40.0);
        if editing {
            let (_, text) = app.ui.rename_track.as_mut().unwrap();
            let r = ui.add(egui::TextEdit::singleline(text).font(fonts::medium(13.0)).desired_width(name_w));
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
            let color = if matches!(t.state, TrackState::Failed(_)) { theme::RED } else { theme::TEXT };
            let label = egui::Label::new(RichText::new(&t.def.name).font(fonts::medium(13.0)).color(color))
                .truncate()
                .sense(Sense::click());
            let hover = match &t.state {
                TrackState::Failed(e) => format!("{}\n{e}", t.def.path.display()),
                _ => format!("{}\nDouble-click to rename", t.def.path.display()),
            };
            let r = ui
                .allocate_ui_with_layout(vec2(name_w, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_width(name_w);
                    ui.add(label)
                })
                .inner
                .on_hover_text(hover);
            if r.double_clicked() {
                app.ui.rename_track = Some((id, app.tracks[i].def.name.clone()));
            } else if r.clicked() {
                action = HeaderAction::Select;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let def = &mut app.tracks[i].def;
            if chip(ui, "S", def.solo, theme::SOLO, "Solo").clicked() {
                def.solo = !def.solo;
                checkpoint = true;
                changed = true;
            }
            if chip(ui, "M", def.mute, theme::MUTE, "Mute").clicked() {
                def.mute = !def.mute;
                checkpoint = true;
                changed = true;
            }
        });
    });

    // Row 2: fader, value, pan, remove.
    ui.horizontal(|ui| {
        ui.set_height(20.0);
        ui.add_space(20.0);
        let color = theme::rgb(app.tracks[i].def.color);
        let def = &mut app.tracks[i].def;
        let w = (ui.available_width() - 92.0).clamp(60.0, 160.0);
        let f = fader(ui, &mut def.gain_db, w, color);
        if f.drag_started() || f.double_clicked() {
            checkpoint = true;
        }
        changed |= f.changed();
        let (vr, _) = ui.allocate_exact_size(vec2(38.0, 20.0), Sense::hover());
        let text = if def.gain_db <= -59.9 { "−∞".to_string() } else { format!("{:+.1}", def.gain_db) };
        tabular(
            ui.painter(),
            pos2(vr.right(), vr.center().y),
            Align2::RIGHT_CENTER,
            &text,
            fonts::text(11.0),
            theme::TEXT_DIM,
        );
        let k = knob(ui, &mut def.pan, color);
        if k.drag_started() || k.double_clicked() {
            checkpoint = true;
        }
        changed |= k.changed();
        if hovered {
            let x = ui
                .add(egui::Button::new(RichText::new("×").font(fonts::text(15.0)).color(theme::TEXT_DIM)).frame(false));
            if x.on_hover_text("Remove track").clicked() {
                action = HeaderAction::Remove;
            }
        }
    });

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
