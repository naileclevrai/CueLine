//! Application state: the project, the audio engine and the glue between them.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use cueline_core::Timecode;
use eframe::egui;

use crate::audio::decode::{decode_file, DecodedAudio};
use crate::audio::peaks::Peaks;
use crate::audio::resample::resample;
use crate::engine::clock::now_ns;
use crate::engine::device::AudioEngine;
use crate::engine::mtc_out::MtcOutput;
use crate::engine::shared::{ClipData, Command, EngineShared, RtTrack, TrackParams};
use crate::history::{History, Snapshot};
use crate::project::{Marker, OutputLayout, Project, TrackDef, EXTENSION};
use crate::settings::Settings;
use crate::ui;

pub fn db_to_gain(db: f32) -> f32 {
    if db <= -90.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackState {
    Loading,
    Ready,
    Failed(String),
}

#[derive(Clone)]
pub struct Track {
    pub id: u64,
    pub def: TrackDef,
    pub params: Arc<TrackParams>,
    pub source: Option<Arc<DecodedAudio>>,
    pub peaks: Option<Arc<Peaks>>,
    pub clip: Option<Arc<ClipData>>,
    pub clip_rate: u32,
    pub state: TrackState,
}

impl Track {
    pub fn duration_secs(&self) -> f64 {
        self.source.as_ref().map_or(0.0, |s| s.duration_secs())
    }

    pub fn end_secs(&self) -> f64 {
        self.def.offset_secs + self.duration_secs()
    }
}

enum LoadMsg {
    Loaded { id: u64, source: Arc<DecodedAudio>, peaks: Arc<Peaks>, clip: Arc<ClipData>, rate: u32 },
    Resampled { id: u64, clip: Arc<ClipData>, rate: u32 },
    Failed { id: u64, error: String },
}

/// Everything the "New Show" assistant collects.
#[derive(Clone, Debug)]
pub struct NewShow {
    pub name: String,
    pub folder: PathBuf,
    pub rate: cueline_core::FrameRate,
    pub start: Timecode,
    pub layout: OutputLayout,
    /// `None` = do not send MIDI Timecode.
    pub mtc_port: Option<String>,
}

/// Keeps only characters that are safe in a folder and file name.
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String =
        name.trim()
            .chars()
            .map(|c| {
                if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                    '-'
                } else {
                    c
                }
            })
            .collect();
    let cleaned = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if cleaned.is_empty() {
        "Untitled Show".into()
    } else {
        cleaned
    }
}

/// `Documents\CueLine`, where new shows are created by default.
pub fn default_show_folder() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| PathBuf::from(h).join("Documents").join("CueLine"))
        .unwrap_or_else(|| PathBuf::from("CueLine"))
}

/// An action that discards the current project and therefore waits for the
/// "save changes?" sheet when there are unsaved edits.
#[derive(Clone, Debug, PartialEq)]
pub enum Discarding {
    /// Show the file picker, then open the chosen show.
    OpenDialog,
    OpenPath(PathBuf),
    CloseProject,
    Quit,
}

pub struct ViewState {
    /// Horizontal zoom.
    pub px_per_sec: f32,
    /// Timeline time at the left edge of the arrange view.
    pub scroll_secs: f64,
    pub track_height: f32,
    /// Vertical scroll of the track lanes, in pixels.
    pub scroll_y: f32,
    pub selected_track: Option<u64>,
    pub selected_marker: Option<usize>,
    pub drag: Option<ui::timeline::Drag>,
    pub context_track: Option<u64>,
    pub context_marker: Option<usize>,
    /// Timeline time where the last context menu was opened.
    pub context_time: f64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            px_per_sec: 40.0,
            scroll_secs: -1.0,
            track_height: 72.0,
            scroll_y: 0.0,
            selected_track: None,
            selected_marker: None,
            drag: None,
            context_track: None,
            context_marker: None,
            context_time: 0.0,
        }
    }
}

