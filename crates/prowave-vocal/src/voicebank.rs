//! OpenUtau / Classic UTAU Voicebank and oto.ini loader.
//!
//! Loads `character.txt`, `prefix.map`, and `oto.ini` files across subdirectories,
//! decoding Shift-JIS (standard UTAU Japanese encoding) or UTF-8 via `encoding_rs`.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
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
    /// Detects the voicebank type (CV, VCV, CVVC) based on alias conventions.
    pub fn detect_type(&self) -> VoicebankType {
        let mut vcv_count = 0usize;
        let mut cvvc_count = 0usize;
        for alias in self.alias_index.keys() {
            if alias.starts_with("- ")
                || alias.starts_with("a ")
                || alias.starts_with("i ")
                || alias.starts_with("u ")
                || alias.starts_with("e ")
                || alias.starts_with("o ")
                || alias.starts_with("n ")
            {
                vcv_count += 1;
            } else if alias.contains(' ') {
                cvvc_count += 1;
            }
        }
        if vcv_count > 3 {
            VoicebankType::VCV
        } else if cvvc_count > 3 {
            VoicebankType::CVVC
        } else {
            VoicebankType::CV
        }
    }

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

/// Classification of UTAU voicebanks based on phoneme alias conventions.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum VoicebankType {
    #[default]
    CV,
    VCV,
    CVVC,
}

impl VoicebankType {
    pub fn label(self) -> &'static str {
        match self {
            Self::CV => "CV",
            Self::VCV => "VCV",
            Self::CVVC => "CVVC",
        }
    }
}

/// Metadata and summary for an installed UTAU / OpenUtau singer.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SingerInfo {
    pub id: String,
    pub name: String,
    pub author: String,
    pub voice: String,
    pub path: PathBuf,
    pub image: Option<PathBuf>,
    pub vb_type: VoicebankType,
    pub sample_count: usize,
    pub oto_count: usize,
    pub resampler: Option<String>,
}

/// Default storage directory for UTAU voicebanks:
/// - Linux: `~/.local/share/douri/Singers/`
/// - Windows: `%APPDATA%/douri/Singers/`
/// - macOS: `~/Library/Application Support/douri/Singers/`
pub fn default_singers_dir() -> PathBuf {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("APPDATA")
            .or_else(|| var("USERPROFILE").map(|p| p.join("AppData").join("Roaming")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("douri")
    } else if cfg!(target_os = "macos") {
        var("HOME")
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join("douri")
    } else {
        var("XDG_DATA_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".local").join("share")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("douri")
    };
    base.join("Singers")
}

/// Inspects a directory to read singer metadata if it contains `character.txt` or `oto.ini`.
pub fn load_singer_info(dir: impl AsRef<Path>) -> Option<SingerInfo> {
    let dir = dir.as_ref();
    if !dir.is_dir() {
        return None;
    }
    let mut root_dir = dir.to_path_buf();
    if !root_dir.join("oto.ini").is_file() && !root_dir.join("character.txt").is_file() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() && (p.join("oto.ini").is_file() || p.join("character.txt").is_file())
                {
                    root_dir = p;
                    break;
                }
            }
        }
    }
    let vb = Voicebank::load(&root_dir).ok()?;
    let vb_type = vb.detect_type();
    let sample_count = vb
        .oto_sets
        .iter()
        .flat_map(|s| &s.otos)
        .map(|o| &o.wav)
        .collect::<std::collections::HashSet<_>>()
        .len();
    let oto_count = vb.alias_index.len();
    let id = root_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| vb.name.clone());

    Some(SingerInfo {
        id,
        name: vb.name,
        author: vb.author,
        voice: vb.voice,
        path: root_dir,
        image: vb.image,
        vb_type,
        sample_count,
        oto_count,
        resampler: None,
    })
}

/// Discovers all voicebanks inside the default storage directory and user search folders.
pub fn discover_singers(extra_dirs: &[PathBuf]) -> Vec<SingerInfo> {
    let mut search_paths = vec![default_singers_dir()];
    search_paths.extend_from_slice(extra_dirs);

    let mut singers = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    for base in search_paths {
        if !base.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(info) = load_singer_info(&path) {
                        if seen_names.insert(info.name.clone()) {
                            singers.push(info);
                        }
                    }
                }
            }
        }
    }

    singers.sort_by(|a, b| a.name.cmp(&b.name));
    singers
}

