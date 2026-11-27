//! Token-authenticated localhost `--control <port>` JSON-lines server.
//!
//! Adapted from Transit / SoundCraft control protocols:
//! - Binds strictly to `127.0.0.1:<port>`.
//! - Generates a per-launch 128-bit hex token written to `<config_dir>/control-token`
//!   (or `$DOURI_CONTROL_TOKEN_FILE`) with mode `0600` on Unix.
//! - First line of every connection must be `auth <token>`.
//! - Subsequent JSON-lines requests support:
//!   - `ui.screenshot` / `screenshot` `{path?}`
//!   - `ui.inspect` / `ui_inspect`
//!   - `execute` / `engine.execute` (dispatched via `mcp_server::McpEngine`)
//!   - `transport.play`, `transport.stop`, `transport.record`, `transport.cycle`
//!   - `app.quit`

use crate::model::Project;
use crate::UiState;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

pub const TOKEN_FILE_ENV: &str = "DOURI_CONTROL_TOKEN_FILE";
pub const TOKEN_FILE_NAME: &str = "control-token";
pub const AUTH_PREFIX: &str = "auth ";

pub fn requested_port(args: &[OsString]) -> Option<u16> {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--control" {
            if let Some(p) = it
                .next()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u16>().ok())
            {
                return Some(p);
            }
        }
    }
    None
}

pub fn token_file_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(TOKEN_FILE_ENV) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    crate::recent_files::config_dir().map(|d| d.join(TOKEN_FILE_NAME))
}

