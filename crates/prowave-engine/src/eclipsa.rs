//! Zero-allocation Eclipsa Audio (IAMF 7.1.2 / 7.1.4) spatial panner & binaural renderer.
//!
//! Inspired by `google/eclipsa-studio` (Apache-2.0) and the AOMedia IAMF specification.
//! Strictly `#![forbid(alloc)]`-compatible: all ITD delay lines and filter states are fixed-size.

use std::f32::consts::FRAC_PI_2;

/// Project-level spatial / surround format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpatialFormat {
    #[default]
    Stereo,
    Surround51,
    Surround712,
    EclipsaAudio,
}

impl SpatialFormat {
    pub const ALL: [Self; 4] = [
        Self::Stereo,
        Self::Surround51,
        Self::Surround712,
        Self::EclipsaAudio,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stereo => "Stereo",
            Self::Surround51 => "5.1 Surround",
            Self::Surround712 => "7.1.2 Surround",
            Self::EclipsaAudio => "Eclipsa Audio (7.1.2 IAMF)",
        }
    }

    pub fn is_spatial(self) -> bool {
        !matches!(self, Self::Stereo)
    }
}

/// Master renderer monitoring layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonitoringFormat {
    #[default]
    Binaural,
    Stereo,
    Surround51,
    Surround714,
}

impl MonitoringFormat {
    pub const ALL: [Self; 4] = [
        Self::Binaural,
        Self::Stereo,
        Self::Surround51,
        Self::Surround714,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Binaural => "Binaural",
            Self::Stereo => "Stereo",
            Self::Surround51 => "5.1",
            Self::Surround714 => "7.1.4",
        }
    }
}

/// Per-bed / per-object binaural distance rendering mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BinauralMode {
    Off,
    Near,
    #[default]
    Mid,
    Far,
}

impl BinauralMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::Near, Self::Mid, Self::Far];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Near => "Near",
            Self::Mid => "Mid",
            Self::Far => "Far",
        }
    }

    pub fn gain(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Near => 1.0,
            Self::Mid => 0.88,
            Self::Far => 0.72,
        }
    }
}

/// Surround panner projection mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PannerMode {
    #[default]
    Planar,
    Spherical,
}

/// Surround bed speaker labels matching the 7.1.2 / 7.1.4 bed.
pub const BED_SPEAKER_NAMES: [&str; 10] = [
    "Left",
    "Right",
    "Center",
    "LFE",
    "Left Mid",
    "Right Mid",
    "Left Surround",
    "Right Surround",
    "Left Top Mid",
    "Right Top Mid",
];

/// 3D Object Panner parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EclipsaObjectParams {
    /// -1.0 (Left) .. +1.0 (Right)
    pub left_right: f32,
    /// -1.0 (Back) .. +1.0 (Front)
    pub back_front: f32,
    /// 0.0 (Ear Level) .. +1.0 (Top)
    pub elevation: f32,
    /// 0.0 .. 1.0 object size
    pub size: f32,
    /// 0.0 .. 180.0 degrees stereo spread
    pub spread_deg: f32,
    pub binaural: BinauralMode,
}

impl Default for EclipsaObjectParams {
    fn default() -> Self {
        Self {
            left_right: 0.0,
            back_front: 0.5,
            elevation: 0.0,
            size: 0.0,
            spread_deg: 90.0,
            binaural: BinauralMode::Mid,
        }
    }
}

/// Surround Panner parameters (5.1 / 7.1.2 bed panner).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurroundPannerParams {
    /// -180.0 .. +180.0 degrees
    pub angle_deg: f32,
    /// 0.0 .. 1.0 diversity / front-back focus
    pub diversity: f32,
    /// 0.0 .. 90.0 degrees elevation
    pub elevation_deg: f32,
    /// 0.0 .. 180.0 degrees stereo spread
    pub spread_deg: f32,
    pub mode: PannerMode,
    /// -60.0 .. 0.0 dB
    pub center_db: f32,
    /// -60.0 .. 0.0 dB
    pub lfe_db: f32,
}

impl Default for SurroundPannerParams {
    fn default() -> Self {
        Self {
            angle_deg: 0.0,
            diversity: 0.24,
            elevation_deg: 0.0,
            spread_deg: 90.0,
            mode: PannerMode::Planar,
            center_db: 0.0,
            lfe_db: -9.8,
        }
    }
}

const DELAY_CAP: usize = 64;

