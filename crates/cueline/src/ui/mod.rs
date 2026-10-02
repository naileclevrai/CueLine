//! User interface.

pub mod bigclock;
pub mod export_dialog;
pub mod fonts;
pub mod headers;
pub mod markers;
pub mod menus;
pub mod new_show;
pub mod prefs;
pub mod ruler;
pub mod sheet;
pub mod shortcuts;
pub mod statusbar;
pub mod theme;
pub mod timeline;
pub mod titlebar;
pub mod transport;
pub mod welcome;
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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Launch screen: new / open / recent.
    #[default]
    Welcome,
    Editor,
}

pub struct UiState {
    pub screen: Screen,
    pub new_show: new_show::NewShowUi,
    pub logo: Option<egui::TextureHandle>,
    pub toasts: Vec<Toast>,
    /// Where the last playback started (Space returns there).
    pub play_started_at: f64,
    /// Text of the "go to timecode" field while it is open.
    pub goto_text: Option<String>,
    pub rename_track: Option<(u64, String)>,
    pub rename_marker: Option<(usize, String)>,
    pub show_markers: bool,
    pub show_help: bool,
    pub show_big_clock: bool,
    pub zoom_to_fit: bool,
    pub zoom_to_fit_after_load: bool,
    pub export: export_dialog::ExportUi,
    pub prefs: prefs::PrefsUi,
    pub last_title: String,
    pub closing: bool,
    meters: HashMap<u64, (f32, Instant)>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            screen: Screen::Welcome,
            new_show: new_show::NewShowUi::default(),
            logo: None,
            toasts: Vec::new(),
            play_started_at: 0.0,
            goto_text: None,
            rename_track: None,
            rename_marker: None,
            show_markers: true,
            show_help: false,
            show_big_clock: false,
            zoom_to_fit: false,
            zoom_to_fit_after_load: false,
            export: export_dialog::ExportUi::default(),
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
    let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
        i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect()
    });
    if dropped.is_empty() {
        return;
    }
    let is_project =
        |p: &std::path::PathBuf| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(crate::project::EXTENSION));
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
        // Dropping audio on the welcome screen starts a quick untitled show.
        app.ui.screen = Screen::Editor;
        app.import_files(audio);
    }
}

fn help_window(app: &mut CueLineApp, ctx: &egui::Context) {
    let mut open = app.ui.show_help;
    sheet::show(ctx, "Keyboard Shortcuts", &mut open, 440.0, |ui| {
        sheet::group(ui, |ui| {
            for (i, (keys, what)) in shortcuts::HELP.iter().enumerate() {
                if i > 0 {
                    sheet::row_separator(ui);
                }
                ui.horizontal(|ui| {
                    ui.set_min_height(24.0);
                    ui.label(egui::RichText::new(*what).color(theme::TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(*keys).font(fonts::medium(12.0)).color(theme::TEXT_DIM));
                    });
                });
            }
        });
    });
    app.ui.show_help = open;
}

/// macOS-style notification banners, top right under the toolbar.
fn draw_toasts(app: &mut CueLineApp, ctx: &egui::Context) {
    let now = Instant::now();
    app.ui.toasts.retain(|t| t.until > now);
    if app.ui.toasts.is_empty() {
        return;
    }
    let top = titlebar::HEIGHT + transport::HEIGHT + 10.0;
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, top))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            for t in app.ui.toasts.iter().rev().take(4) {
                let (color, title) = match t.kind {
                    ToastKind::Info => (theme::BLUE, "CueLine"),
                    ToastKind::Error => (theme::RED, "Attention"),
                };
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x2e, 0x2e, 0x31))
                    .stroke(egui::Stroke::new(0.5, egui::Color32::from_white_alpha(28)))
                    .corner_radius(12)
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 28,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(140),
                    })
                    .inner_margin(egui::Margin { left: 14, right: 16, top: 10, bottom: 11 })
                    .show(ui, |ui| {
                        ui.set_width(320.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 30.0), egui::Sense::hover());
                            ui.painter().circle_filled(egui::pos2(r.center().x, r.top() + 8.0), 4.0, color);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(egui::RichText::new(title).font(fonts::semibold(12.5)).color(theme::TEXT));
                                ui.label(egui::RichText::new(&t.text).font(fonts::text(12.0)).color(theme::TEXT_DIM));
                            });
                        });
                    });
                ui.add_space(8.0);
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
    p.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(150));
    p.rect_stroke(rect.shrink(14.0), 14.0, egui::Stroke::new(2.0, theme::BLUE), egui::StrokeKind::Inside);
    p.text(
        rect.center() - egui::vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        "Drop to import",
        fonts::semibold(20.0),
        theme::TEXT,
    );
    p.text(
        rect.center() + egui::vec2(0.0, 14.0),
        egui::Align2::CENTER_CENTER,
        "Audio files become tracks · a .cueline file opens the project",
        fonts::text(13.0),
        theme::TEXT_DIM,
    );
}

