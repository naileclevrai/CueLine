//! "New Show" assistant: name and location, timecode, output layout and
//! MIDI Timecode in one sheet, with sensible defaults for the current device.

use std::path::PathBuf;

use cueline_core::{FrameRate, Timecode};
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

use super::sheet::{self, footnote, group, row, row_separator, section};
use super::{fonts, theme};
use crate::app::{default_show_folder, sanitize_name, CueLineApp, NewShow};
use crate::engine::mtc_out::list_ports;
use crate::project::{OutputLayout, EXTENSION};

pub struct NewShowUi {
    pub open: bool,
    name: String,
    folder: PathBuf,
    rate: FrameRate,
    start: String,
    layout: OutputLayout,
    mtc_port: Option<String>,
    ports: Vec<String>,
    error: Option<String>,
}

impl Default for NewShowUi {
    fn default() -> Self {
        Self {
            open: false,
            name: String::new(),
            folder: default_show_folder(),
            rate: FrameRate::Fps25,
            start: "01:00:00:00".into(),
            layout: OutputLayout::Stereo,
            mtc_port: None,
            ports: Vec::new(),
            error: None,
        }
    }
}

impl NewShowUi {
    /// Opens the assistant with defaults suited to `outputs` device channels.
    pub fn begin(&mut self, outputs: u16) {
        let folder = self.folder.clone();
        *self = Self { open: true, folder, ..Self::default() };
        self.name = next_free_name(&self.folder);
        self.layout = OutputLayout::default_for(outputs);
        self.ports = list_ports();
    }
}

fn next_free_name(folder: &std::path::Path) -> String {
    (1..)
        .map(|n| if n == 1 { "Untitled Show".to_string() } else { format!("Untitled Show {n}") })
        .find(|n| !folder.join(n).exists())
        .unwrap()
}

pub fn window(app: &mut CueLineApp, ctx: &egui::Context) {
    if !app.ui.new_show.open {
        return;
    }
    let mut open = true;
    let mut create = false;
    sheet::show(ctx, "New Show", &mut open, 560.0, |ui| {
        create = contents(app, ui);
    });
    if !open {
        app.ui.new_show.open = false;
    }
    if create {
        let s = &app.ui.new_show;
        let show = NewShow {
            name: s.name.clone(),
            folder: s.folder.clone(),
            rate: s.rate,
            start: Timecode::parse(&s.start, s.rate).unwrap_or(Timecode::new(1, 0, 0, 0)),
            layout: s.layout,
            mtc_port: s.mtc_port.clone(),
        };
        match app.create_show(&show) {
            Ok(path) => {
                app.ui.new_show.open = false;
                app.ui.toast_info(format!("Created {}", path.display()));
            }
            Err(e) => app.ui.new_show.error = Some(e),
        }
    }
}

/// Returns true when the user asked to create the show.
fn contents(app: &mut CueLineApp, ui: &mut Ui) -> bool {
    let outputs = app.output_channels();
    let device = app.engine.as_ref().map_or("No audio device".to_string(), |e| e.device_name.clone());
    let mut create = false;

    sheet::scroll_body(ui, 70.0, |ui| {
        section(ui, "Show");
        group(ui, |ui| {
            let s = &mut app.ui.new_show;
            row(ui, "Name", |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut s.name).font(fonts::text(13.0)).desired_width(260.0));
                if r.changed() {
                    s.error = None;
                }
            });
            row_separator(ui);
            row(ui, "Location", |ui| {
                if sheet::button(ui, "Choose…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_title("Where to create the show")
                        .set_directory(&s.folder)
                        .pick_folder()
                    {
                        s.folder = dir;
                        s.error = None;
                    }
                }
                let shown = s.folder.display().to_string();
                let r = ui.add(
                    egui::Label::new(RichText::new(&shown).font(fonts::text(12.0)).color(theme::TEXT_DIM)).truncate(),
                );
                r.on_hover_text(&shown);
            });
        });
        let file = format!("{}.{EXTENSION}", sanitize_name(&app.ui.new_show.name));
        footnote(
            ui,
            &format!("Creates a folder with {file} inside. Keep your audio next to it so the show can be moved."),
        );

        section(ui, "Timecode");
        group(ui, |ui| {
            let s = &mut app.ui.new_show;
            row(ui, "Frame rate", |ui| {
                egui::ComboBox::from_id_salt("ns_fps")
                    .width(140.0)
                    .selected_text(format!("{} fps", s.rate.label()))
                    .show_ui(ui, |ui| {
                        for r in FrameRate::ALL {
                            ui.selectable_value(&mut s.rate, r, format!("{} fps", r.label()));
                        }
                    });
            });
            row_separator(ui);
            row(ui, "Start timecode", |ui| {
                let valid = Timecode::parse(&s.start, s.rate).is_some();
                let color = if valid { theme::TEXT } else { theme::RED };
                ui.add(
                    egui::TextEdit::singleline(&mut s.start)
                        .font(fonts::mono(13.0))
                        .text_color(color)
                        .desired_width(130.0),
                );
            });
        });

        section(ui, "Audio Outputs");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let w = (ui.available_width() - 20.0) / 3.0;
            for layout in OutputLayout::ALL {
                let available = outputs >= layout.outputs_needed();
                let selected = app.ui.new_show.layout == layout;
                if layout_card(ui, layout, selected, available, w).clicked() && available {
                    app.ui.new_show.layout = layout;
                }
            }
        });
        footnote(
            ui,
            &format!(
                "{} — {} Output device: {device} ({outputs} outputs).",
                app.ui.new_show.layout.title(),
                app.ui.new_show.layout.description()
            ),
        );

        section(ui, "MIDI Timecode");
        group(ui, |ui| {
            let s = &mut app.ui.new_show;
            row(ui, "Send MTC to", |ui| {
                let label = s.mtc_port.clone().unwrap_or_else(|| "Don't send MTC".into());
                egui::ComboBox::from_id_salt("ns_mtc").width(240.0).selected_text(label).show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.mtc_port, None, "Don't send MTC");
                    for p in s.ports.clone() {
                        ui.selectable_value(&mut s.mtc_port, Some(p.clone()), p);
                    }
                });
                if sheet::button(ui, "Refresh").clicked() {
                    s.ports = list_ports();
                }
            });
        });
        if app.ui.new_show.ports.is_empty() {
            no_midi_hint(ui);
        }
    });

    if let Some(err) = &app.ui.new_show.error {
        ui.add_space(6.0);
        ui.label(RichText::new(err).font(fonts::text(12.0)).color(theme::RED));
    }
    ui.add_space(14.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let ok = Timecode::parse(&app.ui.new_show.start, app.ui.new_show.rate).is_some();
        if sheet::primary_button(ui, "Create Show", ok).clicked()
            || (ok && ui.input(|i| i.key_pressed(egui::Key::Enter)))
        {
            create = true;
        }
        if sheet::button(ui, "Cancel").clicked() {
            app.ui.new_show.open = false;
        }
    });
    create
}

