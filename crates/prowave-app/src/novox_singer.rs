//! Novox The Robot, an in-process singer of the Singer and Vocal Top Line (the engine is
//! `crates/novox`: the C++ core of github.com/songadaymann/novox-the-robot, MIT, by Jonathan
//! Mann, built with the `cc` crate behind a small C interface).
//!
//! Novox is a formant synthesizer in the manner of Myriad's Virtual Singer: deliberately
//! synthetic. A phrase's notes and words become a score; the engine's planner reads the words
//! through the CMU Pronouncing Dictionary (and spelling rules for words it lacks), lays out each
//! syllable's consonants before its beat and its vowel on it, and the synthesizer sings the plan
//! in blocks at the Vocal Top Line rate (no resampling). Novox sings English only.
//!
//! The engine renders a prepared phrase without allocating (`Stream::process`), so the same
//! stream could be run on an audio thread; here, as with CHIMERA, phrases are sung ahead of time
//! on the render thread and played by the engine's `SingerPlayer`.
//!
//! Its controls are the engine's own: a voice preset (std, robot, female, kid, choir and the
//! rest), and over it the vocal tract size (formant), breathiness, brightness, grit, the number
//! of voices, vibrato depth and rate, the scoop into each note and the portamento between notes.
use crate::vocal::ScoreNote;
use std::ffi::{c_char, CStr, CString};
use std::sync::OnceLock;

/// Bumped when the vendored engine or how phrases are given to it changes the sound (cached
/// phrases are sung again).
pub const REV: u64 = 1;
/// The voice a new Novox singer starts with.
pub const DEFAULT_PRESET: &str = "robot";
/// Frames rendered per call (any size works; the engine's output does not depend on it).
const BLOCK: usize = 512;

/// How Novox sings, in the units shown.
#[derive(Clone, PartialEq, Debug)]
pub struct Voice {
    /// The engine's voice preset the knobs below start from.
    pub preset: String,
    /// The vocal tract's size, in % of an adult man's (a woman's is about 117, a child's 130).
    pub formant: f32,
    /// 0..100 each.
    pub breathiness: f32,
    pub grit: f32,
    /// -100 (dark) to 100 (bright).
    pub brightness: f32,
    /// Voices singing together, 1..8.
    pub voices: f32,
    /// Vibrato: depth in cents (to its peak) and rate in Hz.
    pub vibrato_depth: f32,
    pub vibrato_rate: f32,
    /// Where each note's attack starts from, in cents (below the note when negative).
    pub scoop: f32,
    /// The glide from note to note when they are joined, in milliseconds.
    pub portamento: f32,
}

/// Each knob's range: (lowest, highest).
pub const FORMANT: (f32, f32) = (70.0, 150.0);
pub const BREATHINESS: (f32, f32) = (0.0, 100.0);
pub const GRIT: (f32, f32) = (0.0, 100.0);
pub const BRIGHTNESS: (f32, f32) = (-100.0, 100.0);
pub const VOICES: (f32, f32) = (1.0, 8.0);
pub const VIBRATO_DEPTH: (f32, f32) = (0.0, 200.0);
pub const VIBRATO_RATE: (f32, f32) = (3.0, 9.0);
pub const SCOOP: (f32, f32) = (-200.0, 200.0);
pub const PORTAMENTO: (f32, f32) = (0.0, 500.0);

impl Voice {
    /// The engine's preset `name` as knobs (`None`: not a preset).
    pub fn preset(name: &str) -> Option<Voice> {
        let c = CString::new(name).ok()?;
        let mut k = novox::pw_novox_knobs::default();
        // SAFETY: `c` is a NUL-terminated string and `k` a valid knobs struct for the call.
        let ok = unsafe { novox::pw_novox_preset(c.as_ptr(), &mut k) } == 0;
        ok.then(|| Voice {
            preset: name.to_string(),
            formant: k.formant_scale * 100.0,
            breathiness: k.breathiness * 100.0,
            grit: k.grit * 100.0,
            brightness: k.brightness * 100.0,
            voices: k.choir_size as f32,
            vibrato_depth: k.vibrato_depth * 100.0,
            vibrato_rate: k.vibrato_rate,
            scoop: k.scoop * 100.0,
            portamento: k.portamento * 1000.0,
        })
    }

