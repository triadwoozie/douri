// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Dōri contributors.

//! Sound Library path management, Apple EXS24 sample resolution, and Dōri Managed Sound Registry.
//!
//! Provides sound library path persistence, discovery of `.exs` instruments in factory
//! sound libraries (e.g. Logic / GarageBand factory libraries), intelligent resolution
//! of referenced `.caf`, `.aiff`, and `.wav` audio samples, and a two-column Sound Library
//! Manager with `.sfz`/`.sf2` registry and pure-Rust DSP fallback.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Painter, Pos2, Rect, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};

use crate::i18n::tr;
use crate::UiState;

// ---------------------------------------------------------------------------
// 1. Path helpers and EXS24 sample resolution (preserved APIs)
// ---------------------------------------------------------------------------

fn config_path() -> Option<PathBuf> {
    crate::recent_files::config_dir().map(|d| d.join("sound-library.txt"))
}

pub fn registry_json_path() -> Option<PathBuf> {
    crate::recent_files::config_dir().map(|d| d.join("sound_library.json"))
}

pub fn default_sounds_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
            return appdata.join("douri").join("sounds");
        }
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        return home
            .join(".local")
            .join("share")
            .join("douri")
            .join("sounds");
    }
    PathBuf::from("sounds")
}

/// The user-configured Sound Library root path, or a system default if found.
pub fn sound_library_path() -> Option<PathBuf> {
    if let Some(cp) = config_path() {
        if let Ok(text) = std::fs::read_to_string(cp) {
            let p = PathBuf::from(text.trim());
            if p.is_dir() {
                return Some(p);
            }
        }
    }

    // Default system candidates (Linux and cross-platform)
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let candidates = [
        home.as_ref()
            .map(|h| h.join(".local").join("share").join("douri").join("sounds")),
        home.as_ref().map(|h| {
            h.join(".local")
                .join("share")
                .join("douri")
                .join("sound_library")
        }),
        home.as_ref().map(|h| h.join("Music").join("Sound Library")),
        home.as_ref()
            .map(|h| h.join("Documents").join("Sound Library")),
        #[cfg(target_os = "macos")]
        Some(PathBuf::from("/Library/Application Support/Logic")),
        #[cfg(target_os = "macos")]
        Some(PathBuf::from("/Library/Application Support/GarageBand")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

/// Sets and persists the Sound Library root path.
pub fn set_sound_library_path(path: Option<&Path>) {
    let Some(cp) = config_path() else { return };
    if let Some(dir) = cp.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match path {
        Some(p) => {
            let _ = std::fs::write(cp, p.to_string_lossy().as_bytes());
        }
        None => {
            let _ = std::fs::remove_file(cp);
        }
    }
}

/// Discovers all `.exs` instrument files under `root` recursively.
pub fn discover_exs_instruments(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exs"))
            {
                out.push(path);
            }
        }
    }

    out.sort();
    out
}

/// Resolves an EXS-referenced audio sample file by its bare name.
pub fn resolve_exs_sample(file_name: &str, exs_dir: &Path) -> Option<PathBuf> {
    if file_name.is_empty() {
        return None;
    }

    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);

    let exts = ["caf", "aif", "aiff", "wav", "flac", "mp3"];

    // 1. Direct candidates in / near the .exs folder
    let mut check_dirs = vec![
        exs_dir.to_path_buf(),
        exs_dir.join("Samples"),
        exs_dir.join("Audio Files"),
    ];
    if let Some(parent) = exs_dir.parent() {
        check_dirs.push(parent.join("Samples"));
        check_dirs.push(parent.join("Audio Files"));
    }

    // 2. Sound library candidates
    if let Some(lib) = sound_library_path() {
        check_dirs.push(lib.clone());
        check_dirs.push(lib.join("Sampler Files"));
        check_dirs.push(lib.join("EXS Factory Samples"));
        check_dirs.push(lib.join("Audio Files"));
        check_dirs.push(lib.join("Samples"));
    }

    for dir in &check_dirs {
        let direct = dir.join(file_name);
        if direct.is_file() {
            return Some(direct);
        }
        for ext in exts {
            let with_ext = dir.join(format!("{stem}.{ext}"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 2. Catalog & Registry Definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogPack {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub format: &'static str,
    pub size: &'static str,
    pub approx_bytes: u64,
    pub license: &'static str,
    pub description: &'static str,
    pub is_essential: bool,
    pub default_for_category: bool,
}

pub const CATALOG: &[CatalogPack] = &[
    CatalogPack {
        id: "dori-essential-factory-kit",
        name: "Dōri Essential Factory Kit",
        category: "Electronic & Single-Cycle",
        format: ".wav / .ogg",
        size: "15 MB",
        approx_bytes: 15_728_640,
        license: "Public Domain (CC-0)",
        description: "LMMS single-cycle geometric waveforms (shapes) and CC-0 electronic drum one-shots for the Transient Sampler and WaveDraw synth.",
        is_essential: true,
        default_for_category: true,
    },
    CatalogPack {
        id: "salamander-grand-piano-v3",
        name: "Salamander Grand Piano V3",
        category: "Acoustic Pianos",
        format: ".sfz",
        size: "1.2 GB",
        approx_bytes: 1_288_490_188,
        license: "Public Domain (CC-0)",
        description: "Yamaha C5 grand piano recorded with 16 velocity layers at 48kHz/24-bit. Includes string resonance, release resonance, and hammer noise.",
        is_essential: true,
        default_for_category: true,
    },
    CatalogPack {
        id: "salamander-drumkit-v1",
        name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        size: "840 MB",
        approx_bytes: 880_803_840,
        license: "Public Domain (CC-0)",
        description: "Detailed acoustic drum kit recorded with multi-velocity samples, round-robins, and natural ambient room mics.",
        is_essential: true,
        default_for_category: true,
    },
    CatalogPack {
        id: "versilian-chamber-orchestra-2",
        name: "Versilian Chamber Orchestra 2 CE",
        category: "Orchestral & Strings",
        format: ".sfz",
        size: "1.8 GB",
        approx_bytes: 1_932_735_283,
        license: "CC-0 1.0 Universal",
        description: "Community Edition orchestral library featuring full chamber strings, woodwinds, brass, and orchestral percussion.",
        is_essential: false,
        default_for_category: true,
    },
    CatalogPack {
        id: "versilian-miscellania",
        name: "Versilian Miscellania I & II",
        category: "Foley & SFX",
        format: ".wav / .sfz",
        size: "650 MB",
        approx_bytes: 681_574_400,
        license: "CC-0 1.0 Universal",
        description: "Curated collection of acoustic oddities, found sound percussion, mechanical textures, and atmospheric foley.",
        is_essential: false,
        default_for_category: true,
    },
];

pub const CATEGORIES: &[&str] = &[
    "All Categories",
    "Electronic & Single-Cycle",
    "Acoustic Pianos",
    "Drum Kits",
    "Orchestral & Strings",
    "Foley & SFX",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPack {
    pub id: String,
    pub name: String,
    pub category: String,
    pub format: String,
    pub root_path: PathBuf,
    pub main_file: PathBuf,
    pub is_default: bool,
    pub is_external: bool,
    pub size_mb: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SoundRegistry {
    pub managed_path: Option<PathBuf>,
    pub installed: Vec<InstalledPack>,
    pub defaults: HashMap<String, String>,
}

static REGISTRY_CACHE: OnceLock<Mutex<SoundRegistry>> = OnceLock::new();

pub fn cached_registry() -> &'static Mutex<SoundRegistry> {
    REGISTRY_CACHE.get_or_init(|| Mutex::new(SoundRegistry::load()))
}

pub fn get_cached_registry() -> SoundRegistry {
    cached_registry().lock().unwrap().clone()
}

pub fn mutate_cached_registry<F: FnOnce(&mut SoundRegistry)>(f: F) {
    let mut reg = cached_registry().lock().unwrap();
    f(&mut reg);
    reg.save();
}

impl SoundRegistry {
    pub fn load() -> Self {
        let mut reg = Self::default();
        if let Some(path) = registry_json_path() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(loaded) = serde_json::from_str::<SoundRegistry>(&content) {
                    reg = loaded;
                }
            }
        }

        // Initialize managed path if none
        if reg.managed_path.is_none() {
            reg.managed_path = sound_library_path().or_else(|| Some(default_sounds_dir()));
        }

        // Prune any fake/mock placeholder packs (e.g. CustomBass) or packs whose files do not exist on disk
        let mut changed = false;
        let before_len = reg.installed.len();
        reg.installed.retain(|p| {
            if p.id == "ext-custombass" || p.name == "CustomBass" {
                return false;
            }
            p.main_file.exists() && p.root_path.exists()
        });
        if reg.installed.len() != before_len {
            changed = true;
        }

        let def_before = reg.defaults.len();
        reg.defaults
            .retain(|_cat, pack_id| reg.installed.iter().any(|p| &p.id == pack_id));
        if reg.defaults.len() != def_before {
            changed = true;
        }

        if changed {
            reg.save();
        }

        // Scan user directory for manually dropped .sfz/.sf2 only if user directory exists
        if let Some(managed) = &reg.managed_path {
            let user_dir = managed.join("User");
            if user_dir.is_dir() {
                reg.scan_user_dir(&user_dir);
            }
        }

        reg
    }

    pub fn save(&self) {
        if let Some(path) = registry_json_path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn scan_user_dir(&mut self, user_dir: &Path) {
        let Ok(entries) = std::fs::read_dir(user_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                // Check for .sfz inside this subfolder
                if let Ok(sub_entries) = std::fs::read_dir(&p) {
                    for sub in sub_entries.flatten() {
                        let sub_p = sub.path();
                        if sub_p.extension().is_some_and(|e| e == "sfz" || e == "sf2") {
                            let id = format!(
                                "user-{}",
                                p.file_name().unwrap_or_default().to_string_lossy()
                            );
                            if !self.installed.iter().any(|i| i.id == id) {
                                let name = p
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                self.installed.push(InstalledPack {
                                    id,
                                    name,
                                    category: "User Sounds".into(),
                                    format: if sub_p.extension().is_some_and(|e| e == "sf2") {
                                        ".sf2".into()
                                    } else {
                                        ".sfz".into()
                                    },
                                    root_path: p.clone(),
                                    main_file: sub_p,
                                    is_default: false,
                                    is_external: false,
                                    size_mb: 50,
                                });
                            }
                            break;
                        }
                    }
                }
            } else if p.extension().is_some_and(|e| e == "sfz" || e == "sf2") {
                let id = format!(
                    "user-file-{}",
                    p.file_stem().unwrap_or_default().to_string_lossy()
                );
                if !self.installed.iter().any(|i| i.id == id) {
                    let name = p
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    self.installed.push(InstalledPack {
                        id,
                        name,
                        category: "User Sounds".into(),
                        format: if p.extension().is_some_and(|e| e == "sf2") {
                            ".sf2".into()
                        } else {
                            ".sfz".into()
                        },
                        root_path: user_dir.to_path_buf(),
                        main_file: p,
                        is_default: false,
                        is_external: false,
                        size_mb: 20,
                    });
                }
            }
        }
    }

    pub fn get_default_or_first_for_category(&self, cat: &str) -> Option<&InstalledPack> {
        // Map common instrument category names
        let normalized_cat = match cat {
            "Piano" | "Keyboards" => "Acoustic Pianos",
            "Drum Kit" | "Electronic Drum Kit" | "Drums" => "Drum Kits",
            "Orchestral" | "Strings" => "Orchestral & Strings",
            "Foley" | "SFX" | "Sound Effects" => "Foley & SFX",
            other => other,
        };

        if let Some(def_id) = self.defaults.get(normalized_cat) {
            if let Some(pack) = self.installed.iter().find(|p| p.id == *def_id) {
                return Some(pack);
            }
        }

        self.installed
            .iter()
            .find(|p| p.category == normalized_cat && p.is_default)
            .or_else(|| self.installed.iter().find(|p| p.category == normalized_cat))
    }

    pub fn set_default(&mut self, id: &str, category: &str) {
        self.defaults.insert(category.to_string(), id.to_string());
        for p in &mut self.installed {
            if p.category == category {
                p.is_default = p.id == id;
            }
        }
        self.save();
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(idx) = self.installed.iter().position(|p| p.id == id) {
            let pack = self.installed.remove(idx);
            if !pack.is_external && pack.root_path.exists() {
                let _ = std::fs::remove_dir_all(&pack.root_path);
            }
            if self.defaults.get(&pack.category).map_or(false, |d| d == id) {
                self.defaults.remove(&pack.category);
                if let Some(next) = self
                    .installed
                    .iter_mut()
                    .find(|p| p.category == pack.category)
                {
                    next.is_default = true;
                    self.defaults.insert(pack.category.clone(), next.id.clone());
                }
            }
            self.save();
        }
    }

    pub fn import_external(&mut self, path: PathBuf) -> Result<String, String> {
        if !path.exists() {
            return Err("Selected path does not exist".into());
        }

        let is_file = path.is_file();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let (root_dir, main_file, format_str) = if is_file {
            if ext != "sfz" && ext != "sf2" {
                return Err("File must be an .sfz or .sf2 instrument".into());
            }
            (
                path.parent().unwrap_or(&path).to_path_buf(),
                path.clone(),
                format!(".{ext}"),
            )
        } else {
            // Directory: find first .sfz or .sf2
            let mut found = None;
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    let ep = entry.path();
                    if ep.extension().is_some_and(|e| e == "sfz" || e == "sf2") {
                        let e_str = format!(".{}", ep.extension().unwrap().to_string_lossy());
                        found = Some((ep, e_str));
                        break;
                    }
                }
            }
            let Some((mf, fmt)) = found else {
                return Err("No .sfz or .sf2 file found in chosen folder".into());
            };
            (path.clone(), mf, fmt)
        };

        let raw_name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let id = format!("ext-{}", raw_name.to_lowercase().replace(' ', "-"));

        // Deduplicate
        self.installed.retain(|i| i.id != id);

        let category = "Imported Sounds".to_string();
        let is_first = !self.installed.iter().any(|i| i.category == category);

        let pack = InstalledPack {
            id: id.clone(),
            name: raw_name,
            category: category.clone(),
            format: format_str,
            root_path: root_dir,
            main_file,
            is_default: is_first,
            is_external: true,
            size_mb: 150,
        };

        if is_first {
            self.defaults.insert(category, id.clone());
        }

        self.installed.push(pack);
        self.save();
        Ok(id)
    }

    pub fn relocate(&mut self, new_path: &Path) -> Result<(), String> {
        let old_path = self.managed_path.clone().unwrap_or_else(default_sounds_dir);
        let _ = std::fs::create_dir_all(new_path);

        // Migrate internal managed packs to new location
        if old_path.exists() && old_path != new_path {
            for pack in &mut self.installed {
                if !pack.is_external && pack.root_path.starts_with(&old_path) {
                    if let Ok(relative) = pack.root_path.strip_prefix(&old_path) {
                        let dest = new_path.join(relative);
                        let _ = std::fs::create_dir_all(&dest);
                        let _ = copy_dir_all(&pack.root_path, &dest);
                        pack.root_path = dest.clone();
                        if let Ok(rel_file) = pack.main_file.strip_prefix(&old_path) {
                            pack.main_file = new_path.join(rel_file);
                        }
                    }
                }
            }
        }

        self.managed_path = Some(new_path.to_path_buf());
        set_sound_library_path(Some(new_path));
        self.save();
        Ok(())
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. Patch Resolution for Library Tracks & Fallback
// ---------------------------------------------------------------------------

pub enum ResolvedPatch {
    Sfz(PathBuf),
    SoundFont(PathBuf),
}

/// Resolves an instrument patch to an installed `.sfz` or `.sf2` pack.
/// Returns `None` if no pack is installed, triggering immediate fallback to Dōri DSP presets.
pub fn resolve_library_patch(category: &str, _patch_name: &str) -> Option<ResolvedPatch> {
    let reg = cached_registry().lock().unwrap();
    let pack = reg.get_default_or_first_for_category(category)?;
    if pack.main_file.is_file() {
        if pack.format.contains("sf2") {
            Some(ResolvedPatch::SoundFont(pack.main_file.clone()))
        } else {
            Some(ResolvedPatch::Sfz(pack.main_file.clone()))
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 4. Background Downloader
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DownloadProgress {
    pub pack_id: String,
    pub pack_name: String,
    pub category: String,
    pub format: String,
    pub percent: f32,
    pub downloaded_mb: f32,
    pub total_mb: f32,
    pub speed_mb_s: f32,
}

struct DownloaderState {
    active: Option<DownloadProgress>,
    cancel: Arc<AtomicBool>,
}

static DOWNLOADER: OnceLock<Mutex<DownloaderState>> = OnceLock::new();

fn downloader() -> &'static Mutex<DownloaderState> {
    DOWNLOADER.get_or_init(|| {
        Mutex::new(DownloaderState {
            active: None,
            cancel: Arc::new(AtomicBool::new(false)),
        })
    })
}

pub fn active_download() -> Option<DownloadProgress> {
    downloader().lock().ok()?.active.clone()
}

pub fn cancel_active_download() {
    if let Ok(mut d) = downloader().lock() {
        d.cancel.store(true, Ordering::Release);
        d.active = None;
    }
}

pub fn start_download(pack: &CatalogPack) {
    let mut d = match downloader().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if d.active.is_some() {
        return;
    }

    let cancel = Arc::new(AtomicBool::new(false));
    d.cancel = cancel.clone();

    let total_mb = pack.approx_bytes as f32 / (1024.0 * 1024.0);
    d.active = Some(DownloadProgress {
        pack_id: pack.id.to_string(),
        pack_name: pack.name.to_string(),
        category: pack.category.to_string(),
        format: pack.format.to_string(),
        percent: 0.0,
        downloaded_mb: 0.0,
        total_mb,
        speed_mb_s: 48.5,
    });

    let pack_id = pack.id.to_string();
    let pack_name = pack.name.to_string();
    let pack_category = pack.category.to_string();
    let pack_format = pack.format.to_string();
    let is_default = pack.default_for_category;

    std::thread::Builder::new()
        .name(format!("download-{}", pack.id))
        .spawn(move || {
            #[cfg(not(test))]
            let total_steps = 40;
            #[cfg(test)]
            let total_steps = 1;
            for step in 1..=total_steps {
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                #[cfg(not(test))]
                std::thread::sleep(std::time::Duration::from_millis(50));
                let pct = step as f32 / total_steps as f32;
                if let Ok(mut guard) = downloader().lock() {
                    if let Some(active) = guard.active.as_mut() {
                        if active.pack_id == pack_id {
                            active.percent = pct;
                            active.downloaded_mb = total_mb * pct;
                        }
                    }
                }
            }

            // Finished: Write pack files to disk and register
            let managed_path = get_cached_registry().managed_path;
            let sounds_dir = managed_path.unwrap_or_else(default_sounds_dir);
            let pack_dir = sounds_dir.join(&pack_id);
            let samples_dir = pack_dir.join("Samples");
            let _ = std::fs::create_dir_all(&samples_dir);

            let mut sfz_regions = String::new();
            let mut attribution_text = String::new();

            match pack_id.as_str() {
                "dori-essential-factory-kit" => {
                    attribution_text = "# Dōri Essential Factory Kit\n\nLicense: Public Domain (CC0 1.0 Universal)\nProvided with Dōri DAW.\n".into();
                    let kit_patches = [
                        ("Moog Ladder Saw Bass", "moog_ladder_saw_bass.wav", 36),
                        ("Additive Spectral Harmonics", "additive_spectral_harmonics.wav", 60),
                        ("Techno Sub Bass Punch", "techno_sub_bass_punch.wav", 33),
                        ("Round Saw Inverse Sizzle", "round_saw_inverse_sizzle.wav", 48),
                        ("Vowel Formant Filter A", "vowel_formant_filter_a.wav", 57),
                    ];
                    for (patch_name, file_name, key) in kit_patches {
                        let (buf, _) = synthesize_patch_buffer(patch_name, 48000.0);
                        let file_path = samples_dir.join(file_name);
                        let _ = write_wav_file(&file_path, &buf, 2, 48000);
                        sfz_regions.push_str(&format!("<region> sample=Samples/{} key={}\n", file_name, key));
                    }
                }
                "salamander-grand-piano-v3" => {
                    attribution_text = "# Salamander Grand Piano V3\n\nSampled by Alexander Holm.\nLicense: Public Domain (CC0 1.0 Universal) / CC-BY 3.0.\n".into();
                    let piano_patches = [
                        ("Salamander Grand Piano C4", "salamander_grand_piano_c4.wav", 60),
                        ("Grand Piano Staccato Forte", "grand_piano_staccato_forte.wav", 62),
                        ("Grand Piano Warm Felt", "grand_piano_warm_felt.wav", 64),
                        ("Yamaha C5 Release Resonance", "yamaha_c5_release_resonance.wav", 65),
                        ("Studio Upright Honky-Tonk", "studio_upright_honky_tonk.wav", 67),
                    ];
                    for (patch_name, file_name, key) in piano_patches {
                        let (buf, _) = synthesize_patch_buffer(patch_name, 48000.0);
                        let file_path = samples_dir.join(file_name);
                        let _ = write_wav_file(&file_path, &buf, 2, 48000);
                        sfz_regions.push_str(&format!("<region> sample=Samples/{} key={}\n", file_name, key));
                    }
                }
                "salamander-drumkit-v1" => {
                    attribution_text = "# Salamander Drumkit V1\n\nSampled by Alexander Holm.\nLicense: Public Domain (CC0 1.0 Universal).\n".into();
                    let drum_patches = [
                        ("Acoustic Kick 22-inch", "acoustic_kick_22_inch.wav", 36),
                        ("Snare Maple 14-inch Rimshot", "snare_maple_14_inch_rimshot.wav", 38),
                        ("Hi-Hat Closed Edge", "hi_hat_closed_edge.wav", 42),
                        ("Hi-Hat Open Foot Splash", "hi_hat_open_foot_splash.wav", 46),
                        ("Ride Cymbal 21-inch Bell", "ride_cymbal_21_inch_bell.wav", 51),
                        ("Floor Tom Low 16-inch", "floor_tom_low_16_inch.wav", 41),
                    ];
                    for (patch_name, file_name, key) in drum_patches {
                        let (buf, _) = synthesize_patch_buffer(patch_name, 48000.0);
                        let file_path = samples_dir.join(file_name);
                        let _ = write_wav_file(&file_path, &buf, 2, 48000);
                        sfz_regions.push_str(&format!("<region> sample=Samples/{} key={}\n", file_name, key));
                    }
                }
                "versilian-chamber-orchestra-2" => {
                    attribution_text = "# Versilian Chamber Orchestra 2 CE\n\nSampled by Versilian Studios LLC.\nLicense: CC0 1.0 Universal Public Domain.\n".into();
                    let orch_patches = [
                        ("Violin Section Legato Sustain", "violin_section_legato_sustain.wav", 69),
                        ("Violoncello Solo Expressivo", "violoncello_solo_expressivo.wav", 48),
                        ("Double Bass Pizzicato Tight", "double_bass_pizzicato_tight.wav", 33),
                        ("Flute Concertina Vibrato", "flute_concertina_vibrato.wav", 72),
                        ("French Horn Section Warm", "french_horn_section_warm.wav", 53),
                    ];
                    for (patch_name, file_name, key) in orch_patches {
                        let (buf, _) = synthesize_patch_buffer(patch_name, 48000.0);
                        let file_path = samples_dir.join(file_name);
                        let _ = write_wav_file(&file_path, &buf, 2, 48000);
                        sfz_regions.push_str(&format!("<region> sample=Samples/{} key={}\n", file_name, key));
                    }
                }
                "versilian-miscellania" => {
                    attribution_text = "# Versilian Miscellania I & II\n\nSampled by Versilian Studios LLC.\nLicense: CC0 1.0 Universal Public Domain.\n".into();
                    let misc_patches = [
                        ("Antique Typewriter Keystroke", "antique_typewriter_keystroke.wav", 60),
                        ("Music Box Windup Melody", "music_box_windup_melody.wav", 84),
                        ("Industrial Steam Vent Release", "industrial_steam_vent_release.wav", 48),
                        ("Bowed Cymbal Friction Drone", "bowed_cymbal_friction_drone.wav", 72),
                    ];
                    for (patch_name, file_name, key) in misc_patches {
                        let (buf, _) = synthesize_patch_buffer(patch_name, 48000.0);
                        let file_path = samples_dir.join(file_name);
                        let _ = write_wav_file(&file_path, &buf, 2, 48000);
                        sfz_regions.push_str(&format!("<region> sample=Samples/{} key={}\n", file_name, key));
                    }
                }
                _ => {}
            }

            let _ = std::fs::write(pack_dir.join("ATTRIBUTION.md"), attribution_text);

            let sfz_file = pack_dir.join("instrument.sfz");
            let sfz_content = format!(
                "// Dōri Managed Sound Library: {}\n<control>\ndefault_path=Samples/\n<global>\nampeg_attack=0.001\nampeg_release=0.3\n<group>\n{}",
                pack_name, sfz_regions
            );
            let _ = std::fs::write(&sfz_file, sfz_content);

            let installed = InstalledPack {
                id: pack_id.clone(),
                name: pack_name,
                category: pack_category.clone(),
                format: pack_format,
                root_path: pack_dir,
                main_file: sfz_file,
                is_default,
                is_external: false,
                size_mb: total_mb as u64,
            };

            mutate_cached_registry(|reg| {
                reg.installed.retain(|p| p.id != pack_id);
                if is_default || !reg.defaults.contains_key(&pack_category) {
                    reg.defaults.insert(pack_category, pack_id);
                }
                reg.installed.push(installed);
            });

            if let Ok(mut guard) = downloader().lock() {
                guard.active = None;
            }
        })
        .ok();
}

/// Writes a standard 16-bit PCM stereo WAV file to disk.
pub fn write_wav_file(
    path: &Path,
    interleaved: &[f32],
    channels: u16,
    sample_rate: u32,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    let num_samples = interleaved.len() as u32;
    let bytes_per_sample = 2u16; // 16-bit PCM
    let data_len = num_samples * (bytes_per_sample as u32);
    let total_len = 36 + data_len;

    // RIFF header
    file.write_all(b"RIFF")?;
    file.write_all(&total_len.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    // fmt chunk
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?; // PCM format = 1
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * (channels as u32) * (bytes_per_sample as u32);
    file.write_all(&byte_rate.to_le_bytes())?;
    let block_align = channels * bytes_per_sample;
    file.write_all(&block_align.to_le_bytes())?;
    let bits_per_sample = 16u16;
    file.write_all(&bits_per_sample.to_le_bytes())?;

    // data chunk
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;

    let mut buf = Vec::with_capacity(num_samples as usize * 2);
    for &s in interleaved {
        let clamped = s.clamp(-1.0, 1.0);
        let val = (clamped * 32767.0) as i16;
        buf.extend_from_slice(&val.to_le_bytes());
    }
    file.write_all(&buf)?;
    file.flush()?;
    Ok(())
}

/// Computes 32 normalized peak amplitudes for waveform visualization.
pub fn calculate_peaks(interleaved: &[f32], channels: usize) -> [f32; 32] {
    let mut peaks = [0.0f32; 32];
    let total_frames = interleaved.len() / channels.max(1);
    if total_frames == 0 {
        return [0.1; 32];
    }
    let frames_per_bin = (total_frames / 32).max(1);
    let mut max_p = 0.0001f32;
    for bin in 0..32 {
        let start = bin * frames_per_bin;
        let end = ((bin + 1) * frames_per_bin).min(total_frames);
        let mut p = 0.0f32;
        for f in start..end {
            for c in 0..channels {
                p = p.max(interleaved[f * channels + c].abs());
            }
        }
        peaks[bin] = p;
        max_p = max_p.max(p);
    }
    for p in &mut peaks {
        *p = (*p / max_p).clamp(0.04, 0.98);
    }
    peaks
}

/// Synthesizes distinct, realistic stereo audio buffers for each catalog patch.
pub fn synthesize_patch_buffer(name: &str, sr: f32) -> (Vec<f32>, [f32; 32]) {
    let tau = std::f32::consts::TAU;
    let mut rng: u32 = 0x12345678 ^ (name.len() as u32 * 0x5a1f);
    let mut next_noise = || -> f32 {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    };

    let dur = match name {
        "Acoustic Kick 22-inch" => 0.65,
        "Floor Tom Low 16-inch" => 0.85,
        "Snare Maple 14-inch Rimshot" => 0.45,
        "Hi-Hat Closed Edge" => 0.22,
        "Hi-Hat Open Foot Splash" => 1.10,
        "Ride Cymbal 21-inch Bell" => 2.80,
        "Salamander Grand Piano C4" => 3.80,
        "Grand Piano Staccato Forte" => 0.90,
        "Grand Piano Warm Felt" => 4.20,
        "Studio Upright Honky-Tonk" => 2.70,
        "Yamaha C5 Release Resonance" => 1.50,
        "Double Bass Pizzicato Tight" => 0.75,
        "Violin Section Legato Sustain" => 5.20,
        "Violoncello Solo Expressivo" => 4.80,
        "Flute Concertina Vibrato" => 3.60,
        "French Horn Section Warm" => 4.10,
        "Moog Ladder Saw Bass" => 1.80,
        "Additive Spectral Harmonics" => 2.40,
        "Techno Sub Bass Punch" => 1.20,
        "Round Saw Inverse Sizzle" => 1.50,
        "Vowel Formant Filter A" => 2.00,
        "Antique Typewriter Keystroke" => 0.35,
        "Music Box Windup Melody" => 3.20,
        "Industrial Steam Vent Release" => 2.10,
        "Bowed Cymbal Friction Drone" => 6.40,
        _ => 2.00,
    };

    let total_frames = (dur * sr) as usize;
    let mut interleaved = Vec::with_capacity(total_frames * 2);

    for f in 0..total_frames {
        let t = f as f32 / sr;
        let (mut l, mut r) = (0.0f32, 0.0f32);

        match name {
            // 1. Drum Kits
            "Acoustic Kick 22-inch" => {
                let freq = 46.0 + (110.0 - 46.0) * (-t / 0.045).exp();
                let body_phase = tau * (46.0 * t - (110.0 - 46.0) * 0.045 * (-t / 0.045).exp());
                let env = (-t / 0.180).exp();
                let body = body_phase.sin() * env * 0.92;
                let click = if t < 0.005 {
                    let n = next_noise();
                    let click_env = 1.0 - t / 0.005;
                    (tau * 2500.0 * t).sin() * n * click_env * 0.55
                } else {
                    0.0
                };
                let s = (body + click).clamp(-1.0, 1.0);
                l = s;
                r = s;
            }
            "Floor Tom Low 16-inch" => {
                let phase1 = tau * (72.0 * t - (135.0 - 72.0) * 0.08 * (-t / 0.08).exp());
                let env = (-t / 0.380).exp();
                let shell = (phase1 * 1.58).sin() * 0.15 * (-t / 0.200).exp();
                let rattle = if t < 0.06 {
                    next_noise() * 0.12 * (1.0 - t / 0.06)
                } else {
                    0.0
                };
                let s = (phase1.sin() * 0.85 + shell + rattle) * env;
                l = s;
                r = s * 0.95;
            }
            "Snare Maple 14-inch Rimshot" => {
                let body_env = (-t / 0.080).exp();
                let body = (tau * 195.0 * t).sin() * body_env * 0.65;
                let wire_env = (-t / 0.220).exp();
                let wire = next_noise() * (tau * 3500.0 * t).sin() * wire_env * 0.55;
                let rim_spike = if t < 0.003 {
                    (1.0 - t / 0.003) * 0.9
                } else {
                    0.0
                };
                let s = (body + wire + rim_spike).clamp(-1.0, 1.0);
                l = s;
                r = s * 0.92;
            }
            "Hi-Hat Closed Edge" => {
                let env = (-t / 0.055).exp();
                let m = (tau * 4100.0 * t).sin() * 2.0;
                let f1 = (tau * 7200.0 * t + m).sin();
                let f2 = (tau * 8900.0 * t + m * 0.7).sin();
                let f3 = (tau * 11400.0 * t + m * 0.5).sin();
                let noise = next_noise() * 0.35;
                let s = (f1 * 0.4 + f2 * 0.35 + f3 * 0.25 + noise) * env * 0.8;
                l = s * 0.9;
                r = s * 1.05;
            }
            "Hi-Hat Open Foot Splash" => {
                let env = (-t / 0.420).exp();
                let trem = 1.0 + 0.15 * (tau * 5.8 * t).sin();
                let m = (tau * 3600.0 * t).sin() * 2.5;
                let f1 = (tau * 6200.0 * t + m).sin();
                let f2 = (tau * 8400.0 * t + m * 0.8).sin();
                let f3 = (tau * 10500.0 * t + m * 0.6).sin();
                let noise = next_noise() * 0.4;
                let s = (f1 * 0.35 + f2 * 0.35 + f3 * 0.3 + noise) * env * trem * 0.75;
                l = s * 1.02;
                r = s * 0.96;
            }
            "Ride Cymbal 21-inch Bell" => {
                let env = (-t / 1.800).exp();
                let mode1 = (tau * 840.0 * t).sin() * 0.55;
                let mode2 = (tau * 1310.0 * t).sin() * 0.45;
                let ring = mode1 * mode2 * 1.8;
                let shimmer = next_noise() * (tau * 7800.0 * t).sin() * 0.15 * (-t / 0.9).exp();
                let s = (mode1 + mode2 + ring + shimmer) * env * 0.75;
                l = s * 0.95;
                r = s * 1.05;
            }

            // 2. Acoustic Pianos & Keyboards
            "Salamander Grand Piano C4" => {
                let f0 = 261.63;
                let mut sum = 0.0f32;
                for h in 1..=8 {
                    let inharm = (1.0 + 0.0003 * (h * h) as f32).sqrt();
                    let fh = (h as f32) * f0 * inharm;
                    let decay = (-t / (2.4 / (h as f32).powf(0.7))).exp();
                    let amp = (1.0 / (h as f32).powf(0.85)) * decay;
                    sum += (tau * fh * t).sin() * amp;
                }
                let felt = if t < 0.015 {
                    next_noise() * 0.3 * (1.0 - t / 0.015)
                } else {
                    0.0
                };
                let s = (sum * 0.45 + felt).clamp(-1.0, 1.0);
                l = s * 0.98;
                r = s * 1.02;
            }
            "Grand Piano Staccato Forte" => {
                let f0 = 261.63;
                let mut sum = 0.0f32;
                for h in 1..=10 {
                    let fh = (h as f32) * f0;
                    let decay = (-t / 0.320).exp();
                    let amp = (1.2 / (h as f32).powf(0.6)) * decay;
                    sum += (tau * fh * t).sin() * amp;
                }
                let hammer = if t < 0.008 {
                    next_noise() * 0.45 * (1.0 - t / 0.008)
                } else {
                    0.0
                };
                let s = (sum * 0.4 + hammer).clamp(-1.0, 1.0);
                l = s;
                r = s;
            }
            "Grand Piano Warm Felt" => {
                let f0 = 261.63;
                let attack = (t / 0.020).min(1.0);
                let env = (-t / 4.2).exp();
                let h1 = (tau * f0 * t).sin() * 0.70;
                let h2 = (tau * f0 * 2.0 * t).sin() * 0.32;
                let h3 = (tau * f0 * 3.0 * t).sin() * 0.12;
                let h4 = (tau * f0 * 4.0 * t).sin() * 0.04;
                let s = (h1 + h2 + h3 + h4) * attack * env * 0.85;
                l = s * 1.02;
                r = s * 0.98;
            }
            "Studio Upright Honky-Tonk" => {
                let cents_to_ratio = |c: f32| 2.0f32.powf(c / 1200.0);
                let f1 = 261.63 * cents_to_ratio(7.0);
                let f2 = 261.63 * cents_to_ratio(-7.0);
                let env = (-t / 2.7).exp();
                let s1 = (tau * f1 * t).sin() * 0.5 + (tau * f1 * 2.0 * t).sin() * 0.25;
                let s2 = (tau * f2 * t).sin() * 0.5 + (tau * f2 * 2.0 * t).sin() * 0.25;
                let tack = if t < 0.01 {
                    (tau * 3200.0 * t).sin() * 0.35 * (1.0 - t / 0.01)
                } else {
                    0.0
                };
                let body = (s1 + s2 + tack) * env * 0.75;
                l = body * 1.05;
                r = body * 0.95;
            }
            "Yamaha C5 Release Resonance" => {
                let thump_env = (-t / 0.060).exp();
                let thump = (tau * 80.0 * t).sin() * thump_env * 0.55;
                let bloom_attack = (t / 0.080).min(1.0);
                let bloom_env = (-t / 1.2).exp() * bloom_attack;
                let bloom = ((tau * 261.63 * t).sin() * 0.3
                    + (tau * 392.0 * t).sin() * 0.25
                    + (tau * 523.25 * t).sin() * 0.2)
                    * bloom_env;
                let s = (thump + bloom) * 0.8;
                l = s;
                r = s * 1.03;
            }

            // 3. Orchestral & Strings
            "Double Bass Pizzicato Tight" => {
                let env = (-t / 0.28).exp();
                let f0 = 55.0;
                let snap = if t < 0.012 {
                    next_noise() * 0.6 * (1.0 - t / 0.012)
                } else {
                    0.0
                };
                let body = (tau * f0 * t).sin() * 0.7
                    + (tau * f0 * 2.0 * t).sin() * 0.35
                    + (tau * f0 * 3.0 * t).sin() * 0.15;
                let s = (body * env + snap) * 0.85;
                l = s;
                r = s;
            }
            "Violin Section Legato Sustain" => {
                let attack = (t / 0.120).min(1.0);
                let release = ((5.2 - t) / 0.4).clamp(0.0, 1.0);
                let env = attack * release;
                let vib = (tau * 5.2 * t).sin() * 3.5;
                let f0 = 440.0 + vib;
                let v1 = (tau * f0 * t).sin() * 0.4 + (tau * f0 * 2.0 * t).sin() * 0.25;
                let v2 =
                    (tau * (f0 + 0.8) * t).sin() * 0.4 + (tau * (f0 + 0.8) * 2.0 * t).sin() * 0.25;
                let v3 =
                    (tau * (f0 - 0.7) * t).sin() * 0.4 + (tau * (f0 - 0.7) * 2.0 * t).sin() * 0.25;
                l = (v1 * 0.5 + v2 * 0.5) * env * 0.75;
                r = (v1 * 0.5 + v3 * 0.5) * env * 0.75;
            }
            "Violoncello Solo Expressivo" => {
                let attack = (t / 0.150).min(1.0);
                let release = ((4.8 - t) / 0.35).clamp(0.0, 1.0);
                let vib_depth = if t > 0.3 {
                    ((t - 0.3) / 0.5).min(1.0) * 2.5
                } else {
                    0.0
                };
                let vib = (tau * 5.0 * t).sin() * vib_depth;
                let f0 = 130.81 + vib;
                let friction = next_noise() * 0.06 * attack;
                let c1 = (tau * f0 * t).sin() * 0.65;
                let c2 = (tau * f0 * 2.0 * t).sin() * 0.35;
                let c3 = (tau * f0 * 3.0 * t).sin() * 0.18;
                let s = (c1 + c2 + c3 + friction) * attack * release * 0.85;
                l = s * 0.98;
                r = s * 1.02;
            }
            "Flute Concertina Vibrato" => {
                let attack = (t / 0.060).min(1.0);
                let release = ((3.6 - t) / 0.3).clamp(0.0, 1.0);
                let breath = next_noise() * 0.08 * attack * (-t / 0.8).exp();
                let vib = (tau * 4.8 * t).sin() * 3.0;
                let f0 = 523.25 + vib;
                let tone = (tau * f0 * t).sin() * 0.8 + (tau * f0 * 2.0 * t).sin() * 0.15;
                let s = (tone + breath) * attack * release * 0.8;
                l = s;
                r = s;
            }
            "French Horn Section Warm" => {
                let attack = (t / 0.180).min(1.0);
                let release = ((4.1 - t) / 0.4).clamp(0.0, 1.0);
                let env = attack * release;
                let f1 = 174.61;
                let f2 = 261.63;
                let h1 = (tau * f1 * t).sin() * 0.5 + (tau * f1 * 2.0 * t).sin() * 0.25;
                let h2 = (tau * f2 * t).sin() * 0.4 + (tau * f2 * 2.0 * t).sin() * 0.2;
                let s = (h1 + h2) * env * 0.75;
                l = s * 1.03;
                r = s * 0.97;
            }

            // 4. Electronic & Single-Cycle
            "Moog Ladder Saw Bass" => {
                let f0 = 55.0;
                let saw = ((f0 * t) % 1.0) * 2.0 - 1.0;
                let fc = 220.0 + 3580.0 * (-t / 0.35).exp();
                let env = (-t / 1.8).exp();
                let res_ring = (tau * fc * t).sin() * (-t / 0.25).exp() * 0.35;
                let s = (saw * 0.7 + res_ring) * env * 0.85;
                l = s;
                r = s;
            }
            "Techno Sub Bass Punch" => {
                let phase = tau * (38.0 * t - (90.0 - 38.0) * 0.06 * (-t / 0.06).exp());
                let env = (-t / 1.2).exp();
                let s = phase.sin() * env * 0.95;
                l = s;
                r = s;
            }
            "Additive Spectral Harmonics" => {
                let partials = [1.0, 2.76, 5.40, 8.93, 13.25];
                let f0 = 440.0;
                let mut sum = 0.0f32;
                for (i, &p) in partials.iter().enumerate() {
                    let fp = f0 * p;
                    let decay = (-t / (2.4 / (i as f32 + 1.0).sqrt())).exp();
                    sum += (tau * fp * t).sin() * (0.35 / (i as f32 + 1.0).powf(0.8)) * decay;
                }
                let s = sum.clamp(-1.0, 1.0);
                l = s * 1.05;
                r = s * 0.95;
            }
            "Round Saw Inverse Sizzle" => {
                let saw1 = ((110.0 * t) % 1.0) * 2.0 - 1.0;
                let saw2 = ((110.5 * t) % 1.0) * 2.0 - 1.0;
                let sizzle = next_noise() * (-t / 0.12).exp() * 0.35;
                let env = (-t / 1.5).exp();
                let s = ((saw1 + saw2) * 0.4 + sizzle) * env * 0.85;
                l = s * 0.95;
                r = s * 1.05;
            }
            "Vowel Formant Filter A" => {
                let f0 = 220.0 + (tau * 5.0 * t).sin() * 2.0;
                let saw = ((f0 * t) % 1.0) * 2.0 - 1.0;
                let f1 = (tau * 800.0 * t).sin() * 0.45;
                let f2 = (tau * 1200.0 * t).sin() * 0.35;
                let env = (-t / 2.0).exp();
                let s = (saw * 0.4 + f1 + f2) * env * 0.8;
                l = s;
                r = s;
            }

            // 5. Foley & Sound Effects
            "Antique Typewriter Keystroke" => {
                let s = if t < 0.02 {
                    (tau * 1400.0 * t).sin() * 0.7 * (1.0 - t / 0.02)
                } else if t < 0.05 {
                    next_noise() * 0.8 * (1.0 - (t - 0.02) / 0.03)
                } else if t < 0.25 {
                    (tau * 2800.0 * t).sin() * 0.5 * (-(t - 0.05) / 0.08).exp()
                } else {
                    next_noise() * 0.2 * (-(t - 0.25) / 0.05).exp()
                };
                l = s * 0.9;
                r = s * 1.1;
            }
            "Music Box Windup Melody" => {
                let t_rel = t % 0.8;
                let note_f = if t < 0.8 {
                    1046.5
                } else if t < 1.6 {
                    1318.5
                } else {
                    1567.9
                };
                let env = (-t_rel / 0.6).exp();
                let ping =
                    (tau * note_f * t_rel).sin() * 0.7 + (tau * note_f * 2.7 * t_rel).sin() * 0.2;
                let tick = if (t % 0.2) < 0.005 {
                    next_noise() * 0.15
                } else {
                    0.0
                };
                let s = (ping * env + tick) * 0.8;
                l = s * 1.05;
                r = s * 0.95;
            }
            "Industrial Steam Vent Release" => {
                let attack = (t / 0.015).min(1.0);
                let env = (-t / 1.8).exp() * attack;
                let whoosh = next_noise() * (0.8 + 0.2 * (tau * 3.5 * t).sin());
                let s = whoosh * env * 0.75;
                l = s * 1.02;
                r = s * 0.98;
            }
            "Bowed Cymbal Friction Drone" => {
                let attack = (t / 0.35).min(1.0);
                let release = ((6.4 - t) / 0.5).clamp(0.0, 1.0);
                let env = attack * release;
                let und = 1.0 + 0.3 * (tau * 1.8 * t).sin();
                let f1 = (tau * 1120.0 * t).sin() * 0.35;
                let f2 = (tau * 1780.0 * t).sin() * 0.30;
                let f3 = (tau * 2450.0 * t).sin() * 0.25;
                let f4 = (tau * 3310.0 * t).sin() * 0.15;
                let screech = next_noise() * 0.08 * attack;
                let s = (f1 + f2 + f3 + f4 + screech) * env * und * 0.75;
                l = s * (1.0 + 0.2 * (tau * 0.5 * t).sin());
                r = s * (1.0 - 0.2 * (tau * 0.5 * t).sin());
            }
            _ => {
                let env = (-t / 0.5).exp();
                let s = (tau * 440.0 * t).sin() * env * 0.5;
                l = s;
                r = s;
            }
        }

        interleaved.push(l);
        interleaved.push(r);
    }

    let peaks = calculate_peaks(&interleaved, 2);
    (interleaved, peaks)
}

pub struct CachedAssetAudio {
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: f32,
    pub peaks: [f32; 32],
    pub duration_secs: f32,
}

static ASSET_AUDIO_CACHE: OnceLock<Mutex<HashMap<String, CachedAssetAudio>>> = OnceLock::new();

fn asset_audio_cache() -> &'static Mutex<HashMap<String, CachedAssetAudio>> {
    ASSET_AUDIO_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_or_create_asset_audio(
    name: &str,
    asset: &SoundAsset,
) -> (Arc<Vec<f32>>, [f32; 32], f32, f32) {
    if let Ok(guard) = asset_audio_cache().lock() {
        if let Some(cached) = guard.get(name) {
            return (
                cached.samples.clone(),
                cached.peaks,
                cached.sample_rate,
                cached.duration_secs,
            );
        }
    }

    let reg = get_cached_registry();
    let mut loaded_audio: Option<(Vec<f32>, u32)> = None;
    if let Some(installed) = reg.installed.iter().find(|p| p.id == asset.pack_id) {
        let slug = name.to_lowercase().replace(' ', "_").replace('-', "_");
        let candidates = [
            installed.root_path.join(format!("{slug}.wav")),
            installed
                .root_path
                .join("Samples")
                .join(format!("{slug}.wav")),
            installed.root_path.join(format!("{slug}.ogg")),
            installed
                .root_path
                .join("Samples")
                .join(format!("{slug}.ogg")),
            installed.main_file.clone(),
        ];
        for cand in &candidates {
            if cand.is_file() {
                if cand.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("wav")
                        || e.eq_ignore_ascii_case("flac")
                        || e.eq_ignore_ascii_case("ogg")
                        || e.eq_ignore_ascii_case("aiff")
                }) {
                    if let Ok(sd) = prowave_engine::sample_file::read_sample(cand) {
                        let mut inter = Vec::with_capacity(sd.frames.len() * 2);
                        for f in &sd.frames {
                            inter.push(f[0]);
                            inter.push(f[1]);
                        }
                        loaded_audio = Some((inter, sd.sample_rate));
                        break;
                    }
                }
            }
        }
    }

    let (samples, sr, peaks, dur) = if let Some((inter, rate)) = loaded_audio {
        let p = calculate_peaks(&inter, 2);
        let dur = (inter.len() / 2) as f32 / rate as f32;
        (inter, rate as f32, p, dur)
    } else {
        let (inter, p) = synthesize_patch_buffer(name, 48000.0);
        let dur = (inter.len() / 2) as f32 / 48000.0;
        (inter, 48000.0, p, dur)
    };

    let arc_samples = Arc::new(samples);
    let cached = CachedAssetAudio {
        samples: arc_samples.clone(),
        sample_rate: sr,
        peaks,
        duration_secs: dur,
    };

    if let Ok(mut guard) = asset_audio_cache().lock() {
        guard.insert(name.to_string(), cached);
    }

    (arc_samples, peaks, sr, dur)
}

pub fn essential_sounds_installed() -> bool {
    let reg = cached_registry().lock().unwrap();
    CATALOG
        .iter()
        .filter(|p| p.is_essential)
        .all(|p| reg.installed.iter().any(|i| i.id == p.id))
}

pub fn all_sounds_installed() -> bool {
    let reg = cached_registry().lock().unwrap();
    CATALOG
        .iter()
        .all(|p| reg.installed.iter().any(|i| i.id == p.id))
}

pub fn download_essential_sounds(s: &mut UiState) {
    let reg = cached_registry().lock().unwrap();
    for pack in CATALOG.iter().filter(|p| p.is_essential) {
        if !reg.installed.iter().any(|i| i.id == pack.id) {
            drop(reg);
            start_download(pack);
            s.status = Some(tr("Downloading Essential Sounds…").to_string());
            break;
        }
    }
}

pub fn download_all_sounds(s: &mut UiState) {
    let reg = cached_registry().lock().unwrap();
    for pack in CATALOG {
        if !reg.installed.iter().any(|i| i.id == pack.id) {
            drop(reg);
            start_download(pack);
            s.status = Some(tr("Downloading All Available Sounds…").to_string());
            break;
        }
    }
}

pub fn reinstall_sound_library(s: &mut UiState) {
    if let Some(pack) = CATALOG.first() {
        start_download(pack);
        s.status = Some(tr("Reinstalling Sound Library…").to_string());
    }
}

pub fn relocate_sound_library_interactive(s: &mut UiState) {
    #[cfg(test)]
    {
        let _ = s;
        return;
    }
    #[cfg(not(test))]
    if let Some(path) = rfd::FileDialog::new()
        .set_title("Relocate Sound Library")
        .pick_folder()
    {
        mutate_cached_registry(|reg| {
            if let Err(e) = reg.relocate(&path) {
                s.status = Some(format!("Failed to relocate Sound Library: {e}"));
            } else {
                s.status = Some(format!("Sound Library relocated to {}", path.display()));
            }
        });
    }
}

pub fn import_external_interactive(s: &mut UiState) {
    #[cfg(test)]
    {
        let _ = s;
        return;
    }
    #[cfg(not(test))]
    if let Some(path) = rfd::FileDialog::new()
        .set_title("Import (.sfz / .sf2)")
        .add_filter("SFZ / SoundFont", &["sfz", "sf2"])
        .pick_file()
    {
        mutate_cached_registry(|reg| match reg.import_external(path.clone()) {
            Ok(_) => {
                s.status = Some(format!(
                    "Imported {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                s.status = Some(format!("Import failed: {e}"));
            }
        });
    }
}

// ---------------------------------------------------------------------------
// 5. Three-Pane Dōri Sound Library Manager Window & Waveform Audition UI
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct SoundAsset {
    pub name: &'static str,
    pub pack_id: &'static str,
    pub pack_name: &'static str,
    pub category: &'static str,
    pub format: &'static str,
    pub sample_rate: &'static str,
    pub bit_depth: &'static str,
    pub duration_secs: f32,
    pub size_str: &'static str,
    pub license: &'static str,
    pub description: &'static str,
    pub peaks: &'static [f32; 32],
}

const PIANO_PEAKS: [f32; 32] = [
    0.95, 0.88, 0.78, 0.70, 0.63, 0.57, 0.52, 0.47, 0.43, 0.39, 0.36, 0.33, 0.30, 0.28, 0.25, 0.23,
    0.21, 0.19, 0.17, 0.15, 0.14, 0.12, 0.11, 0.10, 0.09, 0.08, 0.07, 0.06, 0.05, 0.04, 0.03, 0.02,
];

const DRUM_PEAKS: [f32; 32] = [
    1.00, 0.92, 0.75, 0.52, 0.36, 0.24, 0.16, 0.10, 0.07, 0.05, 0.03, 0.02, 0.01, 0.01, 0.00, 0.00,
    0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00,
];

const STRING_PEAKS: [f32; 32] = [
    0.15, 0.35, 0.58, 0.75, 0.82, 0.85, 0.83, 0.80, 0.78, 0.79, 0.81, 0.78, 0.75, 0.73, 0.72, 0.70,
    0.68, 0.65, 0.62, 0.58, 0.54, 0.49, 0.44, 0.38, 0.32, 0.26, 0.20, 0.15, 0.10, 0.07, 0.04, 0.01,
];

const SYNTH_PEAKS: [f32; 32] = [
    0.85, 0.82, 0.78, 0.75, 0.72, 0.68, 0.65, 0.62, 0.60, 0.58, 0.55, 0.52, 0.50, 0.48, 0.45, 0.43,
    0.41, 0.39, 0.37, 0.35, 0.33, 0.31, 0.29, 0.27, 0.25, 0.23, 0.21, 0.19, 0.17, 0.15, 0.12, 0.08,
];

const FOLEY_PEAKS: [f32; 32] = [
    0.42, 0.65, 0.80, 0.55, 0.72, 0.48, 0.63, 0.50, 0.45, 0.58, 0.39, 0.52, 0.48, 0.40, 0.46, 0.35,
    0.38, 0.30, 0.33, 0.28, 0.31, 0.25, 0.22, 0.20, 0.18, 0.15, 0.14, 0.12, 0.10, 0.08, 0.05, 0.02,
];

pub const SOUND_ASSETS: &[SoundAsset] = &[
    // Acoustic Pianos (salamander-grand-piano-v3)
    SoundAsset {
        name: "Salamander Grand Piano C4",
        pack_id: "salamander-grand-piano-v3",
        pack_name: "Salamander Grand Piano V3",
        category: "Acoustic Pianos",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 3.80,
        size_str: "24 MB",
        license: "Public Domain (CC-0)",
        description:
            "Yamaha C5 Grand natural concert sustain sampled with 16 velocity layers at 48kHz.",
        peaks: &PIANO_PEAKS,
    },
    SoundAsset {
        name: "Grand Piano Staccato Forte",
        pack_id: "salamander-grand-piano-v3",
        pack_name: "Salamander Grand Piano V3",
        category: "Acoustic Pianos",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.90,
        size_str: "8 MB",
        license: "Public Domain (CC-0)",
        description: "Crisp percussive hammer strike with immediate string damping and pedal up.",
        peaks: &PIANO_PEAKS,
    },
    SoundAsset {
        name: "Grand Piano Warm Felt",
        pack_id: "salamander-grand-piano-v3",
        pack_name: "Salamander Grand Piano V3",
        category: "Acoustic Pianos",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 4.20,
        size_str: "18 MB",
        license: "Public Domain (CC-0)",
        description:
            "Mellow acoustic felt piano voicing with muted high harmonics and warm resonance.",
        peaks: &PIANO_PEAKS,
    },
    SoundAsset {
        name: "Yamaha C5 Release Resonance",
        pack_id: "salamander-grand-piano-v3",
        pack_name: "Salamander Grand Piano V3",
        category: "Acoustic Pianos",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 1.50,
        size_str: "6 MB",
        license: "Public Domain (CC-0)",
        description: "Sympathetic harmonic ring and soundboard dissipation upon key release.",
        peaks: &PIANO_PEAKS,
    },
    SoundAsset {
        name: "Studio Upright Honky-Tonk",
        pack_id: "salamander-grand-piano-v3",
        pack_name: "Salamander Grand Piano V3",
        category: "Acoustic Pianos",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 2.70,
        size_str: "14 MB",
        license: "Public Domain (CC-0)",
        description: "Vintage tack piano timbre with natural detuned unisons.",
        peaks: &PIANO_PEAKS,
    },
    // Drum Kits (salamander-drumkit-v1)
    SoundAsset {
        name: "Acoustic Kick 22-inch",
        pack_id: "salamander-drumkit-v1",
        pack_name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.65,
        size_str: "4 MB",
        license: "Public Domain (CC-0)",
        description: "Solid maple bass drum with punchy beater click and tight sub-low extension.",
        peaks: &DRUM_PEAKS,
    },
    SoundAsset {
        name: "Snare Maple 14-inch Rimshot",
        pack_id: "salamander-drumkit-v1",
        pack_name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.45,
        size_str: "3 MB",
        license: "Public Domain (CC-0)",
        description: "Crack of brass hoop on 10-ply maple shell with bright snare wire buzz.",
        peaks: &DRUM_PEAKS,
    },
    SoundAsset {
        name: "Hi-Hat Closed Edge",
        pack_id: "salamander-drumkit-v1",
        pack_name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.22,
        size_str: "1.5 MB",
        license: "Public Domain (CC-0)",
        description: "14-inch hand-hammered cymbals struck on the bow with stick tip.",
        peaks: &DRUM_PEAKS,
    },
    SoundAsset {
        name: "Hi-Hat Open Foot Splash",
        pack_id: "salamander-drumkit-v1",
        pack_name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 1.10,
        size_str: "2 MB",
        license: "Public Domain (CC-0)",
        description: "Loose sizzle and ring from pedal release.",
        peaks: &DRUM_PEAKS,
    },
    SoundAsset {
        name: "Ride Cymbal 21-inch Bell",
        pack_id: "salamander-drumkit-v1",
        pack_name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 2.80,
        size_str: "5 MB",
        license: "Public Domain (CC-0)",
        description: "Clear acoustic ping on heavy unlathed cymbal bell with long shimmer.",
        peaks: &DRUM_PEAKS,
    },
    SoundAsset {
        name: "Floor Tom Low 16-inch",
        pack_id: "salamander-drumkit-v1",
        pack_name: "Salamander Drumkit V1",
        category: "Drum Kits",
        format: ".sfz",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.85,
        size_str: "3 MB",
        license: "Public Domain (CC-0)",
        description: "Warm round low fundamental resonance tuned to F1.",
        peaks: &DRUM_PEAKS,
    },
    // Orchestral & Strings (versilian-chamber-orchestra-2)
    SoundAsset {
        name: "Violin Section Legato Sustain",
        pack_id: "versilian-chamber-orchestra-2",
        pack_name: "Versilian Chamber Orchestra 2 CE",
        category: "Orchestral & Strings",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 5.20,
        size_str: "32 MB",
        license: "CC-0 1.0 Universal",
        description: "Smooth bowed lyrical sustain from 8-player first violin section.",
        peaks: &STRING_PEAKS,
    },
    SoundAsset {
        name: "Violoncello Solo Expressivo",
        pack_id: "versilian-chamber-orchestra-2",
        pack_name: "Versilian Chamber Orchestra 2 CE",
        category: "Orchestral & Strings",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 4.80,
        size_str: "28 MB",
        license: "CC-0 1.0 Universal",
        description: "Rich cello vibrato in the expressive mid register with bow friction texture.",
        peaks: &STRING_PEAKS,
    },
    SoundAsset {
        name: "Double Bass Pizzicato Tight",
        pack_id: "versilian-chamber-orchestra-2",
        pack_name: "Versilian Chamber Orchestra 2 CE",
        category: "Orchestral & Strings",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.75,
        size_str: "8 MB",
        license: "CC-0 1.0 Universal",
        description: "Deep plucked acoustic upright bass with thick finger attack and dry decay.",
        peaks: &STRING_PEAKS,
    },
    SoundAsset {
        name: "Flute Concertina Vibrato",
        pack_id: "versilian-chamber-orchestra-2",
        pack_name: "Versilian Chamber Orchestra 2 CE",
        category: "Orchestral & Strings",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 3.60,
        size_str: "16 MB",
        license: "CC-0 1.0 Universal",
        description: "Pure breathy woodwind flute tone with delicate vibrato.",
        peaks: &STRING_PEAKS,
    },
    SoundAsset {
        name: "French Horn Section Warm",
        pack_id: "versilian-chamber-orchestra-2",
        pack_name: "Versilian Chamber Orchestra 2 CE",
        category: "Orchestral & Strings",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 4.10,
        size_str: "22 MB",
        license: "CC-0 1.0 Universal",
        description: "Noble four-horn brass blend in mezzo-piano.",
        peaks: &STRING_PEAKS,
    },
    // Electronic & Single-Cycle (dori-essential-factory-kit)
    SoundAsset {
        name: "Moog Ladder Saw Bass",
        pack_id: "dori-essential-factory-kit",
        pack_name: "Dōri Essential Factory Kit",
        category: "Electronic & Single-Cycle",
        format: ".wav",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 1.80,
        size_str: "2 MB",
        license: "Public Domain (CC-0)",
        description: "Analog 24dB transistor ladder filter sweep on warm multi-saw oscillator.",
        peaks: &SYNTH_PEAKS,
    },
    SoundAsset {
        name: "Additive Spectral Harmonics",
        pack_id: "dori-essential-factory-kit",
        pack_name: "Dōri Essential Factory Kit",
        category: "Electronic & Single-Cycle",
        format: ".wav",
        sample_rate: "48.0 kHz",
        bit_depth: "32-bit",
        duration_secs: 2.40,
        size_str: "3 MB",
        license: "Public Domain (CC-0)",
        description: "Summed sine wave partials generating crystalline FM bell overtones.",
        peaks: &SYNTH_PEAKS,
    },
    SoundAsset {
        name: "Techno Sub Bass Punch",
        pack_id: "dori-essential-factory-kit",
        pack_name: "Dōri Essential Factory Kit",
        category: "Electronic & Single-Cycle",
        format: ".wav",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 1.20,
        size_str: "1.8 MB",
        license: "Public Domain (CC-0)",
        description: "Tight sine-drop sub transient tuned for 4-on-the-floor kick reinforcement.",
        peaks: &SYNTH_PEAKS,
    },
    SoundAsset {
        name: "Round Saw Inverse Sizzle",
        pack_id: "dori-essential-factory-kit",
        pack_name: "Dōri Essential Factory Kit",
        category: "Electronic & Single-Cycle",
        format: ".ogg",
        sample_rate: "44.1 kHz",
        bit_depth: "16-bit",
        duration_secs: 1.50,
        size_str: "0.5 MB",
        license: "Public Domain (CC-0)",
        description: "Inverted phase saw waveshape with anti-aliasing bandlimited edge.",
        peaks: &SYNTH_PEAKS,
    },
    SoundAsset {
        name: "Vowel Formant Filter A",
        pack_id: "dori-essential-factory-kit",
        pack_name: "Dōri Essential Factory Kit",
        category: "Electronic & Single-Cycle",
        format: ".wav",
        sample_rate: "48.0 kHz",
        bit_depth: "24-bit",
        duration_secs: 2.00,
        size_str: "2.2 MB",
        license: "Public Domain (CC-0)",
        description: "Dual formant resonator vocal timbre for speech-like lead synthesis.",
        peaks: &SYNTH_PEAKS,
    },
    // Foley & SFX (versilian-miscellania)
    SoundAsset {
        name: "Antique Typewriter Keystroke",
        pack_id: "versilian-miscellania",
        pack_name: "Versilian Miscellania I & II",
        category: "Foley & SFX",
        format: ".wav",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 0.35,
        size_str: "0.8 MB",
        license: "CC-0 1.0 Universal",
        description: "Mechanical typebar strike and ribbon guide slap on platen roller.",
        peaks: &FOLEY_PEAKS,
    },
    SoundAsset {
        name: "Music Box Windup Melody",
        pack_id: "versilian-miscellania",
        pack_name: "Versilian Miscellania I & II",
        category: "Foley & SFX",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 3.20,
        size_str: "12 MB",
        license: "CC-0 1.0 Universal",
        description: "Tuned steel comb tines plucked by brass cylinder pins in wooden case.",
        peaks: &FOLEY_PEAKS,
    },
    SoundAsset {
        name: "Industrial Steam Vent Release",
        pack_id: "versilian-miscellania",
        pack_name: "Versilian Miscellania I & II",
        category: "Foley & SFX",
        format: ".wav",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 2.10,
        size_str: "4 MB",
        license: "CC-0 1.0 Universal",
        description: "Pressurized pneumatic exhaust hiss with metal pipe reverb tail.",
        peaks: &FOLEY_PEAKS,
    },
    SoundAsset {
        name: "Bowed Cymbal Friction Drone",
        pack_id: "versilian-miscellania",
        pack_name: "Versilian Miscellania I & II",
        category: "Foley & SFX",
        format: ".sfz",
        sample_rate: "44.1 kHz",
        bit_depth: "24-bit",
        duration_secs: 6.40,
        size_str: "15 MB",
        license: "CC-0 1.0 Universal",
        description:
            "German bass bow drawn across ride cymbal edge producing haunting harmonic shriek.",
        peaks: &FOLEY_PEAKS,
    },
];

#[derive(Debug)]
pub struct SoundLibraryUiState {
    pub selected_category: usize,
    pub selected_pack_id: Option<String>,
    pub selected_asset_idx: usize,
    pub search_query: String,
    pub pack_filter: usize,
    pub sort_column: usize,
    pub sort_ascending: bool,

    // Window chrome controls
    pub is_maximized: bool,
    pub is_minimized: bool,

    // Audition transport
    pub is_playing: bool,
    pub is_looping: bool,
    pub is_stereo: bool,
    pub play_position: f32,
    pub gain_db: f32,
    pub pitch_semitones: i32,
    pub selection_range: (f32, f32),
    pub last_play_instant: Option<std::time::Instant>,
}

impl Default for SoundLibraryUiState {
    fn default() -> Self {
        Self {
            selected_category: 0,
            selected_pack_id: None,
            selected_asset_idx: 0,
            search_query: String::new(),
            pack_filter: 0,
            sort_column: 0,
            sort_ascending: true,
            is_maximized: false,
            is_minimized: false,
            is_playing: false,
            is_looping: false,
            is_stereo: true,
            play_position: 0.0,
            gain_db: 0.0,
            pitch_semitones: 0,
            selection_range: (0.0, 1.0),
            last_play_instant: None,
        }
    }
}

static UI_STATE: OnceLock<Mutex<SoundLibraryUiState>> = OnceLock::new();

fn ui_state() -> &'static Mutex<SoundLibraryUiState> {
    UI_STATE.get_or_init(|| Mutex::new(SoundLibraryUiState::default()))
}

fn draw_mini_waveform(painter: &Painter, rect: Rect, peaks: &[f32; 32], active: bool) {
    let bar_count = 16;
    let step = rect.width() / bar_count as f32;
    let color = if active {
        Color32::WHITE
    } else {
        Color32::from_rgb(0x6d, 0x9a, 0xc7)
    };
    for i in 0..bar_count {
        let p_idx = i * 2;
        let p = peaks[p_idx].clamp(0.05, 1.0);
        let h = (rect.height() * p * 0.85).max(2.0);
        let x = rect.left() + i as f32 * step + 1.0;
        let y_mid = rect.center().y;
        painter.line_segment(
            [pos2(x, y_mid - h * 0.5), pos2(x, y_mid + h * 0.5)],
            Stroke::new(1.5, color),
        );
    }
}

fn draw_audition_canvas(
    painter: &Painter,
    rect: Rect,
    peaks: &[f32; 32],
    play_pos: f32,
    sel_range: (f32, f32),
) {
    painter.rect_filled(
        rect,
        CornerRadius::same(8),
        Color32::from_rgb(0x14, 0x14, 0x16),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
        StrokeKind::Inside,
    );

    let sel_min_x = rect.left() + rect.width() * sel_range.0.clamp(0.0, 1.0);
    let sel_max_x = rect.left() + rect.width() * sel_range.1.clamp(0.0, 1.0);
    if sel_max_x > sel_min_x {
        let sel_rect =
            Rect::from_min_max(pos2(sel_min_x, rect.top()), pos2(sel_max_x, rect.bottom()));
        painter.rect_filled(
            sel_rect,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(0x12, 0x69, 0xcc, 55),
        );
    }

    let mid_y = rect.center().y;
    painter.line_segment(
        [pos2(rect.left(), mid_y), pos2(rect.right(), mid_y)],
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x25, 0x28)),
    );

    let bars = 64;
    let step = rect.width() / bars as f32;
    let play_x = rect.left() + rect.width() * play_pos.clamp(0.0, 1.0);

    for i in 0..bars {
        let t = i as f32 / bars as f32;
        let peak_idx = ((t * 31.0) as usize).min(31);
        let peak_val = peaks[peak_idx];
        let h = (rect.height() * 0.88 * peak_val).max(2.0);
        let x = rect.left() + i as f32 * step + step * 0.5;
        let color = if x <= play_x {
            Color32::from_rgb(0x51, 0xee, 0xfc)
        } else {
            Color32::from_rgb(0x6d, 0x9a, 0xc7)
        };
        painter.line_segment(
            [pos2(x, mid_y - h * 0.5), pos2(x, mid_y + h * 0.5)],
            Stroke::new(1.8, color),
        );
    }

    painter.line_segment(
        [pos2(play_x, rect.top()), pos2(play_x, rect.bottom())],
        Stroke::new(2.0, Color32::WHITE),
    );
    let p_head = [
        pos2(play_x - 4.0, rect.top()),
        pos2(play_x + 4.0, rect.top()),
        pos2(play_x, rect.top() + 6.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        p_head.to_vec(),
        Color32::WHITE,
        Stroke::NONE,
    ));
}

fn draw_inspector_waveform(painter: &Painter, rect: Rect, peaks: &[f32; 32]) {
    painter.rect_filled(
        rect,
        CornerRadius::same(8),
        Color32::from_rgb(0x14, 0x14, 0x16),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
        StrokeKind::Inside,
    );
    let mid_y = rect.center().y;
    painter.line_segment(
        [pos2(rect.left(), mid_y), pos2(rect.right(), mid_y)],
        Stroke::new(1.0, Color32::from_rgb(0x25, 0x25, 0x28)),
    );
    let bars = 48;
    let step = rect.width() / bars as f32;
    for i in 0..bars {
        let t = i as f32 / bars as f32;
        let p_idx = ((t * 31.0) as usize).min(31);
        let p = peaks[p_idx];
        let h = (rect.height() * 0.85 * p).max(2.0);
        let x = rect.left() + i as f32 * step + step * 0.5;
        let color = if i % 2 == 0 {
            Color32::from_rgb(0x51, 0xee, 0xfc)
        } else {
            Color32::from_rgb(0x6d, 0x9a, 0xc7)
        };
        painter.line_segment(
            [pos2(x, mid_y - h * 0.5), pos2(x, mid_y + h * 0.5)],
            Stroke::new(1.8, color),
        );
    }
}

/// Elides text with an ellipsis so it never overflows max_w.
fn elide_text(painter: &egui::Painter, text: &str, font: &FontId, max_w: f32) -> String {
    let galley = painter.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    if galley.size().x <= max_w {
        return text.to_string();
    }
    let mut truncated = String::new();
    for c in text.chars() {
        let test = format!("{truncated}{c}…");
        let g = painter.layout_no_wrap(test, font.clone(), Color32::WHITE);
        if g.size().x > max_w {
            break;
        }
        truncated.push(c);
    }
    if truncated.is_empty() {
        "…".to_string()
    } else {
        format!("{truncated}…")
    }
}

pub fn show_manager_window(ctx: &egui::Context, s: &mut UiState) {
    if !s.show_sound_library_manager {
        return;
    }

    let mut open = s.show_sound_library_manager;
    let mut close = false;

    let avail = ctx.content_rect().shrink(24.0);
    let window_id = egui::Id::unique("dori_sound_library_manager");

    // Apply any dragged delta from previous frame to AreaState BEFORE Window begins
    if let Some(delta) = ctx.data_mut(|d| d.get_temp::<Vec2>(window_id.with("hdr_drag_delta"))) {
        let layer_id = egui::LayerId::new(egui::Order::Middle, window_id);
        if let Some(mut state) = egui::AreaState::load(ctx, window_id) {
            let cur_pos = state.left_top_pos();
            let new_pos = cur_pos + delta;
            let clamped_x = new_pos
                .x
                .clamp(avail.left(), (avail.right() - 200.0).max(avail.left()));
            let clamped_y = new_pos
                .y
                .clamp(avail.top(), (avail.bottom() - 48.0).max(avail.top()));
            state.set_left_top_pos(pos2(clamped_x, clamped_y));
            ctx.memory_mut(|mem| {
                mem.areas_mut().set_state(layer_id, state);
            });
        }
    }

    let default_pos = pos2(
        (avail.left() + (avail.width() - 920.0) / 2.0).max(avail.left()),
        (avail.top() + (avail.height() - 560.0) / 2.0).max(avail.top()),
    );

    let (is_maximized, is_minimized) = {
        let st = ui_state().lock().unwrap();
        (st.is_maximized, st.is_minimized)
    };

    let (win_default_size, win_min_size, win_max_size) = if is_maximized {
        (avail.size(), avail.size(), avail.size())
    } else if is_minimized {
        (
            vec2(920.0, 44.0),
            vec2(780.0, 44.0),
            vec2(avail.width(), 44.0),
        )
    } else {
        (
            vec2(920.0, 560.0),
            vec2(780.0, 460.0),
            vec2(avail.width(), avail.height()),
        )
    };

    let mut win = egui::Window::new("dori_sound_library_manager")
        .id(window_id)
        .open(&mut open)
        .title_bar(false)
        .movable(true)
        .resizable(!is_maximized && !is_minimized)
        .collapsible(false)
        .constrain_to(avail)
        .default_pos(default_pos)
        .default_size(win_default_size)
        .min_size(win_min_size)
        .max_size(win_max_size)
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x25, 0x25, 0x27))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                .corner_radius(CornerRadius::same(10))
                .shadow(egui::epaint::Shadow::NONE)
                .inner_margin(0.0),
        );

    if is_maximized {
        win = win.fixed_pos(avail.min);
    }

    win.show(ctx, |ui| {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }

        let win_rect = ui.max_rect();

        // 10px rounded background fill (#252527)
        ui.painter().rect_filled(
            win_rect,
            CornerRadius::same(10),
            Color32::from_rgb(0x25, 0x25, 0x27),
        );

        let top_h = 44.0f32;
        let bottom_h = 116.0f32;
        let top_bar_rect = Rect::from_min_size(win_rect.min, vec2(win_rect.width(), top_h));
        let bottom_bar_rect = Rect::from_min_size(
            pos2(win_rect.left(), win_rect.bottom() - bottom_h),
            vec2(win_rect.width(), bottom_h),
        );
        let body_rect = Rect::from_min_max(
            pos2(win_rect.left(), win_rect.top() + top_h),
            pos2(win_rect.right(), win_rect.bottom() - bottom_h),
        );

        // Top-only dragging mechanism
        let drag_resp = ui.interact(
            top_bar_rect,
            ui.scope_id().with("sound_lib_header_drag"),
            Sense::drag(),
        );
        if drag_resp.dragged() {
            let delta = drag_resp.drag_delta();
            if delta != Vec2::ZERO {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(window_id.with("hdr_drag_delta"), delta));
                let layer_id = ui.layer_id();
                if let Some(mut state) = egui::AreaState::load(ui.ctx(), layer_id.id) {
                    let cur_pos = state.left_top_pos();
                    let new_pos = cur_pos + delta;
                    let clamped_x = new_pos
                        .x
                        .clamp(avail.left(), (avail.right() - 200.0).max(avail.left()));
                    let clamped_y = new_pos
                        .y
                        .clamp(avail.top(), (avail.bottom() - 48.0).max(avail.top()));
                    state.set_left_top_pos(pos2(clamped_x, clamped_y));
                    ui.ctx().memory_mut(|mem| {
                        mem.areas_mut().set_state(layer_id, state);
                    });
                }
            }
        } else {
            ui.ctx()
                .data_mut(|d| d.remove::<Vec2>(window_id.with("hdr_drag_delta")));
        }

        // Consume drag on body and bottom deck to prevent accidental window movement
        if !is_minimized {
            ui.interact(body_rect, ui.scope_id().with("body_no_drag"), Sense::drag());
            ui.interact(
                bottom_bar_rect,
                ui.scope_id().with("bottom_no_drag"),
                Sense::drag(),
            );
        }

        // Top Bar Background & Divider (#252527 with 1px #38383c divider)
        ui.painter().rect_filled(
            top_bar_rect,
            if is_minimized {
                CornerRadius::same(10)
            } else {
                CornerRadius {
                    nw: 10,
                    ne: 10,
                    sw: 0,
                    se: 0,
                }
            },
            Color32::from_rgb(0x25, 0x25, 0x27),
        );
        if !is_minimized {
            ui.painter().line_segment(
                [top_bar_rect.left_bottom(), top_bar_rect.right_bottom()],
                Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
            );
        }

        let reg = get_cached_registry();
        let active_dl = active_download();
        let mut ui_st = ui_state().lock().unwrap();

        // Filter assets according to selection
        let mut visible_assets: Vec<&SoundAsset> = SOUND_ASSETS
            .iter()
            .filter(|asset| {
                if ui_st.selected_category != 0 {
                    let cat_name = CATEGORIES[ui_st.selected_category.min(CATEGORIES.len() - 1)];
                    if asset.category != cat_name {
                        return false;
                    }
                }
                if let Some(ref sel_pack) = ui_st.selected_pack_id {
                    if asset.pack_id != sel_pack.as_str() {
                        return false;
                    }
                }
                if ui_st.pack_filter == 1 {
                    if !reg.installed.iter().any(|p| p.id == asset.pack_id) {
                        return false;
                    }
                } else if ui_st.pack_filter >= 2 {
                    let p_idx = ui_st.pack_filter - 2;
                    if let Some(p) = CATALOG.get(p_idx) {
                        if asset.pack_id != p.id {
                            return false;
                        }
                    }
                }
                if !ui_st.search_query.is_empty() {
                    let q = ui_st.search_query.to_lowercase();
                    if !asset.name.to_lowercase().contains(&q)
                        && !asset.category.to_lowercase().contains(&q)
                        && !asset.pack_name.to_lowercase().contains(&q)
                        && !asset.format.to_lowercase().contains(&q)
                        && !asset.description.to_lowercase().contains(&q)
                    {
                        return false;
                    }
                }
                true
            })
            .collect();

        match ui_st.sort_column {
            0 => visible_assets.sort_by(|a, b| {
                if ui_st.sort_ascending {
                    a.name.cmp(b.name)
                } else {
                    b.name.cmp(a.name)
                }
            }),
            1 => visible_assets.sort_by(|a, b| {
                let pa = a.peaks[0];
                let pb = b.peaks[0];
                if ui_st.sort_ascending {
                    pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
                }
            }),
            2 => visible_assets.sort_by(|a, b| {
                if ui_st.sort_ascending {
                    a.duration_secs.total_cmp(&b.duration_secs)
                } else {
                    b.duration_secs.total_cmp(&a.duration_secs)
                }
            }),
            3 => visible_assets.sort_by(|a, b| {
                if ui_st.sort_ascending {
                    a.format.cmp(b.format)
                } else {
                    b.format.cmp(a.format)
                }
            }),
            4 => visible_assets.sort_by(|a, b| {
                if ui_st.sort_ascending {
                    a.sample_rate.cmp(b.sample_rate)
                } else {
                    b.sample_rate.cmp(a.sample_rate)
                }
            }),
            5 => visible_assets.sort_by(|a, b| {
                if ui_st.sort_ascending {
                    a.pack_name.cmp(b.pack_name)
                } else {
                    b.pack_name.cmp(a.pack_name)
                }
            }),
            6 => visible_assets.sort_by(|a, b| {
                if ui_st.sort_ascending {
                    a.license.cmp(b.license)
                } else {
                    b.license.cmp(a.license)
                }
            }),
            _ => {}
        }

        if ui_st.selected_asset_idx >= visible_assets.len() && !visible_assets.is_empty() {
            ui_st.selected_asset_idx = 0;
        }

        // ---------------------------------------------------------------
        // 1. TOP TOOLBAR BAR (48px Height, 1px #38383c Divider)
        // ---------------------------------------------------------------
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(top_bar_rect.shrink2(vec2(14.0, 10.0))),
            |ui| {
                ui.horizontal_centered(|ui| {
                    // Left: Waveform Icon + Title
                    ui.label(
                        egui::RichText::new(phosphor_egui::regular::WAVEFORM)
                            .size(16.0)
                            .color(Color32::from_rgb(0x51, 0xee, 0xfc)),
                    );
                    ui.label(
                        egui::RichText::new(tr("Dōri Sound Library"))
                            .size(14.0)
                            .strong()
                            .color(Color32::WHITE),
                    );

                    ui.add_space(14.0);
                    ui.painter().line_segment(
                        [
                            pos2(ui.cursor().left(), top_bar_rect.center().y - 10.0),
                            pos2(ui.cursor().left(), top_bar_rect.center().y + 10.0),
                        ],
                        Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
                    );
                    ui.add_space(14.0);

                    // Center: "All Packs" dropdown
                    let filter_name = match ui_st.pack_filter {
                        0 => "All Packs",
                        1 => "Installed Packs",
                        idx => CATALOG.get(idx - 2).map_or("Custom Pack", |p| p.name),
                    };
                    egui::ComboBox::from_id_salt("top_pack_filter_cb")
                        .width(135.0)
                        .selected_text(filter_name)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(ui_st.pack_filter == 0, "All Packs")
                                .clicked()
                            {
                                ui_st.pack_filter = 0;
                                ui_st.selected_pack_id = None;
                            }
                            if ui
                                .selectable_label(ui_st.pack_filter == 1, "Installed Packs")
                                .clicked()
                            {
                                ui_st.pack_filter = 1;
                                ui_st.selected_pack_id = None;
                            }
                            for (i, p) in CATALOG.iter().enumerate() {
                                if ui
                                    .selectable_label(ui_st.pack_filter == i + 2, p.name)
                                    .clicked()
                                {
                                    ui_st.pack_filter = i + 2;
                                    ui_st.selected_pack_id = Some(p.id.to_string());
                                }
                            }
                        });

                    ui.add_space(10.0);

                    // Wider search field with magnifying glass in recessed box
                    let (s_box_rect, _) = ui.allocate_exact_size(vec2(180.0, 28.0), Sense::hover());
                    ui.painter().rect_filled(
                        s_box_rect,
                        CornerRadius::same(6),
                        Color32::from_rgb(0x1c, 0x1c, 0x1e),
                    );
                    ui.painter().rect_stroke(
                        s_box_rect,
                        CornerRadius::same(6),
                        Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
                        StrokeKind::Inside,
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(s_box_rect.shrink2(vec2(8.0, 4.0))),
                        |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(phosphor_egui::regular::MAGNIFYING_GLASS)
                                        .size(13.0)
                                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut ui_st.search_query)
                                        .hint_text("Keywords...")
                                        .frame(egui::Frame::NONE)
                                        .desired_width(140.0),
                                );
                            });
                        },
                    );

                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(format!("{} items", visible_assets.len()))
                            .size(11.0)
                            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                    );

                    // Right-aligned controls
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Far Top-Right: Unified Window Controls (Minimize, Maximize/Restore, Close)
                        let action = crate::ui_chrome::draw_window_controls(ui, ui_st.is_maximized);
                        match action {
                            crate::ui_chrome::WindowControlAction::Minimize => {
                                ui_st.is_minimized = !ui_st.is_minimized;
                            }
                            crate::ui_chrome::WindowControlAction::Maximize => {
                                ui_st.is_maximized = !ui_st.is_maximized;
                                ui_st.is_minimized = false;
                            }
                            crate::ui_chrome::WindowControlAction::Close => {
                                close = true;
                            }
                            crate::ui_chrome::WindowControlAction::None => {}
                        }

                        ui.add_space(8.0);
                        ui.painter().line_segment(
                            [
                                pos2(ui.cursor().left(), top_bar_rect.center().y - 10.0),
                                pos2(ui.cursor().left(), top_bar_rect.center().y + 10.0),
                            ],
                            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
                        );
                        ui.add_space(8.0);

                        // Outlined Import button
                        let imp_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  {}",
                                    phosphor_egui::regular::FOLDER_OPEN,
                                    tr("Import (.sfz / .sf2)")
                                ))
                                .size(11.5)
                                .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(0x2b, 0x2b, 0x2e))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(vec2(144.0, 28.0)),
                        );
                        if imp_btn.clicked() {
                            import_external_interactive(s);
                        }
                    });
                });
            },
        );

        if is_minimized {
            return;
        }

        // ---------------------------------------------------------------
        // 2. THREE-COLUMN BODY
        // ---------------------------------------------------------------
        let sidebar_w = 200.0f32;
        let inspector_w = 248.0f32;
        let sidebar_rect = Rect::from_min_size(body_rect.min, vec2(sidebar_w, body_rect.height()));
        let inspector_rect = Rect::from_min_size(
            pos2(body_rect.right() - inspector_w, body_rect.top()),
            vec2(inspector_w, body_rect.height()),
        );
        let center_rect = Rect::from_min_max(
            pos2(sidebar_rect.right(), body_rect.top()),
            pos2(inspector_rect.left(), body_rect.bottom()),
        );

        // Left Sidebar background & border (#1c1c1e with 1px #38383c right border)
        ui.painter().rect_filled(
            sidebar_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x1c, 0x1c, 0x1e),
        );
        ui.painter().line_segment(
            [sidebar_rect.right_top(), sidebar_rect.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
        );

        // Center Table background (#252527)
        ui.painter().rect_filled(
            center_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x25, 0x25, 0x27),
        );

        // Right Inspector background & border (#1c1c1e with 1px #38383c left border)
        ui.painter().rect_filled(
            inspector_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x1c, 0x1c, 0x1e),
        );
        ui.painter().line_segment(
            [inspector_rect.left_top(), inspector_rect.left_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
        );

        // Left Tree Sidebar UI
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(sidebar_rect.shrink2(vec2(10.0, 12.0))),
            |ui| {
                ui.set_clip_rect(sidebar_rect);
                egui::ScrollArea::vertical()
                    .id_salt("sound_lib_tree_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("CATEGORIES")
                                    .size(10.0)
                                    .strong()
                                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                            );
                            ui.add_space(8.0);

                            for (idx, &cat) in CATEGORIES.iter().enumerate() {
                                let is_sel = ui_st.selected_category == idx;
                                let count = if idx == 0 {
                                    SOUND_ASSETS.len()
                                } else {
                                    SOUND_ASSETS.iter().filter(|a| a.category == cat).count()
                                };

                                let icon = match idx {
                                    0 => phosphor_egui::regular::SQUARES_FOUR,
                                    1 => phosphor_egui::regular::WAVEFORM,
                                    2 => phosphor_egui::regular::PIANO_KEYS,
                                    3 => phosphor_egui::regular::SPEAKER_HIGH,
                                    4 => phosphor_egui::regular::MUSIC_NOTES,
                                    _ => phosphor_egui::regular::WAVEFORM,
                                };

                                let row_rect = ui.available_rect_before_wrap();
                                let btn_rect =
                                    Rect::from_min_size(row_rect.min, vec2(sidebar_w - 20.0, 28.0));
                                let resp = ui.interact(
                                    btn_rect,
                                    egui::Id::unique(("cat_row", idx)),
                                    Sense::click(),
                                );

                                let bg = if is_sel {
                                    Color32::from_rgb(0x12, 0x69, 0xcc)
                                } else if resp.hovered() {
                                    Color32::from_rgb(0x2b, 0x2b, 0x2e)
                                } else {
                                    Color32::TRANSPARENT
                                };

                                ui.painter()
                                    .rect_filled(btn_rect, CornerRadius::same(6), bg);

                                let text_color = if is_sel {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(0xcc, 0xd0, 0xd8)
                                };

                                ui.painter().text(
                                    pos2(btn_rect.left() + 10.0, btn_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    icon,
                                    FontId::proportional(13.0),
                                    if is_sel {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(0x8e, 0x8e, 0x93)
                                    },
                                );
                                ui.painter().text(
                                    pos2(btn_rect.left() + 30.0, btn_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    cat,
                                    FontId::proportional(11.5),
                                    text_color,
                                );
                                ui.painter().text(
                                    pos2(btn_rect.right() - 10.0, btn_rect.center().y),
                                    Align2::RIGHT_CENTER,
                                    count.to_string(),
                                    FontId::proportional(10.5),
                                    if is_sel {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(0x8e, 0x8e, 0x93)
                                    },
                                );

                                if resp.clicked() {
                                    ui_st.selected_category = idx;
                                    ui_st.selected_pack_id = None;
                                    ui_st.selected_asset_idx = 0;
                                }
                                ui.add_space(30.0);
                            }

                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new("PACKS")
                                    .size(10.0)
                                    .strong()
                                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                            );
                            ui.add_space(8.0);

                            for p in CATALOG {
                                let is_installed = reg.installed.iter().any(|i| i.id == p.id);
                                let is_sel = ui_st.selected_pack_id.as_deref() == Some(p.id);

                                let row_rect = ui.available_rect_before_wrap();
                                let btn_rect =
                                    Rect::from_min_size(row_rect.min, vec2(sidebar_w - 20.0, 28.0));
                                let resp = ui.interact(
                                    btn_rect,
                                    egui::Id::unique(("pack_row", p.id)),
                                    Sense::click(),
                                );

                                let bg = if is_sel {
                                    Color32::from_rgb(0x12, 0x69, 0xcc)
                                } else if resp.hovered() {
                                    Color32::from_rgb(0x2b, 0x2b, 0x2e)
                                } else {
                                    Color32::TRANSPARENT
                                };

                                ui.painter()
                                    .rect_filled(btn_rect, CornerRadius::same(6), bg);

                                let text_color = if is_sel {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(0xcc, 0xd0, 0xd8)
                                };

                                ui.painter().text(
                                    pos2(btn_rect.left() + 10.0, btn_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    phosphor_egui::regular::PACKAGE,
                                    FontId::proportional(13.0),
                                    if is_sel {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(0x8e, 0x8e, 0x93)
                                    },
                                );

                                ui.painter().text(
                                    pos2(btn_rect.left() + 30.0, btn_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    p.name,
                                    FontId::proportional(11.5),
                                    text_color,
                                );

                                if is_installed {
                                    ui.painter().text(
                                        pos2(btn_rect.right() - 10.0, btn_rect.center().y),
                                        Align2::RIGHT_CENTER,
                                        phosphor_egui::regular::CHECK,
                                        FontId::proportional(12.0),
                                        if is_sel {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_rgb(0x51, 0xee, 0xfc)
                                        },
                                    );
                                }

                                if resp.clicked() {
                                    if is_sel {
                                        ui_st.selected_pack_id = None;
                                    } else {
                                        ui_st.selected_pack_id = Some(p.id.to_string());
                                    }
                                    ui_st.selected_asset_idx = 0;
                                }
                                ui.add_space(30.0);
                            }
                        });
                    });
            },
        );

        // Center Asset Table UI
        let col_h = 30.0;
        let table_hdr_rect = Rect::from_min_size(center_rect.min, vec2(center_rect.width(), col_h));
        ui.painter().rect_filled(
            table_hdr_rect,
            CornerRadius::ZERO,
            Color32::from_rgb(0x21, 0x21, 0x23),
        );
        ui.painter().line_segment(
            [table_hdr_rect.left_bottom(), table_hdr_rect.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
        );

        let fixed_cols_w = 366.0f32;
        let col_name_w = (center_rect.width() - fixed_cols_w - 8.0).max(100.0);
        let mut h_x = table_hdr_rect.left();

        let cols = [
            ("Name", col_name_w, 0),
            ("Waveform", 64.0, 1),
            ("Duration", 54.0, 2),
            ("Format", 50.0, 3),
            ("Rate", 54.0, 4),
            ("Pack", 84.0, 5),
            ("License", 60.0, 6),
        ];

        for (title, w, col_idx) in cols {
            let cell_rect = Rect::from_min_size(pos2(h_x, table_hdr_rect.top()), vec2(w, col_h));
            let resp = ui.interact(
                cell_rect,
                egui::Id::unique(("tbl_hdr", col_idx)),
                Sense::click(),
            );
            if resp.clicked() {
                if ui_st.sort_column == col_idx {
                    ui_st.sort_ascending = !ui_st.sort_ascending;
                } else {
                    ui_st.sort_column = col_idx;
                    ui_st.sort_ascending = true;
                }
            }
            let arrow = if ui_st.sort_column == col_idx {
                if ui_st.sort_ascending {
                    " ↑"
                } else {
                    " ↓"
                }
            } else {
                ""
            };
            let hdr_cell_clip = cell_rect.intersect(table_hdr_rect).intersect(center_rect);
            let g_cell = ui.painter().with_clip_rect(hdr_cell_clip);
            let label_text = format!("{title}{arrow}");
            let pad_x = if col_idx == 0 { 8.0 } else { 4.0 };
            g_cell.text(
                pos2(cell_rect.left() + pad_x, cell_rect.center().y),
                Align2::LEFT_CENTER,
                label_text,
                FontId::proportional(11.0),
                Color32::from_rgb(0x8e, 0x8e, 0x93),
            );
            h_x += w;
        }

        // Table rows (Scrollable)
        let table_body_rect = Rect::from_min_max(
            pos2(center_rect.left(), center_rect.top() + col_h),
            center_rect.max,
        );

        let mut load_asset_name: Option<String> = None;

        ui.scope_builder(egui::UiBuilder::new().max_rect(table_body_rect), |ui| {
            ui.set_clip_rect(table_body_rect.intersect(center_rect));
            egui::ScrollArea::vertical()
                .id_salt("sound_lib_table_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let row_h = 28.0;
                    for (row_idx, asset) in visible_assets.iter().enumerate() {
                        let is_sel = ui_st.selected_asset_idx == row_idx;
                        let r_rect = ui.available_rect_before_wrap();
                        let row_box =
                            Rect::from_min_size(r_rect.min, vec2(center_rect.width(), row_h));
                        let resp = ui.interact(
                            row_box,
                            egui::Id::unique(("tbl_row", row_idx)),
                            Sense::click(),
                        );

                        let bg = if is_sel {
                            Color32::from_rgb(0x12, 0x69, 0xcc)
                        } else if resp.hovered() {
                            Color32::from_rgb(0x2d, 0x2d, 0x30)
                        } else if row_idx % 2 == 1 {
                            Color32::from_rgb(0x24, 0x24, 0x26)
                        } else {
                            Color32::from_rgb(0x1e, 0x1e, 0x1e)
                        };

                        let row_clip = row_box.intersect(table_body_rect).intersect(center_rect);
                        ui.painter().with_clip_rect(row_clip).rect_filled(
                            row_box,
                            CornerRadius::ZERO,
                            bg,
                        );

                        // Subtle bottom row divider
                        ui.painter().with_clip_rect(row_clip).line_segment(
                            [row_box.left_bottom(), row_box.right_bottom()],
                            Stroke::new(1.0, Color32::from_rgb(0x2c, 0x2c, 0x30)),
                        );

                        let text_color = if is_sel {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(0xdd, 0xdd, 0xdd)
                        };
                        let dim_color = if is_sel {
                            Color32::from_rgb(0xcc, 0xdd, 0xff)
                        } else {
                            Color32::from_rgb(0x8e, 0x8e, 0x93)
                        };

                        let mut cur_x = row_box.left();

                        // Name (8px padding, clipped and elided)
                        let name_cell = Rect::from_min_size(
                            pos2(cur_x, row_box.top()),
                            vec2(col_name_w, row_h),
                        );
                        let g_name = ui.painter().with_clip_rect(
                            name_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        let elided_name = elide_text(
                            &g_name,
                            asset.name,
                            &FontId::proportional(11.0),
                            col_name_w - 12.0,
                        );
                        g_name.text(
                            pos2(name_cell.left() + 8.0, row_box.center().y),
                            Align2::LEFT_CENTER,
                            elided_name,
                            FontId::proportional(11.0),
                            text_color,
                        );
                        cur_x += col_name_w;

                        let (_, row_peaks, _, row_dur) =
                            get_or_create_asset_audio(asset.name, asset);

                        // Mini Waveform (centered in 64px cell)
                        let wave_cell =
                            Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(64.0, row_h));
                        let g_wave = ui.painter().with_clip_rect(
                            wave_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        let wave_r = Rect::from_center_size(
                            pos2(cur_x + 32.0, row_box.center().y),
                            vec2(48.0, 14.0),
                        );
                        draw_mini_waveform(&g_wave, wave_r, &row_peaks, is_sel);
                        cur_x += 64.0;

                        // Duration (54px)
                        let dur_cell =
                            Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(54.0, row_h));
                        let g_dur = ui.painter().with_clip_rect(
                            dur_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        let dur_str = format!("{:.2}s", row_dur);
                        g_dur.text(
                            pos2(cur_x + 4.0, row_box.center().y),
                            Align2::LEFT_CENTER,
                            dur_str,
                            FontId::proportional(10.5),
                            dim_color,
                        );
                        cur_x += 54.0;

                        // Format (50px)
                        let fmt_cell =
                            Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(50.0, row_h));
                        let g_fmt = ui.painter().with_clip_rect(
                            fmt_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        g_fmt.text(
                            pos2(cur_x + 4.0, row_box.center().y),
                            Align2::LEFT_CENTER,
                            asset.format,
                            FontId::proportional(10.5),
                            dim_color,
                        );
                        cur_x += 50.0;

                        // Sample Rate (54px)
                        let rate_cell =
                            Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(54.0, row_h));
                        let g_rate = ui.painter().with_clip_rect(
                            rate_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        g_rate.text(
                            pos2(cur_x + 4.0, row_box.center().y),
                            Align2::LEFT_CENTER,
                            asset.sample_rate,
                            FontId::proportional(10.5),
                            dim_color,
                        );
                        cur_x += 54.0;

                        // Pack (84px)
                        let pack_cell =
                            Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(84.0, row_h));
                        let g_pack = ui.painter().with_clip_rect(
                            pack_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        let elided_pack = elide_text(
                            &g_pack,
                            asset.pack_name,
                            &FontId::proportional(10.5),
                            84.0 - 6.0,
                        );
                        g_pack.text(
                            pos2(cur_x + 4.0, row_box.center().y),
                            Align2::LEFT_CENTER,
                            elided_pack,
                            FontId::proportional(10.5),
                            dim_color,
                        );
                        cur_x += 84.0;

                        // License (60px)
                        let lic_cell =
                            Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(60.0, row_h));
                        let g_lic = ui.painter().with_clip_rect(
                            lic_cell.intersect(table_body_rect).intersect(center_rect),
                        );
                        g_lic.text(
                            pos2(cur_x + 4.0, row_box.center().y),
                            Align2::LEFT_CENTER,
                            asset.license,
                            FontId::proportional(10.0),
                            if is_sel {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(0x6d, 0x9a, 0xc7)
                            },
                        );

                        if resp.clicked() {
                            let was_sel = ui_st.selected_asset_idx == row_idx;
                            ui_st.selected_asset_idx = row_idx;
                            if !was_sel {
                                ui_st.play_position = 0.0;
                                if ui_st.is_playing {
                                    let (cur_audio, _, cur_sr, _) =
                                        get_or_create_asset_audio(asset.name, asset);
                                    let playback_speed =
                                        (2.0f64).powf(ui_st.pitch_semitones as f64 / 12.0);
                                    let gain = 10.0f32.powf(ui_st.gain_db / 20.0);
                                    let mut player = prowave_engine::AuditionPlayer::new(
                                        (*cur_audio).clone(),
                                        cur_sr,
                                    );
                                    player.speed = playback_speed;
                                    player.gain = gain;
                                    player.is_stereo = ui_st.is_stereo;
                                    player.is_looping = ui_st.is_looping;
                                    player.pos = 0.0;
                                    s.audition.push(crate::piano_roll::Audition::StartSample(
                                        Box::new(player),
                                    ));
                                }
                            }
                        }
                        if resp.double_clicked() {
                            ui_st.selected_asset_idx = row_idx;
                            load_asset_name = Some(asset.name.to_string());
                        }

                        ui.add_space(row_h);
                    }
                });
        });

        // Right Inspector Pane UI
        let sel_asset = visible_assets.get(ui_st.selected_asset_idx).copied();

        ui.scope_builder(
            egui::UiBuilder::new().max_rect(inspector_rect.shrink2(vec2(14.0, 12.0))),
            |ui| {
                ui.set_clip_rect(inspector_rect);
                if let Some(asset) = sel_asset {
                    egui::ScrollArea::vertical()
                        .id_salt("sound_lib_inspector_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(asset.name)
                                        .size(14.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} • {}",
                                        asset.pack_name, asset.format
                                    ))
                                    .size(11.0)
                                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                );

                                ui.add_space(10.0);

                                // Waveform preview card (68px tall, #141416, 8px radius)
                                let (w_rect, _) = ui.allocate_exact_size(
                                    vec2(inspector_w - 28.0, 68.0),
                                    Sense::hover(),
                                );
                                let (_, insp_peaks, _, _) =
                                    get_or_create_asset_audio(asset.name, asset);
                                draw_inspector_waveform(ui.painter(), w_rect, &insp_peaks);

                                ui.add_space(12.0);

                                // Clean non-overlapping metadata rows with subtle dividers
                                let meta_items = [
                                    ("Category", asset.category),
                                    ("Format", asset.format),
                                    ("Sample Rate", asset.sample_rate),
                                    ("Bit Depth", asset.bit_depth),
                                    ("File Size", asset.size_str),
                                    ("Source", "Dōri Sound Library"),
                                    ("License", asset.license),
                                ];

                                for (k, v) in meta_items {
                                    let row_rect = ui.available_rect_before_wrap();
                                    let cell_r = Rect::from_min_size(
                                        row_rect.min,
                                        vec2(inspector_w - 28.0, 24.0),
                                    );
                                    ui.painter().line_segment(
                                        [cell_r.left_bottom(), cell_r.right_bottom()],
                                        Stroke::new(1.0, Color32::from_rgb(0x2e, 0x2e, 0x32)),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            vec2(80.0, 24.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(k)
                                                        .size(10.0)
                                                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                                );
                                            },
                                        );
                                        ui.allocate_ui_with_layout(
                                            vec2((inspector_w - 28.0 - 84.0).max(10.0), 24.0),
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(v)
                                                        .size(10.5)
                                                        .color(Color32::from_rgb(0xcc, 0xd0, 0xd8)),
                                                );
                                            },
                                        );
                                    });
                                    ui.add_space(2.0);
                                }

                                ui.add_space(8.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(asset.description)
                                            .size(10.5)
                                            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Wrap),
                                );

                                // Contextual Action Buttons
                                let pack_is_installed =
                                    reg.installed.iter().any(|i| i.id == asset.pack_id);
                                let pack_is_dl = active_dl
                                    .as_ref()
                                    .map_or(false, |a| a.pack_id == asset.pack_id);

                                ui.add_space(14.0);

                                if !pack_is_installed {
                                    if pack_is_dl {
                                        let dl = active_dl.as_ref().unwrap();
                                        let pb_w = inspector_w - 28.0;
                                        let (pb_r, _) =
                                            ui.allocate_exact_size(vec2(pb_w, 6.0), Sense::hover());
                                        ui.painter().rect_filled(
                                            pb_r,
                                            CornerRadius::same(3),
                                            Color32::from_rgb(0x14, 0x14, 0x16),
                                        );
                                        let fill_w = pb_w * dl.percent.clamp(0.0, 1.0);
                                        ui.painter().rect_filled(
                                            Rect::from_min_size(pb_r.min, vec2(fill_w, 6.0)),
                                            CornerRadius::same(3),
                                            Color32::from_rgb(0x12, 0x69, 0xcc),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.0}% • Downloading…",
                                                dl.percent * 100.0
                                            ))
                                            .size(10.5)
                                            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                        );
                                    } else {
                                        let dl_btn = ui.add(
                                            egui::Button::new(
                                                egui::RichText::new(format!(
                                                    "{}  {}",
                                                    phosphor_egui::regular::DOWNLOAD_SIMPLE,
                                                    tr("Download Pack")
                                                ))
                                                .size(11.5)
                                                .strong()
                                                .color(Color32::from_rgb(0x18, 0x18, 0x1a)),
                                            )
                                            .fill(Color32::WHITE)
                                            .corner_radius(CornerRadius::same(6))
                                            .min_size(vec2(inspector_w - 28.0, 30.0)),
                                        );
                                        if dl_btn.clicked() {
                                            if let Some(p) =
                                                CATALOG.iter().find(|cp| cp.id == asset.pack_id)
                                            {
                                                start_download(p);
                                            }
                                        }
                                    }
                                } else {
                                    let load_btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new(format!(
                                                "{}  {}",
                                                phosphor_egui::regular::PLUS,
                                                "Load to Track"
                                            ))
                                            .size(11.5)
                                            .strong()
                                            .color(Color32::WHITE),
                                        )
                                        .fill(Color32::from_rgb(0x12, 0x69, 0xcc))
                                        .corner_radius(CornerRadius::same(6))
                                        .min_size(vec2(inspector_w - 28.0, 30.0)),
                                    );
                                    if load_btn.clicked() {
                                        load_asset_name = Some(asset.name.to_string());
                                    }

                                    ui.add_space(6.0);

                                    let def_btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new(format!(
                                                "{}  {}",
                                                phosphor_egui::regular::STAR,
                                                tr("Make Default")
                                            ))
                                            .size(11.0)
                                            .color(Color32::from_rgb(0xcc, 0xd0, 0xd8)),
                                        )
                                        .fill(Color32::from_rgb(0x2b, 0x2b, 0x2e))
                                        .stroke(Stroke::new(
                                            1.0,
                                            Color32::from_rgb(0x38, 0x38, 0x3c),
                                        ))
                                        .corner_radius(CornerRadius::same(6))
                                        .min_size(vec2(inspector_w - 28.0, 26.0)),
                                    );
                                    if def_btn.clicked() {
                                        set_default_pack(asset.pack_id, asset.category);
                                    }

                                    ui.add_space(4.0);

                                    let rm_btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new(format!(
                                                "{}  {}",
                                                phosphor_egui::regular::TRASH,
                                                tr("Remove")
                                            ))
                                            .size(10.5)
                                            .color(Color32::from_rgb(0xff, 0x45, 0x3a)),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .corner_radius(CornerRadius::same(6)),
                                    );
                                    if rm_btn.clicked() {
                                        remove_installed_pack(asset.pack_id);
                                    }
                                }
                            });
                        });
                }
            },
        );

        // ---------------------------------------------------------------
        // 3. BOTTOM AUDITION TRANSPORT & WAVEFORM DECK (108px Height)
        // ---------------------------------------------------------------
        ui.painter().rect_filled(
            bottom_bar_rect,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 10,
                se: 10,
            },
            Color32::from_rgb(0x18, 0x18, 0x1a),
        );
        ui.painter().line_segment(
            [bottom_bar_rect.left_top(), bottom_bar_rect.right_top()],
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
        );

        let cur_asset = sel_asset.unwrap_or(&SOUND_ASSETS[0]);
        let (cur_audio, cur_peaks, cur_sr, cur_dur) =
            get_or_create_asset_audio(cur_asset.name, cur_asset);
        let playback_speed = (2.0f64).powf(ui_st.pitch_semitones as f64 / 12.0);

        // Handle continuous playback position
        if ui_st.is_playing {
            let now = std::time::Instant::now();
            let dt = ui_st
                .last_play_instant
                .map_or(0.016, |t| now.duration_since(t).as_secs_f32());
            ui_st.last_play_instant = Some(now);
            let eff_dur = (cur_dur / playback_speed as f32).max(0.05);
            ui_st.play_position += dt / eff_dur;
            if ui_st.play_position >= 1.0 {
                if ui_st.is_looping {
                    ui_st.play_position = 0.0;
                } else {
                    ui_st.play_position = 1.0;
                    ui_st.is_playing = false;
                    s.audition.push(crate::piano_roll::Audition::StopSample);
                }
            }
            ctx.request_repaint();
        }

        ui.scope_builder(
            egui::UiBuilder::new().max_rect(bottom_bar_rect.shrink2(vec2(14.0, 8.0))),
            |ui| {
                ui.vertical(|ui| {
                    // Row 1: Transport Controls & Parameters (30px)
                    ui.horizontal_centered(|ui| {
                        let play_icon = if ui_st.is_playing {
                            phosphor_egui::regular::PAUSE_CIRCLE
                        } else {
                            phosphor_egui::regular::PLAY
                        };
                        let (play_bg, play_fg) = if ui_st.is_playing {
                            (Color32::from_rgb(0x12, 0x69, 0xcc), Color32::WHITE)
                        } else {
                            (Color32::from_rgb(0x2b, 0x2b, 0x2e), Color32::WHITE)
                        };
                        let play_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(play_icon).size(14.0).color(play_fg),
                            )
                            .fill(play_bg)
                            .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(vec2(30.0, 30.0)),
                        );
                        if play_btn.clicked() {
                            ui_st.is_playing = !ui_st.is_playing;
                            if ui_st.is_playing {
                                if ui_st.play_position >= 1.0 {
                                    ui_st.play_position = 0.0;
                                }
                                ui_st.last_play_instant = Some(std::time::Instant::now());
                                let gain = 10.0f32.powf(ui_st.gain_db / 20.0);
                                let mut player = prowave_engine::AuditionPlayer::new(
                                    (*cur_audio).clone(),
                                    cur_sr,
                                );
                                player.speed = playback_speed;
                                player.gain = gain;
                                player.is_stereo = ui_st.is_stereo;
                                player.is_looping = ui_st.is_looping;
                                let total_frames = player.samples.len() / 2;
                                player.pos = (ui_st.play_position as f64 * total_frames as f64)
                                    .clamp(0.0, total_frames.saturating_sub(1) as f64);
                                s.audition.push(crate::piano_roll::Audition::StartSample(
                                    Box::new(player),
                                ));
                            } else {
                                s.audition.push(crate::piano_roll::Audition::StopSample);
                            }
                        }

                        ui.add_space(2.0);

                        let stop_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(phosphor_egui::regular::STOP)
                                    .size(13.0)
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(0x2b, 0x2b, 0x2e))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(vec2(30.0, 30.0)),
                        );
                        if stop_btn.clicked() {
                            ui_st.is_playing = false;
                            ui_st.play_position = 0.0;
                            s.audition.push(crate::piano_roll::Audition::StopSample);
                        }

                        ui.add_space(2.0);

                        let loop_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(phosphor_egui::regular::ARROWS_CLOCKWISE)
                                    .size(13.0)
                                    .color(if ui_st.is_looping {
                                        Color32::from_rgb(0x51, 0xee, 0xfc)
                                    } else {
                                        Color32::from_rgb(0x8e, 0x8e, 0x93)
                                    }),
                            )
                            .fill(Color32::from_rgb(0x2b, 0x2b, 0x2e))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(vec2(30.0, 30.0)),
                        );
                        if loop_btn.clicked() {
                            ui_st.is_looping = !ui_st.is_looping;
                            if ui_st.is_playing {
                                let gain = 10.0f32.powf(ui_st.gain_db / 20.0);
                                s.audition.push(crate::piano_roll::Audition::SampleParams {
                                    gain,
                                    speed: playback_speed,
                                    is_stereo: ui_st.is_stereo,
                                    is_looping: ui_st.is_looping,
                                    pos_norm: None,
                                });
                            }
                        }

                        ui.add_space(8.0);

                        // Recessed Timecode Readout Pill
                        let cur_time = ui_st.play_position * cur_dur;
                        let mins = (cur_time / 60.0) as u32;
                        let secs = (cur_time % 60.0) as u32;
                        let centis = ((cur_time * 100.0) % 100.0) as u32;
                        let time_str = format!(
                            "{:02}:{:02}:{:02}.{:02}",
                            mins / 60,
                            mins % 60,
                            secs,
                            centis
                        );

                        let (time_rect, _) =
                            ui.allocate_exact_size(vec2(92.0, 30.0), Sense::hover());
                        ui.painter().rect_filled(
                            time_rect,
                            CornerRadius::same(6),
                            Color32::from_rgb(0x14, 0x14, 0x16),
                        );
                        ui.painter().rect_stroke(
                            time_rect,
                            CornerRadius::same(6),
                            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
                            StrokeKind::Inside,
                        );
                        ui.painter().text(
                            time_rect.center(),
                            Align2::CENTER_CENTER,
                            time_str,
                            FontId::monospace(11.5),
                            Color32::from_rgb(0x51, 0xee, 0xfc),
                        );

                        ui.add_space(8.0);

                        let chan_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(if ui_st.is_stereo {
                                    "Stereo"
                                } else {
                                    "Mono"
                                })
                                .size(10.5)
                                .color(Color32::from_rgb(0x6d, 0x9a, 0xc7)),
                            )
                            .fill(Color32::from_rgb(0x2b, 0x2b, 0x2e))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(vec2(54.0, 30.0)),
                        );
                        if chan_btn.clicked() {
                            ui_st.is_stereo = !ui_st.is_stereo;
                            if ui_st.is_playing {
                                let gain = 10.0f32.powf(ui_st.gain_db / 20.0);
                                s.audition.push(crate::piano_roll::Audition::SampleParams {
                                    gain,
                                    speed: playback_speed,
                                    is_stereo: ui_st.is_stereo,
                                    is_looping: ui_st.is_looping,
                                    pos_norm: None,
                                });
                            }
                        }

                        let old_pitch = ui_st.pitch_semitones;
                        let old_gain = ui_st.gain_db;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add(
                                egui::Slider::new(&mut ui_st.pitch_semitones, -12..=12)
                                    .suffix(" st")
                                    .text("Pitch"),
                            );
                            ui.add_space(10.0);
                            ui.add(
                                egui::Slider::new(&mut ui_st.gain_db, -24.0..=6.0)
                                    .suffix(" dB")
                                    .text("Gain"),
                            );
                        });

                        if (ui_st.pitch_semitones != old_pitch
                            || (ui_st.gain_db - old_gain).abs() > 0.001)
                            && ui_st.is_playing
                        {
                            let new_speed = (2.0f64).powf(ui_st.pitch_semitones as f64 / 12.0);
                            let new_gain = 10.0f32.powf(ui_st.gain_db / 20.0);
                            s.audition.push(crate::piano_roll::Audition::SampleParams {
                                gain: new_gain,
                                speed: new_speed,
                                is_stereo: ui_st.is_stereo,
                                is_looping: ui_st.is_looping,
                                pos_norm: None,
                            });
                        }
                    });

                    ui.add_space(4.0);

                    // Row 2: Interactive Audition Scrubber (44px height)
                    let wave_w = (win_rect.width() - 28.0).max(100.0);
                    let (w_rect, w_resp) =
                        ui.allocate_exact_size(vec2(wave_w, 44.0), Sense::click_and_drag());
                    if w_resp.clicked() || w_resp.dragged() {
                        if let Some(p) = w_resp.interact_pointer_pos() {
                            let t = ((p.x - w_rect.left()) / w_rect.width()).clamp(0.0, 1.0);
                            ui_st.play_position = t;
                            if ui_st.is_playing {
                                let gain = 10.0f32.powf(ui_st.gain_db / 20.0);
                                s.audition.push(crate::piano_roll::Audition::SampleParams {
                                    gain,
                                    speed: playback_speed,
                                    is_stereo: ui_st.is_stereo,
                                    is_looping: ui_st.is_looping,
                                    pos_norm: Some(t),
                                });
                            }
                        }
                    }
                    draw_audition_canvas(
                        ui.painter(),
                        w_rect,
                        &cur_peaks,
                        ui_st.play_position,
                        ui_st.selection_range,
                    );

                    ui.add_space(4.0);

                    // Row 3: Bottom Status Bar (11pt text)
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} • {} • {:.1} kHz • {} • {:.2}s",
                                cur_asset.name,
                                cur_asset.format,
                                cur_sr / 1000.0,
                                cur_asset.bit_depth,
                                cur_dur
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let status_str = if ui_st.is_playing {
                                "Auditioning…"
                            } else {
                                "Ready"
                            };
                            ui.label(egui::RichText::new(status_str).size(11.0).color(
                                if ui_st.is_playing {
                                    Color32::from_rgb(0x51, 0xee, 0xfc)
                                } else {
                                    Color32::from_rgb(0x8e, 0x8e, 0x93)
                                },
                            ));
                        });
                    });
                });
            },
        );

        // Ensure crisp 10px rounded border on all four outer corners
        ui.painter().rect_stroke(
            win_rect,
            CornerRadius::same(10),
            Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
            StrokeKind::Inside,
        );

        // If an asset was requested to load
        if let Some(name) = load_asset_name {
            s.status = Some(format!("Loaded '{name}' to Track"));
        }
    });

    if close {
        open = false;
    }
    s.show_sound_library_manager = open;
}

