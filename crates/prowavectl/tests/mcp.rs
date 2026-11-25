//! `prowavectl mcp` over its stdin and stdout, against the fake app.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use base64::Engine;
use common::FakeApp;
use serde_json::{json, Value};

/// A 1x1 pixel PNG file, base64-encoded.
const TINY_PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

const WAIT: Duration = Duration::from_secs(60);

/// A running `prowavectl mcp` and its pipes.
struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    /// Every line the server writes to stdout.
    lines: Receiver<String>,
    next_id: u64,
}

impl Mcp {
    fn start(socket: &str) -> Mcp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_prowavectl"))
            .args(["mcp", "--socket", socket, "--timeout", "20"])
            .env_remove("PROWAVE_CONTROL_SOCKET")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start prowavectl mcp");
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if sender.send(line).is_err() {
                    return;
                }
            }
        });
        // Read stderr too, so the server never blocks on a full pipe.
        let stderr = child.stderr.take().unwrap();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("[mcp stderr] {line}");
            }
        });
        Mcp {
            stdin: child.stdin.take(),
            child,
            lines,
            next_id: 1,
        }
    }

    fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    /// The next line from the server. Everything on stdout must be one JSON-RPC message.
    fn receive(&mut self) -> Value {
        let line = self
            .lines
            .recv_timeout(WAIT)
            .expect("an answer from the MCP server");
        let message: Value = serde_json::from_str(&line).expect("stdout carries only JSON lines");
        assert_eq!(message["jsonrpc"], "2.0", "{line}");
        message
    }

    /// Sends a request and returns the whole answer with the matching id.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let mut message = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if !params.is_null() {
            message["params"] = params;
        }
        self.send_raw(&message.to_string());
        let answer = self.receive();
        assert_eq!(answer["id"], id, "{answer}");
        answer
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    /// Nothing more may be waiting on stdout.
    fn assert_silent(&mut self) {
        if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(300)) {
            panic!("unexpected output: {line}");
        }
    }

    /// Closes stdin and returns the exit code.
    fn close(mut self) -> Option<i32> {
        drop(self.stdin.take());
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.code();
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("the MCP server did not exit after stdin closed");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn text_of(answer: &Value) -> &str {
    let item = &answer["result"]["content"][0];
    assert_eq!(item["type"], "text", "{answer}");
    item["text"].as_str().unwrap()
}

fn tool<'a>(list: &'a Value, name: &str) -> &'a Value {
    list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("no tool named {name} in {list}"))
}

