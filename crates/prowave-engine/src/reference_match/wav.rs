//! Writing a matched master as a stereo WAV file: 16-bit (with TPDF dither), 24-bit or 32-bit
//! float. Matchering writes through libsndfile, which scales by 0x7FFF for 16 bits and does
//! not dither; Pro Wave dithers its 16-bit files, as a master to 16 bits should be.
use super::Stereo;
use std::io::{BufWriter, Write};
use std::path::Path;

/// The sample format of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Pcm16,
    Pcm24,
    Float32,
}

impl Depth {
    fn bytes(self) -> u16 {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Float32 => 4,
        }
    }
}

/// A small, fast noise source for the dither (xorshift).
struct Noise(u64);

impl Noise {
    /// Uniform in [-0.5, 0.5).
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    }
}

/// Writes `audio` at `rate` to `path` (through a temporary file beside it, renamed into place
/// when complete).
pub fn write_wav(path: &Path, audio: &Stereo, rate: u32, depth: Depth) -> std::io::Result<()> {
    let tmp = path.with_extension("wav.part");
    let res = write_to(&tmp, audio, rate, depth).and_then(|()| std::fs::rename(&tmp, path));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

fn write_to(path: &Path, audio: &Stereo, rate: u32, depth: Depth) -> std::io::Result<()> {
    let n = audio.len();
    let bytes = depth.bytes();
    let block = 2 * bytes;
    let data_len = u32::try_from(n as u64 * u64::from(block))
        .map_err(|_| std::io::Error::other("too long for a WAV file"))?;
    let mut w = BufWriter::with_capacity(1 << 20, std::fs::File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    let format: u16 = if depth == Depth::Float32 { 3 } else { 1 };
    w.write_all(&format.to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?;
    w.write_all(&rate.to_le_bytes())?;
    w.write_all(&(rate * u32::from(block)).to_le_bytes())?;
    w.write_all(&block.to_le_bytes())?;
    w.write_all(&(bytes * 8).to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    let mut noise = Noise(0x9E37_79B9_7F4A_7C15);
    for i in 0..n {
        for v in [audio.l[i], audio.r[i]] {
            let v = if v.is_finite() { v } else { 0.0 };
            match depth {
                Depth::Pcm16 => {
                    let d = noise.next() + noise.next();
                    let s = (v * 32767.0 + d).round().clamp(-32768.0, 32767.0) as i16;
                    w.write_all(&s.to_le_bytes())?;
                }
                Depth::Pcm24 => {
                    let s = (v * 8_388_607.0).round().clamp(-8_388_608.0, 8_388_607.0) as i32;
                    w.write_all(&s.to_le_bytes()[..3])?;
                }
                Depth::Float32 => w.write_all(&(v as f32).to_le_bytes())?,
            }
        }
    }
    w.flush()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn every_depth_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let x: Vec<f64> = (0..4410).map(|i| 0.5 * (i as f64 * 0.05).sin()).collect();
        let audio = Stereo {
            l: x.clone(),
            r: x.iter().map(|v| -v).collect(),
        };
        for (depth, tol) in [
            (Depth::Pcm16, 2.0 / 32768.0),
            (Depth::Pcm24, 1.0 / 8_388_607.0),
            (Depth::Float32, 1e-7),
        ] {
            let path = dir.path().join(format!("{depth:?}.wav"));
            write_wav(&path, &audio, 44_100, depth).unwrap();
            let f = crate::AudioFile::open(&path).unwrap();
            assert_eq!((f.sample_rate, f.channels, f.frames), (44_100, 2, 4410));
            let back = f.read_stereo().unwrap();
            for i in 0..4410 {
                assert!(
                    (f64::from(back[2 * i]) - audio.l[i]).abs() <= tol,
                    "{depth:?}"
                );
                assert!(
                    (f64::from(back[2 * i + 1]) - audio.r[i]).abs() <= tol,
                    "{depth:?}"
                );
            }
        }
    }
}
