// Copyright 2017 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from Beads' dsp/grain_envelope.h, voice_stealing_envelope.h, grain.h,
// delay_grain.h, granular_player.h, delay_granular_player.h and randomized_parameter.h.

use super::delay_line::{Codec, DelayLine};
use super::dsp::{constrain, semitones_to_ratio, smooth_step, Frame, Random};
use super::trigger::TriggerOutput;

/// `RandomWalkModulation`: a smoothed random walk used for the parameters that move per
/// block rather than per grain.
#[derive(Clone, Copy, Debug, Default)]
pub struct RandomWalk {
    phase: f32,
    frequency: f32,
    from: f32,
    to: f32,
}

impl RandomWalk {
    pub fn init(&mut self, frequency: f32) {
        self.frequency = frequency;
        self.phase = 0.0;
        self.from = 0.0;
        self.to = 1.0;
    }

    pub fn value(&mut self, rng: &mut Random, attenuverter: f32, step_size: usize) -> f32 {
        let frequency =
            (if attenuverter <= 0.0 { 0.25 } else { 4.0 }) * self.frequency * step_size as f32;
        self.phase += frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.from = self.to;
            self.to = rng.centered();
        }
        self.from + (self.to - self.from) * smooth_step(self.phase)
    }
}

/// `RandomizedParameter`: a knob and its amount (attenuverter) of random modulation. There
/// are no CV inputs here (`has_cv` is always false in the port).
#[derive(Clone, Copy, Debug, Default)]
pub struct RandomizedParameter {
    pub base: f32,
    pub attenuverter: f32,
}

impl RandomizedParameter {
    pub fn randomness_amount(&self) -> f32 {
        self.attenuverter.abs()
    }

    pub fn value_full(
        &self,
        rng: &mut Random,
        modulation: Option<(&mut RandomWalk, usize)>,
        range: f32,
        clamp: bool,
    ) -> f32 {
        let mut result = self.base;
        match modulation {
            Some((m, step)) => {
                result += self.attenuverter * range * m.value(rng, self.attenuverter, step);
            }
            None => {
                let blend = if self.attenuverter > 0.0 {
                    self.attenuverter
                } else {
                    (self.attenuverter * rng.float() * -3.0).min(1.0)
                };
                let random_value = if !clamp {
                    rng.centered() * range
                } else {
                    rng.float() * range - result
                };
                result += random_value * blend;
            }
        }
        if clamp {
            result = constrain(result, 0.0, 1.0);
        }
        result
    }

    /// `value()`: a fresh random draw in 0..1.
    pub fn value(&self, rng: &mut Random) -> f32 {
        self.value_full(rng, None, 1.0, true)
    }

    /// `pitch_value()`: a frequency ratio (base and range in semitones, range 24).
    pub fn pitch_value(&self, rng: &mut Random) -> f32 {
        semitones_to_ratio(self.value_full(rng, None, 24.0, false))
    }
}

/// `beads::Parameters`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Parameters {
    pub time: RandomizedParameter,
    pub size: RandomizedParameter,
    pub shape: RandomizedParameter,
    /// `base` in semitones.
    pub pitch: RandomizedParameter,
}

const STAGE_ATTACK: usize = 0;
const STAGE_SUSTAIN: usize = 1;
const STAGE_DONE: usize = 3;

/// `GrainEnvelope`: attack, sustain, release; SHAPE morphs square, decay, bell and reverse.
#[derive(Clone, Copy, Debug, Default)]
pub struct GrainEnvelope {
    stage: usize,
    phase: f32,
    phase_increment: [f32; 4],
    smoothness: f32,
    min_attack: f32,
    min_release: f32,
}

impl GrainEnvelope {
    pub fn init(&mut self, sample_rate: f32) {
        self.stage = STAGE_DONE;
        self.phase = 0.0;
        self.phase_increment = [0.5, 0.5, 0.5, 0.0];
        self.smoothness = 0.0;
        self.min_attack = 0.0005 * sample_rate;
        self.min_release = 0.010 * sample_rate;
    }

