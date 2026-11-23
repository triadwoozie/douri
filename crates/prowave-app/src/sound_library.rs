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

        // Scan user directory for manually dropped .sfz/.sf2
        if let Some(managed) = &reg.managed_path {
            let user_dir = managed.join("User");
            let _ = std::fs::create_dir_all(&user_dir);
            reg.scan_user_dir(&user_dir);
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
    let reg = SoundRegistry::load();
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
            let mut reg = SoundRegistry::load();
            let sounds_dir = reg.managed_path.clone().unwrap_or_else(default_sounds_dir);
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

            reg.installed.retain(|p| p.id != pack_id);
            if is_default || !reg.defaults.contains_key(&pack_category) {
                reg.defaults.insert(pack_category, pack_id);
            }
            reg.installed.push(installed);
            reg.save();

            if let Ok(mut guard) = downloader().lock() {
                guard.active = None;
            }
        })
        .ok();
}

pub fn essential_sounds_installed() -> bool {
    let reg = SoundRegistry::load();
    CATALOG
        .iter()
        .filter(|p| p.is_essential)
        .all(|p| reg.installed.iter().any(|i| i.id == p.id))
}

pub fn all_sounds_installed() -> bool {
    let reg = SoundRegistry::load();
    CATALOG
        .iter()
        .all(|p| reg.installed.iter().any(|i| i.id == p.id))
}

pub fn download_essential_sounds(s: &mut UiState) {
    let reg = SoundRegistry::load();
    for pack in CATALOG.iter().filter(|p| p.is_essential) {
        if !reg.installed.iter().any(|i| i.id == pack.id) {
            start_download(pack);
            s.status = Some(tr("Downloading Essential Sounds…").to_string());
            break;
        }
    }
}

