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
