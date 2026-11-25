//! Local AI/Spectral Stem Separation, Drum Kit Extraction, and Zero-Crossing Micro-Fades.
//!
//! Runs offline on worker threads (never on the real-time audio thread).
//! Uses STFT + Harmonic-Percussive Source Separation (HPSS) + Mid/Side Wiener masking
//! with optional De-Bleed / Spill Reducer mode (0% to 100%, up to -12 dB attenuation).

use crate::spectral::Fft;
use std::f32::consts::PI;
use std::path::{Path, PathBuf};

/// The four canonical stems.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StemKind {
    Vocals = 0,
    Drums = 1,
    Bass = 2,
    Other = 3,
}

impl StemKind {
    pub const ALL: [Self; 4] = [Self::Vocals, Self::Drums, Self::Bass, Self::Other];

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Vocals => "V",
            Self::Drums => "D",
            Self::Bass => "B",
            Self::Other => "O",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Vocals => "Vocals",
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Other => "Other",
        }
    }
}

/// Deterministic recipe for Git-tracked stem separation.
/// Instead of committing large `.wav` stems, projects store this recipe and regenerate
/// stems locally in `~/.cache/douri/stems/` (or `%LOCALAPPDATA%/douri/cache/stems/`).
#[derive(Clone, Debug, PartialEq)]
pub struct StemRecipe {
    pub source_hash: String,
    pub start_frame: u64,
    pub len_frames: Option<u64>,
    pub model_id: String,
    pub debleed_pct: f32,
}

impl StemRecipe {
    pub fn cache_key(&self) -> String {
        let len_str = self
            .len_frames
            .map(|l| l.to_string())
            .unwrap_or_else(|| "full".to_string());
        format!(
            "{}_{}_{}_{}_db{:.0}",
            self.source_hash, self.start_frame, len_str, self.model_id, self.debleed_pct
        )
    }
}

/// Returns the local stem cache directory (`~/.cache/douri/stems` or `%LOCALAPPDATA%/douri/cache/stems`).
pub fn stem_cache_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("douri").join("cache").join("stems");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("douri").join("stems");
    }
    std::env::temp_dir().join("douri_stems_cache")
}

/// 4 separated stereo stems.
#[derive(Clone, Debug)]
pub struct StemSet {
    pub sample_rate: u32,
    pub frames: usize,
    /// `[Vocals, Drums, Bass, Other]` each `(left, right)`.
    pub stems: [(Vec<f32>, Vec<f32>); 4],
}