    pub fn start(&mut self, duration: f32, shape: f32) {
        self.stage = STAGE_ATTACK;
        self.phase = 0.0;
        let sustain = (1.0 - 3.0 * shape).max(0.0);
        let mut shape = (shape - 0.333_333_33) * 1.5;
        if shape < 0.0 {
            shape = 0.0;
        } else if shape < 0.5 {
            shape = shape * shape * shape * 4.0;
        }
        let attack = shape;
        let release = (1.0 - attack) * (1.0 - sustain);
        let attack_duration = (duration * attack).max(self.min_attack * (1.0 + 3.0 * sustain));
        let sustain_duration = (duration * sustain).max(2.0);
        let release_duration = (duration * release).max(self.min_release);
        self.phase_increment[0] = 1.0 / attack_duration;
        self.phase_increment[1] = 1.0 / sustain_duration;
        self.phase_increment[2] = 1.0 / release_duration;
        self.smoothness = if attack < 0.5 {
            2.0 * attack
        } else {
            2.0 - 2.0 * attack
        };
    }

    pub fn start_at(&mut self, duration: f32, shape: f32, fractional_sample: f32) {
        self.start(duration, shape);
        self.phase += self.phase_increment[self.stage] * fractional_sample;
        self.wrap_phase();
    }

    pub fn stop(&mut self) {
        self.stage = STAGE_DONE;
    }

    fn wrap_phase(&mut self) {
        if self.phase >= 1.0 {
            let t = (self.phase - 1.0) / self.phase_increment[self.stage];
            self.stage = (self.stage + 1).min(STAGE_DONE);
            self.phase = t * self.phase_increment[self.stage];
        }
    }

    pub fn render(&mut self) -> f32 {
        if self.stage == STAGE_DONE {
            return 0.0;
        }
        let value = if self.stage == STAGE_SUSTAIN {
            1.0
        } else {
            let attack = self.stage == STAGE_ATTACK;
            let t = if attack { self.phase } else { 1.0 - self.phase };
            let base = if attack { t } else { (t * t) * (t * t) };
            base + (smooth_step(t) - base) * self.smoothness
        };
        self.phase += self.phase_increment[self.stage];
        self.wrap_phase();
        value
    }

    pub fn done(&self) -> bool {
        self.stage >= STAGE_DONE
    }
}

/// `VoiceStealingEnvelope`: the quick fade of a grain that must make room for a new one.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceStealingEnvelope {
    value: f32,
    decrement: f32,
}

impl VoiceStealingEnvelope {
    pub fn reset_and_pause(&mut self) {
        self.value = 1.0;
        self.decrement = 0.0;
    }
    pub fn start(&mut self, duration: f32) -> i32 {
        self.decrement = 1.0 / duration;
        (self.value * duration) as i32
    }
    pub fn render(&mut self) -> f32 {
        self.value -= self.decrement;
        self.value.max(0.0)
    }
    pub fn done(&self) -> bool {
        self.value <= 0.0
    }
}

/// Length of the "safety zone" near the record head.
pub const HEAD_CROSSING_SAFETY_ZONE: i32 = 256;

/// `DelayGrain`: one grain reading the buffer at its own speed and position.
#[derive(Clone, Copy, Debug, Default)]
pub struct DelayGrain {
    remaining_samples: i32,
    envelope: GrainEnvelope,
    voice_stealing: VoiceStealingEnvelope,
    gain: [[f32; 2]; 2],
    delay: i32,
    speed: i32,
    initial_delay: i32,
}

impl DelayGrain {
    pub fn init(&mut self, sample_rate: f32) {
        self.initial_delay = 0;
        self.delay = 0;
        self.speed = 1024;
        self.gain = [[0.5, 0.5], [0.5, 0.5]];
        self.envelope.init(sample_rate);
        self.voice_stealing.reset_and_pause();
        self.remaining_samples = 0;
    }

    pub fn done(&self) -> bool {
        self.envelope.done() || self.voice_stealing.done()
    }

    pub fn remaining_samples(&self) -> i32 {
        self.remaining_samples
    }

