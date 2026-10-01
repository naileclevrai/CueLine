//! Settings sheet: audio device, routing, timecode and MIDI Timecode.

use cueline_core::{FrameRate, Timecode};
use eframe::egui::{self, RichText, Ui};

use super::headers::channel_combo;
use super::sheet::{self, footnote, group, row, row_separator, section};
use super::widgets::{segmented, switch};
use super::{fonts, theme};
use crate::app::CueLineApp;
use crate::engine::device::{host_names, list_output_devices, AudioConfig, DeviceInfo};
use crate::engine::mtc_out::list_ports;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Audio,
    Timecode,
    Midi,
}

#[derive(Default)]
pub struct PrefsUi {
    pub open: bool,
    tab: Tab,
    loaded: bool,
    hosts: Vec<String>,
    devices: Vec<DeviceInfo>,
    midi_ports: Vec<String>,
    draft: AudioConfig,
    start_tc: String,
    user_bits: String,
}

const BUFFER_SIZES: [u32; 7] = [64, 128, 256, 480, 512, 1024, 2048];
const WIDTH: f32 = 520.0;

impl PrefsUi {
    fn refresh(&mut self, app_cfg: &AudioConfig) {
        self.hosts = host_names();
        self.devices = list_output_devices(app_cfg.host.as_deref());
        self.midi_ports = list_ports();
        self.draft = app_cfg.clone();
        self.loaded = true;
    }
}

pub fn window(app: &mut CueLineApp, ctx: &egui::Context) {
    if !app.ui.prefs.open {
        app.ui.prefs.loaded = false;
        return;
    }
    if !app.ui.prefs.loaded {
        let cfg = app.settings.audio.clone();
        app.ui.prefs.refresh(&cfg);
        app.ui.prefs.start_tc = app.project.start_timecode.to_string();
        app.ui.prefs.user_bits = format!("{:08X}", app.project.user_bits);
    }
    let mut open = true;
    sheet::show(ctx, "Settings", &mut open, WIDTH, |ui| {
        ui.vertical_centered(|ui| {
            let mut tab = app.ui.prefs.tab;
            segmented(
                ui,
                &mut tab,
                &[(Tab::Audio, "Audio"), (Tab::Timecode, "Timecode"), (Tab::Midi, "MIDI Timecode")],
            );
            app.ui.prefs.tab = tab;
        });
        ui.add_space(8.0);
        match app.ui.prefs.tab {
            Tab::Audio => audio_tab(app, ui),
            Tab::Timecode => timecode_tab(app, ui),
            Tab::Midi => midi_tab(app, ui),
        }
    });
    if !open {
        app.ui.prefs.open = false;
    }
}

fn popup(ui: &mut Ui, id: &str, text: String, add: impl FnOnce(&mut Ui)) {
    egui::ComboBox::from_id_salt(id).width(250.0).selected_text(text).show_ui(ui, add);
}