/// Separates stereo audio into 4 stems `[Vocals, Drums, Bass, Other]`.
/// `debleed_pct` (0.0..=100.0) attenuates bleed gently (up to -12 dB) instead of hard-zeroing,
/// avoiding watery phase artifacts for singer-songwriters and podcasters.
pub fn separate_stems(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    debleed_pct: f32,
) -> StemSet {
    let n_frames = left.len().min(right.len());
    if n_frames == 0 {
        return StemSet {
            sample_rate,
            frames: 0,
            stems: [
                (Vec::new(), Vec::new()),
                (Vec::new(), Vec::new()),
                (Vec::new(), Vec::new()),
                (Vec::new(), Vec::new()),
            ],
        };
    }

    const FFT_N: usize = 2048;
    const HOP: usize = 512;
    let bins = FFT_N / 2 + 1;
    let fft = Fft::new(FFT_N);

    let hann: Vec<f32> = (0..FFT_N)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / FFT_N as f32).cos()))
        .collect();

    let num_stft_frames = (n_frames + HOP - 1) / HOP + 1;
    let mut mag_mid = vec![0.0f32; num_stft_frames * bins];
    let mut mag_side = vec![0.0f32; num_stft_frames * bins];

    let mut spec_l_re = vec![0.0f32; num_stft_frames * FFT_N];
    let mut spec_l_im = vec![0.0f32; num_stft_frames * FFT_N];
    let mut spec_r_re = vec![0.0f32; num_stft_frames * FFT_N];
    let mut spec_r_im = vec![0.0f32; num_stft_frames * FFT_N];

    let mut re = vec![0.0f32; FFT_N];
    let mut im = vec![0.0f32; FFT_N];

    // Forward STFT
    for (t, frame_idx) in (0..num_stft_frames).enumerate() {
        let start = frame_idx * HOP;

        for i in 0..FFT_N {
            let s = if start + i < n_frames {
                left[start + i]
            } else {
                0.0
            };
            re[i] = s * hann[i];
            im[i] = 0.0;
        }
        fft.process(&mut re, &mut im, false);
        spec_l_re[t * FFT_N..(t + 1) * FFT_N].copy_from_slice(&re);
        spec_l_im[t * FFT_N..(t + 1) * FFT_N].copy_from_slice(&im);

        for i in 0..FFT_N {
            let s = if start + i < n_frames {
                right[start + i]
            } else {
                0.0
            };
            re[i] = s * hann[i];
            im[i] = 0.0;
        }
        fft.process(&mut re, &mut im, false);
        spec_r_re[t * FFT_N..(t + 1) * FFT_N].copy_from_slice(&re);
        spec_r_im[t * FFT_N..(t + 1) * FFT_N].copy_from_slice(&im);

        let base = t * FFT_N;
        let m_base = t * bins;
        for k in 0..bins {
            let lr = spec_l_re[base + k];
            let li = spec_l_im[base + k];
            let rr = spec_r_re[base + k];
            let ri = spec_r_im[base + k];

            let mr = 0.5 * (lr + rr);
            let mi = 0.5 * (li + ri);
            let sr = 0.5 * (lr - rr);
            let si = 0.5 * (li - ri);

            mag_mid[m_base + k] = (mr * mr + mi * mi).sqrt();
            mag_side[m_base + k] = (sr * sr + si * si).sqrt();
        }
    }

    // Harmonic-Percussive Source Separation (HPSS)
    let mut harm = vec![0.0f32; num_stft_frames * bins];
    let mut perc = vec![0.0f32; num_stft_frames * bins];

    for t in 0..num_stft_frames {
        for k in 0..bins {
            let t0 = t.saturating_sub(2);
            let t1 = (t + 2).min(num_stft_frames - 1);
            let mut sum_t = 0.0f32;
            let mut cnt_t = 0.0f32;
            for tt in t0..=t1 {
                sum_t += mag_mid[tt * bins + k] + mag_side[tt * bins + k];
                cnt_t += 1.0;
            }
            harm[t * bins + k] = sum_t / cnt_t.max(1.0);

            let k0 = k.saturating_sub(2);
            let k1 = (k + 2).min(bins - 1);
            let mut sum_k = 0.0f32;
            let mut cnt_k = 0.0f32;
            for kk in k0..=k1 {
                sum_k += mag_mid[t * bins + kk] + mag_side[t * bins + kk];
                cnt_k += 1.0;
            }
            perc[t * bins + k] = sum_k / cnt_k.max(1.0);
        }
    }

    let bin_hz = sample_rate as f32 / FFT_N as f32;
    let debleed = (debleed_pct.clamp(0.0, 100.0)) / 100.0;
    let bleed_floor = if debleed > 0.0 {
        1.0 - debleed * (1.0 - 0.2512)
    } else {
        0.0
    };

    let mut out_stems: [(Vec<f32>, Vec<f32>); 4] = [
        (vec![0.0; n_frames], vec![0.0; n_frames]),
        (vec![0.0; n_frames], vec![0.0; n_frames]),
        (vec![0.0; n_frames], vec![0.0; n_frames]),
        (vec![0.0; n_frames], vec![0.0; n_frames]),
    ];
    let mut win_norm = vec![0.0f32; n_frames];

    for t in 0..num_stft_frames {
        let start = t * HOP;
        for i in 0..FFT_N {
            if start + i < n_frames {
                win_norm[start + i] += hann[i] * hann[i];
            }
        }
    }

    for stem_idx in 0..4 {
        for t in 0..num_stft_frames {
            let base = t * FFT_N;
            let m_base = t * bins;

            let mut re_l = vec![0.0f32; FFT_N];
            let mut im_l = vec![0.0f32; FFT_N];
            let mut re_r = vec![0.0f32; FFT_N];
            let mut im_r = vec![0.0f32; FFT_N];

            for k in 0..bins {
                let freq = k as f32 * bin_hz;
                let h = harm[m_base + k];
                let p = perc[m_base + k];
                let mid = mag_mid[m_base + k];
                let side = mag_side[m_base + k];
                let eps = 1e-7;

                let w_drums = (p * p) / (p * p + h * h + eps);
                let w_harm = 1.0 - w_drums;

                let w_bass = if freq < 250.0 {
                    w_harm * (1.0 - (freq / 250.0).powi(2))
                } else {
                    0.0
                };

                let center_ratio = (mid * mid) / (mid * mid + side * side + eps);
                let vocal_band = if (180.0..=6500.0).contains(&freq) {
                    1.0
                } else if freq < 180.0 {
                    (freq / 180.0).clamp(0.0, 1.0)
                } else {
                    (1.0 - (freq - 6500.0) / 4000.0).clamp(0.0, 1.0)
                };
                let w_vocals = w_harm * center_ratio * vocal_band * (1.0 - w_bass);
                let w_other = (1.0 - w_drums - w_bass - w_vocals).max(0.0);

                let sum_w = (w_vocals + w_drums + w_bass + w_other).max(eps);
                let mut mask = match stem_idx {
                    0 => w_vocals / sum_w,
                    1 => w_drums / sum_w,
                    2 => w_bass / sum_w,
                    _ => w_other / sum_w,
                };

                if debleed > 0.0 {
                    mask = bleed_floor + (1.0 - bleed_floor) * mask;
                }

                re_l[k] = spec_l_re[base + k] * mask;
                im_l[k] = spec_l_im[base + k] * mask;
                re_r[k] = spec_r_re[base + k] * mask;
                im_r[k] = spec_r_im[base + k] * mask;

                if k > 0 && k < FFT_N / 2 {
                    re_l[FFT_N - k] = re_l[k];
                    im_l[FFT_N - k] = -im_l[k];
                    re_r[FFT_N - k] = re_r[k];
                    im_r[FFT_N - k] = -im_r[k];
                }
            }

            fft.process(&mut re_l, &mut im_l, true);
            fft.process(&mut re_r, &mut im_r, true);

            let start = t * HOP;
            let inv_n = 1.0 / FFT_N as f32;
            for i in 0..FFT_N {
                if start + i < n_frames {
                    out_stems[stem_idx].0[start + i] += re_l[i] * inv_n * hann[i];
                    out_stems[stem_idx].1[start + i] += re_r[i] * inv_n * hann[i];
                }
            }
        }

        for i in 0..n_frames {
            let norm = win_norm[i].max(1e-6);
            out_stems[stem_idx].0[i] /= norm;
            out_stems[stem_idx].1[i] /= norm;
        }
    }

    StemSet {
        sample_rate,
        frames: n_frames,
        stems: out_stems,
    }
}

