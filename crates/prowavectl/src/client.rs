//! The connection to Dōri's control socket.
//!
//! The app speaks JSON-RPC 2.0, one JSON object per line. A request goes out as one line and the
//! answer comes back as one line carrying the same id. After `events.subscribe` the app also
//! writes event lines (no id) whenever something happens; they can arrive before the answer to a
//! later request, so a call keeps reading until it sees its own id and sets events aside.

use std::collections::VecDeque;
use std::fmt;
use std::io::{BufRead, BufReader, ErrorKind, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(not(unix))]
#[derive(Debug)]
pub struct UnixStream;

#[cfg(not(unix))]
impl std::io::Read for UnixStream {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix sockets not supported on Windows",
        ))
    }
}

#[cfg(not(unix))]
impl std::io::Write for UnixStream {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix sockets not supported on Windows",
        ))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(not(unix))]
impl<'a> std::io::Write for &'a UnixStream {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix sockets not supported on Windows",
        ))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(not(unix))]
impl UnixStream {
    pub fn connect<P: AsRef<Path>>(_path: P) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix sockets not supported on Windows",
        ))
    }
    pub fn set_read_timeout(&self, _timeout: Option<Duration>) -> std::io::Result<()> {
        Ok(())
    }
    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> std::io::Result<()> {
        Ok(())
    }
    pub fn set_nonblocking(&self, _nonblocking: bool) -> std::io::Result<()> {
        Ok(())
    }
}

use serde_json::{json, Value};

/// Why the socket could not be opened.
#[derive(Debug)]
pub enum ConnectError {
    /// There is no socket file: the app is not running, or runs with its control API off.
    NotRunning(PathBuf),
    /// The socket file is there but nothing listens on it: left behind by an app that has gone.
    Stale(PathBuf),
    /// The file is there but this user may not open it.
    Permission(PathBuf, String),
    /// Anything else (path too long, not a socket, ...).
    Other(PathBuf, String),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectError::NotRunning(path) => write!(
                f,
                "socket file does not exist: Dōri is not running (or was started with \
                 PROWAVE_NO_CONTROL=1). Socket: {}",
                path.display()
            ),
            ConnectError::Stale(path) => write!(
                f,
                "socket file exists but nothing answers: it is stale, Dōri is not running. \
                 Socket: {}",
                path.display()
            ),
            ConnectError::Permission(path, detail) => write!(
                f,
                "not allowed to open the control socket ({detail}): it belongs to another user \
                 or its folder is not readable. Socket: {}",
                path.display()
            ),
            ConnectError::Other(path, detail) => write!(
                f,
                "cannot open the control socket: {detail}. Socket: {}",
                path.display()
            ),
        }
    }
}

/// An error answer from the app.
#[derive(Debug, Clone, PartialEq)]
pub struct AppError {
    /// The JSON-RPC error number, for example -32602.
    pub number: i64,
    /// The stable machine name from `data.code`, for example `invalid_params`. When the app sent
    /// none, this is the number as text.
    pub code: String,
    /// The plain-English message.
    pub message: String,
    /// The whole `error` object as the app sent it.
    pub error: Value,
}

/// Why a call produced no result.
#[derive(Debug)]
pub enum RpcFailure {
    /// The socket could not be opened.
    Connect(ConnectError),
    /// The app answered with an error.
    App(AppError),
    /// No answer within the time limit.
    Timeout { method: String, after: Duration },
    /// The connection broke, or the app sent something that is not a JSON line.
    ConnectionLost(String),
}

impl RpcFailure {
    /// The process exit code for this failure (see the help text).
    pub fn exit_code(&self) -> i32 {
        match self {
            RpcFailure::App(_) => 1,
            RpcFailure::Connect(_) => 3,
            RpcFailure::Timeout { .. } | RpcFailure::ConnectionLost(_) => 4,
        }
    }
}

impl fmt::Display for RpcFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcFailure::Connect(error) => write!(f, "{error}"),
            RpcFailure::App(error) => write!(f, "error {}: {}", error.code, error.message),
            RpcFailure::Timeout { method, after } => write!(
                f,
                "timeout: no answer to {method} after {} s (Dōri may be busy or stuck)",
                after.as_secs_f64()
            ),
            RpcFailure::ConnectionLost(detail) => write!(f, "connection lost: {detail}"),
        }
    }
}

