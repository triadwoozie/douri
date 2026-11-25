// Copyright 2017-2021 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from Beads' dsp/delay_clock.cc, delay_controller.cc, tap.h and multitap_delay.h:
// the delay mode that replaces the grains when SIZE is fully clockwise (taps at DENSITY's
// rhythm, TIME picking the multiplier, slices of the frozen buffer when frozen).

use super::delay_line::{Codec, DelayLine};
use super::dsp::{
    constrain, interpolate, one_pole, semitones_to_ratio, Frame, HysteresisFilter,
    HysteresisQuantizer, Interp, Random, SineOscillator, Svf,
};
use super::grain::{RandomWalk, RandomizedParameter};
use super::trigger::SeedState;

/// `DelayClock`: intervals from taps (or a free-running interval) with a crossfade between
/// the old and the new one.
#[derive(Clone, Copy, Debug, Default)]
struct DelayClock {
    interval: [f32; 2],
    average_interval: f32,
    fade: f32,
    fade_increment: f32,
    max_fade_increment: f32,
    min_fade_increment: f32,
    acceptable_jitter: f32,
    jitter_sensitivity: f32,
    schedule_change: bool,
    next_interval: i32,
    max_interval: i32,
    rotate: i32,
}

impl DelayClock {
    fn init(&mut self, sample_rate: f32) {
        self.average_interval = sample_rate * 0.25;
        self.interval = [self.average_interval; 2];
        self.fade = 1.0;
        self.min_fade_increment = 1.0 / (sample_rate * 0.25);
        self.max_fade_increment = 1.0 / (sample_rate * 0.02);
        self.fade_increment = self.max_fade_increment;
        self.acceptable_jitter = sample_rate * 0.001;
        self.jitter_sensitivity = 1.0;
        self.schedule_change = false;
        self.next_interval = 0;
        self.max_interval = (sample_rate * 4.0) as i32;
        self.rotate = 0;
    }

    fn process(&mut self, tick: bool, tap: bool) {
        self.next_interval = self.next_interval.saturating_add(1);
        self.fade += self.fade_increment;
        if self.fade >= 1.0 {
            self.fade = 1.0;
        }
        if tick || tap {
            if self.next_interval < self.max_interval {
                let interval = self.next_interval as f32;
                let big_change = (interval - self.interval[1]).abs() > self.acceptable_jitter;
                let small_change = (self.average_interval - self.interval[1]).abs()
                    > self.jitter_sensitivity * self.acceptable_jitter;
                if big_change {
                    self.jitter_sensitivity = 1.0;
                    self.average_interval = interval;
                } else {
                    one_pole(&mut self.average_interval, interval, 0.1);
                    self.jitter_sensitivity = (self.jitter_sensitivity * 0.95).max(0.05);
                }
                self.schedule_change = self.schedule_change || tap || big_change || small_change;
            }
            self.next_interval = 0;
        }
        let can_change = tap || self.fade >= 1.0;
        if self.schedule_change && can_change {
            self.rotate = self.rotate.wrapping_add(1);
            self.interval[0] = self.interval[1];
            self.interval[1] = self.average_interval;
            self.fade = 0.0;
            self.fade_increment = constrain(
                2.0 / self.average_interval,
                self.min_fade_increment,
                self.max_fade_increment,
            );
            self.schedule_change = false;
        }
    }

    fn update(&mut self, interval: f32, lp_coefficient: f32) {
        one_pole(&mut self.interval[0], interval, lp_coefficient);
        self.interval[1] = self.interval[0];
        self.fade = 1.0;
        self.rotate = 0;
    }

    fn slot(&self, i: i32) -> usize {
        (i.wrapping_add(self.rotate).rem_euclid(2)) as usize
    }

    fn interval(&self, i: i32) -> f32 {
        self.interval[self.slot(i)]
    }

    fn gain(&self, i: i32) -> f32 {
        if self.slot(i) == 1 {
            self.fade
        } else {
            1.0 - self.fade
        }
    }

    fn retrig(&self) -> bool {
        self.fade == 0.0 && self.next_interval == 0
    }

    fn new_interval_index(&self) -> i32 {
        (self.rotate.wrapping_add(1)).rem_euclid(2)
    }
}

/// `DelayTap`: a delay in 1/1024 samples and a gain.
#[derive(Clone, Copy, Debug, Default)]
pub struct DelayTap {
    pub delay: i32,
    pub gain: f32,
}

#[inline]
fn triangle(phase: f32, pw: f32) -> f32 {
    let slope_up = 1.0 / pw;
    let slope_down = 1.0 / (1.0 - pw);
    if phase < pw {
        phase * slope_up
    } else {
        1.0 - (phase - pw) * slope_down
    }
}

