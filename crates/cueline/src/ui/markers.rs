//! Right-hand cue list.

use eframe::egui::{self, vec2, CornerRadius, RichText, Sense, Stroke, Ui};

use super::theme;
use crate::app::CueLineApp;

const MARKER_COLORS: [[u8; 3]; 6] = [
    [0xe8, 0xa2, 0x3a],
    [0xe0, 0x5a, 0x4a],
    [0x3f, 0xcf, 0x6b],
    [0x4a, 0x9e, 0xf0],
    [0xa8, 0x8c, 0xff],
    [0xd8, 0xda, 0xde],
];

pub fn panel(app: &mut CueLineApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("MARKERS").small().strong().color(theme::TEXT_DIM));
        ui.label(RichText::new(app.project.markers.len().to_string()).small().color(theme::TEXT_FAINT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+ Add").on_hover_text("Add marker at playhead (M)").clicked() {
                app.add_marker_at(app.position_secs());
            }
        });
    });
    ui.add_space(2.0);
    ui.separator();

    if app.project.markers.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new("No markers yet.\nPress M while playing to drop cues on the fly.").color(theme::TEXT_FAINT));
        return;
    }

    let playhead = app.position_secs();
    let current = app.project.markers.iter().rposition(|m| m.time_secs <= playhead + 1e-6);
    let rate = app.project.frame_rate;
    let mut goto = None;
    let mut remove = None;
    let mut commit_rename = None;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let n = app.project.markers.len();
        for i in 0..n {
            let m = app.project.markers[i].clone();
            let next = app.project.markers.get(i + 1).map(|n| n.time_secs);
            let selected = app.view.selected_marker == Some(i);
            let live = current == Some(i) && app.is_playing();
            let fill = if live {
                theme::rgb(m.color).gamma_multiply(0.18)
            } else if selected {
                theme::BG_WIDGET
            } else {
                theme::BG_PANEL
            };
            let frame = egui::Frame::new().fill(fill).corner_radius(3).inner_margin(egui::Margin::symmetric(6, 4));
            let resp = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, sw) = ui.allocate_exact_size(vec2(10.0, 18.0), Sense::click());
                        ui.painter().rect_filled(r.shrink2(vec2(1.0, 2.0)), CornerRadius::same(2), theme::rgb(m.color));
                        sw.on_hover_text("Right-click to change colour").context_menu(|ui| {
                            for c in MARKER_COLORS {
                                let (r, resp) = ui.allocate_exact_size(vec2(60.0, 16.0), Sense::click());
                                ui.painter().rect_filled(r, 2.0, theme::rgb(c));
                                if resp.clicked() {
                                    app.checkpoint();
                                    app.project.markers[i].color = c;
                                    app.dirty = true;
                                    ui.close();
                                }
                            }
                        });
                        ui.label(RichText::new(format!("{:>2}", i + 1)).font(theme::mono(11.0)).color(theme::TEXT_FAINT));
                        let tc = app.timecode_at(m.time_secs + 1e-6).display(rate).to_string();
                        if ui
                            .add(egui::Label::new(RichText::new(tc).font(theme::mono(12.0)).color(theme::LTC)).sense(Sense::click()))
                            .on_hover_text("Go to marker")
                            .clicked()
                        {
                            goto = Some(i);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("×").size(14.0).color(theme::TEXT_DIM)).frame(false)).on_hover_text("Delete marker").clicked() {
                                remove = Some(i);
                            }
                            if let Some(n) = next {
                                let d = n - m.time_secs;
                                ui.label(RichText::new(format!("{:.1}s", d)).small().color(theme::TEXT_FAINT)).on_hover_text("Time until next marker");
                            }
                        });
                    });
                    let renaming = app.ui.rename_marker.as_ref().is_some_and(|(idx, _)| *idx == i);
                    if renaming {
                        let (_, text) = app.ui.rename_marker.as_mut().unwrap();
                        let r = ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
                        r.request_focus();
                        if r.lost_focus() {
                            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
                            commit_rename = Some(!cancelled);
                        }
                    } else {
                        let name = if m.name.is_empty() { "(unnamed)" } else { m.name.as_str() };
                        let r = ui.add(egui::Label::new(RichText::new(name).color(theme::TEXT)).truncate().sense(Sense::click()));
                        if r.double_clicked() {
                            app.ui.rename_marker = Some((i, m.name.clone()));
                        }
                    }
                })
                .response;
            let row = ui.interact(resp.rect, ui.id().with(("marker_row", i)), Sense::click());
            if row.clicked() {
                app.view.selected_marker = Some(i);
            }
            if row.double_clicked() {
                goto = Some(i);
            }
            if selected {
                ui.painter().rect_stroke(resp.rect, 3.0, Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.6)), egui::StrokeKind::Inside);
            }
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
    if let Some(i) = goto {
        app.goto_marker(i);
    }
    if let Some(i) = remove {
        app.remove_marker(i);
    }
}
