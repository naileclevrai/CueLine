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
        !(rate.is_drop() && self.seconds == 0 && self.frames < 2 && !self.minutes.is_multiple_of(10))
    }

    /// Parses `HH:MM:SS:FF` (any of `:;.,` as separators). Shorter inputs are
    /// right-aligned, so `"1:00"` means one second and `"10:00:00"` is
    /// 10 minutes.
    pub fn parse(text: &str, rate: FrameRate) -> Option<Self> {
        let mut parts = [0u8; 4];
        let fields: Vec<&str> = text
            .trim()
            .split([':', ';', '.', ','])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_drop_roundtrip() {
        for rate in [FrameRate::Fps24, FrameRate::Fps25, FrameRate::Fps30] {
            for n in (0..rate.frames_per_day()).step_by(997) {
                let tc = Timecode::from_frames(n, rate);
                assert!(tc.is_valid(rate));
                assert_eq!(tc.to_frames(rate), n);
            }
        }
    }

    #[test]
    fn drop_frame_roundtrip_every_frame_of_first_hour() {
        let rate = FrameRate::Fps29_97Df;
        let mut prev: Option<Timecode> = None;
        for n in 0..107_892 {
            let tc = Timecode::from_frames(n, rate);
            assert!(tc.is_valid(rate), "{tc} invalid at {n}");
            assert_eq!(tc.to_frames(rate), n);
            if let Some(p) = prev {
                assert_ne!(p, tc);
            }
            prev = Some(tc);
        }
        assert_eq!(Timecode::from_frames(107_892, rate), Timecode::new(1, 0, 0, 0));
    }

    #[test]
    fn drop_frame_skips_labels() {
        let r = FrameRate::Fps29_97Df;
        assert_eq!(Timecode::from_frames(1799, r), Timecode::new(0, 0, 59, 29));
        assert_eq!(Timecode::from_frames(1800, r), Timecode::new(0, 1, 0, 2));
        assert_eq!(Timecode::from_frames(17_982, r), Timecode::new(0, 10, 0, 0));
        assert!(!Timecode::new(0, 1, 0, 0).is_valid(r));
        assert!(Timecode::new(0, 10, 0, 0).is_valid(r));
    }

    #[test]
    fn wraps_at_24h() {
        let r = FrameRate::Fps25;
        assert_eq!(Timecode::from_frames(r.frames_per_day(), r), Timecode::default());
        assert_eq!(Timecode::from_frames(-1, r), Timecode::new(23, 59, 59, 24));
    }

    #[test]
    fn parse_and_display() {
        let r = FrameRate::Fps25;
        assert_eq!(Timecode::parse("01:02:03:04", r), Some(Timecode::new(1, 2, 3, 4)));
        assert_eq!(Timecode::parse("10:00:00", r), Some(Timecode::new(0, 10, 0, 0)));
        assert_eq!(Timecode::parse("1.12", r), Some(Timecode::new(0, 0, 1, 12)));
        assert_eq!(Timecode::parse("00:00:00:25", r), None);
        assert_eq!(Timecode::parse("abc", r), None);
        let df = FrameRate::Fps29_97Df;
        assert_eq!(Timecode::new(1, 2, 3, 4).display(df).to_string(), "01:02:03;04");
    }
}
