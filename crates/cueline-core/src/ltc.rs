//! SMPTE 12M Linear Timecode (LTC).
//!
//! An LTC frame is 80 bits, sent LSB first and biphase-mark encoded: the
//! signal toggles at every bit boundary, and a `1` adds a toggle mid-bit.
//! The polarity-correction bit keeps the number of `1`s even, so every
//! frame starts with the same level. This lets the generator compute the
//! level of any sample directly from its timeline position.

use crate::{FrameRate, Timecode};

/// Bits 64..=79 read as a little-endian `u16`.
pub const SYNC_WORD: u16 = 0xBFFC;

/// The 80 bits of an LTC frame; bit `i` of the frame is bit `i` of the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LtcFrame(pub u128);

impl LtcFrame {
    pub fn encode(tc: Timecode, rate: FrameRate, user_bits: u32) -> Self {
        let mut f = LtcFrame(0);
        f.put(0, 4, tc.frames % 10);
        f.put(8, 2, tc.frames / 10);
        f.put(10, 1, rate.is_drop() as u8);
        f.put(16, 4, tc.seconds % 10);
        f.put(24, 3, tc.seconds / 10);
        f.put(32, 4, tc.minutes % 10);
        f.put(40, 3, tc.minutes / 10);
        f.put(48, 4, tc.hours % 10);
        f.put(56, 2, tc.hours / 10);
        for (i, pos) in USER_BIT_GROUPS.iter().enumerate() {
            f.put(*pos, 4, ((user_bits >> (i * 4)) & 0xF) as u8);
        }
        f.0 |= (SYNC_WORD as u128) << 64;
        if f.0.count_ones() % 2 == 1 {
            f.0 |= 1u128 << polarity_bit(rate);
        }
        f
    }

    pub fn bit(self, i: usize) -> bool {
        (self.0 >> i) & 1 == 1
    }

    pub fn sync_ok(self) -> bool {
        (self.0 >> 64) as u16 == SYNC_WORD
    }

    pub fn timecode(self) -> Timecode {
        let g = |pos, len| self.get(pos, len);
        Timecode {
            frames: g(8, 2) * 10 + g(0, 4),
            seconds: g(24, 3) * 10 + g(16, 4),
            minutes: g(40, 3) * 10 + g(32, 4),
            hours: g(56, 2) * 10 + g(48, 4),
        }
    }

    pub fn drop_frame(self) -> bool {
        self.bit(10)
    }

    pub fn user_bits(self) -> u32 {
        USER_BIT_GROUPS
            .iter()
            .enumerate()
            .fold(0, |acc, (i, pos)| acc | (self.get(*pos, 4) as u32) << (i * 4))
    }

    fn put(&mut self, pos: usize, len: usize, value: u8) {
        let mask = (1u128 << len) - 1;
        self.0 |= (value as u128 & mask) << pos;
    }

    fn get(self, pos: usize, len: usize) -> u8 {
        ((self.0 >> pos) & ((1u128 << len) - 1)) as u8
    }
}

const USER_BIT_GROUPS: [usize; 8] = [4, 12, 20, 28, 36, 44, 52, 60];

/// The polarity-correction bit sits at 59 for 25 fps and 27 otherwise.
const fn polarity_bit(rate: FrameRate) -> usize {
    match rate {
        FrameRate::Fps25 => 59,
        _ => 27,
    }
}

/// Real-time LTC signal generator.
///
/// `render` maps every output sample to its timeline position, so the
/// generated signal is sample-accurate and stays correct across seeks,
/// loops and buffer-size changes. It never allocates.
pub struct LtcGenerator {
    sample_rate: u32,
    rate: FrameRate,
    /// Label frame count at timeline sample 0 (the project start timecode).
    start_frames: i64,
    user_bits: u32,
    amplitude: f32,
    /// One-pole low-pass coefficient giving the SMPTE ~25 µs rise time.
    alpha: f32,
    smoothed: f32,
    cached_frame: i64,
    levels: [bool; 160],
}

impl LtcGenerator {
    pub fn new(sample_rate: u32, rate: FrameRate) -> Self {
        let tau = 25e-6 / 2.197; // 10%-90% rise time of a one-pole filter
        let alpha = 1.0 - (-1.0 / (sample_rate as f64 * tau)).exp();
        Self {
            sample_rate,
            rate,
            start_frames: 0,
            user_bits: 0,
            amplitude: 0.25,
            alpha: alpha as f32,
            smoothed: 0.0,
            cached_frame: i64::MIN,
            levels: [false; 160],
        }
    }

    pub fn set_rate(&mut self, rate: FrameRate) {
        if rate != self.rate {
            self.rate = rate;
            self.cached_frame = i64::MIN;
        }
    }

    pub fn set_start_frames(&mut self, frames: i64) {
        if frames != self.start_frames {
            self.start_frames = frames;
            self.cached_frame = i64::MIN;
        }
    }

    pub fn set_user_bits(&mut self, bits: u32) {
        if bits != self.user_bits {
            self.user_bits = bits;
            self.cached_frame = i64::MIN;
        }
    }

    /// Peak amplitude, linear (1.0 = 0 dBFS).
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude;
    }

    pub fn rate(&self) -> FrameRate {
        self.rate
    }

    /// Timecode label carried by timeline frame `frame`.
    pub fn timecode_at_frame(&self, frame: i64) -> Timecode {
        Timecode::from_frames(self.start_frames + frame, self.rate)
    }

    /// Writes the LTC signal for timeline samples `start..start + out.len()`.
    pub fn render(&mut self, start: i64, out: &mut [f32]) {
        let (n, d) = self.rate.ratio();
        let den = self.sample_rate as i64 * d as i64;
        let mut num = start * n as i64;
        for o in out.iter_mut() {
            let frame = num.div_euclid(den);
            let half = ((num - frame * den) * 160 / den) as usize;
            if frame != self.cached_frame {
                self.load_frame(frame);
            }
            let target = if self.levels[half] { self.amplitude } else { -self.amplitude };
            self.smoothed += self.alpha * (target - self.smoothed);
            *o = self.smoothed;
            num += n as i64;
        }
    }

    /// Lets the output settle to silence instead of clicking when stopping.
    pub fn render_silence(&mut self, out: &mut [f32]) {
        for o in out.iter_mut() {
            self.smoothed -= self.alpha * self.smoothed;
            *o = self.smoothed;
        }
    }

    fn load_frame(&mut self, frame: i64) {
        let bits = LtcFrame::encode(self.timecode_at_frame(frame), self.rate, self.user_bits);
        let mut level = false;
        for b in 0..80 {
            level = !level;
            self.levels[2 * b] = level;
            if bits.bit(b) {
                level = !level;
            }
            self.levels[2 * b + 1] = level;
        }
        debug_assert!(!level, "polarity correction must restore the start level");
        self.cached_frame = frame;
    }
}
