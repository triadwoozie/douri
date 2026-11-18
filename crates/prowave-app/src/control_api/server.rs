//! The socket: a Unix domain socket that takes any number of clients. Each client has a
//! thread that reads its lines and one that writes to it, so a client that sends rubbish, a
//! huge line or stops reading can stall only itself. Requests go to the interface thread
//! through a channel; nothing here touches the project or the engine.
use super::protocol::{self, Code, Request, RpcError, MAX_LINE};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

/// Lines waiting to be written to one client. A client that lets more than this pile up is
/// dropped (it has stopped reading).
const OUT_QUEUE: usize = 4096;

/// A request and the connection it came from.
pub struct Incoming {
    pub conn: u64,
    pub request: Request,
    /// When its line was read, for the round-trip measurement.
    pub at: std::time::Instant,
}

struct Conn {
    out: SyncSender<String>,
    stream: UnixStream,
    subscribed: bool,
}

/// What the socket's threads and the interface thread share.
pub struct Shared {
    conns: Mutex<HashMap<u64, Conn>>,
    wake: Mutex<Option<egui::Context>>,
    stop: AtomicBool,
    next: AtomicU64,
}

impl Shared {
    pub(super) fn new() -> Self {
        Self {
            conns: Mutex::new(HashMap::new()),
            wake: Mutex::new(None),
            stop: AtomicBool::new(false),
            next: AtomicU64::new(1),
        }
    }

    /// The interface to wake when a request arrives.
    pub fn set_wake(&self, ctx: &egui::Context) {
        if let Ok(mut w) = self.wake.lock() {
            if w.is_none() {
                *w = Some(ctx.clone());
            }
        }
    }

    fn wake(&self) {
        if let Ok(w) = self.wake.lock() {
            if let Some(ctx) = w.as_ref() {
                ctx.request_repaint();
            }
        }
    }

