//! Shared DSP model types ported from Marbble (<https://github.com/JackMechem/marbble>),
//! Copyright (c) 2026 Jack Mechem, MIT License.

use serde::{Deserialize, Serialize};

/// Number of send slots per channel strip.
pub const SEND_SLOTS: usize = 8;

/// Filter shape of a Channel EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandKind {
    LowCut,
    LowShelf,
    Bell,
    HighShelf,
    HighCut,
}

/// One band of the Channel EQ, in plain units.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EqBand {
    pub kind: BandKind,
    pub on: bool,
    /// Hz
    pub freq: f32,
    /// dB (ignored by the cut filters)
    pub gain: f32,
    pub q: f32,
    /// dB/octave for the cut filters: 12, 24 or 48.
    #[serde(default = "default_slope")]
    pub slope: u8,
}

fn default_slope() -> u8 {
    24
}

/// The eight-band Channel EQ's settings, modelled on Logic's Channel EQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEq {
    pub on: bool,
    pub bands: Vec<EqBand>,
    #[serde(default)]
    pub output_db: f32,
}

impl Default for ChannelEq {
    fn default() -> Self {
        let b = |kind, on, freq, q| EqBand {
            kind,
            on,
            freq,
            gain: 0.0,
            q,
            slope: 24,
        };
        ChannelEq {
            on: true,
            bands: vec![
                b(BandKind::LowCut, false, 30.0, 0.71),
                b(BandKind::LowShelf, true, 80.0, 0.71),
                b(BandKind::Bell, true, 160.0, 1.1),
                b(BandKind::Bell, true, 500.0, 1.1),
                b(BandKind::Bell, true, 1500.0, 1.1),
                b(BandKind::Bell, true, 5000.0, 1.1),
                b(BandKind::HighShelf, true, 10000.0, 0.71),
                b(BandKind::HighCut, false, 18000.0, 0.71),
            ],
            output_db: 0.0,
        }
    }
}

impl ChannelEq {
    /// True when the EQ would leave the signal untouched, so it can be skipped entirely.
    pub fn is_flat(&self) -> bool {
        Self::flat(self.on, &self.bands, self.output_db)
    }

    /// [`is_flat`](Self::is_flat) for a Channel EQ given by its parts.
    pub fn flat(on: bool, bands: &[EqBand], output_db: f32) -> bool {
        !on || (output_db.abs() < 0.01
            && bands.iter().all(|b| {
                !b.on
                    || (matches!(
                        b.kind,
                        BandKind::Bell | BandKind::LowShelf | BandKind::HighShelf
                    ) && b.gain.abs() < 0.01)
            }))
    }
}
