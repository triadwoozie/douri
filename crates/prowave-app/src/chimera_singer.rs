//! CHIMERA, an in-process singer of the Singer and Vocal Top Line (the engine is
//! `crates/chimera`, vendored from github.com/jamesondh/chimera-vocal-synth, MIT, by Jameson
//! Hodge).
//!
//! CHIMERA makes a voice by simulating an airway (lungs, vocal folds, a moving tongue, lips and
//! nose), not from recordings, so it needs nothing installed and sings on this Mac in the
//! background thread that renders the track. A phrase's notes become sung syllables, the
//! engine's song planner turns them into gestures (each vowel on its beat, the consonants before
//! it), and one voice plays them from the phrase's start. The engine runs at [`RATE`], the rate
//! it is calibrated and tested at; the result is resampled to the Vocal Top Line rate.
//!
//! Lyrics go through the engine's own front ends: English words through the CMU Pronouncing
//! Dictionary, Korean (Hangul), Russian (Cyrillic), pinyin with tone numbers, and Chinese
//! characters through the Mandarin readings vendored with it.
//!
//! Two copies of the engine are built: the current one ([`Engine::Current`], upstream eeb0c19)
//! and the one Pro Wave vendored first ([`Engine::V09fb776`], `crates/chimera-09fb776`, English
//! only), so a track can keep the sound it had before the update. The same code drives both
//! (the `engine!` macro below); they differ in the front ends and the airway's oversampling rule.
//!
//! Its controls are the engine's own: the body (a blend of the male, female and child
//! anatomies), vibrato depth and rate, breathiness, brightness and the singer's ring (a
//! narrowing just above the vocal folds), the controls of CHIMERA's own Sing panel.
use crate::vocal::ScoreNote;

/// Bumped when the vendored engine or how phrases are given to it changes the sound (cached
/// phrases are sung again). The engine copy is part of the cache key too.
pub const REV: u64 = 2;
/// The engine's output rate (it simulates at twice this inside).
pub const RATE: u32 = 48_000;
/// Sound kept after a phrase's planned end (the last consonant and the breath out).
const TAIL: f64 = 0.4;
/// A gap between notes longer than this (seconds) ends a run of notes; the next note starts a
/// new utterance on the same voice.
const JOIN: f64 = 1e-4;
/// The lung pressure (cmH2O) and vocal-fold tension a sung phrase starts from, as CHIMERA's own
/// song renders do.
const PRESSURE: f64 = 9.0;
const TENSION: f64 = 0.55;

/// Which copy of the engine sings.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Engine {
    /// Upstream commit eeb0c19 (2026-11-18), `crates/chimera`.
    #[default]
    Current,
    /// Upstream commit 09fb776 (2026-11-07), `crates/chimera-09fb776`: the sound before the
    /// update.
    V09fb776,
}

/// How CHIMERA sings, in the units shown (percent, cents, Hz).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Voice {
    pub engine: Engine,
    /// The body: 0..100 toward the female anatomy, and 0..100 toward the child's (male is what
    /// is left: at 0 and 0 the body is the adult male's).
    pub female: f32,
    pub child: f32,
    /// Vibrato on held vowels: depth in cents and rate in Hz.
    pub vibrato_depth: f32,
    pub vibrato_rate: f32,
    /// 0..100 each.
    pub breathiness: f32,
    pub brightness: f32,
    pub ring: f32,
}

impl Voice {
    /// The body's male, female and child shares, in percent (the same in both engines).
    pub fn shares(&self) -> [f32; 3] {
        let w = current::body(self).weights();
        [w[0].1, w[1].1, w[2].1].map(|x| (x * 100.0) as f32)
    }
}

/// What a note sings when it has no lyric.
pub const DEFAULT_LYRIC: &str = "la";

/// A note's lyric as CHIMERA takes it: the word, `+` (the word's next syllable), `-` (the vowel
/// held), or [`DEFAULT_LYRIC`].
pub fn lyric(l: &crate::lyrics::Lyric) -> String {
    match l.as_str() {
        "" => DEFAULT_LYRIC.into(),
        s => s.into(),
    }
}

