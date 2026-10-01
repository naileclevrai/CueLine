//! The real-time render loop. Runs inside the audio callback: it never
//! allocates, locks or blocks.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use cueline_core::ltc::LtcGenerator;
use cueline_core::FrameRate;
use rtrb::{Consumer, Producer};

use super::shared::{Command, EngineShared, RtTrack};

const BLOCK: usize = 512;
/// Length of the fade applied when pausing, to avoid a click.
const FADE: usize = 256;

pub struct Mixer {
    shared: Arc<EngineShared>,
    commands: Consumer<Command>,
    /// Track lists replaced by `SetTracks` go back to the UI thread to be freed.
    garbage: Producer<Vec<RtTrack>>,
    tracks: Vec<RtTrack>,
    ltc: LtcGenerator,
    position: i64,
    playing: bool,
    fade_out: usize,
    ltc_buf: Box<[f32; BLOCK]>,
    mix: Box<[[f32; 2]; BLOCK]>,
}

impl Mixer {
    pub fn new(
        shared: Arc<EngineShared>,
        commands: Consumer<Command>,
        garbage: Producer<Vec<RtTrack>>,
        sample_rate: u32,
        position: i64,
    ) -> Self {
        Self {
            shared,
            commands,
            garbage,
            tracks: Vec::new(),
            ltc: LtcGenerator::new(sample_rate, FrameRate::default()),
            position,
            playing: false,
            fade_out: 0,
            ltc_buf: Box::new([0.0; BLOCK]),
            mix: Box::new([[0.0; 2]; BLOCK]),
        }
    }

