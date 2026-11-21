//! Japanese CV / VCV Phonemizer for OpenUtau / UTAU voicebanks.
//!
//! Converts lyric sequences and musical note timing into phoneme aliases
//! resolved against the voicebank's `oto.ini` and `prefix.map`.

use crate::voicebank::Voicebank;
use std::collections::HashMap;

/// The tail vowel of a Japanese hiragana or katakana syllable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JapaneseVowel {
    A,
    I,
    U,
    E,
    O,
    N,
}

impl JapaneseVowel {
    pub fn as_str(&self) -> &'static str {
        match self {
            JapaneseVowel::A => "a",
            JapaneseVowel::I => "i",
            JapaneseVowel::U => "u",
            JapaneseVowel::E => "e",
            JapaneseVowel::O => "o",
            JapaneseVowel::N => "n",
        }
    }
}

/// A resolved phoneme ready for rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPhoneme {
    /// Phoneme alias resolved in the voicebank (e.g. `a か` or `- か`).
    pub alias: String,
    /// Note tone (MIDI 0-127).
    pub tone: u8,
    /// Start position in quarter-note beats.
    pub beat: f64,
    /// Duration in quarter-note beats.
    pub duration_beats: f64,
    /// Lyric originally associated with this note.
    pub lyric: String,
}

/// Japanese VCV and CV Phonemizer.
pub struct JapaneseVcvPhonemizer {
    vowel_lookup: HashMap<String, JapaneseVowel>,
}

impl Default for JapaneseVcvPhonemizer {
    fn default() -> Self {
        Self::new()
    }
}

impl JapaneseVcvPhonemizer {
    pub fn new() -> Self {
        let mut vowel_lookup = HashMap::new();

        let table = [
            (JapaneseVowel::A, "ぁ,あ,か,が,さ,ざ,た,だ,な,は,ば,ぱ,ま,ゃ,や,ら,わ,ァ,ア,カ,ガ,サ,ザ,タ,ダ,ナ,ハ,バ,パ,マ,ャ,ヤ,ラ,ワ,a"),
            (JapaneseVowel::I, "ぃ,い,き,ぎ,し,じ,ち,ぢ,に,ひ,び,ぴ,み,り,ゐ,ィ,イ,キ,ギ,シ,ジ,チ,ヂ,ニ,ヒ,ビ,ピ,ミ,リ,ヰ,i"),
            (JapaneseVowel::U, "ぅ,う,く,ぐ,す,ず,つ,づ,ぬ,ふ,ぶ,ぷ,む,ゅ,ゆ,る,ゥ,ウ,ク,グ,ス,ズ,ツ,ヅ,ヌ,フ,ブ,プ,ム,ュ,ユ,ル,ヴ,u"),
            (JapaneseVowel::E, "ぇ,え,け,げ,せ,ぜ,て,で,ね,へ,べ,ぺ,め,れ,ゑ,ェ,エ,ケ,ゲ,セ,ゼ,テ,デ,ネ,ヘ,ベ,ペ,メ,レ,ヱ,e"),
            (JapaneseVowel::O, "ぉ,お,こ,ご,そ,ぞ,と,ど,の,ほ,ぼ,ぽ,も,ょ,よ,ろ,を,ォ,オ,コ,ゴ,ソ,ゾ,ト,ド,ノ,ホ,ボ,ポ,モ,ョ,ヨ,ロ,ヲ,o"),
            (JapaneseVowel::N, "ん,ン,n,N"),
        ];

        for (vowel, list) in table {
            for item in list.split(',') {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    vowel_lookup.insert(trimmed.to_string(), vowel);
                }
            }
        }

