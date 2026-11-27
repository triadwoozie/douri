//! Revamped Session Player generator: Keyboard Player, Bass Player, and Drummer.
//!
//! Combines chord-aware voicing rules (inspired by `billyblu2000/accomontage2`, MIT) with
//! Euclidean rhythm, swing, ratchet, and LFO modulation engines (ported from
//! `joshband/GenerativeMIDI`, MIT). Generates editable MIDI notes, pitch-bend slides, and CC 74.

use crate::model::{ChordBlock, Control, Note, PITCH_BEND};
use crate::pattern::{drummer_notes, BarSpec, DrumStyle, DrummerParams};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionPlayerKind {
    #[default]
    Keyboard,
    Bass,
    Drummer,
}

impl SessionPlayerKind {
    pub const ALL: [Self; 3] = [Self::Keyboard, Self::Bass, Self::Drummer];
    pub fn label(self) -> &'static str {
        match self {
            Self::Keyboard => "Keyboard Player",
            Self::Bass => "Bass Player",
            Self::Drummer => "Drummer",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardStyle {
    SimplePad,
    ModulatedPad,
    RhythmicChords,
    #[default]
    Freely,
    Arpeggiated,
    BrokenChords,
    BlockChords,
}

impl KeyboardStyle {
    pub const SYNTH: [Self; 3] = [Self::SimplePad, Self::ModulatedPad, Self::RhythmicChords];
    pub const ACOUSTIC: [Self; 4] = [
        Self::Freely,
        Self::Arpeggiated,
        Self::BrokenChords,
        Self::BlockChords,
    ];
    pub const ALL: [Self; 7] = [
        Self::SimplePad,
        Self::ModulatedPad,
        Self::RhythmicChords,
        Self::Freely,
        Self::Arpeggiated,
        Self::BrokenChords,
        Self::BlockChords,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::SimplePad => "Simple Pad",
            Self::ModulatedPad => "Modulated Pad",
            Self::RhythmicChords => "Rhythmic Chords",
            Self::Freely => "Freely",
            Self::Arpeggiated => "Arpeggiated",
            Self::BrokenChords => "Broken Chords",
            Self::BlockChords => "Block Chords",
        }
    }
    pub fn is_pad(self) -> bool {
        matches!(self, Self::SimplePad | Self::ModulatedPad)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BassStyle {
    SequencedBass,
    PumpBass,
    #[default]
    Bass808,
    Fingerstyle,
    Picked,
    Slap,
}

impl BassStyle {
    pub const SYNTH: [Self; 3] = [Self::SequencedBass, Self::PumpBass, Self::Bass808];
    pub const ELECTRIC: [Self; 3] = [Self::Fingerstyle, Self::Picked, Self::Slap];
    pub const ALL: [Self; 6] = [
        Self::SequencedBass,
        Self::PumpBass,
        Self::Bass808,
        Self::Fingerstyle,
        Self::Picked,
        Self::Slap,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::SequencedBass => "Sequenced Bass",
            Self::PumpBass => "Pump Bass",
            Self::Bass808 => "808 Bass",
            Self::Fingerstyle => "Fingerstyle",
            Self::Picked => "Picked",
            Self::Slap => "Slap",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddNotesMode {
    #[default]
    None,
    Eighths,
    Sixteenths,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FollowRhythmMode {
    #[default]
    ChordRhythm,
    Track,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoicingMode {
    RootOnly,
    RootOctave,
    PowerChord,
    ThreeVoiceWide,
    #[default]
    FullChord,
    Stacked,
    Shape1,
    Shape2,
    Shape3,
}

impl VoicingMode {
    pub const ALL: [Self; 9] = [
        Self::RootOnly,
        Self::RootOctave,
        Self::PowerChord,
        Self::ThreeVoiceWide,
        Self::FullChord,
        Self::Stacked,
        Self::Shape1,
        Self::Shape2,
        Self::Shape3,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::RootOnly => "Root Only",
            Self::RootOctave => "Root & Octave",
            Self::PowerChord => "Power Chord",
            Self::ThreeVoiceWide => "3 Voice Wide",
            Self::FullChord => "Full Chord",
            Self::Stacked => "Stacked",
            Self::Shape1 => "Shape 1",
            Self::Shape2 => "Shape 2",
            Self::Shape3 => "Shape 3",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandStyle {
    #[default]
    Simple,
    Syncopated,
    OctaveBass,
    Broken,
}

impl HandStyle {
    pub const ALL: [Self; 4] = [
        Self::Simple,
        Self::Syncopated,
        Self::OctaveBass,
        Self::Broken,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Simple => "Simple",
            Self::Syncopated => "Syncopated",
            Self::OctaveBass => "Octave Bass",
            Self::Broken => "Broken",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandMovement {
    Still,
    #[default]
    Medium,
    Active,
    Arp,
}

impl HandMovement {
    pub const ALL: [Self; 4] = [Self::Still, Self::Medium, Self::Active, Self::Arp];
    pub fn label(self) -> &'static str {
        match self {
            Self::Still => "Still",
            Self::Medium => "Medium",
            Self::Active => "Active",
            Self::Arp => "Arp",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwingSubdiv {
    Eighth,
    #[default]
    Sixteenth,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhraseLength {
    Chord,
    FourBars,
    TwoBars,
    #[default]
    OneBar,
    Half,
    Quarter,
}

impl PhraseLength {
    pub const ALL: [Self; 6] = [
        Self::Chord,
        Self::FourBars,
        Self::TwoBars,
        Self::OneBar,
        Self::Half,
        Self::Quarter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Chord => "Chord",
            Self::FourBars => "4 Bars",
            Self::TwoBars => "2 Bars",
            Self::OneBar => "1 Bar",
            Self::Half => "1/2",
            Self::Quarter => "1/4",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlideStart {
    #[default]
    Auto,
    ToNextNote,
    OnNoteStart,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LfoWave {
    #[default]
    TriSaw,
    Sine,
    Square,
    Random,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionPlayerParams {
    pub kind: SessionPlayerKind,
    pub preset_name: String,
    pub keyboard_style: KeyboardStyle,
    pub bass_style: BassStyle,
    pub drum_style: DrumStyle,
    pub complexity: f32,
    pub intensity: f32,
    pub seed: u32,
    pub pattern_idx: usize,
    pub add_notes: AddNotesMode,
    pub follow_rhythm: FollowRhythmMode,

    // Bass Main
    pub xy_melody: f32,
    pub xy_octave: f32,
    pub note_range_lo: u8,
    pub note_range_hi: u8,
    pub note_length: u8,

    // Keyboard Main
    pub lh_voicing: VoicingMode,
    pub lh_style: HandStyle,
    pub rh_voicing: VoicingMode,
    pub rh_movement: HandMovement,
    pub tie_notes: bool,
    pub key_range_lo: u8,
    pub key_range_hi: u8,

    // Right column Main & Details
    pub fill_amount: f32,
    pub fill_complexity: f32,
    pub swing: f32,
    pub swing_subdiv: SwingSubdiv,
    pub phrase_length: PhraseLength,
    pub chord_response: f32,
    pub feel: f32,
    pub humanize: f32,

    // Details tab
    pub accent_vel: f32,
    pub accent_len: f32,
    pub accent_cc: f32,
    pub cc_target: u8,
    pub var_rhythm: f32,
    pub var_octave: f32,
    pub var_melody: f32,
    pub pump_amount: f32,
    pub pump_shape: f32,
    pub phrasing: f32,
    pub strum: f32,

    // Slides tab (Bass)
    pub slide_amount: f32,
    pub slide_time: f32,
    pub slide_auto: bool,
    pub slide_curve: f32,
    pub slide_s_curve: bool,
    pub slide_start: SlideStart,

    // Envelope tab (Pad)
    pub env_attack: f32,
    pub env_hold: f32,
    pub env_decay: f32,
    pub env_shift_start: f32,
    pub env_align_peak: bool,
    pub env_invert: bool,

    // LFO tab
    pub lfo_wave: LfoWave,
    pub lfo_phase: f32,
    pub lfo_symmetry: f32,
    pub lfo_rate: f32,
    pub lfo_sync_chord: bool,
    pub lfo_amount: f32,
    pub lfo_to_vel: f32,
    pub lfo_to_len: f32,

    // Manual override grid (16 steps = 1 bar)
    pub manual_override: bool,
    pub manual_steps: [bool; 16],

    // Drummer kit piece toggles, secondary pattern & lock states
    #[serde(default = "default_true")]
    pub kit_hihat: bool,
    #[serde(default)]
    pub kit_tambourine: bool,
    #[serde(default)]
    pub kit_cowbell: bool,
    #[serde(default = "default_true")]
    pub kit_kick: bool,
    #[serde(default = "default_true")]
    pub kit_snare: bool,
    #[serde(default)]
    pub pattern_idx_2: usize,
    #[serde(default)]
    pub lock_fill_amount: bool,
    #[serde(default)]
    pub lock_swing: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SessionPlayerParams {
    fn default() -> Self {
        Self {
            kind: SessionPlayerKind::Keyboard,
            preset_name: "Sweet Memories".to_string(),
            keyboard_style: KeyboardStyle::Freely,
            bass_style: BassStyle::Bass808,
            drum_style: DrumStyle::Electronic,
            complexity: 0.42,
            intensity: 0.65,
            seed: 1,
            pattern_idx: 0,
            add_notes: AddNotesMode::None,
            follow_rhythm: FollowRhythmMode::ChordRhythm,
            xy_melody: 0.12,
            xy_octave: 0.25,
            note_range_lo: 36, // C2
            note_range_hi: 64, // E4
            note_length: 36,
            lh_voicing: VoicingMode::RootOnly,
            lh_style: HandStyle::Simple,
            rh_voicing: VoicingMode::FullChord,
            rh_movement: HandMovement::Medium,
            tie_notes: false,
            key_range_lo: 48,
            key_range_hi: 84,
            fill_amount: 0.35,
            fill_complexity: 0.30,
            swing: 0.08,
            swing_subdiv: SwingSubdiv::Sixteenth,
            phrase_length: PhraseLength::OneBar,
            chord_response: 0.65,
            feel: 0.0,
            humanize: 0.15,
            accent_vel: 0.45,
            accent_len: 0.25,
            accent_cc: 0.40,
            cc_target: 74,
            var_rhythm: 0.30,
            var_octave: 0.20,
            var_melody: 0.25,
            pump_amount: 0.65,
            pump_shape: 0.50,
            phrasing: 0.50,
            strum: 0.10,
            slide_amount: 0.35,
            slide_time: 0.35,
            slide_auto: true,
            slide_curve: 0.25,
            slide_s_curve: true,
            slide_start: SlideStart::Auto,
            env_attack: 0.25,
            env_hold: 0.50,
            env_decay: 0.35,
            env_shift_start: 0.0,
            env_align_peak: true,
            env_invert: false,
            lfo_wave: LfoWave::Sine,
            lfo_phase: 0.0,
            lfo_symmetry: 0.50,
            lfo_rate: 0.50,
            lfo_sync_chord: false,
            lfo_amount: 0.40,
            lfo_to_vel: 0.20,
            lfo_to_len: 0.10,
            manual_override: false,
            manual_steps: [
                true, false, false, false, true, false, false, false, true, false, false, false,
                true, false, true, false,
            ],
            kit_hihat: true,
            kit_tambourine: false,
            kit_cowbell: false,
            kit_kick: true,
            kit_snare: true,
            pattern_idx_2: 0,
            lock_fill_amount: false,
            lock_swing: false,
        }
    }
}

pub const SESSION_PRESETS: [&str; 8] = [
    "Back Roads",
    "Sweet Memories",
    "Dirty South Bender",
    "Fog Machine",
    "Straight Up",
    "The Feels",
    "Perpetual Sunrise",
    "Neon Pulse",
];

impl SessionPlayerParams {
    pub fn apply_preset(&mut self, name: &str) {
        self.preset_name = name.to_string();
        match name {
            "Back Roads" => {
                self.kind = SessionPlayerKind::Drummer;
                self.drum_style = DrumStyle::Pop;
                self.complexity = 0.45;
                self.intensity = 0.65;
                self.fill_amount = 0.35;
                self.fill_complexity = 0.30;
                self.swing = 0.08;
            }
            "Sweet Memories" => {
                self.kind = SessionPlayerKind::Keyboard;
                self.keyboard_style = KeyboardStyle::Freely;
                self.complexity = 0.40;
                self.intensity = 0.62;
                self.lh_voicing = VoicingMode::RootOnly;
                self.rh_voicing = VoicingMode::FullChord;
            }
            "Dirty South Bender" => {
                self.kind = SessionPlayerKind::Bass;
                self.bass_style = BassStyle::Bass808;
                self.complexity = 0.42;
                self.intensity = 0.90;
                self.note_length = 36;
                self.slide_amount = 0.55;
            }
            "Fog Machine" => {
                self.kind = SessionPlayerKind::Keyboard;
                self.keyboard_style = KeyboardStyle::ModulatedPad;
                self.complexity = 0.35;
                self.intensity = 0.58;
                self.lfo_amount = 0.65;
            }
            "Straight Up" => {
                self.kind = SessionPlayerKind::Bass;
                self.bass_style = BassStyle::SequencedBass;
                self.complexity = 0.72;
                self.intensity = 0.75;
                self.note_length = 45;
            }
            "The Feels" => {
                self.kind = SessionPlayerKind::Bass;
                self.bass_style = BassStyle::PumpBass;
                self.complexity = 0.55;
                self.intensity = 0.80;
                self.pump_amount = 0.75;
            }
            "Perpetual Sunrise" => {
                self.kind = SessionPlayerKind::Keyboard;
                self.keyboard_style = KeyboardStyle::RhythmicChords;
                self.complexity = 0.68;
                self.intensity = 0.72;
                self.rh_movement = HandMovement::Arp;
            }
            "Neon Pulse" => {
                self.kind = SessionPlayerKind::Drummer;
                self.drum_style = DrumStyle::Electronic;
                self.complexity = 0.60;
                self.intensity = 0.78;
            }
            _ => {}
        }
    }
}

/// Default 4-bar chord progression when "Use Default Chord Progression" is enabled.
pub fn default_chords() -> Vec<ChordBlock> {
    vec![
        ChordBlock {
            name: "Cmaj7".into(),
            start_beat: 0.0,
            len_beats: 4.0,
        },
        ChordBlock {
            name: "Am7".into(),
            start_beat: 4.0,
            len_beats: 4.0,
        },
        ChordBlock {
            name: "Fmaj7".into(),
            start_beat: 8.0,
            len_beats: 4.0,
        },
        ChordBlock {
            name: "G7".into(),
            start_beat: 12.0,
            len_beats: 4.0,
        },
    ]
}

/// Evaluates `ModLfo` (ported from `GenerativeMIDI/Source/Modulation/ModLfo.h`) at `phase` in `0..1`.
pub fn eval_lfo(wave: LfoWave, phase: f32, symmetry: f32, seed: u32) -> f32 {
    let p = phase.rem_euclid(1.0);
    let sym = symmetry.clamp(0.05, 0.95);
    match wave {
        LfoWave::TriSaw => {
            if p < sym {
                p / sym
            } else {
                1.0 - (p - sym) / (1.0 - sym)
            }
        }
        LfoWave::Sine => 0.5 + 0.5 * (p * TAU).sin(),
        LfoWave::Square => {
            if p < sym {
                1.0
            } else {
                0.0
            }
        }
        LfoWave::Random => {
            let step = (p * 8.0) as u32;
            let h = seed.wrapping_mul(1664525).wrapping_add(step * 1013904223);
            ((h >> 16) & 0xff) as f32 / 255.0
        }
    }
}

/// Bjorklund Euclidean rhythm generator (ported from `GenerativeMIDI/Source/Core/EuclideanEngine.cpp`).
pub fn euclidean_steps(steps: usize, pulses: usize, rotation: usize) -> Vec<bool> {
    if steps == 0 {
        return Vec::new();
    }
    let k = pulses.min(steps);
    let mut out = vec![false; steps];
    if k == 0 {
        return out;
    }
    for i in 0..k {
        let idx = ((i * steps) / k + rotation) % steps;
        out[idx] = true;
    }
    out
}

/// Parses a chord name (`Cmaj7`, `Am`, `F#m7`, `Bb`) into `(root_pc, intervals)`.
fn parse_chord_symbol(name: &str, fallback_root: u8) -> (u8, Vec<i8>) {
    let s = name.trim();
    if s.is_empty() {
        return (fallback_root % 12, vec![0, 4, 7]);
    }
    let bytes = s.as_bytes();
    let mut pc = match bytes[0].to_ascii_uppercase() {
        b'C' => 0i8,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => (fallback_root % 12) as i8,
    };
    let mut rest_idx = 1;
    if bytes.get(1) == Some(&b'#') {
        pc = (pc + 1).rem_euclid(12);
        rest_idx = 2;
    } else if bytes.get(1) == Some(&b'b') {
        pc = (pc - 1).rem_euclid(12);
        rest_idx = 2;
    }
    let q = s[rest_idx..].to_ascii_lowercase();
    let intervals = if q.contains("min7") || q.starts_with("m7") {
        vec![0, 3, 7, 10]
    } else if q.contains("maj7") {
        vec![0, 4, 7, 11]
    } else if q.contains('7') {
        vec![0, 4, 7, 10]
    } else if q.starts_with('m') && !q.starts_with("maj") {
        vec![0, 3, 7]
    } else if q.contains("sus2") {
        vec![0, 2, 7]
    } else if q.contains("sus4") {
        vec![0, 5, 7]
    } else if q.contains("dim") {
        vec![0, 3, 6]
    } else {
        vec![0, 4, 7]
    };
    (pc as u8, intervals)
}

fn active_chord_at(chords: &[ChordBlock], beat: f64, fallback_root: u8) -> (u8, Vec<i8>) {
    if chords.is_empty() {
        return (fallback_root % 12, vec![0, 4, 7]);
    }
    for c in chords {
        if beat >= c.start_beat && beat < c.start_beat + c.len_beats {
            return parse_chord_symbol(&c.name, fallback_root);
        }
    }
    let wrapped = beat % chords.iter().map(|c| c.len_beats).sum::<f64>().max(4.0);
    for c in chords {
        if wrapped >= c.start_beat && wrapped < c.start_beat + c.len_beats {
            return parse_chord_symbol(&c.name, fallback_root);
        }
    }
    parse_chord_symbol(&chords[0].name, fallback_root)
}

fn voice_chord(root_pc: u8, intervals: &[i8], voicing: VoicingMode, base_oct: i8) -> Vec<u8> {
    let base = (base_oct as i16 * 12) + i16::from(root_pc);
    let raw: Vec<i16> = match voicing {
        VoicingMode::RootOnly => vec![base],
        VoicingMode::RootOctave => vec![base, base + 12],
        VoicingMode::PowerChord => vec![base, base + 7, base + 12],
        VoicingMode::ThreeVoiceWide => intervals
            .iter()
            .take(3)
            .enumerate()
            .map(|(i, &iv)| base + i16::from(iv) + if i == 1 { 12 } else { 0 })
            .collect(),
        VoicingMode::FullChord => intervals.iter().map(|&iv| base + i16::from(iv)).collect(),
        VoicingMode::Stacked => intervals
            .iter()
            .enumerate()
            .map(|(i, &iv)| base + i16::from(iv) + (i as i16 / 2) * 12)
            .collect(),
        VoicingMode::Shape1 => intervals
            .iter()
            .enumerate()
            .map(|(i, &iv)| base + i16::from(iv) + if i == 0 { 12 } else { 0 })
            .collect(),
        VoicingMode::Shape2 => intervals
            .iter()
            .enumerate()
            .map(|(i, &iv)| base + i16::from(iv) + if i < 2 { 12 } else { 0 })
            .collect(),
        VoicingMode::Shape3 => vec![
            base + 7,
            base + 12,
            base + i16::from(intervals.get(1).copied().unwrap_or(4)) + 12,
        ],
    };
    raw.into_iter().map(|p| p.clamp(24, 108) as u8).collect()
}

/// Generates `(notes, controls)` for a Session Player region across `bars`.
pub fn generate_session_region(
    p: &SessionPlayerParams,
    bars: &[BarSpec],
    chords: &[ChordBlock],
    key_root: u8,
) -> (Vec<Note>, Vec<Control>) {
    if p.kind == SessionPlayerKind::Drummer {
        let mut parts = crate::pattern::parts::TOMS | crate::pattern::parts::CYMBALS;
        if p.kit_kick {
            parts |= crate::pattern::parts::KICK;
        }
        if p.kit_snare {
            parts |= crate::pattern::parts::SNARE;
        }
        if p.kit_hihat {
            parts |= crate::pattern::parts::HIHAT;
        }
        if p.kit_tambourine || p.kit_cowbell {
            parts |= crate::pattern::parts::PERCUSSION;
        }
        let dp = DrummerParams {
            style: p.drum_style,
            complexity: (p.complexity * 100.0).round() as u8,
            loudness: (p.intensity * 100.0).round() as u8,
            swing: (p.swing * 100.0).round() as u8,
            fills: (p.fill_amount * 100.0).round() as u8,
            parts,
            seed: p
                .seed
                .wrapping_add((p.pattern_idx as u32).wrapping_mul(31))
                .wrapping_add((p.pattern_idx_2 as u32).wrapping_mul(97)),
        };
        return (drummer_notes(&dp, bars), Vec::new());
    }

    let mut notes = Vec::new();
    let mut controls = Vec::new();
    let mut rng = p.seed.wrapping_mul(1664525).wrapping_add(1013904223);
    let mut next_rand = || -> f32 {
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        ((rng >> 16) & 0xffff) as f32 / 65535.0
    };

    let mut bar_start = 0.0_f64;
    for (bi, bar) in bars.iter().enumerate() {
        let steps = 16usize;
        let step_len = (bar.beats / steps as f64) as f32;
        let pulses = match p.kind {
            SessionPlayerKind::Bass => {
                if p.bass_style == BassStyle::PumpBass {
                    8
                } else if p.bass_style == BassStyle::SequencedBass {
                    (4.0 + p.complexity * 10.0).round() as usize
                } else {
                    (3.0 + p.complexity * 7.0).round() as usize
                }
            }
            SessionPlayerKind::Keyboard => {
                if p.keyboard_style.is_pad() {
                    1
                } else {
                    (2.0 + p.complexity * 8.0).round() as usize
                }
            }
            SessionPlayerKind::Drummer => 4,
        };
        let pattern = if p.manual_override {
            p.manual_steps.to_vec()
        } else {
            euclidean_steps(steps, pulses, p.pattern_idx)
        };

        let is_fill_bar = bi % 4 == 3 && p.fill_amount > 0.25;
        for (si, &hit) in pattern.iter().enumerate() {
            let extra_fill = is_fill_bar && si >= 12 && next_rand() < p.fill_complexity;
            if !hit && !extra_fill {
                continue;
            }

            let mut beat_pos = bar_start as f32 + si as f32 * step_len;
            // Swing on odd 16th/8th steps
            if si % 2 == 1 {
                beat_pos += step_len * 0.45 * p.swing;
            }
            // Feel (Pull to Push) + Humanize
            beat_pos = (beat_pos + p.feel * 0.04 + (next_rand() - 0.5) * 0.025 * p.humanize)
                .max(bar_start as f32);

            let lfo_val = eval_lfo(
                p.lfo_wave,
                (beat_pos * (0.25 + p.lfo_rate * 1.5)) + p.lfo_phase,
                p.lfo_symmetry,
                p.seed,
            );
            let vel_base = 52.0 + p.intensity * 62.0 + (lfo_val - 0.5) * 28.0 * p.lfo_to_vel;
            let vel = (vel_base
                + if si % 4 == 0 {
                    p.accent_vel * 18.0
                } else {
                    0.0
                })
            .clamp(20.0, 127.0) as u8;

            let (root_pc, intervals) = active_chord_at(chords, f64::from(beat_pos), key_root);

            match p.kind {
                SessionPlayerKind::Bass => {
                    let oct_shift = if next_rand() < p.xy_octave * 0.6 {
                        12
                    } else {
                        0
                    };
                    let iv = if next_rand() < p.xy_melody {
                        *intervals.get((si / 2) % intervals.len()).unwrap_or(&0)
                    } else {
                        0
                    };
                    let raw_pitch =
                        i16::from(p.note_range_lo) + i16::from(root_pc) + i16::from(iv) + oct_shift;
                    let pitch = raw_pitch
                        .clamp(i16::from(p.note_range_lo), i16::from(p.note_range_hi))
                        as u8;
                    let len = (step_len * (0.4 + f32::from(p.note_length) / 50.0))
                        .min(bar.beats as f32 - (beat_pos - bar_start as f32))
                        .max(0.08);
                    notes.push(Note {
                        start: beat_pos,
                        len,
                        pitch,
                        vel,
                        mute: false,
                        lyric: Default::default(),
                    });

                    // 808 / Bass Pitch-Slide curve
                    if p.slide_amount > 0.15 && (extra_fill || next_rand() < p.slide_amount * 0.5) {
                        let bend_steps = 4;
                        for k in 0..=bend_steps {
                            let frac = k as f32 / bend_steps as f32;
                            let shaped = if p.slide_s_curve {
                                frac * frac * (3.0 - 2.0 * frac)
                            } else {
                                frac
                            };
                            controls.push(Control {
                                start: beat_pos + len * 0.5 + frac * len * 0.45,
                                num: PITCH_BEND,
                                val: ((0.5 + shaped * 0.35 * p.slide_amount).clamp(0.0, 1.0)
                                    * 16383.0)
                                    .round() as u16,
                            });
                        }
                        controls.push(Control {
                            start: beat_pos + len,
                            num: PITCH_BEND,
                            val: 8192,
                        });
                    }
                }
                SessionPlayerKind::Keyboard => {
                    if p.keyboard_style.is_pad() {
                        let pad_notes = voice_chord(root_pc, &intervals, p.rh_voicing, 4);
                        for (ni, &pitch) in pad_notes.iter().enumerate() {
                            notes.push(Note {
                                start: beat_pos + ni as f32 * 0.015 * p.strum,
                                len: (bar.beats as f32 * 0.96).max(0.25),
                                pitch: pitch.clamp(p.key_range_lo, p.key_range_hi),
                                vel,
                                mute: false,
                                lyric: Default::default(),
                            });
                        }
                    } else {
                        let lh = voice_chord(root_pc, &intervals, p.lh_voicing, 3);
                        let rh = voice_chord(root_pc, &intervals, p.rh_voicing, 4);
                        if si % 8 == 0 {
                            for &pitch in &lh {
                                notes.push(Note {
                                    start: beat_pos,
                                    len: (step_len * 4.0).max(0.2),
                                    pitch,
                                    vel: (f32::from(vel) * 0.88) as u8,
                                    mute: false,
                                    lyric: Default::default(),
                                });
                            }
                        }
                        if p.rh_movement == HandMovement::Arp
                            || p.keyboard_style == KeyboardStyle::Arpeggiated
                        {
                            let pitch = rh[si % rh.len().max(1)];
                            notes.push(Note {
                                start: beat_pos,
                                len: step_len * 0.85,
                                pitch: pitch.clamp(p.key_range_lo, p.key_range_hi),
                                vel,
                                mute: false,
                                lyric: Default::default(),
                            });
                        } else {
                            for (ni, &pitch) in rh.iter().enumerate() {
                                notes.push(Note {
                                    start: beat_pos + ni as f32 * 0.02 * p.strum,
                                    len: (step_len * 1.8).max(0.15),
                                    pitch: pitch.clamp(p.key_range_lo, p.key_range_hi),
                                    vel,
                                    mute: false,
                                    lyric: Default::default(),
                                });
                            }
                        }
                    }
                }
                SessionPlayerKind::Drummer => {}
            }

            // LFO -> CC target (e.g. CC 74 filter brightness)
            if p.lfo_amount > 0.05 {
                controls.push(Control {
                    start: beat_pos,
                    num: p.cc_target,
                    val: ((0.5 + (lfo_val - 0.5) * p.lfo_amount).clamp(0.0, 1.0) * 127.0).round()
                        as u16,
                });
            }
        }
        bar_start += bar.beats;
    }

    (notes, controls)
}

/// Chord ID: analyzes MIDI notes across 4-beat bars to populate `Project::chords`.
pub fn detect_chords_from_notes(notes: &[Note], total_bars: usize) -> Vec<ChordBlock> {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
    ];
    let mut out = Vec::new();
    for bar in 0..total_bars.max(1) {
        let s = (bar * 4) as f32;
        let e = s + 4.0;
        let mut pcs = [0u32; 12];
        let mut lowest: Option<u8> = None;
        for n in notes {
            if n.start >= s && n.start < e {
                pcs[(n.pitch % 12) as usize] += 1;
                lowest = Some(lowest.map_or(n.pitch, |l| l.min(n.pitch)));
            }
        }
        if let Some(low) = lowest {
            let root = (low % 12) as usize;
            let minor = pcs[(root + 3) % 12] > pcs[(root + 4) % 12];
            let dom7 = pcs[(root + 10) % 12] > 0;
            let maj7 = pcs[(root + 11) % 12] > 0;
            let suffix = if minor {
                if dom7 {
                    "m7"
                } else {
                    "m"
                }
            } else if maj7 {
                "maj7"
            } else if dom7 {
                "7"
            } else {
                ""
            };
            out.push(ChordBlock {
                name: format!("{}{}", NAMES[root], suffix),
                start_beat: f64::from(s),
                len_beats: 4.0,
            });
        }
    }
    if out.is_empty() {
        default_chords()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclidean_steps_distributes_pulses_evenly() {
        let s = euclidean_steps(16, 4, 0);
        assert_eq!(s.iter().filter(|&&b| b).count(), 4);
        assert!(s[0] && s[4] && s[8] && s[12]);
    }

    #[test]
    fn session_generator_produces_keyboard_and_808_slides() {
        let bars = vec![
            BarSpec {
                beats: 4.0,
                num: 4,
                den: 4,
            };
            4
        ];
        let chords = default_chords();

        let mut kb = SessionPlayerParams::default();
        kb.apply_preset("Sweet Memories");
        let (kb_notes, _) = generate_session_region(&kb, &bars, &chords, 0);
        assert!(!kb_notes.is_empty());

        let mut bass = SessionPlayerParams::default();
        bass.apply_preset("Dirty South Bender");
        let (bass_notes, bass_ctrls) = generate_session_region(&bass, &bars, &chords, 0);
        assert!(!bass_notes.is_empty());
        assert!(bass_ctrls.iter().any(|c| c.num == PITCH_BEND));
    }

    #[test]
    fn chord_id_detects_cmaj_and_am() {
        let notes = vec![
            Note {
                start: 0.0,
                len: 1.0,
                pitch: 48,
                vel: 100,
                mute: false,
                lyric: Default::default(),
            },
            Note {
                start: 0.0,
                len: 1.0,
                pitch: 52,
                vel: 100,
                mute: false,
                lyric: Default::default(),
            },
            Note {
                start: 0.0,
                len: 1.0,
                pitch: 55,
                vel: 100,
                mute: false,
                lyric: Default::default(),
            },
            Note {
                start: 4.0,
                len: 1.0,
                pitch: 45,
                vel: 100,
                mute: false,
                lyric: Default::default(),
            },
            Note {
                start: 4.0,
                len: 1.0,
                pitch: 48,
                vel: 100,
                mute: false,
                lyric: Default::default(),
            },
            Note {
                start: 4.0,
                len: 1.0,
                pitch: 52,
                vel: 100,
                mute: false,
                lyric: Default::default(),
            },
        ];
        let chords = detect_chords_from_notes(&notes, 2);
        assert_eq!(chords[0].name, "C");
        assert_eq!(chords[1].name, "Am");
    }
}
