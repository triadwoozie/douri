//! The words of lyrics (a submodule of `lyrics.rs`, for Vocal Top Line): syllable counts, how a
//! word splits over its notes, pasted lines spread over notes, and what the singer is given.
use super::Lyric;
use crate::model::Note;
use std::collections::HashMap;
use std::sync::OnceLock;

impl Lyric {
    /// `+`: the next syllable of the word before.
    pub fn is_next_syllable(&self) -> bool {
        self.as_str() == "+"
    }

    /// `-`: holds the vowel before (melisma).
    pub fn is_hold(&self) -> bool {
        self.as_str() == "-"
    }
}

/// The CMUdict words whose syllable count the spelling rules get wrong: `word count` lines.
const EXCEPTIONS: &str = include_str!("../assets/syllable_exceptions.txt");

fn exceptions() -> &'static HashMap<&'static str, u8> {
    static MAP: OnceLock<HashMap<&'static str, u8>> = OnceLock::new();
    MAP.get_or_init(|| {
        EXCEPTIONS
            .lines()
            .filter_map(|l| {
                let (w, n) = l.split_once(' ')?;
                Some((w, n.trim().parse().ok()?))
            })
            .collect()
    })
}

fn is_vowel(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

/// The word in lower case with only its letters (apostrophes and accents dropped).
pub fn letters(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The vowel groups of a lower-case ASCII word, as byte ranges, after the spelling rules
/// (a silent final e, a silent -ed and -es, a sounded -le). The same rules as
/// `tools/vocal/syllables.py`; change both together.
pub fn vowel_groups(w: &str) -> Vec<(usize, usize)> {
    let b = w.as_bytes();
    let mut groups = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // A y that starts the word, or follows a vowel, is a consonant.
        let v = is_vowel(b[i]) && !(b[i] == b'y' && i == 0);
        if v {
            let s = i;
            while i < b.len() && is_vowel(b[i]) && !(b[i] == b'y' && i > s) {
                i += 1;
            }
            groups.push((s, i));
        } else {
            i += 1;
        }
    }
    let n = b.len();
    let ends = |suffix: &str| w.ends_with(suffix);
    let consonant_at = |k: usize| k < n && !is_vowel(b[k]);
    if groups.len() > 1 {
        let last = groups[groups.len() - 1];
        // A final e alone is silent ("time"), but "-le" after a consonant is a syllable
        // ("table"), and so are "-ee"/"-ie" endings (in the group already).
        if last == (n - 1, n) && b[n - 1] == b'e' {
            let le = n >= 3 && b[n - 2] == b'l' && consonant_at(n - 3);
            if !le {
                groups.pop();
            }
        } else if last == (n - 2, n - 1) && (ends("ed") || ends("es")) && n >= 4 {
            // "-ed" is silent except after t or d ("loved", "wanted"); "-es" is silent except
            // after s, x, z, ch, sh, ce, ge ("hopes", "boxes").
            let before = b[n - 3];
            let sounded = if ends("ed") {
                matches!(before, b't' | b'd')
            } else {
                matches!(before, b's' | b'x' | b'z' | b'c' | b'g')
                    || (n >= 4 && matches!(&w[n - 4..n - 2], "ch" | "sh"))
            };
            if !sounded {
                groups.pop();
            }
        }
    }
    groups
}

/// How many syllables `word` has: CMUdict's count when it differs from the spelling rules,
/// else the rules' (at least 1 for a word with letters).
pub fn syllables(word: &str) -> usize {
    let w = letters(word);
    if w.is_empty() {
        return 0;
    }
    if let Some(n) = exceptions().get(w.as_str()) {
        return (*n as usize).max(1);
    }
    vowel_groups(&w).len().max(1)
}

/// Consonant pairs that stay together when a word is split ("fa-ther", not "fat-her").
const DIGRAPHS: [&str; 6] = ["th", "sh", "ch", "ph", "wh", "qu"];

/// Consonant clusters an English syllable can start with (besides single consonants).
const ONSETS: [&str; 40] = [
    "b", "c", "d", "f", "g", "h", "j", "k", "l", "m", "n", "p", "q", "r", "s", "t", "v", "w", "x",
    "z", "bl", "br", "cl", "cr", "dr", "fl", "fr", "gl", "gr", "pl", "pr", "sc", "sk", "sl", "sp",
    "st", "tr", "thr", "str", "spr",
];

/// `word` cut into `n` syllables, for showing what each note of a word sings. Cuts go
/// between vowel groups: before a single consonant ("ba-by"), after the first of several
/// ("gent-ly"), keeping digraphs together.
pub fn split(word: &str, n: usize) -> Vec<String> {
    let w = word.trim();
    if n <= 1 || w.is_empty() {
        return vec![w.to_string()];
    }
    let low = w.to_ascii_lowercase();
    let groups = vowel_groups(&low);
    let mut cuts: Vec<usize> = Vec::new();
    for pair in groups.windows(2) {
        let (gap_s, gap_e) = (pair[0].1, pair[1].0);
        let gap = gap_e - gap_s;
        let cluster = &low[gap_s..gap_e];
        let cut = if gap <= 1 || DIGRAPHS.contains(&cluster) {
            gap_s
        } else {
            // The next syllable starts with the longest consonants that can begin an English
            // syllable, one at least left before ("gent-ly", "sis-ter", "mon-ster").
            (1..gap)
                .rev()
                .map(|k| gap_e - k)
                .find(|&at| ONSETS.contains(&&low[at..gap_e]))
                .unwrap_or(gap_e - 1)
        };
        if w.is_char_boundary(cut) && cut > 0 && cut < w.len() {
            cuts.push(cut);
        }
    }
    // Too many cuts: keep the first ones; too few: halve the longest piece.
    cuts.truncate(n - 1);
    let mut parts: Vec<String> = Vec::new();
    let mut at = 0;
    for c in cuts {
        parts.push(w[at..c].to_string());
        at = c;
    }
    parts.push(w[at..].to_string());
    while parts.len() < n {
        let (i, longest) = parts
            .iter()
            .enumerate()
            .max_by_key(|(_, p)| p.chars().count())
            .map(|(i, p)| (i, p.clone()))
            .unwrap_or((0, String::new()));
        let chars: Vec<char> = longest.chars().collect();
        if chars.len() < 2 {
            parts.push(String::new());
            continue;
        }
        let half = chars.len() / 2;
        parts[i] = chars[..half].iter().collect();
        parts.insert(i + 1, chars[half..].iter().collect());
    }
    parts
}

/// The lyric tokens of a pasted or typed line, one per note: each word then a `+` for each
/// further syllable; `-` and `+` written in the text are kept (one note each). A hyphen inside
/// a word ("gent-ly") gives its syllables; other punctuation is dropped.
pub fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split_whitespace() {
        if raw == "-" || raw == "+" {
            out.push(raw.to_string());
            continue;
        }
        let word: String = raw
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '\'' | '’' | '-'))
            .collect();
        let word = word.trim_matches('-');
        if word.is_empty() {
            continue;
        }
        let pieces: Vec<&str> = word.split('-').filter(|p| !p.is_empty()).collect();
        let (whole, n) = if pieces.len() > 1 {
            (pieces.concat(), pieces.len())
        } else {
            (word.to_string(), syllables(word).max(1))
        };
        out.push(whole);
        out.extend(std::iter::repeat_n("+".to_string(), n - 1));
    }
    out
}

