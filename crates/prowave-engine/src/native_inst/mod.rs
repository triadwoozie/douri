// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Native instruments: synthesizers that run inside the engine, listed in one table the way
//! the native effects are ([`crate::native_fx`]).
//!
//! Logic Pro equivalent: the built-in software instruments of the Instrument slot (Retro
//! Synth, ES2, Sculpture and the rest).
//!
//! Each instrument is listed in [`native_instruments`] with its parameters, and made with
//! [`open_native_instrument`] on a non-real-time thread (all of its memory is allocated
//! there). It is then a [`BlockProcessor`] for a [`crate::PluginTrack`]'s instrument slot.
//! An instrument itself is a [`NativeInst`]: notes in, audio out. What every instrument needs
//! around that is here once: the MIDI queue that cuts a block at each message ([`Hosted`]),
//! voice allocation ([`voices::Voices`]) and running a fixed-rate engine at the project's rate
//! ([`rate::FixedRate`]).
//!
//! Parameters are whole numbers, as the native effects' are, and the entry type is the same
//! ([`NativeInstInfo`] is [`NativeFxInfo`]), so the app's panels, automation and control
//! interface treat both alike.

pub mod bass_line;
#[cfg(not(target_arch = "wasm32"))]
mod chip;
#[cfg(not(target_arch = "wasm32"))]
pub mod chip_opl;
#[cfg(not(target_arch = "wasm32"))]
pub mod chip_opll;
#[cfg(not(target_arch = "wasm32"))]
pub mod chip_opn;
#[cfg(not(target_arch = "wasm32"))]
pub mod chip_psg;
#[cfg(not(target_arch = "wasm32"))]
pub mod chip_sid;
#[cfg(not(target_arch = "wasm32"))]
pub mod digital_osc;
pub mod envelope;
#[cfg(not(target_arch = "wasm32"))]
pub mod fm6;
pub mod kick;
pub mod macro_osc;
pub mod pluck;
pub mod rate;
#[cfg(not(target_arch = "wasm32"))]
pub mod resonator;
pub mod triple_osc;
pub mod voices;
pub mod wave_draw;

use crate::native_fx::{NativeFxInfo, NativeParam};
use crate::plugin_track::BlockProcessor;
use std::sync::OnceLock;

/// A native instrument's entry in the menu.
pub type NativeInstInfo = NativeFxInfo;

/// Semitones a full pitch-bend moves a note.
pub const BEND_SEMITONES: f32 = 2.0;

/// A native instrument: notes in, audio out. Nothing here may allocate, lock or panic: all of
/// it runs on the audio thread.
pub trait NativeInst: Send {
    /// Sets parameter `index` (a stored value, rounded and kept in range). Unknown indices
    /// are ignored.
    fn set_param(&mut self, index: usize, value: f32);
    /// Parameter `index`'s value in use (0 for an unknown index).
    fn param(&self, index: usize) -> f32;
    fn note_on(&mut self, note: u8, velocity: u8);
    fn note_off(&mut self, note: u8);
    /// Every held note let go (controller 123).
    fn all_notes_off(&mut self);
    /// Silence at once (controller 120).
    fn all_sound_off(&mut self);
    /// The pitch wheel, in semitones.
    fn bend(&mut self, _semitones: f32) {}
    /// The next frames, written over `left` and `right` (the same length).
    fn render(&mut self, left: &mut [f32], right: &mut [f32]);
    /// The delay it adds, in seconds.
    fn latency_seconds(&self) -> f64 {
        0.0
    }
}

/// The stored values for `params` from `values` (missing ones are the defaults).
pub(crate) fn stored<const N: usize>(params: &[NativeParam; N], values: &[f32]) -> [i32; N] {
    crate::native_fx::stored_values(params, values)
}

#[derive(Clone, Copy)]
struct Event {
    offset: u32,
    status: u8,
    d1: u8,
    d2: u8,
}

/// MIDI messages one block can hold; more are dropped.
const MAX_EVENTS: usize = 1024;

/// A native instrument in a track's instrument slot: its MIDI messages wait for their frame,
/// and the block is rendered in pieces between them.
pub struct Hosted {
    inst: Box<dyn NativeInst>,
    events: Vec<Event>,
    /// The sustain pedal is down; the notes let go meanwhile wait in `held`.
    pedal: bool,
    held: [bool; 128],
}