/// Fixed-size zero-allocation binaural + 7.1.4 bed spatializer for one object/track.
#[derive(Clone, Debug)]
pub struct EclipsaObjectDsp {
    delay_l: [f32; DELAY_CAP],
    delay_r: [f32; DELAY_CAP],
    write_idx: usize,
    lp_l: f32,
    lp_r: f32,
    sample_rate: f32,
}

impl Default for EclipsaObjectDsp {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl EclipsaObjectDsp {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            delay_l: [0.0; DELAY_CAP],
            delay_r: [0.0; DELAY_CAP],
            write_idx: 0,
            lp_l: 0.0,
            lp_r: 0.0,
            sample_rate: sample_rate.max(8_000.0),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(8_000.0);
    }

    /// Processes one stereo sample pair into `(binaural_or_stereo_out, bed_12ch)`.
    /// `bed_12ch` order: `[L, R, C, LFE, Ls, Rs, Lrs, Rrs, Ltf, Rtf, Ltr, Rtr]`.
    pub fn process_sample(
        &mut self,
        in_l: f32,
        in_r: f32,
        p: &EclipsaObjectParams,
        monitoring: MonitoringFormat,
    ) -> ([f32; 2], [f32; 12]) {
        let lr = p.left_right.clamp(-1.0, 1.0);
        let bf = p.back_front.clamp(-1.0, 1.0);
        let el = p.elevation.clamp(0.0, 1.0);
        let spread = (p.spread_deg / 180.0).clamp(0.0, 1.0);

        // Push input to ITD ring buffers
        let idx = self.write_idx;
        self.delay_l[idx] = in_l;
        self.delay_r[idx] = in_r;
        self.write_idx = (idx + 1) % DELAY_CAP;

        // Constant-power L/R pan + Front/Back + Elevation bed distribution
        let pan_angle = (lr * 0.5 + 0.5) * FRAC_PI_2;
        let g_l = pan_angle.cos();
        let g_r = pan_angle.sin();
        let front_w = (bf * 0.5 + 0.5).clamp(0.0, 1.0);
        let back_w = 1.0 - front_w;
        let floor_w = (1.0 - el * 0.85).clamp(0.15, 1.0);
        let height_w = el;

        let mono = 0.5 * (in_l + in_r);
        let l_sig = in_l * (0.5 + 0.5 * spread) + mono * (0.5 - 0.5 * spread);
        let r_sig = in_r * (0.5 + 0.5 * spread) + mono * (0.5 - 0.5 * spread);

        let mut bed = [0.0_f32; 12];
        bed[0] = l_sig * g_l * front_w * floor_w; // L
        bed[1] = r_sig * g_r * front_w * floor_w; // R
        bed[2] = mono * (1.0 - lr.abs()) * front_w * floor_w * 0.707; // C
        bed[3] = mono * 0.15; // LFE subtle trim
        bed[4] = l_sig * g_l * (0.5 * front_w + 0.5 * back_w) * floor_w; // Ls
        bed[5] = r_sig * g_r * (0.5 * front_w + 0.5 * back_w) * floor_w; // Rs
        bed[6] = l_sig * g_l * back_w * floor_w; // Lrs
        bed[7] = r_sig * g_r * back_w * floor_w; // Rrs
        bed[8] = l_sig * g_l * front_w * height_w; // Ltf
        bed[9] = r_sig * g_r * front_w * height_w; // Rtf
        bed[10] = l_sig * g_l * back_w * height_w; // Ltr
        bed[11] = r_sig * g_r * back_w * height_w; // Rtr

        // Monitoring downmix (Binaural ITD+ILD+elevation pinna filter vs Stereo)
        let (out_l, out_r) = match monitoring {
            MonitoringFormat::Binaural => {
                // ITD up to ~0.60 ms (29 samples @ 48kHz)
                let max_itd = (0.00060 * self.sample_rate).min((DELAY_CAP - 2) as f32);
                let itd_l = ((-lr).max(0.0) * max_itd).round() as usize;
                let itd_r = (lr.max(0.0) * max_itd).round() as usize;
                let dl = self.delay_l[(idx + DELAY_CAP - itd_l) % DELAY_CAP];
                let dr = self.delay_r[(idx + DELAY_CAP - itd_r) % DELAY_CAP];

                // Back/Far air-absorption & elevation pinna softening
                let cutoff = (0.85 - back_w * 0.28 - el * 0.15).clamp(0.25, 0.92);
                self.lp_l += cutoff * (dl - self.lp_l);
                self.lp_r += cutoff * (dr - self.lp_r);

                let dist_gain = p.binaural.gain();
                let raw_l = self.lp_l * g_l * dist_gain + self.lp_r * (p.size * 0.18);
                let raw_r = self.lp_r * g_r * dist_gain + self.lp_l * (p.size * 0.18);
                (soft_limit_n1db(raw_l), soft_limit_n1db(raw_r))
            }
            MonitoringFormat::Stereo
            | MonitoringFormat::Surround51
            | MonitoringFormat::Surround714 => {
                let raw_l = l_sig * g_l;
                let raw_r = r_sig * g_r;
                (soft_limit_n1db(raw_l), soft_limit_n1db(raw_r))
            }
        };

        ([out_l, out_r], bed)
    }
}

