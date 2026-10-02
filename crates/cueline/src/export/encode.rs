//! File encoders. All of them stream from the [`Renderer`], so memory use
//! stays flat however long the show is (FLAC keeps only compressed frames).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::num::{NonZeroU32, NonZeroU8};
use std::path::Path;

use super::aiff::AiffWriter;
use super::Renderer;
use crate::audio::ffmpeg;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Wav,
    Aiff,
    Flac,
    Mp3,
    Vorbis,
    /// Encoded through ffmpeg.
    Opus,
    /// AAC in an .m4a container, encoded through ffmpeg.
    Aac,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    I16,
    I24,
    I32,
    F32,
}

impl Depth {
    pub const ALL: [Depth; 4] = [Depth::I16, Depth::I24, Depth::I32, Depth::F32];

    pub fn label(self) -> &'static str {
        match self {
            Depth::I16 => "16-bit",
            Depth::I24 => "24-bit",
            Depth::I32 => "32-bit",
            Depth::F32 => "32-bit float",
        }
    }

    fn bits(self) -> u16 {
        match self {
            Depth::I16 => 16,
            Depth::I24 => 24,
            Depth::I32 | Depth::F32 => 32,
        }
    }
}

impl Codec {
    pub const ALL: [Codec; 7] =
        [Codec::Wav, Codec::Aiff, Codec::Flac, Codec::Mp3, Codec::Vorbis, Codec::Opus, Codec::Aac];

    pub fn label(self) -> &'static str {
        match self {
            Codec::Wav => "WAV",
            Codec::Aiff => "AIFF",
            Codec::Flac => "FLAC",
            Codec::Mp3 => "MP3",
            Codec::Vorbis => "Ogg Vorbis",
            Codec::Opus => "Opus",
            Codec::Aac => "AAC (.m4a)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Codec::Wav => "wav",
            Codec::Aiff => "aiff",
            Codec::Flac => "flac",
            Codec::Mp3 => "mp3",
            Codec::Vorbis => "ogg",
            Codec::Opus => "opus",
            Codec::Aac => "m4a",
        }
    }

    pub fn lossless(self) -> bool {
        matches!(self, Codec::Wav | Codec::Aiff | Codec::Flac)
    }

    pub fn needs_ffmpeg(self) -> bool {
        matches!(self, Codec::Opus | Codec::Aac)
    }

    pub fn depths(self) -> &'static [Depth] {
        match self {
            Codec::Wav => &Depth::ALL,
            Codec::Aiff => &[Depth::I16, Depth::I24, Depth::I32],
            Codec::Flac => &[Depth::I16, Depth::I24],
            _ => &[],
        }
    }

    pub fn max_channels(self) -> usize {
        match self {
            Codec::Mp3 => 2,
            Codec::Aac => 8,
            _ => 255,
        }
    }

    pub fn sample_rates(self) -> &'static [u32] {
        match self {
            Codec::Mp3 => &[32_000, 44_100, 48_000],
            Codec::Opus => &[48_000],
            _ => &[44_100, 48_000, 88_200, 96_000, 176_400, 192_000],
        }
    }

    /// Bitrates offered for lossy codecs, in kbit/s.
    pub fn bitrates(self) -> &'static [u32] {
        match self {
            Codec::Mp3 => &[128, 160, 192, 256, 320],
            Codec::Opus => &[64, 96, 128, 160, 192, 256],
            Codec::Aac => &[128, 192, 256, 320],
            _ => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Encoding {
    pub codec: Codec,
    pub depth: Depth,
    /// `None` = the device sample rate.
    pub sample_rate: Option<u32>,
    pub kbps: u32,
    /// Vorbis quality, -0.1 ..= 1.0.
    pub vorbis_quality: f32,
}

impl Encoding {
    pub fn new(codec: Codec) -> Self {
        let depth = codec.depths().iter().copied().find(|d| *d == Depth::I24).unwrap_or(Depth::I16);
        let kbps = match codec {
            Codec::Mp3 => 320,
            Codec::Opus => 160,
            Codec::Aac => 256,
            _ => 0,
        };
        Self { codec, depth, sample_rate: None, kbps, vorbis_quality: 0.8 }
    }

    #[cfg(test)]
    pub fn depth(mut self, depth: Depth) -> Self {
        self.depth = depth;
        self
    }

    #[cfg(test)]
    pub fn sample_rate(mut self, rate: Option<u32>) -> Self {
        self.sample_rate = rate;
        self
    }

    /// Checks the combination before any work is done.
    pub fn validate(&self, channels: usize) -> Result<(), String> {
        let c = self.codec;
        if channels > c.max_channels() {
            return Err(format!("{} supports at most {} channels", c.label(), c.max_channels()));
        }
        if !c.depths().is_empty() && !c.depths().contains(&self.depth) {
            return Err(format!("{} does not support {}", c.label(), self.depth.label()));
        }
        if let Some(r) = self.sample_rate {
            if !c.sample_rates().contains(&r) {
                return Err(format!("{} does not support {r} Hz", c.label()));
            }
        }
        if c.needs_ffmpeg() && ffmpeg::locate().is_none() {
            return Err(format!("{} export needs ffmpeg (see Settings → Formats)", c.label()));
        }
        Ok(())
    }
}

/// Converts to integer samples at `bits`, with TPDF dither for 16-bit.
struct Quantizer {
    bits: u16,
    rng: u32,
}

impl Quantizer {
    fn new(bits: u16) -> Self {
        Self { bits, rng: 0x1234_5678 }
    }

    fn noise(&mut self) -> f32 {
        // xorshift32, uniform in [-0.5, 0.5)
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) - 0.5
    }

    fn convert(&mut self, input: &[f32], out: &mut Vec<i32>) {
        out.clear();
        let max = ((1i64 << (self.bits - 1)) - 1) as f32;
        let dither = self.bits == 16;
        for &s in input {
            let mut v = s.clamp(-1.0, 1.0) * max;
            if dither {
                v += self.noise() + self.noise();
            }
            out.push(v.round().clamp(-max - 1.0, max) as i32);
        }
    }
}