impl Hosted {
    pub fn new(inst: Box<dyn NativeInst>) -> Self {
        Hosted {
            inst,
            events: Vec::with_capacity(MAX_EVENTS),
            pedal: false,
            held: [false; 128],
        }
    }

    /// Parameter `index`'s value in use.
    pub fn param(&self, index: usize) -> f32 {
        self.inst.param(index)
    }

    fn release_held(&mut self) {
        for note in 0..128u8 {
            if std::mem::take(&mut self.held[usize::from(note)]) {
                self.inst.note_off(note);
            }
        }
    }

    fn handle(&mut self, e: Event) {
        let note = e.d1 & 0x7f;
        match e.status & 0xf0 {
            0x90 if e.d2 > 0 => {
                self.held[usize::from(note)] = false;
                self.inst.note_on(note, e.d2.min(127));
            }
            0x80 | 0x90 => {
                if self.pedal {
                    self.held[usize::from(note)] = true;
                } else {
                    self.inst.note_off(note);
                }
            }
            0xe0 => {
                let v = ((i32::from(e.d2 & 0x7f) << 7) | i32::from(e.d1 & 0x7f)) - 8192;
                self.inst.bend(v as f32 / 8192.0 * BEND_SEMITONES);
            }
            0xb0 if e.d1 == 64 => {
                self.pedal = e.d2 >= 64;
                if !self.pedal {
                    self.release_held();
                }
            }
            0xb0 if e.d1 == 120 => {
                self.held = [false; 128];
                self.inst.all_sound_off();
            }
            0xb0 if e.d1 == 123 => {
                self.held = [false; 128];
                self.inst.all_notes_off();
            }
            _ => {}
        }
    }
}

impl BlockProcessor for Hosted {
    fn set_parameter(&mut self, index: usize, value: f32) {
        self.inst.set_param(index, value);
    }

    fn midi(&mut self, status: u8, data1: u8, data2: u8, offset: u32) {
        if self.events.len() < self.events.capacity() {
            self.events.push(Event {
                offset,
                status,
                d1: data1,
                d2: data2,
            });
        }
    }

