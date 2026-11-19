//! Recovery copies and the session folder they live in.
//!
//! Every run of the program has one session folder, `<settings folder>/recovery/<session>/`:
//!
//! ```text
//! lock                 there while the program runs (and held with a file lock); removed on
//!                      a clean exit, left behind by a crash
//! recovery.prowave     the recovery copy of the project with unsaved changes: its
//!                      `project.json` only, each audio region naming its file where it is
//! original-path.txt    the project file the copy belongs to (absent: never saved)
//! recordings/…         audio recorded or rendered in this session
//! <project>-<n>/media  audio unpacked from the project files opened in this session
//! ```
//!
//! The recovery copy is never the user's project file and holds no audio: the audio stays in
//! the session folder (or wherever an imported file is), so writing the copy is quick however
//! much audio the project has. The folder is deleted on exit unless the project still has
//! unsaved changes that would otherwise be lost (see `App::prepare_exit`); the next start
//! offers what it finds ([`sweep`]) and deletes what nobody can use any more.
use crate::model::Project;
use prowave_project::{ProjectDoc, ProjectError};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// The recovery copy's name in its session folder.
pub const COPY: &str = "recovery.prowave";
const LOCK: &str = "lock";
const ORIGINAL: &str = "original-path.txt";
const RECORDINGS: &str = "recordings";
/// A recovery copy older than this is deleted with its folder instead of being offered.
pub const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

/// `<settings folder>/recovery` (tests: a folder of their own in the temp folder).
pub fn root() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("prowave-recovery-test-{}", std::process::id()));
    }
    crate::recent_files::config_dir().map_or_else(
        || std::env::temp_dir().join("prowave-recovery"),
        |d| d.join("recovery"),
    )
}

/// One run's folder, locked while the run lasts.
pub struct Session {
    dir: PathBuf,
    /// Holds the file lock on `lock`; the system lets go of it when the process ends, however
    /// it ends.
    lock: Option<File>,
}

impl Session {
    /// Makes a new folder in `root` and locks it.
    pub fn start(root: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let base = format!("{secs}-{}", std::process::id());
        let mut n = 0;
        let dir = loop {
            let name = if n == 0 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let dir = root.join(name);
            match std::fs::create_dir(&dir) {
                Ok(()) => break dir,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => n += 1,
                Err(e) => return Err(e),
            }
        };
        let lock = lock(&dir);
        Ok(Self { dir, lock })
    }

    /// Takes over the folder a run left behind (`None`: another run of the program has it).
    pub fn take_over(dir: &Path) -> Option<Self> {
        if is_live(dir) {
            return None;
        }
        Some(Self {
            dir: dir.to_path_buf(),
            lock: lock(dir),
        })
    }

