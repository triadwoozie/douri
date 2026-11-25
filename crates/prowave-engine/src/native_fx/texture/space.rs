// Copyright 2014-2017 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from Beads' dsp/fx/fx_engine.h, fx/reverb.h and fx/wow_and_flutter.h: the
// Griesinger/Dattorro reverb on the output (16-bit delay memory, as on the module) and the
// wow and flutter of the dirty tape mode.

use super::dsp::{
    clip16, integral_fractional, one_pole, CosineOscillator, Frame, Interp, Random, SineOscillator,
};

const SIZE: usize = 32768;
const MASK: usize = SIZE - 1;

/// The reverb's delay lines, in order (`E::Reserve<...>`).
const LENGTHS: [i32; 10] = [150, 214, 319, 527, 2182, 2690, 4501, 2525, 2197, 6312];

const fn bases() -> [i32; 10] {
    let mut b = [0i32; 10];
    let mut i = 1;
    while i < 10 {
        b[i] = b[i - 1] + LENGTHS[i - 1] + 1;
        i += 1;
    }
    b
}
const BASES: [i32; 10] = bases();

const AP1: usize = 0;
const AP2: usize = 1;
const AP3: usize = 2;
const AP4: usize = 3;
const DAP1A: usize = 4;
const DAP1B: usize = 5;
const DEL1: usize = 6;
const DAP2A: usize = 7;
const DAP2B: usize = 8;
const DEL2: usize = 9;
/// `TAIL`: the last sample of a line.
const TAIL: i32 = -1;

#[inline]
fn compress(v: f32) -> u16 {
    clip16((v * 32768.0) as i32) as i16 as u16
}

#[inline]
fn decompress(v: u16) -> f32 {
    f32::from(v as i16) / 32768.0
}

/// `FxEngine::Context`: an accumulator walking the delay memory.
struct Context<'a> {
    acc: f32,
    previous_read: f32,
    lfo: [f32; 2],
    buffer: &'a mut [u16],
    write_ptr: i32,
}

impl Context<'_> {
    #[inline]
    fn at(&self, line: usize, offset: i32) -> usize {
        let o = if offset == TAIL {
            LENGTHS[line] - 1
        } else {
            offset
        };
        (self.write_ptr.wrapping_add(BASES[line]).wrapping_add(o) as usize) & MASK
    }
    #[inline]
    fn load(&mut self, v: f32) {
        self.acc = v;
    }
    #[inline]
    fn read_value(&mut self, v: f32, scale: f32) {
        self.acc += v * scale;
    }
    #[inline]
    fn read(&mut self, line: usize, offset: i32, scale: f32) {
        let r = decompress(self.buffer[self.at(line, offset)]);
        self.previous_read = r;
        self.acc += r * scale;
    }
    #[inline]
    fn write(&mut self, line: usize, offset: i32, scale: f32) {
        let i = self.at(line, offset);
        self.buffer[i] = compress(self.acc);
        self.acc *= scale;
    }
    #[inline]
    fn write_all_pass(&mut self, line: usize, offset: i32, scale: f32) {
        self.write(line, offset, scale);
        self.acc += self.previous_read;
    }
    #[inline]
    fn interpolate(&mut self, line: usize, offset: f32, lfo: usize, amplitude: f32, scale: f32) {
        let offset = offset + amplitude * self.lfo[lfo];
        let (oi, of) = integral_fractional(offset);
        let base = self.write_ptr.wrapping_add(oi).wrapping_add(BASES[line]);
        let a = decompress(self.buffer[(base as usize) & MASK]);
        let b = decompress(self.buffer[(base.wrapping_add(1) as usize) & MASK]);
        let x = a + (b - a) * of;
        self.previous_read = x;
        self.acc += x * scale;
    }
    #[inline]
    fn lp(&mut self, state: &mut f32, coefficient: f32) {
        *state += coefficient * (self.acc - *state);
        self.acc = *state;
    }
}