/// Explains why no MIDI port is listed and how to get one.
pub fn no_midi_hint(ui: &mut Ui) {
    ui.add_space(4.0);
    egui::Frame::new()
        .fill(theme::YELLOW.gamma_multiply(0.10))
        .stroke(Stroke::new(0.5, theme::YELLOW.gamma_multiply(0.45)))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("No MIDI output found").font(fonts::semibold(12.0)).color(theme::YELLOW));
            ui.label(
                RichText::new(
                    "Connect a MIDI interface for hardware receivers. To send MTC to software on this computer \
                     (lighting or video app), install a virtual MIDI port such as loopMIDI, create a port, then click Refresh.",
                )
                .font(fonts::text(11.5))
                .color(theme::TEXT_DIM),
            );
            ui.hyperlink_to(
                RichText::new("Get loopMIDI (free)").font(fonts::text(11.5)),
                "https://www.tobias-erichsen.de/software/loopmidi.html",
            );
        });
}

/// A selectable card with a small diagram of the outputs.
fn layout_card(ui: &mut Ui, layout: OutputLayout, selected: bool, available: bool, width: f32) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(width, 96.0), if available { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let fill = if selected {
        theme::BLUE.gamma_multiply(0.18)
    } else if resp.hovered() && available {
        Color32::from_rgb(0x36, 0x36, 0x3a)
    } else {
        Color32::from_rgb(0x30, 0x30, 0x33)
    };
    p.rect_filled(rect, CornerRadius::same(10), fill);
    let stroke = if selected { Stroke::new(2.0, theme::BLUE) } else { Stroke::new(0.5, Color32::from_white_alpha(18)) };
    p.rect_stroke(rect, CornerRadius::same(10), stroke, StrokeKind::Inside);

    // Output boxes: blue = music, orange = LTC, grey = unused.
    let slots: [(Option<&str>, Color32); 3] = match layout {
        OutputLayout::Stereo => [(Some("L"), theme::BLUE), (Some("R"), theme::BLUE), (None, theme::BG_WIDGET)],
        OutputLayout::MonoPlusLtc => [(Some("M"), theme::BLUE), (Some("TC"), theme::ORANGE), (None, theme::BG_WIDGET)],
        OutputLayout::StereoPlusLtc => {
            [(Some("L"), theme::BLUE), (Some("R"), theme::BLUE), (Some("TC"), theme::ORANGE)]
        }
    };
    let bw = 34.0;
    let total = bw * 3.0 + 12.0;
    let x0 = rect.center().x - total / 2.0;
    for (i, (label, color)) in slots.iter().enumerate() {
        let b = Rect::from_min_size(pos2(x0 + i as f32 * (bw + 6.0), rect.top() + 14.0), vec2(bw, 26.0));
        let c = if available { *color } else { color.gamma_multiply(0.4) };
        p.rect_filled(b, CornerRadius::same(5), if label.is_some() { c } else { Color32::from_rgb(0x26, 0x26, 0x29) });
        let text_color = if label.is_some() { Color32::from_rgb(0x12, 0x12, 0x14) } else { theme::TEXT_FAINT };
        p.text(b.center(), Align2::CENTER_CENTER, label.unwrap_or("—"), fonts::semibold(10.5), text_color);
        p.text(
            pos2(b.center().x, b.bottom() + 8.0),
            Align2::CENTER_CENTER,
            format!("{}", i + 1),
            fonts::text(9.5),
            theme::TEXT_FAINT,
        );
    }
    let title_color = if available { theme::TEXT } else { theme::TEXT_FAINT };
    p.text(
        pos2(rect.center().x, rect.bottom() - 26.0),
        Align2::CENTER_CENTER,
        layout.title(),
        fonts::medium(12.5),
        title_color,
    );
    let sub = if available {
        format!("{} outputs", layout.outputs_needed())
    } else {
        format!("Needs {} outputs", layout.outputs_needed())
    };
    p.text(
        pos2(rect.center().x, rect.bottom() - 11.0),
        Align2::CENTER_CENTER,
        sub,
        fonts::text(10.5),
        theme::TEXT_FAINT,
    );
    resp.on_hover_text(layout.description())
}