    pub fn position(&self) -> i64 {
        self.position
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    fn handle_commands(&mut self) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                Command::Play => {
                    self.playing = true;
                    self.fade_out = 0;
                }
                Command::Pause => {
                    if self.playing && self.fade_out == 0 {
                        self.fade_out = FADE;
                    }
                }
                Command::Seek(pos) => self.position = pos,
                Command::SetTracks(tracks) => {
                    let old = std::mem::replace(&mut self.tracks, tracks);
                    // If the UI is not draining (should never happen) we leak
                    // rather than free memory on the audio thread.
                    if let Err(rtrb::PushError::Full(old)) = self.garbage.push(old) {
                        std::mem::forget(old);
                    }
                }
                Command::SetTimecode { rate, start_frames, user_bits } => {
                    self.ltc.set_rate(rate);
                    self.ltc.set_start_frames(start_frames);
                    self.ltc.set_user_bits(user_bits);
                }
            }
        }
    }

    /// Renders an interleaved buffer with `channels` channels.
    pub fn process(&mut self, out: &mut [f32], channels: usize) {
        self.handle_commands();
        out.fill(0.0);
        if channels == 0 {
            return;
        }
        let frames = out.len() / channels;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(BLOCK);
            let chunk = &mut out[done * channels..(done + n) * channels];
            self.render_block(chunk, channels, n);
            done += n;
        }
    }

    fn render_block(&mut self, out: &mut [f32], channels: usize, n: usize) {
        let sh = &*self.shared;
        let ltc_ch = EngineShared::channel(&sh.ltc_channel).filter(|&c| c < channels);
        let ltc_on = sh.ltc_enabled.load(Ordering::Relaxed) && ltc_ch.is_some();

        if !self.playing {
            if ltc_on {
                self.ltc.render_silence(&mut self.ltc_buf[..n]);
                write_channel(out, channels, ltc_ch.unwrap(), &self.ltc_buf[..n]);
            }
            return;
        }

        // Program mix.
        let mix = &mut self.mix[..n];
        mix.fill([0.0; 2]);
        let any_solo = self.tracks.iter().any(|t| t.params.solo.load(Ordering::Relaxed));
        for t in self.tracks.iter() {
            let p = &*t.params;
            if p.mute.load(Ordering::Relaxed) || (any_solo && !p.solo.load(Ordering::Relaxed)) {
                continue;
            }
            let len = t.data.frames() as i64;
            let start = self.position - p.offset.load(Ordering::Relaxed);
            let a = start.max(0);
            let b = (start + n as i64).min(len);
            if a >= b {
                continue;
            }
            let gain = p.gain.load();
            let pan = p.pan.load().clamp(-1.0, 1.0);
            let (gl, gr) = (gain * (1.0 - pan.max(0.0)), gain * (1.0 + pan.min(0.0)));
            let dst = &mut mix[(a - start) as usize..(b - start) as usize];
            let (a, b) = (a as usize, b as usize);
            let left = &t.data.channels[0][a..b];
            let right = &t.data.channels[t.data.channels.len() - 1][a..b];
            let mut peak = 0.0f32;
            for ((d, &l), &r) in dst.iter_mut().zip(left).zip(right) {
                let (l, r) = (l * gl, r * gr);
                d[0] += l;
                d[1] += r;
                peak = peak.max(l.abs()).max(r.abs());
            }
            p.peak.fetch_max(peak);
        }

        let master = sh.master_gain.load();
        let fade_start = self.fade_out;
        let mut peaks = [0.0f32; 2];
        for (i, m) in mix.iter_mut().enumerate() {
            let mut g = master;
            if fade_start > 0 {
                g *= (fade_start.saturating_sub(i)) as f32 / FADE as f32;
            }
            m[0] *= g;
            m[1] *= g;
            peaks[0] = peaks[0].max(m[0].abs());
            peaks[1] = peaks[1].max(m[1].abs());
        }
        sh.master_peak[0].fetch_max(peaks[0]);
        sh.master_peak[1].fetch_max(peaks[1]);

        let l = EngineShared::channel(&sh.main_left).filter(|&c| c < channels);
        let r = EngineShared::channel(&sh.main_right).filter(|&c| c < channels);
        match (l, r) {
            (Some(l), Some(r)) if l != r => {
                for (frame, m) in out.chunks_exact_mut(channels).zip(mix.iter()) {
                    frame[l] += m[0];
                    frame[r] += m[1];
                }
            }
            (Some(c), _) | (None, Some(c)) => {
                for (frame, m) in out.chunks_exact_mut(channels).zip(mix.iter()) {
                    frame[c] += 0.5 * (m[0] + m[1]);
                }
            }
            (None, None) => {}
        }

        // Timecode. LTC is cut on the first sample of the fade so a reader
        // never sees a frame that the program audio does not reach.
        if ltc_on {
            self.ltc.set_amplitude(sh.ltc_gain.load());
            if fade_start > 0 {
                self.ltc.render_silence(&mut self.ltc_buf[..n]);
            } else {
                self.ltc.render(self.position, &mut self.ltc_buf[..n]);
            }
            let peak = self.ltc_buf[..n].iter().fold(0.0f32, |a, s| a.max(s.abs()));
            sh.ltc_peak.fetch_max(peak);
            write_channel(out, channels, ltc_ch.unwrap(), &self.ltc_buf[..n]);
        }

        if fade_start > 0 {
            let used = n.min(fade_start);
            self.position += used as i64;
            self.fade_out -= used;
            if self.fade_out == 0 {
                self.playing = false;
            }
        } else {
            self.position += n as i64;
        }
    }
}