/// `beads::Reverb`.
pub struct Reverb {
    buffer: Vec<u16>,
    write_ptr: i32,
    lfo: [CosineOscillator; 2],
    mix: f32,
    wet: f32,
    dry: f32,
    send: f32,
    reverb_time: f32,
    lp: f32,
    lp_1: f32,
    lp_2: f32,
}

impl Reverb {
    /// Allocates the delay memory (call off the audio thread).
    pub fn new(sample_rate: f32) -> Self {
        let mut r = Self {
            buffer: vec![0; SIZE],
            write_ptr: 0,
            lfo: [CosineOscillator::default(); 2],
            mix: 0.0,
            wet: 0.0,
            dry: 0.0,
            send: 0.0,
            reverb_time: 0.0,
            lp: 0.7,
            lp_1: 0.0,
            lp_2: 0.0,
        };
        r.reset(sample_rate);
        r
    }

    /// `Reset(sample_rate)` (the delay memory is kept, as in Beads).
    pub fn reset(&mut self, sample_rate: f32) {
        self.lfo[0].init_approximate(0.5 / sample_rate * 32.0);
        self.lfo[1].init_approximate(0.3 / sample_rate * 32.0);
        self.lp = 0.7;
        self.lp_1 = 0.0;
        self.lp_2 = 0.0;
        self.mix = 0.0;
        self.dry = 0.0;
        self.wet = 0.0;
        self.send = 0.0;
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix;
    }
    pub fn set_time(&mut self, t: f32) {
        self.reverb_time = t;
    }
    pub fn set_lp(&mut self, lp: f32) {
        self.lp = lp;
    }

    pub fn process(&mut self, frames: &mut [Frame]) {
        let size = frames.len();
        if size == 0 {
            return;
        }
        let kap = 0.625f32;
        let klp = self.lp;
        let krt = self.reverb_time;
        let input_gain = 0.2 * (1.0 - 0.333 * self.reverb_time);
        let output_gain = 0.4 / input_gain;
        let send_amount_sqrt = self.mix * (2.0 - self.mix);
        let send_amount = send_amount_sqrt * (2.0 - send_amount_sqrt);
        let wet = output_gain;
        let dry = (1.0 - self.mix) * (1.0 + self.mix);
        let mut wet_mod = Interp::new(self.wet, wet, size);
        let mut dry_mod = Interp::new(self.dry, dry, size);
        let mut send_mod = Interp::new(self.send, send_amount * input_gain, size);
        let (mut lp_1, mut lp_2) = (self.lp_1, self.lp_2);
        for f in frames.iter_mut() {
            let dry_amount = dry_mod.step();
            let wet_amount = wet_mod.step();
            let send_amount = send_mod.step();
            // Start.
            self.write_ptr -= 1;
            if self.write_ptr < 0 {
                self.write_ptr += SIZE as i32;
            }
            let lfo = if self.write_ptr & 31 == 0 {
                [self.lfo[0].advance(), self.lfo[1].advance()]
            } else {
                [self.lfo[0].value(), self.lfo[1].value()]
            };
            let mut c = Context {
                acc: 0.0,
                previous_read: 0.0,
                lfo,
                buffer: &mut self.buffer,
                write_ptr: self.write_ptr,
            };
            // Smear AP1 inside the loop.
            c.interpolate(AP1, 10.0, 0, 80.0, 1.0);
            c.write(AP1, 100, 0.0);
            c.read_value(f.l + f.r, send_amount);
            // Diffuse through 4 allpasses.
            c.read(AP1, TAIL, kap);
            c.write_all_pass(AP1, 0, -kap);
            c.read(AP2, TAIL, kap);
            c.write_all_pass(AP2, 0, -kap);
            c.read(AP3, TAIL, kap);
            c.write_all_pass(AP3, 0, -kap);
            c.read(AP4, TAIL, kap);
            c.write_all_pass(AP4, 0, -kap);
            let apout = c.acc;
            // Main reverb loop.
            c.load(apout);
            c.interpolate(DEL2, 6261.0, 1, 50.0, krt);
            c.lp(&mut lp_1, klp);
            c.read(DAP1A, TAIL, -kap);
            c.write_all_pass(DAP1A, 0, kap);
            c.read(DAP1B, TAIL, kap);
            c.write_all_pass(DAP1B, 0, -kap);
            c.write(DEL1, 0, 1.0);
            let wet_l = c.acc;
            f.l = f.l * dry_amount + wet_l * wet_amount;

            c.load(apout);
            c.interpolate(DEL1, 4460.0, 0, 40.0, krt);
            c.lp(&mut lp_2, klp);
            c.read(DAP2A, TAIL, kap);
            c.write_all_pass(DAP2A, 0, -kap);
            c.read(DAP2B, TAIL, -kap);
            c.write_all_pass(DAP2B, 0, kap);
            c.write(DEL2, 0, 1.0);
            let wet_r = c.acc;
            f.r = f.r * dry_amount + wet_r * wet_amount;
        }
        self.lp_1 = lp_1;
        self.lp_2 = lp_2;
        wet_mod.store(&mut self.wet);
        dry_mod.store(&mut self.dry);
        send_mod.store(&mut self.send);
    }
}

