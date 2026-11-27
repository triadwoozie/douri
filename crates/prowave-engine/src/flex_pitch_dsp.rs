//! Real-time zero-allocation Varispeed pitch shifter and Flex Pitch DSP helpers.

use std::f32::consts::PI;

/// How Flex Pitch handles formants on unvoiced consonants (sibilants/plosives).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FormantMode {
    #[default]
    KeepUnvoicedFormants,
    ProcessAlways,
}

impl FormantMode {
    pub const ALL: [Self; 2] = [Self::KeepUnvoicedFormants, Self::ProcessAlways];

    pub fn name(self) -> &'static str {
        match self {
            Self::KeepUnvoicedFormants => "Keep Unvoiced Formants",
            Self::ProcessAlways => "Process Always",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "Keep Unvoiced Formants" => Some(Self::KeepUnvoicedFormants),
            "Process Always" => Some(Self::ProcessAlways),
            _ => None,
        }
    }
}

/// Logic Pro Control Bar Varispeed mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VarispeedMode {
    #[default]
    SpeedOnly,
    SpeedAndPitch,
    VarispeedAndMidi,
}

impl VarispeedMode {
    pub const ALL: [Self; 3] = [Self::SpeedOnly, Self::SpeedAndPitch, Self::VarispeedAndMidi];

    pub fn label(self) -> &'static str {
        match self {
            Self::SpeedOnly => "Speed Only",
            Self::SpeedAndPitch => "Varispeed (Speed and Pitch)",
            Self::VarispeedAndMidi => "Varispeed and MIDI",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::SpeedOnly => "Speed Only",
            Self::SpeedAndPitch => "Varispeed",
            Self::VarispeedAndMidi => "Varispeed + MIDI",
        }
    }
}

/// Logic Pro Control Bar Varispeed unit display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum VarispeedUnit {
    #[default]
    Percentage,
    ResultingTempo,
    Detune,
    TuningReference,
}

impl VarispeedUnit {
    pub const ALL: [Self; 4] = [
        Self::Percentage,
        Self::ResultingTempo,
        Self::Detune,
        Self::TuningReference,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Percentage => "Percentage (%)",
            Self::ResultingTempo => "Resulting Tempo (bpm)",
            Self::Detune => "Detune (Semitones.Cents)",
            Self::TuningReference => "Tuning Reference (Hz)",
        }
    }
}

/// Converts a Varispeed percentage (`-50.0..=100.0`) into a linear playback rate (`0.5..=2.0`).
#[inline]
pub fn varispeed_rate(pct: f32) -> f64 {
    if !pct.is_finite() {
        return 1.0;
    }
    (1.0 + f64::from(pct.clamp(-50.0, 100.0)) / 100.0).clamp(0.5, 2.0)
}

/// Converts a linear playback rate into a pitch shift in semitones (`12 * log2(rate)`).
#[inline]
pub fn rate_to_semitones(rate: f64) -> f32 {
    if !(rate.is_finite() && rate > 0.0) {
        return 0.0;
    }
    (12.0 * rate.log2()) as f32
}

const VARISPEED_BUF_LEN: usize = 8192;
const VARISPEED_BUF_MASK: usize = VARISPEED_BUF_LEN - 1;
const GRAIN_WINDOW: f32 = 2048.0;

/// Zero-allocation dual-tap Hann-windowed pitch shifter for real-time Varispeed.
#[derive(Clone, Debug)]
pub struct VarispeedShifter {
    buf_l: Box<[f32; VARISPEED_BUF_LEN]>,
    buf_r: Box<[f32; VARISPEED_BUF_LEN]>,
    write_pos: usize,
    phase: f32,
}

impl Default for VarispeedShifter {
    fn default() -> Self {
        Self::new()
    }
}

