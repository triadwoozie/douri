// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.
//
// The DSP in this module is a Rust port of the granular engine of Mutable Instruments'
// Beads texture synthesizer (github.com/pichenettes/eurorack, beads/dsp, commit f4cfa76,
// with stmlib at e3bd7c9), whose code is released under the MIT licence:
//
//   Copyright 2014-2021 Emilie Gillet.
//
//   Author: Emilie Gillet (emilie.o.gillet@gmail.com)
//
//   Permission is hereby granted, free of charge, to any person obtaining a copy
//   of this software and associated documentation files (the "Software"), to deal
//   in the Software without restriction, including without limitation the rights
//   to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//   copies of the Software, and to permit persons to whom the Software is
//   furnished to do so, subject to the following conditions:
//
//   The above copyright notice and this permission notice shall be included in
//   all copies or substantial portions of the Software.
//
//   THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//   IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//   FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//   AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//   LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//   OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
//   THE SOFTWARE.
//
//   See http://creativecommons.org/licenses/MIT/ for more information.
//
// "Mutable Instruments" and "Beads" are the maker's names and are not used for this effect.
// The module's hardware and panel designs (CC BY-SA 3.0) are not used.

//! Pro Wave Texture: a granular texture effect, a port of the granular engine of Mutable
//! Instruments' Beads.
//!
//! Logic Pro equivalent: none built in (Logic's granular sound comes from Alchemy's granular
//! source, an instrument). The input is recorded into a buffer; grains read it back at TIME
//! (how far back), SIZE (length; below a third, reversed), SHAPE (envelope) and PITCH, started
//! by DENSITY (regular below noon, random above, none at noon). Each of the four grain
//! settings has a RANDOM amount. FEEDBACK sends the grains back into the buffer, REVERB is
//! the module's own reverb, FREEZE stops recording. SIZE fully clockwise turns the grains
//! into the module's delay mode. QUALITY picks one of the module's four buffer formats
//! (rate, bits, length; the dirty tape adds wow and flutter).
//!
//! The engine ([`processor::Processor`]) runs at the module's own rate for the quality mode
//! (48, 32 or 24 kHz) in blocks of 1/4000 s, as the firmware does; [`resample`] converts to
//! and from the project's rate. The whole effect delays the sound by a fixed number of
//! frames ([`Texture::latency_samples`]), reported for delay compensation. Its random
//! numbers start from a fixed seed, so a bounce of the same input is the same every time.
//!
//! Not ported: the module's synth mode (grains of an internal wavetable when nothing is
//! patched), the SEED button and input (grains are seeded by DENSITY alone, the module's
//! power-on "locked" state), the CV inputs, the mono buffer layout, the input gain control
//! and the gate output.

pub mod delay_line;
pub mod delay_mode;
pub mod dsp;
pub mod grain;
mod luts;
pub mod processor;
pub mod resample;
pub mod space;
#[cfg(test)]
mod tests;
pub mod trigger;

use super::{stored_values, NativeFx, NativeParam, NativePreset};
use dsp::{constrain, interpolate_detune, Frame, HysteresisFilter};
use grain::{GranularPlayer, Parameters, RandomizedParameter};
use processor::{Media, Processor, Settings, MAX_BLOCK};
use resample::Resampler;
use trigger::SeedState;

pub const ID: &str = "prowave:texture";
pub const NAME: &str = "Pro Wave Texture";

// The parameters, in saved order.
pub const TIME: usize = 0;
pub const SIZE: usize = 1;
pub const SHAPE: usize = 2;
pub const PITCH: usize = 3;
pub const TIME_RANDOM: usize = 4;
pub const SIZE_RANDOM: usize = 5;
pub const SHAPE_RANDOM: usize = 6;
pub const PITCH_RANDOM: usize = 7;
pub const DENSITY: usize = 8;
pub const FEEDBACK: usize = 9;
pub const DRY_WET: usize = 10;
pub const REVERB: usize = 11;
pub const FREEZE: usize = 12;
pub const QUALITY: usize = 13;
pub const COUNT: usize = 14;