    /// Queues `line` for `conn`. A client that is gone, or has stopped reading, is dropped.
    pub fn send(&self, conn: u64, line: String) {
        let Ok(mut conns) = self.conns.lock() else {
            return;
        };
        let drop_it = match conns.get(&conn) {
            Some(c) => match c.out.try_send(line) {
                Ok(()) => false,
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => true,
            },
            None => false,
        };
        if drop_it {
            if let Some(c) = conns.remove(&conn) {
                let _ = c.stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    pub fn respond(&self, conn: u64, id: &Value, result: &Result<Value, RpcError>) {
        self.send(conn, protocol::response(id, result));
    }

    pub fn set_subscribed(&self, conn: u64, on: bool) {
        if let Ok(mut conns) = self.conns.lock() {
            if let Some(c) = conns.get_mut(&conn) {
                c.subscribed = on;
            }
        }
    }

    /// How many clients asked for events.
    pub fn subscribers(&self) -> usize {
        self.conns
            .lock()
            .map_or(0, |c| c.values().filter(|c| c.subscribed).count())
    }

    pub fn clients(&self) -> usize {
        self.conns.lock().map_or(0, |c| c.len())
    }

    /// Sends an event to every subscribed client.
    pub fn emit(&self, name: &str, fields: Map<String, Value>) {
        let targets: Vec<u64> = match self.conns.lock() {
            Ok(c) => c
                .iter()
                .filter(|(_, c)| c.subscribed)
                .map(|(id, _)| *id)
                .collect(),
            Err(_) => return,
        };
        if targets.is_empty() {
            return;
        }
        let line = protocol::event(name, fields);
        for t in targets {
            self.send(t, line.clone());
        }
    }

    fn remove(&self, conn: u64) {
        if let Ok(mut conns) = self.conns.lock() {
            conns.remove(&conn);
        }
    }
}

/// The listening socket. Dropping it closes every client and removes the socket file.
pub struct Server {
    pub shared: Arc<Shared>,
    pub incoming: Receiver<Incoming>,
    path: PathBuf,
}

impl Server {
    /// Listens on `path`. Its folder is made if needed (mode 0700 when `private_dir`), a
    /// socket file left by a run that is gone is replaced, and the socket is mode 0600. Fails
    /// when another running program answers on it.
    pub fn start(path: &Path, private_dir: bool) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            if private_dir {
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(format!(
                    "{} is in use by another running Pro Wave",
                    path.display()
                ));
            }
            std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let shared = Arc::new(Shared::new());
        let (tx, incoming) = mpsc::channel();
        let accept_shared = shared.clone();
        std::thread::Builder::new()
            .name("control-accept".into())
            .spawn(move || accept_loop(listener, accept_shared, tx))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            incoming,
            path: path.to_path_buf(),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // The accept loop is waiting for a client: this one lets it see the flag.
        let _ = UnixStream::connect(&self.path);
        if let Ok(mut conns) = self.shared.conns.lock() {
            for (_, c) in conns.drain() {
                let _ = c.stream.shutdown(std::net::Shutdown::Both);
            }
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

fn accept_loop(listener: UnixListener, shared: Arc<Shared>, tx: Sender<Incoming>) {
    for stream in listener.incoming() {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        let Ok(stream) = stream else {
            continue;
        };
        let (Ok(read_half), Ok(write_half)) = (stream.try_clone(), stream.try_clone()) else {
            continue;
        };
        let id = shared.next.fetch_add(1, Ordering::Relaxed);
        let (out, out_rx) = mpsc::sync_channel::<String>(OUT_QUEUE);
        if let Ok(mut conns) = shared.conns.lock() {
            conns.insert(
                id,
                Conn {
                    out,
                    stream,
                    subscribed: false,
                },
            );
        }
        let _ = std::thread::Builder::new()
            .name("control-write".into())
            .spawn(move || write_loop(write_half, out_rx));
        let (reader_shared, reader_tx) = (shared.clone(), tx.clone());
        let _ = std::thread::Builder::new()
            .name("control-read".into())
            .spawn(move || {
                read_loop(read_half, id, &reader_shared, &reader_tx);
                reader_shared.remove(id);
            });
    }
}

fn write_loop(mut stream: UnixStream, lines: Receiver<String>) {
    // A client that takes nothing for this long is gone.
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(10)));
    for mut line in lines {
        line.push('\n');
        if stream.write_all(line.as_bytes()).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// One line of at most [`MAX_LINE`] bytes. `Ok(None)`: the client closed. `Ok(Some(Err))`:
/// the line was too long (it was read to its end and thrown away).
fn read_line(reader: &mut impl BufRead) -> std::io::Result<Option<Result<Vec<u8>, usize>>> {
    let mut line = Vec::new();
    let mut dropped = 0usize;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            // The end without a newline: what was read is not a whole request.
            return Ok(None);
        }
        let (take, done) = match buf.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        if dropped == 0 && line.len() + take <= MAX_LINE + 1 {
            line.extend_from_slice(&buf[..take]);
        } else {
            dropped += line.len() + take;
            line.clear();
        }
        reader.consume(take);
        if done {
            return Ok(Some(if dropped > 0 { Err(dropped) } else { Ok(line) }));
        }
    }
}

fn read_loop(stream: UnixStream, conn: u64, shared: &Shared, tx: &Sender<Incoming>) {
    let mut reader = BufReader::with_capacity(64 * 1024, stream);
    while let Ok(Some(line)) = read_line(&mut reader) {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        let at = std::time::Instant::now();
        let line = match line {
            Ok(l) => l,
            Err(n) => {
                let e = RpcError::new(
                    Code::TooLarge,
                    format!("the request line has {n} bytes; the limit is {MAX_LINE}"),
                )
                .with("limit", MAX_LINE);
                shared.respond(conn, &Value::Null, &Err(e));
                continue;
            }
        };
        let text = match std::str::from_utf8(&line) {
            Ok(t) => t.trim(),
            Err(_) => {
                let e = RpcError::new(Code::ParseError, "the line is not UTF-8 text");
                shared.respond(conn, &Value::Null, &Err(e));
                continue;
            }
        };
        if text.is_empty() {
            continue;
        }
        match protocol::parse(text) {
            Ok(request) => {
                if tx.send(Incoming { conn, request, at }).is_err() {
                    break;
                }
                shared.wake();
            }
            Err((id, e)) => shared.respond(conn, &id, &Err(e)),
        }
    }
}