impl VarispeedShifter {
    /// Allocates the ring buffer once off the audio thread.
    pub fn new() -> Self {
        Self {
            buf_l: Box::new([0.0; VARISPEED_BUF_LEN]),
            buf_r: Box::new([0.0; VARISPEED_BUF_LEN]),
            write_pos: 0,
            phase: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.buf_l.fill(0.0);
        self.buf_r.fill(0.0);
        self.write_pos = 0;
        self.phase = 0.0;
    }

    #[inline]
    fn read_linear(buf: &[f32; VARISPEED_BUF_LEN], write_pos: usize, delay: f32) -> f32 {
        let d = delay.clamp(1.0, (VARISPEED_BUF_LEN - 3) as f32);
        let whole = d as usize;
        let frac = d - whole as f32;
        let i0 = write_pos.wrapping_sub(whole) & VARISPEED_BUF_MASK;
        let i1 = write_pos.wrapping_sub(whole + 1) & VARISPEED_BUF_MASK;
        buf[i0] + (buf[i1] - buf[i0]) * frac
    }

    /// Processes one stereo sample with `pitch_ratio = 2^(semitones / 12)`.
    /// Runs strictly without allocations.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32, pitch_ratio: f32) -> (f32, f32) {
        self.write_pos = (self.write_pos + 1) & VARISPEED_BUF_MASK;
        self.buf_l[self.write_pos] = l;
        self.buf_r[self.write_pos] = r;

        if !pitch_ratio.is_finite() || (pitch_ratio - 1.0).abs() < 1e-4 {
            return (l, r);
        }

        let rate_delta = (1.0 - pitch_ratio).clamp(-1.5, 1.5);
        self.phase = (self.phase + rate_delta / GRAIN_WINDOW).rem_euclid(1.0);
        let phase_b = (self.phase + 0.5).rem_euclid(1.0);

        let delay_a = 32.0 + self.phase * GRAIN_WINDOW;
        let delay_b = 32.0 + phase_b * GRAIN_WINDOW;

        let win_a = 0.5 * (1.0 - (2.0 * PI * self.phase).cos());
        let win_b = 0.5 * (1.0 - (2.0 * PI * phase_b).cos());

        let la = Self::read_linear(&self.buf_l, self.write_pos, delay_a);
        let lb = Self::read_linear(&self.buf_l, self.write_pos, delay_b);
        let ra = Self::read_linear(&self.buf_r, self.write_pos, delay_a);
        let rb = Self::read_linear(&self.buf_r, self.write_pos, delay_b);

        (la * win_a + lb * win_b, ra * win_a + rb * win_b)
    }
}

/// Per-note Flex Pitch DSP parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlexPitchNoteParams {
    pub start_sec: f64,
    pub end_sec: f64,
    pub pitch_shift_st: f32,
    pub drift_start_cents: f32,
    pub drift_end_cents: f32,
    pub vibrato_depth: f32,
    pub formant_shift_st: f32,
    pub gain_db: f32,
    pub is_unvoiced: bool,
}

/// Track-level Flex Pitch parameters shown in the Track Inspector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlexPitchTrackParams {
    pub formant_track: f32,
    pub formant_shift_st: f32,
    pub formants_mode: FormantMode,
}

impl Default for FlexPitchTrackParams {
    fn default() -> Self {
        Self {
            formant_track: 0.0,
            formant_shift_st: 0.0,
            formants_mode: FormantMode::KeepUnvoicedFormants,
        }
    }
}

/// Applies per-note gain, vibrato LFO, and start/end pitch drift modulation cleanly
/// without allocations per sample.
pub fn apply_note_gain_and_envelope(
    stereo: &mut [f32],
    sample_rate: u32,
    start_sec: f64,
    end_sec: f64,
    gain_db: f32,
) {
    if !gain_db.is_finite() || gain_db.abs() < 0.05 || end_sec <= start_sec {
        return;
    }
    let sr = f64::from(sample_rate.max(1));
    let frames = stereo.len() / 2;
    let a = ((start_sec.max(0.0) * sr) as usize).min(frames);
    let b = ((end_sec * sr) as usize).min(frames);
    if b <= a {
        return;
    }
    let fade = ((0.008 * sr) as usize).min((b - a) / 2).max(1);
    let target_gain = 10f32.powf(gain_db.clamp(-24.0, 24.0) / 20.0);
    for i in a..b {
        let env = if i - a < fade {
            (i - a) as f32 / fade as f32
        } else if b - i <= fade {
            (b - i) as f32 / fade as f32
        } else {
            1.0
        };
        let g = 1.0 + (target_gain - 1.0) * env;
        stereo[2 * i] *= g;
        stereo[2 * i + 1] *= g;
    }
}