/// The QUALITY choices, in the parameter's order.
pub const QUALITIES: [&str; 4] = ["Digital", "Vintage", "Clean Tape", "Dirty Tape"];
const QUALITY_MEDIA: [Media; 4] = [
    Media::Digital,
    Media::VintageDigital,
    Media::CleanTape,
    Media::DirtyTape,
];

/// The seed of the grains' random numbers (stmlib's own starting state).
pub const SEED: u32 = dsp::Random::DEFAULT_SEED;

/// How long a QUALITY change fades out and back in, in seconds.
const SWITCH_FADE_SECONDS: f32 = 0.005;

const fn knob(
    name: &'static str,
    min: i32,
    max: i32,
    default: i32,
    unit: &'static str,
) -> NativeParam {
    NativeParam::knob(name, min, max, default, unit)
}

/// Knobs go 0..1000 (or -1000..1000 for the bipolar ones): the module's pot positions in
/// steps of 0.001.
pub static PARAMS: [NativeParam; COUNT] = [
    knob("Time", 0, 1000, 300, "%"),
    knob("Size", 0, 1000, 700, "ms"),
    knob("Shape", 0, 1000, 500, "%"),
    knob("Pitch", -1000, 1000, 0, "st"),
    knob("Time Random", -1000, 1000, 0, "%"),
    knob("Size Random", -1000, 1000, 0, "%"),
    knob("Shape Random", -1000, 1000, 0, "%"),
    knob("Pitch Random", -1000, 1000, 0, "%"),
    knob("Density", 0, 1000, 300, "Hz"),
    knob("Feedback", 0, 1000, 0, "%"),
    knob("Dry/Wet", 0, 1000, 500, "%"),
    knob("Reverb", 0, 1000, 200, "%"),
    NativeParam::choice("Freeze", 0, 0, &["Off", "On"]),
    NativeParam::choice("Quality", 0, 0, &QUALITIES),
];

/// The presets. FREEZE is left as it is; only Tape Wash changes QUALITY.
pub static PRESETS: [NativePreset; 6] = [
    NativePreset {
        name: "Cloud",
        values: &[
            (TIME, 400.0),
            (SIZE, 650.0),
            (SHAPE, 600.0),
            (PITCH, 0.0),
            (TIME_RANDOM, 500.0),
            (SIZE_RANDOM, 300.0),
            (SHAPE_RANDOM, 0.0),
            (PITCH_RANDOM, 0.0),
            (DENSITY, 780.0),
            (FEEDBACK, 300.0),
            (DRY_WET, 600.0),
            (REVERB, 500.0),
        ],
    },
    NativePreset {
        name: "Octave Shimmer",
        values: &[
            (TIME, 300.0),
            (SIZE, 750.0),
            (SHAPE, 550.0),
            (PITCH, 850.0),
            (TIME_RANDOM, 300.0),
            (SIZE_RANDOM, 0.0),
            (SHAPE_RANDOM, 0.0),
            (PITCH_RANDOM, 0.0),
            (DENSITY, 250.0),
            (FEEDBACK, 400.0),
            (DRY_WET, 500.0),
            (REVERB, 700.0),
        ],
    },
    NativePreset {
        name: "Reverse Grains",
        values: &[
            (TIME, 500.0),
            (SIZE, 150.0),
            (SHAPE, 700.0),
            (PITCH, 0.0),
            (TIME_RANDOM, 200.0),
            (SIZE_RANDOM, 0.0),
            (SHAPE_RANDOM, 0.0),
            (PITCH_RANDOM, 0.0),
            (DENSITY, 300.0),
            (FEEDBACK, 200.0),
            (DRY_WET, 600.0),
            (REVERB, 300.0),
        ],
    },
    NativePreset {
        name: "Scatter",
        values: &[
            (TIME, 500.0),
            (SIZE, 450.0),
            (SHAPE, 300.0),
            (PITCH, 0.0),
            (TIME_RANDOM, 1000.0),
            (SIZE_RANDOM, 600.0),
            (SHAPE_RANDOM, 400.0),
            (PITCH_RANDOM, -600.0),
            (DENSITY, 900.0),
            (FEEDBACK, 0.0),
            (DRY_WET, 700.0),
            (REVERB, 250.0),
        ],
    },
    NativePreset {
        name: "Stutter Delay",
        values: &[
            (TIME, 0.0),
            (SIZE, 1000.0),
            (SHAPE, 200.0),
            (PITCH, 0.0),
            (TIME_RANDOM, 0.0),
            (SIZE_RANDOM, 0.0),
            (SHAPE_RANDOM, 0.0),
            (PITCH_RANDOM, 0.0),
            (DENSITY, 250.0),
            (FEEDBACK, 400.0),
            (DRY_WET, 500.0),
            (REVERB, 100.0),
        ],
    },
    NativePreset {
        name: "Tape Wash",
        values: &[
            (TIME, 600.0),
            (SIZE, 750.0),
            (SHAPE, 600.0),
            (PITCH, 0.0),
            (TIME_RANDOM, 300.0),
            (SIZE_RANDOM, 200.0),
            (SHAPE_RANDOM, 0.0),
            (PITCH_RANDOM, 0.0),
            (DENSITY, 700.0),
            (FEEDBACK, 500.0),
            (DRY_WET, 700.0),
            (REVERB, 600.0),
            (QUALITY, 3.0),
        ],
    },
];