/// Reads a gzipped text vendored with the engine (empty if it cannot be read).
fn gunzip(gz: &[u8]) -> String {
    use std::io::Read;
    let mut text = String::new();
    if flate2::read::GzDecoder::new(gz)
        .read_to_string(&mut text)
        .is_err()
    {
        text.clear();
    }
    text
}

/// The CMU Pronouncing Dictionary vendored with the engine (the same file in both copies).
const CMUDICT_GZ: &[u8] = include_bytes!("../../chimera/data/cmudict.txt.gz");

/// The parts of the singer that are the same for both engine copies, written once against the
/// crate `$c`. The module it is used in supplies `g2p()`, `word_syllables(word)` and
/// `oversample(fs)`.
macro_rules! engine {
    ($c:ident) => {
        use super::{Voice, DEFAULT_LYRIC, JOIN, PRESSURE, RATE, TAIL, TENSION};
        use crate::vocal::ScoreNote;
        use $c::anatomy::human::{ArticKey, ArticPatch, BodyMix};
        use $c::phonetics::sing::{midi_to_hz, plan_song, SongOptions, SungNote};
        use $c::phonetics::Syllable;
        use $c::voice::human_voice::{HumanVoice, SourcePatch};

        /// The body as the engine takes it.
        pub fn body(v: &Voice) -> BodyMix {
            BodyMix::new(v.female as f64 / 100.0, v.child as f64 / 100.0)
        }

        /// A held vowel: the syllable before, sung on at a new pitch.
        fn melisma(prev: &Syllable) -> Syllable {
            Syllable {
                text: "~".into(),
                onset: Vec::new(),
                coda: Vec::new(),
                melisma: Some(true),
                phrase_end: false,
                ..prev.clone()
            }
        }

        /// The syllable each note sings. A word goes on its note and the `+` notes after it:
        /// one syllable a note when the counts agree; with more notes than syllables, the last
        /// syllable is held over the rest; with fewer, the last note sings the next syllable
        /// and the word's end (its last consonants). `-` holds the vowel before.
        pub fn syllables(lyrics: &[&str]) -> Vec<Syllable> {
            let la = || word_syllables(DEFAULT_LYRIC).remove(0);
            let mut out: Vec<Syllable> = Vec::with_capacity(lyrics.len());
            let mut i = 0;
            while i < lyrics.len() {
                match lyrics[i] {
                    "-" | "+" if out.is_empty() => {
                        out.push(la());
                        i += 1;
                    }
                    "-" => {
                        let held = out.last().map_or_else(la, melisma);
                        out.push(held);
                        i += 1;
                    }
                    word => {
                        // The word's notes: this one and the `+` notes after it, holds in
                        // between.
                        let mut notes = vec![i];
                        let mut j = i + 1;
                        while j < lyrics.len() && matches!(lyrics[j], "+" | "-") {
                            if lyrics[j] == "+" {
                                notes.push(j);
                            }
                            j += 1;
                        }
                        let word = if word == "+" { DEFAULT_LYRIC } else { word };
                        let mut syl = word_syllables(word);
                        if syl.is_empty() {
                            syl.push(la());
                        }
                        let n = notes.len();
                        if syl.len() > n {
                            // The last note: the next syllable, ending as the word does.
                            let coda = syl.last().map(|s| s.coda.clone()).unwrap_or_default();
                            let end = syl.last().map(|s| (s.word_end, s.phrase_end));
                            syl.truncate(n);
                            if let (Some(last), Some((word_end, phrase_end))) =
                                (syl.last_mut(), end)
                            {
                                last.coda = coda;
                                last.word_end = word_end;
                                last.phrase_end = phrase_end;
                            }
                        }
                        let mut k = 0;
                        for (at, l) in lyrics.iter().enumerate().take(j).skip(i) {
                            if *l == "-" || (at != i && k >= syl.len()) {
                                let held = out.last().map_or_else(la, melisma);
                                out.push(held);
                            } else {
                                out.push(syl[k].clone());
                                k += 1;
                            }
                        }
                        i = j;
                    }
                }
            }
            out
        }

        /// Sings a phrase: `notes` in seconds from sample 0 of the result, which is at
        /// [`RATE`].
        pub fn sing(notes: &[ScoreNote], v: &Voice) -> Vec<f32> {
            let Some(first) = notes.first() else {
                return Vec::new();
            };
            let body = body(v);
            let lyrics: Vec<&str> = notes.iter().map(|n| n.lyric.as_str()).collect();
            let syl = syllables(&lyrics);
            let fs = RATE as f64;
            let mut voice = HumanVoice::new(fs, body.body(), oversample(fs));
            voice.set_source(&SourcePatch {
                f0: Some(midi_to_hz(first.pitch as f64)),
                voiced: Some(true),
                pressure: Some(PRESSURE),
                tension: Some(TENSION),
                breathiness: Some(v.breathiness as f64 / 100.0),
                brightness: Some(v.brightness as f64 / 100.0),
                vibrato_rate: Some(v.vibrato_rate as f64),
                ..Default::default()
            });
            voice.set_articulation(
                &ArticPatch::default().with(ArticKey::Epilarynx, v.ring as f64 / 100.0),
                None,
            );
            // Seconds as beats: the planner's beat is one second at 60 BPM.
            let opts = |t0: f64| SongOptions {
                body,
                bpm: 60.0,
                t0,
                vibrato: v.vibrato_depth as f64,
            };
            let mut end: f64 = 0.0;
            let mut run: Vec<SungNote> = Vec::new();
            let mut run_start = first.start;
            for (k, n) in notes.iter().enumerate() {
                run.push(SungNote {
                    midi: n.pitch as f64,
                    beats: n.dur,
                    syl: syl[k].clone(),
                });
                let next_joins = notes
                    .get(k + 1)
                    .is_some_and(|m| m.start - (n.start + n.dur) <= JOIN);
                if !next_joins {
                    let plan = plan_song(&run, &opts(run_start));
                    voice.schedule(&plan.gestures);
                    end = end.max(plan.end).max(n.start + n.dur);
                    run.clear();
                    if let Some(m) = notes.get(k + 1) {
                        run_start = m.start;
                    }
                }
            }
            let mut out = vec![0f32; ((end + TAIL) * fs).round() as usize];
            for block in out.chunks_mut(128) {
                voice.render(block);
            }
            out
        }
    };
}

