//! OpenUtau / Classic UTAU Voicebank and oto.ini loader.
//!
//! Loads `character.txt`, `prefix.map`, and `oto.ini` files across subdirectories,
//! decoding Shift-JIS (standard UTAU Japanese encoding) or UTF-8 via `encoding_rs`.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A single oto.ini record specifying timing parameters for a phoneme sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Oto {
    /// Source audio file name (e.g. `_ka.wav`).
    pub wav: String,
    /// Alias used in phonemizer lookup (e.g. `a か`, `- か`, `か`).
    pub alias: String,
    /// Offset in milliseconds from start of sample to start of phoneme.
    pub offset: f64,
    /// Fixed consonant duration in milliseconds (unstretched part).
    pub consonant: f64,
    /// Cutoff in milliseconds. If negative, distance from end of file.
    pub cutoff: f64,
    /// Preutterance in milliseconds (duration before the rhythmic note onset).
    pub preutterance: f64,
    /// Overlap in milliseconds (crossfade window with previous note).
    pub overlap: f64,
}

impl Default for Oto {
    fn default() -> Self {
        Self {
            wav: String::new(),
            alias: String::new(),
            offset: 0.0,
            consonant: 0.0,
            cutoff: 0.0,
            preutterance: 0.0,
            overlap: 0.0,
        }
    }
}

/// A set of oto entries, usually corresponding to one folder containing an `oto.ini`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OtoSet {
    pub name: String,
    pub dir: PathBuf,
    pub otos: Vec<Oto>,
}

/// Pitch mapping entry from `prefix.map`: maps MIDI tone number (0-127) to prefix and suffix.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrefixMap {
    /// Map from MIDI tone (e.g. 60 for C4) to (prefix, suffix).
    pub map: HashMap<u8, (String, String)>,
}

impl PrefixMap {
    pub fn empty() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Looks up the prefix and suffix for a given MIDI tone.
    pub fn get_prefix_suffix(&self, tone: u8) -> (&str, &str) {
        if let Some((p, s)) = self.map.get(&tone) {
            (p.as_str(), s.as_str())
        } else {
            ("", "")
        }
    }

    /// Builds a mapped alias for a base phoneme alias and note pitch: `<prefix><alias><suffix>`.
    pub fn map_alias(&self, base_alias: &str, tone: u8) -> String {
        let (p, s) = self.get_prefix_suffix(tone);
        format!("{p}{base_alias}{s}")
    }

    /// Parses a standard UTAU `prefix.map` text content.
    pub fn parse(text: &str) -> Self {
        let mut map = HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.is_empty() {
                continue;
            }
            let tone_str = parts[0].trim();
            if let Some(tone) = parse_tone_name(tone_str) {
                let prefix = parts.get(1).map(|s| s.trim()).unwrap_or("");
                let suffix = parts.get(2).map(|s| s.trim()).unwrap_or("");
                map.insert(tone, (prefix.to_string(), suffix.to_string()));
            }
        }
        Self { map }
    }
}

/// A loaded UTAU / OpenUtau Voicebank.
#[derive(Clone, Debug, Default)]
pub struct Voicebank {
    pub name: String,
    pub author: String,
    pub voice: String,
    pub web: String,
    pub version: String,
    pub image: Option<PathBuf>,
    pub base_path: PathBuf,
    pub oto_sets: Vec<OtoSet>,
    pub prefix_map: PrefixMap,
    /// Fast alias lookup: alias -> (Oto, directory path).
    pub alias_index: HashMap<String, (Oto, PathBuf)>,
}

impl Voicebank {
    /// Finds an oto entry matching `alias`, or with prefix/suffix applied for `tone`.
    pub fn find_oto(&self, alias: &str, tone: Option<u8>) -> Option<(&Oto, &Path)> {
        if let Some(t) = tone {
            let mapped = self.prefix_map.map_alias(alias, t);
            if let Some((oto, dir)) = self.alias_index.get(&mapped) {
                return Some((oto, dir.as_path()));
            }
        }
        if let Some((oto, dir)) = self.alias_index.get(alias) {
            return Some((oto, dir.as_path()));
        }
        None
    }

    /// Loads a voicebank from a folder containing `character.txt` or `oto.ini`.
    pub fn load(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref();
        let mut vb = Voicebank {
            base_path: dir.to_path_buf(),
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Voicebank".to_string()),
            ..Default::default()
        };

        // 1. Load character.txt if present
        let char_txt = dir.join("character.txt");
        if char_txt.is_file() {
            if let Ok(content) = read_file_detect_encoding(&char_txt) {
                for line in content.lines() {
                    let line = line.trim();
                    if let Some((k, v)) = line.split_once('=') {
                        let k = k.trim().to_lowercase();
                        let v = v.trim();
                        match k.as_str() {
                            "name" => vb.name = v.to_string(),
                            "author" => vb.author = v.to_string(),
                            "voice" => vb.voice = v.to_string(),
                            "web" => vb.web = v.to_string(),
                            "version" => vb.version = v.to_string(),
                            "image" => vb.image = Some(dir.join(v)),
                            _ => {}
                        }
                    }
                }
            }
        }

        // 2. Load prefix.map if present
        let pmap_file = dir.join("prefix.map");
        if pmap_file.is_file() {
            if let Ok(content) = read_file_detect_encoding(&pmap_file) {
                vb.prefix_map = PrefixMap::parse(&content);
            }
        }

        // 3. Scan for oto.ini files recursively
        vb.scan_oto_sets(dir);

        // 4. Build fast lookup index
        for set in &vb.oto_sets {
            for oto in &set.otos {
                if !oto.alias.is_empty() {
                    vb.alias_index
                        .insert(oto.alias.clone(), (oto.clone(), set.dir.clone()));
                }
            }
        }

        Ok(vb)
    }

