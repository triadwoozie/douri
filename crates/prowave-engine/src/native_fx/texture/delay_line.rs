// Copyright 2017 Emilie Gillet. MIT licence (the notice is in mod.rs).
// Rust port: Copyright (C) 2026 Pro Wave contributors, GPL-3.0-or-later.
//
// Ported from Beads' dsp/delay_line.h: the recording buffer the grains read from, stored at
// the quality mode's resolution (16-bit, 12-bit or 8-bit mu-law), with the freeze tail that
// crossfades the input back in when the buffer is unfrozen. Stereo only (Beads also has a
// mono layout of twice the length for one patched input).

use super::dsp::{clip16, lin_to_mu_law, mu_law_to_lin, smooth_step, Frame};

/// How a buffer stores a sample: two `i16` per frame in one shared allocation.
pub trait Codec {
    /// The stored value of silence (mu-law's is not 0).
    const ZERO: i16;
    fn store(s: f32) -> i16;
    fn load(v: i16) -> f32;
}

/// 16-bit linear (`Item<RESOLUTION_16_BIT, 2>`).
pub struct Pcm16;
impl Codec for Pcm16 {
    const ZERO: i16 = 0;
    #[inline]
    fn store(s: f32) -> i16 {
        clip16((s * 32768.0) as i32) as i16
    }
    #[inline]
    fn load(v: i16) -> f32 {
        f32::from(v) / 32768.0
    }
}

/// 12-bit linear (`Item<RESOLUTION_12_BIT, 2>`: a 12-bit signed field).
pub struct Pcm12;
impl Codec for Pcm12 {
    const ZERO: i16 = 0;
    #[inline]
    fn store(s: f32) -> i16 {
        (clip16((s * 32768.0) as i32) >> 4) as i16
    }
    #[inline]
    fn load(v: i16) -> f32 {
        f32::from(v) / 2048.0
    }
}

/// 8-bit mu-law (`Item<RESOLUTION_8_BIT, 2>`).
pub struct MuLaw8;
impl Codec for MuLaw8 {
    const ZERO: i16 = 0xff;
    #[inline]
    fn store(s: f32) -> i16 {
        i16::from(lin_to_mu_law(clip16((s * 32768.0) as i32) as i16))
    }
    #[inline]
    fn load(v: i16) -> f32 {
        mu_law_to_lin((v & 0xff) as u8)
    }
}

pub const INTERPOLATION_TAIL: usize = 4;
pub const FREEZE_TAIL_SIZE: usize = 256;

/// The frames a buffer of `max_delay` needs in the shared storage.
pub const fn storage_frames(max_delay: usize) -> usize {
    max_delay + INTERPOLATION_TAIL
}

/// The buffer's state; its samples live in a slice passed to each call.
pub struct DelayLine {
    max_delay: i32,
    write_ptr: i32,
    tail_write_ptr: i32,
    tail_xfade_duration: i32,
    written_since_last_frozen: i32,
    duration: i32,
    frozen: bool,
    freeze_tail: [Frame; FREEZE_TAIL_SIZE],
}

impl DelayLine {
    pub fn new(max_delay: usize) -> Self {
        Self {
            max_delay: max_delay as i32,
            write_ptr: 0,
            tail_write_ptr: 0,
            tail_xfade_duration: 0,
            written_since_last_frozen: 0,
            duration: max_delay as i32,
            frozen: false,
            freeze_tail: [Frame::ZERO; FREEZE_TAIL_SIZE],
        }
    }

    pub const fn max_delay(&self) -> i32 {
        self.max_delay
    }

    /// `Reset(true)`: empties the buffer.
    pub fn reset<C: Codec>(&mut self, buf: &mut [i16]) {
        self.write_ptr = 0;
        self.tail_write_ptr = 0;
        self.tail_xfade_duration = 0;
        self.frozen = false;
        self.written_since_last_frozen = 0;
        self.duration = self.max_delay;
        buf.fill(C::ZERO);
        self.freeze_tail = [Frame::ZERO; FREEZE_TAIL_SIZE];
    }