/// The current engine (upstream eeb0c19).
pub mod current {
    use super::{gunzip, CMUDICT_GZ};
    use chimera::phonetics::english::EnglishG2P;
    use chimera::phonetics::mandarin::MandarinLexicon;
    use chimera::phonetics::syllables::lyrics_to_syllables_with;
    use std::sync::OnceLock;

    engine!(chimera);

    /// The CMU Pronouncing Dictionary, loaded once. Without it the engine spells words out by
    /// its letter-to-sound rules.
    fn g2p() -> &'static EnglishG2P {
        static G2P: OnceLock<EnglishG2P> = OnceLock::new();
        G2P.get_or_init(|| {
            let mut g = EnglishG2P::new();
            g.load_dict(&gunzip(CMUDICT_GZ));
            g
        })
    }

    /// The Mandarin readings (vendored with the engine), loaded once. Without them, words in
    /// Chinese characters are not sung.
    fn zh() -> &'static MandarinLexicon {
        static ZH: OnceLock<MandarinLexicon> = OnceLock::new();
        ZH.get_or_init(|| {
            let mut zh = MandarinLexicon::new();
            zh.load(&gunzip(include_bytes!(
                "../../chimera/data/mandarin.txt.gz"
            )));
            zh
        })
    }

    /// A word's syllables through every front end of the engine.
    fn word_syllables(word: &str) -> Vec<Syllable> {
        lyrics_to_syllables_with(word, g2p(), zh())
    }

    /// The airway's oversampling: the engine's own rule (2x at 44.1 and 48 kHz).
    fn oversample(fs: f64) -> usize {
        chimera::voice::human_voice::oversample_for(fs)
    }
}

