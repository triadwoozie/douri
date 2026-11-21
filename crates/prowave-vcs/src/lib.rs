//! `prowave-vcs`: Native Collaborate version history and project alternatives engine for Dōri.
//!
//! Provides a pure-Rust, CLI-backed Git wrapper (zero C / OpenSSL / libgit2 dependencies)
//! with background threading (`VcsWorker`), DAW-aware `.gitignore` scaffolding, smart Studio Link
//! URL normalization, and a musical diff engine comparing `ProjectDoc` states.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub timestamp: i64,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoStatus {
    pub current_branch: String,
    pub is_clean: bool,
    pub untracked_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MusicalChange {
    TrackAdded {
        name: String,
        kind: String,
    },
    TrackRemoved {
        name: String,
        kind: String,
    },
    RegionModified {
        track: String,
        name: String,
        note_delta: i32,
        details: String,
    },
    TempoChanged {
        old: f32,
        new: f32,
    },
    MixerChanged {
        track: String,
        param: String,
        delta: String,
    },
}

impl MusicalChange {
    pub fn description(&self) -> String {
        match self {
            Self::TrackAdded { name, kind } => format!("Added track \"{name}\" ({kind})"),
            Self::TrackRemoved { name, kind } => format!("Removed track \"{name}\" ({kind})"),
            Self::RegionModified {
                track,
                name,
                note_delta,
                details,
            } => {
                if *note_delta != 0 {
                    let sign = if *note_delta > 0 { "+" } else { "" };
                    format!("Region \"{name}\" on {track}: {sign}{note_delta} notes ({details})")
                } else {
                    format!("Region \"{name}\" on {track}: {details}")
                }
            }
            Self::TempoChanged { old, new } => format!("Tempo changed: {old:.1} -> {new:.1} BPM"),
            Self::MixerChanged {
                track,
                param,
                delta,
            } => format!("{track} {param}: {delta}"),
        }
    }
}

/// Normalizes any user-pasted browser or Git repository link into a valid clone/remote URL.
///
/// Accepts:
/// - Shorthand: `alice/sunset-groove` -> `https://github.com/alice/sunset-groove.git`
/// - Domain without scheme: `github.com/alice/sunset-groove` -> `https://github.com/alice/sunset-groove.git`
/// - Browser subpaths: `https://github.com/alice/sunset-groove/tree/main/src` -> `https://github.com/alice/sunset-groove.git`
/// - Issues/PRs subpaths: `https://github.com/alice/sunset-groove/pulls` -> `https://github.com/alice/sunset-groove.git`
/// - GitLab / Codeberg / self-hosted URLs
/// - Standard SSH URLs: `git@github.com:alice/sunset-groove.git` (preserved as-is)
pub fn normalize_studio_link(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Studio link cannot be empty".into());
    }

    // Preserve valid SSH URLs
    if trimmed.starts_with("git@") {
        if trimmed.contains(':') && (trimmed.contains('/') || trimmed.ends_with(".git")) {
            return Ok(trimmed.to_string());
        }
        return Err("Invalid SSH Git URL format".into());
    }

    // Determine scheme and base content
    let (scheme, without_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https://", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http://", rest)
    } else {
        ("https://", trimmed)
    };

    let cleaned = without_scheme.trim_end_matches('/');

    // Split host and path
    let (host, path) = if cleaned.contains('/') {
        let parts: Vec<&str> = cleaned.splitn(2, '/').collect();
        let possible_host = parts[0];
        if possible_host.contains('.') {
            (possible_host, parts[1])
        } else {
            // Shorthand owner/repo without host defaults to github.com
            ("github.com", cleaned)
        }
    } else {
        return Err(
            "Studio link must include owner and project name (e.g. username/project)".into(),
        );
    };

    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if path_parts.len() < 2 {
        return Err("Studio link must include both owner and project name".into());
    }

    let owner = path_parts[0];
    let mut repo = path_parts[1];

    if let Some(r) = repo.strip_suffix(".git") {
        repo = r;
    }

    Ok(format!("{scheme}{host}/{owner}/{repo}.git"))
}

/// Translates low-level Git CLI stderr messages into concise, studio-friendly explanations
/// with zero technical Git jargon or CLI flags.
pub fn map_studio_error(raw: &str) -> String {
    let lower = raw.to_lowercase();

    if lower.contains("no tracking information") || lower.contains("set-upstream") {
        "This alternative hasn't been shared yet — click Share Updates first.".into()
    } else if lower.contains("could not read from remote repository")
        || lower.contains("could not resolve host")
        || lower.contains("repository not found")
        || lower.contains("please make sure you have the correct access rights")
    {
        "Unable to reach studio link. Please verify your internet connection and repository link permissions.".into()
    } else if lower.contains("permission denied (publickey)")
        || lower.contains("authentication failed")
        || lower.contains("terminal prompts disabled")
    {
        "Permission denied. Please verify your studio credentials or SSH keys.".into()
    } else if lower.contains("non-fast-forward")
        || lower.contains("fetch first")
        || lower.contains("failed to push some refs")
    {
        "Updates exist online. Click Get Updates before sharing your changes.".into()
    } else if lower.contains("conflict") || lower.contains("automatic merge failed") {
        "Conflicting edits found with remote version. Please review recent edits.".into()
    } else if lower.contains("not a git repository") {
        "Project history is not enabled. Click Enable Project History to start.".into()
    } else if lower.contains("no such remote") || lower.contains("remote origin does not exist") {
        "No studio link configured. Click Studio Link… to connect.".into()
    } else if lower.contains("nothing to commit") {
        "No unsaved musical changes to save.".into()
    } else if lower.contains("already exists") {
        "An alternative with that name already exists.".into()
    } else {
        let trimmed = raw.trim();
        let cleaned = trimmed
            .strip_prefix("fatal:")
            .or_else(|| trimmed.strip_prefix("error:"))
            .unwrap_or(trimmed)
            .trim();

        let first_line = cleaned.lines().next().unwrap_or(cleaned).trim();
        if first_line.is_empty() {
            "An error occurred while syncing updates.".into()
        } else if first_line.contains("git ") {
            "Unable to sync updates with studio link. Please verify repository settings.".into()
        } else {
            let mut s = first_line.to_string();
            if !s.ends_with('.') && !s.ends_with('!') && !s.ends_with('?') {
                s.push('.');
            }
            s
        }
    }
}

