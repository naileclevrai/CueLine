//! Export sheet: renders the show to WAV, AIFF, FLAC, MP3, Ogg Vorbis, or
//! (with ffmpeg) Opus and AAC, in the background.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use cueline_core::Timecode;
use eframe::egui::{self, RichText};

use super::sheet::{self, footnote, group, row, row_separator, section};
use super::{fonts, theme};
use crate::app::{db_to_gain, CueLineApp};
use crate::audio::ffmpeg;
use crate::engine::atomic::AtomicF32;
use crate::export::{run, Codec, Encoding, ExportJob, ExportKind, ExportTrack};

struct Running {
    progress: Arc<AtomicF32>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<(), String>>>,
    path: PathBuf,
}

pub struct ExportUi {
    pub open: bool,
    kind: ExportKind,
    encoding: Encoding,
    from: String,
    to: String,
    initialised: bool,
    has_ffmpeg: bool,
    running: Option<Running>,
}

impl Default for ExportUi {
    fn default() -> Self {
        Self {
            open: false,
            kind: ExportKind::MonoMixLtc,
            encoding: Encoding::new(Codec::Wav),
            from: String::new(),
            to: String::new(),
            initialised: false,
            has_ffmpeg: false,
            running: None,
        }
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
        app.ui.export.has_ffmpeg = ffmpeg::locate().is_some();
        app.ui.export.initialised = true;
    }
    let mut open = true;
    sheet::show(ctx, "Export Audio", &mut open, 480.0, |ui| contents(app, ui));
    if !open && app.ui.export.running.is_none() {
        app.ui.export.open = false;
    }
}

fn rate_label(rate: Option<u32>, device: u32) -> String {
    match rate {
        None => format!("Project ({:.1} kHz)", device as f32 / 1000.0),
        Some(r) => format!("{:.1} kHz", r as f32 / 1000.0),
    }
}

/// Keeps the encoding valid after the codec or content changed.
fn normalise(e: &mut ExportUi, device_rate: u32) {
    let c = e.encoding.codec;
    if !c.depths().is_empty() && !c.depths().contains(&e.encoding.depth) {
        e.encoding.depth = c.depths()[c.depths().len().min(2) - 1];
    }
    let rate = e.encoding.sample_rate.unwrap_or(device_rate);
    if !c.sample_rates().contains(&rate) {
        e.encoding.sample_rate = Some(if c.sample_rates().contains(&48_000) { 48_000 } else { c.sample_rates()[0] });
    }
    if !c.bitrates().is_empty() && !c.bitrates().contains(&e.encoding.kbps) {
        e.encoding.kbps = Encoding::new(c).kbps;
    }
}

