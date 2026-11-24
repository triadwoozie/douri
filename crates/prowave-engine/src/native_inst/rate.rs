// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Runs an engine built for one sample rate at the project's rate: the engine renders small
//! blocks at its own rate, and a windowed-sinc converter (the one Pro Wave Texture uses)
//! brings them to the output's.
//!
//! Logic Pro equivalent: none (Logic's instruments follow the project's sample rate).

use crate::native_fx::texture::dsp::Frame;
use crate::native_fx::texture::resample::Resampler;

/// An engine at `inner` Hz heard at `outer` Hz. All memory is allocated in
/// [`FixedRate::new`]; [`FixedRate::pull`] does not allocate.
pub struct FixedRate {
    resampler: Resampler,
    /// The engine's block, at its own rate.
    block_l: Vec<f32>,
    block_r: Vec<f32>,
    /// Converted frames not yet played.
    ready: Vec<Frame>,
    read: usize,
    len: usize,
    lag: f64,
}

impl FixedRate {
    /// `block` is the number of frames the engine renders at a time.
    pub fn new(inner: u32, outer: u32, block: usize) -> Self {
        let block = block.max(1);
        let resampler = Resampler::new(inner.max(1), outer.max(1));
        let most =
            (block as f64 * f64::from(outer.max(1)) / f64::from(inner.max(1))).ceil() as usize + 4;
        FixedRate {
            lag: resampler.lag_seconds(),
            resampler,
            block_l: vec![0.0; block],
            block_r: vec![0.0; block],
            ready: vec![Frame::ZERO; most],
            read: 0,
            len: 0,
        }
    }

    /// The converter's delay, in seconds.
    pub fn lag_seconds(&self) -> f64 {
        self.lag
    }

    /// Forgets what was rendered ahead.
    pub fn reset(&mut self) {
        self.resampler.reset();
        self.read = 0;
        self.len = 0;
    }

    /// Fills `left` and `right`; `render` writes one engine block (both slices, the block
    /// size given to [`FixedRate::new`]) each time it is called.
    pub fn pull(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        mut render: impl FnMut(&mut [f32], &mut [f32]),
    ) {
        let n = left.len().min(right.len());
        let mut at = 0;
        while at < n {
            if self.read == self.len {
                self.read = 0;
                self.len = 0;
                render(&mut self.block_l, &mut self.block_r);
                let (ready, len) = (&mut self.ready, &mut self.len);
                for (l, r) in self.block_l.iter().zip(&self.block_r) {
                    self.resampler.push(Frame::new(*l, *r), |f| {
                        if let Some(slot) = ready.get_mut(*len) {
                            *slot = f;
                            *len += 1;
                        }
                    });
                }
                continue;
            }
            let f = self.ready[self.read];
            self.read += 1;
            left[at] = f.l;
            right[at] = f.r;
            at += 1;
        }
    }
}

/// An effect built for one sample rate, run at the project's rate: the input is converted to
/// the effect's rate and gathered into its blocks, and what it returns is converted back. The
/// delay this takes is fixed and reported ([`FixedRateFx::latency_frames`]), and the dry
/// signal is delayed by the same amount for the mix. All memory is allocated in
/// [`FixedRateFx::new`]; [`FixedRateFx::process`] does not allocate.
pub struct FixedRateFx {
    down: Resampler,
    up: Resampler,
    block: usize,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    fill: usize,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    /// Converted output not yet played.
    ring: Vec<Frame>,
    read: usize,
    count: usize,
    /// The dry signal, `latency` frames long.
    dry: Vec<Frame>,
    dry_at: usize,
    latency: usize,
    /// Frames played as silence because the output was not ready (never, by design).
    underruns: u64,
}

impl FixedRateFx {
    /// `block` is the number of frames the effect takes at a time, at `inner` Hz.
    pub fn new(inner: u32, outer: u32, block: usize) -> Self {
        let (inner, outer, block) = (inner.max(1), outer.max(1), block.max(1));
        let down = Resampler::new(outer, inner);
        let up = Resampler::new(inner, outer);
        let block_seconds = block as f64 / f64::from(inner);
        let latency = ((block_seconds + down.lag_seconds() + up.lag_seconds()) * f64::from(outer))
            .ceil() as usize
            + 4;
        let per_block = (block_seconds * f64::from(outer)).ceil() as usize + 4;
        let mut fx = FixedRateFx {
            down,
            up,
            block,
            in_l: vec![0.0; block],
            in_r: vec![0.0; block],
            fill: 0,
            out_l: vec![0.0; block],
            out_r: vec![0.0; block],
            ring: vec![Frame::ZERO; latency + 2 * per_block + 16],
            read: 0,
            count: 0,
            dry: vec![Frame::ZERO; latency],
            dry_at: 0,
            latency,
            underruns: 0,
        };
        fx.reset();
        fx
    }

    /// The delay from input to output, in frames at the outer rate.
    pub fn latency_frames(&self) -> usize {
        self.latency
    }

    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    /// Forgets the sound so far.
    pub fn reset(&mut self) {
        self.down.reset();
        self.up.reset();
        self.fill = 0;
        self.ring.fill(Frame::ZERO);
        self.read = 0;
        // The output starts `latency` frames of silence ahead.
        self.count = self.latency;
        self.dry.fill(Frame::ZERO);
        self.dry_at = 0;
    }

