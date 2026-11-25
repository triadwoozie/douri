//! The command line, by running the built binary against the fake app.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::{prowavectl, prowavectl_with_stdin, stderr, stdout, FakeApp};
use serde_json::{json, Value};

#[test]
fn call_prints_the_result_as_indented_json() {
    let app = FakeApp::start();
    let out = prowavectl(&["--socket", app.socket_str(), "call", "transport.play"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out), "{\n  \"playing\": true\n}\n");
    assert_eq!(stderr(&out), "");
}

#[test]
fn call_takes_params_inline_or_from_stdin() {
    let app = FakeApp::start();
    let out = prowavectl(&[
        "call",
        "transport.locate",
        "{\"bar\": 7}",
        "--socket",
        app.socket_str(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let got: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(got["got"], json!({"bar": 7}));

    let out = prowavectl_with_stdin(
        &[
            "--socket",
            app.socket_str(),
            "call",
            "transport.locate",
            "-",
        ],
        "{\"bar\": 8, \"snap\": \"bar\"}\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let got: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(got["got"], json!({"bar": 8, "snap": "bar"}));
}

#[test]
fn an_error_answer_exits_1_with_the_code_and_message() {
    let app = FakeApp::start();
    let out = prowavectl(&["--socket", app.socket_str(), "call", "fail.now"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    let text = stderr(&out);
    let (first, rest) = text.split_once('\n').unwrap();
    assert_eq!(first, "error failed: it did not work");
    // The rest is the whole error object, indented.
    let error: Value = serde_json::from_str(rest).unwrap();
    assert_eq!(error["code"], -32000);
    assert_eq!(error["data"]["code"], "failed");
    assert!(rest.contains("\n  \"code\": -32000"), "{rest}");

    let out = prowavectl(&[
        "--socket",
        app.socket_str(),
        "call",
        "transport.locate",
        "{\"bar\": 0}",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).starts_with("error invalid_params: bar must be 1 or more\n"));
}

#[test]
fn bad_params_are_a_usage_error_and_the_app_is_not_called() {
    let app = FakeApp::start();
    for bad in ["[1]", "{bar: 1}", "5"] {
        let out = prowavectl(&[
            "--socket",
            app.socket_str(),
            "call",
            "transport.locate",
            bad,
        ]);
        assert_eq!(out.status.code(), Some(2), "{bad}");
        assert!(stderr(&out).contains("params"), "{}", stderr(&out));
    }
    let out = prowavectl_with_stdin(
        &[
            "--socket",
            app.socket_str(),
            "call",
            "transport.locate",
            "-",
        ],
        "not json",
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(app.called().is_empty());
}

#[test]
fn state_is_call_state_get() {
    let app = FakeApp::start();
    let out = prowavectl(&["--socket", app.socket_str(), "state"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let got: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(got["project"]["name"], "Demo");
    assert_eq!(app.called(), ["state.get"]);
}

#[test]
fn the_socket_can_come_from_the_environment() {
    let app = FakeApp::start();
    let out = Command::new(env!("CARGO_BIN_EXE_prowavectl"))
        .arg("state")
        .env("PROWAVE_CONTROL_SOCKET", app.socket_str())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn methods_lists_the_apps_methods_in_groups() {
    let app = FakeApp::start();
    let out = prowavectl(&["--socket", app.socket_str(), "methods"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
    let text = stdout(&out);
    assert!(text.starts_with("rpc\n  rpc.describe"), "{text}");
    assert!(text.contains("\ntransport\n  transport.play    Start playback.\n  transport.locate  Move the playhead.\n"), "{text}");
    assert!(
        text.contains("\nfake\n  fake.only         Only the fake app has this.\n"),
        "{text}"
    );
    assert!(text.ends_with("\n7 methods\n"), "{text}");
}

#[test]
fn describe_shows_one_method() {
    let app = FakeApp::start();
    let out = prowavectl(&["--socket", app.socket_str(), "describe", "transport.locate"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.starts_with("transport.locate  (group: transport)\n  Move the playhead.\n"),
        "{text}"
    );
    assert!(
        text.contains("NAME  TYPE    REQUIRED  UNIT  ALLOWED    DESCRIPTION\n"),
        "{text}"
    );
    assert!(
        text.contains("bar   number  yes       bars  1 to 999   Bar to go to.\n"),
        "{text}"
    );
    assert!(text.contains("none, bar  Rounding.\n"), "{text}");
    assert!(
        text.contains("\nResult:\n  The transport state.\n"),
        "{text}"
    );

    let out = prowavectl(&["--socket", app.socket_str(), "describe", "no.such"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).starts_with("error not_found: no method named no.such\n"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn events_prints_one_line_per_event_then_exits_4_when_the_app_goes() {
    let app = FakeApp::start();
    let out = prowavectl(&["--socket", app.socket_str(), "events"]);
    assert_eq!(out.status.code(), Some(4), "{}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    let first: Value = serde_json::from_str(lines[0]).unwrap();
    let second: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(first, json!({"event": "transport.started", "bar": 1}));
    assert_eq!(second, json!({"event": "transport.stopped", "bar": 2.5}));
    assert!(stderr(&out).contains("connection lost"), "{}", stderr(&out));
}

#[test]
fn events_stops_cleanly_on_interrupt() {
    let app = FakeApp::start();
    app.hold_after_events();
    let mut child = Command::new(env!("CARGO_BIN_EXE_prowavectl"))
        .args(["--socket", app.socket_str(), "events"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Wait for both events, so the command is known to be up and waiting.
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    for _ in 0..2 {
        line.clear();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        assert!(line.contains("transport."), "{line}");
    }
    // The same signal Ctrl-C sends.
    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("events did not stop after the interrupt");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(0));
}

#[test]
fn no_socket_file_exits_3_and_says_the_app_is_not_running() {
    let dir = common::short_temp_dir();
    let socket = dir.path().join("none.sock");
    let socket = socket.to_str().unwrap();
    for command in [&["state"][..], &["call", "transport.play"], &["events"]] {
        let mut args = vec!["--socket", socket];
        args.extend_from_slice(command);
        let out = prowavectl(&args);
        assert_eq!(out.status.code(), Some(3), "{command:?}");
        assert_eq!(stdout(&out), "");
        let text = stderr(&out);
        assert!(text.contains("socket file does not exist"), "{text}");
        assert!(text.contains("Dōri is not running"), "{text}");
        assert!(text.contains(socket), "{text}");
    }
}

#[test]
fn a_stale_socket_file_exits_3_and_says_so() {
    let (_dir, socket) = common::stale_socket();
    let out = prowavectl(&["--socket", socket.to_str().unwrap(), "state"]);
    assert_eq!(out.status.code(), Some(3));
    let text = stderr(&out);
    assert!(
        text.contains("socket file exists but nothing answers"),
        "{text}"
    );
    assert!(text.contains("stale"), "{text}");
}

#[test]
fn methods_and_describe_fall_back_to_the_built_in_list() {
    let dir = common::short_temp_dir();
    let socket = dir.path().join("none.sock");
    let socket = socket.to_str().unwrap();

    let out = prowavectl(&["--socket", socket, "methods"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.trim_end().ends_with("methods") || text.trim_end().ends_with("1 method"),
        "{text}"
    );
    assert!(!text.contains("fake.only"));
    let note = stderr(&out);
    assert!(note.contains("Dōri is not running"), "{note}");
    assert!(note.contains("built into prowavectl"), "{note}");

    // Describe the first method the built-in list names, whatever it is.
    let name = text
        .lines()
        .find_map(|line| line.strip_prefix("  "))
        .and_then(|line| line.split_whitespace().next())
        .expect("the built-in list has a method");
    let out = prowavectl(&["--socket", socket, "describe", name]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stdout(&out).starts_with(name), "{}", stdout(&out));
    assert!(stdout(&out).contains("\nParams:\n"));
    assert!(stderr(&out).contains("built into prowavectl"));

    let out = prowavectl(&["--socket", socket, "describe", "no.such.method"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("there is no method named no.such.method"));
}

#[test]
fn no_answer_in_time_exits_4() {
    let app = FakeApp::start();
    let started = Instant::now();
    let out = prowavectl(&[
        "--socket",
        app.socket_str(),
        "--timeout",
        "0.3",
        "call",
        "slow.never",
    ]);
    assert_eq!(out.status.code(), Some(4), "{}", stderr(&out));
    assert!(stderr(&out).contains("timeout"), "{}", stderr(&out));
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn help_version_and_usage() {
    for args in [&["help"][..], &["--help"], &["call", "-h"]] {
        let out = prowavectl(args);
        assert_eq!(out.status.code(), Some(0));
        assert!(stdout(&out).starts_with("prowavectl: command-line client"));
        assert!(stdout(&out).contains("Exit codes:"));
    }
    let out = prowavectl(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        format!("prowavectl {}\n", env!("CARGO_PKG_VERSION"))
    );

    for args in [
        &[][..],
        &["frobnicate"],
        &["call"],
        &["--timeout", "soon", "state"],
    ] {
        let out = prowavectl(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(stdout(&out), "");
        assert!(stderr(&out).starts_with("prowavectl: "), "{}", stderr(&out));
    }
}