/// Computes normalized 8-channel surround ring gains `[L, R, C, LFE, Ls, Rs, Lrs, Rrs]`.
pub fn compute_surround_gains(p: &SurroundPannerParams) -> [f32; 8] {
    let rad = p.angle_deg.to_radians();
    let x = rad.sin(); // -1 left .. +1 right
    let y = rad.cos(); // +1 front .. -1 back
    let pan = (x * 0.5 + 0.5).clamp(0.0, 1.0) * FRAC_PI_2;
    let g_l = pan.cos();
    let g_r = pan.sin();
    let front = (y * 0.5 + 0.5).clamp(0.0, 1.0);
    let back = 1.0 - front;
    let div = p.diversity.clamp(0.0, 1.0);
    let c_lin = db_to_lin(p.center_db);
    let lfe_lin = db_to_lin(p.lfe_db);
    let el_atten = if p.mode == PannerMode::Spherical {
        (1.0 - (p.elevation_deg.clamp(0.0, 90.0) / 90.0) * 0.25).clamp(0.5, 1.0)
    } else {
        1.0
    };

    [
        g_l * front * el_atten,
        g_r * front * el_atten,
        (1.0 - x.abs()) * front * c_lin * (1.0 - div * 0.35),
        lfe_lin * 0.5,
        g_l * (0.5 * front + 0.5 * back) * el_atten,
        g_r * (0.5 * front + 0.5 * back) * el_atten,
        g_l * back * el_atten,
        g_r * back * el_atten,
    ]
}

fn db_to_lin(db: f32) -> f32 {
    if db <= -59.5 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0)
    }
}

/// Always-on -1 dBFS (`0.89125`) smooth peak limiter matching Eclipsa OAR output protection.
#[inline]
fn soft_limit_n1db(x: f32) -> f32 {
    const CEIL: f32 = 0.891_250_9;
    if x.abs() <= CEIL {
        x
    } else {
        CEIL * (x / CEIL).tanh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_right_object_routes_more_to_right_than_left() {
        let mut dsp = EclipsaObjectDsp::new(48_000.0);
        let p = EclipsaObjectParams {
            left_right: 1.0,
            back_front: 1.0,
            elevation: 0.0,
            ..Default::default()
        };
        let mut out = ([0.0; 2], [0.0; 12]);
        for _ in 0..32 {
            out = dsp.process_sample(0.5, 0.5, &p, MonitoringFormat::Binaural);
        }
        assert!(out.0[1] > out.0[0] * 3.0);
        assert!(out.1[1] > out.1[0] * 3.0);
    }

    #[test]
    fn elevation_routes_energy_to_top_bed_channels() {
        let mut dsp = EclipsaObjectDsp::new(48_000.0);
        let p = EclipsaObjectParams {
            left_right: 0.0,
            back_front: 1.0,
            elevation: 1.0,
            ..Default::default()
        };
        let (_, bed) = dsp.process_sample(0.8, 0.8, &p, MonitoringFormat::Binaural);
        // Ltf (8) and Rtf (9) should exceed ear-level L (0) and R (1)
        assert!(bed[8] > bed[0] && bed[9] > bed[1]);
    }

    #[test]
    fn limiter_caps_output_at_minus_1_dbfs() {
        let mut dsp = EclipsaObjectDsp::new(48_000.0);
        let p = EclipsaObjectParams::default();
        for _ in 0..16 {
            let (out, _) = dsp.process_sample(5.0, 5.0, &p, MonitoringFormat::Binaural);
            assert!(out[0].abs() <= 0.90 && out[1].abs() <= 0.90);
        }
    }

    #[test]
    fn surround_gains_respect_lfe_and_center_levels() {
        let p = SurroundPannerParams {
            lfe_db: -60.0,
            ..Default::default()
        };
        let g = compute_surround_gains(&p);
        assert_eq!(g[3], 0.0);
        assert!(g[2] > 0.5);
    }
}
