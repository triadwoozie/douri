//! The drum-kit library the Kit Browser shows: the kits found in the library folders, kept in
//! an index that is read from a cache at once and made again in the background.
//!
//! A library folder is laid out `<source>/<kit>/…`. A folder right under it that holds audio
//! files itself, or whose folders are named for drum sounds (Kicks, Snares…), is a kit of its
//! own; its source is then the library folder. Folders whose names start with `_` or `.` are
//! passed over. A `MANIFEST.tsv` in the library folder (a header naming `source` and `kit`,
//! and optionally `license`, `url`, `audio_or_instrument_files`) adds what it knows to the
//! kits it lists; a `BEST.md` there ranks kits: a kit is ranked by the first line that names
//! it (`source/kit`, or its source and its name in the cells or emphasis of one line).
//!
//! The index holds a line a kit (not its files: a kit's files are read when it is opened,
//! [`crate::kit_files`]), so a hundred thousand kits are a few megabytes and a search is a
//! pass over their names. Nothing in a library folder is ever written.
use crate::kit_classify::{classify_label, natural_cmp};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// One kit of the index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KitEntry {
    /// The kit's folder.
    pub path: String,
    /// The library folder it was found in.
    pub root: String,
    pub source: String,
    pub name: String,
    /// Audio files in it, when known (the manifest's count, or the count when it was opened).
    pub files: Option<u32>,
    pub license: String,
    /// Its place in `BEST.md`, from 1.
    pub best: Option<u32>,
    /// Source and name in small letters: what a search looks through.
    hay: String,
    name_low: String,
}

impl KitEntry {
    pub fn new(root: &str, path: String, source: &str, name: &str) -> Self {
        Self {
            path,
            root: root.to_string(),
            source: source.to_string(),
            name: name.to_string(),
            files: None,
            license: String::new(),
            best: None,
            hay: format!("{} {}", source.to_lowercase(), name.to_lowercase()),
            name_low: name.to_lowercase(),
        }
    }
}

/// A library folder and what the last scan found there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootInfo {
    pub path: String,
    /// The folder is there (a drive that is not mounted: false).
    pub present: bool,
    pub kits: usize,
}

/// The kits of every library folder that is there.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    pub kits: Vec<KitEntry>,
    pub roots: Vec<RootInfo>,
    /// The sources with their number of kits, sorted.
    pub sources: Vec<(String, usize)>,
}

impl Index {
    fn finish(mut self) -> Self {
        self.kits.sort_by(|a, b| {
            natural_cmp(&a.source, &b.source)
                .then_with(|| natural_cmp(&a.name, &b.name))
                .then_with(|| a.path.cmp(&b.path))
        });
        self.kits.dedup_by(|a, b| a.path == b.path);
        let mut count: Vec<(String, usize)> = Vec::new();
        for k in &self.kits {
            match count.last_mut() {
                Some(c) if c.0 == k.source => c.1 += 1,
                _ => count.push((k.source.clone(), 1)),
            }
        }
        self.sources = count;
        for r in &mut self.roots {
            r.kits = self.kits.iter().filter(|k| k.root == r.path).count();
        }
        self
    }
}

/// What a search keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub query: String,
    pub source: Option<String>,
    pub best: bool,
    pub favorites: bool,
}

/// The kits of `index` that pass `filter`, as indices into `index.kits`: every word of the
/// query has to be in the kit's source or name. Kits whose name starts with the query come
/// first; with Best on, the order is the ranking's. Kits known to hold no audio are left out.
pub fn search(index: &Index, filter: &Filter, favorites: &HashSet<String>) -> Vec<u32> {
    let q = filter.query.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    let first = words.first().copied().unwrap_or("");
    let mut out: Vec<u32> = Vec::new();
    let mut lead = 0;
    for (i, k) in index.kits.iter().enumerate() {
        if k.files == Some(0)
            || (filter.best && k.best.is_none())
            || (filter.favorites && !favorites.contains(&k.path))
            || filter.source.as_ref().is_some_and(|s| *s != k.source)
            || !words.iter().all(|w| k.hay.contains(w))
        {
            continue;
        }
        if !first.is_empty() && k.name_low.starts_with(first) {
            out.insert(lead, i as u32);
            lead += 1;
        } else {
            out.push(i as u32);
        }
    }
    if filter.best {
        out.sort_by_key(|i| index.kits[*i as usize].best.unwrap_or(u32::MAX));
    }
    out
}

