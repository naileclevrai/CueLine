use serde::{Deserialize, Serialize};

/// SMPTE frame rates supported by CueLine.
///
/// The *nominal* rate is the number of frame labels per second (24, 25 or 30),
/// while the *real* rate is the actual number of frames per second, expressed
/// as an exact rational so that sample positions never drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FrameRate {
    #[serde(rename = "23.976")]
    Fps23_976,
    #[serde(rename = "24")]
    Fps24,
    #[default]
    #[serde(rename = "25")]
    Fps25,
    #[serde(rename = "29.97df")]
    Fps29_97Df,
    #[serde(rename = "29.97")]
    Fps29_97Ndf,
    #[serde(rename = "30")]
    Fps30,
}

impl FrameRate {
    pub const ALL: [FrameRate; 6] = [
        FrameRate::Fps23_976,
        FrameRate::Fps24,
        FrameRate::Fps25,
        FrameRate::Fps29_97Df,
        FrameRate::Fps29_97Ndf,
        FrameRate::Fps30,
    ];

    /// Number of frame labels per second.
    pub const fn nominal(self) -> u32 {
        match self {
            FrameRate::Fps23_976 | FrameRate::Fps24 => 24,
            FrameRate::Fps25 => 25,
            FrameRate::Fps29_97Df | FrameRate::Fps29_97Ndf | FrameRate::Fps30 => 30,
        }
    }

    /// Whether frame labels are dropped (29.97 drop-frame only).
    pub const fn is_drop(self) -> bool {
        matches!(self, FrameRate::Fps29_97Df)
    }

    /// Exact frame rate as `(numerator, denominator)` frames per second.
    pub const fn ratio(self) -> (u64, u64) {
        match self {
            FrameRate::Fps23_976 => (24_000, 1001),
            FrameRate::Fps24 => (24, 1),
            FrameRate::Fps25 => (25, 1),
            FrameRate::Fps29_97Df | FrameRate::Fps29_97Ndf => (30_000, 1001),
            FrameRate::Fps30 => (30, 1),
        }
    }

    /// Real frame rate in frames per second.
    pub fn fps(self) -> f64 {
        let (n, d) = self.ratio();
        n as f64 / d as f64
    }

    /// Number of frames in 24 hours of timecode labels.
    pub const fn frames_per_day(self) -> i64 {
        if self.is_drop() {
            // 30 fps minus 2 labels per minute, except every tenth minute.
            2_589_408
        } else {
            self.nominal() as i64 * 86_400
        }
    }

    /// Rate code used in MIDI Timecode (0 = 24, 1 = 25, 2 = 29.97 DF, 3 = 30).
    pub const fn mtc_code(self) -> u8 {
        match self {
            FrameRate::Fps23_976 | FrameRate::Fps24 => 0,
            FrameRate::Fps25 => 1,
            FrameRate::Fps29_97Df => 2,
            FrameRate::Fps29_97Ndf | FrameRate::Fps30 => 3,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            FrameRate::Fps23_976 => "23.976",
            FrameRate::Fps24 => "24",
            FrameRate::Fps25 => "25",
            FrameRate::Fps29_97Df => "29.97 DF",
            FrameRate::Fps29_97Ndf => "29.97 ND",
            FrameRate::Fps30 => "30",
        }
    }

    /// Index of the timeline frame containing `sample`, at `sample_rate` Hz.
    ///
    /// Uses exact integer arithmetic; negative samples floor towards -inf.
    pub fn frame_at_sample(self, sample: i64, sample_rate: u32) -> i64 {
        let (n, d) = self.ratio();
        let num = sample as i128 * n as i128;
        let den = sample_rate as i128 * d as i128;
        num.div_euclid(den) as i64
    }

    /// First sample of timeline frame `frame` (rounded up to the next sample).
    pub fn sample_at_frame(self, frame: i64, sample_rate: u32) -> i64 {
        let (n, d) = self.ratio();
        let num = frame as i128 * sample_rate as i128 * d as i128;
        let den = n as i128;
        // ceil division so that frame_at_sample(sample_at_frame(f)) == f
        (-((-num).div_euclid(den))) as i64
    }
}

impl std::fmt::Display for FrameRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_sample_roundtrip() {
        for rate in FrameRate::ALL {
            for sr in [44_100, 48_000, 96_000] {
                for f in [-5i64, 0, 1, 29, 1799, 107_892, 2_589_407] {
                    let s = rate.sample_at_frame(f, sr);
                    assert_eq!(rate.frame_at_sample(s, sr), f, "{rate} {sr} {f}");
                    assert_eq!(rate.frame_at_sample(s - 1, sr), f - 1, "{rate} {sr} {f}");
                }
            }
        }
    }

    #[test]
    fn ntsc_has_fractional_samples_per_frame() {
        let r = FrameRate::Fps29_97Df;
        // 1001 frames at 30000/1001 fps take exactly 1001 * 1601.6 samples at 48k.
        assert_eq!(r.sample_at_frame(1001 * 5, 48_000), 1601 * 5005 + 3003);
    }
}