const PW: [f32; 6] = [0.0, 0.001, 0.01, 0.5, 0.75, 0.999];

/// `PlayHead`: plays one slice of the frozen buffer.
#[derive(Clone, Copy, Debug, Default)]
struct PlayHead {
    start: i32,
    remaining: i32,
    mid_point: i32,
    phase: f32,
    phase_increment: f32,
    gain: f32,
}

impl PlayHead {
    fn init(&mut self) {
        *self = Self::default();
    }
    fn done(&self) -> bool {
        self.remaining == 0
    }
    fn half_way(&self) -> bool {
        self.remaining != 0 && self.remaining == self.mid_point
    }
    fn start(&mut self, start: i32, duration: i32, gain: f32) {
        self.start = start;
        self.remaining = duration;
        self.mid_point = duration >> 1;
        self.phase = 0.0;
        self.phase_increment = 1.0 / duration as f32;
        self.gain = gain;
    }
    fn render(&mut self, tap: &mut DelayTap, shape: f32, slave: bool) {
        if self.done() {
            tap.delay = 0;
            tap.gain = 0.0;
            return;
        }
        let envelope = if slave {
            let slave_envelopes = [0.0, 0.0, 0.0, triangle(self.phase, 0.5), 0.0, 0.0];
            interpolate(&slave_envelopes, shape, 5.0)
        } else if shape < 0.2 {
            let square = (100.0 * triangle(self.phase, 0.5)).min(1.0);
            let tri = triangle(self.phase, PW[1]);
            square + (tri - square) * shape * 5.0
        } else {
            triangle(self.phase, interpolate(&PW, shape, 5.0))
        };
        tap.delay = self.start.wrapping_add(self.remaining) << 10;
        tap.gain = self.gain * envelope;
        self.remaining -= 1;
        self.phase += self.phase_increment;
    }
}

/// `TapEnvelope`.
#[derive(Clone, Copy, Debug, Default)]
struct TapEnvelope {
    phase: f32,
    phase_increment: f32,
}

impl TapEnvelope {
    fn init(&mut self) {
        *self = Self::default();
    }
    fn render(&mut self, shape: f32) -> f32 {
        let envelope = if shape < 0.2 {
            let square = 1.0;
            let tri = triangle(self.phase, PW[1]);
            square + (tri - square) * shape * 5.0
        } else {
            triangle(self.phase, interpolate(&PW, shape, 5.0))
        };
        self.phase += self.phase_increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        envelope
    }
}

pub const NUM_INTERVALS: usize = 2;
pub const NUM_RATIOS: usize = 2;
pub const NUM_HEADS: usize = NUM_INTERVALS * NUM_RATIOS;
pub const NUM_TAPS: usize = NUM_HEADS * 2;

/// `DelayController::Output`.
#[derive(Clone, Copy, Debug, Default)]
pub struct TapsOutput {
    pub tap: [DelayTap; NUM_TAPS],
    pub gate: bool,
}

const MIN_DELAY: i32 = 64 + 4;
const NUM_DELAY_RATIOS: usize = 20;
const RATIO: [f32; NUM_DELAY_RATIOS + 2] = [
    1.0 / 16.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 4.0,
    1.0 / 2.0,
    1.0 / 2.0,
    1.0,
    1.0,
    1.0,
    1.0,
    1.0 / 2.0,
    1.0 / 3.0,
    1.0 / 4.0,
    1.0 / 6.0,
    1.0 / 8.0,
    1.0 / 10.0,
    1.0 / 12.0,
    1.0 / 16.0,
    1.0 / 16.0,
    1.0 / 16.0,
];

/// `DelayController`: the tap times and gains of the delay mode.
pub struct DelayController {
    max_delay: i32,
    clock: DelayClock,
    play_head: [PlayHead; NUM_HEADS],
    slave_play_head: [PlayHead; NUM_HEADS],
    numerator_quantizer: [HysteresisQuantizer; NUM_HEADS],
    tap_envelope: [TapEnvelope; NUM_TAPS],
    position_modulation: RandomWalk,
    shape_modulation: RandomWalk,
    tap_modulation: SineOscillator,
    density_filter: HysteresisFilter,
    fm_filter: HysteresisFilter,
    slice_index_filter: HysteresisFilter,
    multiplier_lp: f32,
    shape: f32,
    linear_fm: f32,
    fm: f32,
    density: f32,
    freeze: bool,
    seed_state: SeedState,
}