    /// The default voice ([`DEFAULT_PRESET`] as it is).
    pub fn new() -> Voice {
        Voice::preset(DEFAULT_PRESET).unwrap_or_else(|| Voice {
            preset: DEFAULT_PRESET.into(),
            formant: 100.0,
            breathiness: 0.0,
            grit: 0.0,
            brightness: 35.0,
            voices: 1.0,
            vibrato_depth: 0.0,
            vibrato_rate: 5.8,
            scoop: 0.0,
            portamento: 20.0,
        })
    }

    /// Every value held in its range, an unknown preset replaced by the default (for settings
    /// read from a file).
    pub fn clamped(mut self) -> Self {
        let fresh = Voice::new();
        if !presets().contains(&self.preset.as_str()) {
            self.preset = fresh.preset.clone();
        }
        let c =
            |v: f32, (lo, hi): (f32, f32), d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.formant = c(self.formant, FORMANT, fresh.formant);
        self.breathiness = c(self.breathiness, BREATHINESS, fresh.breathiness);
        self.grit = c(self.grit, GRIT, fresh.grit);
        self.brightness = c(self.brightness, BRIGHTNESS, fresh.brightness);
        self.voices = c(self.voices, VOICES, fresh.voices).round();
        self.vibrato_depth = c(self.vibrato_depth, VIBRATO_DEPTH, fresh.vibrato_depth);
        self.vibrato_rate = c(self.vibrato_rate, VIBRATO_RATE, fresh.vibrato_rate);
        self.scoop = c(self.scoop, SCOOP, fresh.scoop);
        self.portamento = c(self.portamento, PORTAMENTO, fresh.portamento);
        self
    }

    /// The knobs in the engine's units.
    fn knobs(&self) -> novox::pw_novox_knobs {
        novox::pw_novox_knobs {
            formant_scale: self.formant / 100.0,
            breathiness: self.breathiness / 100.0,
            brightness: self.brightness / 100.0,
            grit: self.grit / 100.0,
            vibrato_depth: self.vibrato_depth / 100.0,
            vibrato_rate: self.vibrato_rate,
            scoop: self.scoop / 100.0,
            portamento: self.portamento / 1000.0,
            choir_size: self.voices.round().clamp(VOICES.0, VOICES.1) as i32,
        }
    }

    /// What the sound depends on, as numbers (for the phrase cache's key).
    pub fn key_values(&self) -> [f32; 9] {
        [
            self.formant,
            self.breathiness,
            self.grit,
            self.brightness,
            self.voices,
            self.vibrato_depth,
            self.vibrato_rate,
            self.scoop,
            self.portamento,
        ]
    }
}

impl Default for Voice {
    fn default() -> Self {
        Voice::new()
    }
}

/// The engine's voice presets, by name.
pub fn presets() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        // SAFETY: the names are static strings the engine keeps for the life of the program.
        let n = unsafe { novox::pw_novox_preset_count() };
        (0..n)
            .filter_map(|i| {
                let p = unsafe { novox::pw_novox_preset_name(i) };
                if p.is_null() {
                    return None;
                }
                // SAFETY: non-null, NUL-terminated, never freed (see above).
                let name: &'static CStr = unsafe { CStr::from_ptr(p) };
                name.to_str().ok()
            })
            .collect()
    })
}

/// The engine with its dictionary, made once and only read after that (the C++ side may be
/// used from several threads at once; see `crates/novox/shim/pw_novox.h`).
struct Engine(*mut novox::pw_novox);

// SAFETY: the engine is never changed after it is made, and the C++ side reads it from any
// thread (its dictionary is a map read under `const`).
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        use std::io::Read;
        let mut text = Vec::new();
        // Without the dictionary the engine guesses every word from its spelling.
        if flate2::read::GzDecoder::new(novox::CMUDICT_GZ)
            .read_to_end(&mut text)
            .is_err()
        {
            text.clear();
        }
        // SAFETY: `text` is valid for `len` bytes during the call; the engine copies it.
        Engine(unsafe { novox::pw_novox_new(text.as_ptr().cast::<c_char>(), text.len()) })
    })
}