/// Checks whether a given directory is part of the Dōri DAW source tree.
/// Prevents the DAW's own source repository from being mistaken for a song project repository.
pub fn is_daw_source_dir(dir: &Path) -> bool {
    let mut curr = Some(dir);
    while let Some(d) = curr {
        if d.join("crates/prowave-app").exists()
            || (d.join("Cargo.toml").exists() && d.join("xtask").exists())
        {
            return true;
        }
        curr = d.parent();
    }
    false
}

pub fn scaffold_gitignore(repo_path: &Path) -> std::io::Result<()> {
    let ignore_path = repo_path.join(".gitignore");
    let content = "\
# Dōri DAW transient audio caches & temp files
Cache/utau_renders/*.wav
Cache/peaks/*.peak
Cache/
*.douri.tmp
*.prowave.tmp
*.peak
*.tmp
*.bak
.DS_Store
Thumbs.db
";
    if !ignore_path.exists() {
        std::fs::write(&ignore_path, content)?;
    } else {
        let existing = std::fs::read_to_string(&ignore_path)?;
        let mut to_append = String::new();
        for line in [
            "Cache/utau_renders/*.wav",
            "Cache/peaks/*.peak",
            "Cache/",
            "*.douri.tmp",
            "*.prowave.tmp",
            "*.peak",
            "*.tmp",
            "*.bak",
        ] {
            if !existing.contains(line) {
                to_append.push_str(line);
                to_append.push('\n');
            }
        }
        if !to_append.is_empty() {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&ignore_path)?;
            use std::io::Write;
            file.write_all(to_append.as_bytes())?;
        }
    }
    Ok(())
}

pub fn musical_diff(
    old_doc: Option<&prowave_project::ProjectDoc>,
    new_doc: &prowave_project::ProjectDoc,
) -> Vec<MusicalChange> {
    let mut changes = Vec::new();

    let Some(old) = old_doc else {
        if (new_doc.tempo - 120.0).abs() > 0.01 {
            changes.push(MusicalChange::TempoChanged {
                old: 120.0,
                new: new_doc.tempo,
            });
        }
        for track in &new_doc.tracks {
            changes.push(MusicalChange::TrackAdded {
                name: track.name.clone(),
                kind: format!("{:?}", track.kind),
            });
        }
        return changes;
    };

    if (old.tempo - new_doc.tempo).abs() > 0.01 {
        changes.push(MusicalChange::TempoChanged {
            old: old.tempo,
            new: new_doc.tempo,
        });
    }

    // Compare tracks
    let mut matched_old = vec![false; old.tracks.len()];

    for new_t in &new_doc.tracks {
        let old_match_idx = old
            .tracks
            .iter()
            .enumerate()
            .position(|(i, ot)| !matched_old[i] && ot.name == new_t.name);

        if let Some(idx) = old_match_idx {
            matched_old[idx] = true;
            let old_t = &old.tracks[idx];

            // Mixer changes
            if (old_t.volume_db - new_t.volume_db).abs() > 0.1 {
                changes.push(MusicalChange::MixerChanged {
                    track: new_t.name.clone(),
                    param: "Volume".into(),
                    delta: format!("{:.1} dB -> {:.1} dB", old_t.volume_db, new_t.volume_db),
                });
            }
            if (old_t.pan - new_t.pan).abs() > 0.05 {
                changes.push(MusicalChange::MixerChanged {
                    track: new_t.name.clone(),
                    param: "Pan".into(),
                    delta: format!("{:.2} -> {:.2}", old_t.pan, new_t.pan),
                });
            }
            if old_t.mute != new_t.mute {
                changes.push(MusicalChange::MixerChanged {
                    track: new_t.name.clone(),
                    param: "Mute".into(),
                    delta: format!("{} -> {}", old_t.mute, new_t.mute),
                });
            }
            if old_t.solo != new_t.solo {
                changes.push(MusicalChange::MixerChanged {
                    track: new_t.name.clone(),
                    param: "Solo".into(),
                    delta: format!("{} -> {}", old_t.solo, new_t.solo),
                });
            }

            // Region changes
            let mut matched_old_reg = vec![false; old_t.regions.len()];
            for new_r in &new_t.regions {
                let reg_match_idx = old_t
                    .regions
                    .iter()
                    .enumerate()
                    .position(|(ri, or)| !matched_old_reg[ri] && or.name == new_r.name);

                if let Some(ri) = reg_match_idx {
                    matched_old_reg[ri] = true;
                    let old_r = &old_t.regions[ri];

                    let note_delta = (new_r.notes.len() as i32) - (old_r.notes.len() as i32);
                    let mut details = Vec::new();

                    if (old_r.start_bar - new_r.start_bar).abs() > 0.01 {
                        details.push(format!(
                            "start bar {:.1} -> {:.1}",
                            old_r.start_bar, new_r.start_bar
                        ));
                    }
                    if (old_r.len_bars - new_r.len_bars).abs() > 0.01 {
                        details.push(format!(
                            "length {:.1} -> {:.1} bars",
                            old_r.len_bars, new_r.len_bars
                        ));
                    }
                    if old_r.mute != new_r.mute {
                        details.push(format!("mute: {}", new_r.mute));
                    }
                    if old_r.transpose != new_r.transpose {
                        details.push(format!("transpose: {} st", new_r.transpose));
                    }
                    if note_delta != 0 {
                        details.push(format!("{note_delta:+} notes"));
                    } else if old_r.notes != new_r.notes {
                        details.push("notes modified".into());
                    }

                    if !details.is_empty() {
                        changes.push(MusicalChange::RegionModified {
                            track: new_t.name.clone(),
                            name: new_r.name.clone(),
                            note_delta,
                            details: details.join(", "),
                        });
                    }
                } else {
                    // Added region
                    changes.push(MusicalChange::RegionModified {
                        track: new_t.name.clone(),
                        name: new_r.name.clone(),
                        note_delta: new_r.notes.len() as i32,
                        details: format!(
                            "added (+{} notes, len {:.1} bars)",
                            new_r.notes.len(),
                            new_r.len_bars
                        ),
                    });
                }
            }

            for (ri, &matched) in matched_old_reg.iter().enumerate() {
                if !matched {
                    let old_r = &old_t.regions[ri];
                    changes.push(MusicalChange::RegionModified {
                        track: new_t.name.clone(),
                        name: old_r.name.clone(),
                        note_delta: -(old_r.notes.len() as i32),
                        details: "deleted".into(),
                    });
                }
            }
        } else {
            changes.push(MusicalChange::TrackAdded {
                name: new_t.name.clone(),
                kind: format!("{:?}", new_t.kind),
            });
        }
    }

    for (i, &matched) in matched_old.iter().enumerate() {
        if !matched {
            let old_t = &old.tracks[i];
            changes.push(MusicalChange::TrackRemoved {
                name: old_t.name.clone(),
                kind: format!("{:?}", old_t.kind),
            });
        }
    }

    changes
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VcsUiState {
    pub is_repo: bool,
    pub current_branch: String,
    pub branches: Vec<BranchInfo>,
    pub status: RepoStatus,
    pub commits: Vec<CommitInfo>,
    pub changes: Vec<MusicalChange>,
    pub remote_url: Option<String>,
    pub is_busy: bool,
    pub error_msg: Option<String>,
    pub notice_msg: Option<String>,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug)]
pub struct GitRepo {
    pub work_dir: PathBuf,
}

#[cfg(target_arch = "wasm32")]
impl GitRepo {
    pub fn open(_path: &Path) -> Option<Self> {
        None
    }
    pub fn init(_path: &Path) -> Result<Self, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn status(&self) -> Result<RepoStatus, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn branches(&self) -> Result<Vec<BranchInfo>, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn create_branch(&self, _name: &str) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn checkout(&self, _target: &str) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn create_and_checkout(&self, _name: &str) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn create_and_checkout_from(&self, _name: &str, _from_rev: &str) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn commit(&self, _msg: &str) -> Result<String, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn log(&self, _limit: usize) -> Result<Vec<CommitInfo>, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn fetch(&self) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn pull(&self) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn push(&self) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn reset_hard(&self) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn remote_url(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    pub fn set_remote_url(&self, _url: &str) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn remove_remote(&self) -> Result<(), String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn cat_file(&self, _rev: &str, _file: &str) -> Result<Vec<u8>, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn load_project_from_commit(
        &self,
        _rev: &str,
        _project_rel_path: &Path,
    ) -> Result<prowave_project::ProjectDoc, String> {
        Err("Collaborate is unsupported on wasm32".into())
    }
    pub fn find_project_file(&self) -> Option<PathBuf> {
        None
    }
}

#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct GitRepo {
    pub work_dir: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl GitRepo {
    fn git_cmd(&self) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.work_dir);
        // Ensure user identity is provided even if host machine lacks a global git config
        cmd.arg("-c").arg("user.name=Dōri Studio");
        cmd.arg("-c").arg("user.email=studio@douri.local");
        cmd
    }

    pub fn open(path: &Path) -> Option<Self> {
        let work_dir = if path.is_file() {
            path.parent()?.to_path_buf()
        } else {
            path.to_path_buf()
        };

        if is_daw_source_dir(&work_dir) {
            return None;
        }

        let mut cmd = Command::new("git");
        cmd.current_dir(&work_dir);
        cmd.args(["rev-parse", "--is-inside-work-tree"]);

        match cmd.output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim() == "true" {
                    Some(Self { work_dir })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn init(path: &Path) -> Result<Self, String> {
        let work_dir = if path.is_file() {
            path.parent().unwrap_or(path).to_path_buf()
        } else {
            path.to_path_buf()
        };

        if is_daw_source_dir(&work_dir) {
            return Err("Cannot initialize Collaborate in the DAW application directory. Save your project in its own folder.".into());
        }

        if !work_dir.exists() {
            std::fs::create_dir_all(&work_dir)
                .map_err(|e| format!("Cannot create directory {}: {e}", work_dir.display()))?;
        }

        let mut cmd = Command::new("git");
        cmd.current_dir(&work_dir);
        cmd.args(["init", "-b", "main"]);

        let output = cmd
            .output()
            .or_else(|_| {
                let mut fallback = Command::new("git");
                fallback.current_dir(&work_dir);
                fallback.arg("init");
                fallback.output()
            })
            .map_err(|e| format!("Failed to run git init: {e}"))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }

        scaffold_gitignore(&work_dir).map_err(|e| format!("Failed to scaffold .gitignore: {e}"))?;

        Ok(Self { work_dir })
    }

    pub fn status(&self) -> Result<RepoStatus, String> {
        let mut cmd = self.git_cmd();
        cmd.args(["status", "--porcelain=v2", "--branch"]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git status: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut status = RepoStatus::default();

        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("# branch.head ") {
                status.current_branch = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(ahead) = parts[0].trim_start_matches('+').parse::<usize>() {
                        status.ahead = ahead;
                    }
                    if let Ok(behind) = parts[1].trim_start_matches('-').parse::<usize>() {
                        status.behind = behind;
                    }
                }
            } else if let Some(path) = line.strip_prefix("? ") {
                status.untracked_files.push(path.trim().to_string());
            } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(path) = parts.last() {
                    status.modified_files.push(path.to_string());
                }
            }
        }

        status.is_clean = status.untracked_files.is_empty() && status.modified_files.is_empty();
        Ok(status)
    }

    pub fn branches(&self) -> Result<Vec<BranchInfo>, String> {
        let mut cmd = self.git_cmd();
        cmd.args([
            "branch",
            "--format=%(refname:short)|%(HEAD)|%(upstream:short)",
        ]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to list alternatives: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut branches = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.is_empty() || parts[0].trim().is_empty() {
                continue;
            }
            let name = parts[0].trim().to_string();
            let is_head = parts.get(1).is_some_and(|h| h.trim() == "*");
            let upstream = parts.get(2).and_then(|u| {
                let trimmed = u.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });

            branches.push(BranchInfo {
                name,
                is_head,
                upstream,
            });
        }

        Ok(branches)
    }

    pub fn create_branch(&self, name: &str) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.args(["branch", name]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to create alternative: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    }

    pub fn checkout(&self, target: &str) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.args(["checkout", target]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to open version: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    }

    pub fn create_and_checkout(&self, name: &str) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.args(["checkout", "-b", name]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to create alternative: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    }

    pub fn create_and_checkout_from(&self, name: &str, from_rev: &str) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.args(["checkout", "-b", name, from_rev]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to create alternative from version: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    }

    pub fn commit(&self, msg: &str) -> Result<String, String> {
        let mut add_cmd = self.git_cmd();
        add_cmd.args(["add", "-A"]);
        let add_out = add_cmd
            .output()
            .map_err(|e| format!("Staging failed: {e}"))?;
        if !add_out.status.success() {
            return Err(String::from_utf8_lossy(&add_out.stderr).into_owned());
        }

        let mut commit_cmd = self.git_cmd();
        commit_cmd.args(["commit", "-m", msg]);
        let commit_out = commit_cmd
            .output()
            .map_err(|e| format!("Saving version failed: {e}"))?;
        if !commit_out.status.success() {
            return Err(String::from_utf8_lossy(&commit_out.stderr).into_owned());
        }

        let mut rev_cmd = self.git_cmd();
        rev_cmd.args(["rev-parse", "--short", "HEAD"]);
        let rev_out = rev_cmd
            .output()
            .map_err(|e| format!("rev-parse failed: {e}"))?;
        Ok(String::from_utf8_lossy(&rev_out.stdout).trim().to_string())
    }

    pub fn log(&self, limit: usize) -> Result<Vec<CommitInfo>, String> {
        let mut cmd = self.git_cmd();
        let limit_arg = format!("-n{limit}");
        cmd.args(["log", &limit_arg, "--format=%H|%h|%an|%at|%s"]);

        let output = cmd
            .output()
            .map_err(|e| format!("Reading history failed: {e}"))?;
        if !output.status.success() {
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut commits = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 5 {
                continue;
            }
            let hash = parts[0].to_string();
            let short_hash = parts[1].to_string();
            let author = parts[2].to_string();
            let timestamp = parts[3].parse::<i64>().unwrap_or(0);
            let message = parts[4].to_string();

            commits.push(CommitInfo {
                hash,
                short_hash,
                author,
                timestamp,
                message,
            });
        }

        Ok(commits)
    }

    pub fn fetch(&self) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.arg("fetch");
        let out = cmd
            .output()
            .map_err(|e| format!("Fetching updates failed: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    pub fn current_branch(&self) -> Result<String, String> {
        self.status().map(|s| s.current_branch)
    }

    pub fn pull(&self) -> Result<(), String> {
        let current_branch = self.current_branch().unwrap_or_else(|_| "main".to_string());

        // Check if an upstream tracking branch is already configured
        let mut check_upstream = self.git_cmd();
        check_upstream.args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
        let upstream_out = check_upstream.output();
        let has_upstream = upstream_out.as_ref().is_ok_and(|o| o.status.success());

        if !has_upstream {
            // Fetch origin first to see if origin has this branch
            let _ = self.fetch();

            let mut verify_cmd = self.git_cmd();
            verify_cmd.args(["rev-parse", "--verify", &format!("origin/{current_branch}")]);
            let verify_out = verify_cmd.output();
            let exists_on_origin = verify_out.as_ref().is_ok_and(|o| o.status.success());

            if exists_on_origin {
                let mut set_up = self.git_cmd();
                set_up.args([
                    "branch",
                    &format!("--set-upstream-to=origin/{current_branch}"),
                    &current_branch,
                ]);
                let _ = set_up.output();
            } else {
                return Err(
                    "This alternative hasn't been shared yet — click Share Updates first.".into(),
                );
            }
        }

        let mut cmd = self.git_cmd();
        cmd.arg("pull");
        let out = cmd
            .output()
            .map_err(|e| map_studio_error(&format!("Getting updates failed: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            let err_raw = String::from_utf8_lossy(&out.stderr);
            Err(map_studio_error(&err_raw))
        }
    }

    pub fn push(&self) -> Result<(), String> {
        let current_branch = self.current_branch().unwrap_or_else(|_| "main".to_string());
        let mut cmd = self.git_cmd();
        cmd.args(["push", "-u", "origin", &current_branch]);
        let out = cmd
            .output()
            .map_err(|e| map_studio_error(&format!("Sharing updates failed: {e}")))?;
        if out.status.success() {
            Ok(())
        } else {
            let err_raw = String::from_utf8_lossy(&out.stderr);
            Err(map_studio_error(&err_raw))
        }
    }

    pub fn reset_hard(&self) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.args(["reset", "--hard", "HEAD"]);
        let out = cmd
            .output()
            .map_err(|e| format!("Discarding unsaved edits failed: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }

        let mut clean_cmd = self.git_cmd();
        clean_cmd.args(["clean", "-fd"]);
        let clean_out = clean_cmd
            .output()
            .map_err(|e| format!("Clean failed: {e}"))?;
        if clean_out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&clean_out.stderr).into_owned())
        }
    }

    pub fn remote_url(&self) -> Result<Option<String>, String> {
        let mut cmd = self.git_cmd();
        cmd.args(["remote", "get-url", "origin"]);
        match cmd.output() {
            Ok(out) if out.status.success() => {
                let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if url.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(url))
                }
            }
            _ => Ok(None),
        }
    }

    pub fn set_remote_url(&self, url: &str) -> Result<(), String> {
        let mut check_cmd = self.git_cmd();
        check_cmd.args(["remote"]);
        let has_origin = match check_cmd.output() {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.lines().any(|l| l.trim() == "origin")
            }
            _ => false,
        };

        let mut cmd = self.git_cmd();
        if has_origin {
            cmd.args(["remote", "set-url", "origin", url]);
        } else {
            cmd.args(["remote", "add", "origin", url]);
        }

        let out = cmd
            .output()
            .map_err(|e| format!("Failed to set studio link: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    pub fn remove_remote(&self) -> Result<(), String> {
        let mut cmd = self.git_cmd();
        cmd.args(["remote", "remove", "origin"]);
        let out = cmd
            .output()
            .map_err(|e| format!("Failed to remove studio link: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    pub fn cat_file(&self, rev: &str, file: &str) -> Result<Vec<u8>, String> {
        let spec = format!("{rev}:{file}");
        let mut cmd = self.git_cmd();
        cmd.args(["show", &spec]);
        let out = cmd
            .output()
            .map_err(|e| format!("Reading version failed: {e}"))?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    pub fn find_project_file(&self) -> Option<PathBuf> {
        let entries = std::fs::read_dir(&self.work_dir).ok()?;
        let mut douri_file = None;
        let mut prowave_file = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("douri") {
                        douri_file = Some(path);
                        break;
                    } else if ext.eq_ignore_ascii_case("prowave") && prowave_file.is_none() {
                        prowave_file = Some(path);
                    }
                }
            }
        }
        douri_file.or(prowave_file)
    }

    pub fn load_project_from_commit(
        &self,
        rev: &str,
        project_rel_path: &Path,
    ) -> Result<prowave_project::ProjectDoc, String> {
        let path_str = project_rel_path.to_string_lossy();
        let bytes = self.cat_file(rev, &path_str).or_else(|_| {
            if path_str.ends_with(".douri") {
                let fallback = project_rel_path.with_extension("prowave");
                self.cat_file(rev, &fallback.to_string_lossy())
            } else if path_str.ends_with(".prowave") {
                let fallback = project_rel_path.with_extension("douri");
                self.cat_file(rev, &fallback.to_string_lossy())
            } else {
                Err("File not found in version".into())
            }
        })?;

        if path_str.ends_with(".json") {
            let json = String::from_utf8(bytes).map_err(|e| e.to_string())?;
            serde_json::from_str(&json).map_err(|e| e.to_string())
        } else {
            use std::io::Read;
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
                .map_err(|e| format!("Invalid project archive: {e}"))?;
            let mut file = archive
                .by_name(prowave_project::PROJECT_JSON)
                .map_err(|e| format!("Missing project document in version archive: {e}"))?;
            let mut json = String::new();
            file.read_to_string(&mut json)
                .map_err(|e| format!("Read failed: {e}"))?;
            serde_json::from_str(&json).map_err(|e| e.to_string())
        }
    }
}

// Background Worker & Messaging
pub enum VcsTask {
    Refresh {
        current_doc: Option<prowave_project::ProjectDoc>,
        project_file_rel: Option<PathBuf>,
    },
    Commit {
        message: String,
        current_doc: Option<prowave_project::ProjectDoc>,
        project_file_rel: Option<PathBuf>,
    },
    QuickCheckpoint {
        current_doc: Option<prowave_project::ProjectDoc>,
        project_file_rel: Option<PathBuf>,
    },
    CreateBranch {
        name: String,
    },
    CreateBranchFrom {
        name: String,
        from_rev: String,
    },
    Checkout {
        target: String,
    },
    Pull,
    Push,
    RevertUnsaved,
    Init,
    SetRemote {
        url: String,
    },
    RemoveRemote,
}

pub enum VcsEvent {
    StateUpdated(Box<VcsUiState>),
    CheckoutCompleted(String),
    Error(String),
    Notice(String),
}

pub struct VcsWorker {
    task_tx: std::sync::mpsc::Sender<VcsTask>,
    event_rx: std::sync::mpsc::Receiver<VcsEvent>,
    pub repo_dir: PathBuf,
    pub latest_state: VcsUiState,
}

impl VcsWorker {
    pub fn new(repo_dir: PathBuf) -> Self {
        let (task_tx, task_rx) = std::sync::mpsc::channel::<VcsTask>();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<VcsEvent>();

        let thread_repo_dir = repo_dir.clone();
        std::thread::Builder::new()
            .name("douri-collaborate-worker".into())
            .spawn(move || {
                let mut repo = GitRepo::open(&thread_repo_dir);

                while let Ok(task) = task_rx.recv() {
                    match task {
                        VcsTask::Init => match GitRepo::init(&thread_repo_dir) {
                            Ok(r) => {
                                repo = Some(r);
                                let _ = event_tx
                                    .send(VcsEvent::Notice("Project history initialized".into()));
                                Self::send_refresh(&repo, &event_tx, None, None);
                            }
                            Err(e) => {
                                let _ = event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                            }
                        },
                        VcsTask::Refresh {
                            current_doc,
                            project_file_rel,
                        } => {
                            if repo.is_none() {
                                repo = GitRepo::open(&thread_repo_dir);
                            }
                            let project_file_rel = project_file_rel.or_else(|| {
                                repo.as_ref()
                                    .and_then(|r| r.find_project_file())
                                    .and_then(|p| p.file_name().map(PathBuf::from))
                            });
                            Self::send_refresh(
                                &repo,
                                &event_tx,
                                current_doc.as_ref(),
                                project_file_rel.as_deref(),
                            );
                        }
                        VcsTask::Commit {
                            message,
                            current_doc,
                            project_file_rel,
                        } => {
                            if let Some(ref r) = repo {
                                let project_file_rel = project_file_rel.or_else(|| {
                                    r.find_project_file()
                                        .and_then(|p| p.file_name().map(PathBuf::from))
                                });
                                match r.commit(&message) {
                                    Ok(hash) => {
                                        let _ = event_tx.send(VcsEvent::Notice(format!(
                                            "Version saved ({hash})"
                                        )));
                                        Self::send_refresh(
                                            &repo,
                                            &event_tx,
                                            current_doc.as_ref(),
                                            project_file_rel.as_deref(),
                                        );
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::QuickCheckpoint {
                            current_doc,
                            project_file_rel,
                        } => {
                            if let Some(ref r) = repo {
                                let project_file_rel = project_file_rel.or_else(|| {
                                    r.find_project_file()
                                        .and_then(|p| p.file_name().map(PathBuf::from))
                                });
                                let head_doc = project_file_rel
                                    .as_ref()
                                    .and_then(|rel| r.load_project_from_commit("HEAD", rel).ok());

                                let note = if let Some(ref curr) = current_doc {
                                    let diffs = musical_diff(head_doc.as_ref(), curr);
                                    if diffs.is_empty() {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .map(|d| d.as_secs())
                                            .unwrap_or(0);
                                        format!("Quick snapshot [{}]", now)
                                    } else if diffs.len() == 1 {
                                        diffs[0].description()
                                    } else {
                                        format!(
                                            "{}, and {} more edits",
                                            diffs[0].description(),
                                            diffs.len() - 1
                                        )
                                    }
                                } else {
                                    "Quick snapshot".to_string()
                                };

                                match r.commit(&note) {
                                    Ok(hash) => {
                                        let _ = event_tx.send(VcsEvent::Notice(format!(
                                            "Snapshot saved ({hash})"
                                        )));
                                        Self::send_refresh(
                                            &repo,
                                            &event_tx,
                                            current_doc.as_ref(),
                                            project_file_rel.as_deref(),
                                        );
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::CreateBranch { name } => {
                            if let Some(ref r) = repo {
                                match r.create_and_checkout(&name) {
                                    Ok(()) => {
                                        let _ = event_tx.send(VcsEvent::Notice(format!(
                                            "Created and switched to alternative \"{name}\""
                                        )));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::CreateBranchFrom { name, from_rev } => {
                            if let Some(ref r) = repo {
                                match r.create_and_checkout_from(&name, &from_rev) {
                                    Ok(()) => {
                                        let _ = event_tx.send(VcsEvent::Notice(format!(
                                            "Created alternative \"{name}\" from version"
                                        )));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::Checkout { target } => {
                            if let Some(ref r) = repo {
                                match r.checkout(&target) {
                                    Ok(()) => {
                                        let _ = event_tx
                                            .send(VcsEvent::CheckoutCompleted(target.clone()));
                                        let _ = event_tx.send(VcsEvent::Notice(format!(
                                            "Switched to alternative \"{target}\""
                                        )));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::Pull => {
                            if let Some(ref r) = repo {
                                match r.pull() {
                                    Ok(()) => {
                                        let _ = event_tx.send(VcsEvent::Notice(
                                            "Retrieved latest updates from studio link".into(),
                                        ));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::Push => {
                            if let Some(ref r) = repo {
                                match r.push() {
                                    Ok(()) => {
                                        let _ = event_tx.send(VcsEvent::Notice(
                                            "Shared updates to studio link".into(),
                                        ));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::SetRemote { url } => {
                            if let Some(ref r) = repo {
                                match r.set_remote_url(&url) {
                                    Ok(()) => {
                                        let _ = event_tx.send(VcsEvent::Notice(
                                            "Studio link configured successfully".into(),
                                        ));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::RemoveRemote => {
                            if let Some(ref r) = repo {
                                match r.remove_remote() {
                                    Ok(()) => {
                                        let _ = event_tx
                                            .send(VcsEvent::Notice("Studio link removed".into()));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                        VcsTask::RevertUnsaved => {
                            if let Some(ref r) = repo {
                                match r.reset_hard() {
                                    Ok(()) => {
                                        let _ = event_tx.send(VcsEvent::Notice(
                                            "Discarded unsaved edits".into(),
                                        ));
                                        Self::send_refresh(&repo, &event_tx, None, None);
                                    }
                                    Err(e) => {
                                        let _ =
                                            event_tx.send(VcsEvent::Error(map_studio_error(&e)));
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .expect("Failed to spawn douri-collaborate-worker thread");

        let worker = Self {
            task_tx,
            event_rx,
            repo_dir: repo_dir.clone(),
            latest_state: VcsUiState::default(),
        };
        worker.send(VcsTask::Refresh {
            current_doc: None,
            project_file_rel: None,
        });
        worker
    }

    fn send_refresh(
        repo: &Option<GitRepo>,
        tx: &std::sync::mpsc::Sender<VcsEvent>,
        current_doc: Option<&prowave_project::ProjectDoc>,
        project_file_rel: Option<&Path>,
    ) {
        let Some(r) = repo else {
            let _ = tx.send(VcsEvent::StateUpdated(Box::new(VcsUiState {
                is_repo: false,
                ..Default::default()
            })));
            return;
        };

        let status = r.status().unwrap_or_default();
        let branches = r.branches().unwrap_or_default();
        let commits = r.log(30).unwrap_or_default();
        let remote_url = r.remote_url().unwrap_or(None);

        let head_doc =
            project_file_rel.and_then(|rel| r.load_project_from_commit("HEAD", rel).ok());

        let changes = if let Some(curr) = current_doc {
            musical_diff(head_doc.as_ref(), curr)
        } else {
            Vec::new()
        };

        let state = VcsUiState {
            is_repo: true,
            current_branch: status.current_branch.clone(),
            branches,
            status,
            commits,
            changes,
            remote_url,
            is_busy: false,
            error_msg: None,
            notice_msg: None,
        };

        let _ = tx.send(VcsEvent::StateUpdated(Box::new(state)));
    }

    pub fn send(&self, task: VcsTask) {
        let _ = self.task_tx.send(task);
    }

    pub fn poll(&mut self) -> Option<VcsEvent> {
        let mut latest_event = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                VcsEvent::StateUpdated(state) => {
                    self.latest_state = *state.clone();
                }
                VcsEvent::Error(e) => {
                    self.latest_state.error_msg = Some(e.clone());
                    self.latest_state.is_busy = false;
                }
                VcsEvent::Notice(n) => {
                    self.latest_state.notice_msg = Some(n.clone());
                    self.latest_state.is_busy = false;
                }
                _ => {}
            }
            latest_event = Some(event);
        }
        latest_event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_studio_link() {
        // Shorthand owner/repo
        assert_eq!(
            normalize_studio_link("alice/sunset-groove").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        // Domain without scheme
        assert_eq!(
            normalize_studio_link("github.com/alice/sunset-groove").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        // Full https url without .git
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        // Full https url with trailing slash
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove/").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        // Browser subpaths (/tree/main, /blob, /commits, /pulls, /issues)
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove/tree/main/crates/ui")
                .unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove/blob/main/README.md")
                .unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove/commits/main").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove/pulls").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        assert_eq!(
            normalize_studio_link("https://github.com/alice/sunset-groove/issues/12").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        // GitLab / Codeberg
        assert_eq!(
            normalize_studio_link("https://gitlab.com/producer/beats").unwrap(),
            "https://gitlab.com/producer/beats.git"
        );
        assert_eq!(
            normalize_studio_link("codeberg.org/band/album").unwrap(),
            "https://codeberg.org/band/album.git"
        );
        // Valid SSH preserved
        assert_eq!(
            normalize_studio_link("git@github.com:alice/sunset-groove.git").unwrap(),
            "git@github.com:alice/sunset-groove.git"
        );
        // Whitespace trimmed
        assert_eq!(
            normalize_studio_link("   https://github.com/alice/sunset-groove.git  ").unwrap(),
            "https://github.com/alice/sunset-groove.git"
        );
        // Empty / invalid
        assert!(normalize_studio_link("").is_err());
        assert!(normalize_studio_link("just-a-word").is_err());
    }

    #[test]
    fn test_gitignore_scaffolding() {
        let dir = tempfile::tempdir().unwrap();
        scaffold_gitignore(dir.path()).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("Cache/utau_renders/*.wav"));
        assert!(content.contains("Cache/peaks/*.peak"));
        assert!(content.contains("Cache/"));
        assert!(content.contains("*.peak"));
        assert!(content.contains("*.tmp"));

        // Running a second time should not duplicate
        scaffold_gitignore(dir.path()).unwrap();
        let content_after = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(content, content_after);
    }

    #[test]
    fn test_musical_diff_tracks_and_tempo() {
        let mut old = prowave_project::ProjectDoc {
            tempo: 120.0,
            ..Default::default()
        };
        old.tracks.push(prowave_project::TrackDoc {
            name: "Drums".into(),
            volume_db: 0.0,
            ..Default::default()
        });

        let mut new = old.clone();
        new.tempo = 128.0;
        new.tracks[0].volume_db = -3.5;
        new.tracks.push(prowave_project::TrackDoc {
            name: "Bass".into(),
            ..Default::default()
        });

        let diff = musical_diff(Some(&old), &new);
        assert_eq!(diff.len(), 3);

        assert!(diff.iter().any(|c| matches!(
            c,
            MusicalChange::TempoChanged {
                old: 120.0,
                new: 128.0
            }
        )));
        assert!(diff.iter().any(|c| matches!(c, MusicalChange::MixerChanged { track, param, .. } if track == "Drums" && param == "Volume")));
        assert!(diff
            .iter()
            .any(|c| matches!(c, MusicalChange::TrackAdded { name, .. } if name == "Bass")));
    }

    #[test]
    fn test_musical_diff_regions() {
        let mut old = prowave_project::ProjectDoc::default();
        let mut track = prowave_project::TrackDoc {
            name: "Keys".into(),
            ..Default::default()
        };
        track.regions.push(prowave_project::RegionDoc {
            name: "Chords".into(),
            notes: vec![prowave_project::NoteDoc::default()],
            ..Default::default()
        });
        old.tracks.push(track);

        let mut new = old.clone();
        new.tracks[0].regions[0]
            .notes
            .push(prowave_project::NoteDoc::default());

        let diff = musical_diff(Some(&old), &new);
        assert_eq!(diff.len(), 1);
        match &diff[0] {
            MusicalChange::RegionModified {
                track,
                name,
                note_delta,
                ..
            } => {
                assert_eq!(track, "Keys");
                assert_eq!(name, "Chords");
                assert_eq!(*note_delta, 1);
            }
            _ => panic!("Expected RegionModified"),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_git_repo_lifecycle_and_remote() {
        let dir = tempfile::tempdir().unwrap();
        let repo = GitRepo::init(dir.path()).expect("git init failed");

        let status = repo.status().expect("git status failed");
        assert!(status.current_branch == "main" || status.current_branch == "master");

        // Commit initial
        let hash = repo.commit("Initial version").expect("git commit failed");
        assert!(!hash.is_empty());

        let logs = repo.log(5).expect("git log failed");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Initial version");

        // Create alternative
        repo.create_and_checkout("alt-chorus")
            .expect("branch failed");
        let branches = repo.branches().expect("branches failed");
        assert!(branches.iter().any(|b| b.name == "alt-chorus" && b.is_head));

        // Test remote url configuration
        assert_eq!(repo.remote_url().unwrap(), None);
        repo.set_remote_url("https://github.com/producer/beat.git")
            .expect("set remote failed");
        assert_eq!(
            repo.remote_url().unwrap(),
            Some("https://github.com/producer/beat.git".into())
        );

        // Test remote url removal
        repo.remove_remote().expect("remove remote failed");
        assert_eq!(repo.remote_url().unwrap(), None);
    }

    #[test]
    fn test_is_daw_source_dir() {
        let cwd = std::env::current_dir().unwrap();
        // douri repository root or crates/prowave-vcs should be detected as DAW source dir
        assert!(is_daw_source_dir(&cwd));

        let temp = tempfile::tempdir().unwrap();
        assert!(!is_daw_source_dir(temp.path()));
    }

    #[test]
    fn test_map_studio_error() {
        assert_eq!(
            map_studio_error("fatal: There is no tracking information for the current branch."),
            "This alternative hasn't been shared yet — click Share Updates first."
        );
        assert_eq!(
            map_studio_error("fatal: Could not read from remote repository."),
            "Unable to reach studio link. Please verify your internet connection and repository link permissions."
        );
        assert_eq!(
            map_studio_error("error: failed to push some refs to 'origin' (non-fast-forward)"),
            "Updates exist online. Click Get Updates before sharing your changes."
        );
        assert_eq!(
            map_studio_error("CONFLICT (content): Merge conflict in song.prowavedoc"),
            "Conflicting edits found with remote version. Please review recent edits."
        );
        assert_eq!(
            map_studio_error("fatal: not a git repository"),
            "Project history is not enabled. Click Enable Project History to start."
        );
        assert_eq!(
            map_studio_error("fatal: A branch named 'my-alt' already exists"),
            "An alternative with that name already exists."
        );
    }
}
