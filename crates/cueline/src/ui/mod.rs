//! User interface.

pub mod headers;
pub mod menus;
pub mod ruler;
pub mod theme;
pub mod timeline;
pub mod transport;
pub mod widgets;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::CueLineApp;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Error,
}

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub until: Instant,
}

pub const METER_LTC: u64 = 0;
pub const METER_MASTER_L: u64 = 1;
pub const METER_MASTER_R: u64 = 2;
/// Track meters use `METER_TRACK + track id`.
pub const METER_TRACK: u64 = 1000;

#[derive(Default)]
pub struct ExportUi {
    pub open: bool,
}

#[derive(Default)]
pub struct PrefsUi {
    pub open: bool,
}

pub struct UiState {
    pub toasts: Vec<Toast>,
    /// Where the last playback started (Space returns there).
    pub play_started_at: f64,
    /// Text of the "go to timecode" field while it is open.
    pub goto_text: Option<String>,
    pub rename_track: Option<(u64, String)>,
    pub rename_marker: Option<(usize, String)>,
    pub show_markers: bool,
    pub show_help: bool,
    pub zoom_to_fit: bool,
    pub export: ExportUi,
    pub prefs: PrefsUi,
    meters: HashMap<u64, (f32, Instant)>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            toasts: Vec::new(),
            play_started_at: 0.0,
            goto_text: None,
            rename_track: None,
            rename_marker: None,
            show_markers: true,
            show_help: false,
            zoom_to_fit: false,
            export: ExportUi::default(),
            prefs: PrefsUi::default(),
            meters: HashMap::new(),
        }
    }
}

impl UiState {
    /// Peak meter ballistics: instant attack, ~26 dB/s release.
    pub fn meter(&mut self, key: u64, peak: f32) -> f32 {
        let now = Instant::now();
        let (shown, at) = self.meters.entry(key).or_insert((0.0, now));
        let dt = now.duration_since(*at).as_secs_f32();
        *shown = peak.max(*shown * 0.05f32.powf(dt));
        *at = now;
        *shown
    }

    pub fn toast_info(&mut self, text: String) {
        self.push(ToastKind::Info, text, 3);
    }

    pub fn toast_error(&mut self, text: String) {
        log::warn!("{text}");
        self.push(ToastKind::Error, text, 6);
    }

    fn push(&mut self, kind: ToastKind, text: String, secs: u64) {
        self.toasts.push(Toast { kind, text, until: Instant::now() + Duration::from_secs(secs) });
    }
}

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    egui::Panel::top("menu")
        .frame(egui::Frame::new().fill(theme::BG_DEEP).inner_margin(egui::Margin::symmetric(6, 2)))
        .show(ui, |ui| menus::menu_bar(app, ui));
    egui::Panel::top("transport")
        .exact_size(66.0)
        .frame(egui::Frame::new().fill(theme::BG_HEADER).inner_margin(egui::Margin::symmetric(10, 6)))
        .show(ui, |ui| transport::draw(app, ui));
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_LANE_ALT))
        .show(ui, |ui| timeline::draw(app, ui));
}
