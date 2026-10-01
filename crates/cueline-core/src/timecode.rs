use crate::FrameRate;
use serde::{Deserialize, Serialize};
use std::fmt;

/// An SMPTE timecode label `HH:MM:SS:FF`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timecode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
}

impl Timecode {
    pub const fn new(hours: u8, minutes: u8, seconds: u8, frames: u8) -> Self {
        Self { hours, minutes, seconds, frames }
    }

    /// Converts a frame count (since 00:00:00:00) to a label, wrapping at 24h.
    pub fn from_frames(count: i64, rate: FrameRate) -> Self {
        let mut n = count.rem_euclid(rate.frames_per_day());
        if rate.is_drop() {
            // Re-insert the dropped labels (2 per minute except every 10th).
            let tens = n / 17_982;
            let rem = n % 17_982;
            n += 18 * tens + if rem > 1 { 2 * ((rem - 2) / 1798) } else { 0 };
        }
        let fps = rate.nominal() as i64;
        Self {
            frames: (n % fps) as u8,
            seconds: ((n / fps) % 60) as u8,
            minutes: ((n / (fps * 60)) % 60) as u8,
            hours: ((n / (fps * 3600)) % 24) as u8,
        }
    }

    /// Converts the label to a frame count since 00:00:00:00.
    pub fn to_frames(self, rate: FrameRate) -> i64 {
        let fps = rate.nominal() as i64;
        let secs = self.hours as i64 * 3600 + self.minutes as i64 * 60 + self.seconds as i64;
        let mut n = secs * fps + self.frames as i64;
        if rate.is_drop() {
            let total_minutes = self.hours as i64 * 60 + self.minutes as i64;
            n -= 2 * (total_minutes - total_minutes / 10);
        }
        n
    }

    /// Whether this label exists at the given rate.
    pub fn is_valid(self, rate: FrameRate) -> bool {
        if self.hours > 23 || self.minutes > 59 || self.seconds > 59 {
            return false;
        }
        if self.frames as u32 >= rate.nominal() {
            return false;
        }
        // Labels ;00 and ;01 do not exist at the start of non-tenth minutes.
        !(rate.is_drop() && self.seconds == 0 && self.frames < 2 && self.minutes % 10 != 0)
    }

    /// Parses `HH:MM:SS:FF` (any of `:;.,` as separators). Shorter inputs are
    /// right-aligned, so `"1:00"` means one second and `"10:00:00"` is
    /// 10 minutes.
    pub fn parse(text: &str, rate: FrameRate) -> Option<Self> {
        let mut parts = [0u8; 4];
        let fields: Vec<&str> = text
            .trim()
            .split(|c| matches!(c, ':' | ';' | '.' | ','))
            .collect();
        if fields.is_empty() || fields.len() > 4 {
            return None;
        }
        let offset = 4 - fields.len();
        for (i, f) in fields.iter().enumerate() {
            let f = f.trim();
            if f.is_empty() || f.len() > 2 || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            parts[offset + i] = f.parse().ok()?;
        }
        let tc = Self::new(parts[0], parts[1], parts[2], parts[3]);
        tc.is_valid(rate).then_some(tc)
    }

    /// Formats with the conventional `;` separator before frames for drop-frame.
    pub fn display(self, rate: FrameRate) -> TimecodeDisplay {
        TimecodeDisplay { tc: self, drop: rate.is_drop() }
    }
}

pub struct TimecodeDisplay {
    tc: Timecode,
    drop: bool,
}

impl fmt::Display for TimecodeDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = self.tc;
        let sep = if self.drop { ';' } else { ':' };
        write!(f, "{:02}:{:02}:{:02}{}{:02}", t.hours, t.minutes, t.seconds, sep, t.frames)
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}:{:02}:{:02}", self.hours, self.minutes, self.seconds, self.frames)
    }
}
