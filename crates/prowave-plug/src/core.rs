// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! The engine inside a plug-in, without the plug-in framework: an instrument (`Hosted`, which
//! takes MIDI messages at their frames) or an effect (`NativeFx`), its parameters as the host
//! last set them, and the buffers it needs, all allocated when it is opened. `process` runs on
//! the host's audio thread: it allocates nothing, takes no lock and does not panic.

use prowave_engine::native_inst::Hosted;
use prowave_engine::{
    open_native, open_native_instrument, BlockProcessor, BlockTransport, NativeFx, NativeFxInfo,
};

/// Frames the engine is given at a time: a host block is cut into pieces of at most this many.
pub const CHUNK: usize = 256;
/// MIDI messages one host block can hold; more are dropped.
pub const MAX_EVENTS: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Midi {
    offset: u32,
    status: u8,
    d1: u8,
    d2: u8,
}

enum Engine {
    Instrument(Hosted),
    Effect(Box<dyn NativeFx>),
}

pub struct PlugCore {
    info: &'static NativeFxInfo,
    engine: Engine,
    rate: u32,
    /// The stored values the engine has.
    applied: Vec<i32>,
    events: Vec<Midi>,
    /// An effect's input, copied out of the host's buffer (the engine writes over it).
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    /// The right channel of a mono layout.
    spare: Vec<f32>,
}

impl PlugCore {
    /// Opens `info`'s engine at `rate` for host blocks of up to `max_block` frames, with the
    /// stored `values` (missing ones are the defaults). Allocates: not on the audio thread.
    /// A parameter that sizes the effect's memory (Press's oversampling) is opened at its
    /// largest and then set, so the host can raise it again later.
    pub fn open(
        info: &'static NativeFxInfo,
        instrument: bool,
        rate: u32,
        max_block: usize,
        values: &[i32],
    ) -> Option<Self> {
        let mut stored: Vec<f32> = info
            .params
            .iter()
            .enumerate()
            .map(|(k, p)| values.get(k).map_or(p.default, |v| *v as f32))
            .collect();
        let engine = if instrument {
            Engine::Instrument(open_native_instrument(info.id, rate, &stored)?)
        } else {
            let wanted = stored.clone();
            for (k, p) in info.params.iter().enumerate() {
                if !p.live {
                    stored[k] = p.max;
                }
            }
            let mut fx = open_native(info.id, rate, &stored)?;
            for (k, p) in info.params.iter().enumerate() {
                if !p.live {
                    fx.set_param(k, wanted[k]);
                }
            }
            stored = wanted;
            Engine::Effect(fx)
        };
        Some(PlugCore {
            info,
            engine,
            rate: rate.max(1),
            applied: stored.iter().map(|v| *v as i32).collect(),
            events: Vec::with_capacity(MAX_EVENTS),
            in_l: vec![0.0; CHUNK],
            in_r: vec![0.0; CHUNK],
            spare: vec![0.0; max_block.max(1)],
        })
    }