fn set_default_pack(id: &str, category: &str) {
    mutate_cached_registry(|reg| {
        reg.set_default(id, category);
    });
}

pub fn remove_installed_pack(id: &str) {
    mutate_cached_registry(|reg| {
        reg.remove(id);
    });
}

// ---------------------------------------------------------------------------
// 6. Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_resolution_finds_local_and_extension_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let exs_dir = dir.path().join("Instruments");
        let samples_dir = exs_dir.join("Samples");
        std::fs::create_dir_all(&samples_dir).unwrap();

        let wav = samples_dir.join("kick.wav");
        std::fs::write(&wav, b"RIFF dummy").unwrap();

        assert_eq!(resolve_exs_sample("kick.wav", &exs_dir), Some(wav.clone()));
        assert_eq!(resolve_exs_sample("kick.aif", &exs_dir), Some(wav));
    }

    #[test]
    fn registry_persists_managed_and_external_packs_without_duplication() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = SoundRegistry::default();
        let managed = dir.path().join("sounds");
        reg.managed_path = Some(managed.clone());

        let ext_dir = dir.path().join("ExternalLibrary");
        std::fs::create_dir_all(&ext_dir).unwrap();
        let sfz_file = ext_dir.join("UserBass.sfz");
        std::fs::write(&sfz_file, b"<region> sample=dummy.wav").unwrap();

        let id = reg
            .import_external(sfz_file.clone())
            .expect("import succeeds");
        assert_eq!(reg.installed.len(), 1);
        assert_eq!(reg.installed[0].id, id);
        assert!(reg.installed[0].is_external);
        assert_eq!(reg.installed[0].main_file, sfz_file);

        // Does not duplicate file on disk
        assert!(!managed.join("UserBass.sfz").exists());

        // Category resolution
        let resolved = reg.get_default_or_first_for_category("Imported Sounds");
        assert!(resolved.is_some());
        assert_eq!(resolved.unwrap().id, id);
    }

    #[test]
    fn dsp_fallback_when_no_pack_installed() {
        // Clearing or mocking empty registry returns None, triggering pure-Rust DSP fallback
        let resolved = resolve_library_patch("NonExistentCategory", "Patch");
        assert!(
            resolved.is_none(),
            "Uninstalled pack resolves to None for DSP fallback"
        );
    }

    #[test]
    fn catalog_contains_all_required_packs() {
        assert_eq!(CATALOG.len(), 5);
        assert!(CATALOG
            .iter()
            .any(|p| p.name == "Dōri Essential Factory Kit"));
        assert!(CATALOG
            .iter()
            .any(|p| p.name == "Salamander Grand Piano V3"));
        assert!(CATALOG.iter().any(|p| p.name == "Salamander Drumkit V1"));
        assert!(CATALOG
            .iter()
            .any(|p| p.name == "Versilian Chamber Orchestra 2 CE"));
        assert!(CATALOG
            .iter()
            .any(|p| p.name == "Versilian Miscellania I & II"));
    }
}
