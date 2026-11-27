//! Marbble built-in instruments adapter for `prowave-engine` (`BlockProcessor`).
//!
//! Ported from Marbble (<https://github.com/JackMechem/marbble>),
//! Copyright (c) 2026 Jack Mechem, MIT License.

use crate::plugin::PluginInstrument;
use crate::plugin_track::{BlockProcessor, BlockTransport};
use marbble_dsp::instrument::{
    self, FxContext, Instrument, InstrumentInfo, MidiEvent, MAX_BLOCK, MAX_METERS,
};

const MAX_BLOCK_MIDI: usize = 256;

/// Wraps any of Marbble's built-in synthesizers (`Tre`, `Sei`, `Cinque`, `Uno`, `Sedici`, `Onda`)
/// as a zero-allocation [`BlockProcessor`].
pub struct MarbbleInstNode {
    info: InstrumentInfo,
    inst: Box<dyn Instrument>,
    ctx: FxContext,
    meters: [f32; MAX_METERS],
    midi_buf: [MidiEvent; MAX_BLOCK_MIDI],
    midi_len: usize,
    render_l: [f32; 64],
    render_r: [f32; 64],
    render_pos: usize,
}

impl MarbbleInstNode {
    /// Opens a Marbble instrument by id (e.g. `"builtin:tre"`, `"builtin:onda"`).
    /// Allocates buffers up front; call on a non-real-time thread.
    pub fn open(id: &str, sample_rate: f32, seed: u64) -> Option<Self> {
        let info = instrument::find(id)?;
        let inst = (info.new)(sample_rate, seed);
        Some(Self {
            info,
            inst,
            ctx: FxContext {
                sample_rate,
                tempo: 120.0,
                playing: false,
                position: 0.0,
            },
            meters: [0.0; MAX_METERS],
            midi_buf: [MidiEvent {
                offset: 0,
                data: [0; 3],
            }; MAX_BLOCK_MIDI],
            midi_len: 0,
            render_l: [0.0; 64],
            render_r: [0.0; 64],
            render_pos: 64,
        })
    }

    pub fn info(&self) -> &InstrumentInfo {
        &self.info
    }

    pub fn set_param_by_id(&mut self, id: u32, value: f32) {
        if let Some(spec) = self.info.param(id) {
            let clamped = if value.is_finite() {
                value.clamp(spec.min, spec.max)
            } else {
                spec.default
            };
            self.inst.set_param(id, clamped);
        }
    }

    pub fn param_by_id(&self, id: u32) -> f32 {
        self.inst.param(id)
    }

    pub fn meters(&self) -> &[f32; MAX_METERS] {
        &self.meters
    }

    pub fn all_notes_off(&mut self) {
        self.inst.all_notes_off();
    }

    pub fn reset(&mut self) {
        self.inst.reset();
        self.midi_len = 0;
    }

    pub fn tail_secs(&self) -> f32 {
        self.inst.tail_secs()
    }

    /// Swaps off-thread built instrument data (such as a `SamplerMap`) on the audio thread.
    pub fn swap_data(
        &mut self,
        data: marbble_dsp::instrument::InstrumentData,
    ) -> Option<marbble_dsp::instrument::InstrumentData> {
        self.inst.swap_data(data)
    }
}

impl BlockProcessor for MarbbleInstNode {
    fn midi(&mut self, status: u8, data1: u8, data2: u8, offset: u32) {
        if self.midi_len < MAX_BLOCK_MIDI {
            self.midi_buf[self.midi_len] = MidiEvent {
                offset,
                data: [status, data1, data2],
            };
            self.midi_len += 1;
        }
    }

