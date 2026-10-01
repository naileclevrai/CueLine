//! User interface.

pub mod theme;
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
pub struct UiState {
    pub toasts: Vec<Toast>,
    /// Where the last playback started (Space returns there).
    pub play_started_at: f64,
    /// Text of the "go to timecode" field while it is open.
    pub goto_text: Option<String>,
    meters: HashMap<u64, (f32, Instant)>,
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
    egui::Panel::top("transport")
        .exact_size(58.0)
        .frame(egui::Frame::new().fill(theme::BG_HEADER).inner_margin(egui::Margin::symmetric(10, 6)))
        .show_inside(ui, |ui| transport::draw(app, ui));
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.label(app.title());
    });
}
