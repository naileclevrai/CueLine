//! Offline rendering to audio files.
//!
//! The export drives the very same [`Mixer`] used for live playback. When
//! the target sample rate differs from the device rate, the clips are
//! resampled and the mixer runs at the target rate, so LTC is synthesised
//! natively at that rate rather than resampled.

mod aiff;
pub mod encode;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cueline_core::FrameRate;
use rtrb::RingBuffer;

pub use encode::{Codec, Encoding};

use crate::audio::decode::DecodedAudio;
use crate::audio::resample::resample;
use crate::engine::atomic::AtomicF32;
use crate::engine::mixer::Mixer;
use crate::engine::shared::{ClipData, Command, EngineShared, RtTrack, TrackParams, NO_CHANNEL};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    /// Mono file containing only the LTC signal.
    LtcOnly,
    /// Stereo: program mixed to mono on the left, LTC on the right.
    MonoMixLtc,
    /// Three channels: stereo program then LTC.
    StereoMixLtc,
    /// Stereo program without timecode.
    StereoMix,
}

impl ExportKind {
    pub const ALL: [ExportKind; 4] =
        [ExportKind::MonoMixLtc, ExportKind::StereoMixLtc, ExportKind::LtcOnly, ExportKind::StereoMix];

    pub fn label(self) -> &'static str {
        match self {
            ExportKind::LtcOnly => "LTC only (mono)",
            ExportKind::MonoMixLtc => "Mix L + LTC R (stereo)",
            ExportKind::StereoMixLtc => "Stereo mix + LTC (3 ch)",
            ExportKind::StereoMix => "Stereo mix, no LTC",
        }
    }

    pub fn channels(self) -> usize {
        match self {
            ExportKind::LtcOnly => 1,
            ExportKind::MonoMixLtc | ExportKind::StereoMix => 2,
            ExportKind::StereoMixLtc => 3,
        }
    }

    pub fn has_ltc(self) -> bool {
        self != ExportKind::StereoMix
    }
}

/// A track as the exporter needs it, decoupled from the live engine.
pub struct ExportTrack {
    /// Samples at the engine rate.
    pub clip: Arc<ClipData>,
    /// Original decoded file, used for best-quality resampling.
    pub source: Option<Arc<DecodedAudio>>,
    pub gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub offset_secs: f64,
}

pub struct ExportJob {
    pub kind: ExportKind,
    pub path: PathBuf,
    pub encoding: Encoding,
    /// Rate of `ExportTrack::clip` (the device rate).
    pub engine_rate: u32,
    /// Timeline range to render, in seconds.
    pub start_secs: f64,
    pub end_secs: f64,
    pub rate: FrameRate,
    pub start_frames: i64,
    pub user_bits: u32,
    pub ltc_gain: f32,
    pub master_gain: f32,
    pub tracks: Vec<ExportTrack>,
}

/// Pulls rendered audio out of a mixer, with progress and cancellation.
pub struct Renderer<'a> {
    mixer: Mixer,
    pub channels: usize,
    pub sample_rate: u32,
    /// Whether one of the channels carries LTC.
    pub has_ltc: bool,
    remaining: usize,
    total: usize,
    progress: &'a AtomicF32,
    cancel: &'a AtomicBool,
}

impl Renderer<'_> {
    /// Fills `buf` (interleaved) and returns the frames written; 0 when done.
    pub fn fill(&mut self, buf: &mut [f32]) -> Result<usize, String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("export cancelled".into());
        }
        let n = (buf.len() / self.channels).min(self.remaining);
        if n == 0 {
            return Ok(0);
        }
        self.mixer.process(&mut buf[..n * self.channels], self.channels);
        self.remaining -= n;
        self.progress.store(1.0 - self.remaining as f32 / self.total.max(1) as f32);
        Ok(n)
    }

    pub fn total_frames(&self) -> usize {
        self.total
    }
}

fn configure(kind: ExportKind, shared: &EngineShared) {
    let (l, r, ltc) = match kind {
        ExportKind::LtcOnly => (NO_CHANNEL, NO_CHANNEL, 0),
        ExportKind::MonoMixLtc => (0, NO_CHANNEL, 1),
        ExportKind::StereoMixLtc => (0, 1, 2),
        ExportKind::StereoMix => (0, 1, NO_CHANNEL),
    };
    shared.main_left.store(l, Ordering::Relaxed);
    shared.main_right.store(r, Ordering::Relaxed);
    shared.ltc_channel.store(ltc, Ordering::Relaxed);
    shared.ltc_enabled.store(ltc != NO_CHANNEL, Ordering::Relaxed);
}