fn contents(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let rate = app.project.frame_rate;
    let device_rate = app.sample_rate();
    if let Some(r) = &app.ui.export.running {
        let p = r.progress.load();
        let name = r.path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
        ui.label(RichText::new(format!("Rendering “{name}”…")).font(fonts::medium(13.0)).color(theme::TEXT));
        ui.add_space(6.0);
        ui.add(egui::ProgressBar::new(p).desired_height(6.0).fill(theme::BLUE).corner_radius(3));
        ui.add_space(2.0);
        footnote(ui, &format!("{:.0} %", p * 100.0));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if sheet::button(ui, "Cancel").clicked() {
                r.cancel.store(true, Ordering::Relaxed);
            }
        });
        ui.ctx().request_repaint();
        return;
    }

    sheet::scroll_body(ui, 60.0, |ui| {
        section(ui, "Content");
        group(ui, |ui| {
            let e = &mut app.ui.export;
            row(ui, "Channels", |ui| {
                egui::ComboBox::from_id_salt("kind").width(230.0).selected_text(e.kind.label()).show_ui(ui, |ui| {
                    for k in ExportKind::ALL {
                        ui.selectable_value(&mut e.kind, k, k.label());
                    }
                });
            });
            row_separator(ui);
            row(ui, "Start", |ui| {
                ui.add(egui::TextEdit::singleline(&mut e.from).font(fonts::mono(13.0)).desired_width(130.0));
            });
            row_separator(ui);
            row(ui, "End", |ui| {
                ui.add(egui::TextEdit::singleline(&mut e.to).font(fonts::mono(13.0)).desired_width(130.0));
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if sheet::button(ui, "Whole Project").clicked() {
                let end = app.project_end_secs().max(1.0);
                app.ui.export.from = app.timecode_at(0.0).to_string();
                app.ui.export.to = app.timecode_at(end).to_string();
            }
            let markers = &app.project.markers;
            if markers.len() >= 2 && sheet::button(ui, "First → Last Cue").clicked() {
                let (a, b) = (markers[0].time_secs, markers[markers.len() - 1].time_secs);
                app.ui.export.from = app.timecode_at(a + 1e-6).to_string();
                app.ui.export.to = app.timecode_at(b + 1e-6).to_string();
            }
        });

        section(ui, "Format");
        let channels = app.ui.export.kind.channels();
        let has_ffmpeg = app.ui.export.has_ffmpeg;
        group(ui, |ui| {
            let e = &mut app.ui.export;
            row(ui, "File type", |ui| {
                egui::ComboBox::from_id_salt("codec").width(230.0).selected_text(e.encoding.codec.label()).show_ui(
                    ui,
                    |ui| {
                        for c in Codec::ALL {
                            let ok = channels <= c.max_channels() && (!c.needs_ffmpeg() || has_ffmpeg);
                            let mut text = c.label().to_string();
                            if c.needs_ffmpeg() && !has_ffmpeg {
                                text.push_str("  (needs ffmpeg)");
                            } else if channels > c.max_channels() {
                                text.push_str(&format!("  (max {} ch)", c.max_channels()));
                            } else if c.lossless() {
                                text.push_str("  · lossless");
                            }
                            let selected = e.encoding.codec == c;
                            if ui.add_enabled(ok, egui::Button::selectable(selected, text)).clicked() {
                                e.encoding.codec = c;
                                ui.close();
                            }
                        }
                    },
                );
            });
            normalise(e, device_rate);
            let codec = e.encoding.codec;
            if !codec.depths().is_empty() {
                row_separator(ui);
                row(ui, "Bit depth", |ui| {
                    egui::ComboBox::from_id_salt("depth").width(160.0).selected_text(e.encoding.depth.label()).show_ui(
                        ui,
                        |ui| {
                            for d in codec.depths() {
                                ui.selectable_value(&mut e.encoding.depth, *d, d.label());
                            }
                        },
                    );
                });
            }
            row_separator(ui);
            row(ui, "Sample rate", |ui| {
                egui::ComboBox::from_id_salt("srate")
                    .width(160.0)
                    .selected_text(rate_label(e.encoding.sample_rate, device_rate))
                    .show_ui(ui, |ui| {
                        if codec.sample_rates().contains(&device_rate) {
                            ui.selectable_value(&mut e.encoding.sample_rate, None, rate_label(None, device_rate));
                        }
                        for r in codec.sample_rates() {
                            ui.selectable_value(
                                &mut e.encoding.sample_rate,
                                Some(*r),
                                rate_label(Some(*r), device_rate),
                            );
                        }
                    });
            });
            if !codec.bitrates().is_empty() {
                row_separator(ui);
                row(ui, "Bitrate", |ui| {
                    egui::ComboBox::from_id_salt("kbps")
                        .width(160.0)
                        .selected_text(format!("{} kbit/s", e.encoding.kbps))
                        .show_ui(ui, |ui| {
                            for k in codec.bitrates() {
                                ui.selectable_value(&mut e.encoding.kbps, *k, format!("{k} kbit/s"));
                            }
                        });
                });
            }
            if codec == Codec::Vorbis {
                row_separator(ui);
                row(ui, "Quality", |ui| {
                    ui.add(egui::Slider::new(&mut e.encoding.vorbis_quality, 0.0..=1.0).fixed_decimals(1));
                });
            }
        });

        let e = &app.ui.export;
        if e.kind.has_ltc() && !e.encoding.codec.lossless() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Lossy codecs can smear LTC edges. Use WAV, AIFF or FLAC when the file feeds a timecode reader.",
                )
                .font(fonts::text(11.5))
                .color(theme::YELLOW),
            );
        }
        if !has_ffmpeg {
            footnote(ui, "Install ffmpeg to also export Opus and AAC (see Settings → Formats).");
        }
        footnote(
            ui,
            &format!(
                "{} fps LTC at {:.1} dBFS, synthesised at the export sample rate by the playback engine.",
                rate.label(),
                app.project.ltc.level_db
            ),
        );
    });

    ui.add_space(10.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if sheet::primary_button(ui, "Export…", true).clicked() {
            start(app);
        }
        if sheet::button(ui, "Cancel").clicked() {
            app.ui.export.open = false;
        }
    });
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
    let (kind, encoding) = (app.ui.export.kind, app.ui.export.encoding);
    if let Err(e) = encoding.validate(kind.channels()) {
        app.ui.toast_error(e);
        return;
    }
    let ext = encoding.codec.extension();
    let stem = app
        .project_path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map_or("CueLine".into(), |s| s.to_string_lossy().into_owned());
    let suffix = if kind.has_ltc() { " LTC" } else { "" };
    let mut dialog = rfd::FileDialog::new()
        .set_title("Export audio")
        .set_file_name(format!("{stem}{suffix}.{ext}"))
        .add_filter(encoding.codec.label(), &[ext]);
    if let Some(dir) = app.project_path.as_ref().and_then(|p| p.parent()) {
        dialog = dialog.set_directory(dir);
    }
    let Some(mut path) = dialog.save_file() else { return };
    if path.extension().is_none() {
        path.set_extension(ext);
    }

    let sr = app.sample_rate();
    let tracks: Vec<ExportTrack> = app
        .tracks
        .iter()
        .filter(|t| t.clip_rate == sr)
        .filter_map(|t| {
            Some(ExportTrack {
                clip: t.clip.clone()?,
                source: t.source.clone(),
                gain: db_to_gain(t.def.gain_db),
                pan: t.def.pan,
                mute: t.def.mute,
                solo: t.def.solo,
                offset_secs: t.def.offset_secs,
            })
        })
        .collect();
    let job = ExportJob {
        kind,
        path: path.clone(),
        encoding,
        engine_rate: sr,
        start_secs: from,
        end_secs: to,
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
    let handle = std::thread::Builder::new().name("cueline-export".into()).spawn(move || run(job, &p, &c)).ok();
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