    fn process(&mut self, _input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        let frames = out.0.len().min(out.1.len());
        // By frame, keeping the order of messages at one frame (a note off before the next
        // note on). Insertion sort: no allocation, and they nearly always come in order.
        for i in 1..self.events.len() {
            let mut j = i;
            while j > 0 && self.events[j - 1].offset > self.events[j].offset {
                self.events.swap(j - 1, j);
                j -= 1;
            }
        }
        let (mut at, mut next) = (0usize, 0usize);
        while at < frames {
            while next < self.events.len() && self.events[next].offset as usize <= at {
                let e = self.events[next];
                self.handle(e);
                next += 1;
            }
            let end = self
                .events
                .get(next)
                .map_or(frames, |e| (e.offset as usize).min(frames));
            self.inst.render(&mut out.0[at..end], &mut out.1[at..end]);
            at = end;
        }
        while next < self.events.len() {
            let e = self.events[next];
            self.handle(e);
            next += 1;
        }
        self.events.clear();
        // An engine that went wrong is not heard as a burst.
        for v in out.0[..frames].iter_mut().chain(out.1[..frames].iter_mut()) {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
    }

    fn latency_seconds(&self) -> f64 {
        self.inst.latency_seconds()
    }
}

/// How an instrument is made: (sample rate, parameter values).
type Open = fn(u32, &[f32]) -> Box<dyn NativeInst>;

struct Entry {
    info: NativeInstInfo,
    open: Open,
}

fn entries() -> &'static [Entry] {
    static ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        #[allow(unused_mut)]
        let mut entries = vec![
            Entry {
                info: NativeFxInfo::new(
                    macro_osc::ID,
                    macro_osc::NAME,
                    &macro_osc::PARAMS,
                    &[],
                    macro_osc::value_text,
                ),
                open: |rate, values| Box::new(macro_osc::MacroOsc::new(rate, values)),
            },
            Entry {
                info: NativeFxInfo::new(kick::ID, kick::NAME, &kick::PARAMS, &[], kick::value_text),
                open: |rate, values| Box::new(kick::Kick::new(rate, values)),
            },
            Entry {
                info: NativeFxInfo::new(
                    bass_line::ID,
                    bass_line::NAME,
                    &bass_line::PARAMS,
                    &[],
                    bass_line::value_text,
                ),
                open: |rate, values| Box::new(bass_line::BassLine::new(rate, values)),
            },
            Entry {
                info: NativeFxInfo::new(
                    triple_osc::ID,
                    triple_osc::NAME,
                    triple_osc::params(),
                    &[],
                    triple_osc::value_text,
                ),
                open: |rate, values| Box::new(triple_osc::TripleOsc::new(rate, values)),
            },
            Entry {
                info: NativeFxInfo::new(
                    pluck::ID,
                    pluck::NAME,
                    &pluck::PARAMS,
                    &[],
                    pluck::value_text,
                ),
                open: |rate, values| Box::new(pluck::Pluck::new(rate, values)),
            },
            Entry {
                info: NativeFxInfo::new(
                    wave_draw::ID,
                    wave_draw::NAME,
                    wave_draw::params(),
                    &[],
                    wave_draw::value_text,
                ),
                open: |rate, values| Box::new(wave_draw::WaveDraw::new(rate, values)),
            },
            // The instruments written in C++ (the vendored crate `mi_dsp`) are not in the
            // browser build.
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    resonator::ID,
                    resonator::NAME,
                    &resonator::PARAMS,
                    &[],
                    resonator::value_text,
                ),
                open: |rate, values| Box::new(resonator::Resonator::new(rate, values)),
            },
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    digital_osc::ID,
                    digital_osc::NAME,
                    &digital_osc::PARAMS,
                    &[],
                    digital_osc::value_text,
                ),
                open: |rate, values| Box::new(digital_osc::DigitalOsc::new(rate, values)),
            },
            // The instruments from the vendored crate `chip_dsp` (C++): not in the browser
            // build either.
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(fm6::ID, fm6::NAME, fm6::params(), &[], fm6::value_text),
                open: |rate, values| Box::new(fm6::Fm6::new(rate, values)),
            },
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    chip_opl::ID,
                    chip_opl::NAME,
                    &chip_opl::PARAMS,
                    &[],
                    chip_opl::value_text,
                ),
                open: chip_opl::open,
            },
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    chip_opn::ID,
                    chip_opn::NAME,
                    chip_opn::params(),
                    &[],
                    chip_opn::value_text,
                ),
                open: chip_opn::open,
            },
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    chip_opll::ID,
                    chip_opll::NAME,
                    &chip_opll::PARAMS,
                    &[],
                    chip_opll::value_text,
                ),
                open: chip_opll::open,
            },
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    chip_sid::ID,
                    chip_sid::NAME,
                    &chip_sid::PARAMS,
                    &[],
                    chip_sid::value_text,
                ),
                open: chip_sid::open,
            },
            #[cfg(not(target_arch = "wasm32"))]
            Entry {
                info: NativeFxInfo::new(
                    chip_psg::ID,
                    chip_psg::NAME,
                    &chip_psg::PARAMS,
                    &[],
                    chip_psg::value_text,
                ),
                open: chip_psg::open,
            },
        ];
        entries
    })
}

/// Every native instrument, in menu order.
pub fn native_instruments() -> &'static [NativeInstInfo] {
    static INFOS: OnceLock<Vec<NativeInstInfo>> = OnceLock::new();
    INFOS.get_or_init(|| entries().iter().map(|e| e.info).collect())
}

/// The entry for `id`.
pub fn native_instrument_info(id: &str) -> Option<&'static NativeInstInfo> {
    native_instruments().iter().find(|e| e.id == id)
}

/// Makes the instrument `id` at `sample_rate` with parameter `values` (missing ones take
/// their defaults). Allocates: call it off the audio thread. `None` for an unknown id.
pub fn open_native_instrument(id: &str, sample_rate: u32, values: &[f32]) -> Option<Hosted> {
    let e = entries().iter().find(|e| e.info.id == id)?;
    Some(Hosted::new((e.open)(sample_rate.max(1), values)))
}

#[cfg(test)]
pub(crate) mod test_notes {
    use super::*;