impl DelayController {
    pub fn new(sample_rate: f32, max_delay: i32) -> Self {
        let mut c = Self {
            max_delay,
            clock: DelayClock::default(),
            play_head: [PlayHead::default(); NUM_HEADS],
            slave_play_head: [PlayHead::default(); NUM_HEADS],
            numerator_quantizer: [HysteresisQuantizer::default(); NUM_HEADS],
            tap_envelope: [TapEnvelope::default(); NUM_TAPS],
            position_modulation: RandomWalk::default(),
            shape_modulation: RandomWalk::default(),
            tap_modulation: SineOscillator::default(),
            density_filter: HysteresisFilter::default(),
            fm_filter: HysteresisFilter::default(),
            slice_index_filter: HysteresisFilter::default(),
            multiplier_lp: 0.0,
            shape: 0.0,
            linear_fm: 0.0,
            fm: 1.0,
            density: 0.0,
            freeze: false,
            seed_state: SeedState::Off,
        };
        c.clock.init(sample_rate);
        for q in &mut c.numerator_quantizer {
            q.init(2, 0.05, false);
        }
        c.position_modulation.init(0.1 / sample_rate);
        c.shape_modulation.init(0.1 / sample_rate);
        c.tap_modulation.init(0.17 / sample_rate);
        const FILTER_COEFFICIENT: f32 = 0.0025;
        c.density_filter.init(FILTER_COEFFICIENT);
        c.slice_index_filter.init(FILTER_COEFFICIENT);
        c.fm_filter.init(FILTER_COEFFICIENT);
        c
    }