    pub fn fade_out(&mut self, duration: i32) {
        if self.remaining_samples > duration {
            self.remaining_samples = self.voice_stealing.start(duration as f32);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        line: &DelayLine,
        delay: f32,
        speed: f32,
        duration: f32,
        shape: f32,
        gain: f32,
        pan: f32,
        fractional_sample: f32,
    ) {
        let max_delay = line.max_delay() as f32;
        let consumption_speed = speed - if line.frozen() { 0.0 } else { 1.0 };
        let mut delay = delay - fractional_sample * consumption_speed;
        let mut duration = duration;
        // Trim duration so that there's enough space in the buffer.
        if consumption_speed != 0.0 {
            duration = duration.min(max_delay / consumption_speed.abs());
        }
        if consumption_speed > 0.0 {
            // Trim delay time so we don't hit the recording head.
            delay = delay.max(duration * consumption_speed);
        } else if consumption_speed < 0.0 {
            // Trim delay time so we don't loop and hit back of the recording head.
            delay = delay.min(max_delay + duration * consumption_speed);
        }
        self.delay = (delay * 1024.0) as i32;
        self.initial_delay = self.delay;
        self.speed = (speed * 1024.0) as i32;

        // Roughly equal power. Uses first order approximation of square root.
        let mut pan = pan;
        if pan < 0.5 {
            pan *= 2.0;
            self.gain[0][0] = gain;
            self.gain[1][0] = 0.0;
            self.gain[1][1] = pan * (2.0 - pan) * gain;
            self.gain[0][1] = (1.0 - pan) * (1.0 + pan) * gain;
        } else {
            pan = 1.0 - pan;
            pan *= 2.0;
            self.gain[1][1] = gain;
            self.gain[0][1] = 0.0;
            self.gain[0][0] = pan * (2.0 - pan) * gain;
            self.gain[1][0] = (1.0 - pan) * (1.0 + pan) * gain;
        }
        self.envelope.start_at(duration, shape, fractional_sample);
        self.remaining_samples = duration as i32;
        self.voice_stealing.reset_and_pause();
    }

    /// `Render<HEAD_CROSSING_POLICY_STOP, true>`.
    #[inline]
    pub fn render<C: Codec>(&mut self, line: &DelayLine, buf: &[i16], out: &mut Frame) {
        if self.done() {
            return;
        }
        self.delay = self.delay.wrapping_sub(self.speed);
        if !line.frozen() {
            self.delay = self.delay.wrapping_add(1024);
        }
        let tail = line.max_delay() << 10;
        let fade_zone = HEAD_CROSSING_SAFETY_ZONE << 10;
        if self.delay < 0 || self.delay >= tail {
            self.delay = 0;
            self.envelope.stop();
        }
        let d = (if self.delay >= tail / 2 {
            self.delay - tail
        } else {
            self.delay
        })
        .abs();
        let fade_ramp = if d < fade_zone {
            d as f32 / fade_zone as f32
        } else {
            1.0
        };
        let integral = self.delay >> 10;
        let frac = (self.delay & 1023) as f32 / 1024.0;
        let mut sample = line.read_hermite::<C>(buf, integral, frac);
        sample *= smooth_step(fade_ramp);
        let e = self.envelope.render();
        let v = self.voice_stealing.render();
        sample *= e * v;
        out.l += sample.l * self.gain[0][0] + sample.r * self.gain[0][1];
        out.r += sample.l * self.gain[1][0] + sample.r * self.gain[1][1];
        self.remaining_samples -= 1;
        if self.done() {
            self.remaining_samples = 0;
        }
    }
}

pub const NUM_GRAINS: usize = 32;
const INSTANT_CROSSFADE_TIME: f32 = 0.005;
const SIMPLE_SCHEDULER_THRESHOLD: f32 = 0.025;
const PREEMPTIVE_CROSSFADE_TIME: f32 = 0.1;

/// `DelayGranularPlayer` (`GranularPlayer<DelayGrain, 32>`): starts grains on triggers and
/// steals the ones nearest their end when all 32 are busy.
pub struct GranularPlayer {
    grains: [DelayGrain; NUM_GRAINS],
    /// Indices into `grains`, best candidates to steal first.
    priority: [usize; NUM_GRAINS],
    preemptive_crossfade_time: i32,
    instant_crossfade_time: i32,
    simple_scheduler_threshold: i32,
    max_grain_duration: f32,
    sample_rate: f32,
    grain_interval: i32,
}

impl GranularPlayer {
    pub fn new(sample_rate: f32) -> Self {
        let mut p = Self {
            grains: [DelayGrain::default(); NUM_GRAINS],
            priority: std::array::from_fn(|i| i),
            instant_crossfade_time: (sample_rate * INSTANT_CROSSFADE_TIME) as i32,
            preemptive_crossfade_time: (sample_rate * PREEMPTIVE_CROSSFADE_TIME) as i32,
            simple_scheduler_threshold: (sample_rate * SIMPLE_SCHEDULER_THRESHOLD) as i32,
            max_grain_duration: sample_rate * 4.0,
            sample_rate,
            grain_interval: 1,
        };
        p.reset();
        p
    }

