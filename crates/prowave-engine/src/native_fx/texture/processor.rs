// Copyright 2017-2021 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from Beads' dsp/granular_processor.h, dsp/processor.cc (with ENABLE_HARDWARE_24K, as
// built for the module), the 2x sample-rate converters of stmlib/dsp/sample_rate_converter.h
// and their filters from dsp/sample_rate_conversion_filters.h. Runs at the module's codec
// rate (48, 32 or 24 kHz depending on the quality mode) in blocks of rate / 4000 frames.

use super::delay_line::{storage_frames, Codec, DelayLine, MuLaw8, Pcm12, Pcm16};
use super::delay_mode::{DelayController, MultitapDelay, TapsOutput, NUM_TAPS};
use super::dsp::{
    clip16, constrain, lin_to_mu_law, mu_law_to_lin, one_pole, semitones_to_ratio, smooth_step,
    soft_clip, Frame, Interp, Limiter, Random, Svf,
};
use super::grain::{GranularPlayer, Parameters, RandomWalk};
use super::space::{Reverb, WowAndFlutter};
use super::trigger::{SeedState, TriggerGenerator, TriggerOutput};

/// The largest block: 48 kHz / 4000.
pub const MAX_BLOCK: usize = 12;
/// Blocks per second at every codec rate.
pub const BLOCK_RATE: u32 = 4000;

const DELAY_MODE_TRANSITION_TIME: f32 = 1.5;
const DELAY_MODE_FADE_TIME: f32 = 0.75;

/// Beads' four quality settings ("media"), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    /// 24 kHz, 8-bit mu-law, wow and flutter; 16 s of buffer.
    DirtyTape,
    /// 48 kHz codec, buffer at 24 kHz 12-bit; 10.7 s.
    CleanTape,
    /// 32 kHz, 12-bit; 8 s.
    VintageDigital,
    /// 48 kHz, 16-bit; 4 s.
    Digital,
}

impl Media {
    pub const ALL: [Media; 4] = [
        Media::DirtyTape,
        Media::CleanTape,
        Media::VintageDigital,
        Media::Digital,
    ];

    /// The codec rate Beads runs this mode at.
    pub const fn sample_rate(self) -> u32 {
        match self {
            Media::DirtyTape => 24000,
            Media::CleanTape => 48000,
            Media::VintageDigital => 32000,
            Media::Digital => 48000,
        }
    }

    const fn ratio(self) -> usize {
        match self {
            Media::CleanTape => 2,
            _ => 1,
        }
    }

    /// The buffer length in frames at the buffer's own rate.
    const fn max_delay(self) -> usize {
        // kDelayBufferSizeBits / (bits * 2): 768000 bytes of memory.
        match self {
            Media::DirtyTape => 768 * 1000 * 8 / 16,
            Media::CleanTape | Media::VintageDigital => 768 * 1000 * 8 / 24,
            Media::Digital => 768 * 1000 * 8 / 32,
        }
    }

    /// The buffer length in seconds.
    pub fn buffer_seconds(self) -> f32 {
        self.max_delay() as f32 * self.ratio() as f32 / self.sample_rate() as f32
    }

    pub const fn block_size(self) -> usize {
        (self.sample_rate() / BLOCK_RATE) as usize
    }
}

#[allow(clippy::excessive_precision)]
const SRC_UP_H: [f32; 24] = [
    -4.287457573e-04,
    -8.226487157e-04,
    7.100052364e-04,
    2.247177610e-03,
    4.027580292e-04,
    -4.271779932e-03,
    -3.893072135e-03,
    4.843915649e-03,
    1.019934854e-02,
    -9.694387912e-04,
    -1.727659358e-02,
    -1.027548927e-02,
    2.001194882e-02,
    2.929003471e-02,
    -1.083867877e-02,
    -5.164495731e-02,
    -1.821391107e-02,
    6.671043028e-02,
    7.404344683e-02,
    -5.604828388e-02,
    -1.683376729e-01,
    -2.385313356e-02,
    3.910061488e-01,
    7.674091912e-01,
];