    #[inline]
    fn store<C: Codec>(buf: &mut [i16], i: i32, s: Frame) {
        let i = 2 * i as usize;
        if let Some(pair) = buf.get_mut(i..i + 2) {
            pair[0] = C::store(s.l);
            pair[1] = C::store(s.r);
        }
    }

    #[inline]
    fn load<C: Codec>(buf: &[i16], i: i32) -> Frame {
        let i = 2 * i as usize;
        match buf.get(i..i + 2) {
            Some(pair) => Frame::new(C::load(pair[0]), C::load(pair[1])),
            None => Frame::ZERO,
        }
    }

    fn write_plain<C: Codec>(&mut self, buf: &mut [i16], input: Frame) {
        Self::store::<C>(buf, self.write_ptr, input);
        if self.write_ptr < INTERPOLATION_TAIL as i32 {
            Self::store::<C>(buf, self.write_ptr + self.max_delay, input);
        }
        if self.write_ptr == 0 {
            self.write_ptr = self.max_delay - 1;
        } else {
            self.write_ptr -= 1;
        }
        self.written_since_last_frozen += 1;
        if self.written_since_last_frozen >= self.max_delay {
            self.written_since_last_frozen = 0;
        }
        self.duration += 1;
        if self.duration >= self.max_delay {
            self.duration = self.max_delay;
        }
    }

    /// `Write(in, frozen)`: records unless frozen; while frozen the input goes to the freeze
    /// tail, which is crossfaded back in when recording resumes.
    pub fn write<C: Codec>(&mut self, buf: &mut [i16], mut input: Frame, frozen: bool) {
        if frozen {
            if !self.frozen {
                self.tail_write_ptr = 0;
            }
            if (self.tail_write_ptr as usize) < FREEZE_TAIL_SIZE {
                self.freeze_tail[self.tail_write_ptr as usize] = input;
                self.tail_write_ptr += 1;
            }
            self.written_since_last_frozen = 0;
        } else {
            if self.frozen && self.tail_write_ptr != 0 {
                self.tail_xfade_duration = self.tail_write_ptr;
            }
            if self.tail_write_ptr != 0 {
                let tail_position = (self.tail_xfade_duration - self.tail_write_ptr)
                    .clamp(0, FREEZE_TAIL_SIZE as i32 - 1)
                    as usize;
                let t = self.tail_write_ptr as f32 / self.tail_xfade_duration as f32;
                let gain = smooth_step(t);
                let tail = self.freeze_tail[tail_position];
                input.l += (tail.l - input.l) * gain;
                input.r += (tail.r - input.r) * gain;
                self.tail_write_ptr -= 1;
            }
            self.write_plain::<C>(buf, input);
        }
        self.frozen = frozen;
    }

    /// `Read<INTERPOLATION_HERMITE>(delay_integral, delay_fractional)`.
    #[inline]
    pub fn read_hermite<C: Codec>(&self, buf: &[i16], delay_integral: i32, frac: f32) -> Frame {
        let delay_integral = delay_integral.wrapping_add(self.max_delay - 1);
        let i = self
            .write_ptr
            .wrapping_add(delay_integral)
            .rem_euclid(self.max_delay);
        let x0 = Self::load::<C>(buf, i);
        let x1 = Self::load::<C>(buf, i + 1);
        let x2 = Self::load::<C>(buf, i + 2);
        let x3 = Self::load::<C>(buf, i + 3);
        let c = (x2 - x0) * 0.5;
        let v = x1 - x2;
        let w = c + v;
        let a = w + v + (x3 - x1) * 0.5;
        let b_neg = w + a;
        let t = frac;
        (((a * t) - b_neg) * t + c) * frac + x1
    }

    /// `Read<INTERPOLATION_HERMITE>(int32_t delay)`: a delay in 1/1024 samples.
    #[inline]
    pub fn read_hermite_fixed<C: Codec>(&self, buf: &[i16], delay: i32) -> Frame {
        let integral = delay >> 10;
        let frac = (delay & 1023) as f32 / 1024.0;
        self.read_hermite::<C>(buf, integral, frac)
    }

    pub const fn frozen(&self) -> bool {
        self.frozen
    }

    pub const fn duration(&self) -> i32 {
        self.duration
    }

    pub const fn remaining(&self) -> i32 {
        self.max_delay - self.written_since_last_frozen
    }
}
