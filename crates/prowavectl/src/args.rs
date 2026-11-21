//! Command-line parsing and the choice of control socket path.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

/// How long a call waits for the app's answer unless `--timeout` says otherwise.
pub const DEFAULT_TIMEOUT_SECS: f64 = 120.0;

/// Environment variable that overrides the default socket path (the `--socket` flag wins).
pub const SOCKET_ENV: &str = "PROWAVE_CONTROL_SOCKET";

/// The socket path below the home folder.
const SOCKET_SUFFIX: &str = "Library/Application Support/prowave-mac/control.sock";

pub const HELP: &str = "\
prowavectl: command-line client and MCP server for Pro Wave's control API.

Usage:
  prowavectl [--socket PATH] [--timeout SECONDS] <command>

Commands:
  methods                      list every control method, grouped
  describe <method>            show one method: params, types, result
  call <method> ['<json>']     call a method; params are a JSON object, or - to read them from stdin
  state                        same as: call state.get
  events                       subscribe and print each event as one JSON line
  mcp                          run as an MCP server on stdin/stdout
  help, --help, -h             show this text
  --version                    show the version

Options:
  --socket PATH                control socket (default: $PROWAVE_CONTROL_SOCKET, else
                               ~/Library/Application Support/prowave-mac/control.sock)
  --timeout SECONDS            wait this long for an answer (default 120, 0 = no limit)

Exit codes:
  0 ok, 1 the app answered with an error, 2 usage, 3 Pro Wave is not running,
  4 timeout or connection lost

This tool never starts Pro Wave.
";

#[derive(Debug, PartialEq)]
pub enum ParamsArg {
    /// No params given.
    None,
    /// Params JSON given on the command line.
    Inline(String),
    /// `-`: read the params JSON from stdin.
    Stdin,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Version,
    Methods,
    Describe { method: String },
    Call { method: String, params: ParamsArg },
    State,
    Events,
    Mcp,
}

#[derive(Debug, PartialEq)]
pub struct Invocation {
    /// Value of `--socket`, if given.
    pub socket: Option<String>,
    /// `None` means wait without limit.
    pub timeout: Option<Duration>,
    pub command: Command,
}

/// Parses the arguments after the program name. The error is a one-line usage message.
pub fn parse(args: &[String]) -> Result<Invocation, String> {
    let mut socket = None;
    let mut timeout = parse_timeout(&DEFAULT_TIMEOUT_SECS.to_string())?;
    let mut positional: Vec<&str> = Vec::new();
    let mut special = None;

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        i += 1;
        if arg == "--help" || arg == "-h" {
            special = Some(Command::Help);
        } else if arg == "--version" || arg == "-V" {
            special.get_or_insert(Command::Version);
        } else if arg == "--socket" || arg == "--timeout" {
            let value = args
                .get(i)
                .ok_or_else(|| format!("{arg} needs a value"))?
                .as_str();
            i += 1;
            if arg == "--socket" {
                socket = Some(non_empty_socket(value)?);
            } else {
                timeout = parse_timeout(value)?;
            }
        } else if let Some(value) = arg.strip_prefix("--socket=") {
            socket = Some(non_empty_socket(value)?);
        } else if let Some(value) = arg.strip_prefix("--timeout=") {
            timeout = parse_timeout(value)?;
        } else if arg.starts_with('-') && arg != "-" {
            return Err(format!("unknown option {arg}"));
        } else {
            positional.push(arg);
        }
    }

    let command = match special {
        Some(command) => command,
        None => parse_command(&positional)?,
    };
    Ok(Invocation {
        socket,
        timeout,
        command,
    })
}

fn parse_command(positional: &[&str]) -> Result<Command, String> {
    let (name, rest) = positional
        .split_first()
        .ok_or_else(|| "no command given".to_string())?;
    let no_more = |command: Command| {
        if rest.is_empty() {
            Ok(command)
        } else {
            Err(format!("{name} takes no arguments"))
        }
    };
    match *name {
        "help" => Ok(Command::Help),
        "methods" => no_more(Command::Methods),
        "state" => no_more(Command::State),
        "events" => no_more(Command::Events),
        "mcp" => no_more(Command::Mcp),
        "describe" => match rest {
            [method] => Ok(Command::Describe {
                method: method.to_string(),
            }),
            _ => Err("usage: prowavectl describe <method>".to_string()),
        },
        "call" => {
            let usage = "usage: prowavectl call <method> ['<json params>' | -]";
            let (method, params) = match rest {
                [method] => (method, ParamsArg::None),
                [method, "-"] => (method, ParamsArg::Stdin),
                [method, json] => (method, ParamsArg::Inline(json.to_string())),
                _ => return Err(usage.to_string()),
            };
            if *method == "-" {
                return Err(usage.to_string());
            }
            Ok(Command::Call {
                method: method.to_string(),
                params,
            })
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn non_empty_socket(value: &str) -> Result<String, String> {
    if value.is_empty() {
        Err("--socket needs a path".to_string())
    } else {
        Ok(value.to_string())
    }
}

fn parse_timeout(value: &str) -> Result<Option<Duration>, String> {
    let seconds: f64 = value
        .parse()
        .map_err(|_| format!("--timeout needs a number of seconds, got {value}"))?;
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(format!(
            "--timeout needs zero or a positive number of seconds, got {value}"
        ));
    }
    if seconds == 0.0 {
        return Ok(None);
    }
    Duration::try_from_secs_f64(seconds)
        .map(Some)
        .map_err(|_| format!("--timeout {value} is too large"))
}

/// Parses call params: they must be one JSON object.
pub fn parse_params(text: &str) -> Result<Value, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() => Ok(value),
        Ok(_) => Err("params must be a JSON object, for example '{\"bar\": 5}'".to_string()),
        Err(error) => Err(format!("params are not valid JSON: {error}")),
    }
}