/// Extracted drum hit for `.sfz` kit generation.
#[derive(Clone, Debug)]
pub struct DrumHit {
    pub name: String,
    pub midi_note: u8,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

/// Extracted MIDI note from a drum stem.
#[derive(Clone, Debug)]
pub struct DrumMidiEvent {
    pub beat: f32,
    pub midi_note: u8,
    pub velocity: u8,
    pub dur_beats: f32,
}

/// Result of one-click "Extract Kit & Groove".
#[derive(Clone, Debug)]
pub struct ExtractedDrumKit {
    pub hits: Vec<DrumHit>,
    pub events: Vec<DrumMidiEvent>,
    pub sfz_content: String,
}

/// Slices a drum stem by frequency band (`<150 Hz` = Kick, `150Hz-2kHz` = Snare, `>5kHz` = Hats)
/// into individual hits, generates an `.sfz` instrument definition, and extracts the MIDI pattern.
pub fn extract_drum_kit_and_groove(
    drums_l: &[f32],
    drums_r: &[f32],
    sample_rate: u32,
    bpm: f32,
) -> ExtractedDrumKit {
    let n = drums_l.len().min(drums_r.len());
    let sr = sample_rate.max(8000) as f32;
    let hop = (sr * 0.010) as usize;
    let min_gap = (sr * 0.070) as usize;
    let max_hit_len = (sr * 0.350) as usize;

    let mut onsets = Vec::new();
    let mut prev_e = 0.0f32;
    let mut last_onset = 0usize;

    let mut pos = 0usize;
    while pos + hop < n {
        let mut e = 0.0f32;
        for i in pos..pos + hop {
            let m = 0.5 * (drums_l[i] + drums_r[i]);
            e += m * m;
        }
        e = (e / hop as f32).sqrt();

        if e > 0.04 && e > prev_e * 1.8 && (onsets.is_empty() || pos >= last_onset + min_gap) {
            onsets.push((pos, e));
            last_onset = pos;
        }
        prev_e = prev_e * 0.6 + e * 0.4;
        pos += hop;
    }

    let mut hits = Vec::new();
    let mut events = Vec::new();

    for (idx, &(onset, peak)) in onsets.iter().enumerate() {
        let next_onset = onsets.get(idx + 1).map(|o| o.0).unwrap_or(n);
        let end = (onset + max_hit_len).min(next_onset).min(n);
        if end <= onset + 64 {
            continue;
        }

        let mut slice_l = drums_l[onset..end].to_vec();
        let mut slice_r = drums_r[onset..end].to_vec();
        apply_micro_fade_5ms(&mut slice_l, sample_rate);
        apply_micro_fade_5ms(&mut slice_r, sample_rate);

        let mut zcr = 0usize;
        let mut lp = 0.0f32;
        let mut low_e = 0.0f32;
        let mut total_e = 0.0f32;
        let alpha = (2.0 * PI * 150.0 / sr).clamp(0.001, 0.5);

        for i in 1..slice_l.len().min(2048) {
            let m = 0.5 * (slice_l[i] + slice_r[i]);
            let prev_m = 0.5 * (slice_l[i - 1] + slice_r[i - 1]);
            if (m >= 0.0) != (prev_m >= 0.0) {
                zcr += 1;
            }
            lp += alpha * (m - lp);
            low_e += lp * lp;
            total_e += m * m;
        }

        let est_hz = (zcr as f32 * sr) / (2.0 * slice_l.len().min(2048) as f32);
        let low_ratio = low_e / total_e.max(1e-9);

        let (midi_note, base_name) = if low_ratio > 0.45 || est_hz < 180.0 {
            (36u8, "kick")
        } else if est_hz > 3500.0 {
            (42u8, "hat")
        } else {
            (38u8, "snare")
        };

        let beat = (onset as f32 / sr) * (bpm / 60.0);
        let vel = ((peak * 240.0).clamp(28.0, 127.0)) as u8;
        events.push(DrumMidiEvent {
            beat,
            midi_note,
            velocity: vel,
            dur_beats: 0.25,
        });

        if !hits.iter().any(|h: &DrumHit| h.midi_note == midi_note) {
            hits.push(DrumHit {
                name: format!("{base_name}_{idx}.wav"),
                midi_note,
                left: slice_l,
                right: slice_r,
            });
        }
    }

    let mut sfz = String::from(
        "// Auto-generated by Dōri Extract Kit & Groove\n<control>\ndefault_path=\n\n",
    );
    for hit in &hits {
        sfz.push_str(&format!(
            "<region> sample={} key={} lovel=0 hivel=127 amp_veltrack=100\n",
            hit.name, hit.midi_note
        ));
    }

    ExtractedDrumKit {
        hits,
        events,
        sfz_content: sfz,
    }
}

/// Applies a 5ms raised-cosine micro-fade-in and micro-fade-out to prevent clicks at cut points.
pub fn apply_micro_fade_5ms(buf: &mut [f32], sample_rate: u32) {
    let fade_samples = ((sample_rate as f32 * 0.005) as usize)
        .max(1)
        .min(buf.len() / 2);
    for i in 0..fade_samples {
        let t = i as f32 / fade_samples as f32;
        let g = 0.5 * (1.0 - (PI * t).cos());
        buf[i] *= g;
        let end_idx = buf.len() - 1 - i;
        buf[end_idx] *= g;
    }
}

/// Writes a 32-bit float stereo WAV file to disk.
pub fn write_wav_stereo(
    path: &Path,
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let frames = left.len().min(right.len());
    let data_bytes = (frames * 2 * 4) as u32;
    let riff_size = 36 + data_bytes;

    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 8).to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());

    for i in 0..frames {
        out.extend_from_slice(&left[i].to_le_bytes());
        out.extend_from_slice(&right[i].to_le_bytes());
    }

    std::fs::write(path, out).map_err(|e| e.to_string())
}
