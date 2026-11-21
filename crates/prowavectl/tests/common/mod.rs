//! A stand-in for the Pro Wave app: a small control server on a Unix socket in a temp folder.
//! It speaks the same line protocol and knows a handful of methods. Nothing here starts the
//! real app, opens a window or plays audio.

// Each test file uses its own part of this module.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};
use tempfile::TempDir;

/// A temp folder with a short path. Unix socket paths are limited to about 100 bytes and the
/// default temp folder on macOS is too deep.
pub fn short_temp_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("pwc")
        .tempdir_in("/tmp")
        .expect("temp folder under /tmp")
}

#[derive(Default)]
struct Shared {
    /// The file path `ui.screenshot` reports.
    screenshot: Mutex<Option<String>>,
    /// After `events.subscribe` and its two events: keep the connection open instead of
    /// closing it.
    hold_after_events: AtomicBool,
    /// Every open connection, so `stop` can close them.
    connections: Mutex<Vec<UnixStream>>,
    /// Every request received, in order.
    requests: Mutex<Vec<Value>>,
}

pub struct FakeApp {
    pub dir: TempDir,
    pub socket: PathBuf,
    shared: Arc<Shared>,
}

/// What the fake app answers to `rpc.describe`. `fake.only` exists nowhere else, so a test can
/// tell this list from the one built into prowavectl.
pub fn describe() -> Value {
    json!({
        "protocol": 1, "app": "Pro Wave", "version": "9.9.9", "commit": "fake123",
        "socket": "",
        "methods": [
            {"name": "rpc.describe", "group": "rpc", "summary": "List the methods.",
             "changes_project": false,
             "params": [{"name": "method", "type": "string", "required": false,
                         "description": "Only this method."}],
             "result": "The method list."},
            {"name": "rpc.version", "group": "rpc", "summary": "Report the version.",
             "changes_project": false, "params": [], "result": "app, version, commit, protocol."},
            {"name": "state.get", "group": "state", "summary": "Read the app state.",
             "changes_project": false, "params": [], "result": "The state."},
            {"name": "transport.play", "group": "transport", "summary": "Start playback.",
             "changes_project": false, "params": [], "result": "The transport state."},
            {"name": "transport.locate", "group": "transport", "summary": "Move the playhead.",
             "changes_project": false,
             "params": [
                {"name": "bar", "type": "number", "required": true,
                 "description": "Bar to go to.", "unit": "bars", "min": 1, "max": 999},
                {"name": "snap", "type": "string", "required": false,
                 "description": "Rounding.", "values": ["none", "bar"]}],
             "result": "The transport state."},
            {"name": "ui.screenshot", "group": "ui", "summary": "Save a picture of the window.",
             "changes_project": false, "params": [], "result": "path, width, height."},
            {"name": "fake.only", "group": "fake", "summary": "Only the fake app has this.",
             "changes_project": true, "params": [], "result": "Nothing much."}
        ],
        "errors": [{"name": "failed", "number": -32000, "meaning": "It did not work."}]
    })
}

fn error(id: &Value, number: i64, code: &str, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {
        "code": number, "message": message, "data": {"code": code, "detail": "from the fake"}}})
}

fn event(name: &str, extra: Value) -> Value {
    let mut params = json!({"event": name});
    if let (Some(params), Some(extra)) = (params.as_object_mut(), extra.as_object()) {
        params.extend(extra.clone());
    }
    json!({"jsonrpc": "2.0", "method": "event", "params": params})
}

fn send(stream: &mut UnixStream, message: &Value) -> bool {
    writeln!(stream, "{message}")
        .and_then(|()| stream.flush())
        .is_ok()
}

