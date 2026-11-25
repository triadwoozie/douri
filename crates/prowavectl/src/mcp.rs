//! `prowavectl mcp`: an MCP (Model Context Protocol) server on stdin/stdout.
//!
//! An agent such as Claude Code starts this program and sends it JSON-RPC messages, one per
//! line; every control method of the running app appears to the agent as a tool. Only protocol
//! messages go to stdout; anything for a human goes to stderr. The program ends when stdin
//! closes. It never starts the app.
//!
//! Written from the MCP specification (modelcontextprotocol.io/specification), revisions
//! 2026-07-28 (current) and 2025-11-25 / 2025-06-18 (the older handshake form). The two forms
//! differ in how a client opens:
//!
//! - Handshake form (2025-11-25 and earlier): the client sends `initialize` first, the server
//!   answers with the version both will use, the client sends `notifications/initialized`.
//! - Per-request form (2026-07-28): there is no handshake. Every request names its protocol
//!   version and the client's capabilities in `params._meta`, results carry a `resultType`, and
//!   `server/discover` reports what the server supports.
//!
//! This server accepts both, as the specification's "dual-era" server: a request with the
//! per-request version field is served in the per-request form, and `initialize` switches the
//! process to the handshake form.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Map, Value};

use crate::client::{Client, RpcFailure};
use crate::methods::{self, Method};

/// The protocol version served in the per-request form.
pub const MODERN_VERSION: &str = "2026-07-28";

/// The versions served after an `initialize` handshake, newest first.
pub const HANDSHAKE_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// `_meta` keys the 2026-07-28 revision defines.
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";
const META_SUBSCRIPTION_ID: &str = "io.modelcontextprotocol/subscriptionId";

/// JSON-RPC error numbers.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// MCP 2026-07-28: the request names a protocol version this server does not serve.
const UNSUPPORTED_VERSION: i64 = -32022;

/// A screenshot larger than this is not sent as an image (base64 makes it a third larger still,
/// and agents have limits on tool results).
pub const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

/// How long `tools/list` and the status tool wait for the app before giving up. Short, so a
/// stuck app does not hold up the agent's start.
const QUICK_TIMEOUT: Duration = Duration::from_secs(5);

const CALL_TOOL: &str = "prowave_call";
const STATUS_TOOL: &str = "prowave_status";

const INSTRUCTIONS: &str = "\
This server controls Dōri, a music production app (DAW) running on this Mac, through the \
app's control API. Each prowave_* tool calls one control method of the running app; the tool \
prowave_transport_play calls the method transport.play, and so on. Call prowave_rpc_describe \
(the method rpc.describe) for the full, current method list with every param; pass \
{\"method\": \"name\"} to see one method. prowave_call calls any method by name, including ones \
missing from the tool list. prowave_state_get reads the app's state. prowave_status reports \
whether the app can be reached. Dōri must already be running: this server never starts it, \
so if a tool says the app is not running, ask the user to start Dōri and try again.";

/// Which form of the protocol a request is served in.
#[derive(Clone, Copy, PartialEq)]
enum Era {
    /// After an `initialize` handshake (2025-11-25 and earlier).
    Handshake,
    /// Version and capabilities on every request (2026-07-28).
    PerRequest,
}

/// The MCP server's state.
pub struct Server {
    socket: PathBuf,
    /// Time limit for one call to the app. `None` waits without limit.
    timeout: Option<Duration>,
    /// The connection to the app, opened on first use and kept.
    client: Option<Client>,
    /// True once a client has sent `initialize`.
    handshake_done: bool,
    /// The method list the tools were last built from. Used to find the method behind a tool
    /// name.
    methods: Vec<Method>,
}