// ------------------------------------------------------------------------------- settings

/// The library folders and the favourite kits, kept in `kit-library/settings.json` of the
/// settings folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub folders: Vec<String>,
    pub favorites: HashSet<String>,
}

/// The library folders of a new installation.
#[allow(clippy::vec_init_then_push)]
pub fn default_folders() -> Vec<String> {
    #[allow(unused_mut)]
    let mut folders = Vec::new();
    folders
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            folders: default_folders(),
            favorites: HashSet::new(),
        }
    }
}

/// `kit-library` in the settings folder.
pub fn data_dir() -> Option<PathBuf> {
    crate::recent_files::config_dir().map(|d| d.join("kit-library"))
}

impl Settings {
    pub fn from_json(text: &str) -> Self {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let list = |k: &str| -> Option<Vec<String>> {
            Some(
                v[k].as_array()?
                    .iter()
                    .filter_map(|s| s.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string)
                    .collect(),
            )
        };
        let mut folders = list("folders").unwrap_or_else(default_folders);
        let mut seen = HashSet::new();
        folders.retain(|f| seen.insert(f.clone()));
        Self {
            folders,
            favorites: list("favorites").unwrap_or_default().into_iter().collect(),
        }
    }

    pub fn to_json(&self) -> String {
        let mut fav: Vec<&String> = self.favorites.iter().collect();
        fav.sort();
        serde_json::to_string_pretty(&json!({ "folders": self.folders, "favorites": fav }))
            .unwrap_or_default()
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("settings.json"))
            .map(|t| Self::from_json(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("settings.json"), self.to_json())
    }
}

// ------------------------------------------------------------------------------- the scan

/// A manifest's rows: (source, kit) to (audio files, license).
type Manifest = HashMap<(String, String), (Option<u32>, String)>;

/// Reads a `MANIFEST.tsv`: a header line, then a row a kit.
pub fn parse_manifest(text: &str) -> Manifest {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Manifest::new();
    };
    let cols: Vec<&str> = header.split('\t').map(str::trim).collect();
    let col = |name: &str| cols.iter().position(|c| *c == name);
    let (Some(c_source), Some(c_kit)) = (col("source"), col("kit")) else {
        return Manifest::new();
    };
    let c_audio = col("audio_or_instrument_files").or_else(|| col("file_count"));
    let c_license = col("license");
    let mut out = Manifest::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let get = |c: usize| f.get(c).map(|s| s.trim()).unwrap_or("");
        if get(c_source).is_empty() || get(c_kit).is_empty() {
            continue;
        }
        out.insert(
            (get(c_source).to_string(), get(c_kit).to_string()),
            (
                c_audio.and_then(|c| get(c).parse::<u32>().ok()),
                c_license.map(get).unwrap_or("").to_string(),
            ),
        );
    }
    out
}

/// The pieces of a `BEST.md` line that can name a kit or a source: table cells, link texts,
/// emphasised and quoted text, and the line itself without its list mark.
fn line_pieces(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let clean = |s: &str| {
        s.trim()
            .trim_matches(|c: char| c == '*' || c == '_' || c == '`' || c == '"')
            .trim()
            .to_lowercase()
    };
    let body = line.trim().trim_start_matches(|c: char| {
        c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | '+' | '#' | ' ')
    });
    for cell in body.split('|') {
        out.push(clean(cell));
        // "Source / Kit", "Source: Kit", "Source — Kit".
        for sep in [" / ", ": ", " — ", " – ", " - "] {
            if let Some((a, b)) = cell.split_once(sep) {
                out.push(clean(a));
                out.push(clean(b));
            }
        }
    }
    for cell in line.split('|') {
        // [text](link): the text.
        let mut rest = cell;
        while let Some(a) = rest.find('[') {
            let Some(b) = rest[a..].find(']') else { break };
            out.push(clean(&rest[a + 1..a + b]));
            rest = &rest[a + b + 1..];
        }
        for mark in ["**", "`", "\""] {
            let parts: Vec<&str> = cell.split(mark).collect();
            for p in parts.iter().skip(1).step_by(2) {
                out.push(clean(p));
            }
        }
    }
    out.retain(|p| !p.is_empty());
    out
}