impl From<ConnectError> for RpcFailure {
    fn from(error: ConnectError) -> Self {
        RpcFailure::Connect(error)
    }
}

/// Why reading one line stopped without a line.
enum ReadStop {
    TimedOut,
    Lost(String),
}

/// One open connection to the app.
pub struct Client {
    reader: BufReader<UnixStream>,
    /// Bytes of a line that has not ended yet. Kept between reads so a timeout in the middle of a
    /// long line loses nothing. There is no size limit: state dumps run to several megabytes.
    partial: Vec<u8>,
    next_id: u64,
    /// Events that arrived while a call was waiting for its answer, oldest first.
    events: VecDeque<Value>,
}

impl Client {
    /// Opens the socket. This never starts the app.
    pub fn connect(path: &Path) -> Result<Client, ConnectError> {
        let stream = UnixStream::connect(path).map_err(|error| {
            let path = path.to_path_buf();
            match error.kind() {
                ErrorKind::NotFound => ConnectError::NotRunning(path),
                ErrorKind::ConnectionRefused => ConnectError::Stale(path),
                ErrorKind::PermissionDenied => ConnectError::Permission(path, error.to_string()),
                _ => ConnectError::Other(path, error.to_string()),
            }
        })?;
        Ok(Client {
            reader: BufReader::new(stream),
            partial: Vec::new(),
            next_id: 1,
            events: VecDeque::new(),
        })
    }

    /// Calls one method and waits for its answer. `params` is a JSON object or `None`.
    /// `timeout` of `None` waits without limit.
    pub fn call(
        &mut self,
        method: &str,
        params: Option<Value>,
        timeout: Option<Duration>,
    ) -> Result<Value, RpcFailure> {
        let timed_out = || RpcFailure::Timeout {
            method: method.to_string(),
            after: timeout.unwrap_or_default(),
        };
        let deadline = timeout.map(|limit| Instant::now() + limit);

        let id = self.next_id;
        self.next_id += 1;
        let mut request = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if let Some(params) = params {
            request["params"] = params;
        }
        let mut line = request.to_string().into_bytes();
        line.push(b'\n');

        let stream = self.reader.get_ref();
        stream
            .set_write_timeout(timeout.filter(|limit| !limit.is_zero()))
            .map_err(|error| RpcFailure::ConnectionLost(error.to_string()))?;
        let mut writer: &UnixStream = stream;
        if let Err(error) = writer.write_all(&line).and_then(|()| writer.flush()) {
            return Err(match error.kind() {
                ErrorKind::WouldBlock | ErrorKind::TimedOut => timed_out(),
                _ => RpcFailure::ConnectionLost(format!("could not send the request: {error}")),
            });
        }

        loop {
            let line = match self.read_line(deadline) {
                Ok(line) => line,
                Err(ReadStop::TimedOut) => return Err(timed_out()),
                Err(ReadStop::Lost(detail)) => return Err(RpcFailure::ConnectionLost(detail)),
            };
            let Some(mut message) = parse_line(&line)? else {
                continue;
            };
            match message.get("id") {
                Some(got) if got.as_u64() == Some(id) => return answer_to_result(message),
                // An error with a null id: the app could not read our request at all. Only one
                // request is in flight on this connection, so it is ours.
                Some(Value::Null) if message.get("error").is_some() => {
                    return answer_to_result(message)
                }
                None if message.get("method").and_then(Value::as_str) == Some("event") => {
                    self.events.push_back(message["params"].take());
                }
                // An answer to some other id, or a line we do not know: not ours, skip it.
                _ => {}
            }
        }
    }