/// The engine as Pro Wave first vendored it (upstream 09fb776): English only.
pub mod v09fb776 {
    use super::{gunzip, CMUDICT_GZ};
    use chimera_09fb776::phonetics::english::EnglishG2P;
    use chimera_09fb776::phonetics::syllables::lyrics_to_syllables;
    use std::sync::OnceLock;

    engine!(chimera_09fb776);

    fn g2p() -> &'static EnglishG2P {
        static G2P: OnceLock<EnglishG2P> = OnceLock::new();
        G2P.get_or_init(|| {
            let mut g = EnglishG2P::new();
            g.load_dict(&gunzip(CMUDICT_GZ));
            g
        })
    }

    fn word_syllables(word: &str) -> Vec<Syllable> {
        lyrics_to_syllables(word, g2p())
    }

    /// As the glue sang with it: 2x below 60 kHz.
    fn oversample(fs: f64) -> usize {
        if fs > 60_000.0 {
            1
        } else {
            2
        }
    }
}

/// Sings a phrase with the engine `v` names: `notes` in seconds from sample 0 of the result,
/// which is at [`RATE`].
pub fn sing(notes: &[ScoreNote], v: &Voice) -> Vec<f32> {
    match v.engine {
        Engine::Current => current::sing(notes, v),
        Engine::V09fb776 => v09fb776::sing(notes, v),
    }
}