/// Picks the socket path: the flag, else the environment variable, else the default under the
/// home folder. Empty values count as not set.
pub fn resolve_socket(
    flag: Option<&str>,
    env: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, String> {
    if let Some(flag) = flag.filter(|f| !f.is_empty()) {
        return Ok(PathBuf::from(flag));
    }
    if let Some(env) = env.filter(|e| !e.is_empty()) {
        return Ok(PathBuf::from(env));
    }
    match home.filter(|h| !h.is_empty()) {
        Some(home) => Ok(PathBuf::from(home).join(SOCKET_SUFFIX)),
        None => Err(format!(
            "cannot find the control socket: HOME is not set; pass --socket PATH or set {SOCKET_ENV}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Invocation, String> {
        parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn plain_commands() {
        for (word, command) in [
            ("methods", Command::Methods),
            ("state", Command::State),
            ("events", Command::Events),
            ("mcp", Command::Mcp),
            ("help", Command::Help),
            ("--help", Command::Help),
            ("-h", Command::Help),
            ("--version", Command::Version),
        ] {
            let got = parse_strs(&[word]).unwrap();
            assert_eq!(got.command, command, "{word}");
            assert_eq!(got.socket, None);
            assert_eq!(got.timeout, Some(Duration::from_secs(120)));
        }
    }

    #[test]
    fn socket_and_timeout_flags_in_any_position() {
        let got = parse_strs(&["--socket", "/tmp/a.sock", "--timeout", "5", "state"]).unwrap();
        assert_eq!(got.socket.as_deref(), Some("/tmp/a.sock"));
        assert_eq!(got.timeout, Some(Duration::from_secs(5)));
        assert_eq!(got.command, Command::State);

        let got = parse_strs(&["state", "--socket=/tmp/b.sock", "--timeout=0"]).unwrap();
        assert_eq!(got.socket.as_deref(), Some("/tmp/b.sock"));
        assert_eq!(got.timeout, None);

        let got = parse_strs(&["--timeout", "0.25", "methods"]).unwrap();
        assert_eq!(got.timeout, Some(Duration::from_millis(250)));
    }

    #[test]
    fn call_forms() {
        assert_eq!(
            parse_strs(&["call", "transport.play"]).unwrap().command,
            Command::Call {
                method: "transport.play".into(),
                params: ParamsArg::None
            }
        );
        assert_eq!(
            parse_strs(&["call", "transport.locate", "{\"bar\":5}"])
                .unwrap()
                .command,
            Command::Call {
                method: "transport.locate".into(),
                params: ParamsArg::Inline("{\"bar\":5}".into())
            }
        );
        assert_eq!(
            parse_strs(&["call", "transport.locate", "-"])
                .unwrap()
                .command,
            Command::Call {
                method: "transport.locate".into(),
                params: ParamsArg::Stdin
            }
        );
        assert_eq!(
            parse_strs(&["describe", "state.get"]).unwrap().command,
            Command::Describe {
                method: "state.get".into()
            }
        );
    }

    #[test]
    fn usage_errors() {
        for args in [
            &[][..],
            &["frobnicate"],
            &["call"],
            &["call", "a", "{}", "extra"],
            &["call", "-"],
            &["describe"],
            &["describe", "a", "b"],
            &["methods", "extra"],
            &["--socket"],
            &["--socket", "", "state"],
            &["--timeout", "soon", "state"],
            &["--timeout", "-1", "state"],
            &["--bogus", "state"],
            &["-x", "state"],
        ] {
            assert!(
                parse_strs(args).is_err(),
                "{args:?} should be a usage error"
            );
        }
    }

    #[test]
    fn help_wins_over_everything_else() {
        assert_eq!(
            parse_strs(&["call", "--help"]).unwrap().command,
            Command::Help
        );
        assert_eq!(
            parse_strs(&["--version", "--help"]).unwrap().command,
            Command::Help
        );
    }

    #[test]
    fn params_must_be_a_json_object() {
        assert_eq!(
            parse_params("{\"bar\": 5}").unwrap(),
            serde_json::json!({"bar": 5})
        );
        assert_eq!(parse_params(" {} ").unwrap(), serde_json::json!({}));
        for bad in ["[1]", "5", "\"x\"", "null", "{bar: 5}", ""] {
            assert!(parse_params(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn socket_path_resolution() {
        let home = Some(OsStr::new("/Users/someone"));
        let env = Some(OsStr::new("/tmp/env.sock"));
        assert_eq!(
            resolve_socket(Some("/tmp/flag.sock"), env, home).unwrap(),
            PathBuf::from("/tmp/flag.sock")
        );
        assert_eq!(
            resolve_socket(None, env, home).unwrap(),
            PathBuf::from("/tmp/env.sock")
        );
        assert_eq!(
            resolve_socket(None, None, home).unwrap(),
            PathBuf::from("/Users/someone/Library/Application Support/prowave-mac/control.sock")
        );
        assert_eq!(
            resolve_socket(None, Some(OsStr::new("")), home).unwrap(),
            PathBuf::from("/Users/someone/Library/Application Support/prowave-mac/control.sock")
        );
        assert!(resolve_socket(None, None, None).is_err());
        assert!(resolve_socket(None, None, Some(OsStr::new(""))).is_err());
    }
}
