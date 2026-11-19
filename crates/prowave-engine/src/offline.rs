// Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Multi-format offline bounce, TPDF dithering, and sample quantization.
//!
//! Provides faster-than-realtime offline export to WAV (16-bit PCM, 24-bit PCM, 32-bit float)
//! with ITU-R BS.1770 loudness & 4× True Peak measurement and Triangular Probability Density
//! Function (TPDF) dithering.

use std::io::Write;
use std::path::Path;

use crate::meter::{gain_to_db, LoudnessMeter};

/// Audio bit depth for offline bounce and file export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BitDepth {
    /// 16-bit linear PCM (CD standard).
    Pcm16,
    /// 24-bit linear PCM (Studio mastering standard).
    #[default]
    Pcm24,
    /// 32-bit IEEE 754 floating-point (Lossless engine resolution).
    Float32,
}

impl BitDepth {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pcm16 => "16-Bit PCM",
            Self::Pcm24 => "24-Bit PCM",
            Self::Float32 => "32-Bit Float",
        }
    }

    pub fn bits(self) -> u16 {
        match self {
            Self::Pcm16 => 16,
            Self::Pcm24 => 24,
            Self::Float32 => 32,
        }
    }
}

/// Fast, deterministic allocation-free Xorshift32 PRNG for audio dithering.
#[derive(Debug, Clone, Copy)]
pub struct DitherRng(u32);

impl Default for DitherRng {
    fn default() -> Self {
        Self::new(0xD17E)
    }
}

impl DitherRng {
    pub fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform white noise in [-1.0, 1.0].
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }

    /// Triangular Probability Density Function (TPDF) noise in [-1.0, 1.0] (peak-to-peak 2.0).
    #[inline]
    pub fn tpdf(&mut self) -> f32 {
        self.bipolar() - self.bipolar()
    }
}

/// Result of an offline bounce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OfflineBounceResult {
    /// Number of stereo frames rendered.
    pub frames: usize,
    /// Highest peak sample in dBFS.
    pub peak_db: f32,
    /// Integrated loudness in LUFS (ITU-R BS.1770 / EBU R128).
    pub lufs_integrated: f32,
    /// Maximum True Peak in dBTP (4× oversampled).
    pub true_peak_db: f32,
}

/// Writes interleaved stereo audio to a WAV file with selected bit depth and optional TPDF dither.
///
/// Supports:
/// - 16-bit PCM (`WAVE_FORMAT_PCM` = 1, 16 bits)
/// - 24-bit PCM (`WAVE_FORMAT_PCM` = 1, 24 bits)
/// - 32-bit Float (`WAVE_FORMAT_IEEE_FLOAT` = 3, 32 bits)
///
/// Digital silence (0.0) remains pure digital silence without added dither noise.
/// The file is written to `<path>.part` first and atomically renamed on completion.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn write_wav_with_options(
    path: &Path,
    stereo: &[f32],
    sample_rate: u32,
    bit_depth: BitDepth,
    dither: bool,
) -> std::io::Result<OfflineBounceResult> {
    let frames = stereo.len() / 2;
    let channels = 2u16;
    let bits_per_sample = bit_depth.bits();
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = u32::try_from(frames * usize::from(block_align))
        .map_err(|_| std::io::Error::other("audio too long for standard WAV file"))?;

    let format_tag: u16 = match bit_depth {
        BitDepth::Float32 => 3, // IEEE Float
        _ => 1,                 // PCM
    };

    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // subchunk 1 size (16 for PCM)
    b.extend_from_slice(&format_tag.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&byte_rate.to_le_bytes());
    b.extend_from_slice(&block_align.to_le_bytes());
    b.extend_from_slice(&bits_per_sample.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());

    let mut rng = DitherRng::default();
    let mut meter = LoudnessMeter::new(sample_rate as f32, 2);
    let mut peak: f32 = 0.0;

    for frame in stereo.chunks_exact(2) {
        let (mut l, mut r) = (frame[0], frame[1]);
        if !l.is_finite() {
            l = 0.0;
        }
        if !r.is_finite() {
            r = 0.0;
        }

        peak = peak.max(l.abs()).max(r.abs());
        meter.process_sample(l, r);

        match bit_depth {
            BitDepth::Float32 => {
                b.extend_from_slice(&l.to_le_bytes());
                b.extend_from_slice(&r.to_le_bytes());
            }
            BitDepth::Pcm16 => {
                let quantize = |x: f32, rng: &mut DitherRng| -> i16 {
                    if x == 0.0 {
                        0
                    } else {
                        let d = if dither { rng.tpdf() * 0.5 } else { 0.0 };
                        ((x * 32767.0 + d).round()).clamp(-32768.0, 32767.0) as i16
                    }
                };
                let ql = quantize(l, &mut rng);
                let qr = quantize(r, &mut rng);
                b.extend_from_slice(&ql.to_le_bytes());
                b.extend_from_slice(&qr.to_le_bytes());
            }
            BitDepth::Pcm24 => {
                let quantize = |x: f32, rng: &mut DitherRng| -> i32 {
                    if x == 0.0 {
                        0
                    } else {
                        let d = if dither { rng.tpdf() * 0.5 } else { 0.0 };
                        ((x * 8388607.0 + d).round()).clamp(-8388608.0, 8388607.0) as i32
                    }
                };
                let ql = quantize(l, &mut rng);
                let qr = quantize(r, &mut rng);
                let bl = (ql as u32).to_le_bytes();
                b.extend_from_slice(&bl[..3]);
                let br = (qr as u32).to_le_bytes();
                b.extend_from_slice(&br[..3]);
            }
        }
    }

    let tmp = path.with_extension("wav.part");
    std::fs::File::create(&tmp)?.write_all(&b)?;
    std::fs::rename(&tmp, path)?;

    Ok(OfflineBounceResult {
        frames,
        peak_db: gain_to_db(peak),
        lufs_integrated: meter.integrated_lufs(),
        true_peak_db: meter.true_peak_db(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn tpdf_dither_distribution_bounded() {
        let mut rng = DitherRng::new(42);
        for _ in 0..10_000 {
            let d = rng.tpdf();
            assert!((-2.0..=2.0).contains(&d), "TPDF out of bounds: {d}");
        }
    }

    #[test]
    fn write_wav_pcm16_pcm24_float32() {
        let dir = std::env::temp_dir().join(format!("douri-bounce-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let sr = 48_000;
        let mut audio = Vec::new();
        for i in 0..4800 {
            let s = (std::f32::consts::TAU * 440.0 * i as f32 / sr as f32).sin() * 0.5;
            audio.push(s);
            audio.push(s);
        }

        // Test PCM16 with dither
        let p16 = dir.join("test16.wav");
        let res16 = write_wav_with_options(&p16, &audio, sr, BitDepth::Pcm16, true).unwrap();
        assert_eq!(res16.frames, 4800);
        assert!(p16.exists());

        // Test PCM24 with dither
        let p24 = dir.join("test24.wav");
        let res24 = write_wav_with_options(&p24, &audio, sr, BitDepth::Pcm24, true).unwrap();
        assert_eq!(res24.frames, 4800);
        assert!(p24.exists());

        // Test Float32 without dither
        let pf32 = dir.join("test32f.wav");
        let res32 = write_wav_with_options(&pf32, &audio, sr, BitDepth::Float32, false).unwrap();
        assert_eq!(res32.frames, 4800);
        assert!(pf32.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
