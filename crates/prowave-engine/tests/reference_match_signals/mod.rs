//! Test songs for the reference-match tests: noise with a spectrum, a stereo width and a
//! loud/quiet shape, made the same way every run.
#![allow(dead_code)]

use std::io::Write;
use std::path::Path;

/// xorshift64, uniform in [-1, 1).
pub struct Noise(pub u64);

impl Noise {
    pub fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// Pink noise (Paul Kellet's filter): -3 dB per octave.
pub fn pink(seed: u64, n: usize) -> Vec<f64> {
    let mut w = Noise(seed);
    let mut b = [0.0f64; 7];
    (0..n)
        .map(|_| {
            let white = w.next();
            b[0] = 0.99886 * b[0] + white * 0.0555179;
            b[1] = 0.99332 * b[1] + white * 0.0750759;
            b[2] = 0.96900 * b[2] + white * 0.1538520;
            b[3] = 0.86650 * b[3] + white * 0.3104856;
            b[4] = 0.55000 * b[4] + white * 0.5329522;
            b[5] = -0.7616 * b[5] - white * 0.0168980;
            let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.5362;
            b[6] = white * 0.115926;
            out * 0.11
        })
        .collect()
}

/// `x` tilted: `amount` > 0 brighter (adds the first difference), < 0 darker (a one-pole
/// low-pass mixed in).
pub fn tilt(x: &[f64], amount: f64) -> Vec<f64> {
    let mut prev = 0.0;
    let mut lp = 0.0;
    x.iter()
        .map(|&v| {
            let d = v - prev;
            prev = v;
            lp += 0.05 * (v - lp);
            if amount >= 0.0 {
                v + amount * d
            } else {
                v + (-amount) * (lp * 4.0 - v)
            }
        })
        .collect()
}

/// A loud/quiet shape: sections of `seconds` at the given levels, with short ramps between.
pub fn shape(x: &mut [f64], rate: u32, seconds: f64, levels: &[f64]) {
    let sec = (seconds * f64::from(rate)) as usize;
    let ramp = (0.05 * f64::from(rate)) as usize;
    for (i, v) in x.iter_mut().enumerate() {
        let k = (i / sec) % levels.len();
        let next = (k + 1) % levels.len();
        let into = i % sec;
        let g = if into + ramp >= sec {
            let t = (into + ramp - sec) as f64 / ramp as f64;
            levels[k] + (levels[next] - levels[k]) * t
        } else {
            levels[k]
        };
        *v *= g;
    }
}

/// A stereo song: `mid` and a side of its own noise at `width`, scaled to `gain`.
pub fn stereo(mid: &[f64], side_seed: u64, width: f64, gain: f64) -> (Vec<f64>, Vec<f64>) {
    let side = pink(side_seed, mid.len());
    let l = mid
        .iter()
        .zip(&side)
        .map(|(m, s)| (m + width * s) * gain)
        .collect();
    let r = mid
        .iter()
        .zip(&side)
        .map(|(m, s)| (m - width * s) * gain)
        .collect();
    (l, r)
}

/// Writes 32-bit float WAV (one or two channels).
pub fn write_f32_wav(path: &Path, channels: &[&[f64]], rate: u32) {
    let ch = channels.len() as u16;
    let n = channels[0].len();
    let data = (n * channels.len() * 4) as u32;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&3u16.to_le_bytes()).unwrap();
    f.write_all(&ch.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 4 * u32::from(ch)).to_le_bytes())
        .unwrap();
    f.write_all(&(4 * ch).to_le_bytes()).unwrap();
    f.write_all(&32u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..n {
        for c in channels {
            f.write_all(&(c[i] as f32).to_le_bytes()).unwrap();
        }
    }
    f.flush().unwrap();
}

/// Reads a file as Matchering would get it: its channels (one or two) at its rate.
pub fn read_input(path: &Path) -> prowave_engine::reference_match::Input {
    let f = prowave_engine::AudioFile::open(path).unwrap();
    let st = f.read_stereo_up_to(900.0, |_, _| true).unwrap().unwrap();
    let l: Vec<f64> = st.iter().step_by(2).map(|v| f64::from(*v)).collect();
    let r: Vec<f64> = st
        .iter()
        .skip(1)
        .step_by(2)
        .map(|v| f64::from(*v))
        .collect();
    let channels = if f.channels == 1 { vec![l] } else { vec![l, r] };
    prowave_engine::reference_match::Input {
        channels,
        rate: f.sample_rate,
    }
}

/// Two one-pole high-passes at `hz` in series: what is under it goes, as it is gone from
/// mixed and mastered music (pink noise alone carries a lot below 20 Hz).
pub fn highpass(x: &mut [f64], rate: u32, hz: f64) {
    let a = (-2.0 * std::f64::consts::PI * hz / f64::from(rate)).exp();
    for _ in 0..2 {
        let (mut px, mut py) = (0.0, 0.0);
        for v in x.iter_mut() {
            let y = a * (py + *v - px);
            px = *v;
            py = y;
            *v = y;
        }
    }
}