pub struct CueLineApp {
    pub ctx: egui::Context,
    pub project: Project,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    pub settings: Settings,
    pub shared: Arc<EngineShared>,
    pub engine: Option<AudioEngine>,
    pub engine_error: Option<String>,
    pub mtc: MtcOutput,
    pub tracks: Vec<Track>,
    next_id: u64,
    load_tx: Sender<LoadMsg>,
    load_rx: Receiver<LoadMsg>,
    pub view: ViewState,
    /// Edit cursor: where playback starts, in seconds from project start.
    pub cursor_secs: f64,
    pub ui: ui::UiState,
    pub history: History,
    /// When to try re-opening a failed audio device.
    audio_retry_at: Option<std::time::Instant>,
    devshot: Option<crate::devshot::DevShot>,
}

impl CueLineApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        let shared = EngineShared::new();
        let mtc = MtcOutput::spawn(shared.clone());
        let (load_tx, load_rx) = mpsc::channel();
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            project: Project::default(),
            project_path: None,
            dirty: false,
            settings: Settings::load(),
            shared,
            engine: None,
            engine_error: None,
            mtc,
            tracks: Vec::new(),
            next_id: 1,
            load_tx,
            load_rx,
            view: ViewState::default(),
            cursor_secs: 0.0,
            ui: ui::UiState::default(),
            history: History::default(),
            audio_retry_at: None,
            devshot: crate::devshot::DevShot::from_env(),
        };
        crate::audio::ffmpeg::set_override(app.settings.ffmpeg_path.clone());
        app.restart_audio();
        app.apply_project_to_engine();
        if let Some(path) = open {
            app.open_project(&path);
        }
        app
    }

    // ----- audio engine -------------------------------------------------

    pub fn sample_rate(&self) -> u32 {
        self.engine.as_ref().map_or(48_000, |e| e.sample_rate)
    }

    pub fn output_channels(&self) -> u16 {
        self.engine.as_ref().map_or(2, |e| e.channels)
    }

    pub fn secs_to_samples(&self, secs: f64) -> i64 {
        (secs * self.sample_rate() as f64).round() as i64
    }

    pub fn restart_audio(&mut self) {
        let was_playing = self.is_playing();
        let resume_at = self.position_secs();
        self.engine = None; // stop the old stream before opening the device again
        let pos = self.secs_to_samples(self.cursor_secs);
        match AudioEngine::start(&self.settings.audio, self.shared.clone(), pos) {
            Ok(engine) => {
                log::info!(
                    "audio: {} / {} @ {} Hz, {} ch",
                    engine.host_name,
                    engine.device_name,
                    engine.sample_rate,
                    engine.channels
                );
                self.engine = Some(engine);
                self.engine_error = None;
            }
            Err(e) => {
                log::error!("audio: {e}");
                self.engine_error = Some(e);
            }
        }
        self.resample_tracks_if_needed();
        self.apply_project_to_engine();
        self.push_tracks();
        if was_playing {
            self.seek(resume_at);
            self.send(Command::Play);
        }
    }

    pub fn send(&mut self, cmd: Command) {
        if let Some(e) = &mut self.engine {
            e.send(cmd);
        }
    }

    /// Pushes routing, levels and timecode settings to the engine and MTC.
    pub fn apply_project_to_engine(&mut self) {
        let p = &self.project;
        let sh = &self.shared;
        sh.main_left.store(p.routing.main_left, Ordering::Relaxed);
        sh.main_right.store(p.routing.main_right, Ordering::Relaxed);
        sh.master_gain.store(db_to_gain(p.master_db));
        sh.ltc_enabled.store(p.ltc.enabled, Ordering::Relaxed);
        sh.ltc_gain.store(db_to_gain(p.ltc.level_db));
        sh.ltc_channel.store(p.ltc.channel, Ordering::Relaxed);
        let (rate, start_frames, user_bits) = (p.frame_rate, p.start_frames(), p.user_bits);
        self.mtc.set_timecode(rate, start_frames);
        self.mtc.set_enabled(p.mtc.enabled);
        self.mtc.set_offset_ms(p.mtc.offset_ms);
        self.mtc.set_port(p.mtc.port.clone());
        self.send(Command::SetTimecode { rate, start_frames, user_bits });
    }

    /// Re-sends the list of playable tracks to the audio thread.
    pub fn push_tracks(&mut self) {
        let sr = self.sample_rate();
        let list: Vec<RtTrack> = self
            .tracks
            .iter()
            .filter(|t| t.clip_rate == sr)
            .filter_map(|t| Some(RtTrack { data: t.clip.clone()?, params: t.params.clone() }))
            .collect();
        for t in &self.tracks {
            Self::store_params(t, sr);
        }
        self.send(Command::SetTracks(list));
    }

    fn store_params(t: &Track, sr: u32) {
        let p = &t.params;
        p.gain.store(db_to_gain(t.def.gain_db));
        p.pan.store(t.def.pan);
        p.mute.store(t.def.mute, Ordering::Relaxed);
        p.solo.store(t.def.solo, Ordering::Relaxed);
        p.offset.store((t.def.offset_secs * sr as f64).round() as i64, Ordering::Relaxed);
    }

    /// Call after editing a track's definition.
    pub fn track_changed(&mut self, id: u64) {
        let sr = self.sample_rate();
        if let Some(t) = self.tracks.iter().find(|t| t.id == id) {
            Self::store_params(t, sr);
        }
        self.dirty = true;
    }

    fn resample_tracks_if_needed(&mut self) {
        let sr = self.sample_rate();
        for t in &mut self.tracks {
            let Some(src) = t.source.clone() else { continue };
            if t.clip_rate == sr {
                continue;
            }
            t.state = TrackState::Loading;
            let (tx, id, ctx) = (self.load_tx.clone(), t.id, self.ctx.clone());
            std::thread::spawn(move || {
                let msg = match resample(&src.channels, src.sample_rate, sr) {
                    Ok(ch) => LoadMsg::Resampled { id, clip: Arc::new(ClipData { channels: ch }), rate: sr },
                    Err(error) => LoadMsg::Failed { id, error },
                };
                let _ = tx.send(msg);
                ctx.request_repaint();
            });
        }
    }

    /// Per-frame housekeeping: finished imports, freed buffers, device errors.
    pub fn poll(&mut self) {
        let mut changed = false;
        while let Ok(msg) = self.load_rx.try_recv() {
            changed = true;
            match msg {
                LoadMsg::Loaded { id, source, peaks, clip, rate } => {
                    if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
                        t.source = Some(source);
                        t.peaks = Some(peaks);
                        t.clip = Some(clip);
                        t.clip_rate = rate;
                        t.state = TrackState::Ready;
                    }
                }
                LoadMsg::Resampled { id, clip, rate } => {
                    if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
                        t.clip = Some(clip);
                        t.clip_rate = rate;
                        t.state = TrackState::Ready;
                    }
                }
                LoadMsg::Failed { id, error } => {
                    if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
                        self.ui.toast_error(format!("{}: {error}", t.def.name));
                        t.state = TrackState::Failed(error);
                    }
                }
            }
        }
        if changed {
            self.push_tracks();
            if self.ui.zoom_to_fit_after_load && self.tracks.iter().all(|t| t.state != TrackState::Loading) {
                self.ui.zoom_to_fit_after_load = false;
                self.ui.zoom_to_fit = true;
            }
        }
        if let Some(e) = &mut self.engine {
            e.collect_garbage();
        }
        if self.settings.stop_at_end && self.is_playing() {
            let end = self.project_end_secs();
            if end > 0.0 && self.position_secs() >= end {
                self.send(Command::Pause);
                self.seek(end);
            }
        }
        if self.shared.stream_error.swap(false, Ordering::Relaxed) {
            self.ui.toast_error("Audio device lost — trying to reconnect…".into());
            self.audio_retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
        }
        if let Some(at) = self.audio_retry_at {
            if std::time::Instant::now() >= at {
                self.restart_audio();
                if self.engine.is_some() && !self.shared.stream_error.load(Ordering::Relaxed) {
                    self.audio_retry_at = None;
                    self.ui.toast_info("Audio device reconnected".into());
                } else {
                    self.audio_retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                }
            }
            self.ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    // ----- tracks -------------------------------------------------------

    pub fn add_track(&mut self, def: TrackDef) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let params = Arc::new(TrackParams::new(db_to_gain(def.gain_db), 0));
        let path = def.path.clone();
        self.tracks.push(Track {
            id,
            def,
            params,
            source: None,
            peaks: None,
            clip: None,
            clip_rate: 0,
            state: TrackState::Loading,
        });
        self.spawn_load(id, path);
        self.dirty = true;
        id
    }

    /// Decodes, analyses and resamples a file on a worker thread.
    fn spawn_load(&self, id: u64, path: PathBuf) {
        let (tx, sr, ctx) = (self.load_tx.clone(), self.sample_rate(), self.ctx.clone());
        std::thread::spawn(move || {
            let msg = (|| {
                let src = decode_file(&path)?;
                let peaks = Peaks::build(&src.channels, src.sample_rate);
                let clip = resample(&src.channels, src.sample_rate, sr)?;
                Ok(LoadMsg::Loaded {
                    id,
                    source: Arc::new(src),
                    peaks: Arc::new(peaks),
                    clip: Arc::new(ClipData { channels: clip }),
                    rate: sr,
                })
            })()
            .unwrap_or_else(|error: String| LoadMsg::Failed { id, error });
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// Points an offline track at a new file and reloads it.
    pub fn relink_track(&mut self, id: u64, path: PathBuf) {
        self.checkpoint();
        if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
            t.def.path = path.clone();
            t.state = TrackState::Loading;
            self.spawn_load(id, path);
            self.dirty = true;
        }
    }

    /// Moves a track one row up (`-1`) or down (`+1`).
    pub fn move_track(&mut self, id: u64, delta: isize) {
        let Some(i) = self.tracks.iter().position(|t| t.id == id) else { return };
        let j = i as isize + delta;
        if j < 0 || j as usize >= self.tracks.len() {
            return;
        }
        self.checkpoint();
        self.tracks.swap(i, j as usize);
        self.dirty = true;
    }

    pub fn import_files(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        self.checkpoint();
        let at = self.cursor_secs.max(0.0);
        for path in paths {
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let color = ui::theme::TRACK_COLORS[self.tracks.len() % ui::theme::TRACK_COLORS.len()];
            let id = self.add_track(TrackDef { name, path, offset_secs: at, color, ..Default::default() });
            self.view.selected_track = Some(id);
        }
    }

    pub fn remove_track(&mut self, id: u64) {
        self.checkpoint();
        self.tracks.retain(|t| t.id != id);
        if self.view.selected_track == Some(id) {
            self.view.selected_track = None;
        }
        self.push_tracks();
        self.dirty = true;
    }

    /// End of the last clip, at least one minute.
    pub fn project_end_secs(&self) -> f64 {
        let clips = self.tracks.iter().map(Track::end_secs).fold(0.0, f64::max);
        let markers = self.project.markers.iter().map(|m| m.time_secs).fold(0.0, f64::max);
        clips.max(markers)
    }

    // ----- markers ------------------------------------------------------

    pub fn add_marker_at(&mut self, secs: f64) {
        self.checkpoint();
        let n = self.project.markers.len() + 1;
        let secs = ui::timeline::snap_to_frame(secs.max(0.0), self.project.frame_rate);
        self.project.markers.push(Marker { name: format!("Cue {n}"), time_secs: secs, ..Default::default() });
        self.sort_markers();
        self.view.selected_marker = self.project.markers.iter().position(|m| m.time_secs == secs);
        self.dirty = true;
    }

    /// Adds the cues found in a CSV, MIDI or WAV file. Seconds are taken from
    /// the project start; timecodes are converted with the project rate.
    pub fn import_markers(&mut self, path: &Path) {
        let found = match crate::markers_io::import(path) {
            Ok(m) => m,
            Err(e) => return self.ui.toast_error(e),
        };
        self.checkpoint();
        let rate = self.project.frame_rate;
        let (mut added, mut skipped) = (0, 0);
        for (i, m) in found.into_iter().enumerate() {
            let secs = match m.time {
                crate::markers_io::MarkerTime::Seconds(s) => s,
                crate::markers_io::MarkerTime::Timecode(tc) => {
                    (tc.to_frames(rate) - self.project.start_frames()) as f64 / rate.fps()
                }
            };
            if secs < 0.0 {
                skipped += 1;
                continue;
            }
            let name = if m.name.is_empty() { format!("Cue {}", self.project.markers.len() + 1) } else { m.name };
            let colors =
                [[0xff, 0x9f, 0x0a], [0x0a, 0x84, 0xff], [0x30, 0xd1, 0x58], [0xbf, 0x5a, 0xf2], [0xff, 0x45, 0x3a]];
            self.project.markers.push(Marker { name, time_secs: secs, color: colors[i % colors.len()] });
            added += 1;
        }
        self.sort_markers();
        self.dirty = true;
        let mut msg = format!("Imported {added} marker{}", if added == 1 { "" } else { "s" });
        if skipped > 0 {
            msg.push_str(&format!(" ({skipped} before the project start were skipped)"));
        }
        self.ui.toast_info(msg);
    }

    pub fn remove_marker(&mut self, idx: usize) {
        if idx < self.project.markers.len() {
            self.checkpoint();
            self.project.markers.remove(idx);
            self.view.selected_marker = None;
            self.dirty = true;
        }
    }

    /// Keeps markers in time order, preserving the selection.
    pub fn sort_markers(&mut self) {
        let selected = self.view.selected_marker.and_then(|i| self.project.markers.get(i).cloned());
        self.project.markers.sort_by(|a, b| a.time_secs.total_cmp(&b.time_secs));
        if let Some(sel) = selected {
            self.view.selected_marker = self.project.markers.iter().position(|m| *m == sel);
        }
    }

    pub fn goto_marker(&mut self, idx: usize) {
        if let Some(m) = self.project.markers.get(idx) {
            let t = m.time_secs;
            self.view.selected_marker = Some(idx);
            self.seek(t);
        }
    }

    /// Jumps to the next (or previous) marker relative to the playhead.
    pub fn goto_adjacent_marker(&mut self, forward: bool) {
        let here = self.position_secs();
        let eps = 1e-4;
        let idx = if forward {
            self.project.markers.iter().position(|m| m.time_secs > here + eps)
        } else {
            self.project.markers.iter().rposition(|m| m.time_secs < here - eps)
        };
        if let Some(i) = idx {
            self.goto_marker(i);
        }
    }

    // ----- view ---------------------------------------------------------

    /// Zooms around the playhead.
    pub fn zoom_by(&mut self, factor: f32) {
        let v = &mut self.view;
        let anchor = self.cursor_secs;
        let offset = (anchor - v.scroll_secs) * v.px_per_sec as f64;
        v.px_per_sec = (v.px_per_sec * factor).clamp(0.02, 20_000.0);
        v.scroll_secs = anchor - offset / v.px_per_sec as f64;
    }

    // ----- undo ---------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot { tracks: self.tracks.clone(), markers: self.project.markers.clone() }
    }

    /// Records the current state before an edit.
    pub fn checkpoint(&mut self) {
        let s = self.snapshot();
        self.history.record(s);
    }

    fn restore(&mut self, s: Snapshot) {
        self.tracks = s.tracks;
        self.project.markers = s.markers;
        self.view.selected_marker = None;
        if let Some(id) = self.view.selected_track {
            if !self.tracks.iter().any(|t| t.id == id) {
                self.view.selected_track = None;
            }
        }
        self.dirty = true;
        self.push_tracks();
    }

    pub fn undo(&mut self) {
        let current = self.snapshot();
        if let Some(s) = self.history.undo(current) {
            self.restore(s);
        }
    }

    pub fn redo(&mut self) {
        let current = self.snapshot();
        if let Some(s) = self.history.redo(current) {
            self.restore(s);
        }
    }

    // ----- transport ----------------------------------------------------

    pub fn is_playing(&self) -> bool {
        self.shared.clock.read().playing
    }

    /// The position currently audible, in seconds from project start.
    pub fn position_secs(&self) -> f64 {
        let snap = self.shared.clock.read();
        if snap.playing {
            snap.position_at(now_ns()) / self.sample_rate() as f64
        } else {
            self.cursor_secs
        }
    }

    pub fn timecode_at(&self, secs: f64) -> Timecode {
        let rate = self.project.frame_rate;
        let frame = rate.frame_at_sample(self.secs_to_samples(secs), self.sample_rate());
        Timecode::from_frames(self.project.start_frames() + frame, rate)
    }

    pub fn play(&mut self) {
        if self.engine.is_none() {
            self.ui.toast_error("No audio device: open Preferences to choose one".into());
            return;
        }
        let pos = self.secs_to_samples(self.cursor_secs);
        log::debug!("play from {:.3}s", self.cursor_secs);
        self.ui.play_started_at = self.cursor_secs;
        self.send(Command::Seek(pos));
        self.send(Command::Play);
    }

    /// Stops and returns to where playback started (Reaper behaviour).
    pub fn stop(&mut self) {
        self.send(Command::Pause);
        self.seek(self.ui.play_started_at);
    }

    /// Stops and leaves the cursor at the current position.
    pub fn pause(&mut self) {
        let here = self.position_secs();
        self.send(Command::Pause);
        self.seek(here);
    }

    pub fn toggle_play(&mut self) {
        if self.is_playing() {
            self.stop();
        } else {
            self.play();
        }
    }

    pub fn seek(&mut self, secs: f64) {
        self.cursor_secs = secs;
        let pos = self.secs_to_samples(secs);
        self.send(Command::Seek(pos));
    }

    pub fn seek_timecode(&mut self, tc: Timecode) {
        let rate = self.project.frame_rate;
        let frame = tc.to_frames(rate) - self.project.start_frames();
        let secs = rate.sample_at_frame(frame, self.sample_rate()) as f64 / self.sample_rate() as f64;
        self.seek(secs);
    }

    // ----- project files ------------------------------------------------

    pub fn new_project(&mut self) {
        self.send(Command::Pause);
        self.history.clear();
        self.tracks.clear();
        self.project = Project::default();
        self.project_path = None;
        self.dirty = false;
        self.view = ViewState::default();
        self.cursor_secs = 0.0;
        self.seek(0.0);
        self.apply_project_to_engine();
        self.push_tracks();
    }

    /// Creates `<folder>/<name>/<name>.cueline` and opens it.
    pub fn create_show(&mut self, show: &NewShow) -> Result<PathBuf, String> {
        let name = sanitize_name(&show.name);
        let dir = show.folder.join(&name);
        let path = dir.join(format!("{name}.{EXTENSION}"));
        if path.exists() {
            return Err(format!("A show named \"{name}\" already exists in {}", show.folder.display()));
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        self.new_project();
        self.project.frame_rate = show.rate;
        self.project.start_timecode = if show.start.is_valid(show.rate) { show.start } else { Timecode::default() };
        self.project.apply_layout(show.layout);
        self.project.mtc.enabled = show.mtc_port.is_some();
        self.project.mtc.port = show.mtc_port.clone();
        self.apply_project_to_engine();
        if !self.save_project_to(&path) {
            return Err(format!("Cannot save {}", path.display()));
        }
        self.ui.screen = ui::Screen::Editor;
        Ok(path)
    }

    /// Back to the welcome screen (asks to save first).
    pub fn close_project(&mut self) {
        self.guard(Discarding::CloseProject);
    }

    /// Runs `action` now, or asks to save first when there are unsaved edits.
    pub fn guard(&mut self, action: Discarding) {
        if self.dirty {
            self.ui.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    /// Carries out an action once the user has chosen to save or discard.
    pub fn perform(&mut self, action: Discarding) {
        match action {
            Discarding::OpenDialog => ui::menus::pick_and_open(self),
            Discarding::OpenPath(p) => self.open_project(&p),
            Discarding::CloseProject => {
                self.new_project();
                self.ui.screen = ui::Screen::Welcome;
            }
            Discarding::Quit => {
                self.dirty = false;
                self.ui.closing = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    pub fn open_project(&mut self, path: &Path) {
        match Project::load(path) {
            Ok(p) => {
                self.ui.screen = ui::Screen::Editor;
                self.new_project();
                let defs = p.tracks.clone();
                self.project = p;
                for def in defs {
                    self.add_track(def);
                }
                self.project_path = Some(path.to_path_buf());
                self.dirty = false;
                self.settings.push_recent(path.to_path_buf());
                self.settings.save();
                self.apply_project_to_engine();
                self.ui.zoom_to_fit_after_load = true;
            }
            Err(e) => self.ui.toast_error(e),
        }
    }

    pub fn save_project_to(&mut self, path: &Path) -> bool {
        self.project.tracks = self.tracks.iter().map(|t| t.def.clone()).collect();
        match self.project.save(path) {
            Ok(()) => {
                self.project_path = Some(path.to_path_buf());
                self.dirty = false;
                self.settings.push_recent(path.to_path_buf());
                self.settings.save();
                self.ui.toast_info(format!("Saved {}", path.display()));
                true
            }
            Err(e) => {
                self.ui.toast_error(e);
                false
            }
        }
    }

    /// Document name and whether it has unsaved changes.
    pub fn title_parts(&self) -> (String, bool) {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or("Untitled".into(), |s| s.to_string_lossy().into_owned());
        (name, self.dirty)
    }

    pub fn title(&self) -> String {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or("Untitled".into(), |s| s.to_string_lossy().into_owned());
        format!("{name}{} — CueLine", if self.dirty { " *" } else { "" })
    }
}

impl eframe::App for CueLineApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.ctx().cumulative_frame_nr() == 1 {
            crate::platform::round_window_corners();
        }
        self.poll();
        ui::draw(self, ui);
        if let Some(mut shot) = self.devshot.take() {
            shot.update(self, ui.ctx());
            self.devshot = Some(shot);
        }
        if self.is_playing() {
            ui.ctx().request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::timeline::snap_to_frame;
    use cueline_core::FrameRate;

    #[test]
    fn show_names_are_safe_file_names() {
        assert_eq!(sanitize_name("  Opening Night  "), "Opening Night");
        assert_eq!(sanitize_name("Tour 2026: Paris/Lyon?"), "Tour 2026- Paris-Lyon-");
        assert_eq!(sanitize_name("..."), "Untitled Show");
        assert_eq!(sanitize_name(""), "Untitled Show");
    }

    #[test]
    fn decibels() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1e-5);
        assert_eq!(db_to_gain(-120.0), 0.0);
    }

    #[test]
    fn snapping_lands_on_frame_boundaries() {
        let r = FrameRate::Fps25;
        assert_eq!(snap_to_frame(1.03, r), 1.04);
        assert_eq!(snap_to_frame(1.019, r), 1.0);
        assert_eq!(snap_to_frame(0.01, r), 0.0);
        let df = FrameRate::Fps29_97Df;
        let t = snap_to_frame(10.0, df);
        assert!((t * df.fps() - (t * df.fps()).round()).abs() < 1e-9);
    }
}