fn audio_tab(app: &mut CueLineApp, ui: &mut Ui) {
    let mut changed_host = false;
    section(ui, "Output Device");
    group(ui, |ui| {
        let p = &mut app.ui.prefs;
        row(ui, "Driver", |ui| {
            let label = p.draft.host.clone().unwrap_or_else(|| "System default".into());
            popup(ui, "host", label, |ui| {
                changed_host |= ui.selectable_value(&mut p.draft.host, None, "System default").changed();
                for h in p.hosts.clone() {
                    changed_host |= ui.selectable_value(&mut p.draft.host, Some(h.clone()), h).changed();
                }
            });
        });
        row_separator(ui);
        row(ui, "Device", |ui| {
            let label = p.draft.device.clone().unwrap_or_else(|| "Default output".into());
            popup(ui, "device", label, |ui| {
                ui.selectable_value(&mut p.draft.device, None, "Default output");
                for d in &p.devices {
                    ui.selectable_value(
                        &mut p.draft.device,
                        Some(d.name.clone()),
                        format!("{}  —  {} outputs", d.name, d.channels),
                    );
                }
            });
        });
        let info = p.devices.iter().find(|d| Some(&d.name) == p.draft.device.as_ref()).cloned();
        row_separator(ui);
        row(ui, "Sample rate", |ui| {
            let label =
                p.draft.sample_rate.map_or("Device default".into(), |r| format!("{:.1} kHz", r as f32 / 1000.0));
            popup(ui, "rate", label, |ui| {
                ui.selectable_value(&mut p.draft.sample_rate, None, "Device default");
                let rates = info.as_ref().map_or(vec![44_100, 48_000, 96_000], |d| d.sample_rates.clone());
                for r in rates {
                    ui.selectable_value(&mut p.draft.sample_rate, Some(r), format!("{:.1} kHz", r as f32 / 1000.0));
                }
            });
        });
        row_separator(ui);
        row(ui, "Buffer size", |ui| {
            let label = p.draft.buffer_frames.map_or("Driver default".into(), |b| format!("{b} samples"));
            popup(ui, "buffer", label, |ui| {
                ui.selectable_value(&mut p.draft.buffer_frames, None, "Driver default");
                for b in BUFFER_SIZES {
                    if info.as_ref().and_then(|d| d.buffer_range).is_none_or(|(lo, hi)| (lo..=hi).contains(&b)) {
                        ui.selectable_value(&mut p.draft.buffer_frames, Some(b), format!("{b} samples"));
                    }
                }
            });
        });
    });
    if changed_host {
        let host = app.ui.prefs.draft.host.clone();
        app.ui.prefs.devices = list_output_devices(host.as_deref());
        app.ui.prefs.draft.device = None;
    }

    match (&app.engine, &app.engine_error) {
        (Some(e), _) => footnote(
            ui,
            &format!(
                "Running · {} · {} · {:.1} kHz · {} outputs",
                e.host_name,
                e.device_name,
                e.sample_rate as f32 / 1000.0,
                e.channels
            ),
        ),
        (None, Some(err)) => {
            ui.label(RichText::new(err).font(fonts::text(11.5)).color(theme::RED));
        }
        _ => {}
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if sheet::button(ui, "Refresh").clicked() {
            let cfg = app.ui.prefs.draft.clone();
            app.ui.prefs.refresh(&cfg);
        }
        if sheet::button(ui, "Restart Audio").on_hover_text("Re-open the device, e.g. after it was unplugged").clicked()
        {
            app.restart_audio();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let dirty = app.ui.prefs.draft != app.settings.audio;
            if sheet::primary_button(ui, "Apply", dirty).clicked() {
                app.settings.audio = app.ui.prefs.draft.clone();
                app.settings.save();
                app.restart_audio();
            }
        });
    });

    section(ui, "Routing");
    let channels = app.output_channels();
    let mut changed = false;
    group(ui, |ui| {
        let r = &mut app.project.routing;
        row(ui, "Program left", |ui| changed |= channel_combo(ui, "main_l", &mut r.main_left, channels, true));
        row_separator(ui);
        row(ui, "Program right", |ui| changed |= channel_combo(ui, "main_r", &mut r.main_right, channels, true));
        row_separator(ui);
        row(ui, "Timecode (LTC)", |ui| {
            changed |= channel_combo(ui, "ltc_ch", &mut app.project.ltc.channel, channels, true)
        });
    });
    footnote(
        ui,
        "Route only one program channel to sum the mix to mono — the classic setup is program on 1, LTC on 2.",
    );
    let r = &app.project.routing;
    let ltc = app.project.ltc.channel;
    if ltc >= 0 && (ltc == r.main_left || ltc == r.main_right) {
        ui.label(
            RichText::new("LTC shares an output with the program mix.").font(fonts::text(11.5)).color(theme::YELLOW),
        );
    }
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}