/// The indexes of `notes` in the order they are sung (by start, then pitch, muted ones left
/// out).
pub fn sung_order(notes: &[Note]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..notes.len()).filter(|i| !notes[*i].mute).collect();
    order.sort_by(|a, b| {
        notes[*a]
            .start
            .total_cmp(&notes[*b].start)
            .then(notes[*b].pitch.cmp(&notes[*a].pitch))
    });
    order
}

/// Puts the tokens of `text` on the notes from `first` (an index into `notes`) on, in sung
/// order. With `only`, just those notes (the selection) take lyrics. Returns how many notes
/// got one.
pub fn spread(notes: &mut [Note], text: &str, first: Option<usize>, only: &[usize]) -> usize {
    let order: Vec<usize> = sung_order(notes)
        .into_iter()
        .filter(|i| only.is_empty() || only.contains(i))
        .collect();
    let from = first
        .and_then(|f| order.iter().position(|i| *i == f))
        .unwrap_or(0);
    let mut n = 0;
    for (tok, i) in tokens(text).iter().zip(order[from..].iter()) {
        notes[*i].lyric = Lyric::new(tok);
        n += 1;
    }
    n
}

/// What each note sings, for drawing: its word, or for a `+` the syllable of the word
/// before it falls on, or `-` for a held vowel. Indexed like `notes`.
pub fn shown(notes: &[Note]) -> Vec<String> {
    let mut out = vec![String::new(); notes.len()];
    let order = sung_order(notes);
    let mut k = 0;
    while k < order.len() {
        let i = order[k];
        let lyric = notes[i].lyric;
        if lyric.is_empty() || lyric.is_hold() || lyric.is_next_syllable() {
            out[i] = lyric.as_str().to_string();
            k += 1;
            continue;
        }
        // The word and the `+` notes after it (holds between them belong to the word).
        let mut members = vec![i];
        let mut j = k + 1;
        while j < order.len() {
            let l = notes[order[j]].lyric;
            if l.is_next_syllable() || l.is_hold() {
                members.push(order[j]);
                j += 1;
            } else {
                break;
            }
        }
        let syl_notes: Vec<usize> = members
            .iter()
            .copied()
            .filter(|m| *m == i || notes[*m].lyric.is_next_syllable())
            .collect();
        let parts = split(lyric.as_str(), syl_notes.len());
        for (m, part) in syl_notes.iter().zip(parts) {
            out[*m] = part;
        }
        for m in members.iter().filter(|m| notes[**m].lyric.is_hold()) {
            out[*m] = "-".into();
        }
        k = j;
    }
    out
}

