//! Headless-friendly screenshots for documentation and visual checks.
//!
//! `CUELINE_SCREENSHOT=out.ppm` makes CueLine render itself into a PPM
//! file and quit, without any OS-level input or screen capture.
//! Optional: `CUELINE_SCREENSHOT_PLAY=<seconds>` plays first, and
//! `CUELINE_SCREENSHOT_WINDOW=prefs|export|help` opens a window.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::{CueLineApp, TrackState};

pub struct DevShot {
    path: PathBuf,
    play_secs: f64,
    window: Option<String>,
    phase: Phase,
}

enum Phase {
    Loading,
    Playing(Instant),
    Settle(Instant),
    Requested,
}

impl DevShot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("CUELINE_SCREENSHOT")?.into();
        let play_secs = std::env::var("CUELINE_SCREENSHOT_PLAY").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let window = std::env::var("CUELINE_SCREENSHOT_WINDOW").ok();
        Some(Self { path, play_secs, window, phase: Phase::Loading })
    }

    pub fn update(&mut self, app: &mut CueLineApp, ctx: &egui::Context) {
        ctx.request_repaint();
        match self.phase {
            Phase::Loading => {
                if app.tracks.iter().all(|t| t.state != TrackState::Loading) && ctx.cumulative_frame_nr() > 30 {
                    match self.window.as_deref() {
                        Some("prefs") => app.ui.prefs.open = true,
                        Some("export") => app.ui.export.open = true,
                        Some("help") => app.ui.show_help = true,
                        _ => {}
                    }
                    if self.play_secs > 0.0 {
                        app.play();
                        self.phase = Phase::Playing(Instant::now());
                    } else {
                        self.phase = Phase::Settle(Instant::now());
                    }
                }
            }
            Phase::Playing(t) => {
                if t.elapsed().as_secs_f64() >= self.play_secs {
                    self.phase = Phase::Settle(Instant::now());
                }
            }
            Phase::Settle(t) => {
                if t.elapsed() > Duration::from_millis(400) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                    self.phase = Phase::Requested;
                }
            }
            Phase::Requested => {
                let image = ctx.input(|i| {
                    i.raw.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(img) = image {
                    let mut out = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
                    for p in &img.pixels {
                        out.extend_from_slice(&[p.r(), p.g(), p.b()]);
                    }
                    match std::fs::write(&self.path, out) {
                        Ok(()) => log::info!("screenshot written to {}", self.path.display()),
                        Err(e) => log::error!("screenshot: {e}"),
                    }
                    app.dirty = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}