/// Ranks the kits a `BEST.md` names: the first line naming a kit gives its place (1 for the
/// first kit named). A line names a kit by `source/kit`, or by the kit's name with its source
/// on the same line, or by the name alone when one kit only has that name.
pub fn parse_best(text: &str, kits: &[(String, String)]) -> HashMap<usize, u32> {
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (source, name)) in kits.iter().enumerate() {
        by_path.insert(
            format!("{}/{}", source.to_lowercase(), name.to_lowercase()),
            i,
        );
        by_name.entry(name.to_lowercase()).or_default().push(i);
    }
    let mut out: HashMap<usize, u32> = HashMap::new();
    let mut rank = 0u32;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let pieces = line_pieces(line);
        let mut hit: Option<usize> = None;
        for p in &pieces {
            if let Some(i) = by_path.get(p.trim_end_matches('/')) {
                hit = Some(*i);
                break;
            }
        }
        if hit.is_none() {
            for p in &pieces {
                let Some(list) = by_name.get(p) else { continue };
                let with_source = list
                    .iter()
                    .find(|i| pieces.iter().any(|q| *q == kits[**i].0.to_lowercase()));
                hit = with_source
                    .copied()
                    .or_else(|| (list.len() == 1).then(|| list[0]));
                if hit.is_some() {
                    break;
                }
            }
        }
        if let Some(i) = hit {
            if let std::collections::hash_map::Entry::Vacant(e) = out.entry(i) {
                rank += 1;
                e.insert(rank);
            }
        }
    }
    out
}

fn skipped(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('.') || name == "__MACOSX"
}

/// The folders and whether there are audio or kit-definition files right in `dir`.
fn list_dir(dir: &Path) -> (Vec<String>, bool) {
    let mut dirs = Vec::new();
    let mut audio = false;
    if let Ok(read) = std::fs::read_dir(dir) {
        for e in read.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if skipped(&name) {
                continue;
            }
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                dirs.push(name);
            } else if !audio {
                let low = name.to_ascii_lowercase();
                audio = crate::import::is_audio_path(Path::new(&name))
                    || low.ends_with(".sfz")
                    || low == "drumkit.xml";
            }
        }
    }
    (dirs, audio)
}

/// Whether the folders of a folder make it one kit: most are named for drum sounds or for
/// what sample packs call their parts.
fn folders_of_one_kit(dirs: &[String]) -> bool {
    let known = dirs
        .iter()
        .filter(|d| {
            classify_label(d).is_some()
                || matches!(
                    d.to_lowercase().as_str(),
                    "samples"
                        | "wav"
                        | "wavs"
                        | "one shots"
                        | "one-shots"
                        | "oneshots"
                        | "drums"
                        | "hits"
                        | "audio"
                        | "midi"
                        | "presets"
                )
        })
        .count();
    known > 0 && known * 2 >= dirs.len()
}

fn join(root: &str, rel: &str) -> String {
    Path::new(root).join(rel).to_string_lossy().into_owned()
}

/// The kits of one library folder, or `None` when the folder is not there.
pub fn scan_root(root: &str) -> Option<Vec<KitEntry>> {
    let dir = Path::new(root);
    if !dir.is_dir() {
        return None;
    }
    let manifest = std::fs::read_to_string(dir.join("MANIFEST.tsv"))
        .map(|t| parse_manifest(&t))
        .unwrap_or_default();
    let manifest_sources: HashSet<&str> = manifest.keys().map(|k| k.0.as_str()).collect();
    let own = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string());
    let mut kits: Vec<KitEntry> = Vec::new();
    let (tops, audio_here) = list_dir(dir);
    if audio_here {
        kits.push(KitEntry::new(root, root.to_string(), &own, &own));
    }
    for top in tops {
        let (subs, audio) = list_dir(&dir.join(&top));
        let listed = manifest_sources.contains(top.as_str());
        if !listed && (audio || subs.is_empty() || folders_of_one_kit(&subs)) {
            kits.push(KitEntry::new(root, join(root, &top), &own, &top));
            continue;
        }
        for sub in subs {
            let mut k = KitEntry::new(root, join(root, &format!("{top}/{sub}")), &top, &sub);
            if let Some((files, license)) = manifest.get(&(top.clone(), sub)) {
                k.files = *files;
                k.license = license.clone();
            }
            kits.push(k);
        }
    }
    if let Ok(best) = std::fs::read_to_string(dir.join("BEST.md")) {
        let names: Vec<(String, String)> = kits
            .iter()
            .map(|k| (k.source.clone(), k.name.clone()))
            .collect();
        for (i, rank) in parse_best(&best, &names) {
            kits[i].best = Some(rank);
        }
    }
    Some(kits)
}

