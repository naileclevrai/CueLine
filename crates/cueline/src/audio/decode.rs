//! Audio file decoding (WAV, AIFF, FLAC, MP3, AAC/M4A, ALAC, Ogg Vorbis).

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Decoded audio in planar `f32`, at the file's native sample rate.
/// Files with more than two channels keep their first two.
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl DecodedAudio {
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / self.sample_rate as f64
    }
}

pub fn decode_file(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("unsupported format: {e}"))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or("no audio track found")?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("track has no audio parameters")?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;

    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let mut out: Vec<Vec<f32>> = Vec::new();
    let mut scratch: Vec<Vec<f32>> = Vec::new();
    if let Some(n) = track.num_frames {
        out.reserve(2);
        out.push(Vec::with_capacity(n as usize));
    }

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(format!("read error: {e}")),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            // Corrupt packets are skipped, as every player does.
            Err(SymError::DecodeError(_)) => continue,
            Err(e) => return Err(format!("decode error: {e}")),
        };
        if sample_rate == 0 {
            sample_rate = buf.spec().rate();
        }
        buf.copy_to_vecs_planar::<f32>(&mut scratch);
        let n_ch = scratch.len().min(2);
        if out.len() < n_ch {
            let len = out.first().map_or(0, Vec::len);
            out.resize_with(n_ch, || vec![0.0; len]);
        }
        let frames = buf.frames();
        for (dst, src) in out.iter_mut().zip(scratch.iter()) {
            dst.extend_from_slice(&src[..frames.min(src.len())]);
        }
    }

    if sample_rate == 0 || out.first().is_none_or(Vec::is_empty) {
        return Err("file contains no audio".into());
    }
    Ok(DecodedAudio { sample_rate, channels: out })
}
