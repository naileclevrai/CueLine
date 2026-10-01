//! State shared between the UI thread and the real-time audio thread.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;

use cueline_core::FrameRate;

use super::atomic::AtomicF32;
use super::clock::SharedClock;

/// Sample data of one clip, already converted to the engine sample rate.
pub struct ClipData {
    pub channels: Vec<Vec<f32>>,
}

impl ClipData {
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }
}

/// Per-track parameters the UI may change at any time without locking.
#[derive(Debug)]
pub struct TrackParams {
    pub gain: AtomicF32,
    pub pan: AtomicF32,
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    /// Timeline sample where the clip starts.
    pub offset: AtomicI64,
    pub peak: AtomicF32,
}

impl TrackParams {
    pub fn new(gain: f32, offset: i64) -> Self {
        Self {
            gain: AtomicF32::new(gain),
            pan: AtomicF32::new(0.0),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            offset: AtomicI64::new(offset),
            peak: AtomicF32::new(0.0),
        }
    }
}

/// What the audio thread needs to play one track.
#[derive(Clone)]
pub struct RtTrack {
    pub data: Arc<ClipData>,
    pub params: Arc<TrackParams>,
}

pub const NO_CHANNEL: i32 = -1;

/// Output routing and global levels.
#[derive(Debug)]
pub struct EngineShared {
    pub sample_rate: AtomicU32,
    pub channels: AtomicU32,
    pub master_gain: AtomicF32,
    /// Output channels (0-based) for the program mix; `NO_CHANNEL` = unused.
    pub main_left: AtomicI32,
    pub main_right: AtomicI32,
    pub ltc_enabled: AtomicBool,
    pub ltc_gain: AtomicF32,
    pub ltc_channel: AtomicI32,
    pub master_peak: [AtomicF32; 2],
    pub ltc_peak: AtomicF32,
    /// Fraction of the buffer period spent rendering (0..1+).
    pub dsp_load: AtomicF32,
    pub buffer_frames: AtomicU32,
    pub latency_ns: AtomicU32,
    pub stream_error: AtomicBool,
    pub clock: SharedClock,
}

impl EngineShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sample_rate: AtomicU32::new(48_000),
            channels: AtomicU32::new(2),
            master_gain: AtomicF32::new(1.0),
            main_left: AtomicI32::new(0),
            main_right: AtomicI32::new(NO_CHANNEL),
            ltc_enabled: AtomicBool::new(true),
            ltc_gain: AtomicF32::new(0.25),
            ltc_channel: AtomicI32::new(1),
            master_peak: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            ltc_peak: AtomicF32::new(0.0),
            dsp_load: AtomicF32::new(0.0),
            buffer_frames: AtomicU32::new(0),
            latency_ns: AtomicU32::new(0),
            stream_error: AtomicBool::new(false),
            clock: SharedClock::default(),
        })
    }

    pub fn channel(a: &AtomicI32) -> Option<usize> {
        let v = a.load(Ordering::Relaxed);
        (v >= 0).then_some(v as usize)
    }
}

/// Messages from the UI to the audio thread.
pub enum Command {
    Play,
    Pause,
    Seek(i64),
    SetTracks(Vec<RtTrack>),
    SetTimecode { rate: FrameRate, start_frames: i64, user_bits: u32 },
}