#[test]
fn a_whole_session_with_the_handshake() {
    let app = FakeApp::start();
    let mut mcp = Mcp::start(app.socket_str());

    // The handshake.
    let answer = mcp.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "test", "version": "0"}}),
    );
    let result = &answer["result"];
    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert_eq!(result["capabilities"], json!({"tools": {}}));
    assert_eq!(result["serverInfo"]["name"], "prowavectl");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    let instructions = result["instructions"].as_str().unwrap();
    assert!(instructions.contains("rpc.describe"), "{instructions}");
    assert!(
        instructions.contains("must already be running"),
        "{instructions}"
    );
    mcp.send_raw(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
    mcp.assert_silent();

    assert_eq!(mcp.request("ping", Value::Null)["result"], json!({}));

    // The tools come from the app: fake.only exists only there.
    let list = mcp.request("tools/list", Value::Null);
    assert!(list["result"].get("resultType").is_none());
    let play = tool(&list, "prowave_transport_play");
    assert_eq!(
        play["description"],
        "Start playback.\nResult: The transport state."
    );
    assert_eq!(
        play["inputSchema"],
        json!({"type": "object", "additionalProperties": false})
    );
    let locate = tool(&list, "prowave_transport_locate");
    assert_eq!(
        locate["inputSchema"],
        json!({
            "type": "object",
            "properties": {
                "bar": {"type": "number", "description": "Bar to go to. (unit: bars)",
                        "minimum": 1, "maximum": 999},
                "snap": {"type": "string", "description": "Rounding.", "enum": ["none", "bar"]}
            },
            "required": ["bar"]
        })
    );
    tool(&list, "prowave_fake_only");
    tool(&list, "prowave_ui_screenshot");
    let call = tool(&list, "prowave_call");
    assert_eq!(call["inputSchema"]["type"], "object");
    assert_eq!(
        call["inputSchema"]["properties"]["method"]["type"],
        "string"
    );
    assert_eq!(
        call["inputSchema"]["properties"]["params"]["type"],
        "object"
    );
    assert_eq!(call["inputSchema"]["required"], json!(["method"]));
    let status = tool(&list, "prowave_status");
    assert_eq!(status["inputSchema"]["type"], "object");
    for tool in list["result"]["tools"].as_array().unwrap() {
        let name = tool["name"].as_str().unwrap();
        assert!(name.starts_with("prowave_"), "{name}");
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{name}"
        );
        assert!(tool["description"].is_string());
    }

    // A tool call the app answers.
    let answer = mcp.call_tool("prowave_transport_locate", json!({"bar": 12}));
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let got: Value = serde_json::from_str(text_of(&answer)).unwrap();
    assert_eq!(got["got"], json!({"bar": 12}));
    assert!(
        text_of(&answer).contains("\n  "),
        "the result is indented JSON"
    );

    // The general tool, with and without params.
    let answer = mcp.call_tool("prowave_call", json!({"method": "state.get"}));
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let got: Value = serde_json::from_str(text_of(&answer)).unwrap();
    assert_eq!(got["project"]["name"], "Demo");
    let answer = mcp.call_tool(
        "prowave_call",
        json!({"method": "transport.locate", "params": {"bar": 3}}),
    );
    let got: Value = serde_json::from_str(text_of(&answer)).unwrap();
    assert_eq!(got["bar"], 3.0);

    // The status tool.
    let answer = mcp.call_tool("prowave_status", json!({}));
    assert_eq!(answer["result"]["isError"], false);
    let report: Value = serde_json::from_str(text_of(&answer)).unwrap();
    assert_eq!(report["reachable"], true);
    assert_eq!(report["socket"], app.socket_str());
    assert_eq!(report["version"]["version"], "9.9.9");

    // An error from the app is a tool result marked as an error, not a protocol error.
    let answer = mcp.call_tool("prowave_transport_locate", json!({"bar": 0}));
    assert!(answer.get("error").is_none(), "{answer}");
    assert_eq!(answer["result"]["isError"], true);
    let text = text_of(&answer);
    assert!(
        text.starts_with("error invalid_params: bar must be 1 or more\n"),
        "{text}"
    );
    let answer = mcp.call_tool("prowave_call", json!({"method": "fail.now"}));
    assert_eq!(answer["result"]["isError"], true);
    assert!(text_of(&answer).starts_with("error failed: it did not work\n"));

    // A screenshot comes back as text plus the picture itself.
    let png = base64::engine::general_purpose::STANDARD
        .decode(TINY_PNG)
        .unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let png_path = app.dir.path().join("shot.png");
    std::fs::write(&png_path, &png).unwrap();
    app.set_screenshot(png_path.to_str().unwrap());
    let answer = mcp.call_tool("prowave_ui_screenshot", json!({}));
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let content = answer["result"]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2, "{answer}");
    let got: Value = serde_json::from_str(text_of(&answer)).unwrap();
    assert_eq!(got["path"], png_path.to_str().unwrap());
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    let sent = base64::engine::general_purpose::STANDARD
        .decode(content[1]["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(sent, png);

    // A picture over the size limit is left out, and the text says so.
    let big_path = app.dir.path().join("big.png");
    std::fs::write(&big_path, vec![0u8; 4 * 1024 * 1024 + 1]).unwrap();
    app.set_screenshot(big_path.to_str().unwrap());
    let answer = mcp.call_tool("prowave_call", json!({"method": "ui.screenshot"}));
    assert_eq!(answer["result"]["isError"], false);
    assert_eq!(answer["result"]["content"].as_array().unwrap().len(), 1);
    assert!(
        text_of(&answer).contains("No image attached"),
        "{}",
        text_of(&answer)
    );
    assert!(text_of(&answer).contains(big_path.to_str().unwrap()));

    // An unknown tool is a protocol error.
    let answer = mcp.call_tool("prowave_no_such_thing", json!({}));
    assert_eq!(answer["error"]["code"], -32602);
    assert_eq!(
        answer["error"]["message"],
        "Unknown tool: prowave_no_such_thing"
    );
    assert!(answer.get("result").is_none());

    // An unknown method.
    let answer = mcp.request("resources/list", Value::Null);
    assert_eq!(answer["error"]["code"], -32601);

    // A line that is not JSON gets a parse error and the server carries on.
    mcp.send_raw("{this is not json");
    let answer = mcp.receive();
    assert_eq!(answer["error"]["code"], -32700);
    assert_eq!(answer["id"], Value::Null);
    assert_eq!(mcp.request("ping", Value::Null)["result"], json!({}));

    // The app quits: the tool says to start it, and the server stays up.
    app.stop();
    let answer = mcp.call_tool("prowave_transport_play", json!({}));
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    let text = text_of(&answer);
    assert!(text.contains("Start Dōri"), "{text}");
    assert!(text.contains("Dōri is not running"), "{text}");
    let answer = mcp.call_tool("prowave_status", json!({}));
    let report: Value = serde_json::from_str(text_of(&answer)).unwrap();
    assert_eq!(report["reachable"], false);

    // The tool list still works, now from the built-in copy.
    let list = mcp.request("tools/list", Value::Null);
    tool(&list, "prowave_call");
    assert!(list["result"]["tools"].as_array().unwrap().len() > 2);
    assert!(list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["name"] != "prowave_fake_only"));

    mcp.assert_silent();
    assert_eq!(mcp.close(), Some(0));
}