/// Imports a voicebank from a `.zip` archive, decoding Shift-JIS or UTF-8 file paths.
pub fn import_voicebank_zip(
    zip_path: &Path,
    target_dir: Option<&Path>,
) -> Result<SingerInfo, String> {
    let file = File::open(zip_path).map_err(|e| format!("Could not open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| format!("Invalid zip archive: {e}"))?;
    let dest_base = target_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_singers_dir);
    fs::create_dir_all(&dest_base)
        .map_err(|e| format!("Failed to create singers directory: {e}"))?;

    let stem = zip_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ImportedSinger");

    // Check if the zip has a single root folder
    let mut common_root: Option<String> = None;
    let mut has_common_root = true;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw = entry.name_raw();
        let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(raw);
        let path_str = if had_errors {
            String::from_utf8_lossy(raw).to_string()
        } else {
            std::str::from_utf8(raw)
                .map(String::from)
                .unwrap_or_else(|_| cow.into_owned())
        };
        let norm = path_str.replace('\\', "/");
        if let Some((first, rest)) = norm.split_once('/') {
            if !rest.is_empty() {
                if let Some(ref r) = common_root {
                    if r != first {
                        has_common_root = false;
                        break;
                    }
                } else {
                    common_root = Some(first.to_string());
                }
            }
        } else {
            has_common_root = false;
            break;
        }
    }

    let extract_folder = if has_common_root && common_root.is_some() {
        dest_base.clone()
    } else {
        dest_base.join(stem)
    };

    fs::create_dir_all(&extract_folder).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let raw = entry.name_raw();
        let (cow, _, had_errors) = encoding_rs::SHIFT_JIS.decode(raw);
        let path_str = if had_errors {
            String::from_utf8_lossy(raw).to_string()
        } else {
            std::str::from_utf8(raw)
                .map(String::from)
                .unwrap_or_else(|_| cow.into_owned())
        };
        let norm = path_str.replace('\\', "/");
        let safe_rel = norm.trim_start_matches('/');
        if safe_rel.is_empty() || safe_rel.contains("..") {
            continue;
        }

        let out_path = extract_folder.join(safe_rel);
        if entry.is_dir() {
            let _ = fs::create_dir_all(&out_path);
        } else {
            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut outfile = File::create(&out_path)
                .map_err(|e| format!("Failed to create {}: {e}", out_path.display()))?;
            io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("Failed to extract {}: {e}", out_path.display()))?;
        }
    }

    let singer_path = match (has_common_root, common_root) {
        (true, Some(root)) => dest_base.join(root),
        _ => extract_folder,
    };

    load_singer_info(&singer_path).ok_or_else(|| {
        "No valid UTAU voicebank found in archive (missing oto.ini or character.txt)".to_string()
    })
}

/// Imports a voicebank from a folder, copying it into the singers directory.
pub fn import_voicebank_folder(
    src_folder: &Path,
    target_dir: Option<&Path>,
) -> Result<SingerInfo, String> {
    if !src_folder.is_dir() {
        return Err("Source is not a directory".into());
    }
    let dest_base = target_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_singers_dir);
    fs::create_dir_all(&dest_base).map_err(|e| e.to_string())?;

    let folder_name = src_folder.file_name().ok_or("Invalid folder name")?;
    let dest_folder = dest_base.join(folder_name);

    if dest_folder != src_folder {
        copy_dir_recursive(src_folder, &dest_folder)?;
    }

    load_singer_info(&dest_folder)
        .ok_or_else(|| "No valid UTAU voicebank found in folder".to_string())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let entries = fs::read_dir(src).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir_recursive(&p, &target)?;
        } else {
            fs::copy(&p, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Removes an installed singer folder from disk.
pub fn remove_singer(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
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

    #[test]
    fn detects_vcv_and_cv_types() {
        let mut vb_vcv = Voicebank::default();
        for alias in ["- か", "a か", "i か", "u か", "e か", "o か"] {
            vb_vcv
                .alias_index
                .insert(alias.to_string(), (Oto::default(), PathBuf::new()));
        }
        assert_eq!(vb_vcv.detect_type(), VoicebankType::VCV);

        let mut vb_cv = Voicebank::default();
        for alias in ["か", "き", "く", "け", "こ"] {
            vb_cv
                .alias_index
                .insert(alias.to_string(), (Oto::default(), PathBuf::new()));
        }
        assert_eq!(vb_cv.detect_type(), VoicebankType::CV);
    }

    #[test]
    fn loads_and_discovers_singers() {
        let temp = tempfile::tempdir().unwrap();
        let singer_dir = temp.path().join("KasaneTeto");
        fs::create_dir_all(&singer_dir).unwrap();
        fs::write(
            singer_dir.join("character.txt"),
            "name=Kasane Teto\nauthor=TwinDrill\n",
        )
        .unwrap();
        fs::write(
            singer_dir.join("oto.ini"),
            "_ka.wav=- か,50,120,-100,80,40\n",
        )
        .unwrap();

        let info = load_singer_info(&singer_dir).unwrap();
        assert_eq!(info.name, "Kasane Teto");
        assert_eq!(info.author, "TwinDrill");

        let discovered = discover_singers(&[temp.path().to_path_buf()]);
        assert!(discovered.iter().any(|s| s.name == "Kasane Teto"));
    }
}