/// `stmlib::DelayLine<float, 128>`.
#[derive(Clone, Copy)]
struct ShortDelay {
    line: [f32; 128],
    write_ptr: usize,
}

impl ShortDelay {
    fn write_read(&mut self, sample: f32, delay: f32) -> f32 {
        self.line[self.write_ptr] = sample;
        self.write_ptr = (self.write_ptr + 127) % 128;
        let (di, df) = integral_fractional(delay);
        let i = self.write_ptr.wrapping_add(di as usize);
        let a = self.line[i % 128];
        let b = self.line[i.wrapping_add(1) % 128];
        a + (b - a) * df
    }
}

/// `beads::WowAndFlutter`.
pub struct WowAndFlutter {
    one_hertz: f32,
    delay: f32,
    amount: f32,
    lp: f32,
    random_frequency: f32,
    lp_state: Frame,
    random_modulation: SineOscillator,
    wow: SineOscillator,
    flutter: SineOscillator,
    line: [ShortDelay; 2],
}

impl WowAndFlutter {
    pub fn new(sample_rate: f32, lp: f32) -> Self {
        let one_hertz = 1.0 / sample_rate;
        Self {
            one_hertz,
            delay: 0.0,
            amount: 1.0,
            lp,
            random_frequency: 0.125 * one_hertz,
            lp_state: Frame::ZERO,
            random_modulation: SineOscillator::default(),
            wow: SineOscillator::default(),
            flutter: SineOscillator::default(),
            line: [ShortDelay {
                line: [0.0; 128],
                write_ptr: 0,
            }; 2],
        }
    }

    pub fn process(&mut self, rng: &mut Random, frames: &mut [Frame]) {
        let size = frames.len();
        if size == 0 {
            return;
        }
        let previous_phase = self.random_modulation.phase();
        let random_modulation = self
            .random_modulation
            .render_at(self.random_frequency * size as f32);
        if self.random_modulation.phase() >= 0.25 && previous_phase < 0.25 {
            self.random_frequency = self.one_hertz * (0.05 + rng.float() * 0.2);
        }
        let fm = 0.5 + 1.5 * random_modulation * random_modulation;
        let amount = (1.0 - 0.4 * random_modulation) * self.amount;
        let wow_frequency = self.one_hertz * 0.23 * size as f32;
        let wow = self.wow.render_at(wow_frequency * fm);
        let flutter_frequency = self.one_hertz * 1.99 * size as f32;
        let flutter = self.flutter.render_at(flutter_frequency);
        let delay = 1.0 + (105.0 * wow + 17.0 * flutter) * amount;
        let mut delay_mod = Interp::new(self.delay, delay, size);
        for f in frames.iter_mut() {
            let d = delay_mod.step();
            let l = self.line[0].write_read(f.l, d);
            one_pole(&mut self.lp_state.l, l, self.lp);
            let r = self.line[1].write_read(f.r, d);
            one_pole(&mut self.lp_state.r, r, self.lp);
            *f = self.lp_state;
        }
        delay_mod.store(&mut self.delay);
    }
}