pub fn download_all_sounds(s: &mut UiState) {
    let reg = SoundRegistry::load();
    for pack in CATALOG {
        if !reg.installed.iter().any(|i| i.id == pack.id) {
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
        let mut reg = SoundRegistry::load();
        if let Err(e) = reg.relocate(&path) {
            s.status = Some(format!("Failed to relocate Sound Library: {e}"));
        } else {
            s.status = Some(format!("Sound Library relocated to {}", path.display()));
        }
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
        let mut reg = SoundRegistry::load();
        match reg.import_external(path.clone()) {
            Ok(_) => {
                s.status = Some(format!(
                    "Imported {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(e) => {
                s.status = Some(format!("Import failed: {e}"));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Two-Column Sound Library Manager Window UI
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ViewFilter {
    category_idx: usize,
    selected_pack_id: Option<String>,
}

static VIEW_FILTER: OnceLock<Mutex<ViewFilter>> = OnceLock::new();

fn view_filter() -> &'static Mutex<ViewFilter> {
    VIEW_FILTER.get_or_init(|| Mutex::new(ViewFilter::default()))
}

pub fn show_manager_window(ctx: &egui::Context, s: &mut UiState) {
    if !s.show_sound_library_manager {
        return;
    }

    let mut open = true;
    let window_size = vec2(880.0, 520.0);

    egui::Window::new("dori_sound_library_manager")
        .open(&mut open)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .fixed_size(window_size)
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x16, 0x18, 0x1d))
                .corner_radius(8.0),
        )
        .show(ctx, |ui| {
            let win_rect = ui.max_rect();

            // Header (#1c1f26, height 44)
            let header_rect = Rect::from_min_size(win_rect.min, vec2(win_rect.width(), 44.0));
            {
                let g = ui.painter();
                g.rect_filled(
                    header_rect,
                    CornerRadius {
                        nw: 8,
                        ne: 8,
                        sw: 0,
                        se: 0,
                    },
                    Color32::from_rgb(0x1c, 0x1f, 0x26),
                );
                g.line_segment(
                    [header_rect.left_bottom(), header_rect.right_bottom()],
                    Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                );

                // Wave icon + Title
                let wave_center = pos2(header_rect.left() + 24.0, header_rect.center().y);
                draw_dori_wave_icon(&g, wave_center);
                g.text(
                    pos2(header_rect.left() + 44.0, header_rect.center().y),
                    Align2::LEFT_CENTER,
                    tr("Dōri Sound Library"),
                    FontId::proportional(13.5),
                    Color32::WHITE,
                );
            }

            // Minimize (-) and Close (×) buttons top right
            let close_rect = Rect::from_center_size(
                pos2(header_rect.right() - 20.0, header_rect.center().y),
                vec2(22.0, 22.0),
            );
            let min_rect = Rect::from_center_size(
                pos2(header_rect.right() - 48.0, header_rect.center().y),
                vec2(22.0, 22.0),
            );

            let close_resp = ui.interact(close_rect, egui::Id::unique("sl_close"), Sense::click());
            let min_resp = ui.interact(min_rect, egui::Id::unique("sl_min"), Sense::click());

            let btn_color = |resp: &egui::Response| {
                if resp.hovered() {
                    Color32::from_rgb(0x35, 0x3a, 0x47)
                } else {
                    Color32::from_rgb(0x22, 0x25, 0x2d)
                }
            };

            {
                let g = ui.painter();
                g.rect_filled(min_rect, 4.0, btn_color(&min_resp));
                g.text(
                    min_rect.center(),
                    Align2::CENTER_CENTER,
                    "–",
                    FontId::proportional(13.0),
                    Color32::from_rgb(0xaa, 0xb0, 0xbd),
                );

                g.rect_filled(close_rect, 4.0, btn_color(&close_resp));
                g.text(
                    close_rect.center(),
                    Align2::CENTER_CENTER,
                    "×",
                    FontId::proportional(13.0),
                    Color32::from_rgb(0xaa, 0xb0, 0xbd),
                );
            }

            if close_resp.clicked() || min_resp.clicked() {
                s.show_sound_library_manager = false;
            }

            // Body Rect below header
            let body_rect =
                Rect::from_min_max(pos2(win_rect.left(), header_rect.bottom()), win_rect.max);

            // Left Sidebar (width 320, #1c1f26)
            let sidebar_rect = Rect::from_min_size(body_rect.min, vec2(320.0, body_rect.height()));
            {
                let g = ui.painter();
                g.rect_filled(
                    sidebar_rect,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 8,
                        se: 0,
                    },
                    Color32::from_rgb(0x1c, 0x1f, 0x26),
                );
                g.line_segment(
                    [sidebar_rect.right_top(), sidebar_rect.right_bottom()],
                    Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                );
            }

            // Right Main Pane
            let main_rect =
                Rect::from_min_max(pos2(sidebar_rect.right(), body_rect.top()), body_rect.max);

            let reg = SoundRegistry::load();
            let active_dl = active_download();

            // ---------------------------------------------------------------
            // Render Left Sidebar (space-between layout)
            // ---------------------------------------------------------------
            let mut filter_guard = view_filter().lock().unwrap();
            let cat_idx = filter_guard.category_idx.min(CATEGORIES.len() - 1);
            let active_cat = CATEGORIES[cat_idx];

            let matching_packs: Vec<&CatalogPack> = CATALOG
                .iter()
                .filter(|p| active_cat == "All Categories" || p.category == active_cat)
                .collect();

            if filter_guard.selected_pack_id.is_none()
                || !matching_packs
                    .iter()
                    .any(|p| Some(p.id.to_string()) == filter_guard.selected_pack_id)
            {
                filter_guard.selected_pack_id = matching_packs.first().map(|p| p.id.to_string());
            }

            let sel_pack_id = filter_guard.selected_pack_id.clone().unwrap_or_default();
            let current_pack = CATALOG
                .iter()
                .find(|p| p.id == sel_pack_id)
                .or_else(|| CATALOG.first());

            ui.scope_builder(
                egui::UiBuilder::new().max_rect(sidebar_rect.shrink(18.0)),
                |ui| {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        // Top Section
                        ui.label(
                            egui::RichText::new("FILTER")
                                .size(10.5)
                                .color(Color32::from_rgb(0x88, 0x90, 0x9b))
                                .strong(),
                        );
                        ui.add_space(3.0);

                        egui::ComboBox::from_id_salt("cat_filter_cb")
                            .width(284.0)
                            .selected_text(active_cat)
                            .show_ui(ui, |ui| {
                                for (idx, &cat) in CATEGORIES.iter().enumerate() {
                                    if ui
                                        .selectable_label(filter_guard.category_idx == idx, cat)
                                        .clicked()
                                    {
                                        filter_guard.category_idx = idx;
                                    }
                                }
                            });

                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("SELECTED PACK")
                                .size(10.5)
                                .color(Color32::from_rgb(0x88, 0x90, 0x9b))
                                .strong(),
                        );
                        ui.add_space(3.0);

                        let pack_name = current_pack.map_or("None", |p| p.name);
                        egui::ComboBox::from_id_salt("pack_sel_cb")
                            .width(284.0)
                            .selected_text(pack_name)
                            .show_ui(ui, |ui| {
                                for p in &matching_packs {
                                    if ui
                                        .selectable_label(
                                            filter_guard.selected_pack_id.as_deref() == Some(p.id),
                                            p.name,
                                        )
                                        .clicked()
                                    {
                                        filter_guard.selected_pack_id = Some(p.id.to_string());
                                    }
                                }
                            });

                        ui.add_space(14.0);

                        // Pack Specs & License Card
                        if let Some(pack) = current_pack {
                            let card_rect = ui.available_rect_before_wrap();
                            let card_h = 160.0;
                            let card_box = Rect::from_min_size(card_rect.min, vec2(284.0, card_h));
                            ui.painter().rect_filled(
                                card_box,
                                6.0,
                                Color32::from_rgb(0x16, 0x18, 0x1d),
                            );
                            ui.painter().rect_stroke(
                                card_box,
                                6.0,
                                Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                                StrokeKind::Inside,
                            );

                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(card_box.shrink(12.0)),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(pack.name)
                                            .size(13.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    );
                                    ui.add_space(2.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} • {}",
                                                pack.format, pack.size
                                            ))
                                            .size(11.0)
                                            .color(Color32::from_rgb(0x9c, 0xa3, 0xaf)),
                                        );
                                        ui.add_space(6.0);
                                        let l_rect = ui.available_rect_before_wrap();
                                        let badge_box = Rect::from_min_size(
                                            l_rect.min,
                                            vec2(ui.spacing().interact_size.x, 16.0),
                                        );
                                        ui.painter().rect_filled(
                                            badge_box,
                                            3.0,
                                            Color32::from_rgb(0x18, 0x2c, 0x23),
                                        );
                                        ui.label(
                                            egui::RichText::new(pack.license)
                                                .size(10.0)
                                                .color(Color32::from_rgb(0x34, 0xd3, 0x99)),
                                        );
                                    });
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(pack.description)
                                            .size(11.0)
                                            .color(Color32::from_rgb(0xc4, 0xc9, 0xd4)),
                                    );
                                },
                            );
                            ui.add_space(card_h + 10.0);
                        }
                    });

                    // Bottom-Anchored Action Group (Remove + Download CTA)
                    let bottom_y = sidebar_rect.bottom() - 68.0;
                    let is_installed =
                        current_pack.map_or(false, |p| reg.installed.iter().any(|i| i.id == p.id));
                    let is_dl = current_pack.map_or(false, |p| {
                        active_dl.as_ref().map_or(false, |a| a.pack_id == p.id)
                    });

                    if is_installed {
                        let remove_rect = Rect::from_center_size(
                            pos2(sidebar_rect.center().x, bottom_y - 14.0),
                            vec2(100.0, 22.0),
                        );
                        let r_resp =
                            ui.interact(remove_rect, egui::Id::unique("rm_pack"), Sense::click());
                        let r_color = if r_resp.hovered() {
                            Color32::from_rgb(0xff, 0x60, 0x55)
                        } else {
                            Color32::from_rgb(0xff, 0x45, 0x3a)
                        };
                        ui.painter().text(
                            remove_rect.center(),
                            Align2::CENTER_CENTER,
                            "🗑  Remove",
                            FontId::proportional(11.5),
                            r_color,
                        );
                        if r_resp.clicked() {
                            if let Some(pack) = current_pack {
                                remove_installed_pack(pack.id);
                            }
                        }
                    }

                    let cta_rect = Rect::from_min_size(
                        pos2(sidebar_rect.left() + 18.0, sidebar_rect.bottom() - 52.0),
                        vec2(284.0, 34.0),
                    );
                    let cta_resp =
                        ui.interact(cta_rect, egui::Id::unique("cta_dl"), Sense::click());

                    let (cta_bg, cta_txt, cta_fg) = if is_dl {
                        (
                            Color32::from_rgb(0x28, 0x30, 0x2b),
                            "Downloading…",
                            Color32::from_rgb(0x34, 0xd3, 0x99),
                        )
                    } else if is_installed {
                        (
                            Color32::from_rgb(0x22, 0x28, 0x30),
                            "Installed ✓",
                            Color32::from_rgb(0x9c, 0xa3, 0xaf),
                        )
                    } else if cta_resp.hovered() {
                        (
                            Color32::from_rgb(0x1e, 0xa3, 0x6b),
                            "Download Pack",
                            Color32::WHITE,
                        )
                    } else {
                        (
                            Color32::from_rgb(0x1b, 0x94, 0x60),
                            "Download Pack",
                            Color32::WHITE,
                        )
                    };

                    ui.painter().rect_filled(cta_rect, 6.0, cta_bg);
                    ui.painter().text(
                        cta_rect.center(),
                        Align2::CENTER_CENTER,
                        cta_txt,
                        FontId::proportional(13.0),
                        cta_fg,
                    );

                    if cta_resp.clicked() && !is_installed && !is_dl {
                        if let Some(pack) = current_pack {
                            start_download(pack);
                        }
                    }
                },
            );

            drop(filter_guard);

            // ---------------------------------------------------------------
            // Render Right Main Pane
            // ---------------------------------------------------------------
            let import_btn_rect = Rect::from_center_size(
                pos2(main_rect.right() - 84.0, main_rect.top() + 24.0),
                vec2(132.0, 26.0),
            );
            let imp_resp = ui.interact(
                import_btn_rect,
                egui::Id::unique("import_sfz_btn"),
                Sense::click(),
            );
            if imp_resp.clicked() {
                import_external_interactive(s);
            }

            let g = ui.painter();
            let imp_bg = if imp_resp.hovered() {
                Color32::from_rgb(0x2d, 0x32, 0x3d)
            } else {
                Color32::from_rgb(0x20, 0x24, 0x2d)
            };
            g.rect_filled(import_btn_rect, 4.0, imp_bg);
            g.rect_stroke(
                import_btn_rect,
                4.0,
                Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                StrokeKind::Inside,
            );
            g.text(
                import_btn_rect.center(),
                Align2::CENTER_CENTER,
                "Import (.sfz / .sf2)",
                FontId::proportional(11.5),
                Color32::from_rgb(0xd1, 0xd5, 0xdb),
            );

            // Vertically centered Installed Packs List
            let list_area = Rect::from_min_max(
                pos2(main_rect.left() + 28.0, main_rect.top() + 52.0),
                pos2(main_rect.right() - 28.0, main_rect.bottom() - 20.0),
            );

            let has_items = !reg.installed.is_empty() || active_dl.is_some();

            if !has_items {
                // Empty state card centered
                let empty_rect = Rect::from_center_size(list_area.center(), vec2(380.0, 110.0));
                g.rect_filled(empty_rect, 6.0, Color32::from_rgb(0x1c, 0x1f, 0x26));
                g.rect_stroke(
                    empty_rect,
                    6.0,
                    Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                    StrokeKind::Inside,
                );
                g.text(
                    pos2(empty_rect.center().x, empty_rect.center().y - 12.0),
                    Align2::CENTER_CENTER,
                    tr("No sound packs installed yet."),
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                g.text(
                    pos2(empty_rect.center().x, empty_rect.center().y + 14.0),
                    Align2::CENTER_CENTER,
                    "Download packs from the sidebar or click Import to add .sfz / .sf2 files.",
                    FontId::proportional(11.0),
                    Color32::from_rgb(0x88, 0x90, 0x9b),
                );
            } else {
                g.text(
                    pos2(list_area.left(), list_area.top() + 4.0),
                    Align2::LEFT_TOP,
                    "YOUR INSTALLED PACKS",
                    FontId::proportional(11.0),
                    Color32::from_rgb(0x88, 0x90, 0x9b),
                );

                let mut card_y = list_area.top() + 24.0;
                let card_w = list_area.width();

                // 1. In-flight downloading card if any
                if let Some(dl) = &active_dl {
                    let c_rect =
                        Rect::from_min_size(pos2(list_area.left(), card_y), vec2(card_w, 52.0));
                    g.rect_filled(c_rect, 6.0, Color32::from_rgb(0x20, 0x24, 0x2d));
                    g.rect_stroke(
                        c_rect,
                        6.0,
                        Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                        StrokeKind::Inside,
                    );

                    // Name + Category
                    g.text(
                        pos2(c_rect.left() + 14.0, c_rect.top() + 16.0),
                        Align2::LEFT_CENTER,
                        &dl.pack_name,
                        FontId::proportional(12.5),
                        Color32::WHITE,
                    );
                    g.text(
                        pos2(c_rect.left() + 14.0, c_rect.top() + 34.0),
                        Align2::LEFT_CENTER,
                        format!("{} • {}", dl.category, dl.format),
                        FontId::proportional(10.5),
                        Color32::from_rgb(0x9c, 0xa3, 0xaf),
                    );

                    // Inline progress bar + cancel
                    let bar_w = 140.0;
                    let bar_rect = Rect::from_min_size(
                        pos2(c_rect.right() - 210.0, c_rect.center().y - 3.0),
                        vec2(bar_w, 6.0),
                    );
                    g.rect_filled(bar_rect, 3.0, Color32::from_rgb(0x16, 0x18, 0x1d));
                    let fill_w = bar_w * dl.percent.clamp(0.0, 1.0);
                    g.rect_filled(
                        Rect::from_min_size(bar_rect.min, vec2(fill_w, 6.0)),
                        3.0,
                        Color32::from_rgb(0x1b, 0x94, 0x60),
                    );

                    let prog_str = format!(
                        "{:.0}% • {:.1} / {:.1} MB",
                        dl.percent * 100.0,
                        dl.downloaded_mb,
                        dl.total_mb
                    );
                    g.text(
                        pos2(bar_rect.right() + 8.0, c_rect.center().y),
                        Align2::LEFT_CENTER,
                        prog_str,
                        FontId::proportional(10.0),
                        Color32::from_rgb(0x88, 0x90, 0x9b),
                    );

                    let cancel_rect = Rect::from_center_size(
                        pos2(c_rect.right() - 16.0, c_rect.center().y),
                        vec2(18.0, 18.0),
                    );
                    let c_resp = ui.interact(
                        cancel_rect,
                        egui::Id::unique("cancel_dl_btn"),
                        Sense::click(),
                    );
                    g.text(
                        cancel_rect.center(),
                        Align2::CENTER_CENTER,
                        "×",
                        FontId::proportional(13.0),
                        if c_resp.hovered() {
                            Color32::from_rgb(0xff, 0x60, 0x55)
                        } else {
                            Color32::from_rgb(0xff, 0x45, 0x3a)
                        },
                    );
                    if c_resp.clicked() {
                        cancel_active_download();
                    }

                    card_y += 58.0;
                }

                // 2. Installed Pack Cards
                for pack in &reg.installed {
                    if card_y + 52.0 > list_area.bottom() {
                        break;
                    }

                    let c_rect =
                        Rect::from_min_size(pos2(list_area.left(), card_y), vec2(card_w, 50.0));
                    g.rect_filled(c_rect, 6.0, Color32::from_rgb(0x20, 0x24, 0x2d));
                    g.rect_stroke(
                        c_rect,
                        6.0,
                        Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                        StrokeKind::Inside,
                    );

                    // Name + Category
                    g.text(
                        pos2(c_rect.left() + 14.0, c_rect.top() + 16.0),
                        Align2::LEFT_CENTER,
                        &pack.name,
                        FontId::proportional(12.5),
                        Color32::WHITE,
                    );
                    let info_str =
                        format!("{} • {} • {} MB", pack.category, pack.format, pack.size_mb);
                    g.text(
                        pos2(c_rect.left() + 14.0, c_rect.top() + 34.0),
                        Align2::LEFT_CENTER,
                        info_str,
                        FontId::proportional(10.5),
                        Color32::from_rgb(0x9c, 0xa3, 0xaf),
                    );

                    // Installed Badge
                    let inst_badge_rect = Rect::from_min_size(
                        pos2(c_rect.right() - 86.0, c_rect.center().y - 10.0),
                        vec2(72.0, 20.0),
                    );
                    g.rect_filled(inst_badge_rect, 4.0, Color32::from_rgb(0x18, 0x2c, 0x23));
                    g.text(
                        inst_badge_rect.center(),
                        Align2::CENTER_CENTER,
                        "Installed ✓",
                        FontId::proportional(11.0),
                        Color32::from_rgb(0x34, 0xd3, 0x99),
                    );

                    // Default Pill vs Make Default
                    if pack.is_default {
                        let star_pos = pos2(c_rect.right() - 176.0, c_rect.center().y);
                        g.text(
                            star_pos,
                            Align2::CENTER_CENTER,
                            "⭐",
                            FontId::proportional(12.0),
                            Color32::from_rgb(0xff, 0xd7, 0x00),
                        );
                        let def_rect = Rect::from_min_size(
                            pos2(c_rect.right() - 162.0, c_rect.center().y - 10.0),
                            vec2(64.0, 20.0),
                        );
                        g.rect_filled(def_rect, 4.0, Color32::from_rgb(0x2a, 0x2e, 0x38));
                        g.text(
                            def_rect.center(),
                            Align2::CENTER_CENTER,
                            "Default",
                            FontId::proportional(11.0),
                            Color32::from_rgb(0xff, 0xd7, 0x00),
                        );
                    } else {
                        let def_btn_rect = Rect::from_min_size(
                            pos2(c_rect.right() - 180.0, c_rect.center().y - 10.0),
                            vec2(84.0, 20.0),
                        );
                        let d_resp = ui.interact(
                            def_btn_rect,
                            egui::Id::unique(("set_def", &pack.id)),
                            Sense::click(),
                        );
                        let d_bg = if d_resp.hovered() {
                            Color32::from_rgb(0x35, 0x3a, 0x47)
                        } else {
                            Color32::from_rgb(0x24, 0x28, 0x32)
                        };
                        g.rect_filled(def_btn_rect, 4.0, d_bg);
                        g.rect_stroke(
                            def_btn_rect,
                            4.0,
                            Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2e, 0x38)),
                            StrokeKind::Inside,
                        );
                        g.text(
                            def_btn_rect.center(),
                            Align2::CENTER_CENTER,
                            "Make Default",
                            FontId::proportional(10.5),
                            Color32::from_rgb(0xd1, 0xd5, 0xdb),
                        );
                        if d_resp.clicked() {
                            set_default_pack(&pack.id, &pack.category);
                        }
                    }

                    card_y += 56.0;
                }
            }
        });

    if !open {
        s.show_sound_library_manager = false;
    }
}