fn serve(mut stream: UnixStream, shared: Arc<Shared>) {
    let reader = BufReader::new(stream.try_clone().expect("clone the connection"));
    for line in reader.lines() {
        let Ok(line) = line else { return };
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            send(
                &mut stream,
                &error(&Value::Null, -32700, "parse_error", "not JSON"),
            );
            continue;
        };
        shared.requests.lock().unwrap().push(request.clone());
        let id = &request["id"];
        let method = request["method"].as_str().unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        let ok = |result: Value| json!({"jsonrpc": "2.0", "id": id, "result": result});
        let reply = match method {
            "rpc.describe" => match params.get("method").and_then(Value::as_str) {
                None => ok(describe()),
                Some(name) => {
                    let mut all = describe();
                    let list = all["methods"].as_array_mut().unwrap();
                    list.retain(|m| m["name"] == name);
                    if list.is_empty() {
                        error(id, -32001, "not_found", &format!("no method named {name}"))
                    } else {
                        ok(all)
                    }
                }
            },
            "rpc.version" => ok(json!({
                "app": "Pro Wave", "version": "9.9.9", "commit": "fake123", "protocol": 1})),
            "state.get" => ok(json!({
                "project": {"name": "Demo"}, "transport": {"playing": false, "bar": 1}})),
            "transport.play" => ok(json!({"playing": true})),
            "transport.locate" => match params.get("bar").and_then(Value::as_f64) {
                Some(bar) if bar >= 1.0 => ok(json!({"bar": bar, "got": params})),
                _ => error(id, -32602, "invalid_params", "bar must be 1 or more"),
            },
            "fake.only" => ok(json!({"fake": true})),
            "fail.now" => error(id, -32000, "failed", "it did not work"),
            "ui.screenshot" => {
                let path = shared.screenshot.lock().unwrap().clone();
                ok(json!({"path": path, "width": 1, "height": 1}))
            }
            // Several megabytes on one line.
            "big.blob" => ok(json!({"blob": "x".repeat(3 * 1024 * 1024)})),
            // An event line arrives before the answer, as can happen after a subscription.
            "noisy.call" => {
                send(&mut stream, &event("transport.started", json!({"bar": 3})));
                ok(json!({"noisy": true}))
            }
            // Never answers.
            "slow.never" => continue,
            "events.subscribe" => {
                send(&mut stream, &ok(json!({"subscribed": true})));
                send(&mut stream, &event("transport.started", json!({"bar": 1})));
                send(
                    &mut stream,
                    &event("transport.stopped", json!({"bar": 2.5})),
                );
                if shared.hold_after_events.load(Ordering::SeqCst) {
                    continue;
                }
                // The app "quits".
                let _ = stream.shutdown(Shutdown::Both);
                return;
            }
            other => error(
                id,
                -32601,
                "method_not_found",
                &format!("no method named {other}"),
            ),
        };
        if !send(&mut stream, &reply) {
            return;
        }
    }
}

impl FakeApp {
    pub fn start() -> FakeApp {
        let dir = short_temp_dir();
        let socket = dir.path().join("c.sock");
        let listener = UnixListener::bind(&socket).expect("bind the fake control socket");
        let shared = Arc::new(Shared::default());
        let for_thread = Arc::clone(&shared);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                if let Ok(copy) = stream.try_clone() {
                    for_thread.connections.lock().unwrap().push(copy);
                }
                let shared = Arc::clone(&for_thread);
                thread::spawn(move || serve(stream, shared));
            }
        });
        FakeApp {
            dir,
            socket,
            shared,
        }
    }

    pub fn socket_str(&self) -> &str {
        self.socket.to_str().expect("socket path is UTF-8")
    }

    pub fn set_screenshot(&self, path: &str) {
        *self.shared.screenshot.lock().unwrap() = Some(path.to_string());
    }

    pub fn hold_after_events(&self) {
        self.shared.hold_after_events.store(true, Ordering::SeqCst);
    }

    /// The methods called so far, in order.
    pub fn called(&self) -> Vec<String> {
        self.shared
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request["method"].as_str().unwrap_or("").to_string())
            .collect()
    }

    /// Closes every open connection; the app keeps listening. As if the app had restarted.
    pub fn close_connections(&self) {
        for connection in self.shared.connections.lock().unwrap().drain(..) {
            let _ = connection.shutdown(Shutdown::Both);
        }
    }

    /// The fake app "quits": the socket file goes away and every open connection closes.
    pub fn stop(&self) {
        let _ = std::fs::remove_file(&self.socket);
        for connection in self.shared.connections.lock().unwrap().drain(..) {
            let _ = connection.shutdown(Shutdown::Both);
        }
    }
}

/// A socket file that nothing listens on, as an app that crashed leaves behind.
pub fn stale_socket() -> (TempDir, PathBuf) {
    let dir = short_temp_dir();
    let socket = dir.path().join("c.sock");
    drop(UnixListener::bind(&socket).expect("bind"));
    assert!(
        socket.exists(),
        "the socket file stays after the listener closes"
    );
    (dir, socket)
}

/// Runs the built prowavectl binary with these arguments and nothing on stdin.
pub fn prowavectl(args: &[&str]) -> Output {
    prowavectl_with_stdin(args, "")
}

pub fn prowavectl_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_prowavectl"))
        .args(args)
        .env_remove("PROWAVE_CONTROL_SOCKET")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start prowavectl");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait for prowavectl")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
