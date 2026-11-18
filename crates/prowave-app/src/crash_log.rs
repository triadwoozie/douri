//! The crash log: `<settings folder>/crash.log`.
//!
//! A release build aborts on a panic (`panic = "abort"`), and plug-ins run inside the
//! program, so a panic ends the session. The panic hook installed here writes what happened to
//! the log first: the time, the version, the thread, the message and a backtrace. The file
//! keeps the last [`KEEP`] crashes. Nothing is sent anywhere.
use std::path::{Path, PathBuf};

/// How many crashes the log keeps.
const KEEP: usize = 10;
/// The most one crash takes in the log (a backtrace can be long).
const MAX_ENTRY_BYTES: usize = 64 * 1024;
/// The first line of every entry.
const HEADING: &str = "==== Pro Wave crash ====";

/// `<settings folder>/crash.log`.
pub fn path() -> Option<PathBuf> {
    crate::recent_files::config_dir().map(|d| d.join("crash.log"))
}

/// Makes every panic write to the crash log before the hook that was there runs (the report
/// on the terminal, and then the abort in a release build).
pub fn install() {
    if let Some(file) = path() {
        install_at(file);
    }
}

fn install_at(file: PathBuf) {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A log that cannot be written must not stand in the way of the report.
        let _ = append(&file, &entry(info));
        before(info);
    }));
}

/// One crash, as it is written to the log.
fn entry(info: &std::panic::PanicHookInfo<'_>) -> String {
    let thread = std::thread::current();
    let message = info.payload_as_str().unwrap_or("(no message)");
    let place = info
        .location()
        .map_or_else(|| "(unknown)".to_string(), ToString::to_string);
    let backtrace = std::backtrace::Backtrace::force_capture();
    let mut text = format!(
        "{HEADING}\ntime: {}\nversion: {} {}\nthread: {}\npanic: {message}\nat: {place}\nbacktrace:\n{backtrace}",
        crate::recovery::utc_time_text(std::time::SystemTime::now()),
        crate::APP_NAME,
        crate::about_version(),
        thread.name().unwrap_or("(unnamed)"),
    );
    if text.len() > MAX_ENTRY_BYTES {
        let mut end = MAX_ENTRY_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n(cut)");
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Adds `entry` to the log at `file`, which keeps its last [`KEEP`] entries.
fn append(file: &Path, entry: &str) -> std::io::Result<()> {
    let old = std::fs::read_to_string(file).unwrap_or_default();
    let mut starts: Vec<usize> = old.match_indices(HEADING).map(|(at, _)| at).collect();
    // Only a heading at the start of a line begins an entry (a panic message may quote one).
    starts.retain(|&at| at == 0 || old.as_bytes()[at - 1] == b'\n');
    let from = starts
        .len()
        .checked_sub(KEEP - 1)
        .map_or(starts.first().copied().unwrap_or(old.len()), |i| starts[i]);
    let text = format!("{}{entry}", &old[from..]);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, text)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_thread_is_written_to_the_log_before_it_ends() {
        let dir = std::env::temp_dir().join(format!("prowave-crash-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("settings").join("crash.log");
        // The hook stays for the rest of the test run: other tests' panics land in this file
        // too, so the entry is looked for by its thread.
        install_at(file.clone());
        let thread = std::thread::Builder::new()
            .name("crash-log-test".into())
            .spawn(|| panic!("the filter cutoff was {} Hz", -1))
            .unwrap();
        assert!(thread.join().is_err(), "the thread panicked");

        let log = std::fs::read_to_string(&file).unwrap();
        let entry = log
            .split(HEADING)
            .find(|e| e.contains("thread: crash-log-test"))
            .expect("the crash is in the log");
        assert!(
            entry.contains("panic: the filter cutoff was -1 Hz"),
            "{entry}"
        );
        assert!(
            entry.contains("crash_log.rs:"),
            "where it happened: {entry}"
        );
        assert!(entry.contains(&format!("version: {}", crate::APP_NAME)));
        assert!(entry.contains(env!("CARGO_PKG_VERSION")));
        assert!(entry.contains(" UTC\n"), "the time: {entry}");
        let backtrace = entry.split("backtrace:\n").nth(1).unwrap();
        assert!(backtrace.lines().count() > 3, "a backtrace: {backtrace}");
    }

    #[test]
    fn the_log_keeps_the_last_crashes_only() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("crash.log");
        for i in 0..KEEP + 5 {
            append(
                &file,
                &format!("{HEADING}\npanic: number {i}, see {HEADING}\n"),
            )
            .unwrap();
        }
        let log = std::fs::read_to_string(&file).unwrap();
        let kept: Vec<&str> = log.lines().filter(|l| l.starts_with("panic:")).collect();
        assert_eq!(kept.len(), KEEP);
        assert!(
            kept[0].contains("number 5,"),
            "the oldest are dropped: {}",
            kept[0]
        );
        assert!(kept[KEEP - 1].contains(&format!("number {},", KEEP + 4)));
        assert!(log.starts_with(HEADING));
    }
}