fn write_channel(out: &mut [f32], channels: usize, ch: usize, src: &[f32]) {
    for (frame, &s) in out.chunks_exact_mut(channels).zip(src) {
        frame[ch] += s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::shared::{ClipData, TrackParams, NO_CHANNEL};
    use cueline_core::ltc::LtcDecoder;
    use cueline_core::Timecode;
    use rtrb::RingBuffer;

    fn setup() -> (Mixer, Producer<Command>, Arc<EngineShared>) {
        let shared = EngineShared::new();
        let (cmd_tx, cmd_rx) = RingBuffer::new(64);
        let (gb_tx, _gb_rx) = RingBuffer::new(8);
        let m = Mixer::new(shared.clone(), cmd_rx, gb_tx, 48_000, 0);
        (m, cmd_tx, shared)
    }

    fn track(value: f32, frames: usize, offset: i64) -> RtTrack {
        RtTrack {
            data: Arc::new(ClipData { channels: vec![vec![value; frames]] }),
            params: Arc::new(TrackParams::new(1.0, offset)),
        }
    }

    #[test]
    fn silent_when_stopped() {
        let (mut m, _tx, _) = setup();
        let mut out = vec![1.0; 1024];
        m.process(&mut out, 2);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn clip_offset_is_sample_exact() {
        let (mut m, mut tx, sh) = setup();
        sh.ltc_enabled.store(false, Ordering::Relaxed);
        sh.main_right.store(1, Ordering::Relaxed);
        tx.push(Command::SetTracks(vec![track(0.5, 100, 1000)])).ok();
        tx.push(Command::Play).ok();
        let mut out = vec![0.0; 2 * 2048];
        m.process(&mut out, 2);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left[999], 0.0);
        assert_eq!(left[1000], 0.5);
        assert_eq!(left[1099], 0.5);
        assert_eq!(left[1100], 0.0);
        assert_eq!(m.position(), 2048);
    }

    #[test]
    fn solo_and_mute() {
        let (mut m, mut tx, sh) = setup();
        sh.ltc_enabled.store(false, Ordering::Relaxed);
        let a = track(0.25, 4096, 0);
        let b = track(0.5, 4096, 0);
        b.params.solo.store(true, Ordering::Relaxed);
        tx.push(Command::SetTracks(vec![a.clone(), b.clone()])).ok();
        tx.push(Command::Play).ok();
        let mut out = vec![0.0; 2 * 64];
        m.process(&mut out, 2);
        assert_eq!(out[0], 0.5); // only the soloed track, mono-summed on channel 0
        b.params.mute.store(true, Ordering::Relaxed);
        m.process(&mut out, 2);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn ltc_on_its_own_channel_decodes() {
        let (mut m, mut tx, sh) = setup();
        sh.main_left.store(0, Ordering::Relaxed);
        sh.main_right.store(NO_CHANNEL, Ordering::Relaxed);
        sh.ltc_channel.store(2, Ordering::Relaxed);
        let start = Timecode::new(1, 0, 0, 0).to_frames(FrameRate::Fps25);
        tx.push(Command::SetTimecode { rate: FrameRate::Fps25, start_frames: start, user_bits: 0 }).ok();
        tx.push(Command::Play).ok();
        let mut out = vec![0.0; 3 * 48_000];
        for c in out.chunks_mut(3 * 441) {
            m.process(c, 3);
        }
        let ltc: Vec<f32> = out.chunks(3).map(|f| f[2]).collect();
        assert!(out.chunks(3).all(|f| f[0] == 0.0 && f[1] == 0.0));
        let mut dec = LtcDecoder::new(48_000);
        let mut got = Vec::new();
        dec.feed(&ltc, |f| got.push(f.frame.timecode()));
        assert_eq!(got.first(), Some(&Timecode::new(1, 0, 0, 0)));
        // The 25th frame is only closed by the edge at sample 48 000.
        assert_eq!(got.len(), 24);
    }

    #[test]
    fn pause_fades_then_stops() {
        let (mut m, mut tx, sh) = setup();
        sh.ltc_enabled.store(false, Ordering::Relaxed);
        tx.push(Command::SetTracks(vec![track(1.0, 48_000, 0)])).ok();
        tx.push(Command::Play).ok();
        let mut out = vec![0.0; 2 * 128];
        m.process(&mut out, 2);
        tx.push(Command::Pause).ok();
        let mut out = vec![0.0; 2 * 1024];
        m.process(&mut out, 2);
        assert!(!m.is_playing());
        assert!(out[0] > 0.9 && out[2 * 300] == 0.0);
        assert_eq!(m.position(), 128 + FADE as i64);
    }
}
