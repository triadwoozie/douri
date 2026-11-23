// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Loopback round-trip latency calibrator.
//!
//! Emits a band-limited click burst on an output channel and captures the looped back input.
//! Uses normalized cross-correlation peak finding to detect the exact sample delay between
//! output and input, allowing accurate recording latency compensation.

use std::f32::consts::TAU;

pub const CLICK_LEVEL: f32 = 0.8;
pub const BURST_HZ: f32 = 1500.0;
pub const DURATION_MS: u64 = 1500;
pub const START_MS: u64 = 50;
pub const INTERVAL_MS: u64 = 250;
pub const MAX_ROUND_TRIP_MS: u64 = 200;
pub const MIN_ENERGY_RATIO: f32 = 0.05;
pub const MIN_CORRELATION: f32 = 0.65;
pub const MIN_FOUND: usize = 3;

/// Generates a band-limited Hann-windowed sine burst template for latency correlation.
pub fn click_template(rate: u32) -> Vec<f32> {
    let rate = rate.max(8_000);
    let burst = (rate / 1000).max(8) as usize;
    let mut samples = vec![0.0f32; burst + 1];
    samples[0] = CLICK_LEVEL;
    for n in 0..burst {
        let phase = (n as f32 + 0.5) / burst as f32;
        let window = 0.5 * (1.0 - (TAU * phase).cos());
        samples[n + 1] = CLICK_LEVEL * window * (TAU * BURST_HZ * n as f32 / rate as f32).sin();
    }
    samples
}

#[inline]
pub fn frames_of(rate: u32, ms: u64) -> u64 {
    u64::from(rate) * ms / 1000
}

/// The result of a successful loopback latency measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measured {
    /// Median round trip in frames of the sample rate.
    pub round_trip_frames: u32,
    /// Spread (max - min) over valid clicks in frames (uncertainty).
    pub spread_frames: u32,
    /// Clicks successfully detected.
    pub found: usize,
    /// Total clicks sent.
    pub sent: usize,
}

impl Measured {
    /// Latency in milliseconds.
    pub fn ms(&self, rate: u32) -> f64 {
        f64::from(self.round_trip_frames) * 1000.0 / f64::from(rate.max(1))
    }
}

/// Why a latency measurement failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    NoSignal { found: usize, sent: usize },
    BufferTooShort,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSignal { found, sent } => {
                write!(
                    f,
                    "Loopback click detected on only {found} of {sent} bursts"
                )
            }
            Self::BufferTooShort => write!(f, "Recorded buffer is too short for measurement"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Analyzes a recorded audio stream containing loopback clicks to determine the round-trip latency.
pub fn analyze(recorded: &[f32], rate: u32) -> Result<Measured, ProbeError> {
    let template = click_template(rate);
    let energy: f32 = template.iter().map(|s| s * s).sum();
    let first = frames_of(rate, START_MS) as usize;
    let interval = frames_of(rate, INTERVAL_MS).max(1) as usize;
    let duration = frames_of(rate, DURATION_MS) as usize;
    let reach = frames_of(rate, MAX_ROUND_TRIP_MS) as usize;

    if recorded.len() < first + reach + template.len() {
        return Err(ProbeError::BufferTooShort);
    }

    let sent = (0..)
        .map(|k| first + k * interval)
        .take_while(|s| *s < duration && *s + reach + template.len() <= recorded.len())
        .count();

    if sent == 0 {
        return Err(ProbeError::BufferTooShort);
    }

    let mut lags: Vec<usize> = Vec::with_capacity(sent);

    for k in 0..sent {
        let start = first + k * interval;
        let Some(window) = recorded.get(start..) else {
            break;
        };
        let window = window
            .get(..(reach + template.len()).min(window.len()))
            .unwrap_or(&[]);

        let mut best: Option<(usize, f32)> = None;
        let search_len = window.len().saturating_sub(template.len());
        for lag in 0..search_len {
            let seg = &window[lag..lag + template.len()];
            let dot: f32 = seg.iter().zip(&template).map(|(a, b)| a * b).sum();
            let e: f32 = seg.iter().map(|a| a * a).sum();
            if e < energy * MIN_ENERGY_RATIO {
                continue;
            }
            let denom = (e * energy).sqrt();
            if denom <= 1e-12 {
                continue;
            }
            let c = dot / denom;
            if c >= MIN_CORRELATION && best.as_ref().is_none_or(|(_, b)| c > *b) {
                best = Some((lag, c));
            }
        }

        if let Some((lag, _)) = best {
            lags.push(lag);
        }
    }

    if lags.len() < MIN_FOUND {
        return Err(ProbeError::NoSignal {
            found: lags.len(),
            sent,
        });
    }

    lags.sort_unstable();
    let median = lags[lags.len() / 2];
    let spread = lags[lags.len() - 1] - lags[0];

    Ok(Measured {
        round_trip_frames: u32::try_from(median).unwrap_or(u32::MAX),
        spread_frames: u32::try_from(spread).unwrap_or(u32::MAX),
        found: lags.len(),
        sent,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detects_known_sample_delay() {
        let rate = 48_000;
        let delay_frames = 256;
        let total_frames = frames_of(rate, DURATION_MS + 200) as usize;
        let mut recorded = vec![0.0f32; total_frames];

        let template = click_template(rate);
        let first = frames_of(rate, START_MS) as usize;
        let interval = frames_of(rate, INTERVAL_MS) as usize;
        let duration = frames_of(rate, DURATION_MS) as usize;

        let sent = (0..)
            .map(|k| first + k * interval)
            .take_while(|s| *s < duration)
            .count();

        for k in 0..sent {
            let emit_pos = first + k * interval;
            let recv_pos = emit_pos + delay_frames;
            if recv_pos + template.len() <= recorded.len() {
                for (i, &s) in template.iter().enumerate() {
                    recorded[recv_pos + i] += s * 0.95; // slight attenuation
                }
            }
        }

        let res = analyze(&recorded, rate).expect("analysis should succeed");
        assert_eq!(res.round_trip_frames, delay_frames as u32);
        assert_eq!(res.spread_frames, 0);
        assert_eq!(res.found, sent);
    }
}
