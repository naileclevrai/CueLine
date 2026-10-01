//! Preferences window: audio device, routing, timecode and MIDI.

use cueline_core::{FrameRate, Timecode};
use eframe::egui::{self, RichText, Ui};

use super::headers::channel_combo;
use super::theme;
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
    egui::Window::new("Preferences")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tab = &mut app.ui.prefs.tab;
                ui.selectable_value(tab, Tab::Audio, "Audio device");
                ui.selectable_value(tab, Tab::Timecode, "Timecode");
                ui.selectable_value(tab, Tab::Midi, "MIDI Timecode");
            });
            ui.separator();
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

fn grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([16.0, 8.0]).min_col_width(120.0).show(ui, add);
}

fn audio_tab(app: &mut CueLineApp, ui: &mut Ui) {
    let mut changed_host = false;
    grid(ui, "audio", |ui| {
        let p = &mut app.ui.prefs;
        ui.label("Driver");
        let host_label = p.draft.host.clone().unwrap_or_else(|| "System default".into());
        egui::ComboBox::from_id_salt("host").width(260.0).selected_text(host_label).show_ui(ui, |ui| {
            changed_host |= ui.selectable_value(&mut p.draft.host, None, "System default").changed();
            for h in p.hosts.clone() {
                changed_host |= ui.selectable_value(&mut p.draft.host, Some(h.clone()), h).changed();
            }
        });
        ui.end_row();

        ui.label("Output device");
        let dev_label = p.draft.device.clone().unwrap_or_else(|| "Default output".into());
        egui::ComboBox::from_id_salt("device").width(260.0).selected_text(dev_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut p.draft.device, None, "Default output");
            for d in &p.devices {
                ui.selectable_value(
                    &mut p.draft.device,
                    Some(d.name.clone()),
                    format!("{}  ({} ch)", d.name, d.channels),
                );
            }
        });
        ui.end_row();

        let info = p.devices.iter().find(|d| Some(&d.name) == p.draft.device.as_ref());
        ui.label("Sample rate");
        let rate_label = p.draft.sample_rate.map_or("Device default".into(), |r| format!("{r} Hz"));
        egui::ComboBox::from_id_salt("rate").width(260.0).selected_text(rate_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut p.draft.sample_rate, None, "Device default");
            let rates = info.map_or(vec![44_100, 48_000, 96_000], |d| d.sample_rates.clone());
            for r in rates {
                ui.selectable_value(&mut p.draft.sample_rate, Some(r), format!("{r} Hz"));
            }
        });
        ui.end_row();

        ui.label("Buffer size");
        let buf_label = p.draft.buffer_frames.map_or("Driver default".into(), |b| format!("{b} samples"));
        egui::ComboBox::from_id_salt("buffer").width(260.0).selected_text(buf_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut p.draft.buffer_frames, None, "Driver default");
            for b in BUFFER_SIZES {
                if info.and_then(|d| d.buffer_range).is_none_or(|(lo, hi)| (lo..=hi).contains(&b)) {
                    ui.selectable_value(&mut p.draft.buffer_frames, Some(b), format!("{b} samples"));
                }
            }
        });
        ui.end_row();
    });
    if changed_host {
        let host = app.ui.prefs.draft.host.clone();
        app.ui.prefs.devices = list_output_devices(host.as_deref());
        app.ui.prefs.draft.device = None;
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let dirty = app.ui.prefs.draft != app.settings.audio;
        if ui.add_enabled(dirty, egui::Button::new("Apply")).clicked() {
            app.settings.audio = app.ui.prefs.draft.clone();
            app.settings.save();
            app.restart_audio();
        }
        if ui.button("Refresh devices").clicked() {
            let cfg = app.ui.prefs.draft.clone();
            app.ui.prefs.refresh(&cfg);
        }
        if ui.button("Restart audio").on_hover_text("Re-open the device, e.g. after it was unplugged").clicked() {
            app.restart_audio();
        }
    });
    match (&app.engine, &app.engine_error) {
        (Some(e), _) => {
            ui.label(
                RichText::new(format!(
                    "Running: {} — {} @ {} Hz, {} outputs",
                    e.host_name, e.device_name, e.sample_rate, e.channels
                ))
                .small()
                .color(theme::PLAYING),
            );
        }
        (None, Some(err)) => {
            ui.label(RichText::new(err).small().color(theme::ERROR));
        }
        _ => {}
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Routing").strong());
    let channels = app.output_channels();
    let mut changed = false;
    grid(ui, "routing", |ui| {
        let r = &mut app.project.routing;
        ui.label("Program left");
        changed |= channel_combo(ui, "main_l", &mut r.main_left, channels, true);
        ui.end_row();
        ui.label("Program right");
        changed |= channel_combo(ui, "main_r", &mut r.main_right, channels, true);
        ui.end_row();
        ui.label("LTC");
        changed |= channel_combo(ui, "ltc_ch", &mut app.project.ltc.channel, channels, true);
        ui.end_row();
    });
    ui.label(
        RichText::new("Set only one program channel to sum the mix to mono (typical: program on 1, LTC on 2).")
            .small()
            .color(theme::TEXT_DIM),
    );
    let r = &app.project.routing;
    let ltc = app.project.ltc.channel;
    if ltc >= 0 && (ltc == r.main_left || ltc == r.main_right) {
        ui.label(RichText::new("Warning: LTC shares an output with the program mix.").color(theme::WARN));
    }
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}

