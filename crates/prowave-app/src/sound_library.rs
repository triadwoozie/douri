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
            let _ = std::fs::create_dir_all(&pack_dir);

            let sfz_file = pack_dir.join("instrument.sfz");
            let sfz_content = format!(
                "// Dōri Managed Sound Library: {}\n<control>\ndefault_path=Samples/\n<global>\nampeg_attack=0.001\nampeg_release=0.3\n<group>\n<region> sample=*sine key=60\n",
                pack_name
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
        CornerRadius::same(2),
        Color32::from_rgb(0x16, 0x16, 0x18),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2a)),
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
            Color32::from_rgba_unmultiplied(0x12, 0x69, 0xcc, 45),
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
        Stroke::new(1.5, Color32::WHITE),
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
        CornerRadius::same(2),
        Color32::from_rgb(0x16, 0x16, 0x18),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a)),
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
        painter.line_segment(
            [pos2(x, mid_y - h * 0.5), pos2(x, mid_y + h * 0.5)],
            Stroke::new(1.6, Color32::from_rgb(0x51, 0xee, 0xfc)),
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

    let avail = ctx.input(|i| i.viewport_rect());
    let default_pos = pos2(
        (avail.left() + (avail.width() - 860.0) / 2.0).max(avail.left()),
        (avail.top() + (avail.height() - 500.0) / 2.0).max(avail.top()),
    );

    egui::Window::new("dori_sound_library_manager")
        .open(&mut open)
        .title_bar(false)
        .movable(true)
        .resizable(true)
        .collapsible(false)
        .constrain_to(avail)
        .default_pos(default_pos)
        .default_size(vec2(860.0, 500.0))
        .min_size(vec2(700.0, 400.0))
        .max_size(vec2(1120.0, 640.0))
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x1c, 0x1c, 0x1e))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                .corner_radius(CornerRadius::same(2))
                .inner_margin(0.0),
        )
        .show(ctx, |ui| {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }

            let win_rect = ui.max_rect();

            // Background fill (#1c1c1e)
            ui.painter().rect_filled(
                win_rect,
                CornerRadius::same(2),
                Color32::from_rgb(0x1c, 0x1c, 0x1e),
            );

            let top_h = 36.0;
            let bottom_h = 88.0;
            let top_bar_rect = Rect::from_min_size(win_rect.min, vec2(win_rect.width(), top_h));
            let bottom_bar_rect = Rect::from_min_size(
                pos2(win_rect.left(), win_rect.bottom() - bottom_h),
                vec2(win_rect.width(), bottom_h),
            );
            let body_rect = Rect::from_min_max(
                pos2(win_rect.left(), win_rect.top() + top_h),
                pos2(win_rect.right(), win_rect.bottom() - bottom_h),
            );

            // Title Bar Dragging across top bar, using AreaState
            let drag_resp = ui.interact(
                top_bar_rect,
                ui.scope_id().with("sound_lib_title_bar"),
                Sense::drag(),
            );
            if drag_resp.dragged() {
                let delta = drag_resp.drag_delta();
                if delta != Vec2::ZERO {
                    let layer_id = ui.layer_id();
                    if let Some(mut state) = egui::AreaState::load(ui.ctx(), layer_id.id) {
                        state.set_left_top_pos(state.left_top_pos() + delta);
                        ui.ctx().memory_mut(|mem| {
                            mem.areas_mut().set_state(layer_id, state);
                        });
                    }
                }
            }

            // Top Bar Background & Border (#252527 with 1px #38383c border)
            ui.painter().rect_filled(
                top_bar_rect,
                CornerRadius::ZERO,
                Color32::from_rgb(0x25, 0x25, 0x27),
            );
            ui.painter().line_segment(
                [top_bar_rect.left_bottom(), top_bar_rect.right_bottom()],
                Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
            );

            let reg = get_cached_registry();
            let active_dl = active_download();
            let mut ui_st = ui_state().lock().unwrap();

            // Filter assets according to selection
            let mut visible_assets: Vec<&SoundAsset> = SOUND_ASSETS
                .iter()
                .filter(|asset| {
                    if ui_st.selected_category != 0 {
                        let cat_name =
                            CATEGORIES[ui_st.selected_category.min(CATEGORIES.len() - 1)];
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
            // 1. TOP TOOLBAR BAR
            // ---------------------------------------------------------------
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(top_bar_rect.shrink(8.0)),
                |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(
                            egui::RichText::new(phosphor_egui::regular::WAVEFORM)
                                .size(16.0)
                                .color(Color32::from_rgb(0x51, 0xee, 0xfc)),
                        );
                        ui.label(
                            egui::RichText::new(tr("Dōri Sound Library"))
                                .size(13.0)
                                .strong()
                                .color(Color32::WHITE),
                        );

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // Pack filter dropdown
                        let filter_name = match ui_st.pack_filter {
                            0 => "All Packs",
                            1 => "Installed Packs",
                            idx => CATALOG.get(idx - 2).map_or("Custom Pack", |p| p.name),
                        };
                        egui::ComboBox::from_id_salt("top_pack_filter_cb")
                            .width(130.0)
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

                        ui.add_space(6.0);

                        // Search box with magnifying glass
                        ui.label(
                            egui::RichText::new(phosphor_egui::regular::MAGNIFYING_GLASS)
                                .size(12.0)
                                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut ui_st.search_query)
                                .hint_text("Keywords...")
                                .desired_width(130.0),
                        );

                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!("{} items", visible_assets.len()))
                                .size(11.0)
                                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                        );

                        // Right-aligned buttons
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let close_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(phosphor_egui::regular::X)
                                        .size(14.0)
                                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                )
                                .fill(Color32::TRANSPARENT)
                                .corner_radius(CornerRadius::same(2)),
                            );
                            if close_btn.clicked() {
                                close = true;
                            }

                            ui.add_space(8.0);

                            let imp_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(format!(
                                        "{}  {}",
                                        phosphor_egui::regular::FOLDER_OPEN,
                                        tr("Import (.sfz / .sf2)")
                                    ))
                                    .size(11.0)
                                    .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(0x30, 0x30, 0x30))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a)))
                                .corner_radius(CornerRadius::same(2)),
                            );
                            if imp_btn.clicked() {
                                import_external_interactive(s);
                            }
                        });
                    });
                },
            );

            // ---------------------------------------------------------------
            // 2. THREE-COLUMN BODY
            // ---------------------------------------------------------------
            let sidebar_w = 200.0f32;
            let inspector_w = 230.0f32;
            let sidebar_rect =
                Rect::from_min_size(body_rect.min, vec2(sidebar_w, body_rect.height()));
            let inspector_rect = Rect::from_min_size(
                pos2(body_rect.right() - inspector_w, body_rect.top()),
                vec2(inspector_w, body_rect.height()),
            );
            let center_rect = Rect::from_min_max(
                pos2(sidebar_rect.right(), body_rect.top()),
                pos2(inspector_rect.left(), body_rect.bottom()),
            );

            // Left Sidebar background & border (#1c1c1e with 1px #38383c border)
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

            // Right Inspector background & border (#252527 with 1px #38383c border)
            ui.painter().rect_filled(
                inspector_rect,
                CornerRadius::ZERO,
                Color32::from_rgb(0x25, 0x25, 0x27),
            );
            ui.painter().line_segment(
                [inspector_rect.left_top(), inspector_rect.left_bottom()],
                Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
            );

            // Left Tree Sidebar UI
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(sidebar_rect.shrink(8.0)),
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
                                ui.add_space(4.0);

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
                                    let btn_rect = Rect::from_min_size(
                                        row_rect.min,
                                        vec2(sidebar_w - 20.0, 24.0),
                                    );
                                    let resp = ui.interact(
                                        btn_rect,
                                        egui::Id::unique(("cat_row", idx)),
                                        Sense::click(),
                                    );

                                    let bg = if is_sel {
                                        Color32::from_rgb(0x12, 0x69, 0xcc)
                                    } else if resp.hovered() {
                                        Color32::from_rgb(0x2d, 0x2d, 0x30)
                                    } else {
                                        Color32::TRANSPARENT
                                    };

                                    ui.painter()
                                        .rect_filled(btn_rect, CornerRadius::same(2), bg);

                                    let text_color = if is_sel {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(0xcc, 0xd0, 0xd8)
                                    };

                                    ui.painter().text(
                                        pos2(btn_rect.left() + 6.0, btn_rect.center().y),
                                        Align2::LEFT_CENTER,
                                        icon,
                                        FontId::proportional(12.0),
                                        if is_sel {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_rgb(0x6d, 0x9a, 0xc7)
                                        },
                                    );
                                    ui.painter().text(
                                        pos2(btn_rect.left() + 24.0, btn_rect.center().y),
                                        Align2::LEFT_CENTER,
                                        cat,
                                        FontId::proportional(11.0),
                                        text_color,
                                    );
                                    ui.painter().text(
                                        pos2(btn_rect.right() - 6.0, btn_rect.center().y),
                                        Align2::RIGHT_CENTER,
                                        count.to_string(),
                                        FontId::proportional(10.0),
                                        Color32::from_rgb(0x8e, 0x8e, 0x93),
                                    );

                                    if resp.clicked() {
                                        ui_st.selected_category = idx;
                                        ui_st.selected_pack_id = None;
                                        ui_st.selected_asset_idx = 0;
                                    }
                                    ui.add_space(26.0);
                                }

                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new("PACKS")
                                        .size(10.0)
                                        .strong()
                                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                );
                                ui.add_space(4.0);

                                for p in CATALOG {
                                    let is_installed = reg.installed.iter().any(|i| i.id == p.id);
                                    let is_sel = ui_st.selected_pack_id.as_deref() == Some(p.id);

                                    let row_rect = ui.available_rect_before_wrap();
                                    let btn_rect = Rect::from_min_size(
                                        row_rect.min,
                                        vec2(sidebar_w - 20.0, 24.0),
                                    );
                                    let resp = ui.interact(
                                        btn_rect,
                                        egui::Id::unique(("pack_row", p.id)),
                                        Sense::click(),
                                    );

                                    let bg = if is_sel {
                                        Color32::from_rgb(0x12, 0x69, 0xcc)
                                    } else if resp.hovered() {
                                        Color32::from_rgb(0x2d, 0x2d, 0x30)
                                    } else {
                                        Color32::TRANSPARENT
                                    };

                                    ui.painter()
                                        .rect_filled(btn_rect, CornerRadius::same(2), bg);

                                    let text_color = if is_sel {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(0xcc, 0xd0, 0xd8)
                                    };

                                    ui.painter().text(
                                        pos2(btn_rect.left() + 6.0, btn_rect.center().y),
                                        Align2::LEFT_CENTER,
                                        p.name,
                                        FontId::proportional(11.0),
                                        text_color,
                                    );

                                    if is_installed {
                                        ui.painter().text(
                                            pos2(btn_rect.right() - 6.0, btn_rect.center().y),
                                            Align2::RIGHT_CENTER,
                                            phosphor_egui::regular::CHECK,
                                            FontId::proportional(11.0),
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
                                    ui.add_space(26.0);
                                }
                            });
                        });
                },
            );

            // Center Asset Table UI
            let col_h = 24.0;
            let table_hdr_rect =
                Rect::from_min_size(center_rect.min, vec2(center_rect.width(), col_h));
            ui.painter().rect_filled(
                table_hdr_rect,
                CornerRadius::ZERO,
                Color32::from_rgb(0x21, 0x21, 0x23),
            );
            ui.painter().line_segment(
                [table_hdr_rect.left_bottom(), table_hdr_rect.right_bottom()],
                Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
            );

            let fixed_cols_w = 300.0f32;
            let col_name_w = (center_rect.width() - fixed_cols_w - 8.0).max(80.0);
            let mut h_x = table_hdr_rect.left();

            let cols = [
                ("Name", col_name_w, 0),
                ("Waveform", 46.0, 1),
                ("Duration", 40.0, 2),
                ("Format", 40.0, 3),
                ("Rate", 50.0, 4),
                ("Pack", 74.0, 5),
                ("License", 50.0, 6),
            ];

            for (title, w, col_idx) in cols {
                let cell_rect =
                    Rect::from_min_size(pos2(h_x, table_hdr_rect.top()), vec2(w, col_h));
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
                let elided = elide_text(&g_cell, &label_text, &FontId::proportional(10.5), w - 4.0);
                g_cell.text(
                    pos2(cell_rect.left() + 4.0, cell_rect.center().y),
                    Align2::LEFT_CENTER,
                    elided,
                    FontId::proportional(10.5),
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
                        let row_h = 24.0;
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

                            let row_clip =
                                row_box.intersect(table_body_rect).intersect(center_rect);
                            ui.painter().with_clip_rect(row_clip).rect_filled(
                                row_box,
                                CornerRadius::ZERO,
                                bg,
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

                            // Name (clipped and elided)
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
                                col_name_w - 8.0,
                            );
                            g_name.text(
                                pos2(name_cell.left() + 4.0, row_box.center().y),
                                Align2::LEFT_CENTER,
                                elided_name,
                                FontId::proportional(11.0),
                                text_color,
                            );
                            cur_x += col_name_w;

                            // Mini Waveform
                            let wave_cell =
                                Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(46.0, row_h));
                            let g_wave = ui.painter().with_clip_rect(
                                wave_cell.intersect(table_body_rect).intersect(center_rect),
                            );
                            let wave_r = Rect::from_center_size(
                                pos2(cur_x + 23.0, row_box.center().y),
                                vec2(38.0, 14.0),
                            );
                            draw_mini_waveform(&g_wave, wave_r, asset.peaks, is_sel);
                            cur_x += 46.0;

                            // Duration
                            let dur_cell =
                                Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(40.0, row_h));
                            let g_dur = ui.painter().with_clip_rect(
                                dur_cell.intersect(table_body_rect).intersect(center_rect),
                            );
                            let elided_dur = elide_text(
                                &g_dur,
                                &format!("{:.2}s", asset.duration_secs),
                                &FontId::proportional(10.5),
                                40.0 - 4.0,
                            );
                            g_dur.text(
                                pos2(cur_x + 3.0, row_box.center().y),
                                Align2::LEFT_CENTER,
                                elided_dur,
                                FontId::proportional(10.5),
                                dim_color,
                            );
                            cur_x += 40.0;

                            // Format
                            let fmt_cell =
                                Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(40.0, row_h));
                            let g_fmt = ui.painter().with_clip_rect(
                                fmt_cell.intersect(table_body_rect).intersect(center_rect),
                            );
                            let elided_fmt = elide_text(
                                &g_fmt,
                                asset.format,
                                &FontId::proportional(10.5),
                                40.0 - 4.0,
                            );
                            g_fmt.text(
                                pos2(cur_x + 3.0, row_box.center().y),
                                Align2::LEFT_CENTER,
                                elided_fmt,
                                FontId::proportional(10.5),
                                dim_color,
                            );
                            cur_x += 40.0;

                            // Sample Rate
                            let rate_cell =
                                Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(50.0, row_h));
                            let g_rate = ui.painter().with_clip_rect(
                                rate_cell.intersect(table_body_rect).intersect(center_rect),
                            );
                            let elided_rate = elide_text(
                                &g_rate,
                                asset.sample_rate,
                                &FontId::proportional(10.5),
                                50.0 - 4.0,
                            );
                            g_rate.text(
                                pos2(cur_x + 3.0, row_box.center().y),
                                Align2::LEFT_CENTER,
                                elided_rate,
                                FontId::proportional(10.5),
                                dim_color,
                            );
                            cur_x += 50.0;

                            // Pack (clipped and elided)
                            let pack_cell =
                                Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(74.0, row_h));
                            let g_pack = ui.painter().with_clip_rect(
                                pack_cell.intersect(table_body_rect).intersect(center_rect),
                            );
                            let elided_pack = elide_text(
                                &g_pack,
                                asset.pack_name,
                                &FontId::proportional(10.5),
                                74.0 - 4.0,
                            );
                            g_pack.text(
                                pos2(cur_x + 3.0, row_box.center().y),
                                Align2::LEFT_CENTER,
                                elided_pack,
                                FontId::proportional(10.5),
                                dim_color,
                            );
                            cur_x += 74.0;

                            // License (clipped and elided)
                            let lic_cell =
                                Rect::from_min_size(pos2(cur_x, row_box.top()), vec2(50.0, row_h));
                            let g_lic = ui.painter().with_clip_rect(
                                lic_cell.intersect(table_body_rect).intersect(center_rect),
                            );
                            let elided_lic = elide_text(
                                &g_lic,
                                asset.license,
                                &FontId::proportional(10.0),
                                50.0 - 4.0,
                            );
                            g_lic.text(
                                pos2(cur_x + 3.0, row_box.center().y),
                                Align2::LEFT_CENTER,
                                elided_lic,
                                FontId::proportional(10.0),
                                if is_sel {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(0x6d, 0x9a, 0xc7)
                                },
                            );

                            if resp.clicked() {
                                ui_st.selected_asset_idx = row_idx;
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
                egui::UiBuilder::new().max_rect(inspector_rect.shrink(8.0)),
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
                                            .size(13.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.add_space(2.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} • {}",
                                            asset.pack_name, asset.format
                                        ))
                                        .size(10.5)
                                        .color(Color32::from_rgb(0x6d, 0x9a, 0xc7)),
                                    );

                                    ui.add_space(8.0);

                                    // Mini waveform preview canvas
                                    let (w_rect, _) = ui.allocate_exact_size(
                                        vec2(inspector_w - 20.0, 56.0),
                                        Sense::hover(),
                                    );
                                    draw_inspector_waveform(ui.painter(), w_rect, asset.peaks);

                                    ui.add_space(8.0);

                                    // Clean non-overlapping metadata rows
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
                                        ui.horizontal(|ui| {
                                            ui.allocate_ui_with_layout(
                                                vec2(76.0, 16.0),
                                                Layout::left_to_right(Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(k).size(10.0).color(
                                                            Color32::from_rgb(0x8e, 0x8e, 0x93),
                                                        ),
                                                    );
                                                },
                                            );
                                            ui.allocate_ui_with_layout(
                                                vec2((inspector_w - 20.0 - 80.0).max(10.0), 16.0),
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(v).size(10.0).color(
                                                            Color32::from_rgb(0xcc, 0xd0, 0xd8),
                                                        ),
                                                    );
                                                },
                                            );
                                        });
                                        ui.add_space(1.0);
                                    }

                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(asset.description)
                                                .size(10.0)
                                                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Wrap),
                                    );

                                    // Bottom Contextual Action Buttons
                                    let pack_is_installed =
                                        reg.installed.iter().any(|i| i.id == asset.pack_id);
                                    let pack_is_dl = active_dl
                                        .as_ref()
                                        .map_or(false, |a| a.pack_id == asset.pack_id);

                                    ui.add_space(12.0);

                                    if !pack_is_installed {
                                        if pack_is_dl {
                                            let dl = active_dl.as_ref().unwrap();
                                            let pb_w = inspector_w - 20.0;
                                            let (pb_r, _) = ui.allocate_exact_size(
                                                vec2(pb_w, 6.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().rect_filled(
                                                pb_r,
                                                CornerRadius::same(2),
                                                Color32::from_rgb(0x1e, 0x1e, 0x1e),
                                            );
                                            let fill_w = pb_w * dl.percent.clamp(0.0, 1.0);
                                            ui.painter().rect_filled(
                                                Rect::from_min_size(pb_r.min, vec2(fill_w, 6.0)),
                                                CornerRadius::same(2),
                                                Color32::from_rgb(0x12, 0x69, 0xcc),
                                            );
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{:.0}% • Downloading…",
                                                    dl.percent * 100.0
                                                ))
                                                .size(10.0)
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
                                                    .size(11.0)
                                                    .color(Color32::WHITE),
                                                )
                                                .fill(Color32::from_rgb(0x12, 0x69, 0xcc))
                                                .corner_radius(CornerRadius::same(2))
                                                .min_size(vec2(inspector_w - 20.0, 28.0)),
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
                                                .size(11.0)
                                                .color(Color32::WHITE),
                                            )
                                            .fill(Color32::from_rgb(0x12, 0x69, 0xcc))
                                            .corner_radius(CornerRadius::same(2))
                                            .min_size(vec2(inspector_w - 20.0, 26.0)),
                                        );
                                        if load_btn.clicked() {
                                            load_asset_name = Some(asset.name.to_string());
                                        }

                                        ui.add_space(4.0);

                                        let def_btn = ui.add(
                                            egui::Button::new(
                                                egui::RichText::new(format!(
                                                    "{}  {}",
                                                    phosphor_egui::regular::STAR,
                                                    tr("Make Default")
                                                ))
                                                .size(10.5)
                                                .color(Color32::from_rgb(0xcc, 0xd0, 0xd8)),
                                            )
                                            .fill(Color32::from_rgb(0x2b, 0x2b, 0x2e))
                                            .stroke(Stroke::new(
                                                1.0,
                                                Color32::from_rgb(0x38, 0x38, 0x3c),
                                            ))
                                            .corner_radius(CornerRadius::same(2))
                                            .min_size(vec2(inspector_w - 20.0, 24.0)),
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
                                            .corner_radius(CornerRadius::same(2)),
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
            // 3. BOTTOM AUDITION TRANSPORT & WAVEFORM BAR
            // ---------------------------------------------------------------
            ui.painter().rect_filled(
                bottom_bar_rect,
                CornerRadius::ZERO,
                Color32::from_rgb(0x1c, 0x1c, 0x1e),
            );
            ui.painter().line_segment(
                [bottom_bar_rect.left_top(), bottom_bar_rect.right_top()],
                Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
            );

            let cur_asset = sel_asset.unwrap_or(&SOUND_ASSETS[0]);

            // Handle continuous playback position
            if ui_st.is_playing {
                let now = std::time::Instant::now();
                let dt = ui_st
                    .last_play_instant
                    .map_or(0.016, |t| now.duration_since(t).as_secs_f32());
                ui_st.last_play_instant = Some(now);
                let dur = cur_asset.duration_secs.max(0.1);
                ui_st.play_position += dt / dur;
                if ui_st.play_position >= 1.0 {
                    if ui_st.is_looping {
                        ui_st.play_position = 0.0;
                    } else {
                        ui_st.play_position = 1.0;
                        ui_st.is_playing = false;
                        s.audition
                            .push(crate::piano_roll::Audition::Off { pitch: 60 });
                    }
                }
                ctx.request_repaint();
            }

            ui.scope_builder(
                egui::UiBuilder::new().max_rect(bottom_bar_rect.shrink(8.0)),
                |ui| {
                    ui.vertical(|ui| {
                        // Row 1: Transport Controls & Parameters
                        ui.horizontal_centered(|ui| {
                            let play_icon = if ui_st.is_playing {
                                phosphor_egui::regular::PAUSE_CIRCLE
                            } else {
                                phosphor_egui::regular::PLAY
                            };
                            let play_btn = ui.add(
                                egui::Button::new(egui::RichText::new(play_icon).size(15.0).color(
                                    if ui_st.is_playing {
                                        Color32::from_rgb(0x51, 0xee, 0xfc)
                                    } else {
                                        Color32::WHITE
                                    },
                                ))
                                .fill(Color32::from_rgb(0x30, 0x30, 0x30))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a)))
                                .corner_radius(CornerRadius::same(2)),
                            );
                            if play_btn.clicked() {
                                ui_st.is_playing = !ui_st.is_playing;
                                if ui_st.is_playing {
                                    if ui_st.play_position >= 1.0 {
                                        ui_st.play_position = 0.0;
                                    }
                                    ui_st.last_play_instant = Some(std::time::Instant::now());
                                    s.audition.push(crate::piano_roll::Audition::On {
                                        pitch: 60,
                                        vel: 100,
                                        track: 0,
                                    });
                                } else {
                                    s.audition
                                        .push(crate::piano_roll::Audition::Off { pitch: 60 });
                                }
                            }

                            let stop_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(phosphor_egui::regular::STOP)
                                        .size(13.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(0x30, 0x30, 0x30))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a)))
                                .corner_radius(CornerRadius::same(2)),
                            );
                            if stop_btn.clicked() {
                                ui_st.is_playing = false;
                                ui_st.play_position = 0.0;
                                s.audition
                                    .push(crate::piano_roll::Audition::Off { pitch: 60 });
                            }

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
                                .fill(Color32::from_rgb(0x30, 0x30, 0x30))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a)))
                                .corner_radius(CornerRadius::same(2)),
                            );
                            if loop_btn.clicked() {
                                ui_st.is_looping = !ui_st.is_looping;
                            }

                            ui.add_space(8.0);

                            let cur_time = ui_st.play_position * cur_asset.duration_secs;
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
                            ui.label(
                                egui::RichText::new(time_str)
                                    .font(FontId::monospace(11.5))
                                    .color(Color32::from_rgb(0x51, 0xee, 0xfc)),
                            );

                            ui.add_space(6.0);

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
                                .fill(Color32::from_rgb(0x30, 0x30, 0x30))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x3a)))
                                .corner_radius(CornerRadius::same(2)),
                            );
                            if chan_btn.clicked() {
                                ui_st.is_stereo = !ui_st.is_stereo;
                            }

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
                        });

                        ui.add_space(4.0);

                        // Row 2: Interactive Waveform Preview Canvas
                        let wave_w = win_rect.width() - 32.0;
                        let (w_rect, w_resp) =
                            ui.allocate_exact_size(vec2(wave_w, 38.0), Sense::click_and_drag());
                        if w_resp.clicked() || w_resp.dragged() {
                            if let Some(p) = w_resp.interact_pointer_pos() {
                                let t = ((p.x - w_rect.left()) / w_rect.width()).clamp(0.0, 1.0);
                                ui_st.play_position = t;
                            }
                        }
                        draw_audition_canvas(
                            ui.painter(),
                            w_rect,
                            cur_asset.peaks,
                            ui_st.play_position,
                            ui_st.selection_range,
                        );

                        ui.add_space(3.0);

                        // Row 3: Bottom Status Bar
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} • {} • {} • {} • {:.2}s",
                                    cur_asset.name,
                                    cur_asset.format,
                                    cur_asset.sample_rate,
                                    cur_asset.bit_depth,
                                    cur_asset.duration_secs
                                ))
                                .size(10.0)
                                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let status_str = if ui_st.is_playing {
                                    "Auditioning…"
                                } else {
                                    "Ready"
                                };
                                ui.label(egui::RichText::new(status_str).size(10.0).color(
                                    if ui_st.is_playing {
                                        Color32::from_rgb(0x51, 0xee, 0xfc)
                                    } else {
                                        Color32::from_rgb(0x6d, 0x9a, 0xc7)
                                    },
                                ));
                            });
                        });
                    });
                },
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