/// The module's attenuverter pot law: position -1..1 to an amount -1..1 with a dead zone
/// in the middle.
pub fn attenuverter_law(position: f32) -> f32 {
    let v = 0.5 * position;
    (2.0 / 0.47) * v * (v.abs() - 0.03).max(0.0)
}

/// The module's PITCH pot law: position -1..1 to semitones, resting on unison, minor
/// second, minor and major third, fifth, octave and two octaves.
pub fn pitch_law(position: f32) -> f32 {
    interpolate_detune(position)
}

/// The QUALITY mode of a stored value.
pub fn media_of(v: i32) -> Media {
    QUALITY_MEDIA[v.clamp(0, 3) as usize]
}

/// The grain length for SIZE `v` (0..1000), in seconds; negative when grains play reversed;
/// `None` in delay mode.
pub fn grain_seconds(v: i32) -> Option<f32> {
    let size = v as f32 / 1000.0;
    if size > 0.99 {
        return None;
    }
    // The length does not depend on the rate: 4 s at most, in samples of any rate.
    let s = GranularPlayer::grain_duration(4.0, size);
    Some(if size < 0.333_333_3 { -s } else { s })
}

/// DENSITY `v` (0..1000) as (grains per second, random); `None` at noon (no grains).
pub fn density_rate(v: i32) -> Option<(f32, bool)> {
    let d = v as f32 / 1000.0;
    let max = 130.812_78f32;
    if d < 0.48 {
        Some((max * dsp::semitones_to_ratio(-d * 192.0), false))
    } else if d > 0.52 {
        Some((
            0.5 * max * dsp::semitones_to_ratio(-(1.0 - d) * 192.0),
            true,
        ))
    } else {
        None
    }
}