    /// `Render` with no external SEED input.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        rng: &mut Random,
        seed: SeedState,
        freeze: &[bool],
        density: f32,
        linear_fm: f32,
        exponential_fm: f32,
        position: &RandomizedParameter,
        shape: &RandomizedParameter,
        out: &mut [TapsOutput],
    ) {
        let synchronized = seed != SeedState::Locked;
        let size = out.len().min(freeze.len());
        let (mut linear_fm, mut exponential_fm) = (linear_fm, exponential_fm);
        if synchronized {
            linear_fm += exponential_fm;
            exponential_fm = 0.0;
        }
        one_pole(&mut self.linear_fm, linear_fm, 0.05);
        let density = constrain(density + self.linear_fm, 0.0, 1.0);

        let slice_modulation = 1.005
            * position.value_full(rng, Some((&mut self.position_modulation, size)), 1.0, true);
        let shape_target =
            shape.value_full(rng, Some((&mut self.shape_modulation, size)), 1.0, true) * 0.995;
        let mut modulated_shape = Interp::new(self.shape, shape_target, size);
        let fm_target = semitones_to_ratio(-self.fm_filter.process(exponential_fm) * 60.0);
        let mut modulated_fm = Interp::new(self.fm, fm_target, size);
        let density_target = self.density_filter.process(density);
        let mut modulated_density = Interp::new(self.density, density_target, size);

        let mut tap = seed == SeedState::On && self.seed_state == SeedState::Off;
        self.seed_state = seed;

        for (o, frozen) in out[..size].iter_mut().zip(freeze) {
            let mut tap_ratio = [0.0f32; NUM_RATIOS];
            let mut tap_gain = [0.0f32; NUM_RATIOS];
            let density = modulated_density.step();
            let fm = modulated_fm.step();
            if synchronized {
                self.clock.process(false, tap);
                tap = false;
                let (ratio_integral, mut ratio_fractional) =
                    super::dsp::integral_fractional(density * NUM_DELAY_RATIOS as f32);
                ratio_fractional -= 0.5;
                ratio_fractional *= if density < 0.5 { 4.0 } else { 1.5 };
                ratio_fractional += 0.5;
                let ratio_fractional = constrain(ratio_fractional, 0.0, 1.0);
                let ri = ratio_integral.clamp(0, NUM_DELAY_RATIOS as i32) as usize;
                let flip = ri & 1;
                tap_ratio[flip] = RATIO[ri];
                tap_ratio[1 - flip] = RATIO[ri + 1];
                tap_gain[flip] = 1.0 - ratio_fractional;
                tap_gain[1 - flip] = ratio_fractional;
                tap_ratio[0] *= fm;
                tap_ratio[1] *= fm;
            } else {
                let new_interval = (MIN_DELAY as f32).max(
                    fm * self.max_delay as f32 * semitones_to_ratio(-(density - 0.5).abs() * 216.0),
                );
                let lp_coefficient = constrain(10.0 / new_interval, 0.005, 0.7);
                self.clock.update(new_interval, lp_coefficient);
                tap_ratio[0] = 1.00;
                tap_ratio[1] = 0.77;
                let mut fade_to_odd = constrain((density - 0.5) * 25.0, 0.0, 1.0);
                fade_to_odd *= 0.25 + 0.75 * self.tap_modulation.render();
                tap_gain[0] = 1.0 - fade_to_odd * 0.2;
                tap_gain[1] = fade_to_odd * 0.8;
            }

            let shape = modulated_shape.step();

            if *frozen {
                // Reset all play heads when freezing.
                if !self.freeze {
                    for i in 0..NUM_HEADS {
                        self.play_head[i].init();
                        self.slave_play_head[i].init();
                    }
                }
                // Reset and resync the play heads that will be faded in.
                if self.clock.retrig() {
                    let start = self.clock.new_interval_index() as usize * NUM_RATIOS;
                    for i in start..start + NUM_RATIOS {
                        self.play_head[i].init();
                        self.slave_play_head[i].init();
                    }
                }
                let slice_index_smooth = self.slice_index_filter.process(slice_modulation);
                for i in 0..NUM_HEADS {
                    let which = if self.play_head[i].done() {
                        Some(false)
                    } else if self.play_head[i].half_way() {
                        Some(true)
                    } else {
                        None
                    };
                    let interval = (i / NUM_RATIOS) as i32;
                    let ratio = i % NUM_RATIOS;
                    if let Some(slave) = which {
                        let slice_duration =
                            ((self.clock.interval(interval) * tap_ratio[ratio]) as i32).max(1);
                        let num_slices = self.max_delay / slice_duration;
                        if self.numerator_quantizer[i].num_steps() != num_slices {
                            self.numerator_quantizer[i].init(
                                num_slices,
                                0.05 / 5.0 * num_slices as f32,
                                false,
                            );
                        }
                        let slice_index = self.numerator_quantizer[i].process(slice_index_smooth);
                        let head = if slave {
                            &mut self.slave_play_head[i]
                        } else {
                            &mut self.play_head[i]
                        };
                        head.start(
                            slice_duration * slice_index + MIN_DELAY,
                            slice_duration,
                            tap_gain[ratio] * self.clock.gain(interval),
                        );
                    }
                    self.play_head[i].render(&mut o.tap[2 * i], shape, false);
                    self.slave_play_head[i].render(&mut o.tap[2 * i + 1], shape, true);
                }
                let k = self.clock.new_interval_index() as usize * NUM_RATIOS;
                o.gate = self.play_head[k.min(NUM_HEADS - 1)].phase < 0.5;
            } else {
                // Reset all envelopes when unfreezing.
                if self.freeze {
                    for e in &mut self.tap_envelope {
                        e.init();
                    }
                }
                // Reset and resync the envelopes that will be faded in.
                if self.clock.retrig() {
                    let start = self.clock.new_interval_index() as usize * NUM_HEADS;
                    for e in self.tap_envelope.iter_mut().skip(start).take(NUM_HEADS) {
                        e.init();
                    }
                }
                one_pole(&mut self.multiplier_lp, slice_modulation, 0.01);
                let multiplier = constrain(self.multiplier_lp * 15.0 + 1.0, 1.0, 16.0);
                let (multiplier_integral, multiplier_fractional) =
                    super::dsp::integral_fractional(multiplier);
                let multiplier_fractional =
                    constrain(0.5 + 8.0 * (multiplier_fractional - 0.5), 0.0, 1.0);

                let mut loudest = 0.0f32;
                let mut loudest_index = 0usize;
                for t in 0..NUM_TAPS {
                    let interval = t / NUM_HEADS;
                    let ratio = (t / NUM_INTERVALS) % NUM_RATIOS;
                    let tp = t & 1;
                    let i = (2 * ratio
                        + if multiplier_integral & 1 != 0 {
                            tp
                        } else {
                            1 - tp
                        })
                        + interval * NUM_HEADS;
                    let delay_time = constrain(
                        self.clock.interval(interval as i32)
                            * tap_ratio[ratio]
                            * (multiplier_integral + tp as i32) as f32,
                        MIN_DELAY as f32,
                        self.max_delay as f32,
                    );
                    self.tap_envelope[i].phase_increment = 1.0 / delay_time;
                    o.tap[i].delay = (delay_time * 1024.0) as i32;
                    let ratio_fade = if tp != 0 {
                        multiplier_fractional
                    } else {
                        1.0 - multiplier_fractional
                    };
                    let base_gain = tap_gain[ratio] * ratio_fade * self.clock.gain(interval as i32);
                    o.tap[i].gain = base_gain * self.tap_envelope[i].render(shape);
                    if base_gain > loudest {
                        loudest = base_gain;
                        loudest_index = i;
                    }
                }
                o.gate = self.tap_envelope[loudest_index].phase < 0.5;
            }
            self.freeze = *frozen;
        }
        modulated_shape.store(&mut self.shape);
        modulated_fm.store(&mut self.fm);
        modulated_density.store(&mut self.density);
    }
}