    /// Processes `left` and `right` in place. `run(in_l, in_r, out_l, out_r)` is one block of
    /// the effect at its own rate; `mix(dry, wet)` gives each output frame from the delayed
    /// input and the effect's output.
    pub fn process(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        mut run: impl FnMut(&[f32], &[f32], &mut [f32], &mut [f32]),
        mut mix: impl FnMut(Frame, Frame) -> Frame,
    ) {
        let FixedRateFx {
            down,
            up,
            block,
            in_l,
            in_r,
            fill,
            out_l,
            out_r,
            ring,
            read,
            count,
            dry,
            dry_at,
            underruns,
            ..
        } = self;
        let cap = ring.len();
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let x = Frame::new(*l, *r);
            down.push(x, |f| {
                in_l[*fill] = f.l;
                in_r[*fill] = f.r;
                *fill += 1;
                if *fill < *block {
                    return;
                }
                *fill = 0;
                run(in_l, in_r, out_l, out_r);
                for (a, b) in out_l.iter().zip(out_r.iter()) {
                    up.push(Frame::new(*a, *b), |g| {
                        if *count < cap {
                            ring[(*read + *count) % cap] = g;
                            *count += 1;
                        }
                    });
                }
            });
            let wet = if *count > 0 {
                let f = ring[*read];
                *read = (*read + 1) % cap;
                *count -= 1;
                f
            } else {
                *underruns += 1;
                Frame::ZERO
            };
            let was = match dry.get_mut(*dry_at) {
                Some(slot) => std::mem::replace(slot, x),
                None => x,
            };
            *dry_at = (*dry_at + 1) % dry.len().max(1);
            let y = mix(was, wet);
            *l = y.l;
            *r = y.r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine made at 48 kHz keeps its frequency and level at 44.1 and 96 kHz, and is the
    /// engine's own output at 48 kHz.
    #[test]
    fn a_tone_keeps_its_pitch_and_level_and_pulling_does_not_allocate() {
        for outer in [44100u32, 48000, 96000] {
            let mut fixed = FixedRate::new(48000, outer, 12);
            let mut phase = 0.0f64;
            let n = outer as usize / 2;
            let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
            for (bl, br) in l.chunks_mut(100).zip(r.chunks_mut(100)) {
                assert_no_alloc::assert_no_alloc(|| {
                    fixed.pull(bl, br, |a, b| {
                        for (x, y) in a.iter_mut().zip(b.iter_mut()) {
                            *x = (phase * std::f64::consts::TAU).sin() as f32 * 0.5;
                            *y = *x;
                            phase += 1000.0 / 48000.0;
                        }
                    })
                });
            }
            assert_eq!(l, r);
            let tail = &l[n / 2..];
            let peak = tail.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!((peak - 0.5).abs() < 0.01, "{outer}: {peak}");
            let ups = tail
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count();
            let hz = ups as f64 / (tail.len() as f64 / f64::from(outer));
            assert!((hz - 1000.0).abs() < 8.0, "{outer}: {hz}");
        }
    }

    /// An effect that passes its input on comes out delayed by exactly the reported latency,
    /// with the dry signal beside it, at every rate and block size, and never runs dry.
    #[test]
    fn an_effect_at_another_rate_is_delayed_by_its_latency_and_does_not_allocate() {
        for (inner, block) in [(48000u32, 24usize), (32000, 32), (96000, 24)] {
            for outer in [44100u32, 48000, 88200, 96000] {
                let mut fx = FixedRateFx::new(inner, outer, block);
                let lat = fx.latency_frames();
                let n = outer as usize / 2;
                let x: Vec<f32> = (0..n)
                    .map(|i| {
                        (i as f64 * std::f64::consts::TAU * 500.0 / f64::from(outer)).sin() as f32
                            * 0.5
                    })
                    .collect();
                let (mut wet, mut dry) = (x.clone(), vec![0.0f32; n]);
                let mut at = 0;
                for chunk in [1usize, 7, 64, 333, 512].iter().cycle() {
                    if at >= n {
                        break;
                    }
                    let end = (at + chunk).min(n);
                    let (wl, dl) = (&mut wet[at..end], &mut dry[at..end]);
                    let mut scratch = wl.to_vec();
                    let mut k = 0;
                    assert_no_alloc::assert_no_alloc(|| {
                        fx.process(
                            wl,
                            &mut scratch,
                            |il, ir, ol, or| {
                                ol.copy_from_slice(il);
                                or.copy_from_slice(ir);
                            },
                            |d, w| {
                                dl[k] = d.l;
                                k += 1;
                                w
                            },
                        )
                    });
                    at = end;
                }
                assert_eq!(fx.underruns(), 0, "{inner} in {outer}");
                // The dry signal is the input, `lat` frames late, to the sample.
                assert_eq!(&dry[lat..], &x[..n - lat], "{inner} in {outer}");
                // The wet one is too, within the converters' error.
                let err = (lat + 2000..n)
                    .map(|i| (wet[i] - x[i - lat]).abs())
                    .fold(0.0f32, f32::max);
                assert!(err < 0.02, "{inner} in {outer}: off by {err}");
            }
        }
    }
}