    /// Plays `inst` for `frames` in blocks of `block`, each inside `assert_no_alloc`; `midi`
    /// is (frame, status, data1, data2), in order. Returns the left and right output.
    pub fn play(
        inst: &mut Hosted,
        frames: usize,
        block: usize,
        midi: &[(usize, u8, u8, u8)],
    ) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
        let mut next = 0;
        for at in (0..frames).step_by(block) {
            let end = (at + block).min(frames);
            while next < midi.len() && midi[next].0 < end {
                let (f, s, a, b) = midi[next];
                inst.midi(s, a, b, f.saturating_sub(at) as u32);
                next += 1;
            }
            let (bl, br) = (&mut l[at..end], &mut r[at..end]);
            assert_no_alloc::assert_no_alloc(|| inst.process((&[], &[]), (bl, br)));
        }
        (l, r)
    }

    pub fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    /// The strongest frequency of `x` between `lo` and `hi` Hz (autocorrelation), in Hz.
    pub fn pitch(x: &[f32], rate: f64, lo: f64, hi: f64) -> f64 {
        let (min_lag, max_lag) = ((rate / hi) as usize, (rate / lo) as usize);
        let n = x.len().saturating_sub(max_lag + 1);
        let mut best = (0.0f64, min_lag);
        let mut scores = vec![0.0f64; max_lag + 2];
        for lag in min_lag.max(1)..=max_lag {
            let mut s = 0.0f64;
            for i in 0..n {
                s += f64::from(x[i]) * f64::from(x[i + lag]);
            }
            scores[lag] = s;
        }
        // The first lag whose score is close to the best one: a multiple of the period scores
        // as high as the period itself.
        let top = scores.iter().cloned().fold(0.0f64, f64::max);
        // Past the first dip: the shortest lags score high only because they are short.
        let mut dipped = false;
        for lag in min_lag.max(1)..=max_lag {
            dipped |= scores[lag] < top * 0.3;
            if dipped
                && scores[lag] > top * 0.92
                && scores[lag] >= scores[lag - 1]
                && scores[lag] >= scores[lag + 1]
            {
                best = (scores[lag], lag);
                break;
            }
        }
        let lag = best.1;
        // A parabola through the peak and its neighbours.
        let (a, b, c) = (scores[lag - 1], scores[lag], scores[lag + 1]);
        let d = a - 2.0 * b + c;
        let shift = if d.abs() > 1e-12 {
            0.5 * (a - c) / d
        } else {
            0.0
        };
        rate / (lag as f64 + shift)
    }

    /// The level of `hz` in `x` (a single-bin Fourier sum).
    pub fn level_at(x: &[f32], hz: f64, rate: f64) -> f64 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, v) in x.iter().enumerate() {
            let a = i as f64 * std::f64::consts::TAU * hz / rate;
            re += f64::from(*v) * a.cos();
            im += f64::from(*v) * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / x.len().max(1) as f64
    }

    /// How much stronger the first four harmonics of `hz` are in `x` than the same four of
    /// the note a tritone away: far above 1 for a sound tuned to `hz`.
    pub fn tuned_to(x: &[f32], hz: f64, rate: f64) -> f64 {
        let sum = |f: f64| {
            (1..=4)
                .map(|k| level_at(x, f * f64::from(k), rate))
                .sum::<f64>()
        };
        sum(hz) / sum(hz * std::f64::consts::SQRT_2).max(1e-12)
    }

    /// The checks every instrument passes: a note sounds without allocation, every sample is
    /// finite, and after the note is let go and its tail has passed there is silence.
    pub fn sounds_and_falls_silent(id: &str, rate: u32, values: &[f32], tail_seconds: f32) {
        let Some(mut inst) = open_native_instrument(id, rate, values) else {
            panic!("{id} does not open")
        };
        let n = rate as usize;
        let (l, r) = play(
            &mut inst,
            n,
            256,
            &[(0, 0x90, 60, 100), (n / 2, 0x80, 60, 0)],
        );
        assert!(l.iter().chain(&r).all(|v| v.is_finite()), "{id}");
        // An instrument may sound on one side only (a voice of the Resonator does).
        let loud = peak(&l[..n / 2]).max(peak(&r[..n / 2]));
        assert!(loud > 0.01, "{id}: peak {loud}");
        let tail = (tail_seconds * rate as f32) as usize;
        let _ = play(&mut inst, tail, 256, &[]);
        let (l, r) = play(&mut inst, n / 4, 256, &[]);
        assert!(
            peak(&l) < 1e-4 && peak(&r) < 1e-4,
            "{id}: {} after the tail",
            peak(&l)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::test_notes::{peak, play};
    use super::*;

    #[test]
    fn every_instrument_opens_with_its_defaults_and_ids_are_unique() {
        let all = native_instruments();
        assert!(!all.is_empty());
        for (i, e) in all.iter().enumerate() {
            assert!(e.id.starts_with("prowave:"), "{}", e.id);
            assert!(all[i + 1..]
                .iter()
                .all(|o| o.id != e.id && o.name != e.name));
            assert!(crate::native_fx::native_info(e.id).is_none(), "{}", e.id);
            let Some(inst) = open_native_instrument(e.id, 48000, &[]) else {
                panic!("{} does not open", e.id)
            };
            for (k, p) in e.params.iter().enumerate() {
                assert!(
                    p.min <= p.default && p.default <= p.max,
                    "{} {}",
                    e.id,
                    p.name
                );
                assert!(p.integer, "{} {}", e.id, p.name);
                assert_eq!(inst.param(k), p.default, "{} {}", e.id, p.name);
                assert!(
                    !e.value_text(k, p.default).is_empty(),
                    "{} {}",
                    e.id,
                    p.name
                );
                for (c, _) in p.choices.iter().enumerate() {
                    assert!(!e.value_text(k, p.min + c as f32).is_empty());
                }
            }
        }
        assert!(open_native_instrument("prowave:nothing", 48000, &[]).is_none());
    }

    /// Every instrument at every parameter's minimum, default and maximum, at 44.1 and 96 kHz:
    /// a chord, the pedal, the wheel and both "all off" messages give finite output, with no
    /// allocation while playing or setting parameters.
    #[test]
    fn every_instrument_at_its_extremes_does_not_allocate() {
        for e in native_instruments() {
            for rate in [44100u32, 96000] {
                for setting in 0..3 {
                    let Some(mut inst) = open_native_instrument(e.id, rate, &[]) else {
                        panic!("{} does not open", e.id)
                    };
                    for (k, p) in e.params.iter().enumerate() {
                        let v = match setting {
                            0 => p.min,
                            1 => p.default,
                            _ => p.max,
                        };
                        assert_no_alloc::assert_no_alloc(|| inst.set_parameter(k, v));
                    }
                    let n = rate as usize / 4;
                    let midi = [
                        (0, 0x90, 36, 127),
                        (10, 0x90, 60, 100),
                        (10, 0x90, 64, 1),
                        (300, 0xb0, 64, 127),
                        (400, 0x80, 60, 0),
                        (500, 0xe0, 0, 127),
                        (900, 0x90, 96, 90),
                        (n / 2, 0xb0, 64, 0),
                        (n / 2 + 50, 0xb0, 123, 0),
                        (n - 300, 0x90, 20, 100),
                        (n - 100, 0xb0, 120, 0),
                    ];
                    let (l, r) = play(&mut inst, n, 333, &midi);
                    assert!(
                        l.iter().chain(&r).all(|v| v.is_finite()),
                        "{} at {rate}, setting {setting}",
                        e.id
                    );
                    // Loud is allowed (every gain at its top); a runaway is not.
                    assert!(peak(&l) < 100.0, "{} setting {setting}: {}", e.id, peak(&l));
                }
            }
        }
    }

    /// A message past the end of the block is still handled, and more messages than the
    /// queue holds are dropped without allocation.
    #[test]
    fn the_midi_queue_does_not_allocate() {
        let Some(mut inst) = open_native_instrument(macro_osc::ID, 48000, &[]) else {
            panic!("no instrument")
        };
        assert_no_alloc::assert_no_alloc(|| {
            for k in 0..3000u32 {
                inst.midi(0x90, (k % 100) as u8, 100, 5000);
            }
        });
        let (mut l, mut r) = ([0.0f32; 64], [0.0f32; 64]);
        assert_no_alloc::assert_no_alloc(|| inst.process((&[], &[]), (&mut l, &mut r)));
        assert_no_alloc::assert_no_alloc(|| inst.process((&[], &[]), (&mut l, &mut r)));
        assert!(peak(&l) > 0.0);
    }
}
