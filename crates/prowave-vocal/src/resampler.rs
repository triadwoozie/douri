//! UTAU Resampler abstraction: external CLI resamplers & pure-Rust built-in fallback.
//!
//! Handles invocation of standard UTAU resamplers (e.g. `moresampler`, `straycat`, `worldline`)
//! and provides a pure-Rust built-in time-stretcher and pitch-shifter fallback powered by
//! `prowave-stretch`.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::voicebank::tone_to_name;

/// Arguments for a single phoneme resampler invocation.
#[derive(Clone, Debug)]
pub struct ResamplerItem {
    pub input_file: PathBuf,
    pub output_file: PathBuf,
    pub tone: u8,
    pub velocity: u8,
    pub flags: String,
    pub offset: f64,
    pub dur_required: f64,
    pub consonant: f64,
    pub cutoff: f64,
    pub volume: f64,
    pub modulation: f64,
    pub tempo: f64,
    pub pitch_bends: Vec<i16>,
}

impl Default for ResamplerItem {
    fn default() -> Self {
        Self {
            input_file: PathBuf::new(),
            output_file: PathBuf::new(),
            tone: 60,
            velocity: 100,
            flags: String::new(),
            offset: 0.0,
            dur_required: 500.0,
            consonant: 100.0,
            cutoff: 0.0,
            volume: 100.0,
            modulation: 0.0,
            tempo: 120.0,
            pitch_bends: Vec::new(),
        }
    }
}

/// Base64 encoding of 12-bit signed integers for UTAU pitch bends.
pub fn encode_utau_pitch_bends(pitches: &[i16]) -> String {
    if pitches.is_empty() {
        return String::new();
    }
    const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(pitches.len() * 2);
    for &p in pitches {
        // Clamp to 12-bit signed (-2048..=2047)
        let v = (p & 0x0FFF) as u16;
        let c1 = ((v >> 6) & 0x3F) as usize;
        let c2 = (v & 0x3F) as usize;
        out.push(B64_CHARS[c1] as char);
        out.push(B64_CHARS[c2] as char);
    }
    out
}