pub fn run(job: ExportJob, progress: &AtomicF32, cancel: &AtomicBool) -> Result<(), String> {
    job.encoding.validate(job.kind.channels())?;
    let sr = job.encoding.sample_rate.unwrap_or(job.engine_rate);
    let shared = EngineShared::new();
    configure(job.kind, &shared);
    shared.ltc_gain.store(job.ltc_gain);
    shared.master_gain.store(job.master_gain);

    // Bring every clip to the target rate (from the original file when known).
    let mut tracks = Vec::with_capacity(job.tracks.len());
    for t in &job.tracks {
        let data = if sr == job.engine_rate {
            t.clip.clone()
        } else if let Some(src) = &t.source {
            Arc::new(ClipData { channels: resample(&src.channels, src.sample_rate, sr)? })
        } else {
            Arc::new(ClipData { channels: resample(&t.clip.channels, job.engine_rate, sr)? })
        };
        let p = TrackParams::new(t.gain, (t.offset_secs * sr as f64).round() as i64);
        p.pan.store(t.pan);
        p.mute.store(t.mute, Ordering::Relaxed);
        p.solo.store(t.solo, Ordering::Relaxed);
        tracks.push(RtTrack { data, params: Arc::new(p) });
        if cancel.load(Ordering::Relaxed) {
            return Err("export cancelled".into());
        }
    }

    let start = (job.start_secs * sr as f64).round() as i64;
    let end = (job.end_secs * sr as f64).round() as i64;
    let (mut tx, rx) = RingBuffer::new(8);
    let (gb_tx, _gb_rx) = RingBuffer::new(8);
    let mut mixer = Mixer::new(shared, rx, gb_tx, sr, start);
    let _ = tx.push(Command::SetTimecode { rate: job.rate, start_frames: job.start_frames, user_bits: job.user_bits });
    let _ = tx.push(Command::SetTracks(tracks));
    let _ = tx.push(Command::Play);
    // Apply the commands before the first rendered block.
    mixer.process(&mut [], job.kind.channels());

    let total = (end - start).max(0) as usize;
    let mut renderer = Renderer {
        mixer,
        channels: job.kind.channels(),
        sample_rate: sr,
        has_ltc: job.kind.has_ltc(),
        remaining: total,
        total,
        progress,
        cancel,
    };
    encode::write_file(&job.path, &job.encoding, &mut renderer)
}

#[cfg(test)]
mod tests {
    use super::encode::Depth;
    use super::*;
    use crate::audio::decode::decode_file;
    use cueline_core::ltc::LtcDecoder;
    use cueline_core::Timecode;

