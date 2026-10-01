//! User interface.

pub mod headers;
pub mod markers;
pub mod menus;
pub mod prefs;
pub mod ruler;
pub mod shortcuts;
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
    pub prefs: prefs::PrefsUi,
    pub last_title: String,
    pub closing: bool,
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
            prefs: prefs::PrefsUi::default(),
            last_title: String::new(),
            closing: false,
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

/// Imports dropped audio files, or opens a dropped project.
fn handle_dropped_files(app: &mut CueLineApp, ctx: &egui::Context) {
    let dropped: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
    if dropped.is_empty() {
        return;
    }
    let is_project = |p: &std::path::PathBuf| {
        p.extension().is_some_and(|e| e.eq_ignore_ascii_case(crate::project::EXTENSION))
    };
    if let Some(project) = dropped.iter().find(|p| is_project(p)) {
        if app.confirm_discard() {
            app.open_project(project);
        }
        return;
    }
    let audio: Vec<_> = dropped
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| menus::AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
        })
        .collect();
    if audio.is_empty() {
        app.ui.toast_error("Unsupported file type".into());
    } else {
        app.import_files(audio);
    }
}

fn draw_toasts(app: &mut CueLineApp, ctx: &egui::Context) {
    let now = Instant::now();
    app.ui.toasts.retain(|t| t.until > now);
    if app.ui.toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -14.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            for t in app.ui.toasts.iter().rev().take(4) {
                let color = match t.kind {
                    ToastKind::Info => theme::ACCENT,
                    ToastKind::Error => theme::ERROR,
                };
                egui::Frame::new()
                    .fill(theme::BG_HEADER)
                    .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.7)))
                    .corner_radius(4)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.label(egui::RichText::new(&t.text).color(theme::TEXT));
                    });
                ui.add_space(6.0);
            }
        });
    ctx.request_repaint_after(Duration::from_millis(250));
}

/// Keeps the OS window title in sync and guards against losing changes.
fn window_chrome(app: &mut CueLineApp, ctx: &egui::Context) {
    let title = app.title();
    if app.ui.last_title != title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        app.ui.last_title = title;
    }
    if ctx.input(|i| i.viewport().close_requested()) && !app.ui.closing {
        if app.confirm_discard() {
            app.ui.closing = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }
}

/// Shows a hint overlay while files are dragged over the window.
fn drop_overlay(ctx: &egui::Context) {
    if ctx.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let rect = ctx.content_rect();
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
    p.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(160));
    p.rect_stroke(rect.shrink(12.0), 6.0, egui::Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Inside);
    p.text(rect.center(), egui::Align2::CENTER_CENTER, "Drop audio files to import, or a .cueline project to open", egui::FontId::proportional(18.0), theme::TEXT);
}

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    shortcuts::handle(app, &ctx);
    handle_dropped_files(app, &ctx);
    egui::Panel::top("menu")
        .frame(egui::Frame::new().fill(theme::BG_DEEP).inner_margin(egui::Margin::symmetric(6, 2)))
        .show(ui, |ui| menus::menu_bar(app, ui));
    egui::Panel::top("transport")
        .exact_size(66.0)
        .frame(egui::Frame::new().fill(theme::BG_HEADER).inner_margin(egui::Margin::symmetric(10, 6)))
        .show(ui, |ui| transport::draw(app, ui));
    if app.ui.show_markers {
        egui::Panel::right("markers")
            .resizable(true)
            .default_size(260.0)
            .size_range(200.0..=480.0)
            .frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(egui::Margin::same(8)))
            .show(ui, |ui| markers::panel(app, ui));
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_LANE_ALT))
        .show(ui, |ui| timeline::draw(app, ui));
    prefs::window(app, &ctx);
    draw_toasts(app, &ctx);
    drop_overlay(&ctx);
    window_chrome(app, &ctx);
}