const BLOCK: usize = 4096;

/// Renders and encodes to `path`, writing a temporary file renamed on success.
pub fn write_file(path: &Path, enc: &Encoding, r: &mut Renderer<'_>) -> Result<(), String> {
    let part = path.with_extension(format!("{}.part", enc.codec.extension()));
    let result = encode(&part, enc, r);
    match result {
        Ok(()) => std::fs::rename(&part, path).map_err(|e| format!("cannot write {}: {e}", path.display())),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

fn encode(path: &Path, enc: &Encoding, r: &mut Renderer<'_>) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    match enc.codec {
        Codec::Wav => wav(path, enc.depth, r),
        Codec::Aiff => {
            let file = File::create(path).map_err(io)?;
            let mut w = AiffWriter::create(file, r.channels as u16, enc.depth.bits(), r.sample_rate).map_err(io)?;
            let mut q = Quantizer::new(enc.depth.bits());
            let (mut buf, mut ints) = (vec![0.0f32; BLOCK * r.channels], Vec::new());
            loop {
                let n = r.fill(&mut buf)?;
                if n == 0 {
                    break;
                }
                q.convert(&buf[..n * r.channels], &mut ints);
                w.write(&ints).map_err(io)?;
            }
            w.finish().map_err(io)
        }
        Codec::Flac => flac(path, enc.depth, r),
        Codec::Mp3 => mp3(path, enc.kbps, r),
        Codec::Vorbis => vorbis(path, enc.vorbis_quality, r),
        Codec::Opus | Codec::Aac => {
            let ffmpeg = ffmpeg::locate().ok_or("ffmpeg not found")?;
            let tmp = ffmpeg::TempFile::new("wav");
            wav(&tmp.0, Depth::F32, r)?;
            let codec = if enc.codec == Codec::Opus { "libopus" } else { "aac" };
            let kbps = format!("{}k", enc.kbps);
            let format = if enc.codec == Codec::Opus { "opus" } else { "ipod" };
            let args: [&std::ffi::OsStr; 9] = [
                "-i".as_ref(),
                tmp.0.as_os_str(),
                "-c:a".as_ref(),
                codec.as_ref(),
                "-b:a".as_ref(),
                kbps.as_ref(),
                "-f".as_ref(),
                format.as_ref(),
                path.as_os_str(),
            ];
            ffmpeg::run(&ffmpeg, &args)
        }
    }
}

fn wav(path: &Path, depth: Depth, r: &mut Renderer<'_>) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: r.channels as u16,
        sample_rate: r.sample_rate,
        bits_per_sample: depth.bits(),
        sample_format: if depth == Depth::F32 { hound::SampleFormat::Float } else { hound::SampleFormat::Int },
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    let mut q = Quantizer::new(depth.bits());
    let (mut buf, mut ints) = (vec![0.0f32; BLOCK * r.channels], Vec::new());
    loop {
        let n = r.fill(&mut buf)?;
        if n == 0 {
            break;
        }
        let block = &buf[..n * r.channels];
        if depth == Depth::F32 {
            for &s in block {
                w.write_sample(s).map_err(|e| e.to_string())?;
            }
        } else {
            q.convert(block, &mut ints);
            for &s in &ints {
                w.write_sample(s).map_err(|e| e.to_string())?;
            }
        }
    }
    w.finalize().map_err(|e| e.to_string())
}

/// Feeds the FLAC encoder on demand from the renderer.
struct FlacSource<'r, 'a> {
    r: &'r mut Renderer<'a>,
    q: Quantizer,
    buf: Vec<f32>,
    ints: Vec<i32>,
    error: Option<String>,
}

impl flacenc::source::Source for FlacSource<'_, '_> {
    fn channels(&self) -> usize {
        self.r.channels
    }
    fn bits_per_sample(&self) -> usize {
        self.q.bits as usize
    }
    fn sample_rate(&self) -> usize {
        self.r.sample_rate as usize
    }
    fn read_samples<F: flacenc::source::Fill>(
        &mut self,
        block_size: usize,
        dest: &mut F,
    ) -> Result<usize, flacenc::error::SourceError> {
        self.buf.resize(block_size * self.r.channels, 0.0);
        let n = match self.r.fill(&mut self.buf) {
            Ok(n) => n,
            Err(e) => {
                self.error = Some(e);
                0
            }
        };
        self.q.convert(&self.buf[..n * self.r.channels], &mut self.ints);
        dest.fill_interleaved(&self.ints)?;
        Ok(n)
    }
    fn len_hint(&self) -> Option<usize> {
        Some(self.r.total_frames())
    }
}

fn flac(path: &Path, depth: Depth, r: &mut Renderer<'_>) -> Result<(), String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let mut config = flacenc::config::Encoder::default();
    if depth == Depth::I24 && r.has_ltc {
        // flacenc caps the Rice parameter at 14 without escape codes, so the
        // full-scale square edges of 24-bit LTC explode predicted subframes.
        // Verbatim subframes keep the file lossless at about WAV size.
        config.subframe_coding.use_fixed = false;
        config.subframe_coding.use_lpc = false;
    }
    let config = config.into_verified().map_err(|e| format!("{e:?}"))?;
    let block = config.block_size;
    let mut src = FlacSource { r, q: Quantizer::new(depth.bits()), buf: Vec::new(), ints: Vec::new(), error: None };
    let stream = flacenc::encode_with_fixed_block_size(&config, &mut src, block).map_err(|e| format!("{e:?}"))?;
    if let Some(e) = src.error {
        return Err(e);
    }
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| format!("{e:?}"))?;
    std::fs::write(path, sink.as_slice()).map_err(|e| e.to_string())
}

