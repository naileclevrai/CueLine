//! Export dialog: renders LTC (and optionally the program) to WAV in the
//! background.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use cueline_core::Timecode;
use eframe::egui::{self, RichText};

use super::theme;
use crate::app::{db_to_gain, CueLineApp};
use crate::engine::atomic::AtomicF32;
use crate::engine::shared::{RtTrack, TrackParams};
use crate::export::{run, ExportJob, ExportKind};

struct Running {
    progress: Arc<AtomicF32>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<(), String>>>,
    path: PathBuf,
}

pub struct ExportUi {
    pub open: bool,
    kind: ExportKind,
    from: String,
    to: String,
    initialised: bool,
    running: Option<Running>,
}

impl Default for ExportUi {
    fn default() -> Self {
        Self { open: false, kind: ExportKind::MonoMixLtc, from: String::new(), to: String::new(), initialised: false, running: None }
    }
}

pub fn window(app: &mut CueLineApp, ctx: &egui::Context) {
    poll_running(app);
    if !app.ui.export.open {
        app.ui.export.initialised = false;
        return;
    }
    if !app.ui.export.initialised {
        let end = app.project_end_secs().max(1.0);
        app.ui.export.from = app.timecode_at(0.0).to_string();
        app.ui.export.to = app.timecode_at(end).to_string();
        app.ui.export.initialised = true;
    }
    let mut open = true;
    egui::Window::new("Export WAV")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| contents(app, ui));
    if !open && app.ui.export.running.is_none() {
        app.ui.export.open = false;
    }
}

fn contents(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let rate = app.project.frame_rate;
    if let Some(r) = &app.ui.export.running {
        let p = r.progress.load();
        ui.label(format!("Rendering {}", r.path.display()));
        ui.add(egui::ProgressBar::new(p).show_percentage());
        if ui.button("Cancel").clicked() {
            r.cancel.store(true, Ordering::Relaxed);
        }
        ui.ctx().request_repaint();
        return;
    }

    egui::Grid::new("export").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        let e = &mut app.ui.export;
        ui.label("Content");
        egui::ComboBox::from_id_salt("kind").width(220.0).selected_text(e.kind.label()).show_ui(ui, |ui| {
            for k in ExportKind::ALL {
                ui.selectable_value(&mut e.kind, k, k.label());
            }
        });
        ui.end_row();
        ui.label("From");
        ui.add(egui::TextEdit::singleline(&mut e.from).font(theme::mono(13.0)).desired_width(120.0));
        ui.end_row();
        ui.label("To");
        ui.add(egui::TextEdit::singleline(&mut e.to).font(theme::mono(13.0)).desired_width(120.0));
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.small_button("Whole project").clicked() {
            let end = app.project_end_secs().max(1.0);
            app.ui.export.from = app.timecode_at(0.0).to_string();
            app.ui.export.to = app.timecode_at(end).to_string();
        }
        let markers = &app.project.markers;
        if markers.len() >= 2 && ui.small_button("First → last marker").clicked() {
            let (a, b) = (markers[0].time_secs, markers[markers.len() - 1].time_secs);
            app.ui.export.from = app.timecode_at(a + 1e-6).to_string();
            app.ui.export.to = app.timecode_at(b + 1e-6).to_string();
        }
    });
    ui.label(
        RichText::new(format!("24-bit WAV at {} Hz, {} fps LTC at {:.1} dBFS.", app.sample_rate(), rate.label(), app.project.ltc.level_db))
            .small()
            .color(theme::TEXT_DIM),
    );
    ui.add_space(6.0);
    if ui.button("Export…").clicked() {
        start(app);
    }
}

fn secs_of(app: &CueLineApp, text: &str) -> Option<f64> {
    let rate = app.project.frame_rate;
    let tc = Timecode::parse(text, rate)?;
    let frame = tc.to_frames(rate) - app.project.start_frames();
    Some(rate.sample_at_frame(frame, app.sample_rate()) as f64 / app.sample_rate() as f64)
}

fn start(app: &mut CueLineApp) {
    let (Some(from), Some(to)) = (secs_of(app, &app.ui.export.from), secs_of(app, &app.ui.export.to)) else {
        app.ui.toast_error("Invalid export range timecode".into());
        return;
    };
    if to <= from {
        app.ui.toast_error("Export end must be after its start".into());
        return;
    }
    let stem = app.project_path.as_ref().and_then(|p| p.file_stem()).map_or("CueLine".into(), |s| s.to_string_lossy().into_owned());
    let Some(mut path) = rfd::FileDialog::new()
        .set_title("Export WAV")
        .set_file_name(format!("{stem} LTC.wav"))
        .add_filter("WAV", &["wav"])
        .save_file()
    else {
        return;
    };
    if path.extension().is_none() {
        path.set_extension("wav");
    }

    let sr = app.sample_rate();
    let tracks: Vec<RtTrack> = app
        .tracks
        .iter()
        .filter(|t| t.clip_rate == sr)
        .filter_map(|t| {
            let p = TrackParams::new(db_to_gain(t.def.gain_db), (t.def.offset_secs * sr as f64).round() as i64);
            p.pan.store(t.def.pan);
            p.mute.store(t.def.mute, Ordering::Relaxed);
            p.solo.store(t.def.solo, Ordering::Relaxed);
            Some(RtTrack { data: t.clip.clone()?, params: Arc::new(p) })
        })
        .collect();
    let job = ExportJob {
        kind: app.ui.export.kind,
        path: path.clone(),
        sample_rate: sr,
        start: app.secs_to_samples(from),
        end: app.secs_to_samples(to),
        rate: app.project.frame_rate,
        start_frames: app.project.start_frames(),
        user_bits: app.project.user_bits,
        ltc_gain: db_to_gain(app.project.ltc.level_db),
        master_gain: db_to_gain(app.project.master_db),
        tracks,
    };
    let progress = Arc::new(AtomicF32::new(0.0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (p, c) = (progress.clone(), cancel.clone());
    let handle = std::thread::Builder::new()
        .name("cueline-export".into())
        .spawn(move || run(job, &p, &c))
        .ok();
    app.ui.export.running = Some(Running { progress, cancel, handle, path });
}

fn poll_running(app: &mut CueLineApp) {
    let finished = app.ui.export.running.as_ref().and_then(|r| r.handle.as_ref()).is_some_and(|h| h.is_finished());
    if !finished {
        return;
    }
    let mut r = app.ui.export.running.take().unwrap();
    match r.handle.take().unwrap().join() {
        Ok(Ok(())) => {
            app.ui.toast_info(format!("Exported {}", r.path.display()));
            app.ui.export.open = false;
        }
        Ok(Err(e)) => app.ui.toast_error(format!("Export failed: {e}")),
        Err(_) => app.ui.toast_error("Export crashed".into()),
    }
}