/// `Tap<window_size>`: a delay tap, pitch-shifted by two crossfaded heads when `WINDOW > 1`.
#[derive(Clone, Copy, Debug, Default)]
struct Tap<const WINDOW: i32> {
    phase: i32,
}

impl<const WINDOW: i32> Tap<WINDOW> {
    #[inline]
    fn render<C: Codec>(
        &mut self,
        line: &DelayLine,
        buf: &[i16],
        delay: i32,
        gain: f32,
        speed: f32,
        out: &mut Frame,
    ) {
        if delay == 0 {
            return;
        }
        if WINDOW == 1 {
            *out += line.read_hermite_fixed::<C>(buf, delay) * gain;
            return;
        }
        let full = 1024 * WINDOW;
        let half = full >> 1;
        self.phase = self.phase.wrapping_add((1024.0 * (speed - 1.0)) as i32);
        if self.phase >= full {
            self.phase -= full;
        } else if self.phase < 0 {
            self.phase += full;
        }
        let delay = (full + 1).max(delay.wrapping_add(half));
        let phase = self.phase as f32 / full as f32;
        let tri = if phase < 0.5 {
            2.0 * phase
        } else {
            2.0 - 2.0 * phase
        };
        let head_1 = line.read_hermite_fixed::<C>(buf, delay.wrapping_sub(self.phase));
        let head_2 = line.read_hermite_fixed::<C>(
            buf,
            delay.wrapping_sub((self.phase + half).rem_euclid(full)),
        );
        *out += (head_2 + (head_1 - head_2) * tri) * gain;
    }
}

/// `MultitapDelay`: the taps, pitch-shifted or not, through a low- or high-pass filter.
pub struct MultitapDelay {
    tap: [Tap<2048>; NUM_TAPS],
    no_latency_tap: [Tap<1>; NUM_TAPS],
    filter: [Svf; 2],
}

impl Default for MultitapDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl MultitapDelay {
    pub fn new() -> Self {
        let mut m = Self {
            tap: [Tap::default(); NUM_TAPS],
            no_latency_tap: [Tap::default(); NUM_TAPS],
            filter: [Svf::default(); 2],
        };
        for f in &mut m.filter {
            f.init();
        }
        m
    }

    pub fn reset(&mut self) {
        self.tap = [Tap::default(); NUM_TAPS];
        self.no_latency_tap = [Tap::default(); NUM_TAPS];
    }

    #[inline]
    pub fn render<C: Codec>(
        &mut self,
        line: &DelayLine,
        buf: &[i16],
        taps: &TapsOutput,
        speed: f32,
        bandwidth: f32,
        out: &mut Frame,
    ) {
        // Cross-fade between the original and pitch-shifted delays.
        let gain = constrain(100.0 * ((speed - 1.0).abs() - 0.01), 0.0, 1.0);
        let mut mix = Frame::ZERO;
        for i in 0..NUM_TAPS {
            self.tap[i].render::<C>(
                line,
                buf,
                taps.tap[i].delay,
                taps.tap[i].gain * gain,
                speed,
                &mut mix,
            );
            self.no_latency_tap[i].render::<C>(
                line,
                buf,
                taps.tap[i].delay,
                taps.tap[i].gain * (1.0 - gain),
                speed,
                &mut mix,
            );
        }
        let amount = (2.0 * bandwidth - 1.0).abs();
        // LP: -7 to -2 octaves; HP: -8 to -3 octaves.
        let octave = if bandwidth < 0.5 {
            -7.0 + bandwidth * 10.0
        } else {
            -8.0 + (bandwidth - 0.5) * 10.0
        };
        let f = 0.49 * semitones_to_ratio(12.0 * octave);
        let q = 1.0 + 16.0 * amount * f;
        let wet_amount_sqrt = amount * (2.0 - amount);
        let wet_amount = wet_amount_sqrt * (2.0 - wet_amount_sqrt);
        self.filter[0].set_f_q_fast(f, q);
        let f0 = self.filter[0];
        self.filter[1].set_from(&f0);
        let wet = if bandwidth < 0.5 {
            Frame::new(
                self.filter[0].low_pass(mix.l),
                self.filter[1].low_pass(mix.r),
            )
        } else {
            Frame::new(
                self.filter[0].high_pass(mix.l),
                self.filter[1].high_pass(mix.r),
            )
        };
        *out += mix + (wet - mix) * wet_amount;
    }
}