    pub fn reset(&mut self) {
        for (i, g) in self.grains.iter_mut().enumerate() {
            g.init(self.sample_rate);
            self.priority[i] = i;
        }
    }

    fn spread(trigger: &TriggerOutput, p: &Parameters) -> f32 {
        let mut total = 0.0f32;
        total += p.time.randomness_amount();
        total += p.size.randomness_amount();
        total += p.shape.randomness_amount();
        total += p.pitch.randomness_amount();
        total += 1.0 - trigger.regularity;
        total.min(1.0)
    }

    /// `ComputeGrainDuration`: SIZE to a length in samples (below a third, reversed).
    pub fn grain_duration(max_grain_duration: f32, duration: f32) -> f32 {
        let reverse = duration < 0.333_333_3;
        let clamped_shortness = if reverse {
            duration * 3.0 * (2.0 - duration * 3.0)
        } else {
            1.0 - 1.5 * (duration - 0.333_333_3)
        };
        let shortness = (1.025 * clamped_shortness).min(1.0);
        semitones_to_ratio(shortness * -108.0) * max_grain_duration
    }

    fn count_active(&self) -> i32 {
        self.grains.iter().filter(|g| !g.done()).count() as i32
    }

    fn amplitude_and_pan(
        &self,
        rng: &mut Random,
        trigger: &TriggerOutput,
        p: &Parameters,
    ) -> (f32, f32) {
        let active = (self.count_active() + 1) as f32;
        let amplitude = 1.0 / active.sqrt();
        let total_spread = Self::spread(trigger, p);
        let pan_spread = 0.5 * active * total_spread;
        (amplitude, rng.bounded(pan_spread))
    }

    /// `DelayGranularPlayer::Render`.
    pub fn render<C: Codec>(
        &mut self,
        rng: &mut Random,
        line: &DelayLine,
        buf: &[i16],
        trigger: &TriggerOutput,
        p: &Parameters,
        out: &mut Frame,
    ) {
        if trigger.trigger {
            let duration = p.size.value(rng);
            let reverse = duration < 0.333_333_3;
            let (amplitude, pan) = self.amplitude_and_pan(rng, trigger, p);
            let safety = HEAD_CROSSING_SAFETY_ZONE + 1;
            let max_delay = line.duration() - 2 * safety;
            // The C++ passes these as call arguments; clang evaluates them left to right,
            // which is the order the random draws are taken here.
            let delay = p.time.value(rng) * max_delay as f32 + safety as f32;
            let speed = (if reverse { -1.0 } else { 1.0 }) * p.pitch.pitch_value(rng);
            let length = Self::grain_duration(self.max_grain_duration, duration);
            let shape = p.shape.value(rng);
            let g = self.priority[0];
            self.grains[g].start(
                line,
                delay,
                speed,
                length,
                shape,
                amplitude,
                pan,
                trigger.fractional_sample,
            );
            self.sort_grain_list();
        }
        for g in self.grains.iter_mut() {
            g.render::<C>(line, buf, out);
        }
        self.grain_interval = self.grain_interval.wrapping_add(1);
    }

    /// `SortGrainList(false)`: after a new grain starts, the first grains of the priority
    /// list are faded so they end before they are stolen.
    fn sort_grain_list(&mut self) {
        const SAFETY: usize = 4;
        let mut fade_time = [0i32; SAFETY];
        if self.grain_interval < self.simple_scheduler_threshold {
            // Grains arrive very fast: steal the first one, the new grain goes to the end.
            self.priority.rotate_left(1);
            fade_time[0] = self.grain_interval + 1;
            fade_time[1] = self.instant_crossfade_time;
            fade_time[2] = self.instant_crossfade_time;
            fade_time[3] = self.preemptive_crossfade_time;
            if self.grain_interval > self.instant_crossfade_time {
                fade_time.swap(0, 2);
            }
        } else {
            let grains = &self.grains;
            let less = |a: &usize, b: &usize| {
                grains[*a].remaining_samples() < grains[*b].remaining_samples()
            };
            libcxx_partial_sort(&mut self.priority, SAFETY, &less);
            fade_time[0] = self.instant_crossfade_time;
            fade_time[1] = self.instant_crossfade_time;
            fade_time[2] = self.grain_interval;
            fade_time[3] = self.preemptive_crossfade_time;
            if self.grain_interval > self.preemptive_crossfade_time {
                fade_time.swap(2, 3);
            }
        }
        for (i, t) in fade_time.iter().enumerate() {
            let g = self.priority[i];
            self.grains[g].fade_out(*t);
        }
        self.grain_interval = 0;
    }