    pub fn info(&self) -> &'static NativeFxInfo {
        self.info
    }

    /// Sets parameter `k` to the stored value `v` when the engine has another.
    pub fn set_param(&mut self, k: usize, v: i32) {
        let Some(old) = self.applied.get_mut(k) else {
            return;
        };
        if *old == v {
            return;
        }
        *old = v;
        match &mut self.engine {
            Engine::Instrument(h) => h.set_parameter(k, v as f32),
            Engine::Effect(fx) => fx.set_param(k, v as f32),
        }
    }

    /// A MIDI message `offset` frames into the next block (an effect has no use for them).
    pub fn midi(&mut self, status: u8, d1: u8, d2: u8, offset: u32) {
        if self.events.len() < self.events.capacity() {
            self.events.push(Midi {
                offset,
                status,
                d1,
                d2,
            });
        }
    }

    /// Silence at once: an instrument's voices stop (as controller 120).
    pub fn silence(&mut self) {
        self.events.clear();
        if let Engine::Instrument(h) = &mut self.engine {
            h.midi(0xb0, 120, 0, 0);
        }
    }

    /// Where the host's transport stood at the start of the next block.
    pub fn transport(&mut self, t: &BlockTransport) {
        if let Engine::Effect(fx) = &mut self.engine {
            fx.block_transport(t);
        }
    }

    /// The delay the engine adds, in frames.
    pub fn latency_samples(&self) -> u32 {
        let s = match &self.engine {
            Engine::Instrument(h) => h.latency_seconds(),
            Engine::Effect(fx) => fx.latency_seconds(),
        };
        (s * f64::from(self.rate)).round().max(0.0) as u32
    }

    /// One host block in place: an effect's input is in `l` and `r` and its output goes over
    /// it; an instrument writes over both. The MIDI messages given since the last block are
    /// played at their frames.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let Self {
            engine,
            events,
            in_l,
            in_r,
            ..
        } = self;
        let mut next = 0;
        let mut at = 0;
        while at < n {
            let end = (at + CHUNK).min(n);
            let (cl, cr) = (&mut l[at..end], &mut r[at..end]);
            match engine {
                Engine::Instrument(h) => {
                    // Messages for this piece; a late one (past the block) goes with the last.
                    while next < events.len() && ((events[next].offset as usize) < end || end == n)
                    {
                        let e = events[next];
                        let offset = (e.offset as usize).clamp(at, end - 1) - at;
                        h.midi(e.status, e.d1, e.d2, offset as u32);
                        next += 1;
                    }
                    h.process((&[], &[]), (cl, cr));
                }
                Engine::Effect(fx) => {
                    let m = end - at;
                    in_l[..m].copy_from_slice(cl);
                    in_r[..m].copy_from_slice(cr);
                    fx.process((&in_l[..m], &in_r[..m]), (&mut *cl, &mut *cr));
                    // A piece that went wrong is not heard (as the program's effect chain).
                    if cl.iter().chain(cr.iter()).any(|v| !v.is_finite()) {
                        cl.fill(0.0);
                        cr.fill(0.0);
                    }
                }
            }
            at = end;
        }
        events.clear();
    }

    /// One host block of a mono layout: the effect hears `x` on both sides; what comes out is
    /// the mean of its two sides.
    pub fn process_mono(&mut self, x: &mut [f32]) {
        let n = x.len().min(self.spare.len());
        let mut spare = std::mem::take(&mut self.spare);
        spare[..n].copy_from_slice(&x[..n]);
        self.process(&mut x[..n], &mut spare[..n]);
        for (a, b) in x[..n].iter_mut().zip(&spare[..n]) {
            *a = 0.5 * (*a + *b);
        }
        self.spare = spare;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every() -> impl Iterator<Item = (&'static NativeFxInfo, bool)> {
        prowave_engine::native_instruments()
            .iter()
            .map(|i| (i, true))
            .chain(prowave_engine::native_effects().iter().map(|i| (i, false)))
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    /// Every plug-in's engine through this wrapper's process path, with the allocation checker
    /// nih-plug installs counting: parameters moved to their ends and back, notes, the pedal,
    /// the wheel and both "all off" controllers for instruments, loud noise for effects, in
    /// blocks of odd sizes up to 4096 frames, mono and stereo. The output is finite, and not
    /// silent while a note sounds or the noise plays.
    #[test]
    fn every_plugin_runs_without_allocating() {
        // The checker is live in this test binary: an allocation inside it is counted.
        assert_no_alloc::reset_violation_count();
        assert_no_alloc::assert_no_alloc(|| {
            std::hint::black_box(vec![1u8; 100]);
        });
        assert!(
            assert_no_alloc::violation_count() >= 1,
            "the checker is off"
        );
        let rate = 48_000u32;
        let max_block = 4096;
        for (info, instrument) in every() {
            assert_no_alloc::reset_violation_count();
            let values: Vec<i32> = info.params.iter().map(|p| p.default as i32).collect();
            let Some(mut core) = PlugCore::open(info, instrument, rate, max_block, &values) else {
                panic!("{} does not open", info.id)
            };
            let mut l = vec![0.0f32; max_block];
            let mut r = vec![0.0f32; max_block];
            let mut seed = 7u32;
            let mut noise = move || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as f32 / u32::MAX as f32 - 0.5
            };
            let blocks = [512usize, 333, 4096, 64, 1000, 1];
            let mut loudest = 0.0f32;
            let mut frames = 0usize;
            for (b, &n) in blocks.iter().cycle().take(40).enumerate() {
                if !instrument {
                    for i in 0..n {
                        l[i] = noise();
                        r[i] = noise();
                    }
                }
                assert_no_alloc::assert_no_alloc(|| {
                    if b == 10 {
                        for (k, p) in info.params.iter().enumerate() {
                            core.set_param(k, p.max as i32);
                        }
                    }
                    if b == 20 {
                        for (k, p) in info.params.iter().enumerate() {
                            core.set_param(k, p.min as i32);
                        }
                    }
                    if b == 30 {
                        for (k, p) in info.params.iter().enumerate() {
                            core.set_param(k, p.default as i32);
                        }
                    }
                    if instrument {
                        match b % 10 {
                            0 => {
                                // 36 is the Drum Synth's first pad.
                                core.midi(0x90, 36, 110, 0);
                                core.midi(0x90, 60, 100, 0);
                                core.midi(0x90, 64, 90, (n / 2) as u32);
                            }
                            3 => core.midi(0xe0, 0, 100, 0),
                            5 => core.midi(0xb0, 64, 127, 0),
                            6 => core.midi(0x80, 60, 0, 0),
                            7 => core.midi(0xb0, 64, 0, (n.saturating_sub(1)) as u32),
                            8 => core.midi(0xb0, 123, 0, 0),
                            9 => core.midi(0xb0, 120, 0, 0),
                            _ => {}
                        }
                    }
                    core.transport(&BlockTransport {
                        playing: true,
                        sample_pos: frames as i64,
                        ..BlockTransport::default()
                    });
                    if b % 7 == 3 {
                        core.process_mono(&mut l[..n]);
                    } else {
                        core.process(&mut l[..n], &mut r[..n]);
                    }
                    let _ = core.latency_samples();
                });
                assert!(
                    l[..n].iter().chain(&r[..n]).all(|v| v.is_finite()),
                    "{} block {b}",
                    info.id
                );
                if b < 10 {
                    loudest = loudest.max(peak(&l[..n])).max(peak(&r[..n]));
                }
                frames += n;
            }
            assert!(loudest > 1e-3, "{}: silent ({loudest})", info.id);
            assert_eq!(
                assert_no_alloc::violation_count(),
                0,
                "{}: allocation on the audio thread",
                info.id
            );
        }
    }

    /// An effect's latency is what its engine reports, at the plug-in's rate.
    #[test]
    fn latency_follows_the_engine() {
        let info = prowave_engine::native_info(prowave_engine::tune::ID).expect("tune");
        let core = PlugCore::open(info, false, 48_000, 512, &[]).expect("opens");
        assert_eq!(
            core.latency_samples(),
            (prowave_engine::tune::LATENCY_SECONDS * 48_000.0).round() as u32
        );
    }
}