    /// A session with no folder of its own in the recovery folder (that one could not be
    /// made): its media goes to the temp folder, and nothing of it is kept.
    fn unlocked(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, lock: None }
    }

    #[cfg(test)]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A clean exit. `keep`: the folder stays (it holds a recovery copy the next start will
    /// offer) and only the lock goes; otherwise the folder goes with everything in it.
    pub fn end(self, keep: bool) {
        let Self { dir, lock } = self;
        drop(lock);
        let result = if keep {
            match std::fs::remove_file(dir.join(LOCK)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else if dir.exists() {
            std::fs::remove_dir_all(&dir)
        } else {
            Ok(())
        };
        if let Err(e) = result {
            tracing::warn!("cannot remove {}: {e}", dir.display());
        }
    }

    /// The process dies: the system drops the file lock and everything stays on disk.
    #[cfg(test)]
    pub fn crash(self) -> PathBuf {
        self.dir.clone()
    }
}

/// Writes `dir/lock` and takes the file lock on it.
fn lock(dir: &Path) -> Option<File> {
    let file = File::create(dir.join(LOCK)).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Whether a running program holds the folder's lock.
fn is_live(dir: &Path) -> bool {
    let Ok(file) = File::open(dir.join(LOCK)) else {
        // No lock file: the run ended cleanly (or never got as far as writing it).
        return false;
    };
    match file.try_lock() {
        // Nobody holds it; ours goes when `file` is dropped.
        Ok(()) => false,
        Err(std::fs::TryLockError::WouldBlock) => true,
        // Locks do not work here: the folder's age decides ([`sweep`] still removes it once
        // it is older than [`MAX_AGE`]).
        Err(std::fs::TryLockError::Error(_)) => modified(dir)
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age < MAX_AGE),
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// This run's sessions: the one in use, and folders it has left but may still hold files of
/// the open project in (deleted on exit).
struct Live {
    current: Session,
    retired: Vec<Session>,
    /// What the program decided to do with the folder on exit; undecided, the folder stays
    /// when it holds a recovery copy.
    keep: Option<bool>,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn with_live<R>(f: impl FnOnce(&mut Live) -> R) -> R {
    let mut live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    let live = live.get_or_insert_with(|| {
        let current = Session::start(&root()).unwrap_or_else(|e| {
            tracing::warn!("no recovery folder ({e}); nothing can be recovered after a crash");
            let dir = std::env::temp_dir()
                .join("prowave-media")
                .join(std::process::id().to_string());
            Session::unlocked(dir)
        });
        Live {
            current,
            retired: Vec::new(),
            keep: None,
        }
    });
    f(live)
}

/// This run's session folder (made and locked the first time it is asked for).
pub fn session_dir() -> PathBuf {
    with_live(|l| l.current.dir.clone())
}

/// Where a session keeps the audio recorded or rendered in it.
pub fn recordings_in(session: &Path) -> PathBuf {
    session.join(RECORDINGS)
}

/// The folder a recovered project's files are in becomes this run's session folder, so the
/// paths in the project stay true and new recordings land next to the old ones. False when
/// another run of the program has it.
pub fn adopt(dir: &Path) -> bool {
    let Some(taken) = Session::take_over(dir) else {
        return false;
    };
    with_live(|l| {
        let old = std::mem::replace(&mut l.current, taken);
        l.retired.push(old);
        l.keep = None;
    });
    true
}

/// Whether the session folder stays when the program exits.
pub fn keep_on_exit(keep: bool) {
    with_live(|l| l.keep = Some(keep));
}

/// On exit, after the engine has closed its files: the session folder is deleted, or kept
/// with its recovery copy for the next start.
pub fn end_session() {
    let live = LIVE.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(Live {
        current,
        retired,
        keep,
    }) = live
    else {
        return;
    };
    let keep = current.lock.is_some() && keep.unwrap_or_else(|| current.dir.join(COPY).is_file());
    current.end(keep);
    for old in retired {
        old.end(false);
    }
}

/// What a recovery copy holds: the project's document and the file it belongs to.
pub struct Snapshot {
    pub doc: ProjectDoc,
    pub original: Option<PathBuf>,
}

/// The recovery copy's content for `p`. Every audio region names its file where it is on disk,
/// as a region saved without its audio does (`RegionDoc::audio_source`), because the copy
/// carries no audio.
pub fn snapshot(p: &Project, original: Option<&Path>) -> Snapshot {
    let (mut doc, media) = crate::project_io::to_doc(p);
    let sources: std::collections::HashMap<&str, &Path> = media
        .iter()
        .map(|m| (m.zip_path.as_str(), m.source.as_path()))
        .collect();
    for r in doc.tracks.iter_mut().flat_map(|t| t.regions.iter_mut()) {
        let source = r.audio.as_deref().and_then(|a| sources.get(a));
        if let Some(source) = source {
            r.audio_source = source.to_string_lossy().into_owned();
        }
    }
    Snapshot {
        doc,
        original: original.map(Path::to_path_buf),
    }
}

/// Writes the recovery copy into the session folder `dir`, through the project format's own
/// save (written beside the old copy, then renamed over it).
pub fn write(dir: &Path, snap: &Snapshot) -> Result<(), ProjectError> {
    std::fs::create_dir_all(dir)?;
    let note = dir.join(ORIGINAL);
    match snap.original.as_deref().and_then(Path::to_str) {
        Some(path) => {
            let tmp = dir.join(format!("{ORIGINAL}.tmp"));
            std::fs::write(&tmp, path)?;
            std::fs::rename(&tmp, &note)?;
        }
        None => {
            let _ = std::fs::remove_file(&note);
        }
    }
    prowave_project::save(&dir.join(COPY), &snap.doc, &[])?;
    Ok(())
}

/// The project was saved or closed: its recovery copy in `dir` is of no use any more. The
/// session's media stays (the open project may use it).
pub fn clear_copy(dir: &Path) {
    for name in [COPY, ORIGINAL] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

/// Whether `p` uses audio that was recorded or rendered into the session folder `dir` and is
/// in no saved project file: closing without saving would delete the only copy.
pub fn has_unsaved_recordings(p: &Project, dir: &Path) -> bool {
    let recordings = recordings_in(dir);
    crate::project_io::to_doc(p)
        .1
        .iter()
        .any(|m| m.source.starts_with(&recordings) && m.source.is_file())
}

/// A recovery copy found at start, to be offered.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// Its session folder.
    pub dir: PathBuf,
    /// The project's name.
    pub name: String,
    /// The project file it belongs to, if it was ever saved.
    pub original: Option<PathBuf>,
    /// When the copy was written.
    pub written: SystemTime,
    /// The run that wrote it did not end cleanly (its lock was left behind).
    pub unclean: bool,
}

/// At start: looks through the session folders in `root` that no running program holds.
/// Returns the recovery copies to offer, newest first, and deletes every other folder: one
/// without a recovery copy (nothing points at its media), one whose copy is older than
/// [`MAX_AGE`] or cannot be read, and one whose project file was saved after the copy.
pub fn sweep(root: &Path, now: SystemTime) -> Vec<Candidate> {
    let own: Vec<PathBuf> = LIVE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|l| {
            std::iter::once(&l.current)
                .chain(&l.retired)
                .map(|s| s.dir.clone())
                .collect()
        })
        .unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for dir in entries.flatten().map(|e| e.path()) {
        if !dir.is_dir() || own.contains(&dir) || is_live(&dir) {
            continue;
        }
        match candidate(&dir, now) {
            Some(c) => found.push(c),
            None => {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    tracing::warn!("cannot remove {}: {e}", dir.display());
                }
            }
        }
    }
    found.sort_by_key(|c| std::cmp::Reverse(c.written));
    found
}

/// The recovery copy in session folder `dir`, if it is worth offering.
fn candidate(dir: &Path, now: SystemTime) -> Option<Candidate> {
    let copy = dir.join(COPY);
    let written = modified(&copy)?;
    if now.duration_since(written).is_ok_and(|age| age > MAX_AGE) {
        return None;
    }
    let original = std::fs::read_to_string(dir.join(ORIGINAL))
        .ok()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from);
    // The project file was saved after the copy was written: the copy is the older one.
    let saved_later = original
        .as_deref()
        .and_then(modified)
        .is_some_and(|saved| saved >= written);
    if saved_later {
        return None;
    }
    let doc = load(dir).ok()?;
    let name = original
        .as_deref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .map_or(doc.name, str::to_string);
    Some(Candidate {
        dir: dir.to_path_buf(),
        name,
        original,
        written,
        unclean: dir.join(LOCK).exists(),
    })
}

fn load(dir: &Path) -> Result<ProjectDoc, ProjectError> {
    // The copy has no media entries, so nothing is unpacked.
    prowave_project::load(&dir.join(COPY), &dir.join("recovery-media"))
}

/// Opens the recovery copy `c`: the project, and the audio files that are not where the copy
/// says they are (their regions are there, as missing audio).
pub fn open(c: &Candidate, ctx: &egui::Context) -> Result<(Project, Vec<String>), ProjectError> {
    let doc = load(&c.dir)?;
    let (mut p, warnings) = crate::project_io::from_doc(&doc, &c.dir.join("recovery-media"), ctx);
    p.name = c.name.clone();
    Ok((p, warnings))
}

/// Deletes the recovery copy `c` and the media that was kept for it.
pub fn discard(c: &Candidate) {
    if let Err(e) = std::fs::remove_dir_all(&c.dir) {
        tracing::warn!("cannot remove {}: {e}", c.dir.display());
    }
}

/// What a project that was recovered is compared with to see whether it has unsaved changes:
/// a document no project ever equals, because a recovered project is unsaved until Save.
pub fn never_saved() -> ProjectDoc {
    ProjectDoc {
        format_version: 0,
        ..ProjectDoc::default()
    }
}

/// `t` as the date and time on this computer's clock, to the minute: `2026-11-10 14:32`.
pub fn local_time_text(t: SystemTime) -> String {
    let secs = unix_seconds(t);
    let text = civil_text(secs + utc_offset(secs));
    // Without the seconds.
    text[..text.len() - 3].to_string()
}

/// `t` in UTC, to the second: `2026-11-10 13:32:07 UTC`.
pub fn utc_time_text(t: SystemTime) -> String {
    format!("{} UTC", civil_text(unix_seconds(t)))
}

fn unix_seconds(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// How far this computer's clock is from UTC at `secs`, in seconds.
#[cfg(unix)]
fn utc_offset(secs: i64) -> i64 {
    let t = secs as libc::time_t;
    // SAFETY: `tm` is plain data for which all zeroes is a valid value.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to live values of the right types; `localtime_r` writes only
    // into `tm` and keeps neither pointer.
    let filled = unsafe { libc::localtime_r(&t, &mut tm) };
    if filled.is_null() {
        0
    } else {
        tm.tm_gmtoff as i64
    }
}

#[cfg(not(unix))]
fn utc_offset(_secs: i64) -> i64 {
    0
}

/// Seconds since 1970 as `YYYY-MM-DD HH:MM:SS` (the proleptic Gregorian calendar).
fn civil_text(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let of_day = secs.rem_euclid(86_400);
    // Days to a date, counting eras of 400 years from 0000-03-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn dirty_project() -> Project {
        let mut p = Project::demo();
        p.tempo = 97.0;
        p
    }

    #[test]
    fn a_session_is_locked_while_it_runs_and_its_folder_goes_on_a_clean_exit() {
        let root = tempfile::tempdir().unwrap();
        let s = Session::start(root.path()).unwrap();
        let dir = s.dir().to_path_buf();
        assert!(dir.join(LOCK).is_file(), "the lock is written at start");
        assert!(is_live(&dir), "and held");
        // A running session is neither offered nor deleted.
        write(&dir, &snapshot(&dirty_project(), None)).unwrap();
        assert!(sweep(root.path(), SystemTime::now()).is_empty());
        assert!(dir.join(COPY).is_file());

        s.end(false);
        assert!(!dir.exists(), "a clean exit with nothing to keep");
    }

    #[test]
    fn a_clean_exit_that_keeps_the_copy_removes_only_the_lock() {
        let root = tempfile::tempdir().unwrap();
        let s = Session::start(root.path()).unwrap();
        let dir = s.dir().to_path_buf();
        write(&dir, &snapshot(&dirty_project(), None)).unwrap();
        s.end(true);
        assert!(!dir.join(LOCK).exists());
        let found = sweep(root.path(), SystemTime::now());
        assert_eq!(found.len(), 1);
        assert!(!found[0].unclean);
        assert_eq!(found[0].name, "Untitled");
        assert_eq!(found[0].original, None);
    }

    #[test]
    fn after_a_crash_the_copy_is_offered_with_its_project_and_time() {
        let root = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let original = projects.path().join("Night Drive.prowave");
        crate::project_io::save_project(&Project::demo(), &original).unwrap();
        let before = std::fs::read(&original).unwrap();

        let s = Session::start(root.path()).unwrap();
        let later = modified(&original).unwrap() + Duration::from_secs(60);
        write(s.dir(), &snapshot(&dirty_project(), Some(&original))).unwrap();
        File::open(s.dir().join(COPY))
            .unwrap()
            .set_modified(later)
            .unwrap();
        let dir = s.crash();

        assert!(dir.join(LOCK).is_file(), "the lock is left behind");
        assert!(!is_live(&dir), "but nobody holds it");
        let found = sweep(root.path(), SystemTime::now());
        assert_eq!(found.len(), 1);
        let c = &found[0];
        assert!(c.unclean);
        assert_eq!(c.name, "Night Drive");
        assert_eq!(c.original.as_deref(), Some(original.as_path()));
        assert_eq!(c.written, later);
        assert_eq!(
            std::fs::read(&original).unwrap(),
            before,
            "the user's own file is not written to"
        );

        let (p, warnings) = open(c, &egui::Context::default()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(p.tempo, 97.0);
        assert_eq!(p.name, "Night Drive");
    }

    #[test]
    fn discarding_removes_the_copy_and_its_media() {
        let root = tempfile::tempdir().unwrap();
        let s = Session::start(root.path()).unwrap();
        let take = recordings_in(s.dir()).join("Audio 1#01.wav");
        std::fs::create_dir_all(take.parent().unwrap()).unwrap();
        crate::project_io::tests::write_wav(&take);
        write(s.dir(), &snapshot(&dirty_project(), None)).unwrap();
        let dir = s.crash();

        let found = sweep(root.path(), SystemTime::now());
        assert_eq!(found.len(), 1);
        assert!(take.is_file(), "kept for the copy");
        discard(&found[0]);
        assert!(!dir.exists() && !take.exists());
        assert!(sweep(root.path(), SystemTime::now()).is_empty());
    }

    #[test]
    fn folders_nobody_can_use_are_deleted_at_start() {
        let root = tempfile::tempdir().unwrap();
        let projects = tempfile::tempdir().unwrap();
        let now = SystemTime::now();

        // A crashed run that never wrote a copy: only media nothing points at.
        let s = Session::start(root.path()).unwrap();
        std::fs::create_dir_all(recordings_in(s.dir())).unwrap();
        std::fs::write(recordings_in(s.dir()).join("take.wav"), b"x").unwrap();
        let orphan = s.crash();

        // A copy older than the limit.
        let s = Session::start(root.path()).unwrap();
        write(s.dir(), &snapshot(&dirty_project(), None)).unwrap();
        let old = s.crash();
        File::open(old.join(COPY))
            .unwrap()
            .set_modified(now - MAX_AGE - Duration::from_secs(60))
            .unwrap();

        // A copy whose project file was saved after it.
        let original = projects.path().join("Song.prowave");
        let s = Session::start(root.path()).unwrap();
        write(s.dir(), &snapshot(&dirty_project(), Some(&original))).unwrap();
        let stale = s.crash();
        File::open(stale.join(COPY))
            .unwrap()
            .set_modified(now - Duration::from_secs(3600))
            .unwrap();
        crate::project_io::save_project(&Project::demo(), &original).unwrap();

        // A copy that is not a project.
        let s = Session::start(root.path()).unwrap();
        std::fs::write(s.dir().join(COPY), b"not a zip").unwrap();
        let broken = s.crash();

        // And one worth offering, whose project file is gone.
        let s = Session::start(root.path()).unwrap();
        let gone = projects.path().join("Gone.prowave");
        write(s.dir(), &snapshot(&dirty_project(), Some(&gone))).unwrap();
        let good = s.crash();

        let found = sweep(root.path(), now);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dir, good);
        assert_eq!(found[0].name, "Gone");
        for dir in [orphan, old, stale, broken] {
            assert!(!dir.exists(), "{} is deleted", dir.display());
        }
    }

    #[test]
    fn the_newest_copy_is_offered_first() {
        let root = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        let mut dirs = Vec::new();
        for age in [300, 30, 3000] {
            let s = Session::start(root.path()).unwrap();
            write(s.dir(), &snapshot(&dirty_project(), None)).unwrap();
            let dir = s.crash();
            File::open(dir.join(COPY))
                .unwrap()
                .set_modified(now - Duration::from_secs(age))
                .unwrap();
            dirs.push(dir);
        }
        let found: Vec<PathBuf> = sweep(root.path(), now).into_iter().map(|c| c.dir).collect();
        assert_eq!(found, [dirs[1].clone(), dirs[0].clone(), dirs[2].clone()]);
    }

    #[test]
    fn the_copy_holds_no_audio_and_names_each_file_where_it_is() {
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let s = Session::start(root.path()).unwrap();
        let take = recordings_in(s.dir()).join("Audio 1#01.wav");
        std::fs::create_dir_all(take.parent().unwrap()).unwrap();
        crate::project_io::tests::write_wav(&take);
        let mut p = Project::demo();
        crate::import::import_audio(&mut p, &take, &ctx).unwrap();
        assert!(has_unsaved_recordings(&p, s.dir()));
        assert!(!has_unsaved_recordings(&Project::demo(), s.dir()));

        write(s.dir(), &snapshot(&p, None)).unwrap();
        let copy = s.dir().join(COPY);
        assert!(prowave_project::media_names(&copy).unwrap().is_empty());
        assert!(std::fs::metadata(&copy).unwrap().len() < std::fs::metadata(&take).unwrap().len());
        assert!(!s.dir().join(format!("{COPY}.tmp")).exists());
        let dir = s.crash();

        let found = sweep(root.path(), SystemTime::now());
        let (q, warnings) = open(&found[0], &ctx).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let region = &q.tracks.last().unwrap().regions[0];
        assert_eq!(region.audio.as_ref().unwrap().file.path, take);
        assert!(take.starts_with(&dir));

        // The take is gone: the project still opens, and says which file is missing.
        std::fs::remove_file(&take).unwrap();
        let (_q, warnings) = open(&found[0], &ctx).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn a_folder_another_run_took_over_cannot_be_taken_again() {
        let root = tempfile::tempdir().unwrap();
        let dir = Session::start(root.path()).unwrap().crash();
        let taken = Session::take_over(&dir).unwrap();
        assert!(is_live(&dir));
        assert!(Session::take_over(&dir).is_none());
        taken.end(false);
        assert!(!dir.exists());
    }

    #[test]
    fn dates_are_written_as_year_month_day_and_time() {
        assert_eq!(civil_text(0), "1970-01-01 00:00:00");
        assert_eq!(civil_text(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(civil_text(1_794_321_127), "2026-11-10 14:32:07");
        assert_eq!(civil_text(1_794_321_127 - 5 * 3600), "2026-11-10 09:32:07");
        assert_eq!(civil_text(-1), "1969-12-31 23:59:59");
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_321_127);
        assert_eq!(utc_time_text(t), "2026-11-10 14:32:07 UTC");
        assert_eq!(local_time_text(t).len(), "2026-11-10 14:32".len());
    }
}