/// Runs an external UTAU resampler executable.
pub fn run_external_resampler(
    resampler_bin: &Path,
    item: &ResamplerItem,
) -> std::io::Result<Vec<f32>> {
    let tone_name = tone_to_name(item.tone);
    let pitches_b64 = encode_utau_pitch_bends(&item.pitch_bends);

    // Standard CLI arguments:
    // "<input>" "<output>" <tone> <velocity> "<flags>" <offset> <dur_required> <consonant> <cutoff> <volume> <modulation> !<tempo> <pitches>
    let mut cmd = Command::new(resampler_bin);
    cmd.arg(&item.input_file)
        .arg(&item.output_file)
        .arg(&tone_name)
        .arg(item.velocity.to_string())
        .arg(&item.flags)
        .arg(format!("{:.1}", item.offset))
        .arg(format!("{:.1}", item.dur_required))
        .arg(format!("{:.1}", item.consonant))
        .arg(format!("{:.1}", item.cutoff))
        .arg(format!("{:.1}", item.volume))
        .arg(format!("{:.1}", item.modulation))
        .arg(format!("!{:.2}", item.tempo))
        .arg(&pitches_b64);

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Resampler exited with code {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    read_mono_wav(&item.output_file)
}

/// Pure-Rust built-in resampler fallback using `prowave-stretch`.
///
/// 1. Reads the source WAV audio file.
/// 2. Cuts the segment defined by `offset` and `cutoff`.
/// 3. Computes the target semitone shift: `target_tone - root_tone`.
/// 4. Preserves the consonant un-stretched, and stretches the vowel to `dur_required`.
/// 5. Applies volume scaling.
pub fn run_builtin_resampler(
    item: &ResamplerItem,
    root_tone: u8,
    sample_rate: u32,
) -> std::io::Result<Vec<f32>> {
    let raw_samples = read_mono_wav(&item.input_file)?;
    if raw_samples.is_empty() {
        return Ok(Vec::new());
    }

    let sr = sample_rate as f64;
    let total_frames = raw_samples.len();

    // 1. Calculate sample slice bounds
    let offset_samples = ((item.offset / 1000.0) * sr).round().max(0.0) as usize;
    let cutoff_samples = if item.cutoff < 0.0 {
        // Cutoff from end of file
        let end = (total_frames as f64 + (item.cutoff / 1000.0) * sr).round();
        end.max(offset_samples as f64) as usize
    } else if item.cutoff > 0.0 {
        // Cutoff from start of file
        let end = ((item.cutoff / 1000.0) * sr).round();
        end.min(total_frames as f64).max(offset_samples as f64) as usize
    } else {
        total_frames
    };

    if offset_samples >= total_frames || offset_samples >= cutoff_samples {
        return Ok(Vec::new());
    }

    let slice = &raw_samples[offset_samples..cutoff_samples.min(total_frames)];
    if slice.is_empty() {
        return Ok(Vec::new());
    }

    // 2. Pitch shifting
    let semitones = item.tone as f64 - root_tone as f64;
    let pitch_shifted = if semitones.abs() > 0.01 {
        prowave_stretch::shift_pitch(slice, 1, semitones).unwrap_or_else(|_| slice.to_vec())
    } else {
        slice.to_vec()
    };

    // 3. Time stretching: preserve consonant, stretch vowel
    let target_total_samples = ((item.dur_required / 1000.0) * sr).round().max(1.0) as usize;
    let consonant_samples = ((item.consonant / 1000.0) * sr).round().max(0.0) as usize;

    let mut result = if consonant_samples >= pitch_shifted.len() {
        // Whole segment is consonant
        pitch_shifted
    } else {
        let consonant_part = &pitch_shifted[..consonant_samples];
        let vowel_part = &pitch_shifted[consonant_samples..];

        let target_vowel_samples = target_total_samples.saturating_sub(consonant_samples);
        if target_vowel_samples == 0 || vowel_part.is_empty() {
            consonant_part.to_vec()
        } else {
            let stretch_factor = target_vowel_samples as f64 / vowel_part.len() as f64;
            let stretched_vowel = if (stretch_factor - 1.0).abs() > 0.05 {
                prowave_stretch::stretch(vowel_part, 1, stretch_factor)
                    .unwrap_or_else(|_| vowel_part.to_vec())
            } else {
                vowel_part.to_vec()
            };

            let mut combined = Vec::with_capacity(consonant_part.len() + stretched_vowel.len());
            combined.extend_from_slice(consonant_part);
            combined.extend_from_slice(&stretched_vowel);
            combined
        }
    };

    // 4. Volume scaling
    let gain = (item.volume / 100.0) as f32;
    if (gain - 1.0).abs() > 0.001 {
        for s in &mut result {
            *s *= gain;
        }
    }

    Ok(result)
}

/// Reads a 16-bit or 32-bit float PCM WAV file and returns mono f32 samples.
pub fn read_mono_wav(path: &Path) -> std::io::Result<Vec<f32>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Not a valid RIFF/WAVE file",
        ));
    }

    let mut pos = 12;
    let mut num_channels = 1u16;
    let mut bits_per_sample = 16u16;
    let mut audio_format = 1u16; // 1 = PCM, 3 = IEEE float
    let mut data_start = 0;
    let mut data_len = 0;

    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let chunk_data = pos + 8;
        if chunk_id == b"fmt " && chunk_data + 16 <= bytes.len() {
            audio_format = u16::from_le_bytes(bytes[chunk_data..chunk_data + 2].try_into().unwrap());
            num_channels = u16::from_le_bytes(bytes[chunk_data + 2..chunk_data + 4].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(bytes[chunk_data + 14..chunk_data + 16].try_into().unwrap());
        } else if chunk_id == b"data" {
            data_start = chunk_data;
            data_len = chunk_size.min(bytes.len().saturating_sub(chunk_data));
            break;
        }
        pos = chunk_data + chunk_size;
    }

    if data_start == 0 || data_len == 0 {
        return Ok(Vec::new());
    }

    let data = &bytes[data_start..data_start + data_len];
    let channels = num_channels.max(1) as usize;
    let mut samples = Vec::new();

    match (audio_format, bits_per_sample) {
        (1, 16) => {
            // 16-bit signed PCM
            let total_samples = data.len() / 2;
            let frames = total_samples / channels;
            samples.reserve(frames);
            for frame in 0..frames {
                let mut sum = 0.0f32;
                for ch in 0..channels {
                    let idx = (frame * channels + ch) * 2;
                    let val = i16::from_le_bytes([data[idx], data[idx + 1]]);
                    sum += val as f32 / 32768.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        (3, 32) => {
            // 32-bit IEEE float
            let total_samples = data.len() / 4;
            let frames = total_samples / channels;
            samples.reserve(frames);
            for frame in 0..frames {
                let mut sum = 0.0f32;
                for ch in 0..channels {
                    let idx = (frame * channels + ch) * 4;
                    let val = f32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
                    sum += val;
                }
                samples.push(sum / channels as f32);
            }
        }
        (1, 24) => {
            // 24-bit signed PCM
            let total_samples = data.len() / 3;
            let frames = total_samples / channels;
            samples.reserve(frames);
            for frame in 0..frames {
                let mut sum = 0.0f32;
                for ch in 0..channels {
                    let idx = (frame * channels + ch) * 3;
                    let b0 = data[idx];
                    let b1 = data[idx + 1];
                    let b2 = data[idx + 2];
                    let val = i32::from_le_bytes([b0, b1, b2, if b2 & 0x80 != 0 { 0xFF } else { 0x00 }]);
                    sum += val as f32 / 8388608.0;
                }
                samples.push(sum / channels as f32);
            }
        }
        _ => {
            // Fallback: silence
        }
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_pitch_bends_base64() {
        let pitches = vec![0, 10, -10, 100];
        let encoded = encode_utau_pitch_bends(&pitches);
        assert!(!encoded.is_empty());
        assert_eq!(encoded.len(), 8); // 2 chars per 12-bit int
    }
}
