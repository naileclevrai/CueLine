//! Tick spacing for the timecode ruler.

use cueline_core::FrameRate;

/// A grid step expressed either in frames or in whole seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Frames(i64),
    Seconds(i64),
}

impl Step {
    pub fn secs(self, rate: FrameRate) -> f64 {
        match self {
            Step::Frames(n) => n as f64 / rate.fps(),
            Step::Seconds(n) => n as f64,
        }
    }
}

/// Picks major and minor ruler steps so labels are at least `min_px` apart.
pub fn choose_steps(px_per_sec: f32, rate: FrameRate, min_px: f32) -> (Step, Step) {
    let nominal = rate.nominal() as i64;
    let mut frame_steps = vec![1, 2, 5, 10];
    if nominal % 2 == 0 {
        frame_steps.push(nominal / 2);
    }
    let candidates: Vec<Step> = frame_steps
        .into_iter()
        .filter(|&f| f < nominal)
        .map(Step::Frames)
        .chain([1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600].into_iter().map(Step::Seconds))
        .collect();
    let idx =
        candidates.iter().position(|s| s.secs(rate) as f32 * px_per_sec >= min_px).unwrap_or(candidates.len() - 1);
    let major = candidates[idx];
    let minor = match major {
        Step::Frames(1) => Step::Frames(1),
        Step::Frames(n) => Step::Frames((n / 5).max(1)),
        Step::Seconds(1) => Step::Frames((nominal / 5).max(1)),
        Step::Seconds(n) if n % 5 == 0 => Step::Seconds(n / 5),
        Step::Seconds(n) => Step::Seconds((n / 2).max(1)),
    };
    (major, minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_grow_with_zoom_out() {
        let r = FrameRate::Fps25;
        assert_eq!(choose_steps(4000.0, r, 80.0).0, Step::Frames(1));
        assert_eq!(choose_steps(100.0, r, 80.0).0, Step::Seconds(1));
        assert_eq!(choose_steps(10.0, r, 80.0).0, Step::Seconds(10));
        assert_eq!(choose_steps(0.01, r, 80.0).0, Step::Seconds(3600));
    }

    #[test]
    fn minor_divides_major() {
        for pps in [0.5, 3.0, 40.0, 300.0, 2000.0] {
            for rate in FrameRate::ALL {
                let (maj, min) = choose_steps(pps, rate, 80.0);
                let ratio = maj.secs(rate) / min.secs(rate);
                assert!((ratio - ratio.round()).abs() < 1e-9, "{maj:?}/{min:?} at {rate}");
            }
        }
    }
}