/// In the delay mode (SIZE fully clockwise): the time between repeats, in seconds, and the
/// multiplier TIME picks, for QUALITY `quality`, DENSITY `density` and TIME `time` (stored
/// values), as the module's free-running delay clock sets them.
pub fn delay_repeat(quality: i32, density: i32, time: i32) -> (f32, i32) {
    let m = media_of(quality);
    let rate = m.sample_rate() as f32;
    let max_delay = m.buffer_seconds() * rate;
    let d = density as f32 / 1000.0;
    let interval = (68.0f32).max(max_delay * dsp::semitones_to_ratio(-(d - 0.5).abs() * 216.0));
    let multiplier = constrain(1.005 * time as f32 / 1000.0, 0.0, 1.0) * 15.0 + 1.0;
    let multiplier = constrain(multiplier, 1.0, 16.0) as i32;
    let delay = (interval * multiplier as f32).min(max_delay);
    (delay / rate, multiplier)
}

/// The grain envelope SHAPE `v` (0..1000) gives a grain of `n` points, for drawing.
pub fn envelope_points(v: i32, n: usize) -> Vec<f32> {
    let mut e = grain::GrainEnvelope::default();
    let rate = 48000.0;
    e.init(rate);
    // A long grain, so the minimum attack and release do not show.
    let len = rate * 2.0;
    e.start(len, constrain(v as f32 / 1000.0, 0.0, 1.0));
    let step = (len as usize / n.max(1)).max(1);
    let mut out = Vec::with_capacity(n);
    for i in 0..len as usize {
        let x = e.render();
        if i % step == 0 && out.len() < n {
            out.push(x);
        }
    }
    out
}

pub fn value_text(k: usize, v: i32) -> String {
    let pct = |v: i32| format!("{}%", (v as f32 / 10.0).round() as i32);
    match k {
        SIZE => match grain_seconds(v) {
            None => "Delay".into(),
            Some(s) if s.abs() < 1.0 => format!("{:.0} ms", s * 1000.0),
            Some(s) => format!("{s:.2} s"),
        },
        PITCH => {
            let st = pitch_law(v as f32 / 1000.0);
            if st.abs() < 0.05 {
                "0.0 st".into()
            } else {
                format!("{st:+.1} st")
            }
        }
        TIME_RANDOM | SIZE_RANDOM | SHAPE_RANDOM | PITCH_RANDOM => {
            let a = (attenuverter_law(v as f32 / 1000.0) * 100.0).round() as i32;
            if a == 0 {
                "0%".into()
            } else {
                format!("{a:+}%")
            }
        }
        DENSITY => match density_rate(v) {
            None => "Off".into(),
            Some((hz, random)) => {
                let unit = if random { "RND" } else { "Hz" };
                if hz < 10.0 {
                    format!("{hz:.2} {unit}")
                } else {
                    format!("{hz:.1} {unit}")
                }
            }
        },
        FREEZE => if v != 0 { "On" } else { "Off" }.into(),
        QUALITY => QUALITIES[v.clamp(0, 3) as usize].into(),
        _ => pct(v),
    }
}

/// One knob as the module reads it: the pot law, then a one-pole low-pass per block
/// (`PotChannel`, coefficient 0.05 at 4000 blocks a second) and an optional hysteresis.
#[derive(Clone, Copy, Debug, Default)]
struct Pot {
    lp: f32,
    hysteresis: HysteresisFilter,
}

impl Pot {
    fn new(value: f32, hysteresis: f32) -> Self {
        let mut h = HysteresisFilter::default();
        h.init(hysteresis);
        h.process(value);
        Self {
            lp: value,
            hysteresis: h,
        }
    }
    fn step(&mut self, target: f32) -> f32 {
        dsp::one_pole(&mut self.lp, target, 0.05);
        self.hysteresis.process(self.lp)
    }
}

/// The pots in parameter order (the first twelve parameters).
const POTS: usize = 12;

