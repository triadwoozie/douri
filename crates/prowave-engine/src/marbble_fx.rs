//! Marbble built-in effects adapter for `prowave-engine` (`BlockProcessor`).
//!
//! Ported from Marbble (<https://github.com/JackMechem/marbble>),
//! Copyright (c) 2026 Jack Mechem, MIT License.

use crate::plugin_track::{BlockProcessor, BlockTransport};
use marbble_dsp::fx::{self, Effect, EffectInfo, FxContext, MAX_METERS};

/// Wraps any of Marbble's 17 built-in effects as a zero-allocation [`BlockProcessor`].
pub struct MarbbleFxNode {
    info: EffectInfo,
    effect: Box<dyn Effect>,
    sample_rate: f32,
    ctx: FxContext,
    meters: [f32; MAX_METERS],
}

impl MarbbleFxNode {
    /// Opens a Marbble built-in effect by id (e.g. `"builtin:compressor"`).
    /// Allocates buffers up front; call on a non-real-time thread.
    pub fn open(id: &str, sample_rate: f32) -> Option<Self> {
        let info = fx::find(id)?;
        let effect = (info.new)(sample_rate);
        Some(Self {
            info,
            effect,
            sample_rate,
            ctx: FxContext {
                sample_rate,
                tempo: 120.0,
                playing: false,
                position: 0.0,
            },
            meters: [0.0; MAX_METERS],
        })
    }

    pub fn info(&self) -> &EffectInfo {
        &self.info
    }

    pub fn set_param_by_id(&mut self, id: u32, value: f32) {
        if let Some(spec) = self.info.param(id) {
            let clamped = if value.is_finite() {
                value.clamp(spec.min, spec.max)
            } else {
                spec.default
            };
            self.effect.set_param(id, clamped);
        }
    }

    pub fn param_by_id(&self, id: u32) -> f32 {
        self.effect.param(id)
    }

    pub fn meters(&self) -> &[f32; MAX_METERS] {
        &self.meters
    }

    pub fn latency_samples(&self) -> usize {
        self.effect.latency()
    }

    pub fn reset(&mut self) {
        self.effect.reset();
    }
}

impl BlockProcessor for MarbbleFxNode {
    fn midi(&mut self, _status: u8, _data1: u8, _data2: u8, _offset: u32) {}

    fn process(&mut self, input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let n = input
            .0
            .len()
            .min(input.1.len())
            .min(out.0.len())
            .min(out.1.len());
        out.0[..n].copy_from_slice(&input.0[..n]);
        out.1[..n].copy_from_slice(&input.1[..n]);
        for chunk_start in (0..n).step_by(fx::MAX_BLOCK) {
            let end = (chunk_start + fx::MAX_BLOCK).min(n);
            self.effect.process(
                &mut out.0[chunk_start..end],
                &mut out.1[chunk_start..end],
                None,
                &self.ctx,
            );
        }
        self.effect.meters(&mut self.meters);
    }

    fn latency_seconds(&self) -> f64 {
        self.effect.latency() as f64 / f64::from(self.sample_rate.max(1.0))
    }

    fn set_parameter(&mut self, index: usize, value: f32) {
        if let Some(spec) = self.info.params.get(index) {
            let clamped = if value.is_finite() {
                value.clamp(spec.min, spec.max)
            } else {
                spec.default
            };
            self.effect.set_param(spec.id, clamped);
        }
    }

    fn wants_transport(&self) -> bool {
        true
    }

    fn block_transport(&mut self, t: &BlockTransport) {
        self.ctx.playing = t.playing;
        self.ctx.tempo = t.tempo;
        self.ctx.position = t.beat;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_no_alloc::assert_no_alloc;
    use marbble_dsp::pdc::{DelayLine, PdcParams, PdcTaps, Tap};

    #[test]
    fn all_17_marbble_effects_and_pdc_are_allocation_free_in_process() {
        let sr = 48_000.0;
        let reg = fx::registry();
        assert_eq!(reg.len(), 17);

        let in_l = [0.25_f32; 256];
        let in_r = [-0.25_f32; 256];
        let mut out_l = [0.0_f32; 256];
        let mut out_r = [0.0_f32; 256];

        for info in &reg {
            let Some(mut node) = MarbbleFxNode::open(info.id, sr) else {
                panic!("failed to open marbble fx {}", info.id);
            };
            // Warm up once before assert_no_alloc (e.g. thread handles)
            node.process((&in_l, &in_r), (&mut out_l, &mut out_r));
            assert_no_alloc(|| {
                node.set_parameter(0, info.params.first().map_or(0.0, |p| p.default));
                node.process((&in_l, &in_r), (&mut out_l, &mut out_r));
            });
            assert!(out_l.iter().all(|x| x.is_finite()));
            assert!(out_r.iter().all(|x| x.is_finite()));
        }

        let mut pdc_line = DelayLine::new(2048);
        let mut taps = PdcTaps::at(&PdcParams {
            output: 128,
            ..Default::default()
        });
        taps.output.aim(256, 64);
        assert_no_alloc(|| {
            pdc_line.write(&in_l, &in_r);
            pdc_line.read_tap(&mut taps.output, &mut out_l, &mut out_r);
            pdc_line.read(64, &mut out_l, &mut out_r);
        });
        let _ = Tap::at(0);
    }
}