// ------------------------------------------------------------------------------- the cache

/// The cache: for each library folder ever scanned, its kits. A folder that is not there now
/// keeps its lines, so the list is back at once when its drive is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cache {
    pub roots: HashMap<String, Vec<KitEntry>>,
}

const CACHE_VERSION: u64 = 1;

impl Cache {
    pub fn to_json(&self) -> String {
        let mut roots = serde_json::Map::new();
        for (root, kits) in &self.roots {
            let rows: Vec<Value> = kits
                .iter()
                .map(|k| {
                    json!([
                        k.path.strip_prefix(root.as_str()).unwrap_or(&k.path),
                        k.source,
                        k.name,
                        k.files,
                        k.license,
                        k.best
                    ])
                })
                .collect();
            roots.insert(root.clone(), Value::Array(rows));
        }
        json!({ "version": CACHE_VERSION, "roots": roots }).to_string()
    }

    pub fn from_json(text: &str) -> Self {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        if v["version"].as_u64() != Some(CACHE_VERSION) {
            return Self::default();
        }
        let mut out = Self::default();
        let Some(roots) = v["roots"].as_object() else {
            return out;
        };
        for (root, rows) in roots {
            let kits = rows
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|r| {
                            let tail = r[0].as_str()?;
                            let mut k = KitEntry::new(
                                root,
                                format!("{root}{tail}"),
                                r[1].as_str()?,
                                r[2].as_str()?,
                            );
                            k.files = r[3].as_u64().map(|n| n as u32);
                            k.license = r[4].as_str().unwrap_or("").to_string();
                            k.best = r[5].as_u64().map(|n| n as u32);
                            Some(k)
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.roots.insert(root.clone(), kits);
        }
        out
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("index.json"))
            .map(|t| Self::from_json(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("index.json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(tmp, dir.join("index.json"))
    }

    /// The index of `folders` from the cache alone: the folders that are there now.
    pub fn index(&self, folders: &[String]) -> Index {
        let mut index = Index::default();
        for f in folders {
            let present = Path::new(f).is_dir();
            if present {
                if let Some(kits) = self.roots.get(f) {
                    index.kits.extend(kits.iter().cloned());
                }
            }
            index.roots.push(RootInfo {
                path: f.clone(),
                present,
                kits: 0,
            });
        }
        index.finish()
    }
}

/// Scans `folders`. A count of audio files found by opening a kit (it is in `cache`) is kept
/// where the manifest has none. Returns the index and the cache to write.
pub fn scan(folders: &[String], mut cache: Cache) -> (Index, Cache) {
    let mut index = Index::default();
    for f in folders {
        let found = scan_root(f);
        if let Some(mut kits) = found.clone() {
            if let Some(old) = cache.roots.get(f) {
                let counts: HashMap<&str, u32> = old
                    .iter()
                    .filter_map(|k| Some((k.path.as_str(), k.files?)))
                    .collect();
                for k in kits.iter_mut().filter(|k| k.files.is_none()) {
                    k.files = counts.get(k.path.as_str()).copied();
                }
            }
            index.kits.extend(kits.iter().cloned());
            cache.roots.insert(f.clone(), kits);
        }
        index.roots.push(RootInfo {
            path: f.clone(),
            present: found.is_some(),
            kits: 0,
        });
    }
    (index.finish(), cache)
}

// ------------------------------------------------------------------------------- the worker

/// What the background scan sends.
pub enum ScanMsg {
    /// The index from the cache (the first thing sent).
    Cached(Index),
    /// The index from the folders as they are, and how long the scan took.
    Scanned(Index, std::time::Duration),
}

/// Starts a scan of `folders` on its own thread: the cached index first, then the scanned one
/// (which is also written to the cache in `dir`).
pub fn spawn_scan(folders: Vec<String>, dir: Option<PathBuf>) -> Receiver<ScanMsg> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("kit-library-scan".into())
        .spawn(move || {
            let cache = dir.as_deref().map(Cache::load).unwrap_or_default();
            if tx.send(ScanMsg::Cached(cache.index(&folders))).is_err() {
                return;
            }
            let started = std::time::Instant::now();
            let (index, cache) = scan(&folders, cache);
            let took = started.elapsed();
            if let Some(dir) = &dir {
                if let Err(e) = cache.save(dir) {
                    tracing::warn!("could not write the kit index: {e}");
                }
            }
            let _ = tx.send(ScanMsg::Scanned(index, took));
        });
    if let Err(e) = spawned {
        tracing::warn!("could not start the kit scan: {e}");
    }
    rx
}

/// Notes in the cache in `dir` that the kit at `path` (in library folder `root`) holds
/// `files` audio files. Background thread.
pub fn note_count(dir: &Path, root: &str, path: &str, files: u32) {
    let mut cache = Cache::load(dir);
    let Some(k) = cache
        .roots
        .get_mut(root)
        .and_then(|kits| kits.iter_mut().find(|k| k.path == path))
    else {
        return;
    };
    if k.files == Some(files) {
        return;
    }
    k.files = Some(files);
    if let Err(e) = cache.save(dir) {
        tracing::warn!("could not write the kit index: {e}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("prowave-kitlib-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// A library laid out as the real one is.
    fn library(name: &str) -> PathBuf {
        let dir = temp(name);
        touch(
            &dir,
            "MANIFEST.tsv",
            "source\tkit\turl\tlicense\tfile_count\tbytes\tdate\taudio_or_instrument_files\n\
             kb6\tAkai MPC2000\thttps://x\tCC BY-NC-ND 4.0\t44\t1722199\t2026-11-23\t42\n\
             kb6\tAcetone Rhythm-Ace\thttps://x\tCC BY-NC-ND 4.0\t18\t175113\t2026-11-23\t16\n\
             MusicRadar\t808-samples\thttps://y\tRoyalty free\t400\t9\t2026-11-23\t398\n",
        );
        touch(
            &dir,
            "kb6/Akai MPC2000/[KB6]_Akai_MPC2000/808_HH__CL.wav",
            "",
        );
        touch(&dir, "kb6/Acetone Rhythm-Ace/a/kick.wav", "");
        touch(
            &dir,
            "MusicRadar/808-samples/x/Hits/Bass Drum [BD]/E808_BD-01.wav",
            "",
        );
        // A kit the manifest does not list yet, and working folders.
        touch(&dir, "MusicRadar/909-samples/Kick.wav", "");
        touch(&dir, "_incoming/Junk/thing/a.wav", "");
        touch(&dir, "_work/dk.py", "");
        touch(&dir, "PROGRESS.txt", "");
        dir
    }

    #[test]
    fn a_library_folder_is_scanned_into_sources_and_kits() {
        let dir = library("scan");
        let root = dir.to_string_lossy().into_owned();
        let (index, cache) = scan(std::slice::from_ref(&root), Cache::default());
        let got: Vec<(&str, &str, Option<u32>)> = index
            .kits
            .iter()
            .map(|k| (k.source.as_str(), k.name.as_str(), k.files))
            .collect();
        assert_eq!(
            got,
            [
                ("kb6", "Acetone Rhythm-Ace", Some(16)),
                ("kb6", "Akai MPC2000", Some(42)),
                ("MusicRadar", "808-samples", Some(398)),
                ("MusicRadar", "909-samples", None),
            ]
        );
        assert_eq!(index.kits[1].license, "CC BY-NC-ND 4.0");
        assert_eq!(index.kits[1].path, format!("{root}/kb6/Akai MPC2000"));
        assert_eq!(
            index.sources,
            [("kb6".to_string(), 2), ("MusicRadar".to_string(), 2)]
        );
        assert_eq!(
            index.roots,
            [RootInfo {
                path: root.clone(),
                present: true,
                kits: 4
            }]
        );
        // The cache gives the same index back.
        let again = Cache::from_json(&cache.to_json());
        assert_eq!(again, cache);
        assert_eq!(again.index(std::slice::from_ref(&root)), index);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_folder_is_no_error_and_the_others_are_shown() {
        let dir = library("missing");
        let root = dir.to_string_lossy().into_owned();
        let gone = "/Volumes/NOT-MOUNTED-HERE/DRUM-KITS".to_string();
        let folders = vec![gone.clone(), root.clone()];
        let (index, cache) = scan(&folders, Cache::default());
        assert_eq!(index.kits.len(), 4);
        assert_eq!(
            index.roots[0],
            RootInfo {
                path: gone.clone(),
                present: false,
                kits: 0
            }
        );
        assert!(index.roots[1].present);
        // A drive that goes away: its kits leave the index and stay in the cache.
        std::fs::rename(&dir, dir.with_extension("away")).unwrap();
        let (index2, cache2) = scan(&folders, cache.clone());
        assert!(index2.kits.is_empty());
        assert_eq!(cache2.roots[&root].len(), 4);
        assert!(cache2.index(&folders).kits.is_empty());
        std::fs::rename(dir.with_extension("away"), &dir).unwrap();
        assert_eq!(cache2.index(&folders).kits.len(), 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_without_a_manifest_tells_kits_from_sources() {
        let dir = temp("plain");
        // A pack with its sounds in folders: one kit.
        touch(&dir, "OZ Pack/Kicks/k.wav", "");
        touch(&dir, "OZ Pack/Snares/s.wav", "");
        touch(&dir, "OZ Pack/Hi Hats/h.wav", "");
        // A kit with its files right in it.
        touch(&dir, "Lone Kit/kick.wav", "");
        // A maker with two kits.
        touch(&dir, "Maker/Kit A/kick.wav", "");
        touch(&dir, "Maker/Kit B/sub/kick.wav", "");
        // Loose files in the library folder itself.
        touch(&dir, "clap.wav", "");
        let root = dir.to_string_lossy().into_owned();
        let own = dir.file_name().unwrap().to_string_lossy().into_owned();
        let kits = scan_root(&root).unwrap();
        let mut got: Vec<(String, String)> = kits
            .iter()
            .map(|k| (k.source.clone(), k.name.clone()))
            .collect();
        got.sort();
        let mut want = vec![
            (own.clone(), own.clone()),
            (own.clone(), "Lone Kit".to_string()),
            (own.clone(), "OZ Pack".to_string()),
            ("Maker".to_string(), "Kit A".to_string()),
            ("Maker".to_string(), "Kit B".to_string()),
        ];
        want.sort();
        assert_eq!(got, want);
        assert!(scan_root("/no/such/folder/anywhere").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn sample_index() -> Index {
        let mut kits = Vec::new();
        for (s, n) in [
            ("kb6", "Roland TR-808"),
            ("kb6", "Roland TR-909"),
            ("kb6", "Akai MPC2000"),
            ("MusicRadar", "808-samples"),
            ("MusicRadar", "Empty Thing"),
            ("Boxed-Ear", "CR-78"),
        ] {
            kits.push(KitEntry::new("/lib", format!("/lib/{s}/{n}"), s, n));
        }
        kits[4].files = Some(0);
        kits[5].best = Some(1);
        kits[0].best = Some(2);
        Index {
            kits,
            roots: Vec::new(),
            sources: Vec::new(),
        }
        .finish()
    }

    #[test]
    fn a_search_matches_every_word_in_names_and_sources() {
        let index = sample_index();
        let names = |f: &Filter, fav: &HashSet<String>| -> Vec<String> {
            search(&index, f, fav)
                .iter()
                .map(|i| index.kits[*i as usize].name.clone())
                .collect()
        };
        let none = HashSet::new();
        let q = |s: &str| Filter {
            query: s.into(),
            ..Filter::default()
        };
        // Everything, in the index's order, without the kit known to be empty.
        assert_eq!(
            names(&Filter::default(), &none),
            [
                "CR-78",
                "Akai MPC2000",
                "Roland TR-808",
                "Roland TR-909",
                "808-samples"
            ]
        );
        // A name that starts with the query comes first.
        assert_eq!(names(&q("808"), &none), ["808-samples", "Roland TR-808"]);
        assert_eq!(names(&q("ROLAND kb6 9"), &none), ["Roland TR-909"]);
        assert_eq!(names(&q("musicradar"), &none), ["808-samples"]);
        assert!(names(&q("zzz"), &none).is_empty());
        let by_source = Filter {
            source: Some("kb6".into()),
            ..Filter::default()
        };
        assert_eq!(names(&by_source, &none).len(), 3);
        let best = Filter {
            best: true,
            ..Filter::default()
        };
        assert_eq!(names(&best, &none), ["CR-78", "Roland TR-808"]);
        let fav: HashSet<String> = ["/lib/kb6/Akai MPC2000".to_string()].into();
        let favs = Filter {
            favorites: true,
            ..Filter::default()
        };
        assert_eq!(names(&favs, &fav), ["Akai MPC2000"]);
    }

    #[test]
    fn a_search_over_a_hundred_thousand_kits_is_quick() {
        let mut kits = Vec::new();
        for i in 0..100_000 {
            let s = format!("Source {}", i % 300);
            let n = format!("Drum Kit Number {i} Deluxe");
            kits.push(KitEntry::new("/lib", format!("/lib/{s}/{n}"), &s, &n));
        }
        let index = Index {
            kits,
            roots: Vec::new(),
            sources: Vec::new(),
        }
        .finish();
        let none = HashSet::new();
        let started = std::time::Instant::now();
        let hits = search(
            &index,
            &Filter {
                query: "kit 4242 source".into(),
                ..Filter::default()
            },
            &none,
        );
        let took = started.elapsed();
        assert_eq!(hits.len(), 20, "4242, 14242 to 94242, 42420 to 42429");
        // Debug builds on a busy machine: far above what it takes, far below a stall.
        assert!(took.as_millis() < 500, "{took:?}");
        let cache = Cache {
            roots: [("/lib".to_string(), index.kits.clone())].into(),
        };
        let text = cache.to_json();
        assert_eq!(Cache::from_json(&text).roots["/lib"].len(), 100_000);
    }

    #[test]
    fn best_md_ranks_the_kits_it_names() {
        let kits: Vec<(String, String)> = [
            ("kb6", "Roland TR-808"),
            ("kb6", "Akai MPC2000"),
            ("MusicRadar", "808-samples"),
            ("Boxed-Ear", "CR-78"),
            ("Other", "CR-78"),
        ]
        .iter()
        .map(|(s, n)| (s.to_string(), n.to_string()))
        .collect();
        let text = "# Best kits\n\nRanked shortlist.\n\n\
            | # | Source | Kit | Why |\n|---|---|---|---|\n\
            | 1 | Boxed-Ear | CR-78 | clean |\n\
            2. **Akai MPC2000** (kb6): punchy\n\
            - `MusicRadar/808-samples/` big\n\
            - kb6 / Roland TR-808\n\
            - [Akai MPC2000](https://x) again\n\
            - Nothing Known\n";
        let got = parse_best(text, &kits);
        assert_eq!(
            got.get(&3),
            Some(&1),
            "the table row, told from Other's CR-78"
        );
        assert_eq!(got.get(&1), Some(&2));
        assert_eq!(got.get(&2), Some(&3));
        assert_eq!(got.get(&0), Some(&4));
        assert_eq!(got.len(), 4);
        // A name two sources share, with no source on the line, ranks nothing.
        assert!(parse_best("- CR-78\n", &kits).is_empty());
    }

    #[test]
    fn settings_and_the_manifest_read_what_they_are_given() {
        let s = Settings::from_json(r#"{"folders":["/a","/b","/a"," "],"favorites":["/a/k"]}"#);
        assert_eq!(s.folders, ["/a", "/b"]);
        assert!(s.favorites.contains("/a/k"));
        assert_eq!(Settings::from_json(&s.to_json()), s);
        assert_eq!(Settings::from_json("not json"), Settings::default());
        // No folders is a choice, kept as it is.
        assert!(Settings::from_json(r#"{"folders":[]}"#).folders.is_empty());
        let dir = temp("settings");
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        assert_eq!(Settings::load(&dir.join("nothing")), Settings::default());
        assert!(parse_manifest("").is_empty());
        assert!(
            parse_manifest("a\tb\nc\td\n").is_empty(),
            "no source and kit columns"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_count_found_by_opening_a_kit_is_kept_through_a_scan() {
        let lib = library("count");
        let root = lib.to_string_lossy().into_owned();
        let data = temp("count-data");
        let folders = vec![root.clone()];
        let rx = spawn_scan(folders.clone(), Some(data.clone()));
        let mut scanned = None;
        while let Ok(m) = rx.recv() {
            if let ScanMsg::Scanned(i, _) = m {
                scanned = Some(i);
            }
        }
        let index = scanned.unwrap();
        let path = format!("{root}/MusicRadar/909-samples");
        assert_eq!(
            index.kits.iter().find(|k| k.path == path).unwrap().files,
            None
        );
        note_count(&data, &root, &path, 1);
        let (index, _) = scan(&folders, Cache::load(&data));
        assert_eq!(
            index.kits.iter().find(|k| k.path == path).unwrap().files,
            Some(1)
        );
        let _ = std::fs::remove_dir_all(lib);
        let _ = std::fs::remove_dir_all(data);
    }
}