#[test]
fn the_app_can_start_after_the_server_and_restart() {
    // The server starts first, as when Claude Code is opened before Pro Wave.
    let dir = common::short_temp_dir();
    let absent = dir.path().join("none.sock");
    let mut mcp = Mcp::start(absent.to_str().unwrap());
    mcp.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "0"}}),
    );
    let list = mcp.request("tools/list", Value::Null);
    tool(&list, "prowave_call");
    let answer = mcp.call_tool("prowave_call", json!({"method": "transport.play"}));
    assert_eq!(answer["result"]["isError"], true);
    assert!(text_of(&answer).contains("Start Dōri"));
    assert_eq!(mcp.close(), Some(0));

    // A kept connection that died is replaced without the agent noticing.
    let app = FakeApp::start();
    let mut mcp = Mcp::start(app.socket_str());
    mcp.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "0"}}),
    );
    let answer = mcp.call_tool("prowave_call", json!({"method": "transport.play"}));
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    app.close_connections();
    std::thread::sleep(Duration::from_millis(200));
    let answer = mcp.call_tool("prowave_call", json!({"method": "transport.play"}));
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    // A tool that was not listed yet is still found, by asking the app.
    let answer = mcp.call_tool("prowave_fake_only", json!({}));
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    assert_eq!(mcp.close(), Some(0));
}

#[test]
fn a_session_without_the_handshake() {
    let app = FakeApp::start();
    let mut mcp = Mcp::start(app.socket_str());
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "0"},
        "io.modelcontextprotocol/clientCapabilities": {}
    });

    let answer = mcp.request("server/discover", json!({"_meta": meta}));
    let result = &answer["result"];
    assert_eq!(result["resultType"], "complete");
    assert_eq!(result["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(result["capabilities"], json!({"tools": {}}));
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "prowavectl"
    );
    assert!(result["instructions"].is_string());

    let list = mcp.request("tools/list", json!({"_meta": meta}));
    assert_eq!(list["result"]["resultType"], "complete");
    assert_eq!(list["result"]["cacheScope"], "public");
    assert!(list["result"]["ttlMs"].is_u64());
    tool(&list, "prowave_transport_play");

    let answer = mcp.request(
        "tools/call",
        json!({"name": "prowave_transport_play", "arguments": {}, "_meta": meta}),
    );
    assert_eq!(answer["result"]["resultType"], "complete");
    assert_eq!(answer["result"]["isError"], false);
    assert_eq!(
        serde_json::from_str::<Value>(text_of(&answer)).unwrap(),
        json!({"playing": true})
    );

    // A version this server does not serve.
    let answer = mcp.request(
        "tools/list",
        json!({"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2030-01-01",
            "io.modelcontextprotocol/clientCapabilities": {}}}),
    );
    assert_eq!(answer["error"]["code"], -32022);
    assert_eq!(answer["error"]["data"]["supported"], json!(["2026-07-28"]));
    assert_eq!(answer["error"]["data"]["requested"], "2030-01-01");

    // No handshake and no version on the request.
    let answer = mcp.request("tools/list", Value::Null);
    assert_eq!(answer["error"]["code"], -32602);

    assert_eq!(mcp.close(), Some(0));
}