    /// Grains that are sounding.
    #[cfg(test)]
    pub fn active(&self) -> usize {
        self.count_active() as usize
    }
}

// `std::partial_sort` as LLVM's libc++ (the C++ library of Apple's clang) does it: a heap of
// the first `middle` elements, a pass over the rest, then a heap sort. Grains that tie (often
// several finished ones at 0) come out in the same order as in the C++ build, so the same
// grain slots are reused.

fn sift_down<F: Fn(&usize, &usize) -> bool>(v: &mut [usize], comp: &F, len: usize, start: usize) {
    let mut start = start;
    let mut child = start;
    if len < 2 || (len - 2) / 2 < child {
        return;
    }
    child = 2 * child + 1;
    if child + 1 < len && comp(&v[child], &v[child + 1]) {
        child += 1;
    }
    if comp(&v[child], &v[start]) {
        return;
    }
    let top = v[start];
    loop {
        v[start] = v[child];
        start = child;
        if (len - 2) / 2 < child {
            break;
        }
        child = 2 * child + 1;
        if child + 1 < len && comp(&v[child], &v[child + 1]) {
            child += 1;
        }
        if comp(&v[child], &top) {
            break;
        }
    }
    v[start] = top;
}

fn floyd_sift_down<F: Fn(&usize, &usize) -> bool>(v: &mut [usize], comp: &F, len: usize) -> usize {
    let mut hole = 0usize;
    let mut child_i = 0usize;
    let mut child = 0usize;
    loop {
        child_i += child + 1;
        child = 2 * child + 1;
        if child + 1 < len && comp(&v[child_i], &v[child_i + 1]) {
            child_i += 1;
            child += 1;
        }
        v[hole] = v[child_i];
        hole = child_i;
        if child > (len - 2) / 2 {
            return hole;
        }
    }
}

fn sift_up<F: Fn(&usize, &usize) -> bool>(v: &mut [usize], last: usize, comp: &F, len: usize) {
    if len > 1 {
        let mut len = (len - 2) / 2;
        let mut ptr = len;
        let mut last = last - 1;
        if comp(&v[ptr], &v[last]) {
            let t = v[last];
            loop {
                v[last] = v[ptr];
                last = ptr;
                if len == 0 {
                    break;
                }
                len = (len - 1) / 2;
                ptr = len;
                if !comp(&v[ptr], &t) {
                    break;
                }
            }
            v[last] = t;
        }
    }
}

fn pop_heap<F: Fn(&usize, &usize) -> bool>(v: &mut [usize], last: usize, comp: &F, len: usize) {
    if len > 1 {
        let top = v[0];
        let mut hole = floyd_sift_down(v, comp, len);
        let last = last - 1;
        if hole == last {
            v[hole] = top;
        } else {
            v[hole] = v[last];
            hole += 1;
            v[last] = top;
            sift_up(v, hole, comp, hole);
        }
    }
}

fn libcxx_partial_sort<F: Fn(&usize, &usize) -> bool>(v: &mut [usize], middle: usize, comp: &F) {
    if middle == 0 {
        return;
    }
    let len = middle;
    if len > 1 {
        let mut start = (len - 2) / 2;
        loop {
            sift_down(v, comp, len, start);
            if start == 0 {
                break;
            }
            start -= 1;
        }
    }
    for i in middle..v.len() {
        if comp(&v[i], &v[0]) {
            v.swap(i, 0);
            sift_down(v, comp, len, 0);
        }
    }
    let mut last = middle;
    let mut n = middle;
    while n > 1 {
        pop_heap(v, last, comp, n);
        last -= 1;
        n -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_sort_puts_the_smallest_first_in_order() {
        let keys = [5i32, 3, 9, 0, 0, 7, 1, 0, 4, 2, 8, 6];
        let mut v: Vec<usize> = (0..keys.len()).collect();
        let less = |a: &usize, b: &usize| keys[*a] < keys[*b];
        libcxx_partial_sort(&mut v, 4, &less);
        let first: Vec<i32> = v[..4].iter().map(|i| keys[*i]).collect();
        assert_eq!(first, vec![0, 0, 0, 1]);
        let mut all = v.clone();
        all.sort_unstable();
        assert_eq!(all, (0..keys.len()).collect::<Vec<_>>());
    }
}