fn timecode_tab(app: &mut CueLineApp, ui: &mut Ui) {
    let mut changed = false;
    section(ui, "Timecode");
    group(ui, |ui| {
        row(ui, "Frame rate", |ui| {
            let before = app.project.frame_rate;
            egui::ComboBox::from_id_salt("fps").width(140.0).selected_text(format!("{} fps", before.label())).show_ui(
                ui,
                |ui| {
                    for r in FrameRate::ALL {
                        changed |=
                            ui.selectable_value(&mut app.project.frame_rate, r, format!("{} fps", r.label())).changed();
                    }
                },
            );
            if app.project.frame_rate != before && !app.project.start_timecode.is_valid(app.project.frame_rate) {
                let tc = app.project.start_timecode;
                app.project.start_timecode = Timecode { frames: 0, ..tc };
                app.ui.prefs.start_tc = app.project.start_timecode.to_string();
            }
        });
        row_separator(ui);
        row(ui, "Start timecode", |ui| {
            let p = &mut app.ui.prefs;
            let resp = ui.add(egui::TextEdit::singleline(&mut p.start_tc).font(fonts::mono(13.0)).desired_width(130.0));
            if resp.lost_focus() {
                match Timecode::parse(&p.start_tc, app.project.frame_rate) {
                    Some(tc) => {
                        changed |= tc != app.project.start_timecode;
                        app.project.start_timecode = tc;
                        p.start_tc = tc.to_string();
                    }
                    None => p.start_tc = app.project.start_timecode.to_string(),
                }
            }
        });
        row_separator(ui);
        row(ui, "User bits", |ui| {
            let p = &mut app.ui.prefs;
            let resp = ui.add(
                egui::TextEdit::singleline(&mut p.user_bits).font(fonts::mono(13.0)).desired_width(130.0).char_limit(8),
            );
            if resp.lost_focus() {
                match u32::from_str_radix(p.user_bits.trim(), 16) {
                    Ok(v) => {
                        changed |= v != app.project.user_bits;
                        app.project.user_bits = v;
                    }
                    Err(_) => p.user_bits = format!("{:08X}", app.project.user_bits),
                }
            }
        });
    });
    footnote(ui, "The start timecode is the label at project time zero. User bits are 8 hexadecimal digits.");

    section(ui, "Levels");
    group(ui, |ui| {
        row(ui, "LTC output", |ui| {
            let mut on = app.project.ltc.enabled;
            if switch(ui, &mut on).changed() {
                app.project.ltc.enabled = on;
                changed = true;
            }
        });
        row_separator(ui);
        row(ui, "LTC level", |ui| {
            changed |= ui
                .add(egui::Slider::new(&mut app.project.ltc.level_db, -40.0..=0.0).suffix(" dBFS").fixed_decimals(1))
                .changed();
        });
        row_separator(ui);
        row(ui, "Master volume", |ui| {
            changed |= ui
                .add(egui::Slider::new(&mut app.project.master_db, -40.0..=12.0).suffix(" dB").fixed_decimals(1))
                .changed();
        });
    });
    footnote(
        ui,
        "LTC is generated inside the audio callback, sample-locked to the program. −18 to −10 dBFS suits most readers.",
    );
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}

fn midi_tab(app: &mut CueLineApp, ui: &mut Ui) {
    let mut changed = false;
    section(ui, "MIDI Timecode");
    group(ui, |ui| {
        row(ui, "Send MTC", |ui| {
            let mut on = app.project.mtc.enabled;
            if switch(ui, &mut on).changed() {
                app.project.mtc.enabled = on;
                changed = true;
            }
        });
        row_separator(ui);
        row(ui, "Output port", |ui| {
            let label = app.project.mtc.port.clone().unwrap_or_else(|| "None".into());
            popup(ui, "midi_port", label, |ui| {
                changed |= ui.selectable_value(&mut app.project.mtc.port, None, "None").changed();
                for p in app.ui.prefs.midi_ports.clone() {
                    changed |= ui.selectable_value(&mut app.project.mtc.port, Some(p.clone()), p).changed();
                }
            });
        });
        row_separator(ui);
        row(ui, "Latency compensation", |ui| {
            changed |= ui
                .add(
                    egui::DragValue::new(&mut app.project.mtc.offset_ms)
                        .range(-200.0..=200.0)
                        .speed(0.1)
                        .suffix(" ms")
                        .fixed_decimals(1),
                )
                .on_hover_text("Positive values send MTC earlier to compensate receiver latency")
                .changed();
        });
    });
    if let Some(err) = app.mtc.status.error.lock().unwrap().clone() {
        ui.label(RichText::new(err).font(fonts::text(11.5)).color(theme::RED));
    }
    footnote(
        ui,
        "Quarter-frames are scheduled on a time-critical thread against the audio clock and the output latency, \
         so MTC lines up with what you hear. A full-frame message is sent on every locate.",
    );
    ui.add_space(6.0);
    if sheet::button(ui, "Refresh Ports").clicked() {
        app.ui.prefs.midi_ports = list_ports();
    }
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}