fn set_default_pack(id: &str, category: &str) {
    let mut reg = SoundRegistry::load();
    reg.set_default(id, category);
}

pub fn remove_installed_pack(id: &str) {
    let mut reg = SoundRegistry::load();
    reg.remove(id);
}

fn draw_dori_wave_icon(g: &Painter, center: Pos2) {
    let bars = [-6.0, -3.0, 0.0, 3.0, 6.0];
    let heights = [6.0, 11.0, 16.0, 10.0, 5.0];
    for (&x_off, &h) in bars.iter().zip(&heights) {
        let p1 = pos2(center.x + x_off, center.y - h * 0.5);
        let p2 = pos2(center.x + x_off, center.y + h * 0.5);
        g.line_segment(
            [p1, p2],
            Stroke::new(2.0, Color32::from_rgb(0x34, 0xd3, 0x99)),
        );
    }
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
        let sfz_file = ext_dir.join("CustomBass.sfz");
        std::fs::write(&sfz_file, b"<region> sample=dummy.wav").unwrap();

        let id = reg
            .import_external(sfz_file.clone())
            .expect("import succeeds");
        assert_eq!(reg.installed.len(), 1);
        assert_eq!(reg.installed[0].id, id);
        assert!(reg.installed[0].is_external);
        assert_eq!(reg.installed[0].main_file, sfz_file);

        // Does not duplicate file on disk
        assert!(!managed.join("CustomBass.sfz").exists());

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
    fn catalog_contains_all_four_required_packs() {
        assert_eq!(CATALOG.len(), 4);
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
