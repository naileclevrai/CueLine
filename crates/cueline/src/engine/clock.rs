//! Mapping between the audio stream and wall-clock time.
//!
//! Audio callbacks arrive with scheduling jitter. A second-order
//! delay-locked loop (F. Adriaensen, "Using a DLL to filter time", 2005)
//! smooths their timestamps into a stable estimate of *when* each sample
//! will leave the converter. The MTC thread and the UI read that estimate
//! through a seqlock to extrapolate the current position between callbacks.

use std::sync::atomic::{fence, AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use super::atomic::AtomicF64;

static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Monotonic nanoseconds since process start (QueryPerformanceCounter on Windows).
pub fn now_ns() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

/// Second-order DLL tracking seconds-per-sample and buffer start times.
pub struct Dll {
    bandwidth_hz: f64,
    t0: f64,
    sps: f64,
    nominal_sps: f64,
    ready: bool,
}

impl Dll {
    pub fn new(sample_rate: u32) -> Self {
        let sps = 1.0 / sample_rate as f64;
        Self { bandwidth_hz: 0.2, t0: 0.0, sps, nominal_sps: sps, ready: false }
    }

    /// Feeds the raw time (seconds) of a callback whose buffer starts
    /// `prev_frames` samples after the previous one. Returns the filtered
    /// time of this buffer's first sample.
    pub fn update(&mut self, raw: f64, prev_frames: u32) -> f64 {
        if !self.ready || prev_frames == 0 {
            self.t0 = raw;
            self.ready = true;
            return raw;
        }
        let n = prev_frames as f64;
        let predicted = self.t0 + n * self.sps;
        let err = raw - predicted;
        if err.abs() > 0.05 {
            // Device stall or discontinuity: start over.
            self.t0 = raw;
            self.sps = self.nominal_sps;
            return raw;
        }
        let omega = 2.0 * std::f64::consts::PI * self.bandwidth_hz * n * self.sps;
        self.t0 = predicted + std::f64::consts::SQRT_2 * omega * err;
        self.sps += omega * omega * err / n;
        // Hardware clocks never deviate by more than a few hundred ppm.
        self.sps = self.sps.clamp(self.nominal_sps * 0.995, self.nominal_sps * 1.005);
        self.t0
    }

    pub fn seconds_per_sample(&self) -> f64 {
        self.sps
    }
}

/// A consistent view of the transport, published once per audio callback.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClockSnapshot {
    /// Timeline sample at the start of the last rendered buffer.
    pub position: i64,
    /// Wall time (ns, see [`now_ns`]) at which that sample becomes audible.
    pub audible_ns: u64,
    pub ns_per_sample: f64,
    pub playing: bool,
}

impl ClockSnapshot {
    /// Timeline position (fractional samples) audible at wall time `ns`.
    pub fn position_at(&self, ns: u64) -> f64 {
        if !self.playing || self.ns_per_sample <= 0.0 {
            return self.position as f64;
        }
        self.position as f64 + (ns as f64 - self.audible_ns as f64) / self.ns_per_sample
    }

    /// Wall time (ns) at which timeline sample `pos` becomes audible.
    pub fn ns_at(&self, pos: f64) -> f64 {
        self.audible_ns as f64 + (pos - self.position as f64) * self.ns_per_sample
    }
}

/// Single-writer seqlock around a [`ClockSnapshot`].
#[derive(Debug, Default)]
pub struct SharedClock {
    seq: AtomicU64,
    position: AtomicI64,
    audible_ns: AtomicU64,
    ns_per_sample: AtomicF64,
    playing: AtomicBool,
}

impl SharedClock {
    /// Audio thread only.
    pub fn publish(&self, s: ClockSnapshot) {
        let seq = self.seq.load(Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.position.store(s.position, Ordering::Relaxed);
        self.audible_ns.store(s.audible_ns, Ordering::Relaxed);
        self.ns_per_sample.store(s.ns_per_sample);
        self.playing.store(s.playing, Ordering::Relaxed);
        self.seq.store(seq.wrapping_add(2), Ordering::Release);
    }

    pub fn read(&self) -> ClockSnapshot {
        loop {
            let a = self.seq.load(Ordering::Acquire);
            if a & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let s = ClockSnapshot {
                position: self.position.load(Ordering::Relaxed),
                audible_ns: self.audible_ns.load(Ordering::Relaxed),
                ns_per_sample: self.ns_per_sample.load(),
                playing: self.playing.load(Ordering::Relaxed),
            };
            fence(Ordering::Acquire);
            if self.seq.load(Ordering::Relaxed) == a {
                return s;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dll_filters_jitter() {
        let sr = 48_000;
        let mut dll = Dll::new(sr);
        let true_sps = 1.0 / 48_010.0; // device runs 200 ppm fast
        let mut worst = 0.0f64;
        let mut state = 12345u32;
        for k in 0..2000u32 {
            let ideal = k as f64 * 480.0 * true_sps;
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let jitter = ((state >> 16) as f64 / 65_536.0 - 0.5) * 0.002; // +/-1 ms
            let t = dll.update(1.0 + ideal + jitter, if k == 0 { 0 } else { 480 });
            if k > 1000 {
                worst = worst.max((t - 1.0 - ideal).abs());
            }
        }
        assert!(worst < 0.00015, "residual error {worst}");
        assert!((dll.seconds_per_sample() / true_sps - 1.0).abs() < 50e-6);
    }

    #[test]
    fn snapshot_extrapolates() {
        let s =
            ClockSnapshot { position: 48_000, audible_ns: 1_000_000_000, ns_per_sample: 1e9 / 48_000.0, playing: true };
        assert!((s.position_at(1_500_000_000) - 72_000.0).abs() < 1e-6);
        assert!((s.ns_at(72_000.0) - 1.5e9).abs() < 1e-3);
    }

    #[test]
    fn seqlock_roundtrip() {
        let c = SharedClock::default();
        let s = ClockSnapshot { position: 7, audible_ns: 9, ns_per_sample: 2.5, playing: true };
        c.publish(s);
        let r = c.read();
        assert_eq!((r.position, r.audible_ns, r.ns_per_sample, r.playing), (7, 9, 2.5, true));
    }
}