#[allow(clippy::excessive_precision)]
const SRC_DOWN_H: [f32; 24] = [
    -2.143728787e-04,
    -4.113243578e-04,
    3.550026182e-04,
    1.123588805e-03,
    2.013790146e-04,
    -2.135889966e-03,
    -1.946536068e-03,
    2.421957824e-03,
    5.099674271e-03,
    -4.847193956e-04,
    -8.638296788e-03,
    -5.137744633e-03,
    1.000597441e-02,
    1.464501735e-02,
    -5.419339385e-03,
    -2.582247866e-02,
    -9.106955536e-03,
    3.335521514e-02,
    3.702172341e-02,
    -2.802414194e-02,
    -8.416883646e-02,
    -1.192656678e-02,
    1.955030744e-01,
    3.837045956e-01,
];

const SRC_TAPS: usize = 48;

/// The filter's tap `i` (the tables hold half of a symmetric response).
#[inline]
fn mirrored(h: &[f32; 24], i: usize) -> f32 {
    if i >= SRC_TAPS / 2 {
        h[SRC_TAPS - 1 - i]
    } else {
        h[i]
    }
}

/// `SampleRateConverter<SRC_DOWN, 2, 48>` (its circular-buffer path, used for short blocks).
#[derive(Clone, Copy)]
struct SrcDown {
    x: [f32; 2 * SRC_TAPS],
    ptr: usize,
}

impl SrcDown {
    fn new() -> Self {
        Self {
            x: [0.0; 2 * SRC_TAPS],
            ptr: SRC_TAPS - 1,
        }
    }
    fn process(&mut self, input: &[f32], out: &mut [f32]) {
        for (pair, o) in input.as_chunks::<2>().0.iter().zip(out.iter_mut()) {
            for s in pair {
                self.x[self.ptr] = *s;
                self.x[self.ptr + SRC_TAPS] = *s;
                if self.ptr == 0 {
                    self.ptr = SRC_TAPS - 1;
                } else {
                    self.ptr -= 1;
                }
            }
            let x = &self.x[self.ptr + 1..self.ptr + 1 + SRC_TAPS];
            let mut acc = 0.0f32;
            for i in (0..SRC_TAPS).rev() {
                acc += x[i] * mirrored(&SRC_DOWN_H, i);
            }
            *o = acc;
        }
    }
}

/// `SampleRateConverter<SRC_UP, 2, 48>`.
#[derive(Clone, Copy)]
struct SrcUp {
    /// Newest first.
    x: [f32; SRC_TAPS / 2],
}

impl SrcUp {
    fn new() -> Self {
        Self {
            x: [0.0; SRC_TAPS / 2],
        }
    }
    fn process(&mut self, input: &[f32], out: &mut [f32]) {
        const N: usize = SRC_TAPS / 2;
        for (s, pair) in input.iter().zip(out.as_chunks_mut::<2>().0.iter_mut()) {
            self.x.copy_within(0..N - 1, 1);
            self.x[0] = *s;
            for (k, o) in pair.iter_mut().enumerate() {
                let mut acc = 0.0f32;
                for i in (0..N).rev() {
                    acc += self.x[i] * mirrored(&SRC_UP_H, k + i * 2);
                }
                *o = acc;
            }
        }
    }
}

/// The resolution-dependent constants of `ProcessInternal`.
fn resolution_constants<C: 'static>() -> (f32, f32) {
    use std::any::TypeId;
    let t = TypeId::of::<C>();
    if t == TypeId::of::<MuLaw8>() {
        (0.75, 1.75)
    } else if t == TypeId::of::<Pcm12>() {
        (0.9, 1.3)
    } else {
        (0.999, 0.0)
    }
}

/// `GranularProcessor<resolution, max_delay, ratio>` (stereo layout).
pub struct GranularProcessor<C: Codec> {
    line: DelayLine,
    player: GranularPlayer,
    multitap: MultitapDelay,
    ratio: usize,
    src_down: [SrcDown; 2],
    src_up: [SrcUp; 2],
    feedback: f32,
    speed: f32,
    bandwidth: f32,
    previous_sample: Frame,
    dc_blocker: [Svf; 2],
    pitch_modulation: RandomWalk,
    input_limiter: Limiter,
    lp_coefficient: f32,
    soft_limit_gain: f32,
    _codec: std::marker::PhantomData<C>,
}