/// The dictionary's entries (0 when it could not be read).
#[cfg(test)]
pub fn dictionary_words() -> usize {
    let e = engine();
    // SAFETY: a live engine or null, which the call allows.
    unsafe { novox::pw_novox_words(e.0) }
}

/// What a note sings when it has no lyric.
pub const DEFAULT_LYRIC: &str = crate::chimera_singer::DEFAULT_LYRIC;

/// A typed word as Novox takes it: its own marks (a trailing `-`, a lone `_` or `+`) taken out
/// so a word is never read as one of them; `{AA R}` phonemes and `~` (several syllables on one
/// note) are Novox's and kept.
fn word(w: &str) -> String {
    let w = w.trim().trim_end_matches('-');
    match w {
        "" | "_" | "+" => DEFAULT_LYRIC.into(),
        w => w.to_string(),
    }
}

/// The phrase's lyrics (as typed in Pro Wave: a word, `+` for its next syllable on this note,
/// `-` to hold the vowel) as Novox's note tokens: a word sung over several notes is `word-` on
/// its first, `-` on the ones between and an empty token on its last (Novox splits the word's
/// vowels over them, holding the last when the notes outnumber them); a hold is `_`. Notes with
/// nothing before them to go on sing [`DEFAULT_LYRIC`].
pub fn tokens(lyrics: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lyrics.len());
    let mut i = 0;
    while i < lyrics.len() {
        match lyrics[i] {
            "-" if !out.is_empty() => {
                out.push("_".into());
                i += 1;
            }
            first => {
                // The word's notes: this one and the `+` notes after it, holds in between.
                let w = if matches!(first, "-" | "+") {
                    DEFAULT_LYRIC.to_string()
                } else {
                    word(first)
                };
                let mut plus = Vec::new();
                let mut j = i + 1;
                while j < lyrics.len() && matches!(lyrics[j], "+" | "-") {
                    if lyrics[j] == "+" {
                        plus.push(j);
                    }
                    j += 1;
                }
                let first_out = out.len();
                out.push(if plus.is_empty() { w } else { format!("{w}-") });
                for at in i + 1..j {
                    out.push(match plus.iter().position(|p| *p == at) {
                        Some(k) if k + 1 == plus.len() => String::new(),
                        Some(_) => "-".into(),
                        None => "_".into(),
                    });
                }
                debug_assert_eq!(out.len() - first_out, j - i);
                i = j;
            }
        }
    }
    out
}

/// A prepared phrase: renders its samples in blocks without allocating.
pub struct Stream {
    ptr: *mut novox::pw_novox_stream,
}

// SAFETY: a stream is used by one thread at a time (it takes `&mut self` to render).
unsafe impl Send for Stream {}

impl Stream {
    /// Prepares `notes` (seconds from the phrase's start) sung by `v` at `rate` (allocates).
    pub fn new(notes: &[ScoreNote], v: &Voice, rate: u32) -> Option<Stream> {
        let lyrics: Vec<&str> = notes.iter().map(|n| n.lyric.as_str()).collect();
        let words: Vec<CString> = tokens(&lyrics)
            .into_iter()
            .map(|t| CString::new(t.replace('\0', "")).unwrap_or_default())
            .collect();
        let sung: Vec<novox::pw_novox_note> = notes
            .iter()
            .zip(&words)
            .map(|(n, w)| novox::pw_novox_note {
                start: n.start,
                dur: n.dur,
                pitch: n.pitch as f32,
                lyric: w.as_ptr(),
            })
            .collect();
        let preset = CString::new(v.preset.as_str()).ok()?;
        let knobs = v.knobs();
        // SAFETY: every pointer is valid for the call (the strings outlive it); the engine
        // copies what it keeps.
        let ptr = unsafe {
            novox::pw_novox_stream_new(
                engine().0,
                preset.as_ptr(),
                &knobs,
                sung.as_ptr(),
                sung.len(),
                f64::from(rate),
            )
        };
        (!ptr.is_null()).then_some(Stream { ptr })
    }