/// What each note sings, as the singer takes it (OpenUtau's lyrics): the word on its first
/// note, `+` on each note that continues it (next syllables and held vowels alike; the
/// phonemizer spreads the word's syllables over them), `la` for a note with no lyric.
pub fn engine_lyric(lyric: &Lyric) -> String {
    match lyric.as_str() {
        "" => "la".into(),
        "+" | "-" => "+".into(),
        s => s.replace(['"', '\\'], ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::MAX_LYRIC;

    fn note(start: f32, lyric: &str) -> Note {
        Note {
            start,
            len: 1.0,
            pitch: 60,
            vel: 100,
            mute: false,
            lyric: Lyric::new(lyric),
        }
    }

    #[test]
    fn a_lyric_is_kept_in_the_note_and_cut_at_a_character_boundary() {
        assert_eq!(Lyric::new("  row ").as_str(), "row");
        assert!(Lyric::NONE.is_empty());
        let long = "é".repeat(40);
        let l = Lyric::new(&long);
        assert!(l.as_str().len() <= MAX_LYRIC);
        assert_eq!(l.as_str().chars().count(), MAX_LYRIC / 2);
        assert!(Lyric::new("+").is_next_syllable() && Lyric::new("-").is_hold());
    }

    #[test]
    fn syllables_follow_the_dictionary_and_the_spelling_rules() {
        for (w, n) in [
            ("row", 1),
            ("boat", 1),
            ("gently", 2),
            ("stream", 1),
            ("time", 1),
            ("table", 2),
            ("loved", 1),
            ("wanted", 2),
            ("hopes", 1),
            ("boxes", 2),
            ("beautiful", 3),
            ("baby", 2),
            ("tonight", 2),
            ("I", 1),
            ("Don't", 1),
            ("everything", 3),
            ("", 0),
        ] {
            assert_eq!(syllables(w), n, "{w}");
        }
    }

    #[test]
    fn words_split_into_the_syllables_their_notes_sing() {
        assert_eq!(split("gently", 2), ["gent", "ly"]);
        assert_eq!(split("baby", 2), ["ba", "by"]);
        assert_eq!(split("father", 2), ["fa", "ther"]);
        assert_eq!(split("beautiful", 3).concat(), "beautiful");
        assert_eq!(split("row", 1), ["row"]);
        // More notes than vowel groups: the longest piece is halved.
        assert_eq!(split("ah", 2).len(), 2);
    }

    #[test]
    fn a_line_becomes_one_token_per_note() {
        assert_eq!(
            tokens("Row, row, row your boat, gently down the stream"),
            ["Row", "row", "row", "your", "boat", "gently", "+", "down", "the", "stream"]
        );
        assert_eq!(tokens("you - - +"), ["you", "-", "-", "+"]);
        assert_eq!(tokens("gent-ly"), ["gently", "+"]);
        assert_eq!(tokens("don't stop"), ["don't", "stop"]);
    }

    #[test]
    fn pasting_spreads_the_line_over_the_notes_in_order() {
        let mut notes = vec![note(2.0, ""), note(0.0, ""), note(1.0, ""), note(3.0, "")];
        notes[3].mute = true;
        let n = spread(&mut notes, "row your boat", None, &[]);
        assert_eq!(n, 3);
        assert_eq!(notes[1].lyric.as_str(), "row");
        assert_eq!(notes[2].lyric.as_str(), "your");
        assert_eq!(notes[0].lyric.as_str(), "boat");
        assert!(notes[3].lyric.is_empty(), "muted notes are skipped");
        // From a note on, and only the selected notes.
        let mut notes = vec![note(0.0, "a"), note(1.0, "b"), note(2.0, "c")];
        spread(&mut notes, "x y", Some(1), &[]);
        assert_eq!(
            notes.iter().map(|n| n.lyric.as_str()).collect::<Vec<_>>(),
            ["a", "x", "y"]
        );
        let mut notes = vec![note(0.0, ""), note(1.0, ""), note(2.0, "")];
        spread(&mut notes, "x y", None, &[0, 2]);
        assert_eq!(notes[2].lyric.as_str(), "y");
        assert!(notes[1].lyric.is_empty());
    }

    #[test]
    fn continuation_notes_show_their_syllable() {
        let notes = vec![
            note(0.0, "gently"),
            note(1.0, "+"),
            note(2.0, "you"),
            note(3.0, "-"),
            note(4.0, "-"),
        ];
        assert_eq!(shown(&notes), ["gent", "ly", "you", "-", "-"]);
        assert_eq!(engine_lyric(&Lyric::new("-")), "+");
        assert_eq!(engine_lyric(&Lyric::NONE), "la");
        assert_eq!(engine_lyric(&Lyric::new("row")), "row");
    }
}