fn lame_bitrate(kbps: u32) -> mp3lame_encoder::Bitrate {
    use mp3lame_encoder::Bitrate::*;
    match kbps {
        0..=128 => Kbps128,
        129..=160 => Kbps160,
        161..=192 => Kbps192,
        193..=256 => Kbps256,
        _ => Kbps320,
    }
}

fn mp3(path: &Path, kbps: u32, r: &mut Renderer<'_>) -> Result<(), String> {
    use mp3lame_encoder::{Builder, DualPcm, FlushNoGap, MonoPcm, Quality};
    let err = |e: &dyn std::fmt::Debug| format!("MP3 encoder: {e:?}");
    let mut b = Builder::new().ok_or("cannot create the MP3 encoder")?;
    b.set_num_channels(r.channels as u8).map_err(|e| err(&e))?;
    b.set_sample_rate(r.sample_rate).map_err(|e| err(&e))?;
    b.set_brate(lame_bitrate(kbps)).map_err(|e| err(&e))?;
    b.set_quality(Quality::Best).map_err(|e| err(&e))?;
    let mut enc = b.build().map_err(|e| err(&e))?;
    let mut out = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
    let mut buf = vec![0.0f32; BLOCK * r.channels];
    let (mut left, mut right, mut bytes) = (Vec::new(), Vec::new(), Vec::new());
    loop {
        let n = r.fill(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.clear();
        bytes.reserve(mp3lame_encoder::max_required_buffer_size(n));
        if r.channels == 1 {
            enc.encode_to_vec(MonoPcm(&buf[..n]), &mut bytes).map_err(|e| err(&e))?;
        } else {
            left.clear();
            right.clear();
            for f in buf[..n * 2].as_chunks::<2>().0 {
                left.push(f[0]);
                right.push(f[1]);
            }
            enc.encode_to_vec(DualPcm { left: &left, right: &right }, &mut bytes).map_err(|e| err(&e))?;
        }
        out.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    bytes.clear();
    bytes.reserve(7200);
    enc.flush_to_vec::<FlushNoGap>(&mut bytes).map_err(|e| err(&e))?;
    out.write_all(&bytes).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())
}

fn vorbis(path: &Path, quality: f32, r: &mut Renderer<'_>) -> Result<(), String> {
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
    let err = |e: vorbis_rs::VorbisError| format!("Vorbis encoder: {e}");
    let file = BufWriter::new(File::create(path).map_err(|e| e.to_string())?);
    let sr = NonZeroU32::new(r.sample_rate).ok_or("invalid sample rate")?;
    let ch = NonZeroU8::new(r.channels as u8).ok_or("invalid channel count")?;
    let mut enc = VorbisEncoderBuilder::new(sr, ch, file)
        .map_err(err)?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: quality.clamp(-0.1, 1.0),
        })
        .build()
        .map_err(err)?;
    let mut buf = vec![0.0f32; BLOCK * r.channels];
    let mut planar = vec![Vec::with_capacity(BLOCK); r.channels];
    loop {
        let n = r.fill(&mut buf)?;
        if n == 0 {
            break;
        }
        for (c, ch) in planar.iter_mut().enumerate() {
            ch.clear();
            ch.extend(buf[..n * r.channels].iter().skip(c).step_by(r.channels));
        }
        enc.encode_audio_block(&planar).map_err(err)?;
    }
    enc.finish().map_err(err)?.flush().map_err(|e| e.to_string())
}