        Self { vowel_lookup }
    }

    /// Extracts the trailing vowel from a Japanese syllable.
    pub fn get_trailing_vowel(&self, lyric: &str) -> Option<JapaneseVowel> {
        let trimmed = lyric.trim();
        if let Some(&v) = self.vowel_lookup.get(trimmed) {
            return Some(v);
        }
        // If multi-char (e.g. "きゃ" or "kya"), check the last character
        if let Some(last_ch) = trimmed.chars().last() {
            let s = last_ch.to_string();
            if let Some(&v) = self.vowel_lookup.get(&s) {
                return Some(v);
            }
        }
        None
    }

    /// Phonemizes a sequence of notes with lyrics.
    ///
    /// Notes are tuples of `(beat, duration_beats, tone, lyric)`.
    pub fn phonemize(
        &self,
        notes: &[(f64, f64, u8, String)],
        voicebank: &Voicebank,
    ) -> Vec<ResolvedPhoneme> {
        let mut results = Vec::new();
        let mut prev_vowel: Option<JapaneseVowel> = None;
        let mut prev_end_beat = -1.0;

        for (beat, duration_beats, tone, lyric) in notes {
            let lyric_clean = lyric.trim();
            // Handle rests
            if lyric_clean.is_empty() || lyric_clean.eq_ignore_ascii_case("r") {
                prev_vowel = None;
                prev_end_beat = beat + duration_beats;
                continue;
            }

            // Check if there is a gap between notes (> 0.05 beats)
            if prev_end_beat >= 0.0 && (*beat - prev_end_beat) > 0.05 {
                prev_vowel = None;
            }

            // Build candidate aliases to search
            let mut candidates = Vec::new();
            if let Some(v) = prev_vowel {
                let v_str = v.as_str();
                candidates.push(format!("{v_str} {lyric_clean}"));
                candidates.push(format!("* {lyric_clean}"));
                candidates.push(lyric_clean.to_string());
                candidates.push(format!("- {lyric_clean}"));
            } else {
                candidates.push(format!("- {lyric_clean}"));
                candidates.push(lyric_clean.to_string());
            }

            // Search voicebank for the best candidate
            let mut matched_alias = lyric_clean.to_string();
            for c in &candidates {
                if voicebank.find_oto(c, Some(*tone)).is_some() {
                    matched_alias = c.clone();
                    break;
                }
            }

            results.push(ResolvedPhoneme {
                alias: matched_alias,
                tone: *tone,
                beat: *beat,
                duration_beats: *duration_beats,
                lyric: lyric_clean.to_string(),
            });

            // Update vowel context for next note
            prev_vowel = self.get_trailing_vowel(lyric_clean);
            prev_end_beat = beat + duration_beats;
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voicebank::{Oto, OtoSet};
    use std::path::PathBuf;

    #[test]
    fn detects_vowels() {
        let p = JapaneseVcvPhonemizer::new();
        assert_eq!(p.get_trailing_vowel("あ"), Some(JapaneseVowel::A));
        assert_eq!(p.get_trailing_vowel("か"), Some(JapaneseVowel::A));
        assert_eq!(p.get_trailing_vowel("き"), Some(JapaneseVowel::I));
        assert_eq!(p.get_trailing_vowel("く"), Some(JapaneseVowel::U));
        assert_eq!(p.get_trailing_vowel("け"), Some(JapaneseVowel::E));
        assert_eq!(p.get_trailing_vowel("こ"), Some(JapaneseVowel::O));
        assert_eq!(p.get_trailing_vowel("ん"), Some(JapaneseVowel::N));
        assert_eq!(p.get_trailing_vowel("きゃ"), Some(JapaneseVowel::A));
    }

    #[test]
    fn phonemizes_vcv_chain() {
        let p = JapaneseVcvPhonemizer::new();
        let mut vb = Voicebank::default();
        let otos = vec![
            Oto {
                alias: "- か".into(),
                wav: "ka.wav".into(),
                ..Default::default()
            },
            Oto {
                alias: "a わ".into(),
                wav: "wa.wav".into(),
                ..Default::default()
            },
            Oto {
                alias: "a い".into(),
                wav: "i.wav".into(),
                ..Default::default()
            },
        ];
        let set = OtoSet {
            name: "test".into(),
            dir: PathBuf::from("/test"),
            otos: otos.clone(),
        };
        for oto in otos {
            vb.alias_index
                .insert(oto.alias.clone(), (oto, PathBuf::from("/test")));
        }
        vb.oto_sets.push(set);

        let notes = vec![
            (0.0, 1.0, 60, "か".to_string()),
            (1.0, 1.0, 60, "わ".to_string()),
            (2.0, 1.0, 60, "い".to_string()),
        ];

        let res = p.phonemize(&notes, &vb);
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].alias, "- か");
        assert_eq!(res[1].alias, "a わ");
        assert_eq!(res[2].alias, "a い");
    }
}