/// Runs the server until stdin closes. Returns the process exit code.
pub fn run(socket: PathBuf, timeout: Option<Duration>) -> i32 {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut server = Server::new(socket, timeout);
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            // Stdin closed: the client is done with us.
            Ok(0) => return 0,
            Ok(_) => {}
            Err(error) => {
                eprintln!("prowavectl mcp: cannot read stdin: {error}");
                return 0;
            }
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            // `to_string` never puts a raw newline inside the JSON, as the transport requires.
            let sent = writeln!(output, "{reply}").and_then(|()| output.flush());
            if sent.is_err() {
                // The client stopped reading: nothing left to do.
                return 0;
            }
        }
    }
}

fn error_reply(id: Value, code: i64, message: impl Into<String>, data: Option<Value>) -> Value {
    let mut error = json!({"code": code, "message": message.into()});
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": error})
}

fn server_info() -> Value {
    json!({
        "name": "prowavectl",
        "title": "Dōri control",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn text_content(text: impl Into<String>) -> Value {
    json!({"type": "text", "text": text.into()})
}

/// A tool result that reports a failure the agent can act on.
fn tool_error(text: impl Into<String>) -> Value {
    json!({"content": [text_content(text)], "isError": true})
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

impl Server {
    pub fn new(socket: PathBuf, timeout: Option<Duration>) -> Server {
        Server {
            socket,
            timeout,
            client: None,
            handshake_done: false,
            methods: Vec::new(),
        }
    }

    /// Handles one line from the client. Returns the message to send back, if any
    /// (notifications get no answer).
    pub fn handle_line(&mut self, line: &[u8]) -> Option<Value> {
        let message: Value = match serde_json::from_slice(line) {
            Ok(message) => message,
            Err(error) => {
                return Some(error_reply(
                    Value::Null,
                    PARSE_ERROR,
                    format!("Parse error: {error}"),
                    None,
                ))
            }
        };
        let Some(object) = message.as_object() else {
            return Some(error_reply(
                Value::Null,
                INVALID_REQUEST,
                "Invalid request: a message must be one JSON object",
                None,
            ));
        };
        let id = object.get("id");
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            if object.contains_key("result") || object.contains_key("error") {
                // An answer from the client. This server never asks the client anything.
                return None;
            }
            return Some(error_reply(
                id.cloned().unwrap_or(Value::Null),
                INVALID_REQUEST,
                "Invalid request: no method",
                None,
            ));
        };
        let Some(id) = id else {
            // A notification (notifications/initialized, notifications/cancelled, ...). Calls
            // are handled one at a time, so there is never anything running to cancel.
            return None;
        };
        if !(id.is_string() || id.is_number()) {
            return Some(error_reply(
                Value::Null,
                INVALID_REQUEST,
                "Invalid request: id must be a string or a number",
                None,
            ));
        }
        let empty = Map::new();
        let params = match object.get("params") {
            None => &empty,
            Some(Value::Object(params)) => params,
            Some(_) => {
                return Some(error_reply(
                    id.clone(),
                    INVALID_PARAMS,
                    "Invalid params: params must be an object",
                    None,
                ))
            }
        };
        self.handle_request(id, method, params)
    }

    fn handle_request(
        &mut self,
        id: &Value,
        method: &str,
        params: &Map<String, Value>,
    ) -> Option<Value> {
        if method == "initialize" {
            return Some(self.initialize(id, params));
        }
        let era = match self.era_of(method, params) {
            Ok(era) => era,
            Err((code, message, data)) => {
                return Some(error_reply(id.clone(), code, message, data))
            }
        };
        let result = match (method, era) {
            ("ping", _) => json!({}),
            ("server/discover", Era::PerRequest) => json!({
                "supportedVersions": [MODERN_VERSION],
                "capabilities": {"tools": {}},
                "instructions": INSTRUCTIONS,
                // The answer never changes while this process lives.
                "ttlMs": 3_600_000,
                "cacheScope": "public",
            }),
            ("subscriptions/listen", Era::PerRequest) => {
                // This server has no change notifications to send, so it agrees to none. The
                // request then stays open, as the specification describes, until the client
                // cancels it; there is no answer to send.
                return Some(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/subscriptions/acknowledged",
                    "params": {"_meta": {META_SUBSCRIPTION_ID: id}, "notifications": {}},
                }));
            }
            ("tools/list", _) => {
                let mut result = json!({"tools": self.tools()});
                if era == Era::PerRequest {
                    // The list changes when the app starts or is updated: do not cache it.
                    result["ttlMs"] = json!(0);
                    result["cacheScope"] = json!("public");
                }
                result
            }
            ("tools/call", _) => match self.tools_call(params) {
                Ok(result) => result,
                Err(message) => {
                    return Some(error_reply(id.clone(), INVALID_PARAMS, message, None))
                }
            },
            _ => {
                return Some(error_reply(
                    id.clone(),
                    METHOD_NOT_FOUND,
                    format!("Method not found: {method}"),
                    None,
                ))
            }
        };
        Some(self.result_reply(id, result, era))
    }

    /// Wraps a result. The per-request form wants `resultType` and the server's name on every
    /// result.
    fn result_reply(&self, id: &Value, mut result: Value, era: Era) -> Value {
        if era == Era::PerRequest {
            result["resultType"] = json!("complete");
            result["_meta"] = json!({META_SERVER_INFO: server_info()});
        }
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }

    /// Decides which form of the protocol a request uses, or why it must be refused.
    fn era_of(
        &self,
        method: &str,
        params: &Map<String, Value>,
    ) -> Result<Era, (i64, String, Option<Value>)> {
        let meta = params.get("_meta").and_then(Value::as_object);
        match meta.and_then(|meta| meta.get(META_VERSION)) {
            Some(Value::String(version)) if version == MODERN_VERSION => {
                let capabilities = meta.and_then(|meta| meta.get(META_CLIENT_CAPABILITIES));
                if capabilities.is_some_and(Value::is_object) {
                    Ok(Era::PerRequest)
                } else {
                    Err((
                        INVALID_PARAMS,
                        format!("Invalid params: _meta needs \"{META_CLIENT_CAPABILITIES}\" (an object)"),
                        None,
                    ))
                }
            }
            Some(Value::String(version)) => Err((
                UNSUPPORTED_VERSION,
                "Unsupported protocol version".to_string(),
                Some(json!({"supported": [MODERN_VERSION], "requested": version})),
            )),
            Some(_) => Err((
                INVALID_PARAMS,
                format!("Invalid params: _meta \"{META_VERSION}\" must be a string"),
                None,
            )),
            // No version on the request: fine after a handshake, and for a ping before one.
            None if self.handshake_done || method == "ping" => Ok(Era::Handshake),
            None => Err((
                INVALID_PARAMS,
                format!(
                    "Invalid params: the request names no protocol version. Either send \
                     initialize first (versions {}), or put \"{META_VERSION}\": \
                     \"{MODERN_VERSION}\" and \"{META_CLIENT_CAPABILITIES}\" in params._meta.",
                    HANDSHAKE_VERSIONS.join(", ")
                ),
                None,
            )),
        }
    }

    /// The `initialize` handshake of protocol versions 2025-11-25 and earlier.
    fn initialize(&mut self, id: &Value, params: &Map<String, Value>) -> Value {
        let Some(requested) = params.get("protocolVersion").and_then(Value::as_str) else {
            return error_reply(
                id.clone(),
                INVALID_PARAMS,
                "Invalid params: initialize needs a protocolVersion string",
                Some(json!({"supported": HANDSHAKE_VERSIONS})),
            );
        };
        // The rule: answer with the client's version when we serve it, otherwise with the
        // newest one we do serve; the client then decides whether it can go on.
        let version = HANDSHAKE_VERSIONS
            .iter()
            .find(|version| **version == requested)
            .unwrap_or(&HANDSHAKE_VERSIONS[0]);
        self.handshake_done = true;
        json!({"jsonrpc": "2.0", "id": id, "result": {
            "protocolVersion": version,
            "capabilities": {"tools": {}},
            "serverInfo": server_info(),
            "instructions": INSTRUCTIONS,
        }})
    }

    /// The tool list: one tool per control method, plus the two general tools. The methods come
    /// from the running app when it answers, otherwise from the list built into this program.
    fn tools(&mut self) -> Vec<Value> {
        self.refresh_methods();
        let mut tools = Vec::new();
        let mut names: Vec<String> = vec![CALL_TOOL.to_string(), STATUS_TOOL.to_string()];
        for method in &self.methods {
            let name = methods::tool_name(&method.name);
            if names.contains(&name) {
                eprintln!(
                    "prowavectl mcp: skipping {}: its tool name {name} is already taken",
                    method.name
                );
                continue;
            }
            names.push(name);
            tools.push(methods::tool_definition(method));
        }
        tools.push(json!({
            "name": CALL_TOOL,
            "description": "Call any Dōri control method by its name, for example \
                transport.play. Use this for a method that has no tool of its own. The method \
                rpc.describe lists every method and its params.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "method": {"type": "string",
                               "description": "The control method name, for example state.get."},
                    "params": {"type": "object",
                               "description": "The method's named parameters. Leave out when \
                                               the method takes none."},
                },
                "required": ["method"],
                "additionalProperties": false,
            },
        }));
        tools.push(json!({
            "name": STATUS_TOOL,
            "description": "Report whether the Dōri app can be reached, the control socket \
                path, and the app's version when it is running. This never starts the app.",
            "inputSchema": {"type": "object", "additionalProperties": false},
        }));
        tools
    }

    /// Loads the method list from the app, or from the built-in copy when the app does not
    /// answer.
    fn refresh_methods(&mut self) {
        let limit = Some(self.timeout.map_or(QUICK_TIMEOUT, |t| t.min(QUICK_TIMEOUT)));
        let from_app = match self.app_call("rpc.describe", None, limit) {
            Ok(describe) => methods::parse_methods(&describe),
            Err(failure) => Err(failure.to_string()),
        };
        match from_app.or_else(|reason| {
            eprintln!(
                "prowavectl mcp: using the built-in method list, the app did not give one: {reason}"
            );
            methods::embedded_methods()
        }) {
            Ok(list) => self.methods = list,
            Err(reason) => eprintln!("prowavectl mcp: no method list: {reason}"),
        }
    }

    /// Finds the control method a tool name stands for.
    fn method_for_tool(&mut self, tool: &str) -> Option<String> {
        let find = |list: &[Method]| {
            list.iter()
                .find(|method| methods::tool_name(&method.name) == tool)
                .map(|method| method.name.clone())
        };
        if let Some(name) = find(&self.methods) {
            return Some(name);
        }
        // Not in the list we have (or we have none yet): the app may have started since.
        self.refresh_methods();
        find(&self.methods)
    }

    /// Handles `tools/call`. An `Err` is a malformed request or an unknown tool, which the
    /// specification answers with error -32602; everything else is a tool result.
    fn tools_call(&mut self, params: &Map<String, Value>) -> Result<Value, String> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or("Invalid params: tools/call needs a tool name")?;
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(arguments)) => arguments.clone(),
            Some(_) => return Err("Invalid params: arguments must be an object".to_string()),
        };
        match name {
            STATUS_TOOL => Ok(self.status()),
            CALL_TOOL => {
                let Some(method) = arguments.get("method").and_then(Value::as_str) else {
                    return Ok(tool_error(
                        "prowave_call needs \"method\": the control method name as a string, \
                         for example {\"method\": \"state.get\"}.",
                    ));
                };
                let method_params = match arguments.get("params") {
                    None | Some(Value::Null) => None,
                    Some(params @ Value::Object(_)) => Some(params.clone()),
                    Some(_) => {
                        return Ok(tool_error(
                            "prowave_call: \"params\" must be an object of named parameters.",
                        ))
                    }
                };
                Ok(self.call_method(method, method_params))
            }
            _ => match self.method_for_tool(name) {
                Some(method) => Ok(self.call_method(&method, Some(Value::Object(arguments)))),
                None => Err(format!("Unknown tool: {name}")),
            },
        }
    }

    /// Calls a control method and turns the outcome into a tool result.
    fn call_method(&mut self, method: &str, params: Option<Value>) -> Value {
        let result = match self.app_call(method, params, self.timeout) {
            Ok(result) => result,
            Err(RpcFailure::App(error)) => {
                return tool_error(format!(
                    "error {}: {}\n{}",
                    error.code,
                    error.message,
                    pretty(&error.error)
                ))
            }
            Err(RpcFailure::Connect(error)) => {
                return tool_error(format!(
                    "Dōri cannot be reached: {error}\nStart Dōri, then try again. \
                     (This tool never starts the app itself.)"
                ))
            }
            Err(failure @ RpcFailure::Timeout { .. }) => {
                return tool_error(format!(
                    "{failure}. The call may still finish inside the app."
                ))
            }
            Err(failure @ RpcFailure::ConnectionLost(_)) => {
                return tool_error(format!(
                    "{failure}. The call may or may not have been carried out; check with \
                     prowave_status and prowave_state_get."
                ))
            }
        };

        let mut text = pretty(&result);
        let mut content = Vec::new();
        if method == "ui.screenshot" {
            match screenshot_image(&result) {
                Ok(image) => content.push(image),
                Err(note) => text.push_str(&format!("\n\n{note}")),
            }
        }
        content.insert(0, text_content(text));
        json!({"content": content, "isError": false})
    }

    /// The status tool: can the app be reached, where is the socket, which version runs.
    fn status(&mut self) -> Value {
        let limit = Some(self.timeout.map_or(QUICK_TIMEOUT, |t| t.min(QUICK_TIMEOUT)));
        let socket = self.socket.display().to_string();
        let report = match self.app_call("rpc.version", None, limit) {
            Ok(version) => json!({"reachable": true, "socket": socket, "version": version}),
            Err(failure) => {
                let mut reason = failure.to_string();
                if matches!(failure, RpcFailure::Connect(_)) {
                    reason.push_str(" Start Dōri to use the other tools.");
                }
                json!({"reachable": false, "socket": socket, "reason": reason})
            }
        };
        json!({"content": [text_content(pretty(&report))], "isError": false})
    }

    /// One call to the app. Opens the connection when there is none, and opens a fresh one when
    /// the kept one has gone dead (the app was restarted).
    fn app_call(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Option<Duration>,
    ) -> Result<Value, RpcFailure> {
        let mut client = match self.client.take() {
            Some(client) => client,
            None => Client::connect(&self.socket)?,
        };
        if !client.is_alive() {
            client = Client::connect(&self.socket)?;
        }
        let outcome = client.call(method, params, timeout);
        // Nobody reads events here; drop any that came in.
        client.discard_events();
        let keep = match &outcome {
            // After a timeout a late answer could still arrive; after a break the connection is
            // gone. Either way the next call starts on a fresh one.
            Err(RpcFailure::Timeout { .. } | RpcFailure::ConnectionLost(_)) => false,
            // A subscription would fill this connection with events nobody reads. Closing the
            // connection ends the subscription.
            _ => method != "events.subscribe",
        };
        if keep {
            self.client = Some(client);
        }
        outcome
    }
}