impl<C: Codec + 'static> GranularProcessor<C> {
    fn new(sample_rate: f32, max_delay: usize, ratio: usize) -> Self {
        let mut dc = [Svf::default(); 2];
        for f in &mut dc {
            f.init();
            f.set_f_q_fast(40.0 / sample_rate, 0.5);
        }
        let mut pitch_modulation = RandomWalk::default();
        pitch_modulation.init(0.2 / sample_rate);
        let mut input_limiter = Limiter::default();
        input_limiter.init(1.0 / sample_rate, 0.95);
        let (lp_coefficient, soft_limit_gain) = resolution_constants::<C>();
        Self {
            line: DelayLine::new(max_delay),
            player: GranularPlayer::new(sample_rate),
            multitap: MultitapDelay::new(),
            ratio,
            src_down: [SrcDown::new(); 2],
            src_up: [SrcUp::new(); 2],
            feedback: 0.0,
            speed: 0.0,
            bandwidth: 0.0,
            previous_sample: Frame::ZERO,
            dc_blocker: dc,
            pitch_modulation,
            input_limiter,
            lp_coefficient,
            soft_limit_gain,
            _codec: std::marker::PhantomData,
        }
    }

    fn reset(&mut self, buf: &mut [i16]) {
        self.line.reset::<C>(buf);
        self.player.reset();
        self.multitap.reset();
    }

    fn max_delay_times_ratio(&self) -> i32 {
        self.line.max_delay() * self.ratio as i32
    }

    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        rng: &mut Random,
        buf: &mut [i16],
        triggers: Option<&[TriggerOutput]>,
        taps: Option<&[TapsOutput]>,
        freeze: &[bool],
        p: &Parameters,
        feedback: f32,
        input: &[Frame],
        out: &mut [Frame],
    ) {
        if self.ratio == 1 {
            self.process_internal(rng, buf, triggers, taps, freeze, p, feedback, input, out);
            return;
        }
        let size = input.len().min(out.len());
        let ratio = self.ratio;
        let d_size = size / ratio;
        let mut d_freeze = [false; MAX_BLOCK];
        let mut d_triggers = [TriggerOutput::default(); MAX_BLOCK];
        let mut d_taps = [TapsOutput::default(); MAX_BLOCK];
        let mut d_in = [Frame::ZERO; MAX_BLOCK];
        let mut d_out = [Frame::ZERO; MAX_BLOCK];
        let mut downsampled = [0.0f32; MAX_BLOCK];
        let mut upsampled = [0.0f32; MAX_BLOCK];

        for (o, pair) in d_freeze.iter_mut().zip(freeze[..size].chunks_exact(ratio)) {
            *o = pair.iter().any(|f| *f);
        }
        if let Some(t) = triggers {
            let inv_ratio = 1.0 / ratio as f32;
            for (o, group) in d_triggers.iter_mut().zip(t[..size].chunks_exact(ratio)) {
                o.regularity = group[0].regularity;
                let mut has_trigger = false;
                let mut fractional_sample = 0.0;
                let mut gate = false;
                for (j, x) in group.iter().enumerate() {
                    if x.trigger && !has_trigger {
                        has_trigger = true;
                        fractional_sample = (x.fractional_sample + j as f32) * inv_ratio;
                    }
                    gate |= x.gate;
                }
                o.trigger = has_trigger;
                o.gate = gate;
                o.fractional_sample = fractional_sample;
            }
        }
        if let Some(t) = taps {
            for (o, x) in d_taps.iter_mut().zip(t[..size].iter().step_by(ratio)) {
                for j in 0..NUM_TAPS {
                    o.tap[j].delay = x.tap[j].delay / ratio as i32;
                    o.tap[j].gain = x.tap[j].gain;
                }
            }
        }
        for (channel, src_down) in self.src_down.iter_mut().enumerate() {
            for (u, f) in upsampled.iter_mut().zip(&input[..size]) {
                *u = if channel == 0 { f.l } else { f.r };
            }
            src_down.process(&upsampled[..size], &mut downsampled[..d_size]);
            for (d, s) in d_in.iter_mut().zip(&downsampled[..d_size]) {
                if channel == 0 {
                    d.l = *s;
                } else {
                    d.r = *s;
                }
            }
        }
        self.process_internal(
            rng,
            buf,
            triggers.map(|_| &d_triggers[..d_size]),
            taps.map(|_| &d_taps[..d_size]),
            &d_freeze[..d_size],
            p,
            feedback,
            &d_in[..d_size],
            &mut d_out[..d_size],
        );
        for (channel, src_up) in self.src_up.iter_mut().enumerate() {
            for (d, f) in downsampled.iter_mut().zip(&d_out[..d_size]) {
                *d = if channel == 0 { f.l } else { f.r };
            }
            src_up.process(&downsampled[..d_size], &mut upsampled[..size]);
            for (o, s) in out[..size].iter_mut().zip(&upsampled[..size]) {
                if channel == 0 {
                    o.l = *s;
                } else {
                    o.r = *s;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn process_internal(
        &mut self,
        rng: &mut Random,
        buf: &mut [i16],
        triggers: Option<&[TriggerOutput]>,
        taps: Option<&[TapsOutput]>,
        freeze: &[bool],
        p: &Parameters,
        feedback: f32,
        input: &[Frame],
        out: &mut [Frame],
    ) {
        let size = input.len().min(out.len()).min(freeze.len());
        if size == 0 {
            return;
        }
        let speed = semitones_to_ratio(p.pitch.value_full(
            rng,
            Some((&mut self.pitch_modulation, size)),
            24.0,
            false,
        ));
        let bandwidth = constrain(0.5 * (1.0 + p.size.attenuverter), 0.0, 1.0);
        let mut bandwidth_mod = Interp::new(self.bandwidth, bandwidth, size);
        let mut feedback_mod = Interp::new(self.feedback, feedback, size);
        let mut speed_mod = Interp::new(self.speed, speed, size);
        let max_feedback_gain = if taps.is_some() { 1.5 } else { 3.0 };
        let lp_coefficient = self.lp_coefficient;
        let soft_limit_gain = self.soft_limit_gain;

        for i in 0..size {
            let feedback = feedback_mod.step();
            let input_frame = input[i];
            let mut s = input_frame;
            if soft_limit_gain != 0.0 {
                let gain = max_feedback_gain / soft_limit_gain;
                s += self.previous_sample * feedback * gain;
            } else {
                s += self.previous_sample * feedback * feedback * max_feedback_gain;
            }
            self.input_limiter.process_one(&mut s);
            if soft_limit_gain != 0.0 {
                s *= soft_limit_gain;
                s.l = soft_clip(s.l);
                s.r = soft_clip(s.r);
                s = input_frame + (s - input_frame) * feedback;
            }
            let mut o = Frame::ZERO;
            self.line.write::<C>(buf, s, freeze[i]);
            if let Some(t) = triggers.and_then(|t| t.get(i)) {
                self.player.render::<C>(rng, &self.line, buf, t, p, &mut o);
            }
            if let Some(t) = taps.and_then(|t| t.get(i)) {
                let sp = speed_mod.step();
                let bw = bandwidth_mod.step();
                self.multitap
                    .render::<C>(&self.line, buf, t, sp, bw, &mut o);
            }
            let hp_l = self.dc_blocker[0].high_pass(o.l);
            one_pole(&mut self.previous_sample.l, hp_l, lp_coefficient);
            let hp_r = self.dc_blocker[1].high_pass(o.r);
            one_pole(&mut self.previous_sample.r, hp_r, lp_coefficient);
            out[i] = o;
        }
        bandwidth_mod.store(&mut self.bandwidth);
        feedback_mod.store(&mut self.feedback);
        speed_mod.store(&mut self.speed);
    }
}

/// The knob and mix settings for one block.
#[derive(Clone, Copy, Debug, Default)]
pub struct Settings {
    /// DENSITY, 0..1 (below 0.48 regular, above 0.52 random).
    pub density: f32,
    pub params: Parameters,
    pub feedback: f32,
    pub dry_wet: f32,
    pub reverb: f32,
}

/// `beads::Processor`: the whole effect at the module's codec rate.
pub struct Processor {
    media: Media,
    pub rng: Random,
    /// The delay memory every quality mode shares, as on the module.
    buf: Vec<i16>,
    digital: GranularProcessor<Pcm16>,
    vintage_digital: GranularProcessor<Pcm12>,
    clean_tape: GranularProcessor<Pcm12>,
    dirty_tape: GranularProcessor<MuLaw8>,
    trigger: TriggerGenerator,
    delay_controller: DelayController,
    delay_mode_transition: f32,
    dry: f32,
    wet: f32,
    wow_and_flutter: WowAndFlutter,
    reverb: Reverb,
    pre_reverb_limiter: Limiter,
    output_limiter: Limiter,
    triggers: [TriggerOutput; MAX_BLOCK],
    taps: [TapsOutput; MAX_BLOCK],
}

impl Processor {
    /// Allocates the buffers (call off the audio thread). Starts in `media`.
    pub fn new(media: Media, seed: u32) -> Self {
        let frames = Media::ALL
            .iter()
            .map(|m| storage_frames(m.max_delay()))
            .max()
            .unwrap_or(0);
        let mut p = Self {
            media,
            rng: Random(seed),
            buf: vec![0; 2 * frames],
            digital: GranularProcessor::new(48000.0, Media::Digital.max_delay(), 1),
            vintage_digital: GranularProcessor::new(32000.0, Media::VintageDigital.max_delay(), 1),
            clean_tape: GranularProcessor::new(24000.0, Media::CleanTape.max_delay(), 2),
            dirty_tape: GranularProcessor::new(24000.0, Media::DirtyTape.max_delay(), 1),
            trigger: TriggerGenerator::new(48000.0),
            delay_controller: DelayController::new(48000.0, 1),
            delay_mode_transition: -DELAY_MODE_TRANSITION_TIME,
            dry: 0.0,
            wet: 0.0,
            wow_and_flutter: WowAndFlutter::new(24000.0, 0.95),
            reverb: Reverb::new(48000.0),
            pre_reverb_limiter: Limiter::default(),
            output_limiter: Limiter::default(),
            triggers: [TriggerOutput::default(); MAX_BLOCK],
            taps: [TapsOutput::default(); MAX_BLOCK],
        };
        p.configure(media);
        p
    }

    pub fn media(&self) -> Media {
        self.media
    }

    pub fn sample_rate(&self) -> f32 {
        self.media.sample_rate() as f32
    }

    /// `Configure(media, false)`: switches the quality mode, emptying the buffer. Real-time
    /// safe (it clears the existing memory).
    pub fn configure(&mut self, media: Media) {
        self.media = media;
        let max_delay = match media {
            Media::Digital => {
                self.digital.reset(&mut self.buf);
                self.digital.max_delay_times_ratio()
            }
            Media::VintageDigital => {
                self.vintage_digital.reset(&mut self.buf);
                self.vintage_digital.max_delay_times_ratio()
            }
            Media::CleanTape => {
                self.clean_tape.reset(&mut self.buf);
                self.clean_tape.max_delay_times_ratio()
            }
            Media::DirtyTape => {
                self.dirty_tape.reset(&mut self.buf);
                self.dirty_tape.max_delay_times_ratio()
            }
        };
        let sr = self.sample_rate();
        self.pre_reverb_limiter.init(1.0 / (sr * 4.0), 0.9);
        self.output_limiter.init(1.0 / (sr * 6.0), 0.9);
        self.trigger = TriggerGenerator::new(sr);
        self.delay_controller = DelayController::new(sr, max_delay);
        self.reverb.reset(sr);
    }

    /// A checksum of the delay memory (tests: a frozen buffer must not change).
    #[cfg(test)]
    pub fn buffer_checksum(&self) -> u64 {
        self.buf.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, v| {
            (h ^ u64::from(*v as u16)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Whether the buffer is in delay mode (SIZE fully clockwise for a while).
    pub fn delay_mode(&self) -> bool {
        self.delay_mode_transition > 0.0
    }

    /// `Process`: one block of at most [`MAX_BLOCK`] frames at the codec rate.
    pub fn process(
        &mut self,
        seed: SeedState,
        freeze: &[bool],
        s: &Settings,
        input: &[Frame],
        out: &mut [Frame],
    ) {
        let size = input.len().min(out.len()).min(freeze.len()).min(MAX_BLOCK);
        if size == 0 {
            return;
        }
        let (input, out, freeze) = (&input[..size], &mut out[..size], &freeze[..size]);
        let delay_mode = self.delay_mode_transition > 0.0;
        let threshold = if delay_mode { 0.98 } else { 0.99 };
        let direction = if s.params.size.base > threshold {
            1.0
        } else {
            -1.0
        };
        self.delay_mode_transition += direction * size as f32 / self.sample_rate();
        self.delay_mode_transition = constrain(
            self.delay_mode_transition,
            -DELAY_MODE_TRANSITION_TIME,
            DELAY_MODE_TRANSITION_TIME,
        );
        let expo_fm_amount = constrain(20.0 * ((s.density - 0.5).abs() - 0.225), 0.0, 1.0);
        // No density CV: the linear and exponential modulations are 0.
        let fm = expo_fm_amount * 0.0;
        if delay_mode {
            self.delay_controller.render(
                &mut self.rng,
                seed,
                freeze,
                s.density,
                (1.0 - expo_fm_amount) * 0.0,
                fm,
                &s.params.time,
                &s.params.shape,
                &mut self.taps[..size],
            );
        } else {
            self.trigger.render(
                &mut self.rng,
                seed,
                s.density,
                fm,
                &mut self.triggers[..size],
            );
        }
        let triggers = (!delay_mode).then_some(&self.triggers[..size]);
        let taps = delay_mode.then_some(&self.taps[..size]);

        let mut dry_signal = [Frame::ZERO; MAX_BLOCK];
        let rng = &mut self.rng;
        let buf = &mut self.buf;
        match self.media {
            Media::Digital => {
                self.digital.process(
                    rng, buf, triggers, taps, freeze, &s.params, s.feedback, input, out,
                );
                dry_signal[..size].copy_from_slice(input);
            }
            Media::VintageDigital => {
                self.vintage_digital.process(
                    rng, buf, triggers, taps, freeze, &s.params, s.feedback, input, out,
                );
                for (d, x) in dry_signal.iter_mut().zip(input) {
                    d.l = ((x.l * 4096.0) as i32) as f32 / 4096.0;
                    d.r = ((x.r * 4096.0) as i32) as f32 / 4096.0;
                }
            }
            Media::CleanTape => {
                self.clean_tape.process(
                    rng, buf, triggers, taps, freeze, &s.params, s.feedback, input, out,
                );
                dry_signal[..size].copy_from_slice(input);
            }
            Media::DirtyTape => {
                self.dirty_tape.process(
                    rng, buf, triggers, taps, freeze, &s.params, s.feedback, input, out,
                );
                let mu = |v: f32| mu_law_to_lin(lin_to_mu_law(clip16((v * 32768.0) as i32) as i16));
                for (d, x) in dry_signal.iter_mut().zip(input) {
                    d.l = mu(x.l);
                    d.r = mu(x.r);
                }
            }
        }

        // Fade-out / fade-in the wet signal when switching between granular and delay mode.
        let wet_fade =
            smooth_step((self.delay_mode_transition.abs() / DELAY_MODE_FADE_TIME).min(1.0));
        let post_gain_dry = 0.75;
        let post_gain_wet = 1.5;
        let dry_wet = s.dry_wet;
        let dry = (1.0 - dry_wet) * (1.0 + dry_wet) * post_gain_dry;
        let wet = dry_wet * (2.0 - dry_wet) * post_gain_wet * wet_fade;
        let mut dry_mod = Interp::new(self.dry, dry, size);
        let mut wet_mod = Interp::new(self.wet, wet, size);
        for (o, d) in out.iter_mut().zip(&dry_signal[..size]) {
            let a = dry_mod.step();
            let b = wet_mod.step();
            *o = *d * a + *o * b;
        }
        dry_mod.store(&mut self.dry);
        wet_mod.store(&mut self.wet);

        if self.media == Media::DirtyTape {
            self.wow_and_flutter.process(&mut self.rng, out);
        }
        self.pre_reverb_limiter.process(out);

        let caveman = self.media != Media::Digital;
        let reverb = s.reverb;
        let reverb_mix = reverb * reverb * 0.9;
        let reverb_time = reverb * reverb * reverb;
        self.reverb.set_mix(reverb_mix);
        self.reverb.set_time(if caveman {
            0.45 + 0.5 * reverb_time
        } else {
            0.3 + 0.6 * reverb_time
        });
        self.reverb.set_lp(if caveman {
            0.4
        } else if self.media == Media::VintageDigital {
            0.75
        } else {
            0.6
        });
        self.reverb.process(out);
        self.output_limiter.process(out);
    }
}
