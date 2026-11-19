// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Dōri contributors.

//! Sound Library path management and Apple EXS24 sample resolution.
//!
//! Provides sound library path persistence, discovery of `.exs` instruments in factory
//! sound libraries (e.g. Logic / GarageBand factory libraries), and intelligent resolution
//! of referenced `.caf`, `.aiff`, and `.wav` audio samples.

use std::path::{Path, PathBuf};

fn config_path() -> Option<PathBuf> {
    crate::recent_files::config_dir().map(|d| d.join("sound-library.txt"))
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
        home.as_ref().map(|h| h.join(".local").join("share").join("douri").join("sound_library")),
        home.as_ref().map(|h| h.join("Music").join("Sound Library")),
        home.as_ref().map(|h| h.join("Documents").join("Sound Library")),
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
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exs")) {
                out.push(path);
            }
        }
    }

    out.sort();
    out
}

/// Resolves an EXS-referenced audio sample file by its bare name.
/// Searches local folders next to the instrument, then the Sound Library directories,
/// with fallback extensions for `.caf`, `.aif`, `.aiff`, and `.wav`.
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
        // Direct file name match
        let direct = dir.join(file_name);
        if direct.is_file() {
            return Some(direct);
        }

        // Try candidate extensions
        for ext in exts {
            let with_ext = dir.join(format!("{stem}.{ext}"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }

    None
}

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

        // Exact match
        assert_eq!(
            resolve_exs_sample("kick.wav", &exs_dir),
            Some(wav.clone())
        );

        // Extension fallback from .aif to .wav
        assert_eq!(
            resolve_exs_sample("kick.aif", &exs_dir),
            Some(wav)
        );
    }
}
