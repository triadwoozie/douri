//! The client against the fake app, in this process.

mod common;

use std::time::{Duration, Instant};

use common::FakeApp;
use prowavectl::client::{Client, ConnectError, RpcFailure};
use serde_json::json;

const LIMIT: Option<Duration> = Some(Duration::from_secs(20));

#[test]
fn a_call_returns_the_result() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    assert_eq!(
        client.call("transport.play", None, LIMIT).unwrap(),
        json!({"playing": true})
    );
    let got = client
        .call("transport.locate", Some(json!({"bar": 5})), LIMIT)
        .unwrap();
    assert_eq!(got["bar"], 5.0);
    assert_eq!(got["got"], json!({"bar": 5}));
    // No time limit at all also works.
    assert_eq!(
        client.call("rpc.version", None, None).unwrap()["version"],
        "9.9.9"
    );
    assert_eq!(
        app.called(),
        ["transport.play", "transport.locate", "rpc.version"]
    );
}

#[test]
fn an_error_answer_keeps_its_code_and_message() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    match client.call("transport.locate", Some(json!({"bar": 0})), LIMIT) {
        Err(failure @ RpcFailure::App(_)) => {
            assert_eq!(failure.exit_code(), 1);
            assert_eq!(
                failure.to_string(),
                "error invalid_params: bar must be 1 or more"
            );
            let RpcFailure::App(error) = failure else {
                unreachable!()
            };
            assert_eq!(error.number, -32602);
            assert_eq!(error.code, "invalid_params");
            assert_eq!(error.error["data"]["detail"], "from the fake");
        }
        other => panic!("expected an app error, got {other:?}"),
    }
    // The connection is still good after an error.
    assert!(client.call("transport.play", None, LIMIT).is_ok());
    match client.call("no.such", None, LIMIT) {
        Err(RpcFailure::App(error)) => assert_eq!(error.code, "method_not_found"),
        other => panic!("expected an app error, got {other:?}"),
    }
}

#[test]
fn an_event_before_the_answer_is_set_aside() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    assert_eq!(
        client.call("noisy.call", None, LIMIT).unwrap(),
        json!({"noisy": true})
    );
    assert_eq!(
        client.next_event().unwrap(),
        json!({"event": "transport.started", "bar": 3})
    );
}

#[test]
fn events_then_the_app_goes_away() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    assert_eq!(
        client.call("events.subscribe", None, LIMIT).unwrap(),
        json!({"subscribed": true})
    );
    assert_eq!(client.next_event().unwrap()["event"], "transport.started");
    assert_eq!(client.next_event().unwrap()["event"], "transport.stopped");
    match client.next_event() {
        Err(failure @ RpcFailure::ConnectionLost(_)) => assert_eq!(failure.exit_code(), 4),
        other => panic!("expected a lost connection, got {other:?}"),
    }
}

#[test]
fn a_line_of_several_megabytes_arrives_whole() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    let got = client.call("big.blob", None, LIMIT).unwrap();
    assert_eq!(got["blob"].as_str().unwrap().len(), 3 * 1024 * 1024);
}

#[test]
fn no_answer_is_a_timeout() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    let started = Instant::now();
    match client.call("slow.never", None, Some(Duration::from_millis(300))) {
        Err(failure @ RpcFailure::Timeout { .. }) => {
            assert_eq!(failure.exit_code(), 4);
            assert!(failure.to_string().contains("slow.never"));
        }
        other => panic!("expected a timeout, got {other:?}"),
    }
    let took = started.elapsed();
    assert!(took >= Duration::from_millis(300), "{took:?}");
    assert!(took < Duration::from_secs(10), "{took:?}");
}

#[test]
fn a_missing_socket_means_not_running() {
    let dir = common::short_temp_dir();
    let socket = dir.path().join("none.sock");
    match Client::connect(&socket) {
        Err(error @ ConnectError::NotRunning(_)) => {
            let text = error.to_string();
            assert!(text.contains("socket file does not exist"), "{text}");
            assert!(text.contains("Pro Wave is not running"), "{text}");
            assert!(text.contains("PROWAVE_NO_CONTROL=1"), "{text}");
            assert_eq!(RpcFailure::from(error).exit_code(), 3);
        }
        Err(other) => panic!("expected not running, got {other:?}"),
        Ok(_) => panic!("connected to nothing"),
    }
}

#[test]
fn a_socket_file_nobody_listens_on_is_stale() {
    let (_dir, socket) = common::stale_socket();
    match Client::connect(&socket) {
        Err(error @ ConnectError::Stale(_)) => {
            let text = error.to_string();
            assert!(text.contains("exists but nothing answers"), "{text}");
            assert!(text.contains("stale"), "{text}");
            assert_eq!(RpcFailure::from(error).exit_code(), 3);
        }
        Err(other) => panic!("expected stale, got {other:?}"),
        Ok(_) => panic!("connected to nothing"),
    }
}

#[test]
fn a_kept_connection_notices_that_the_app_quit() {
    let app = FakeApp::start();
    let mut client = Client::connect(&app.socket).unwrap();
    assert!(client.call("transport.play", None, LIMIT).is_ok());
    assert!(client.is_alive());
    // Still usable after the check.
    assert!(client.call("transport.play", None, LIMIT).is_ok());
    app.stop();
    // The close may take a moment to arrive.
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.is_alive() {
        assert!(
            Instant::now() < deadline,
            "the closed connection still looks alive"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