/// Pro Wave Texture, ready for an effect chain.
pub struct Texture {
    rate: u32,
    values: [i32; COUNT],
    processor: Processor,
    /// The QUALITY the next fade-out switches to.
    pending: Option<Media>,
    /// Output gain while switching QUALITY, and its step per frame (negative: fading out).
    switch_gain: f32,
    switch_step: f32,
    /// Project rate to engine rate and back, one pair per engine rate (48, 32, 24 kHz).
    down: [Resampler; 3],
    up: [Resampler; 3],
    /// Engine-rate input waiting for a full block.
    block_in: [Frame; MAX_BLOCK],
    block_len: usize,
    /// Project-rate output, a ring.
    ring: Vec<Frame>,
    ring_read: usize,
    ring_len: usize,
    latency: usize,
    pots: [Pot; POTS],
    /// Frames played as silence because the output was not ready (never, by design).
    pub(crate) underruns: u64,
}

fn rate_index(m: Media) -> usize {
    match m.sample_rate() {
        48000 => 0,
        32000 => 1,
        _ => 2,
    }
}

/// Parameter `k`'s pot value after its law, before smoothing.
fn pot_target(values: &[i32; COUNT], k: usize) -> f32 {
    let v = values[k] as f32 / 1000.0;
    match k {
        PITCH => pitch_law(v),
        TIME_RANDOM | SIZE_RANDOM | SHAPE_RANDOM | PITCH_RANDOM => attenuverter_law(v),
        _ => v,
    }
}

/// The settings of the next engine block (the pots move one step).
fn next_settings(pots: &mut [Pot; POTS], values: &[i32; COUNT]) -> Settings {
    let mut v = [0.0f32; POTS];
    for (k, x) in v.iter_mut().enumerate() {
        *x = pots[k].step(pot_target(values, k));
    }
    let rp = |base: f32, amount: f32| RandomizedParameter {
        base,
        attenuverter: amount,
    };
    let mix = |x: f32| constrain(x * 1.005, 0.0, 1.0);
    Settings {
        density: v[DENSITY],
        params: Parameters {
            time: rp(v[TIME], v[TIME_RANDOM]),
            size: rp(v[SIZE], v[SIZE_RANDOM]),
            shape: rp(v[SHAPE], v[SHAPE_RANDOM]),
            pitch: rp(v[PITCH], v[PITCH_RANDOM]),
        },
        feedback: mix(v[FEEDBACK]),
        dry_wet: mix(v[DRY_WET]),
        reverb: mix(v[REVERB]),
    }
}

impl Texture {
    /// Allocates everything (call off the audio thread).
    pub fn new(sample_rate: u32, values: &[f32]) -> Self {
        let rate = sample_rate.max(1);
        let values = stored_values(&PARAMS, values);
        let media = media_of(values[QUALITY]);
        let rates = [48000u32, 32000, 24000];
        let down = rates.map(|r| Resampler::new(rate, r));
        let up = rates.map(|r| Resampler::new(r, rate));
        // The fixed delay: the longest any quality mode needs for its output to be ready.
        let need = Media::ALL
            .iter()
            .map(|m| {
                let i = rate_index(*m);
                down[i].lag_seconds()
                    + up[i].lag_seconds()
                    + (m.block_size() as f64 - 1.0) / f64::from(m.sample_rate())
            })
            .fold(0.0f64, f64::max);
        let latency = (need * f64::from(rate)).ceil() as usize + 2;
        let ring_size =
            (latency + 4 * MAX_BLOCK * (rate as usize / 8000 + 1) + 64).next_power_of_two();
        let mut t = Self {
            rate,
            values,
            processor: Processor::new(media, SEED),
            pending: None,
            switch_gain: 1.0,
            switch_step: 0.0,
            down,
            up,
            block_in: [Frame::ZERO; MAX_BLOCK],
            block_len: 0,
            ring: vec![Frame::ZERO; ring_size],
            ring_read: 0,
            ring_len: 0,
            latency,
            pots: [Pot::default(); POTS],
            underruns: 0,
        };
        for k in 0..POTS {
            let target = pot_target(&t.values, k);
            t.pots[k] = Pot::new(target, if k == TIME { 0.001 } else { 0.0 });
        }
        t.prime();
        t
    }

