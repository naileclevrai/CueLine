//! Offline, high-quality sample-rate conversion of whole clips.
//!
//! Clips are converted once at import (or when the device rate changes) so
//! the real-time mixer only ever copies samples.

use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};

pub fn resample(channels: &[Vec<f32>], from: u32, to: u32) -> Result<Vec<Vec<f32>>, String> {
    if from == to || channels.is_empty() {
        return Ok(channels.to_vec());
    }
    let n_ch = channels.len();
    let frames = channels[0].len();
    let mut rs = Fft::<f32>::new(from as usize, to as usize, 4096, n_ch, FixedSync::Input)
        .map_err(|e| format!("resampler: {e}"))?;
    let input = SequentialSliceOfVecs::new(channels, n_ch, frames).map_err(|e| e.to_string())?;
    let out_len = rs.process_all_needed_output_len(frames);
    let mut out = vec![vec![0.0f32; out_len]; n_ch];
    let written = {
        let mut output =
            SequentialSliceOfVecs::new_mut(&mut out, n_ch, out_len).map_err(|e| e.to_string())?;
        rs.process_all_into_buffer(&input, &mut output, frames, None)
            .map_err(|e| format!("resampling failed: {e}"))?
            .1
    };
    for ch in &mut out {
        ch.truncate(written);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_scales_with_ratio() {
        let sine: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = resample(&[sine.clone(), sine], 44_100, 48_000).unwrap();
        assert_eq!(out.len(), 2);
        assert!((out[0].len() as i64 - 48_000).abs() <= 2, "{}", out[0].len());
    }
}