    fn process(&mut self, _input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let n = out.0.len().min(out.1.len());
        if n == 0 {
            self.midi_len = 0;
            return;
        }
        // Single-chunk fast path (PLUGIN_BLOCK is 64 <= MAX_BLOCK 1024).
        let len = n.min(MAX_BLOCK);
        self.inst.process(
            &mut out.0[..len],
            &mut out.1[..len],
            &self.midi_buf[..self.midi_len],
            &self.ctx,
        );
        for chunk_start in (len..n).step_by(MAX_BLOCK) {
            let end = (chunk_start + MAX_BLOCK).min(n);
            self.inst.process(
                &mut out.0[chunk_start..end],
                &mut out.1[chunk_start..end],
                &[],
                &self.ctx,
            );
        }
        self.midi_len = 0;
        self.inst.meters(&mut self.meters);
    }

    fn latency_seconds(&self) -> f64 {
        0.0
    }

    fn set_parameter(&mut self, index: usize, value: f32) {
        if let Some(spec) = self.info.params.get(index) {
            let clamped = if value.is_finite() {
                value.clamp(spec.min, spec.max)
            } else {
                spec.default
            };
            self.inst.set_param(spec.id, clamped);
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

impl PluginInstrument for MarbbleInstNode {
    fn note_on(&mut self, pitch: u8, vel: u8) {
        self.midi(0x90, pitch & 127, vel.max(1) & 127, 0);
    }

    fn note_off(&mut self, pitch: u8) {
        self.midi(0x80, pitch & 127, 0, 0);
    }

    fn control(&mut self, number: u8, value: u16) {
        if number == 128 {
            let lsb = (value & 0x7F) as u8;
            let msb = ((value >> 7) & 0x7F) as u8;
            self.midi(0xE0, lsb, msb, 0);
        } else {
            self.midi(0xB0, number & 127, (value as u8) & 127, 0);
        }
    }

    fn all_notes_off(&mut self) {
        self.inst.all_notes_off();
    }

    fn render(&mut self) -> (f32, f32) {
        if self.render_pos >= 64 {
            let mut l = [0.0_f32; 64];
            let mut r = [0.0_f32; 64];
            self.process((&[], &[]), (&mut l, &mut r));
            self.render_l = l;
            self.render_r = r;
            self.render_pos = 0;
        }
        let i = self.render_pos;
        self.render_pos += 1;
        (self.render_l[i], self.render_r[i])
    }

    fn set_param(&mut self, index: usize, norm: f32) {
        if let Some(spec) = self.info.params.get(index) {
            let n = if norm.is_finite() {
                norm.clamp(0.0, 1.0)
            } else {
                0.0
            };
            let v = if spec.log && spec.min > 0.0 {
                spec.min * (spec.max / spec.min).powf(n)
            } else {
                spec.min + (spec.max - spec.min) * n
            };
            self.inst.set_param(spec.id, v.clamp(spec.min, spec.max));
        }
    }

    fn reset(&mut self) {
        self.inst.reset();
        self.midi_len = 0;
        self.render_pos = 64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_no_alloc::assert_no_alloc;

    #[test]
    fn all_6_marbble_synths_are_allocation_free_in_process() {
        let sr = 48_000.0;
        let reg = instrument::registry();
        assert_eq!(reg.len(), 7);

        let mut out_l = [0.0_f32; 256];
        let mut out_r = [0.0_f32; 256];

        for info in &reg {
            let Some(mut node) = MarbbleInstNode::open(info.id, sr, 42) else {
                panic!("failed to open marbble instrument {}", info.id);
            };
            assert_no_alloc(|| {
                node.midi(0x90, 60, 100, 0);
                node.set_parameter(0, info.params.first().map_or(0.0, |p| p.default));
                node.process((&[], &[]), (&mut out_l, &mut out_r));
            });
            assert!(out_l.iter().all(|x| x.is_finite()));
            assert!(out_r.iter().all(|x| x.is_finite()));
            let energy: f32 = out_l.iter().chain(&out_r).map(|x| x * x).sum();
            if info.id != "builtin:sampler" {
                assert!(energy > 0.0, "{} produced silence on NoteOn", info.id);
            }
        }
    }
}