    /// The delay the effect adds, in project frames.
    pub fn latency_samples(&self) -> usize {
        self.latency
    }

    /// The QUALITY mode playing now.
    pub fn media(&self) -> Media {
        self.processor.media()
    }

    /// Empties the pipeline and fills the output with the fixed delay's silence.
    fn prime(&mut self) {
        self.block_len = 0;
        self.ring_read = 0;
        self.ring_len = self.latency.min(self.ring.len());
        for f in self.ring.iter_mut() {
            *f = Frame::ZERO;
        }
        for r in self.down.iter_mut().chain(self.up.iter_mut()) {
            r.reset();
        }
    }

    fn switch_now(&mut self, media: Media) {
        self.processor.configure(media);
        self.prime();
    }

    pub(crate) fn run(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let fade_step = 1.0 / (SWITCH_FADE_SECONDS * self.rate as f32).max(1.0);
        for i in 0..n {
            // A QUALITY change: fade out, switch (the buffer empties), fade in.
            if let Some(m) = self.pending {
                if self.switch_step >= 0.0 && self.switch_gain > 0.0 {
                    self.switch_step = -fade_step;
                }
                if self.switch_gain <= 0.0 {
                    self.pending = None;
                    self.switch_now(m);
                    self.switch_gain = 0.0;
                    self.switch_step = fade_step;
                }
            }
            let ri = rate_index(self.processor.media());
            let x = Frame::new(l[i], r[i]);
            let block = self.processor.media().block_size();
            let freeze_on = self.values[FREEZE] != 0;
            // Split the borrows: the closures below fill the block, run it and fill the ring.
            let Self {
                down,
                up,
                processor,
                block_in,
                block_len,
                ring,
                ring_read,
                ring_len,
                pots,
                values,
                ..
            } = self;
            let mask = ring.len() - 1;
            down[ri].push(x, |f| {
                block_in[*block_len] = f;
                *block_len += 1;
                if *block_len < block {
                    return;
                }
                *block_len = 0;
                let settings = next_settings(pots, values);
                let freeze = [freeze_on; MAX_BLOCK];
                let mut out = [Frame::ZERO; MAX_BLOCK];
                processor.process(
                    SeedState::Locked,
                    &freeze[..block],
                    &settings,
                    &block_in[..block],
                    &mut out[..block],
                );
                for f in &out[..block] {
                    up[ri].push(*f, |y| {
                        if *ring_len < ring.len() {
                            ring[(*ring_read + *ring_len) & mask] = y;
                            *ring_len += 1;
                        }
                    });
                }
            });
            let y = if self.ring_len > 0 {
                let y = self.ring[self.ring_read];
                self.ring_read = (self.ring_read + 1) & (self.ring.len() - 1);
                self.ring_len -= 1;
                y
            } else {
                self.underruns += 1;
                Frame::ZERO
            };
            let g = self.switch_gain;
            self.switch_gain = (self.switch_gain + self.switch_step).clamp(0.0, 1.0);
            if self.switch_gain >= 1.0 {
                self.switch_step = 0.0;
            }
            l[i] = y.l * g;
            r[i] = y.r * g;
        }
    }
}

super::block_effect!(Texture);

impl NativeFx for Texture {
    fn set_param(&mut self, index: usize, value: f32) {
        let Some(p) = PARAMS.get(index) else { return };
        let v = p.stored(value);
        let before = self.values[index];
        self.values[index] = v;
        if index == QUALITY && v != before {
            let m = media_of(v);
            self.pending = (m != self.processor.media()).then_some(m);
            if self.pending.is_none() {
                // Changed back before the switch: fade back in.
                self.switch_step = 1.0 / (SWITCH_FADE_SECONDS * self.rate as f32).max(1.0);
            }
        }
    }

    fn param(&self, index: usize) -> f32 {
        self.values.get(index).map_or(0.0, |v| *v as f32)
    }
}
