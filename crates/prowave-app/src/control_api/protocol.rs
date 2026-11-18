//! The wire format: newline-delimited JSON-RPC 2.0. One request a line, one response a line.
//! An error carries the JSON-RPC number, a plain-English message and, in `data.code`, a name
//! that does not change (see [`Code`]).
use serde_json::{json, Map, Value};

/// The protocol's version, reported by `rpc.version` and `rpc.describe`. It goes up when a
/// method is removed or changes meaning; new methods do not change it.
pub const PROTOCOL: u32 = 1;

/// Longest request line, in bytes. A longer line is answered with [`Code::TooLarge`] and
/// skipped; the connection stays open.
pub const MAX_LINE: usize = 8 * 1024 * 1024;

/// What went wrong, as a name that stays the same (`data.code`) and its JSON-RPC number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Code {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Internal,
    NotFound,
    NotSupported,
    Failed,
    Timeout,
    Busy,
    TooLarge,
    UnsavedChanges,
    DialogOpen,
}

impl Code {
    pub const ALL: [Code; 13] = [
        Code::ParseError,
        Code::InvalidRequest,
        Code::MethodNotFound,
        Code::InvalidParams,
        Code::Internal,
        Code::NotFound,
        Code::NotSupported,
        Code::Failed,
        Code::Timeout,
        Code::Busy,
        Code::TooLarge,
        Code::UnsavedChanges,
        Code::DialogOpen,
    ];

    pub fn number(self) -> i64 {
        match self {
            Code::ParseError => -32700,
            Code::InvalidRequest => -32600,
            Code::MethodNotFound => -32601,
            Code::InvalidParams => -32602,
            Code::Internal => -32603,
            Code::NotFound => -32000,
            Code::NotSupported => -32001,
            Code::Failed => -32002,
            Code::Timeout => -32003,
            Code::Busy => -32004,
            Code::TooLarge => -32005,
            Code::UnsavedChanges => -32006,
            Code::DialogOpen => -32007,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Code::ParseError => "parse_error",
            Code::InvalidRequest => "invalid_request",
            Code::MethodNotFound => "method_not_found",
            Code::InvalidParams => "invalid_params",
            Code::Internal => "internal_error",
            Code::NotFound => "not_found",
            Code::NotSupported => "not_supported",
            Code::Failed => "failed",
            Code::Timeout => "timeout",
            Code::Busy => "busy",
            Code::TooLarge => "too_large",
            Code::UnsavedChanges => "unsaved_changes",
            Code::DialogOpen => "dialog_open",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Code::ParseError => "The line is not JSON.",
            Code::InvalidRequest => "The line is JSON but not a JSON-RPC 2.0 request.",
            Code::MethodNotFound => "No method has that name (rpc.describe lists them).",
            Code::InvalidParams => {
                "A parameter is missing, unknown, of the wrong type or out of range."
            }
            Code::Internal => "The app failed while running the method (a bug).",
            Code::NotFound => "The track, region, effect, marker or file named does not exist.",
            Code::NotSupported => "The app cannot do this (here, or for this kind of thing).",
            Code::Failed => "The app tried and could not (the message says why).",
            Code::Timeout => "The work did not finish in time.",
            Code::Busy => "The app is doing something that must finish first.",
            Code::TooLarge => "The request line is longer than the limit.",
            Code::UnsavedChanges => {
                "The project has unsaved changes and the call did not say what to do with them."
            }
            Code::DialogOpen => {
                "A question is open in the app and must be answered first (ui.dialog, ui.dialog.answer)."
            }
        }
    }
}

/// An error response.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    pub code: Code,
    pub message: String,
    /// More fields for `data`, beside `code`.
    pub data: Map<String, Value>,
}

impl RpcError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: Map::new(),
        }
    }

    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.data.insert(key.to_string(), value.into());
        self
    }

    pub fn invalid(param: &str, message: impl Into<String>) -> Self {
        Self::new(Code::InvalidParams, message).with("param", param)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(Code::NotFound, message)
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(Code::Failed, message)
    }

    pub fn to_json(&self) -> Value {
        let mut data = self.data.clone();
        data.insert("code".into(), self.code.name().into());
        json!({"code": self.code.number(), "message": self.message, "data": data})
    }
}

/// A request read from a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// `None`: a notification, which gets no response.
    pub id: Option<Value>,
    pub method: String,
    pub params: Map<String, Value>,
}

/// Reads one line. On failure: the id to answer with (null when it could not be read) and
/// the error.
pub fn parse(line: &str) -> Result<Request, (Value, RpcError)> {
    let v: Value = serde_json::from_str(line).map_err(|e| {
        (
            Value::Null,
            RpcError::new(Code::ParseError, format!("the line is not JSON: {e}")),
        )
    })?;
    let Value::Object(mut o) = v else {
        return Err((
            Value::Null,
            RpcError::new(
                Code::InvalidRequest,
                "a request is a JSON object: {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"...\",\"params\":{...}} (batches go through the method `batch`)",
            ),
        ));
    };
    let id = o.remove("id");
    let reply_id = id.clone().unwrap_or(Value::Null);
    if let Some(id) = &id {
        if !(id.is_string() || id.is_number() || id.is_null()) {
            return Err((
                Value::Null,
                RpcError::new(Code::InvalidRequest, "`id` must be a string or a number"),
            ));
        }
    }
    if o.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err((
            reply_id,
            RpcError::new(Code::InvalidRequest, "`jsonrpc` must be the string \"2.0\""),
        ));
    }
    let Some(Value::String(method)) = o.remove("method") else {
        return Err((
            reply_id,
            RpcError::new(Code::InvalidRequest, "`method` must be a string"),
        ));
    };
    let params = match o.remove("params") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(m)) => m,
        Some(_) => {
            return Err((
                reply_id,
                RpcError::new(
                    Code::InvalidParams,
                    "`params` must be an object of named parameters, for example {\"bpm\":120}",
                ),
            ))
        }
    };
    Ok(Request { id, method, params })
}

pub fn response(id: &Value, result: &Result<Value, RpcError>) -> String {
    let v = match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e.to_json()}),
    };
    v.to_string()
}

/// A notification to a subscribed connection.
pub fn event(name: &str, mut fields: Map<String, Value>) -> String {
    fields.insert("event".into(), name.into());
    json!({"jsonrpc": "2.0", "method": "event", "params": fields}).to_string()
}
