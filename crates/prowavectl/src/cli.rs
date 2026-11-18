//! The commands of the `prowavectl` program. `run` returns the process exit code:
//! 0 ok, 1 the app answered with an error, 2 usage, 3 Pro Wave is not running,
//! 4 timeout or connection lost.

use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::args::{self, Command, ParamsArg};
use crate::client::{Client, RpcFailure};
use crate::mcp;
use crate::methods::{self, Method};

const EXIT_OK: i32 = 0;
const EXIT_APP_ERROR: i32 = 1;
const EXIT_USAGE: i32 = 2;

/// Runs the program with the process's own arguments.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let invocation = match args::parse(&args) {
        Ok(invocation) => invocation,
        Err(message) => return usage_error(&message),
    };
    match invocation.command {
        Command::Help => return print(args::HELP),
        Command::Version => return print(&format!("prowavectl {}\n", env!("CARGO_PKG_VERSION"))),
        _ => {}
    }
    let socket = match args::resolve_socket(
        invocation.socket.as_deref(),
        std::env::var_os(args::SOCKET_ENV).as_deref(),
        std::env::var_os("HOME").as_deref(),
    ) {
        Ok(socket) => socket,
        Err(message) => return usage_error(&message),
    };
    let timeout = invocation.timeout;
    match invocation.command {
        Command::Help | Command::Version => EXIT_OK,
        Command::Methods => list_methods(&socket, timeout),
        Command::Describe { method } => describe_method(&socket, timeout, &method),
        Command::Call { method, params } => {
            let params = match params {
                ParamsArg::None => None,
                ParamsArg::Inline(text) => Some(text),
                ParamsArg::Stdin => {
                    let mut text = String::new();
                    if let Err(error) = io::stdin().read_to_string(&mut text) {
                        return usage_error(&format!("cannot read params from stdin: {error}"));
                    }
                    Some(text)
                }
            };
            // Bad params are a usage error, found before the app is contacted.
            let params = match params.map(|text| args::parse_params(&text)).transpose() {
                Ok(params) => params,
                Err(message) => return usage_error(&message),
            };
            call(&socket, timeout, &method, params)
        }
        Command::State => call(&socket, timeout, "state.get", None),
        Command::Events => events(&socket, timeout),
        Command::Mcp => mcp::run(socket, timeout),
    }
}

fn usage_error(message: &str) -> i32 {
    eprintln!("prowavectl: {message}");
    eprintln!("Run `prowavectl help` for the commands.");
    EXIT_USAGE
}

/// Writes to stdout. A reader that has gone away (a closed pipe) is not an error.
fn print(text: &str) -> i32 {
    let mut out = io::stdout().lock();
    let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
    EXIT_OK
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Prints a failed call on stderr and returns its exit code.
fn report(failure: &RpcFailure) -> i32 {
    match failure {
        RpcFailure::App(error) => {
            eprintln!("error {}: {}", error.code, error.message);
            eprintln!("{}", pretty(&error.error));
        }
        other => eprintln!("prowavectl: {other}"),
    }
    failure.exit_code()
}

fn connect_and_call(
    socket: &Path,
    timeout: Option<Duration>,
    method: &str,
    params: Option<Value>,
) -> Result<Value, RpcFailure> {
    Client::connect(socket)?.call(method, params, timeout)
}

/// `call` and `state`: print the result as indented JSON.
fn call(socket: &Path, timeout: Option<Duration>, method: &str, params: Option<Value>) -> i32 {
    match connect_and_call(socket, timeout, method, params) {
        Ok(result) => print(&format!("{}\n", pretty(&result))),
        Err(failure) => report(&failure),
    }
}

/// Gets the method list from the running app. When the app is not running, falls back to the
/// list built into this program and says so on stderr.
fn load_methods(
    socket: &Path,
    timeout: Option<Duration>,
    only: Option<&str>,
) -> Result<Vec<Method>, i32> {
    let params = only.map(|method| json!({"method": method}));
    let broken_list = |message: String| {
        eprintln!("prowavectl: {message}");
        EXIT_APP_ERROR
    };
    match connect_and_call(socket, timeout, "rpc.describe", params) {
        Ok(describe) => methods::parse_methods(&describe).map_err(broken_list),
        Err(RpcFailure::Connect(error)) => {
            eprintln!("prowavectl: {error}");
            eprintln!(
                "prowavectl: Pro Wave is not running, so this is the method list built into \
                 prowavectl; the app's own list may differ."
            );
            let mut list = methods::embedded_methods().map_err(broken_list)?;
            if let Some(only) = only {
                list.retain(|method| method.name == only);
            }
            Ok(list)
        }
        Err(failure) => Err(report(&failure)),
    }
}

fn list_methods(socket: &Path, timeout: Option<Duration>) -> i32 {
    match load_methods(socket, timeout, None) {
        Ok(list) => print(&methods::format_method_list(&list)),
        Err(code) => code,
    }
}

fn describe_method(socket: &Path, timeout: Option<Duration>, name: &str) -> i32 {
    let list = match load_methods(socket, timeout, Some(name)) {
        Ok(list) => list,
        Err(code) => return code,
    };
    match list.iter().find(|method| method.name == name) {
        Some(method) => print(&methods::format_method(method)),
        None => {
            eprintln!("error not_found: there is no method named {name}");
            eprintln!("Run `prowavectl methods` for the list.");
            EXIT_APP_ERROR
        }
    }
}

/// Ends the process with exit code 0. Runs inside a signal handler.
extern "C" fn exit_quietly(_signal: libc::c_int) {
    // SAFETY: `_exit` is one of the few functions allowed inside a signal handler; it ends the
    // process at once without touching any shared state. Each event line is flushed as it is
    // printed, so nothing is lost.
    unsafe { libc::_exit(EXIT_OK) }
}

/// `events`: subscribe, then print each event's fields as one JSON line until interrupted
/// (exit 0) or until the app goes away (exit 4).
fn events(socket: &Path, timeout: Option<Duration>) -> i32 {
    // Ctrl-C is the normal way to stop this command, so it is a clean exit.
    let handler = exit_quietly as extern "C" fn(libc::c_int);
    // SAFETY: installs a handler that only calls `_exit` (see `exit_quietly`).
    unsafe {
        libc::signal(libc::SIGINT, handler as libc::sighandler_t);
    }

    let mut client = match Client::connect(socket) {
        Ok(client) => client,
        Err(error) => return report(&error.into()),
    };
    if let Err(failure) = client.call("events.subscribe", None, timeout) {
        return report(&failure);
    }
    let mut out = io::stdout().lock();
    loop {
        match client.next_event() {
            Ok(event) => {
                // Compact JSON has no raw newlines: one event, one line. Flushed at once so a
                // reader on a pipe sees each event when it happens.
                if writeln!(out, "{event}").and_then(|()| out.flush()).is_err() {
                    // The reader closed the pipe: done.
                    return EXIT_OK;
                }
            }
            Err(failure) => return report(&failure),
        }
    }
}