    /// Waits for the next event and returns its `params`. Blocks without limit; fails when the
    /// app closes the connection.
    pub fn next_event(&mut self) -> Result<Value, RpcFailure> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Ok(event);
            }
            let line = match self.read_line(None) {
                Ok(line) => line,
                Err(ReadStop::TimedOut) => continue,
                Err(ReadStop::Lost(detail)) => return Err(RpcFailure::ConnectionLost(detail)),
            };
            let Some(mut message) = parse_line(&line)? else {
                continue;
            };
            if message.get("id").is_none()
                && message.get("method").and_then(Value::as_str) == Some("event")
            {
                self.events.push_back(message["params"].take());
            }
        }
    }

    /// Throws away events that were set aside during calls.
    pub fn discard_events(&mut self) {
        self.events.clear();
    }

    /// Checks, without waiting, whether the app still holds the other end. A connection kept
    /// open across an app restart fails this check.
    pub fn is_alive(&mut self) -> bool {
        if !self.reader.buffer().is_empty() {
            return true;
        }
        if self.reader.get_ref().set_nonblocking(true).is_err() {
            return false;
        }
        let alive = match self.reader.fill_buf() {
            // No bytes and no error: the other end has closed.
            Ok(bytes) => !bytes.is_empty(),
            Err(error) => error.kind() == ErrorKind::WouldBlock,
        };
        self.reader.get_ref().set_nonblocking(false).is_ok() && alive
    }

    /// Reads one whole line (without its newline), waiting until `deadline` at most.
    fn read_line(&mut self, deadline: Option<Instant>) -> Result<Vec<u8>, ReadStop> {
        loop {
            let wait = match deadline {
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(ReadStop::TimedOut);
                    }
                    Some(left)
                }
                None => None,
            };
            self.reader
                .get_ref()
                .set_read_timeout(wait)
                .map_err(|error| ReadStop::Lost(error.to_string()))?;
            match self.reader.read_until(b'\n', &mut self.partial) {
                Ok(_) if self.partial.last() == Some(&b'\n') => {
                    let mut line = std::mem::take(&mut self.partial);
                    line.pop();
                    return Ok(line);
                }
                // The stream ended before the line did.
                Ok(_) => {
                    return Err(ReadStop::Lost(
                        "Dōri closed the connection (it may have quit)".to_string(),
                    ))
                }
                // The read timeout ran out; what was read so far stays in `partial`.
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(error) => return Err(ReadStop::Lost(error.to_string())),
            }
        }
    }
}

/// Parses one line from the app. Blank lines give `None`.
fn parse_line(line: &[u8]) -> Result<Option<Value>, RpcFailure> {
    if line.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    serde_json::from_slice(line).map(Some).map_err(|error| {
        RpcFailure::ConnectionLost(format!("Dōri sent a line that is not JSON: {error}"))
    })
}

/// Splits an answer into its result or its error.
fn answer_to_result(mut message: Value) -> Result<Value, RpcFailure> {
    let Some(error) = message.get_mut("error").map(Value::take) else {
        return Ok(message
            .get_mut("result")
            .map(Value::take)
            .unwrap_or(Value::Null));
    };
    let number = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    let code = error
        .pointer("/data/code")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| number.to_string());
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("the app gave no message")
        .to_string();
    Err(RpcFailure::App(AppError {
        number,
        code,
        message,
        error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_split_into_result_or_error() {
        let ok = json!({"jsonrpc": "2.0", "id": 1, "result": {"a": 1}});
        assert_eq!(answer_to_result(ok).unwrap(), json!({"a": 1}));

        let bad = json!({"jsonrpc": "2.0", "id": 1, "error": {
            "code": -32602, "message": "bar must be 1 or more",
            "data": {"code": "invalid_params", "param": "bar"}}});
        match answer_to_result(bad) {
            Err(RpcFailure::App(error)) => {
                assert_eq!(error.number, -32602);
                assert_eq!(error.code, "invalid_params");
                assert_eq!(error.message, "bar must be 1 or more");
                assert_eq!(error.error["data"]["param"], "bar");
            }
            other => panic!("expected an app error, got {other:?}"),
        }

        // No data.code: fall back to the number.
        let bare = json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32000, "message": "x"}});
        match answer_to_result(bare) {
            Err(RpcFailure::App(error)) => assert_eq!(error.code, "-32000"),
            other => panic!("expected an app error, got {other:?}"),
        }
    }

    #[test]
    fn exit_codes() {
        let app = RpcFailure::App(AppError {
            number: -32000,
            code: "failed".into(),
            message: "x".into(),
            error: Value::Null,
        });
        assert_eq!(app.exit_code(), 1);
        assert_eq!(
            RpcFailure::Connect(ConnectError::NotRunning("/x".into())).exit_code(),
            3
        );
        assert_eq!(
            RpcFailure::Connect(ConnectError::Stale("/x".into())).exit_code(),
            3
        );
        let timeout = RpcFailure::Timeout {
            method: "m".into(),
            after: Duration::from_secs(1),
        };
        assert_eq!(timeout.exit_code(), 4);
        assert_eq!(RpcFailure::ConnectionLost("x".into()).exit_code(), 4);
    }
}