fn timecode_tab(app: &mut CueLineApp, ui: &mut Ui) {
    let mut changed = false;
    grid(ui, "tc", |ui| {
        ui.label("Frame rate");
        let before = app.project.frame_rate;
        egui::ComboBox::from_id_salt("fps").width(160.0).selected_text(before.label()).show_ui(ui, |ui| {
            for r in FrameRate::ALL {
                changed |= ui.selectable_value(&mut app.project.frame_rate, r, r.label()).changed();
            }
        });
        if app.project.frame_rate != before && !app.project.start_timecode.is_valid(app.project.frame_rate) {
            let tc = app.project.start_timecode;
            app.project.start_timecode = Timecode { frames: 0, ..tc };
            app.ui.prefs.start_tc = app.project.start_timecode.to_string();
        }
        ui.end_row();

        ui.label("Start timecode").on_hover_text("Timecode at the project start (time 0)");
        let p = &mut app.ui.prefs;
        let resp = ui.add(egui::TextEdit::singleline(&mut p.start_tc).font(theme::mono(13.0)).desired_width(160.0));
        if resp.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) && resp.has_focus() {
            match Timecode::parse(&p.start_tc, app.project.frame_rate) {
                Some(tc) => {
                    if tc != app.project.start_timecode {
                        app.project.start_timecode = tc;
                        changed = true;
                    }
                    p.start_tc = tc.to_string();
                }
                None => p.start_tc = app.project.start_timecode.to_string(),
            }
        }
        ui.end_row();

        ui.label("User bits (hex)").on_hover_text("32 user bits carried in every LTC frame");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut p.user_bits).font(theme::mono(13.0)).desired_width(160.0).char_limit(8),
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
        ui.end_row();

        ui.label("LTC level");
        changed |= ui
            .add(egui::Slider::new(&mut app.project.ltc.level_db, -40.0..=0.0).suffix(" dBFS").fixed_decimals(1))
            .changed();
        ui.end_row();

        ui.label("Master volume");
        changed |= ui
            .add(egui::Slider::new(&mut app.project.master_db, -40.0..=12.0).suffix(" dB").fixed_decimals(1))
            .changed();
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new("LTC is generated inside the audio callback, sample-locked to the program audio. -18 to -10 dBFS suits most readers.")
            .small()
            .color(theme::TEXT_DIM),
    );
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}

fn midi_tab(app: &mut CueLineApp, ui: &mut Ui) {
    let mut changed = false;
    grid(ui, "midi", |ui| {
        ui.label("Send MTC");
        changed |= ui.checkbox(&mut app.project.mtc.enabled, "").changed();
        ui.end_row();

        ui.label("Output port");
        let label = app.project.mtc.port.clone().unwrap_or_else(|| "None".into());
        egui::ComboBox::from_id_salt("midi_port").width(260.0).selected_text(label).show_ui(ui, |ui| {
            changed |= ui.selectable_value(&mut app.project.mtc.port, None, "None").changed();
            for p in app.ui.prefs.midi_ports.clone() {
                changed |= ui.selectable_value(&mut app.project.mtc.port, Some(p.clone()), p).changed();
            }
        });
        ui.end_row();

        ui.label("Offset").on_hover_text("Positive values send MTC earlier to compensate receiver latency");
        changed |= ui
            .add(
                egui::DragValue::new(&mut app.project.mtc.offset_ms)
                    .range(-200.0..=200.0)
                    .speed(0.1)
                    .suffix(" ms")
                    .fixed_decimals(1),
            )
            .changed();
        ui.end_row();
    });
    if ui.button("Refresh ports").clicked() {
        app.ui.prefs.midi_ports = list_ports();
    }
    if let Some(err) = app.mtc.status.error.lock().unwrap().clone() {
        ui.label(RichText::new(err).small().color(theme::ERROR));
    }
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Quarter-frames are scheduled on a time-critical thread against the audio clock, \
             compensated for the output latency, so MTC lines up with what you hear. \
             A full-frame message is sent on every locate.",
        )
        .small()
        .color(theme::TEXT_DIM),
    );
    if changed {
        app.dirty = true;
        app.apply_project_to_engine();
    }
}
