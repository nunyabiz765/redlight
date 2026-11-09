//! 24-bit PCM WAV writing and a minimal reader for tests. Lifted from Plaza
//! (~/projects/plaza/src/wav.rs, same author, MIT OR Apache-2.0).

use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct WavWriter {
    out: BufWriter<File>,
    frames: u32,
    channels: u16,
}

impl WavWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> io::Result<Self> {
        let mut out = BufWriter::new(File::create(path)?);
        write_header(&mut out, sample_rate, channels, 0)?;
        Ok(WavWriter {
            out,
            frames: 0,
            channels,
        })
    }

    /// Interleaved samples, clamped to ±1 and rounded to 24 bits.
    pub fn write(&mut self, samples: &[f32]) -> io::Result<()> {
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
            self.out.write_all(&v.to_le_bytes()[..3])?;
        }
        self.frames += (samples.len() / self.channels as usize) as u32;
        Ok(())
    }

    pub fn frames(&self) -> u32 {
        self.frames
    }

    pub fn finish(mut self) -> io::Result<()> {
        let data = self.frames * self.channels as u32 * 3;
        self.out.flush()?;
        let f = self.out.get_mut();
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&(36 + data).to_le_bytes())?;
        f.seek(SeekFrom::Start(40))?;
        f.write_all(&data.to_le_bytes())?;
        f.flush()
    }
}

fn write_header(w: &mut impl Write, sr: u32, ch: u16, data: u32) -> io::Result<()> {
    let block = ch * 3;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&ch.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * block as u32).to_le_bytes())?;
    w.write_all(&block.to_le_bytes())?;
    w.write_all(&24u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data.to_le_bytes())
}

pub fn write_file(path: &Path, sample_rate: u32, channels: u16, samples: &[f32]) -> io::Result<()> {
    let mut w = WavWriter::create(path, sample_rate, channels)?;
    w.write(samples)?;
    w.finish()
}

/// Reads back a file written by this module: (sample rate, channels, samples).
pub fn read_file(path: &Path) -> io::Result<(u32, u16, Vec<f32>)> {
    let mut b = Vec::new();
    File::open(path)?.read_to_end(&mut b)?;
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "not a 24-bit PCM WAV from redlight",
        )
    };
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..16] != b"WAVEfmt " || &b[36..40] != b"data" {
        return Err(bad());
    }
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    if u16_at(20) != 1 || u16_at(34) != 24 {
        return Err(bad());
    }
    let ch = u16_at(22);
    let sr = u32_at(24);
    let len = u32_at(40) as usize;
    let data = b.get(44..44 + len).ok_or_else(bad)?;
    let samples = data
        .chunks_exact(3)
        .map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32 / 8_388_607.0)
        .collect();
    Ok((sr, ch, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("redlight-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        let s: Vec<f32> = (0..2000).map(|i| ((i as f32) * 0.01).sin() * 0.9).collect();
        let mut w = WavWriter::create(&path, 48_000, 2).unwrap();
        w.write(&s[..1000]).unwrap();
        w.write(&s[1000..]).unwrap();
        assert_eq!(w.frames(), 1000);
        w.finish().unwrap();
        let (sr, ch, back) = read_file(&path).unwrap();
        assert_eq!((sr, ch, back.len()), (48_000, 2, 2000));
        for (a, b) in s.iter().zip(&back) {
            assert!((a - b).abs() < 1e-6);
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