    /// Samples in all, the tail included.
    pub fn samples(&self) -> usize {
        // SAFETY: a live stream.
        unsafe { novox::pw_novox_stream_length(self.ptr) }
    }

    /// Seconds before the phrase's time 0 that sample 0 is.
    pub fn lead(&self) -> f64 {
        // SAFETY: a live stream.
        unsafe { novox::pw_novox_stream_lead(self.ptr) }
    }

    /// The next `out.len()` samples (silence past the end); returns how many had sound.
    /// Allocates nothing.
    pub fn process(&mut self, out: &mut [f32]) -> usize {
        // SAFETY: a live stream, and `out` valid for `out.len()` samples.
        unsafe { novox::pw_novox_stream_process(self.ptr, out.as_mut_ptr(), out.len()) }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: made by `pw_novox_stream_new`, freed once.
        unsafe { novox::pw_novox_stream_free(self.ptr) }
    }
}

/// Sings a phrase at `rate`: `notes` in seconds from sample 0 of the result. Empty when the
/// engine could not prepare it.
pub fn sing(notes: &[ScoreNote], v: &Voice, rate: u32) -> Vec<f32> {
    if notes.is_empty() {
        return Vec::new();
    }
    let Some(mut stream) = Stream::new(notes, v, rate) else {
        return Vec::new();
    };
    let mut out = vec![0f32; stream.samples()];
    for block in out.chunks_mut(BLOCK) {
        stream.process(block);
    }
    // Sample 0 of the stream is `lead` seconds before the phrase's time 0 (the first onset
    // consonant starts before its beat): line it up.
    let skip = (stream.lead() * f64::from(rate)).round().max(0.0) as usize;
    out.drain(..skip.min(out.len()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: f64, dur: f64, pitch: u8, lyric: &str) -> ScoreNote {
        ScoreNote {
            start,
            dur,
            pitch,
            lyric: lyric.into(),
        }
    }

    const RATE: u32 = crate::vocal::SAMPLE_RATE;

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    fn midi_hz(m: f64) -> f64 {
        440.0 * 2f64.powf((m - 69.0) / 12.0)
    }

    /// The pitch in 80 ms around `at` seconds of `x`, by the autocorrelation peak between 60
    /// and 1000 Hz, refined by a parabola through it.
    fn pitch_at(x: &[f32], at: f64) -> f64 {
        let fs = RATE as f64;
        let c = (at * fs) as usize;
        let half = (0.04 * fs) as usize;
        let f: Vec<f64> = x[c - half..c + half].iter().map(|v| *v as f64).collect();
        let n = f.len();
        let ac = |lag: usize| -> f64 {
            let (mut s, mut e0, mut e1) = (0.0, 0.0, 0.0);
            for i in 0..n - lag {
                s += f[i] * f[i + lag];
                e0 += f[i] * f[i];
                e1 += f[i + lag] * f[i + lag];
            }
            s / (e0 * e1).sqrt().max(1e-12)
        };
        let (lo, hi) = ((fs / 1000.0) as usize, (fs / 60.0) as usize);
        let r: Vec<f64> = (0..=hi + 1)
            .map(|l| if l < lo - 1 { 0.0 } else { ac(l) })
            .collect();
        // The first peak within 10 % of the highest (so not an octave below).
        let best = (lo..=hi).map(|l| r[l]).fold(f64::MIN, f64::max);
        let lag = (lo..=hi)
            .find(|&l| r[l] >= 0.9 * best && r[l] >= r[l - 1] && r[l] >= r[l + 1])
            .unwrap_or(lo);
        let (a, b, cc) = (r[lag - 1], r[lag], r[lag + 1]);
        let shift = 0.5 * (a - cc) / (a - 2.0 * b + cc);
        fs / (lag as f64 + if shift.is_finite() { shift } else { 0.0 })
    }

    /// A robot singing "la" on four notes a beat each at 92 BPM from half a second in: each
    /// note's pitch in the middle of the note within 15 cents (the robot preset has no vibrato,
    /// scoop or wander), and sound only where the notes are.
    #[test]
    fn novox_sings_each_note_at_its_pitch_and_time() {
        let beat = 60.0 / 92.0;
        let pitches = [57u8, 60, 62, 67];
        let notes: Vec<ScoreNote> = pitches
            .iter()
            .enumerate()
            .map(|(k, p)| note(0.5 + k as f64 * beat, beat * 0.9, *p, "la"))
            .collect();
        let x = sing(&notes, &Voice::new(), RATE);
        let fs = RATE as f64;
        assert!(x.iter().all(|v| v.is_finite()));
        let peak = x.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.05 && peak <= 1.0, "peak {peak}");
        let end = 0.5 + 3.0 * beat + beat * 0.9;
        let secs = x.len() as f64 / fs;
        assert!(
            secs >= end && secs < end + 1.0,
            "{secs} s for notes ending at {end} s"
        );
        for (k, p) in pitches.iter().enumerate() {
            let mid = 0.5 + (k as f64 + 0.45) * beat;
            let got = pitch_at(&x, mid);
            let cents = 1200.0 * (got / midi_hz(*p as f64)).log2();
            println!("novox note {k}: midi {p}, {got:.2} Hz, {cents:+.1} cents");
            assert!(
                cents.abs() < 15.0,
                "note {k}: {got} Hz is {cents} cents off"
            );
        }
        // Silence before the first consonant (it starts under 0.25 s before its beat).
        let lead = rms(&x[..(0.2 * fs) as usize]);
        let body = rms(&x[(0.5 * fs) as usize..(end * fs) as usize]);
        assert!(lead < body * 1e-3, "lead-in {lead}, notes {body}");
    }

    /// Each note starts where it should: a note after a rest is silent until just before its
    /// beat and sounding just after it. A phrase moved later by a whole number of the engine's
    /// control blocks (32 samples) comes out the same, later by as much, to the sample; moved by
    /// any other amount, later by as much to within one control block (0.7 ms at 44.1 kHz: the
    /// engine updates its controls on absolute block boundaries).
    #[test]
    fn novox_follows_the_notes_timing() {
        let fs = RATE as f64;
        let notes = [note(0.5, 0.4, 60, "ah"), note(1.6, 0.5, 64, "ah")];
        let x = sing(&notes, &Voice::new(), RATE);
        let level = |a: f64, b: f64| rms(&x[(a * fs) as usize..(b * fs) as usize]);
        let held = level(0.6, 0.8);
        // The rest between the notes, and each vowel sounding from its beat.
        assert!(level(1.05, 1.5) < held * 1e-2, "the rest is not silent");
        assert!(level(1.62, 1.75) > held * 0.3, "the second note is late");
        assert!(level(0.52, 0.65) > held * 0.3, "the first note is late");
        let first_sound = |x: &[f32]| x.iter().position(|v| v.abs() > 1e-3).unwrap_or(x.len());
        // "ah" has no consonant: the voice starts within 30 ms of the beat.
        let onset = first_sound(&x) as f64 / fs - 0.5;
        println!(
            "novox: \"ah\" sounds {:.1} ms from its beat",
            onset * 1000.0
        );
        assert!(
            onset.abs() < 0.03,
            "the first note starts {onset} s off its beat"
        );
        let moved = |shift: usize| {
            let later: Vec<ScoreNote> = notes
                .iter()
                .map(|n| ScoreNote {
                    start: n.start + shift as f64 / fs,
                    ..n.clone()
                })
                .collect();
            sing(&later, &Voice::new(), RATE)
        };
        // Whole control blocks: the same samples, later.
        let shift = 32 * 350;
        let y = moved(shift);
        assert_eq!(first_sound(&y), first_sound(&x) + shift);
        let n = x.len().min(y.len() - shift);
        let diff = (0..n)
            .map(|i| (x[i] - y[i + shift]).abs())
            .fold(0f32, f32::max);
        assert!(diff < 1e-4, "moved later it sings differently: {diff}");
        // Any amount: within a control block.
        for shift in [11_025, 4_411, 7] {
            let y = moved(shift);
            let off = first_sound(&y) as i64 - (first_sound(&x) + shift) as i64;
            assert!(
                off.abs() <= 32,
                "moved {shift} samples, it moved {off} more"
            );
        }
    }

    /// Words: a word over its `+` notes is split by its vowels, a `-` holds, and words change
    /// what is sung (a different word, a different sound; the same word, the same samples).
    #[test]
    fn novox_handles_words() {
        assert_eq!(tokens(&["twinkle", "+", "star"]), ["twinkle-", "", "star"]);
        assert_eq!(
            tokens(&["won", "+", "-", "+", "ful"]),
            ["won-", "-", "_", "", "ful"]
        );
        assert_eq!(tokens(&["you", "-", "-"]), ["you", "_", "_"]);
        assert_eq!(tokens(&["+", "-"]), ["la", "_"]);
        assert_eq!(tokens(&["well-", "-"]), ["well", "_"]);
        assert_eq!(tokens(&["_", "+"]), ["la-", ""]);
        assert_eq!(tokens(&["{HH AH0 L OW1}"]), ["{HH AH0 L OW1}"]);
        let sung = |words: &[&str]| {
            let notes: Vec<ScoreNote> = words
                .iter()
                .enumerate()
                .map(|(k, w)| note(0.5 + 0.5 * k as f64, 0.45, 62, w))
                .collect();
            sing(&notes, &Voice::new(), RATE)
        };
        let a = sung(&["hello", "+", "world"]);
        let b = sung(&["hello", "+", "world"]);
        let c = sung(&["goodbye", "+", "moon"]);
        let la = sung(&["la", "la", "la"]);
        assert_eq!(a, b, "the same words sing the same samples");
        for other in [&c, &la] {
            let n = a.len().min(other.len());
            let diff: Vec<f32> = (0..n).map(|i| a[i] - other[i]).collect();
            assert!(rms(&diff) > 0.1 * rms(&a), "other words sang the same");
        }
        assert!(a.iter().chain(&c).all(|v| v.is_finite()));
        assert!(dictionary_words() > 100_000, "{} words", dictionary_words());
    }

    /// Every knob changes the sound, every preset sings, and odd input stays finite.
    #[test]
    fn every_novox_control_changes_the_sound() {
        let notes = [note(0.5, 1.2, 57, "ah")];
        let mut base = Voice::preset("std").unwrap();
        base.vibrato_depth = 0.0;
        let reference = sing(&notes, &base, RATE);
        type Change = (&'static str, fn(&mut Voice));
        let changes: [Change; 9] = [
            ("formant", |v| v.formant = 130.0),
            ("breathiness", |v| v.breathiness = 60.0),
            ("grit", |v| v.grit = 80.0),
            ("brightness", |v| v.brightness = 80.0),
            ("voices", |v| v.voices = 4.0),
            ("vibrato depth", |v| v.vibrato_depth = 80.0),
            ("vibrato rate", |v| v.vibrato_rate = 8.0),
            ("scoop", |v| v.scoop = -150.0),
            ("portamento", |v| v.portamento = 400.0),
        ];
        for (name, change) in changes {
            let mut v = base.clone();
            if matches!(name, "vibrato rate") {
                v.vibrato_depth = 60.0;
            }
            let before = if matches!(name, "vibrato rate") {
                sing(&notes, &v, RATE)
            } else {
                reference.clone()
            };
            change(&mut v);
            // Portamento shows on a second note joined to the first.
            let (x, y) = if name == "portamento" {
                let two = [note(0.5, 0.6, 57, "ah"), note(1.1, 0.6, 64, "-")];
                (sing(&two, &base, RATE), sing(&two, &v, RATE))
            } else {
                (before, sing(&notes, &v, RATE))
            };
            let n = x.len().min(y.len());
            let diff: Vec<f32> = (0..n).map(|i| x[i] - y[i]).collect();
            assert!(rms(&diff) > 0.01 * rms(&x), "{name} made no difference");
        }
        for name in presets() {
            let x = sing(&notes, &Voice::preset(name).unwrap(), RATE);
            assert!(x.iter().all(|v| v.is_finite()) && rms(&x) > 1e-3, "{name}");
        }
        let odd = [
            note(0.5, 0.01, 0, ""),
            note(0.51, 0.3, 127, "xqzzy"),
            note(0.81, 0.2, 60, "{ZZ}"),
        ];
        assert!(sing(&odd, &Voice::new(), RATE)
            .iter()
            .all(|v| v.is_finite()));
        assert!(sing(&[], &Voice::new(), RATE).is_empty());
    }

    /// The layout of the structs shared with C++ is the same on both sides.
    #[test]
    fn the_c_interface_structs_match() {
        // SAFETY: plain functions returning sizes.
        let (k, n) = unsafe { (novox::pw_novox_knobs_size(), novox::pw_novox_note_size()) };
        assert_eq!(k, std::mem::size_of::<novox::pw_novox_knobs>());
        assert_eq!(n, std::mem::size_of::<novox::pw_novox_note>());
        assert!(presets().contains(&"robot") && presets().contains(&"std"));
        assert!(Voice::preset("no such voice").is_none());
    }

    #[cfg(target_os = "macos")]
    mod malloc_count {
        //! Counts every malloc, calloc, realloc and free made by one thread while armed, through
        //! macOS's `malloc_logger` hook (the system allocator calls it for every zone, so it sees
        //! the C++ engine's `new` as well as Rust's allocations). The hook is set only while
        //! counting, and reads no thread-local storage (a thread being torn down still
        //! allocates): the counting thread is told apart by `pthread_self`.
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;

        type Logger = unsafe extern "C" fn(u32, usize, usize, usize, usize, u32);
        unsafe extern "C" {
            static mut malloc_logger: Option<Logger>;
        }
        static THREAD: AtomicUsize = AtomicUsize::new(0);
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        /// One count at a time.
        static ONE: Mutex<()> = Mutex::new(());

        unsafe extern "C" fn log(_ty: u32, _a: usize, _b: usize, _c: usize, _r: usize, _s: u32) {
            // SAFETY: pthread_self only reads the thread's own register.
            let me = unsafe { libc::pthread_self() } as usize;
            if THREAD.load(Ordering::Relaxed) == me {
                COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }

        /// The allocations `f` makes on this thread.
        pub fn count(f: impl FnOnce()) -> usize {
            let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
            COUNT.store(0, Ordering::SeqCst);
            // SAFETY: as above.
            THREAD.store(unsafe { libc::pthread_self() } as usize, Ordering::SeqCst);
            // SAFETY: the allocator only reads the pointer; `log` is a plain function that
            // stays valid, so a thread that read it before it is cleared may still call it.
            unsafe { malloc_logger = Some(log) };
            f();
            unsafe { malloc_logger = None };
            THREAD.store(0, Ordering::SeqCst);
            COUNT.load(Ordering::SeqCst)
        }
    }

    /// Rendering a prepared phrase allocates nothing, in Rust or in the C++ engine: what a live
    /// host would run on the audio thread. Preparing it allocates (and the counter sees that,
    /// so it is counting).
    #[test]
    fn novox_render_path_does_not_allocate() {
        let notes = [
            note(0.5, 0.5, 60, "twinkle"),
            note(1.0, 0.5, 67, "+"),
            note(1.5, 1.0, 69, "star"),
        ];
        let mut choir = Voice::preset("choir").unwrap();
        choir.vibrato_depth = 40.0;
        for v in [Voice::new(), choir] {
            let mut block = [0f32; 256];
            #[cfg(target_os = "macos")]
            let made = malloc_count::count(|| {
                let s = Stream::new(&notes, &v, RATE);
                drop(s);
            });
            #[cfg(target_os = "macos")]
            assert!(
                made > 10,
                "the counter saw {made} allocations while preparing"
            );
            let mut s = Stream::new(&notes, &v, RATE).expect("a stream");
            let blocks = s.samples() / block.len() + 2;
            let mut sounding = 0;
            let mut render = || {
                for _ in 0..blocks {
                    sounding += s.process(&mut block);
                }
            };
            #[cfg(target_os = "macos")]
            {
                let n = malloc_count::count(|| assert_no_alloc::assert_no_alloc(&mut render));
                assert_eq!(n, 0, "{} allocated {n} times while rendering", v.preset);
            }
            #[cfg(not(target_os = "macos"))]
            assert_no_alloc::assert_no_alloc(&mut render);
            assert!(sounding > 0 && block.iter().all(|x| x.is_finite()));
        }
    }
}
