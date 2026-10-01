//! Multi-resolution min/max peak pyramid for fast waveform drawing.

const BASE_BLOCK: usize = 32;
const FACTOR: usize = 4;

struct Level {
    block: usize,
    min: Vec<f32>,
    max: Vec<f32>,
}

pub struct Peaks {
    pub sample_rate: u32,
    pub frames: usize,
    levels: Vec<Level>,
}

impl Peaks {
    pub fn build(channels: &[Vec<f32>], sample_rate: u32) -> Self {
        let frames = channels.first().map_or(0, Vec::len);
        let bins = frames.div_ceil(BASE_BLOCK);
        let mut min = vec![0.0f32; bins];
        let mut max = vec![0.0f32; bins];
        for ch in channels {
            for (i, chunk) in ch.chunks(BASE_BLOCK).enumerate() {
                for &s in chunk {
                    min[i] = min[i].min(s);
                    max[i] = max[i].max(s);
                }
            }
        }
        let mut levels = vec![Level { block: BASE_BLOCK, min, max }];
        while levels.last().unwrap().min.len() > 64 {
            let prev = levels.last().unwrap();
            let reduce = |v: &[f32], f: fn(f32, f32) -> f32| -> Vec<f32> {
                v.chunks(FACTOR).map(|c| c.iter().copied().reduce(f).unwrap()).collect()
            };
            let next = Level {
                block: prev.block * FACTOR,
                min: reduce(&prev.min, f32::min),
                max: reduce(&prev.max, f32::max),
            };
            levels.push(next);
        }
        Self { sample_rate, frames, levels }
    }

    /// Min/max over the source frames `[start, end)`; `None` outside the clip.
    pub fn range(&self, start: f64, end: f64) -> Option<(f32, f32)> {
        let start = start.max(0.0);
        let end = end.min(self.frames as f64);
        if end <= start {
            return None;
        }
        let span = end - start;
        let level = self
            .levels
            .iter()
            .rev()
            .find(|l| (l.block as f64) <= span)
            .unwrap_or(&self.levels[0]);
        let a = (start as usize) / level.block;
        let b = ((end.ceil() as usize).div_ceil(level.block)).clamp(a + 1, level.min.len());
        let lo = level.min[a..b].iter().copied().fold(f32::MAX, f32::min);
        let hi = level.max[a..b].iter().copied().fold(f32::MIN, f32::max);
        Some((lo, hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_peaks_at_every_zoom() {
        let mut s = vec![0.0f32; 100_000];
        s[54_321] = 0.9;
        s[54_400] = -0.7;
        let p = Peaks::build(&[s], 48_000);
        assert_eq!(p.range(0.0, 100_000.0), Some((-0.7, 0.9)));
        assert_eq!(p.range(54_000.0, 55_000.0), Some((-0.7, 0.9)));
        let (lo, hi) = p.range(0.0, 1000.0).unwrap();
        assert_eq!((lo, hi), (0.0, 0.0));
        assert_eq!(p.range(200_000.0, 300_000.0), None);
    }
}
