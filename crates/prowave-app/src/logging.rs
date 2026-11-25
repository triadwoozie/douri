//! Rotating file logger (`<settings dir>/logs/douri.log`) + RT-safe audio-thread buffer.
//!
//! Adapted from SoundCraft's logging pattern:
//! - Rotates `douri.log` -> `douri.1.log` -> `douri.2.log` on startup.
//! - Caps the active log file at [`MAX_FILE_BYTES`] (16 MiB) so runaway warnings cannot fill disk.
//! - Skips file logging when `DOURI_NO_PREFS=1` is set (headless/automated test runs).
//! - Provides a lock-free / `try_lock` fixed-size audio-thread buffer so audio callbacks never allocate.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

pub const LOG_FILE: &str = "douri.log";
pub const LOG_DIR: &str = "logs";
pub const KEEP: usize = 2;
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const REALTIME_MESSAGE_BYTES: usize = 480;

static GLOBAL_SINK: OnceLock<Mutex<FileSink>> = OnceLock::new();
static REALTIME: Realtime = Realtime::new();

#[derive(Default)]
struct FileSink {
    file: Option<File>,
    written: u64,
    capped: bool,
    disabled: bool,
}

impl FileSink {
    fn write_bytes(&mut self, bytes: &[u8]) {
        if self.disabled || self.capped {
            return;
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let len = bytes.len() as u64;
        if self.written.saturating_add(len) > MAX_FILE_BYTES {
            self.capped = true;
            let _ = writeln!(
                file,
                "log file reached {MAX_FILE_BYTES} bytes; later records go to stderr only"
            );
            return;
        }
        self.written = self.written.saturating_add(len);
        if file.write_all(bytes).is_err() {
            self.file = None;
            self.capped = true;
        }
    }
}

fn sink_lock() -> &'static Mutex<FileSink> {
    GLOBAL_SINK.get_or_init(|| Mutex::new(FileSink::default()))
}

/// A fixed-size non-allocating buffer for audio-thread diagnostics.
struct Bounded<const N: usize> {
    buf: [u8; N],
    len: usize,
    cut: bool,
}

impl<const N: usize> Bounded<N> {
    const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
            cut: false,
        }
    }

    fn as_str(&self) -> &str {
        self.buf
            .get(..self.len)
            .and_then(|b| std::str::from_utf8(b).ok())
            .unwrap_or("")
    }
}

impl<const N: usize> std::fmt::Write for Bounded<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = N.saturating_sub(self.len);
        let mut n = s.len().min(room);
        while !s.is_char_boundary(n) {
            n = n.saturating_sub(1);
        }
        if let (Some(dst), Some(src)) = (
            self.buf.get_mut(self.len..self.len + n),
            s.as_bytes().get(..n),
        ) {
            dst.copy_from_slice(src);
            self.len += n;
        }
        if n < s.len() {
            self.cut = true;
        }
        Ok(())
    }
}

struct Realtime {
    records: AtomicUsize,
    held: Mutex<Option<Bounded<REALTIME_MESSAGE_BYTES>>>,
}

impl Realtime {
    const fn new() -> Self {
        Self {
            records: AtomicUsize::new(0),
            held: Mutex::new(None),
        }
    }

    fn keep(&self, msg: &str) {
        self.records.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = self.held.try_lock() {
            if guard.is_none() {
                let mut b = Bounded::new();
                let _ = b.write_str(msg);
                *guard = Some(b);
            }
        }
    }

    fn take_line(&self) -> Option<String> {
        let count = self.records.load(Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        let held = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let total = self.records.swap(0, Ordering::Relaxed);
        let mut out = String::new();
        if let Some(h) = held {
            out.push_str("[audio] ");
            out.push_str(h.as_str());
            if h.cut {
                out.push_str("...");
            }
            out.push('\n');
        }
        if total > 1 {
            let _ = writeln!(
                out,
                "[audio] {} more realtime log messages suppressed",
                total - 1
            );
        }
        (!out.is_empty()).then_some(out)
    }
}

/// Logs an audio-thread message without allocating or blocking.
pub fn log_rt(msg: &str) {
    REALTIME.keep(msg);
}

/// Flushes any buffered audio-thread diagnostics to the log file and stderr (UI thread).
pub fn report_audio_thread() {
    if let Some(line) = REALTIME.take_line() {
        let _ = std::io::stderr().write_all(line.as_bytes());
        sink_lock()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write_bytes(line.as_bytes());
    }
}

/// Rotates `logs/douri.log` -> `douri.1.log` -> `douri.2.log` inside `settings_dir`.
pub fn rotate_and_open(settings_dir: &Path) -> Result<(PathBuf, File), std::io::Error> {
    let dir = settings_dir.join(LOG_DIR);
    fs::create_dir_all(&dir)?;
    for idx in (1..KEEP).rev() {
        let src = dir.join(format!("douri.{idx}.log"));
        let dst = dir.join(format!("douri.{}.log", idx + 1));
        if src.exists() {
            let _ = fs::rename(&src, &dst);
        }
    }
    let active = dir.join(LOG_FILE);
    if active.exists() {
        let _ = fs::rename(&active, dir.join("douri.1.log"));
    }
    let file = File::create(&active)?;
    Ok((active, file))
}

/// Attaches `<settings_dir>/logs/douri.log` unless `DOURI_NO_PREFS=1` is set.
pub fn attach_default_dir() {
    if std::env::var_os("DOURI_NO_PREFS").is_some_and(|v| !v.is_empty() && v != "0") {
        sink_lock()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .disabled = true;
        return;
    }
    if let Some(dir) = crate::recent_files::config_dir() {
        if let Ok((_path, file)) = rotate_and_open(&dir) {
            let mut s = sink_lock().lock().unwrap_or_else(PoisonError::into_inner);
            s.file = Some(file);
            s.written = 0;
            s.capped = false;
        }
    }
}

/// `tracing_subscriber::fmt::MakeWriter` that tees to stderr and the rotating log file.
#[derive(Clone, Copy)]
pub struct TeeMakeWriter;

pub struct TeeWriter;

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stderr().write_all(buf);
        sink_lock()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write_bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stderr().flush();
        if let Some(f) = sink_lock()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .file
            .as_mut()
        {
            let _ = f.flush();
        }
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TeeMakeWriter {
    type Writer = TeeWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TeeWriter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_and_open_rotates_existing_logs() {
        let root = std::env::temp_dir().join(format!("douri-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (p1, mut f1) = rotate_and_open(&root).unwrap();
        writeln!(f1, "run1").unwrap();
        drop(f1);

        let (p2, mut f2) = rotate_and_open(&root).unwrap();
        writeln!(f2, "run2").unwrap();
        drop(f2);

        assert_eq!(p1, p2);
        assert_eq!(fs::read_to_string(&p2).unwrap().trim(), "run2");
        assert_eq!(
            fs::read_to_string(root.join(LOG_DIR).join("douri.1.log"))
                .unwrap()
                .trim(),
            "run1"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bounded_truncates_on_char_boundary() {
        let mut b: Bounded<5> = Bounded::new();
        let _ = b.write_str("abcdeFG");
        assert_eq!(b.as_str(), "abcde");
        assert!(b.cut);
    }
}
