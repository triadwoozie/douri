//! File > Open Recent: the projects last opened or saved, newest first. Kept in a plain text
//! file (one path per line) in the user's config folder, so it survives restarts.
use std::path::{Path, PathBuf};

/// How many projects the menu remembers.
const MAX: usize = 10;

/// `<config folder>/douri/recent.txt`.
fn default_file() -> Option<PathBuf> {
    Some(config_dir()?.join("recent.txt"))
}

fn config_root() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))
    }
}

/// `<config folder>/douri`: %APPDATA% on Windows, ~/Library/Application Support on macOS,
/// $XDG_CONFIG_HOME or ~/.config elsewhere. `None` when no home folder is known.
pub(crate) fn config_dir() -> Option<PathBuf> {
    Some(config_root()?.join("douri"))
}

/// Legacy `<config folder>/prowave` directory for one-time migration.
pub(crate) fn legacy_prowave_config_dir() -> Option<PathBuf> {
    Some(config_root()?.join("prowave"))
}

const MIGRATION_MARKER: &str = "migration-done";
const MIGRATE_FILES: &[&str] = &[
    "recent.txt",
    "audio.txt",
    "keymap.txt",
    "language.txt",
    "ui-scale.txt",
    "plugins.txt",
    "plugin-paths.txt",
    "sound-library.txt",
    "sound_library.json",
    "loop-folder",
    "nudge-value",
    "audio-input",
];

pub fn migration_needed_in(douri_dir: &Path, prowave_dir: &Path) -> bool {
    if !prowave_dir.is_dir() || douri_dir.join(MIGRATION_MARKER).is_file() {
        return false;
    }
    MIGRATE_FILES
        .iter()
        .any(|name| prowave_dir.join(name).is_file())
}

pub fn migration_needed() -> bool {
    let (Some(douri_dir), Some(prowave_dir)) = (config_dir(), legacy_prowave_config_dir()) else {
        return false;
    };
    migration_needed_in(&douri_dir, &prowave_dir)
}

pub fn dismiss_migration_in(douri_dir: &Path) {
    let _ = std::fs::create_dir_all(douri_dir);
    let _ = std::fs::write(douri_dir.join(MIGRATION_MARKER), b"dismissed\n");
}

pub fn dismiss_migration() {
    if let Some(douri_dir) = config_dir() {
        dismiss_migration_in(&douri_dir);
    }
}

pub fn migrate_from_prowave_in(douri_dir: &Path, prowave_dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(douri_dir).map_err(|e| e.to_string())?;
    let mut copied = 0usize;
    for name in MIGRATE_FILES {
        let src = prowave_dir.join(name);
        let dst = douri_dir.join(name);
        if src.is_file() && !dst.exists() {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
            copied += 1;
        }
    }
    let _ = std::fs::write(douri_dir.join(MIGRATION_MARKER), b"migrated\n");
    Ok(copied)
}

pub fn migrate_from_prowave() -> Result<usize, String> {
    let douri_dir = config_dir().ok_or_else(|| "No config directory".to_string())?;
    let prowave_dir =
        legacy_prowave_config_dir().ok_or_else(|| "No Pro-Wave directory".to_string())?;
    migrate_from_prowave_in(&douri_dir, &prowave_dir)
}

/// The recent-projects list. `Recent::default()` is an in-memory list that never touches disk
/// (used by tests); [`Recent::load`] is the real, persistent one.
#[derive(Default)]
pub struct Recent {
    paths: Vec<PathBuf>,
    file: Option<PathBuf>,
}

impl Recent {
    /// Reads the saved list, leaving out projects that no longer exist.
    pub fn load() -> Self {
        Self::from_file(default_file())
    }

    fn from_file(file: Option<PathBuf>) -> Self {
        let text = file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .unwrap_or_default();
        let paths = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .take(MAX)
            .collect();
        Self { paths, file }
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Moves `path` to the top (adding it if new) and saves.
    pub fn add(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.to_path_buf());
        self.paths.truncate(MAX);
        self.save();
    }

    /// Drops `path`, e.g. after it turned out to be unreadable.
    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.save();
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.save();
    }

    /// Best effort: a list that cannot be saved is not worth interrupting the user for.
    fn save(&self) {
        let Some(file) = &self.file else { return };
        let text: String = self
            .paths
            .iter()
            .filter_map(|p| p.to_str())
            .map(|p| format!("{p}\n"))
            .collect();
        let result = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(file, text));
        if let Err(e) = result {
            tracing::warn!("could not save the recent projects list: {e}");
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn newest_first_without_duplicates_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Recent::default();
        let files: Vec<PathBuf> = (0..12)
            .map(|i| touch(dir.path(), &format!("p{i}.prowave")))
            .collect();
        for f in &files {
            r.add(f);
        }
        assert_eq!(r.paths().len(), MAX);
        assert_eq!(r.paths()[0], files[11]);
        r.add(&files[5]);
        assert_eq!(r.paths()[0], files[5]);
        assert_eq!(r.paths().iter().filter(|p| **p == files[5]).count(), 1);
        r.remove(&files[5]);
        assert!(!r.paths().contains(&files[5]));
        r.clear();
        assert!(r.paths().is_empty());
    }

    #[test]
    fn survives_a_restart_and_forgets_deleted_projects() {
        let dir = tempfile::tempdir().unwrap();
        let list = dir.path().join("config").join("prowave").join("recent.txt");
        let (a, b) = (
            touch(dir.path(), "a.prowave"),
            touch(dir.path(), "b.prowave"),
        );
        let mut r = Recent::from_file(Some(list.clone()));
        r.add(&a);
        r.add(&b);

        let reloaded = Recent::from_file(Some(list.clone()));
        assert_eq!(reloaded.paths(), [b.clone(), a.clone()]);

        std::fs::remove_file(&b).unwrap();
        assert_eq!(Recent::from_file(Some(list)).paths(), [a]);
    }

    #[test]
    fn a_missing_or_unwritable_list_is_just_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Recent::from_file(Some(dir.path().join("nope.txt")))
            .paths()
            .is_empty());
        // The "file" is really a folder, so saving fails; that must not panic.
        let mut r = Recent::from_file(Some(dir.path().to_path_buf()));
        r.add(&touch(dir.path(), "a.prowave"));
        assert_eq!(r.paths().len(), 1);
    }

    #[test]
    fn migration_copies_prowave_config_and_respects_dismiss_flag() {
        let root = tempfile::tempdir().unwrap();
        let prowave = root.path().join("prowave");
        let douri = root.path().join("douri");
        std::fs::create_dir_all(&prowave).unwrap();
        assert!(!migration_needed_in(&douri, &prowave));

        std::fs::write(prowave.join("recent.txt"), "/tmp/song.douri\n").unwrap();
        std::fs::write(prowave.join("audio.txt"), "48000\n").unwrap();
        assert!(migration_needed_in(&douri, &prowave));

        let copied = migrate_from_prowave_in(&douri, &prowave).unwrap();
        assert_eq!(copied, 2);
        assert!(douri.join("recent.txt").is_file());
        assert!(douri.join("audio.txt").is_file());
        assert!(!migration_needed_in(&douri, &prowave));

        let douri2 = root.path().join("douri2");
        assert!(migration_needed_in(&douri2, &prowave));
        dismiss_migration_in(&douri2);
        assert!(!migration_needed_in(&douri2, &prowave));
    }
}
