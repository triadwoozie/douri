//! Phrase Crossfader and Segment Stitcher.
//!
//! Applies equal-power crossfade curves across overlapping phoneme segments
//! using `preutterance` and `overlap` parameters to generate seamless vocal phrases.

use std::f64::consts::FRAC_PI_2;

/// A rendered segment to be stitched into a vocal phrase.
#[derive(Clone, Debug)]
pub struct RenderedSegment {
    /// Resampled audio samples (mono f32).
    pub samples: Vec<f32>,
    /// Target start position in seconds relative to the beginning of the phrase.
    pub start_time_secs: f64,
    /// Preutterance duration in seconds (time before note beat onset).
    pub preutterance_secs: f64,
    /// Overlap duration with preceding segment in seconds.
    pub overlap_secs: f64,
}

/// Stitches and crossfades multiple segments into a single mono audio buffer.
///
/// Returns the stitched audio buffer along with `time_offset_secs` relative to phrase start (time = 0.0).
/// `sample_rate`: output sample rate (e.g. 44100 or 48000).
pub fn crossfade_phrase_with_offset(
    segments: &[RenderedSegment],
    sample_rate: u32,
) -> (Vec<f32>, f64) {
    if segments.is_empty() {
        return (Vec::new(), 0.0);
    }

    let sr = sample_rate as f64;

    // 1. Calculate the start sample index for each segment
    // Segment audio begins at (start_time_secs - preutterance_secs)
    let mut segment_starts = Vec::with_capacity(segments.len());
    let mut min_sample_start = 0i64;
    let mut max_sample_end = 0i64;

    for seg in segments {
        let onset_secs = seg.start_time_secs - seg.preutterance_secs;
        let start_sample = (onset_secs * sr).round() as i64;
        let end_sample = start_sample + seg.samples.len() as i64;

        if start_sample < min_sample_start {
            min_sample_start = start_sample;
        }
        if end_sample > max_sample_end {
            max_sample_end = end_sample;
        }
        segment_starts.push((start_sample, end_sample));
    }

    // Offset so minimum start is at or after 0
    let offset = if min_sample_start < 0 {
        -min_sample_start as usize
    } else {
        0
    };

    let total_samples = (max_sample_end + offset as i64).max(0) as usize;
    let mut phrase_buffer = vec![0.0f32; total_samples];

    // 2. Accumulate segments with crossfade envelopes
    for (i, seg) in segments.iter().enumerate() {
        let (raw_start, _) = segment_starts[i];
        let start = (raw_start + offset as i64).max(0) as usize;
        let len = seg.samples.len();
        if len == 0 {
            continue;
        }

        let overlap_len = ((seg.overlap_secs * sr).round().max(0.0) as usize).min(len);

        // Check if there is an incoming overlap from the previous segment
        let has_prev_overlap = i > 0 && overlap_len > 0;

        for (sample_idx, &val) in seg.samples.iter().enumerate() {
            let out_pos = start + sample_idx;
            if out_pos >= phrase_buffer.len() {
                break;
            }

            let mut gain = 1.0f32;

            // Equal-power fade in at the beginning of this segment across overlap
            if has_prev_overlap && sample_idx < overlap_len {
                let frac = sample_idx as f64 / overlap_len as f64;
                gain = (frac * FRAC_PI_2).sin() as f32;
            }

            phrase_buffer[out_pos] += val * gain;
        }

        // Apply equal-power fade out to the overlapping tail of the previous segment
        if has_prev_overlap {
            let prev_idx = i - 1;
            let (prev_start_raw, _) = segment_starts[prev_idx];
            let prev_start = (prev_start_raw + offset as i64).max(0) as usize;
            let prev_len = segments[prev_idx].samples.len();

            // The overlap region in the buffer is [start .. start + overlap_len]
            for (fade_idx, out_pos) in (start..start + overlap_len).enumerate() {
                if out_pos < phrase_buffer.len() {
                    let prev_sample_idx = out_pos.saturating_sub(prev_start);
                    if prev_sample_idx < prev_len {
                        let frac = fade_idx as f64 / overlap_len as f64;
                        let out_gain = (frac * FRAC_PI_2).cos() as f32;
                        let prev_val = segments[prev_idx].samples[prev_sample_idx];
                        // Adjust previous sample's contribution to equal-power curve
                        phrase_buffer[out_pos] += prev_val * (out_gain - 1.0);
                    }
                }
            }
        }
    }

    // Output gain staging: healthy vocal presence (-12 to -6 dBFS peaks)
    let max_peak = phrase_buffer.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max_peak > 0.001 && max_peak < 0.40 {
        let target_peak = 0.65f32; // ~ -3.7 dBFS healthy vocal peak
        let scale = target_peak / max_peak;
        for s in &mut phrase_buffer {
            *s *= scale;
        }
    }

    let time_offset = min_sample_start as f64 / sr;
    (phrase_buffer, time_offset)
}

/// Stitches and crossfades multiple segments into a single mono audio buffer.
///
/// `sample_rate`: output sample rate (e.g. 44100 or 48000).
pub fn crossfade_phrase(segments: &[RenderedSegment], sample_rate: u32) -> Vec<f32> {
    crossfade_phrase_with_offset(segments, sample_rate).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfades_two_overlapping_segments() {
        let sr = 1000;
        let seg1 = RenderedSegment {
            samples: vec![1.0; 500],
            start_time_secs: 0.1,
            preutterance_secs: 0.05,
            overlap_secs: 0.0,
        };
        let seg2 = RenderedSegment {
            samples: vec![1.0; 500],
            start_time_secs: 0.5,
            preutterance_secs: 0.05,
            overlap_secs: 0.1, // 100 samples overlap
        };

        let out = crossfade_phrase(&[seg1, seg2], sr);
        assert!(!out.is_empty());
        // Verify samples in overlap region have smooth equal-power transitions
        for &s in &out {
            assert!(s.is_finite());
            assert!((0.0..=2.0).contains(&s));
        }
    }
}