    fn job(path: PathBuf, encoding: Encoding, kind: ExportKind) -> ExportJob {
        let rate = FrameRate::Fps25;
        // A one-second 440 Hz tone at 48 kHz, starting at 0.5 s.
        let tone: Vec<f32> =
            (0..48_000).map(|i| (i as f32 * 440.0 / 48_000.0 * std::f32::consts::TAU).sin() * 0.5).collect();
        ExportJob {
            kind,
            path,
            encoding,
            engine_rate: 48_000,
            start_secs: 0.0,
            end_secs: 2.0,
            rate,
            start_frames: Timecode::new(10, 0, 0, 0).to_frames(rate),
            user_bits: 0,
            ltc_gain: 0.5,
            master_gain: 1.0,
            tracks: vec![ExportTrack {
                clip: Arc::new(ClipData { channels: vec![tone] }),
                source: None,
                gain: 1.0,
                pan: 0.0,
                mute: false,
                solo: false,
                offset_secs: 0.5,
            }],
        }
    }

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cueline-export-{}-{name}", std::process::id()))
    }

    fn export(name: &str, encoding: Encoding, kind: ExportKind) -> PathBuf {
        let path = tmp(name);
        run(job(path.clone(), encoding, kind), &AtomicF32::new(0.0), &AtomicBool::new(false)).unwrap();
        path
    }

    fn first_ltc(channel: &[f32], sr: u32) -> Option<Timecode> {
        let mut dec = LtcDecoder::new(sr);
        let mut first = None;
        dec.feed(channel, |f| {
            first.get_or_insert(f.frame.timecode());
        });
        first
    }

    #[test]
    fn lossless_formats_keep_ltc() {
        let cases = [
            ("a.wav", Encoding::new(Codec::Wav).depth(Depth::I16)),
            ("b.wav", Encoding::new(Codec::Wav).depth(Depth::I24)),
            ("c.wav", Encoding::new(Codec::Wav).depth(Depth::F32)),
            ("d.aiff", Encoding::new(Codec::Aiff).depth(Depth::I24)),
            ("e.aiff", Encoding::new(Codec::Aiff).depth(Depth::I16)),
            ("f.flac", Encoding::new(Codec::Flac).depth(Depth::I16)),
            ("g.flac", Encoding::new(Codec::Flac).depth(Depth::I24)),
        ];
        for (name, enc) in cases {
            let path = export(name, enc, ExportKind::MonoMixLtc);
            let a = decode_file(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(a.sample_rate, 48_000, "{name}");
            assert_eq!(a.channels.len(), 2, "{name}");
            assert!((a.frames() as i64 - 96_000).abs() <= 1, "{name}: {} frames", a.frames());
            assert_eq!(first_ltc(&a.channels[1], 48_000), Some(Timecode::new(10, 0, 0, 0)), "{name}");
            // The tone starts exactly at 0.5 s on the left channel.
            assert_eq!(a.channels[0][23_999], 0.0, "{name}");
            assert!(a.channels[0][24_000..24_100].iter().any(|s| s.abs() > 0.1), "{name}");
            if name.ends_with(".flac") {
                // Even uncompressed (24-bit LTC case) FLAC stays near raw size.
                let size = std::fs::metadata(&path).unwrap().len();
                assert!(size < 96_000 * 2 * 3 + 64 * 1024, "{name}: {size} bytes");
            }
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn lossy_formats_decode_with_the_right_length() {
        for (name, enc) in [("h.mp3", Encoding::new(Codec::Mp3)), ("i.ogg", Encoding::new(Codec::Vorbis))] {
            let path = export(name, enc, ExportKind::StereoMix);
            let a = decode_file(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(a.channels.len(), 2, "{name}");
            let secs = a.duration_secs();
            assert!((secs - 2.0).abs() < 0.1, "{name}: {secs} s");
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn ffmpeg_formats_roundtrip_when_available() {
        if crate::audio::ffmpeg::locate().is_none() {
            eprintln!("ffmpeg not installed: skipping");
            return;
        }
        for (name, codec) in [("l.opus", Codec::Opus), ("m.m4a", Codec::Aac)] {
            let path = export(name, Encoding::new(codec), ExportKind::StereoMix);
            // Opus is not decoded natively: this also exercises the ffmpeg import path.
            let a = decode_file(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!((a.duration_secs() - 2.0).abs() < 0.1, "{name}: {} s", a.duration_secs());
            std::fs::remove_file(&path).ok();
        }
    }

    #[test]
    fn other_sample_rates_synthesise_ltc_natively() {
        let enc = Encoding::new(Codec::Wav).depth(Depth::I24).sample_rate(Some(44_100));
        let path = export("j.wav", enc, ExportKind::MonoMixLtc);
        let a = decode_file(&path).unwrap();
        assert_eq!(a.sample_rate, 44_100);
        assert!((a.frames() as i64 - 88_200).abs() <= 1);
        assert_eq!(first_ltc(&a.channels[1], 44_100), Some(Timecode::new(10, 0, 0, 0)));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn invalid_combinations_are_rejected() {
        assert!(Encoding::new(Codec::Mp3).validate(3).is_err());
        assert!(Encoding::new(Codec::Aiff).depth(Depth::F32).validate(2).is_err());
        assert!(Encoding::new(Codec::Mp3).sample_rate(Some(96_000)).validate(2).is_err());
        assert!(Encoding::new(Codec::Wav).depth(Depth::F32).validate(3).is_ok());
    }

    #[test]
    fn cancelling_leaves_no_file() {
        let path = tmp("k.wav");
        let err = run(
            job(path.clone(), Encoding::new(Codec::Wav), ExportKind::LtcOnly),
            &AtomicF32::new(0.0),
            &AtomicBool::new(true),
        );
        assert!(err.is_err());
        assert!(!path.exists());
    }
}