/// Reads the PNG a screenshot call wrote and makes it an MCP image item. The error is a note
/// for the text part of the result, saying why there is no image.
fn screenshot_image(result: &Value) -> Result<Value, String> {
    let path = result
        .get("path")
        .and_then(Value::as_str)
        .ok_or("No image attached: the result names no file path.")?;
    let size = std::fs::metadata(path)
        .map_err(|error| format!("No image attached: cannot read {path}: {error}"))?
        .len();
    if size > MAX_IMAGE_BYTES {
        return Err(format!(
            "No image attached: the file is {:.1} MB, over the {} MB limit for sending images. \
             Open the file at the path above instead.",
            size as f64 / (1024.0 * 1024.0),
            MAX_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|error| format!("No image attached: cannot read {path}: {error}"))?;
    Ok(json!({
        "type": "image",
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        "mimeType": "image/png",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server whose socket path leads nowhere: the app is "not running".
    fn server() -> Server {
        Server::new(
            PathBuf::from("/nonexistent-prowavectl-test/control.sock"),
            Some(Duration::from_secs(1)),
        )
    }

    fn send(server: &mut Server, message: Value) -> Option<Value> {
        server.handle_line(message.to_string().as_bytes())
    }

    fn modern_meta() -> Value {
        json!({META_VERSION: MODERN_VERSION, META_CLIENT_CAPABILITIES: {}})
    }

    #[test]
    fn handshake_echoes_a_served_version_and_offers_the_newest_otherwise() {
        for (asked, answered) in [
            ("2025-11-25", "2025-11-25"),
            ("2025-06-18", "2025-06-18"),
            ("2024-11-05", "2024-11-05"),
            ("1999-01-01", "2025-11-25"),
            ("2026-07-28", "2025-11-25"),
        ] {
            let mut server = server();
            let reply = send(
                &mut server,
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                    "protocolVersion": asked, "capabilities": {},
                    "clientInfo": {"name": "t", "version": "0"}}}),
            )
            .unwrap();
            assert_eq!(reply["result"]["protocolVersion"], answered, "{asked}");
            assert_eq!(reply["result"]["capabilities"], json!({"tools": {}}));
            assert_eq!(reply["result"]["serverInfo"]["name"], "prowavectl");
            assert!(reply["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("rpc.describe"));
            assert!(reply["result"].get("resultType").is_none());
        }
    }

    #[test]
    fn notifications_and_client_answers_get_no_reply() {
        let mut server = server();
        for message in [
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                   "params": {"requestId": 5}}),
            json!({"jsonrpc": "2.0", "method": "no/such/notification"}),
            json!({"jsonrpc": "2.0", "id": 9, "result": {}}),
        ] {
            assert_eq!(send(&mut server, message), None);
        }
    }

    #[test]
    fn malformed_messages() {
        let mut server = server();
        let reply = server.handle_line(b"{not json").unwrap();
        assert_eq!(reply["error"]["code"], PARSE_ERROR);
        assert_eq!(reply["id"], Value::Null);

        let reply = server.handle_line(b"[1, 2]").unwrap();
        assert_eq!(reply["error"]["code"], INVALID_REQUEST);

        let reply = send(&mut server, json!({"jsonrpc": "2.0", "id": 3})).unwrap();
        assert_eq!(reply["error"]["code"], INVALID_REQUEST);
        assert_eq!(reply["id"], 3);

        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": null, "method": "ping"}),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], INVALID_REQUEST);

        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 4, "method": "ping", "params": [1]}),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn requests_need_a_handshake_or_a_version() {
        let mut server = server();
        // A ping is allowed before the handshake.
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}),
        )
        .unwrap();
        assert_eq!(reply["result"], json!({}));

        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], INVALID_PARAMS);
        assert!(reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains(MODERN_VERSION));
    }

    #[test]
    fn per_request_form() {
        let mut server = server();
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": "d", "method": "server/discover",
                   "params": {"_meta": modern_meta()}}),
        )
        .unwrap();
        let result = &reply["result"];
        assert_eq!(reply["id"], "d");
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["supportedVersions"], json!([MODERN_VERSION]));
        assert_eq!(result["capabilities"], json!({"tools": {}}));
        assert_eq!(result["_meta"][META_SERVER_INFO]["name"], "prowavectl");
        assert!(result["ttlMs"].as_u64().is_some());
        assert_eq!(result["cacheScope"], "public");
        assert!(result["instructions"].is_string());

        // Tools without a handshake; the app is not running, so the built-in list is used.
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list",
                   "params": {"_meta": modern_meta()}}),
        )
        .unwrap();
        let result = &reply["result"];
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["ttlMs"], 0);
        let names: Vec<&str> = result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&CALL_TOOL));
        assert!(names.contains(&STATUS_TOOL));
        assert!(names.len() > 2);

        // A version this server does not serve.
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {"_meta": {
                META_VERSION: "2031-01-01", META_CLIENT_CAPABILITIES: {}}}}),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], UNSUPPORTED_VERSION);
        assert_eq!(
            reply["error"]["data"],
            json!({"supported": [MODERN_VERSION], "requested": "2031-01-01"})
        );

        // The capabilities field is required on every request.
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list",
                   "params": {"_meta": {META_VERSION: MODERN_VERSION}}}),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], INVALID_PARAMS);

        // A subscription is acknowledged with nothing agreed to, and stays unanswered.
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 5, "method": "subscriptions/listen", "params": {
                "_meta": modern_meta(), "notifications": {"toolsListChanged": true}}}),
        )
        .unwrap();
        assert_eq!(reply["method"], "notifications/subscriptions/acknowledged");
        assert_eq!(reply["params"]["_meta"][META_SUBSCRIPTION_ID], 5);
        assert_eq!(reply["params"]["notifications"], json!({}));
        assert!(reply.get("id").is_none());

        // Anything else is an unknown method.
        let reply = send(
            &mut server,
            json!({"jsonrpc": "2.0", "id": 6, "method": "nope/nope",
                   "params": {"_meta": modern_meta()}}),
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn tool_calls_without_the_app() {
        let mut server = server();
        let call = |server: &mut Server, id: u64, params: Value| {
            let mut params = params;
            params["_meta"] = modern_meta();
            send(
                server,
                json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": params}),
            )
            .unwrap()
        };

        let reply = call(
            &mut server,
            1,
            json!({"name": CALL_TOOL, "arguments": {"method": "state.get"}}),
        );
        assert_eq!(reply["result"]["isError"], true);
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Start Dōri"), "{text}");
        assert!(text.contains("not running"), "{text}");

        let reply = call(
            &mut server,
            2,
            json!({"name": STATUS_TOOL, "arguments": {}}),
        );
        assert_eq!(reply["result"]["isError"], false);
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        let report: Value = serde_json::from_str(text).unwrap();
        assert_eq!(report["reachable"], false);
        assert_eq!(
            report["socket"],
            "/nonexistent-prowavectl-test/control.sock"
        );

        let reply = call(&mut server, 3, json!({"name": "prowave_no_such_thing"}));
        assert_eq!(reply["error"]["code"], INVALID_PARAMS);
        assert_eq!(
            reply["error"]["message"],
            "Unknown tool: prowave_no_such_thing"
        );

        let reply = call(&mut server, 4, json!({"arguments": {}}));
        assert_eq!(reply["error"]["code"], INVALID_PARAMS);
        let reply = call(&mut server, 5, json!({"name": CALL_TOOL, "arguments": [1]}));
        assert_eq!(reply["error"]["code"], INVALID_PARAMS);

        // Wrong input to a tool is a tool result the agent can read, not a protocol error.
        let reply = call(&mut server, 6, json!({"name": CALL_TOOL, "arguments": {}}));
        assert_eq!(reply["result"]["isError"], true);
        let reply = call(
            &mut server,
            7,
            json!({"name": CALL_TOOL, "arguments": {"method": "a.b", "params": 5}}),
        );
        assert_eq!(reply["result"]["isError"], true);
    }

    #[test]
    fn screenshot_notes() {
        assert!(screenshot_image(&json!({}))
            .unwrap_err()
            .contains("no file path"));
        assert!(
            screenshot_image(&json!({"path": "/nonexistent-prowavectl-test/a.png"}))
                .unwrap_err()
                .contains("cannot read")
        );
    }
}
