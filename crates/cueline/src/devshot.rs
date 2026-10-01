//! Headless-friendly screenshots for documentation and visual checks.
//!
//! `CUELINE_SCREENSHOT=out.ppm` makes CueLine render itself into a PPM
//! file and quit, without any OS-level input or screen capture. Audio output
//! is silenced while this mode is active. Optional variables:
//!
//! * `CUELINE_SCREENSHOT_SEEK=<seconds>` — move the playhead first;
//! * `CUELINE_SCREENSHOT_PLAY=<seconds>` — play for that long first;
//! * `CUELINE_SCREENSHOT_WINDOW=prefs|export|help` — open a dialog;
//! * `CUELINE_SCREENSHOT_FRAMES=<n>` and `CUELINE_SCREENSHOT_INTERVAL=<ms>` —
//!   record a sequence (`out_000.ppm`, `out_001.ppm`, …) for animations.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::{CueLineApp, TrackState};

pub struct DevShot {
    path: PathBuf,
    seek: Option<f64>,
    play_secs: f64,
    window: Option<String>,
    frames: usize,
    interval: Duration,
    phase: Phase,
}

enum Phase {
    Loading,
    Playing(Instant),
    Capturing { next_at: Instant, index: usize, pending: bool },
}

fn env_num<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|s| s.parse().ok())
}

impl DevShot {
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("CUELINE_SCREENSHOT")?.into();
        Some(Self {
            path,
            seek: env_num("CUELINE_SCREENSHOT_SEEK"),
            play_secs: env_num("CUELINE_SCREENSHOT_PLAY").unwrap_or(0.0),
            window: std::env::var("CUELINE_SCREENSHOT_WINDOW").ok(),
            frames: env_num("CUELINE_SCREENSHOT_FRAMES").unwrap_or(1).max(1),
            interval: Duration::from_millis(env_num("CUELINE_SCREENSHOT_INTERVAL").unwrap_or(80)),
            phase: Phase::Loading,
        })
    }

    fn frame_path(&self, index: usize) -> PathBuf {
        if self.frames == 1 {
            return self.path.clone();
        }
        let stem = self.path.file_stem().map_or("frame".into(), |s| s.to_string_lossy().into_owned());
        self.path.with_file_name(format!("{stem}_{index:03}.ppm"))
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
                    if let Some(t) = self.seek {
                        app.seek(t);
                    }
                    if self.play_secs > 0.0 || self.frames > 1 {
                        app.play();
                    }
                    let settle = Instant::now() + Duration::from_millis(400);
                    let start = Instant::now() + Duration::from_secs_f64(self.play_secs);
                    self.phase = Phase::Playing(start.max(settle));
                }
            }
            Phase::Playing(until) => {
                if Instant::now() >= until {
                    self.phase = Phase::Capturing { next_at: Instant::now(), index: 0, pending: false };
                }
            }
            Phase::Capturing { next_at, index, pending } => {
                if !pending && Instant::now() >= next_at {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                    self.phase = Phase::Capturing { next_at, index, pending: true };
                    return;
                }
                let image = ctx.input(|i| {
                    i.raw.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                let Some(img) = image else { return };
                let mut out = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
                for p in &img.pixels {
                    out.extend_from_slice(&[p.r(), p.g(), p.b()]);
                }
                let path = self.frame_path(index);
                if let Err(e) = std::fs::write(&path, out) {
                    log::error!("screenshot: {e}");
                }
                if index + 1 >= self.frames {
                    log::info!("screenshot(s) written to {}", self.path.display());
                    app.dirty = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.phase =
                        Phase::Capturing { next_at: next_at + self.interval, index: index + 1, pending: false };
                }
            }
        }
    }
}
