//! Real-time DSP effects, plug-in delay compensation, and instrument engines ported from
//! Marbble (<https://github.com/JackMechem/marbble>), Copyright (c) 2026 Jack Mechem, MIT License.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod fx;
pub mod instrument;
pub mod model;
pub mod pdc;
pub mod synth_dsp;

/// The largest block `process` is called with.
pub const MAX_BLOCK: usize = fx::MAX_BLOCK;
