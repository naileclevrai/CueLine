//! Offline rendering to WAV.
//!
//! The export drives the very same [`Mixer`] used for live playback, so the
//! file is bit-identical to what the audio interface receives.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use cueline_core::FrameRate;
use rtrb::RingBuffer;

use crate::engine::atomic::AtomicF32;
use crate::engine::mixer::Mixer;
use crate::engine::shared::{Command, EngineShared, RtTrack, NO_CHANNEL};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    /// Mono file containing only the LTC signal.
    LtcOnly,
    /// Stereo: program mixed to mono on the left, LTC on the right.
    MonoMixLtc,
    /// Three channels: stereo program then LTC.
    StereoMixLtc,
}

impl ExportKind {
    pub const ALL: [ExportKind; 3] = [ExportKind::LtcOnly, ExportKind::MonoMixLtc, ExportKind::StereoMixLtc];

    pub fn label(self) -> &'static str {
        match self {
            ExportKind::LtcOnly => "LTC only (mono)",
            ExportKind::MonoMixLtc => "Mix L + LTC R (stereo)",
            ExportKind::StereoMixLtc => "Stereo mix + LTC (3 ch)",
        }
    }
}

pub struct ExportJob {
    pub kind: ExportKind,
    pub path: PathBuf,
    pub sample_rate: u32,
    /// Timeline sample range to render.
    pub start: i64,
    pub end: i64,
    pub rate: FrameRate,
    pub start_frames: i64,
    pub user_bits: u32,
    pub ltc_gain: f32,
    pub master_gain: f32,
    /// Snapshot copies of the tracks (parameters are not shared with the UI).
    pub tracks: Vec<RtTrack>,
}

pub fn run(job: ExportJob, progress: &AtomicF32, cancel: &AtomicBool) -> Result<(), String> {
    let shared = EngineShared::new();
    let channels: usize = match job.kind {
        ExportKind::LtcOnly => {
            shared.main_left.store(NO_CHANNEL, Ordering::Relaxed);
            shared.ltc_channel.store(0, Ordering::Relaxed);
            1
        }
        ExportKind::MonoMixLtc => {
            shared.main_left.store(0, Ordering::Relaxed);
            shared.main_right.store(NO_CHANNEL, Ordering::Relaxed);
            shared.ltc_channel.store(1, Ordering::Relaxed);
            2
        }
        ExportKind::StereoMixLtc => {
            shared.main_left.store(0, Ordering::Relaxed);
            shared.main_right.store(1, Ordering::Relaxed);
            shared.ltc_channel.store(2, Ordering::Relaxed);
            3
        }
    };
    shared.ltc_enabled.store(true, Ordering::Relaxed);
    shared.ltc_gain.store(job.ltc_gain);
    shared.master_gain.store(job.master_gain);

    let (mut tx, rx) = RingBuffer::new(8);
    let (gb_tx, _gb_rx) = RingBuffer::new(8);
    let mut mixer = Mixer::new(shared, rx, gb_tx, job.sample_rate, job.start);
    let _ = tx.push(Command::SetTimecode { rate: job.rate, start_frames: job.start_frames, user_bits: job.user_bits });
    let _ = tx.push(Command::SetTracks(Box::new(job.tracks)));
    let _ = tx.push(Command::Play);

    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: job.sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let tmp = job.path.with_extension("wav.part");
    let mut writer = hound::WavWriter::create(&tmp, spec).map_err(|e| e.to_string())?;
    let total = (job.end - job.start).max(0) as usize;
    let mut buf = vec![0.0f32; 4096 * channels];
    let mut done = 0usize;
    while done < total {
        if cancel.load(Ordering::Relaxed) {
            drop(writer);
            let _ = std::fs::remove_file(&tmp);
            return Err("export cancelled".into());
        }
        let n = (total - done).min(4096);
        let chunk = &mut buf[..n * channels];
        mixer.process(chunk, channels);
        for &s in chunk.iter() {
            let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
            writer.write_sample(v).map_err(|e| e.to_string())?;
        }
        done += n;
        progress.store(done as f32 / total as f32);
    }
    writer.finalize().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &job.path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cueline_core::ltc::LtcDecoder;
    use cueline_core::Timecode;

    #[test]
    fn exported_ltc_decodes_from_file() {
        let path = std::env::temp_dir().join(format!("cueline-export-{}.wav", std::process::id()));
        let rate = FrameRate::Fps25;
        let job = ExportJob {
            kind: ExportKind::MonoMixLtc,
            path: path.clone(),
            sample_rate: 48_000,
            start: 0,
            end: 48_000 * 2,
            rate,
            start_frames: Timecode::new(10, 0, 0, 0).to_frames(rate),
            user_bits: 0,
            ltc_gain: 0.5,
            master_gain: 1.0,
            tracks: Vec::new(),
        };
        run(job, &AtomicF32::new(0.0), &AtomicBool::new(false)).unwrap();

        let mut r = hound::WavReader::open(&path).unwrap();
        assert_eq!(r.spec().channels, 2);
        let right: Vec<f32> = r
            .samples::<i32>()
            .skip(1)
            .step_by(2)
            .map(|s| s.unwrap() as f32 / 8_388_607.0)
            .collect();
        assert_eq!(right.len(), 96_000);
        let mut dec = LtcDecoder::new(48_000);
        let mut first = None;
        dec.feed(&right, |f| {
            first.get_or_insert(f.frame.timecode());
        });
        assert_eq!(first, Some(Timecode::new(10, 0, 0, 0)));
        std::fs::remove_file(&path).ok();
    }
}