/// Paints the shared gradient behind the title bar and toolbar.
fn chrome_background(ui: &egui::Ui) {
    let full = ui.max_rect();
    let rect = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), titlebar::HEIGHT + transport::HEIGHT));
    let mut mesh = egui::Mesh::default();
    let (top, bottom) = (theme::BG_TOOLBAR_TOP, theme::BG_TOOLBAR);
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    let p = ui.painter();
    p.add(egui::Shape::mesh(mesh));
    p.hline(rect.x_range(), rect.top() + 0.5, egui::Stroke::new(1.0, egui::Color32::from_white_alpha(14)));
    p.hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, theme::HAIRLINE));
}

/// The launch screen keeps the title bar (window controls, menus) only.
fn draw_welcome(app: &mut CueLineApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let full = ui.max_rect();
    let bar = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), titlebar::HEIGHT));
    ui.painter().rect_filled(bar, 0.0, theme::BG_TOOLBAR_TOP);
    egui::Panel::top("titlebar")
        .exact_size(titlebar::HEIGHT)
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show(ui, |ui| titlebar::draw(app, ui));
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| welcome::draw(app, ui));
    prefs::window(app, ctx);
    new_show::window(app, ctx);
    help_window(app, ctx);
    draw_toasts(app, ctx);
    drop_overlay(ctx);
    window_chrome(app, ctx);
    titlebar::resize_edges(ctx);
}

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    shortcuts::handle(app, &ctx);
    handle_dropped_files(app, &ctx);
    if app.ui.screen == Screen::Welcome {
        draw_welcome(app, ui, &ctx);
        return;
    }
    chrome_background(ui);
    egui::Panel::top("titlebar")
        .exact_size(titlebar::HEIGHT)
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show(ui, |ui| titlebar::draw(app, ui));
    egui::Panel::top("toolbar")
        .exact_size(transport::HEIGHT)
        .frame(egui::Frame::NONE)
        .show_separator_line(false)
        .show(ui, |ui| transport::draw(app, ui));
    egui::Panel::bottom("status")
        .exact_size(statusbar::HEIGHT)
        .frame(egui::Frame::new().fill(theme::BG_TOOLBAR).inner_margin(egui::Margin::symmetric(10, 0)))
        .show_separator_line(false)
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().hline(r.x_range(), r.top() + 0.5, egui::Stroke::new(1.0, theme::HAIRLINE));
            statusbar::draw(app, ui)
        });
    if app.ui.show_markers {
        egui::Panel::right("markers")
            .resizable(true)
            .default_size(280.0)
            .size_range(220.0..=480.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(theme::BG_PANEL).inner_margin(egui::Margin {
                left: 12,
                right: 12,
                top: 12,
                bottom: 8,
            }))
            .show(ui, |ui| {
                let r = ui.max_rect().expand2(egui::vec2(12.0, 12.0));
                ui.painter().vline(r.left() + 0.5, r.y_range(), egui::Stroke::new(1.0, theme::HAIRLINE));
                markers::panel(app, ui)
            });
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_CONTENT))
        .show(ui, |ui| timeline::draw(app, ui));
    prefs::window(app, &ctx);
    new_show::window(app, &ctx);
    export_dialog::window(app, &ctx);
    help_window(app, &ctx);
    bigclock::window(app, &ctx);
    draw_toasts(app, &ctx);
    drop_overlay(&ctx);
    window_chrome(app, &ctx);
    titlebar::resize_edges(&ctx);
}