pub fn generate_token() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut f = std::fs::File::open("/dev/urandom")
            .map_err(|e| format!("cannot open /dev/urandom: {e}"))?;
        f.read_exact(&mut bytes)
            .map_err(|e| format!("cannot read /dev/urandom: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        for chunk in bytes.chunks_mut(8) {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64,
            );
            let v = h.finish().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn write_token_file(path: &Path, token: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let _ = std::fs::remove_file(path);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(token.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn constant_time_eq(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    let mut diff = a.len() ^ b.len();
    for (i, x) in a.iter().enumerate() {
        diff |= usize::from(x ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

pub struct ControlRequest {
    pub method: String,
    pub params: Value,
    pub reply: Sender<Value>,
}

pub struct ControlServer {
    rx: Receiver<ControlRequest>,
}

impl ControlServer {
    pub fn start(port: u16, ctx: egui::Context) -> Result<Self, String> {
        let token = generate_token()?;
        if let Some(path) = token_file_path() {
            write_token_file(&path, &token)?;
        }
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr).map_err(|e| format!("control bind {addr}: {e}"))?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("douri-control".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let tx = tx.clone();
                    let tok = token.clone();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || handle_client(stream, &tok, &tx, &ctx));
                }
            })
            .map_err(|e| format!("control thread: {e}"))?;
        Ok(Self { rx })
    }

    pub fn poll(&self, ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
        while let Ok(req) = self.rx.try_recv() {
            let reply = dispatch(ctx, p, s, &req.method, req.params);
            if let Some(v) = reply {
                let _ = req.reply.send(v);
            }
            // Note: screenshot requests reply asynchronously from the screenshot callback.
            let _ = req.reply;
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    token: &str,
    tx: &Sender<ControlRequest>,
    ctx: &egui::Context,
) {
    let Ok(cloned) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(cloned);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return;
    }
    let given = first.trim_end_matches(['\r', '\n']);
    let Some(tok) = given.strip_prefix(AUTH_PREFIX) else {
        let _ = writeln!(stream, r#"{{"ok":false,"error":"auth required"}}"#);
        return;
    };
    if !constant_time_eq(token, tok) {
        let _ = writeln!(stream, r#"{{"ok":false,"error":"invalid token"}}"#);
        return;
    }
    let _ = writeln!(stream, r#"{{"ok":true}}"#);

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
                    let _ = writeln!(stream, r#"{{"ok":false,"error":"invalid json"}}"#);
                    continue;
                };
                let method = val["method"].as_str().unwrap_or("").to_string();
                let params = val.get("params").cloned().unwrap_or_else(|| json!({}));
                let (rtx, rrx) = mpsc::channel();
                if tx
                    .send(ControlRequest {
                        method,
                        params,
                        reply: rtx,
                    })
                    .is_err()
                {
                    break;
                }
                ctx.request_repaint();
                match rrx.recv_timeout(std::time::Duration::from_secs(10)) {
                    Ok(resp) => {
                        let _ = writeln!(stream, "{resp}");
                    }
                    Err(_) => {
                        let _ = writeln!(stream, r#"{{"ok":false,"error":"timeout"}}"#);
                    }
                }
            }
        }
    }
}

fn dispatch(
    ctx: &egui::Context,
    p: &mut Project,
    s: &mut UiState,
    method: &str,
    params: Value,
) -> Option<Value> {
    match method {
        "ui.inspect" | "ui_inspect" => {
            let bottom = match s.bottom {
                crate::Bottom::Mixer => "mixer",
                crate::Bottom::PianoRoll => "piano_roll",
                crate::Bottom::PitchEditor => "pitch_editor",
                crate::Bottom::MacroControls => "macro_controls",
                crate::Bottom::SessionPlayer => "session_player",
            };
            Some(json!({
                "ok": true,
                "ui": {
                    "show_library": s.show_library,
                    "show_inspector": s.show_inspector,
                    "show_quick_help": s.show_quick_help,
                    "show_toolbar": s.show_toolbar,
                    "show_mixer": s.show_mixer,
                    "bottom": bottom,
                },
                "transport": {
                    "playing": p.playing,
                    "recording": p.recording,
                    "cycle": p.cycle,
                    "position_beats": p.position_beats,
                    "tempo": p.tempo_at(p.position_beats),
                },
                "project": {
                    "tracks": p.tracks.len(),
                    "selected_track": p.selected_track,
                }
            }))
        }
        "transport.play" => {
            p.playing = true;
            Some(json!({"ok": true, "playing": p.playing}))
        }
        "transport.stop" => {
            crate::transport::stop(p);
            Some(json!({"ok": true, "playing": p.playing}))
        }
        "transport.record" => {
            p.recording = !p.recording;
            Some(json!({"ok": true, "recording": p.recording}))
        }
        "transport.cycle" => {
            p.cycle = !p.cycle;
            Some(json!({"ok": true, "cycle": p.cycle}))
        }
        "app.quit" => {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            Some(json!({"ok": true}))
        }
        "execute" | "engine.execute" => {
            let tool = params["name"]
                .as_str()
                .or_else(|| params["command"].as_str())
                .unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(params.clone());
            let req = crate::mcp_server::JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(json!(1)),
                method: "tools/call".into(),
                params: Some(json!({ "name": tool, "arguments": args })),
            };
            let resp = crate::mcp_server::McpEngine::handle_request(req, p, s, None);
            Some(json!({"ok": true, "result": resp}))
        }
        "ui.screenshot" | "screenshot" => {
            let path = params["path"].as_str().map(PathBuf::from);
            // Handled via ctx.request_screenshot: save PNG to disk or encode base64.
            let _ = path;
            let (stx, _srx) = mpsc::channel::<Value>();
            // Wait: ControlServer::poll needs the reply Sender to hand off to the screenshot callback.
            // To avoid changing poll's signature, handle screenshot synchronously via a dedicated helper below.
            let _ = stx;
            Some(json!({"ok": false, "error": "use screenshot_with_reply"}))
        }
        _ => Some(json!({"ok": false, "error": format!("unknown method: {method}")})),
    }
}

impl ControlServer {
    pub fn poll_with_screenshots(&self, ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
        while let Ok(req) = self.rx.try_recv() {
            if matches!(req.method.as_str(), "ui.screenshot" | "screenshot") {
                let path = req.params["path"].as_str().map(PathBuf::from);
                let reply = req.reply;
                ctx.request_screenshot(move |image| {
                    if let Some(file) = path {
                        let outcome = crate::bench::save_png(&file, &image);
                        let res = match outcome {
                            crate::bench::ShotOutcome::Saved => {
                                json!({"ok": true, "path": file.display().to_string()})
                            }
                            crate::bench::ShotOutcome::Failed(e) => {
                                json!({"ok": false, "error": e})
                            }
                            crate::bench::ShotOutcome::TimedOut => {
                                json!({"ok": false, "error": "timed out"})
                            }
                        };
                        let _ = reply.send(res);
                    } else {
                        let [w, h] = image.size;
                        let rgba: Vec<u8> =
                            image.pixels.iter().flat_map(|c| c.to_array()).collect();
                        let mut png = Vec::new();
                        let enc = image::codecs::png::PngEncoder::new(&mut png);
                        let res = match image::ImageEncoder::write_image(
                            enc,
                            &rgba,
                            w as u32,
                            h as u32,
                            image::ExtendedColorType::Rgba8,
                        ) {
                            Ok(()) => {
                                let b64 = encode_base64(&png);
                                json!({"ok": true, "width": w, "height": h, "png_base64": b64})
                            }
                            Err(e) => json!({"ok": false, "error": e.to_string()}),
                        };
                        let _ = reply.send(res);
                    }
                });
                ctx.request_repaint();
            } else if let Some(v) = dispatch(ctx, p, s, &req.method, req.params) {
                let _ = req.reply.send(v);
            }
        }
    }
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map(u32::from).unwrap_or(0);
        let b2 = chunk.get(2).copied().map(u32::from).unwrap_or(0);
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_32_hex_chars_and_constant_time_eq_works() {
        let a = generate_token().unwrap();
        let b = generate_token().unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(constant_time_eq(&a, &a));
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn parses_control_port_from_args() {
        let args: Vec<OsString> = vec!["douri".into(), "--control".into(), "49152".into()];
        assert_eq!(requested_port(&args), Some(49152));
        assert_eq!(requested_port(&["douri".into()]), None);
    }
}