/// Zeroth-order modified Bessel function of the first kind (the Kaiser window).
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, q) = (1.0, 1.0, x * x / 4.0);
    for k in 1..64 {
        term *= q / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Band-limited resampling from rate `from` to `to`: a Kaiser-windowed sinc (64 taps at the
/// lower rate, about 90 dB of stop band), cut off just under the lower rate's Nyquist
/// frequency, one filter per phase of the ratio. Sample 0 stays at time 0.
pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let g = gcd(from as u64, to as u64);
    let (up, down) = ((to as u64 / g) as usize, (from as u64 / g) as usize);
    // Input samples per output sample, and the filter's reach in input samples.
    let step = from as f64 / to as f64;
    let scale = step.max(1.0);
    const HALF: f64 = 32.0;
    let reach = (HALF * scale).ceil() as i64;
    let cutoff = 0.5 * 0.94 / scale;
    let beta = 8.6;
    let norm = bessel_i0(beta);
    let tap = |t: f64| -> f64 {
        let r = t / (HALF * scale);
        if r.abs() >= 1.0 {
            return 0.0;
        }
        let w = bessel_i0(beta * (1.0 - r * r).sqrt()) / norm;
        let a = 2.0 * std::f64::consts::PI * cutoff * t;
        let sinc = if t == 0.0 { 1.0 } else { a.sin() / a };
        2.0 * cutoff * sinc * w
    };
    // Output sample j sits at input position j·down/up: whole part and phase j·down mod up.
    let width = (2 * reach + 1) as usize;
    let phases: Vec<Vec<f64>> = (0..up)
        .map(|p| {
            let frac = p as f64 / up as f64;
            let taps: Vec<f64> = (0..width)
                .map(|i| tap(frac - (i as i64 - reach) as f64))
                .collect();
            // Unity gain at 0 Hz for every phase.
            let sum: f64 = taps.iter().sum();
            taps.iter().map(|t| t / sum).collect()
        })
        .collect();
    let n_out = ((x.len() as u64 * up as u64) as f64 / down as f64).round() as usize;
    (0..n_out)
        .map(|j| {
            let pos = j as u64 * down as u64;
            let base = (pos / up as u64) as i64;
            let taps = &phases[(pos % up as u64) as usize];
            let mut acc = 0.0;
            for (i, t) in taps.iter().enumerate() {
                let at = base + i as i64 - reach;
                if at >= 0 && (at as usize) < x.len() {
                    acc += x[at as usize] as f64 * t;
                }
            }
            acc as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::current::syllables;
    use super::*;
    use chimera::analysis::pitch::{yin_pitch, YinOpts};
    use chimera::anatomy::human::BodyMix;
    use chimera::phonetics::sing::midi_to_hz;
    use chimera::voice::human_voice::HumanVoice;

    pub fn voice() -> Voice {
        Voice {
            engine: Engine::Current,
            female: 0.0,
            child: 0.0,
            vibrato_depth: 0.0,
            vibrato_rate: 5.5,
            breathiness: 15.0,
            brightness: 25.0,
            ring: 0.0,
        }
    }

    fn note(start: f64, dur: f64, pitch: u8, lyric: &str) -> ScoreNote {
        ScoreNote {
            start,
            dur,
            pitch,
            lyric: lyric.into(),
        }
    }

    fn texts(lyrics: &[&str]) -> Vec<String> {
        syllables(lyrics).iter().map(|s| s.text.clone()).collect()
    }

    #[test]
    fn words_and_their_plus_notes_get_the_words_syllables() {
        let s = syllables(&["twinkle", "+", "star"]);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].melisma, None);
        assert_eq!(s[1].melisma, None);
        assert!(s[1].word_end && !s[0].word_end);
        // A hold carries the vowel before; extra notes hold the last syllable.
        let held = syllables(&["you", "-", "-"]);
        assert_eq!(held.len(), 3);
        assert!(held[1..].iter().all(|x| x.melisma == Some(true)));
        assert_eq!(held[2].nucleus, held[0].nucleus);
        let more = syllables(&["star", "+"]);
        assert_eq!((more.len(), more[1].melisma), (2, Some(true)));
        // Fewer notes than syllables: the last note ends the word.
        let fewer = syllables(&["wonderful"]);
        assert_eq!(fewer.len(), 1);
        assert!(fewer[0].word_end);
        // No word to go on: "la".
        assert_eq!(texts(&["+", "-"]).len(), 2);
    }

    /// A note's sound: the pitch YIN finds in 60 ms around `at` seconds of `x` (at [`RATE`]).
    fn pitch_at(x: &[f32], at: f64) -> f64 {
        let fs = RATE as f64;
        let c = (at * fs) as usize;
        let half = (0.03 * fs) as usize;
        let frame: Vec<f64> = x[c - half..c + half].iter().map(|v| *v as f64).collect();
        yin_pitch(&frame, fs, YinOpts::default()).f0
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    /// "la la la la", a beat each at 92 BPM, from half a second in (as a phrase starts): the
    /// sound is as long as the notes and the tail, not silent, and each note's pitch is there
    /// in the middle of its note. Both engine copies.
    #[test]
    fn a_phrase_is_sung_with_each_note_at_its_time_and_pitch() {
        for engine in [Engine::Current, Engine::V09fb776] {
            a_phrase_is_sung_in_time_and_in_tune_by(engine);
        }
    }

    fn a_phrase_is_sung_in_time_and_in_tune_by(engine: Engine) {
        let beat = 60.0 / 92.0;
        let pitches = [57u8, 60, 62, 64];
        let notes: Vec<ScoreNote> = pitches
            .iter()
            .enumerate()
            .map(|(k, p)| note(0.5 + k as f64 * beat, beat, *p, "la"))
            .collect();
        let x = sing(&notes, &Voice { engine, ..voice() });
        let fs = RATE as f64;
        let end = 0.5 + 4.0 * beat;
        let secs = x.len() as f64 / fs;
        assert!(
            secs >= end + TAIL - 0.01 && secs <= end + TAIL + 0.2,
            "{secs} s for notes ending at {end} s"
        );
        assert!(x.iter().all(|v| v.is_finite()));
        let peak = x.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.02 && peak <= 1.0, "peak {peak}");
        // Quiet before the first consonant, sounding through the notes.
        let lead = rms(&x[..(0.25 * fs) as usize]);
        let body = rms(&x[(0.5 * fs) as usize..(end * fs) as usize]);
        assert!(lead < body * 0.05, "lead-in {lead}, notes {body}");
        for (k, p) in pitches.iter().enumerate() {
            let mid = 0.5 + (k as f64 + 0.5) * beat;
            let want = midi_to_hz(*p as f64);
            let got = pitch_at(&x, mid);
            let cents = 1200.0 * (got / want).log2();
            assert!(
                cents.abs() < 50.0,
                "{engine:?} note {k}: {got} Hz, want {want} Hz"
            );
        }
    }

    /// Level in dB and the spectral centroid in Hz of `x` (at [`RATE`]), from 2048-sample
    /// frames.
    fn level_and_centroid(x: &[f32]) -> (f64, f64) {
        let n = 2048;
        let (mut num, mut den) = (0.0, 0.0);
        for frame in x.chunks_exact(n) {
            // A plain DFT of the frame's lower half of bins at a stride (enough for a centroid).
            for k in (1..n / 2).step_by(4) {
                let (mut re, mut im) = (0.0f64, 0.0f64);
                for (i, v) in frame.iter().enumerate() {
                    let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
                    let a = 2.0 * std::f64::consts::PI * (k * i) as f64 / n as f64;
                    re += *v as f64 * w * a.cos();
                    im -= *v as f64 * w * a.sin();
                }
                let mag = (re * re + im * im).sqrt();
                num += mag * k as f64 * RATE as f64 / n as f64;
                den += mag;
            }
        }
        (20.0 * rms(x).max(1e-12).log10(), num / den.max(1e-12))
    }

    /// The update changes the sound: the same phrase sung by the two engine copies differs by
    /// much more than the difference between two runs of one (none). Prints the level and the
    /// spectral centroid of each, for the record.
    #[test]
    fn the_update_changes_the_sound_and_the_old_engine_keeps_it() {
        let notes = [
            note(0.5, 0.6, 57, "row"),
            note(1.1, 0.6, 60, "row"),
            note(1.7, 1.2, 64, "boat"),
        ];
        for body in [(0.0, 0.0), (100.0, 0.0)] {
            let at = |engine| Voice {
                engine,
                female: body.0,
                child: body.1,
                ..voice()
            };
            let new = sing(&notes, &at(Engine::Current));
            let old = sing(&notes, &at(Engine::V09fb776));
            let n = new.len().min(old.len());
            let diff: Vec<f32> = (0..n).map(|i| new[i] - old[i]).collect();
            let (ln, cn) = level_and_centroid(&new[..n]);
            let (lo, co) = level_and_centroid(&old[..n]);
            println!(
                "female {}%: 09fb776 {lo:.1} dB, centroid {co:.0} Hz; eeb0c19 {ln:.1} dB, \
                 centroid {cn:.0} Hz; difference {:.1} dB below the new",
                body.0,
                20.0 * (rms(&diff) / rms(&new[..n])).log10().abs()
            );
            assert!(rms(&diff) > 0.05 * rms(&new[..n]), "the engines sang alike");
        }
    }

    /// Every control shown for CHIMERA changes what it sings.
    #[test]
    fn every_control_changes_the_sound() {
        let notes = [note(0.3, 0.8, 57, "ah")];
        let base = sing(&notes, &voice());
        type Change = (&'static str, fn(&mut Voice));
        let changes: [Change; 7] = [
            ("female", |v| v.female = 100.0),
            ("child", |v| v.child = 100.0),
            ("vibrato depth", |v| v.vibrato_depth = 80.0),
            ("vibrato rate", |v| {
                v.vibrato_depth = 60.0;
                v.vibrato_rate = 7.5;
            }),
            ("breathiness", |v| v.breathiness = 90.0),
            ("brightness", |v| v.brightness = 90.0),
            ("ring", |v| v.ring = 100.0),
        ];
        let with_vibrato = {
            let mut v = voice();
            v.vibrato_depth = 60.0;
            sing(&notes, &v)
        };
        for (name, change) in changes {
            let mut v = voice();
            change(&mut v);
            let other = sing(&notes, &v);
            let reference = if name == "vibrato rate" {
                &with_vibrato
            } else {
                &base
            };
            let n = other.len().min(reference.len());
            let diff: Vec<f32> = (0..n).map(|i| other[i] - reference[i]).collect();
            assert!(
                rms(&diff) > 0.01 * rms(reference),
                "{name} made no difference"
            );
        }
    }

    #[test]
    fn the_same_phrase_sings_the_same_samples() {
        let notes = [note(0.3, 0.4, 60, "row"), note(0.7, 0.4, 62, "boat")];
        assert_eq!(sing(&notes, &voice()), sing(&notes, &voice()));
    }

    /// Words in other scripts go through the engine's other front ends instead of being spelt
    /// out as English or left silent.
    #[test]
    fn korean_russian_and_chinese_words_get_their_own_syllables() {
        let la = syllables(&["la"]).remove(0);
        for word in ["안", "да", "好", "hao3"] {
            let s = syllables(&[word]);
            assert_eq!(s.len(), 1, "{word}");
            assert_ne!(s[0], la, "{word} sang la");
            assert!(!s[0].nucleus.ipa.is_empty(), "{word}: no vowel");
        }
        let x = sing(&[note(0.3, 0.6, 60, "好")], &voice());
        assert!(x.iter().all(|v| v.is_finite()) && rms(&x) > 1e-3);
    }

    #[test]
    fn the_body_shares_add_up() {
        let mut v = voice();
        v.female = 50.0;
        v.child = 50.0;
        let [m, f, c] = v.shares();
        assert_eq!((m, f, c), (25.0, 25.0, 50.0));
    }

    /// 48 kHz to 44.1 kHz: a 1 kHz sine comes out as the same sine at the new rate, sample 0
    /// at time 0, and the length scales with the rates.
    #[test]
    fn resampling_keeps_a_sine_and_its_timing() {
        let f = 1000.0;
        let x: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * std::f64::consts::PI * f * i as f64 / 48_000.0).sin() as f32)
            .collect();
        let y = resample(&x, 48_000, 44_100);
        assert_eq!(y.len(), 44_100);
        let worst = (200..44_100 - 200)
            .map(|i| {
                let want = (2.0 * std::f64::consts::PI * f * i as f64 / 44_100.0).sin();
                (y[i] as f64 - want).abs()
            })
            .fold(0.0, f64::max);
        assert!(worst < 1e-3, "worst error {worst}");
        // Above the new Nyquist frequency (22.05 kHz), a 23 kHz tone is taken out.
        let hi: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * std::f64::consts::PI * 23_000.0 * i as f64 / 48_000.0).sin() as f32)
            .collect();
        let z = resample(&hi, 48_000, 44_100);
        assert!(
            rms(&z[200..44_000]) < 1e-3,
            "alias {}",
            rms(&z[200..44_000])
        );
        assert_eq!(resample(&x, 48_000, 48_000), x);
    }

    /// The parts a live keyboard instrument would run on the audio thread (scheduling planned
    /// gestures, rendering, releasing a held note) allocate nothing once the voice has rendered
    /// one block; the planning allocates. The first block allocates once: the airway makes its
    /// slew-limit buffers on its first control step (`HumanAirway::apply`), so a live host must
    /// render a block before handing the voice to the audio thread.
    #[test]
    fn rendering_and_scheduling_do_not_allocate_but_planning_does() {
        use chimera::phonetics::sing::{plan_note_off, plan_note_on, NoteOnOptions};
        chimera::init();
        let mut voice = HumanVoice::new(RATE as f64, BodyMix::MALE.body(), 2);
        let syl = syllables(&["la"]).remove(0);
        let opts = NoteOnOptions {
            body: BodyMix::MALE,
            vibrato: 30.0,
            tag: 1.0,
            legato_from: None,
        };
        // Planning a note (off the audio thread) allocates: it builds segments and gestures.
        let on = plan_note_on(&syl, 60.0, &opts);
        let off = plan_note_off(&syl, 60.0, BodyMix::MALE);
        let mut block = [0f32; 128];
        // The one allocation: the first control step's buffers.
        voice.render(&mut block);
        assert_no_alloc::assert_no_alloc(|| {
            voice.schedule(&on);
            for _ in 0..200 {
                voice.render(&mut block);
            }
            voice.release(1.0);
            voice.release(2.0);
            voice.schedule(&off);
            for _ in 0..100 {
                voice.render(&mut block);
            }
        });
        assert!(block.iter().all(|v| v.is_finite()));
    }
}
