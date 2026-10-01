//! Right-hand cue list, styled like a macOS source list.

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui};

use super::widgets::{tabular, tool_button, Icon};
use super::{fonts, theme};
use crate::app::CueLineApp;

const MARKER_COLORS: [[u8; 3]; 7] = [
    [0xff, 0x9f, 0x0a],
    [0xff, 0x45, 0x3a],
    [0x30, 0xd1, 0x58],
    [0x0a, 0x84, 0xff],
    [0xbf, 0x5a, 0xf2],
    [0xff, 0xd6, 0x0a],
    [0xd8, 0xd8, 0xdc],
];
const ROW_H: f32 = 48.0;

pub fn panel(app: &mut CueLineApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.set_height(26.0);
        ui.label(RichText::new("Cues").font(fonts::semibold(13.0)).color(theme::TEXT));
        ui.label(RichText::new(app.project.markers.len().to_string()).font(fonts::text(12.0)).color(theme::TEXT_FAINT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tool_button(ui, Icon::Plus, false, theme::TEXT, vec2(28.0, 26.0))
                .on_hover_text("Add cue at playhead (M)")
                .clicked()
            {
                app.add_marker_at(app.position_secs());
            }
        });
    });
    ui.add_space(6.0);

    if app.project.markers.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No Cues").font(fonts::semibold(15.0)).color(theme::TEXT_DIM));
            ui.add_space(4.0);
            ui.label(
                RichText::new("Press M during playback to drop\na cue at the playhead.")
                    .font(fonts::text(12.0))
                    .color(theme::TEXT_FAINT),
            );
        });
        return;
    }

    let playhead = app.position_secs();
    let playing = app.is_playing();
    let current = app.project.markers.iter().rposition(|m| m.time_secs <= playhead + 1e-6);
    let rate = app.project.frame_rate;
    let mut goto = None;
    let mut remove = None;
    let mut commit_rename = None;
    let mut recolor = None;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for i in 0..app.project.markers.len() {
            let m = app.project.markers[i].clone();
            let next = app.project.markers.get(i + 1).map(|n| n.time_secs);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
            let selected = app.view.selected_marker == Some(i);
            let live = current == Some(i) && playing;
            let color = theme::rgb(m.color);
            let p = ui.painter();
            let fill = if selected {
                theme::BLUE.gamma_multiply(0.32)
            } else if live {
                color.gamma_multiply(0.14)
            } else if resp.hovered() {
                Color32::from_white_alpha(9)
            } else {
                Color32::TRANSPARENT
            };
            p.rect_filled(rect, CornerRadius::same(8), fill);
            if live {
                let bar = Rect::from_min_max(
                    pos2(rect.left() + 2.0, rect.top() + 10.0),
                    pos2(rect.left() + 5.0, rect.bottom() - 10.0),
                );
                p.rect_filled(bar, CornerRadius::same(2), color);
            }

            // Numbered colour disc.
            let disc = pos2(rect.left() + 22.0, rect.center().y);
            p.circle_filled(disc, 11.0, color);
            p.text(
                disc,
                Align2::CENTER_CENTER,
                format!("{}", i + 1),
                fonts::semibold(11.0),
                Color32::from_rgb(0x14, 0x14, 0x16),
            );

            let text_x = rect.left() + 42.0;
            let renaming = app.ui.rename_marker.as_ref().is_some_and(|(idx, _)| *idx == i);
            if renaming {
                let r = Rect::from_min_max(
                    pos2(text_x - 2.0, rect.top() + 5.0),
                    pos2(rect.right() - 8.0, rect.top() + 25.0),
                );
                let (_, text) = app.ui.rename_marker.as_mut().unwrap();
                let edit = ui.put(r, egui::TextEdit::singleline(text).font(fonts::medium(13.0)));
                edit.request_focus();
                if edit.lost_focus() {
                    commit_rename = Some(!ui.input(|i| i.key_pressed(egui::Key::Escape)));
                }
            } else {
                let name = if m.name.is_empty() { format!("Cue {}", i + 1) } else { m.name.clone() };
                let clip = Rect::from_min_max(pos2(text_x, rect.top()), pos2(rect.right() - 30.0, rect.bottom()));
                p.with_clip_rect(clip).text(
                    pos2(text_x, rect.top() + 16.0),
                    Align2::LEFT_CENTER,
                    name,
                    fonts::medium(13.0),
                    theme::TEXT,
                );
            }
            let p = ui.painter();
            let tc = app.timecode_at(m.time_secs + 1e-6).display(rate).to_string();
            let tc_rect = tabular(
                p,
                pos2(text_x, rect.top() + 33.0),
                Align2::LEFT_CENTER,
                &tc,
                fonts::text(11.5),
                theme::TEXT_DIM,
            );
            if let Some(n) = next {
                p.text(
                    pos2(tc_rect.right() + 8.0, tc_rect.center().y),
                    Align2::LEFT_CENTER,
                    format!("· {:.1} s", n - m.time_secs),
                    fonts::text(11.5),
                    theme::TEXT_FAINT,
                );
            }

            if resp.hovered() && !renaming {
                let x = Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(20.0, 20.0));
                let xr = ui.interact(x, ui.id().with(("del_cue", i)), Sense::click());
                let p = ui.painter();
                if xr.hovered() {
                    p.circle_filled(x.center(), 9.0, Color32::from_white_alpha(18));
                }
                p.text(x.center(), Align2::CENTER_CENTER, "×", fonts::text(15.0), theme::TEXT_DIM);
                if xr.on_hover_text("Delete cue").clicked() {
                    remove = Some(i);
                }
            }

            if resp.clicked() {
                app.view.selected_marker = Some(i);
            }
            if resp.double_clicked() {
                goto = Some(i);
            }
            resp.on_hover_text("Double-click to go to cue · right-click for options").context_menu(|ui| {
                if ui.button("Go to cue").clicked() {
                    goto = Some(i);
                    ui.close();
                }
                if ui.button("Rename").clicked() {
                    app.ui.rename_marker = Some((i, m.name.clone()));
                    ui.close();
                }
                ui.separator();
                ui.horizontal(|ui| {
                    for c in MARKER_COLORS {
                        let (r, cr) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                        ui.painter().circle_filled(r.center(), 8.0, theme::rgb(c));
                        if c == m.color {
                            ui.painter().circle_stroke(r.center(), 8.0, Stroke::new(1.5, Color32::WHITE));
                        }
                        if cr.clicked() {
                            recolor = Some((i, c));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Delete").clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
        }
    });

    if let Some(apply) = commit_rename {
        if let Some((idx, name)) = app.ui.rename_marker.take() {
            if apply && app.project.markers.get(idx).is_some_and(|m| m.name != name) {
                app.checkpoint();
                app.project.markers[idx].name = name;
                app.dirty = true;
            }
        }
    }
    if let Some((i, c)) = recolor {
        app.checkpoint();
        app.project.markers[i].color = c;
        app.dirty = true;
    }
    if let Some(i) = goto {
        app.goto_marker(i);
    }
    if let Some(i) = remove {
        app.remove_marker(i);
    }
}
