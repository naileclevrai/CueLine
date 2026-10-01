//! MIDI Timecode (MTC) message construction and scheduling math.
//!
//! A full timecode is spread over eight quarter-frame messages, i.e. two
//! frames. CueLine aligns piece 0 on even label frames, and each sequence
//! carries the label of the frame where its piece 0 was sent, as the MIDI
//! spec requires.

use crate::{FrameRate, Timecode};

/// Two-byte quarter-frame message (`F1 0nnn dddd`).
pub fn quarter_frame(piece: u8, tc: Timecode, rate: FrameRate) -> [u8; 2] {
    let nibble = match piece & 7 {
        0 => tc.frames & 0x0F,
        1 => (tc.frames >> 4) & 0x01,
        2 => tc.seconds & 0x0F,
        3 => (tc.seconds >> 4) & 0x03,
        4 => tc.minutes & 0x0F,
        5 => (tc.minutes >> 4) & 0x03,
        6 => tc.hours & 0x0F,
        _ => ((tc.hours >> 4) & 0x01) | (rate.mtc_code() << 1),
    };
    [0xF1, ((piece & 7) << 4) | nibble]
}

/// Full-frame SysEx (`F0 7F 7F 01 01 hh mm ss ff F7`), sent on locate.
pub fn full_frame(tc: Timecode, rate: FrameRate) -> [u8; 10] {
    [
        0xF0,
        0x7F,
        0x7F,
        0x01,
        0x01,
        (rate.mtc_code() << 5) | (tc.hours & 0x1F),
        tc.minutes,
        tc.seconds,
        tc.frames,
        0xF7,
    ]
}

/// Converts quarter-frame indices on the timeline to MTC messages and times.
#[derive(Clone, Copy, Debug)]
pub struct MtcSequencer {
    pub rate: FrameRate,
    /// Label frame count at timeline frame 0.
    pub start_frames: i64,
}

impl MtcSequencer {
    /// The quarter-frame index whose message is due at or after `sample`.
    pub fn next_quarter_at(&self, sample: i64, sample_rate: u32) -> i64 {
        let (n, d) = self.rate.ratio();
        let num = sample as i128 * n as i128 * 4;
        let den = sample_rate as i128 * d as i128;
        (-((-num).div_euclid(den))) as i64
    }

    /// Timeline position (fractional samples) of quarter-frame `q`.
    pub fn quarter_sample(&self, q: i64, sample_rate: u32) -> f64 {
        let (n, d) = self.rate.ratio();
        q as f64 * sample_rate as f64 * d as f64 / (4.0 * n as f64)
    }

    /// The message for quarter-frame `q`.
    pub fn message(&self, q: i64) -> [u8; 2] {
        let frame = self.start_frames + q.div_euclid(4);
        let odd = frame.rem_euclid(2);
        let piece = (odd * 4 + q.rem_euclid(4)) as u8;
        let tc = Timecode::from_frames(frame - odd, self.rate);
        quarter_frame(piece, tc, self.rate)
    }

    pub fn label_at_frame(&self, frame: i64) -> Timecode {
        Timecode::from_frames(self.start_frames + frame, self.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reassembles a timecode from eight quarter frames, like a receiver.
    fn assemble(msgs: &[[u8; 2]]) -> (Timecode, u8) {
        let mut n = [0u8; 8];
        for m in msgs {
            n[(m[1] >> 4) as usize] = m[1] & 0x0F;
        }
        let tc = Timecode {
            frames: n[0] | (n[1] << 4),
            seconds: n[2] | (n[3] << 4),
            minutes: n[4] | (n[5] << 4),
            hours: n[6] | ((n[7] & 1) << 4),
        };
        (tc, n[7] >> 1)
    }

    #[test]
    fn quarter_frames_reassemble() {
        for rate in FrameRate::ALL {
            let seq = MtcSequencer { rate, start_frames: Timecode::new(10, 59, 59, 0).to_frames(rate) };
            let first = seq.start_frames.rem_euclid(2); // first even label frame
            for frame in (first..200).step_by(2) {
                let msgs: Vec<_> = (0..8).map(|i| seq.message(frame * 4 + i)).collect();
                let pieces: Vec<u8> = msgs.iter().map(|m| m[1] >> 4).collect();
                assert_eq!(pieces, [0, 1, 2, 3, 4, 5, 6, 7]);
                let (tc, code) = assemble(&msgs);
                assert_eq!(tc, seq.label_at_frame(frame));
                assert_eq!(code, rate.mtc_code());
            }
        }
    }

    #[test]
    fn odd_start_frame_still_aligns_piece_zero_on_even_labels() {
        let seq = MtcSequencer { rate: FrameRate::Fps25, start_frames: 1 };
        // Timeline frame 0 is label frame 1 (odd) so it carries pieces 4..7.
        assert_eq!(seq.message(0)[1] >> 4, 4);
        assert_eq!(seq.message(4)[1] >> 4, 0);
    }

    #[test]
    fn full_frame_layout() {
        let m = full_frame(Timecode::new(1, 2, 3, 4), FrameRate::Fps29_97Df);
        assert_eq!(m, [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x41, 2, 3, 4, 0xF7]);
    }

    #[test]
    fn quarter_timing() {
        let seq = MtcSequencer { rate: FrameRate::Fps25, start_frames: 0 };
        assert_eq!(seq.quarter_sample(4, 48_000), 1920.0);
        assert_eq!(seq.next_quarter_at(0, 48_000), 0);
        assert_eq!(seq.next_quarter_at(1, 48_000), 1);
        assert_eq!(seq.next_quarter_at(480, 48_000), 1);
        assert_eq!(seq.next_quarter_at(481, 48_000), 2);
    }
}
