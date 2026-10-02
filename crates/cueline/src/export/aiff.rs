//! Minimal streaming AIFF writer (big-endian integer PCM).

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};

/// Sample rate as an 80-bit IEEE 754 extended float, as AIFF requires.
fn extended(rate: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if rate == 0 {
        return out;
    }
    let exp = 31 - rate.leading_zeros();
    let mantissa = (rate as u64) << (63 - exp);
    let e = (exp + 16383) as u16;
    out[..2].copy_from_slice(&e.to_be_bytes());
    out[2..].copy_from_slice(&mantissa.to_be_bytes());
    out
}

pub struct AiffWriter {
    out: BufWriter<File>,
    channels: u16,
    bits: u16,
    frames: u32,
}

impl AiffWriter {
    pub fn create(file: File, channels: u16, bits: u16, sample_rate: u32) -> std::io::Result<Self> {
        let mut out = BufWriter::new(file);
        out.write_all(b"FORM")?;
        out.write_all(&0u32.to_be_bytes())?; // patched in finish()
        out.write_all(b"AIFF")?;
        out.write_all(b"COMM")?;
        out.write_all(&18u32.to_be_bytes())?;
        out.write_all(&channels.to_be_bytes())?;
        out.write_all(&0u32.to_be_bytes())?; // frame count, patched
        out.write_all(&bits.to_be_bytes())?;
        out.write_all(&extended(sample_rate))?;
        out.write_all(b"SSND")?;
        out.write_all(&0u32.to_be_bytes())?; // chunk size, patched
        out.write_all(&0u32.to_be_bytes())?; // offset
        out.write_all(&0u32.to_be_bytes())?; // block size
        Ok(Self { out, channels, bits, frames: 0 })
    }

    /// Writes interleaved samples already scaled to the bit depth.
    pub fn write(&mut self, samples: &[i32]) -> std::io::Result<()> {
        let bytes = (self.bits / 8) as usize;
        for &s in samples {
            let be = s.to_be_bytes();
            self.out.write_all(&be[4 - bytes..])?;
        }
        self.frames += (samples.len() / self.channels as usize) as u32;
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<()> {
        let data = self.frames as u64 * self.channels as u64 * (self.bits / 8) as u64;
        let pad = data % 2; // chunks are word-aligned
        if pad == 1 {
            self.out.write_all(&[0])?;
        }
        let ssnd_size = (data + 8) as u32;
        let form_size = (4 + 8 + 18 + 8 + ssnd_size as u64 + pad) as u32;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&form_size.to_be_bytes())?;
        f.seek(SeekFrom::Start(22))?;
        f.write_all(&self.frames.to_be_bytes())?;
        f.seek(SeekFrom::Start(42))?;
        f.write_all(&ssnd_size.to_be_bytes())?;
        f.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_sample_rates() {
        // Reference encodings of common rates.
        assert_eq!(extended(44_100), [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(48_000), [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(96_000), [0x40, 0x0F, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
    }
}