    fn scan_oto_sets(&mut self, current_dir: &Path) {
        let oto_ini = current_dir.join("oto.ini");
        if oto_ini.is_file() {
            if let Ok(content) = read_file_detect_encoding(&oto_ini) {
                let set = parse_oto_ini(&content, current_dir);
                self.oto_sets.push(set);
            }
        }

        if let Ok(entries) = std::fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.scan_oto_sets(&path);
                }
            }
        }
    }
}

/// Parses an `oto.ini` file's content into an `OtoSet`.
pub fn parse_oto_ini(content: &str, dir: &Path) -> OtoSet {
    let mut otos = Vec::new();
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Format: <wav>=<alias>,<offset>,<consonant>,<cutoff>,<preutter>,<overlap>
        if let Some((wav, rest)) = line.split_once('=') {
            let wav = wav.trim().to_string();
            let parts: Vec<&str> = rest.split(',').collect();
            let alias = parts
                .first()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    Path::new(&wav)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| wav.clone())
                });

            let parse_num = |idx: usize| -> f64 {
                parts
                    .get(idx)
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .unwrap_or(0.0)
            };

            let offset = parse_num(1);
            let consonant = parse_num(2);
            let cutoff = parse_num(3);
            let preutterance = parse_num(4);
            let overlap = parse_num(5);

            otos.push(Oto {
                wav,
                alias,
                offset,
                consonant,
                cutoff,
                preutterance,
                overlap,
            });
        }
    }

    OtoSet {
        name,
        dir: dir.to_path_buf(),
        otos,
    }
}

/// Reads a file and automatically decodes Shift-JIS or UTF-8 text.
pub fn read_file_detect_encoding(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    // Check for UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(&bytes[3..]) {
            return Ok(s.to_string());
        }
    }

    // Try UTF-8 first (strict)
    if let Ok(s) = std::str::from_utf8(&bytes) {
        return Ok(s.to_string());
    }

    // Fall back to Shift-JIS (standard UTAU Japanese encoding)
    let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
    if !had_errors {
        return Ok(cow.into_owned());
    }

    // Lossy UTF-8 fallback
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Converts a musical note name like "C4", "F#3", "Bb5" or integer tone string to MIDI number (0-127).
pub fn parse_tone_name(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Ok(n) = s.parse::<u8>() {
        return Some(n);
    }
    if s.len() < 2 {
        return None;
    }
    let mut chars = s.chars();
    let note_letter = chars.next()?.to_ascii_uppercase();
    let base_semitone = match note_letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    let rem: String = chars.collect();
    let (accidentals, octave_str) = if rem.starts_with('#') || rem.starts_with('♯') {
        (1, &rem[1..])
    } else if rem.starts_with('b') || rem.starts_with('♭') {
        (-1, &rem[1..])
    } else {
        (0, rem.as_str())
    };

    let octave: i32 = octave_str.parse().ok()?;
    // MIDI: C-1 = 0, C4 = 60
    let midi = (octave + 1) * 12 + base_semitone + accidentals;
    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

/// Formats a MIDI tone number as standard note name (e.g. 60 -> "C4").
pub fn tone_to_name(tone: u8) -> String {
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (tone as i32 / 12) - 1;
    let semitone = (tone % 12) as usize;
    format!("{}{}", names[semitone], octave)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tone_names_correctly() {
        assert_eq!(parse_tone_name("C4"), Some(60));
        assert_eq!(parse_tone_name("A4"), Some(69));
        assert_eq!(parse_tone_name("C#4"), Some(61));
        assert_eq!(parse_tone_name("Db4"), Some(61));
        assert_eq!(parse_tone_name("B3"), Some(59));
        assert_eq!(parse_tone_name("60"), Some(60));
        assert_eq!(tone_to_name(60), "C4");
        assert_eq!(tone_to_name(69), "A4");
    }

    #[test]
    fn parses_oto_ini_lines() {
        let content = r#"
# Sample oto.ini
_ka.wav=- か,50,120,-100,80,40
_ka_a.wav=a か,45,115,-90,75,35
_blank.wav=,10,20,30,40,50
"#;
        let set = parse_oto_ini(content, Path::new("/test/bank"));
        assert_eq!(set.otos.len(), 3);
        assert_eq!(set.otos[0].wav, "_ka.wav");
        assert_eq!(set.otos[0].alias, "- か");
        assert_eq!(set.otos[0].offset, 50.0);
        assert_eq!(set.otos[0].consonant, 120.0);
        assert_eq!(set.otos[0].cutoff, -100.0);
        assert_eq!(set.otos[0].preutterance, 80.0);
        assert_eq!(set.otos[0].overlap, 40.0);

        assert_eq!(set.otos[1].alias, "a か");
        assert_eq!(set.otos[2].alias, "_blank"); // Fallback to file stem
    }

    #[test]
    fn parses_prefix_map() {
        let content = "C4\t\t_C4\nA4\thigh_\t\n";
        let map = PrefixMap::parse(content);
        assert_eq!(map.get_prefix_suffix(60), ("", "_C4"));
        assert_eq!(map.get_prefix_suffix(69), ("high_", ""));
        assert_eq!(map.get_prefix_suffix(70), ("", ""));
        assert_eq!(map.map_alias("あ", 60), "あ_C4");
        assert_eq!(map.map_alias("あ", 69), "high_あ");
    }
}
