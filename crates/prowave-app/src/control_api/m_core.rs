//! Methods about the interface itself (`rpc.*`), the whole state, the project file, the
//! transport, and `batch`.
use super::params::{place, place_json, place_params, req_place, round3, P};
use super::protocol::{Code, RpcError, PROTOCOL};
use super::registry::{self, m, p, Method, Reply, Ty};
use super::{ok, state, Cx};
use crate::model::{Project, MAX_BPM, MAX_COUNT_IN_BARS, MIN_BPM};
use serde_json::{json, Value};

// ------------------------------------------------------------------ rpc

fn rpc_describe(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let only = params.str("method");
    if let Some(name) = only {
        if registry::find(name).is_none() {
            let e = registry::unknown(name);
            return Err(RpcError::not_found(e.message));
        }
    }
    ok(registry::describe(only, Some(&cx.core.socket)))
}

fn rpc_version(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ok(json!({
        "app": crate::APP_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("PROWAVE_COMMIT").unwrap_or(""),
        "protocol": PROTOCOL,
        "methods": registry::all().len(),
        "socket": cx.core.socket.display().to_string(),
        "pid": std::process::id(),
        "headless": cx.core.headless,
        "uptime_seconds": round3(cx.core.started.elapsed().as_secs_f64()),
    }))
}

fn rpc_ping(_: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ok(json!({"pong": true}))
}

fn rpc_stats(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    let visible = cx.ctx.input(|i| i.viewport().visible()).unwrap_or(true);
    ok(json!({
        "handling": cx.core.times.to_json(),
        "clients": cx.core.shared.clients(),
        "subscribers": cx.core.shared.subscribers(),
        "window_visible": visible,
        "note": "handling is the time from a request's line being read to its answer being queued, in the app; it includes the wait for the interface thread's next turn",
    }))
}

fn rpc_wait(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let frames = params.i64("frames").unwrap_or(2).clamp(1, 600) as u32;
    let limit = params
        .f64("timeout_ms")
        .unwrap_or(5000.0)
        .clamp(1.0, 60_000.0);
    cx.core.waits.push(super::Wait {
        conn: cx.conn,
        id: cx.id.clone(),
        // The turn that takes this request is not a frame gone by.
        frames: frames + 1,
        asked: std::time::Instant::now(),
        limit: std::time::Duration::from_secs_f64(limit / 1000.0),
    });
    cx.ctx.request_repaint();
    Ok(Reply::Later)
}

// ------------------------------------------------------------------ state

fn state_get(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let regions = params.bool("regions").unwrap_or(true);
    let notes = params.bool("notes").unwrap_or(true);
    cx.core.ids.reconcile(&cx.app.project);
    ok(state::project_json(cx.app, &cx.core.ids, regions, notes))
}

// ------------------------------------------------------------------ the project file

const IF_UNSAVED: &[&str] = &["error", "save", "discard"];

fn if_unsaved_param() -> registry::Param {
    p("if_unsaved", Ty::Str, "what to do when the open project has unsaved changes: error (the default: refuse), save (save it to its file first) or discard (lose them)").vals(IF_UNSAVED)
}

/// Before the open project is replaced: its unsaved changes are saved or given up as the call
/// says, or the call is refused.
fn settle_unsaved(cx: &mut Cx, params: &P) -> Result<(), RpcError> {
    if !cx.app.has_unsaved_changes() {
        return Ok(());
    }
    match params.str("if_unsaved").unwrap_or("error") {
        "discard" => Ok(()),
        "save" => {
            let Some(path) = cx.app.path.clone() else {
                return Err(RpcError::new(
                    Code::UnsavedChanges,
                    format!(
                        "the open project \"{}\" has unsaved changes and has never been saved, so there is no file to save it to: call project.save_as first, or pass if_unsaved: \"discard\"",
                        cx.app.project.name
                    ),
                ));
            };
            if cx.app.save_to(&path) {
                Ok(())
            } else {
                Err(RpcError::failed(cx.app.ui.status.clone().unwrap_or_else(
                    || format!("could not save {}", path.display()),
                )))
            }
        }
        _ => Err(RpcError::new(
            Code::UnsavedChanges,
            format!(
                "the open project \"{}\" has unsaved changes: pass if_unsaved: \"save\" or \"discard\"",
                cx.app.project.name
            ),
        )),
    }
}

fn project_get(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ok(state::file_json(cx.app))
}

fn project_new(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    settle_unsaved(cx, params)?;
    cx.app.replace(Project::empty(), None);
    cx.ctx.request_repaint();
    ok(state::file_json(cx.app))
}

fn project_open(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let path = params.req_path("path")?;
    if !path.is_file() {
        return Err(RpcError::not_found(format!(
            "there is no file {}",
            path.display()
        )));
    }
    settle_unsaved(cx, params)?;
    let before = cx.app.project.song_rev;
    cx.app.ui.status = None;
    cx.app.open(&path, cx.ctx);
    if cx.app.path.as_deref() != Some(path.as_path()) || cx.app.project.song_rev == before {
        return Err(RpcError::failed(
            cx.app
                .ui
                .status
                .clone()
                .unwrap_or_else(|| format!("could not open {}", path.display())),
        ));
    }
    cx.ctx.request_repaint();
    let mut out = state::file_json(cx.app);
    if let Some(o) = out.as_object_mut() {
        o.insert("missing_media".into(), json!(cx.app.ui.missing_audio));
        o.insert(
            "dialog".into(),
            json!(super::m_ui::open_dialog(cx.app).map(|d| d.to_json())),
        );
    }
    ok(out)
}

fn save_error(cx: &Cx, path: &std::path::Path) -> RpcError {
    RpcError::failed(
        cx.app
            .ui
            .status
            .clone()
            .unwrap_or_else(|| format!("could not save {}", path.display())),
    )
}

fn project_save(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    let Some(path) = cx.app.path.clone() else {
        return Err(RpcError::new(
            Code::Failed,
            "the project has never been saved, so it has no file: use project.save_as with a path",
        ));
    };
    if !cx.app.save_to(&path) {
        return Err(save_error(cx, &path));
    }
    ok(state::file_json(cx.app))
}

fn project_save_as(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let path = params.req_path("path")?;
    if path.exists()
        && params.bool("overwrite") != Some(true)
        && cx.app.path.as_deref() != Some(path.as_path())
    {
        return Err(RpcError::failed(format!(
            "{} is already there: pass overwrite: true to replace it",
            path.display()
        )));
    }
    if !cx.app.save_to(&path) {
        return Err(save_error(cx, &path));
    }
    ok(state::file_json(cx.app))
}

fn project_save_copy(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let path = params.req_path("path")?;
    if path.exists() && params.bool("overwrite") != Some(true) {
        return Err(RpcError::failed(format!(
            "{} is already there: pass overwrite: true to replace it",
            path.display()
        )));
    }
    if !cx.app.save_copy_to(&path) {
        return Err(save_error(cx, &path));
    }
    ok(json!({"copy": path.display().to_string(), "project": state::file_json(cx.app)}))
}

fn project_revert(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let Some(path) = cx.app.path.clone() else {
        return Err(RpcError::failed(
            "nothing to revert to: the project has not been saved yet",
        ));
    };
    if params.bool("discard_changes") != Some(true) {
        return Err(RpcError::invalid(
            "discard_changes",
            "reverting throws away every change since the last save: pass discard_changes: true to do it",
        ));
    }
    cx.app.open(&path, cx.ctx);
    ok(state::file_json(cx.app))
}

fn undo_redo(cx: &mut Cx, redo: bool) -> Result<Reply, RpcError> {
    let app = &mut *cx.app;
    if app.ui.rec.active() || app.ui.audio_rec.active() {
        return Err(RpcError::new(
            Code::Busy,
            "a recording is running: stop it first (transport.stop)",
        ));
    }
    let done = if redo {
        app.history
            .redo_followed(&mut app.project, &mut app.ui.flex)
    } else {
        app.history
            .undo_followed(&mut app.project, &mut app.ui.flex)
    };
    if done {
        // As `App::track_history` does after a step: track and region numbers may point
        // elsewhere now, and what the step changed was not moved by hand.
        app.ui.tracks.clear_selection();
        app.ui.piano.selected.clear();
        app.ui.auto.forget();
        cx.ctx.request_repaint();
    }
    app.ui.can_undo = app.history.can_undo();
    app.ui.can_redo = app.history.can_redo();
    ok(json!({
        "done": done,
        "can_undo": app.ui.can_undo,
        "can_redo": app.ui.can_redo,
    }))
}

fn project_undo(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    undo_redo(cx, false)
}

fn project_redo(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    undo_redo(cx, true)
}

// ------------------------------------------------------------------ transport

fn transport_get(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ok(state::transport_json(&cx.app.project))
}

fn transport_play(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    if let Some(at) = place(pr, params, "")? {
        pr.position_beats = at;
    }
    pr.playing = true;
    cx.ctx.request_repaint();
    ok(state::transport_json(&cx.app.project))
}

fn transport_stop(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    // As the Stop key: playing ends, and with it a recording (the recorder closes its region
    // on the next frame).
    pr.playing = false;
    pr.recording = false;
    if let Some(at) = place(pr, params, "")? {
        pr.position_beats = at;
    }
    cx.ctx.request_repaint();
    ok(state::transport_json(&cx.app.project))
}

fn transport_record(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let on = params.bool("on").unwrap_or(true);
    let pr = &mut cx.app.project;
    if on {
        if let Some(at) = place(pr, params, "")? {
            pr.position_beats = at;
        }
    }
    // The Record key: the recorders take it from here on the next frame (the armed audio
    // tracks, or the selected track; the count-in when it is on).
    pr.recording = on;
    cx.ctx.request_repaint();
    let armed: Vec<String> = (0..cx.app.project.tracks.len())
        .filter(|i| cx.app.project.tracks[*i].rec)
        .map(|i| cx.core.ids.track_id(i))
        .collect();
    let mut out = state::transport_json(&cx.app.project);
    if let Some(o) = out.as_object_mut() {
        o.insert("armed_tracks".into(), json!(armed));
        o.insert(
            "records_on".into(),
            json!(cx.core.ids.track_id(cx.app.project.selected_track)),
        );
    }
    ok(out)
}

fn transport_locate(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    pr.position_beats = req_place(pr, params, "")?;
    cx.ctx.request_repaint();
    ok(state::transport_json(&cx.app.project))
}

fn transport_cycle(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    let start = place(pr, params, "")?.unwrap_or(pr.cycle_start);
    let end = place(pr, params, "end_")?.unwrap_or(pr.cycle_end);
    if end <= start {
        return Err(RpcError::invalid(
            "end_bar",
            format!(
                "the cycle must end after it starts: it would run from bar {} to bar {}",
                round3(pr.beats_to_bar(start) + 1.0),
                round3(pr.beats_to_bar(end) + 1.0)
            ),
        ));
    }
    pr.cycle_start = start;
    pr.cycle_end = end;
    if let Some(on) = params.bool("on") {
        pr.cycle = on;
    }
    cx.ctx.request_repaint();
    ok(state::transport_json(&cx.app.project))
}

fn transport_tempo(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    let at = place(pr, params, "")?.unwrap_or(pr.position_beats);
    if let Some(bpm) = params.f32("bpm") {
        pr.set_tempo_at(at, bpm);
    }
    ok(json!({
        "bpm": round3(f64::from(pr.tempo_at(at))),
        "at": place_json(pr, at),
        "tempo_changes": state::tempo_json(pr),
    }))
}

fn transport_signature(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    let bar = params.i64("from_bar").unwrap_or(1).max(1) as u32;
    if let (Some(n), Some(d)) = (params.i64("numerator"), params.i64("denominator")) {
        if !crate::model::SIGNATURE_DENOMINATORS.contains(&(d as u8)) {
            return Err(RpcError::invalid(
                "denominator",
                format!("the denominator must be one of 1, 2, 4, 8, 16, 32, 64; got {d}"),
            ));
        }
        pr.set_signature_from(bar, (n as u8, d as u8));
    } else if params.has("numerator") || params.has("denominator") {
        return Err(RpcError::invalid(
            "numerator",
            "give both numerator and denominator",
        ));
    }
    ok(json!({"signatures": state::signatures_json(pr)}))
}

fn transport_metronome(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    if let Some(on) = params.bool("on") {
        pr.metronome = on;
    }
    if let Some(db) = params.f32("volume_db") {
        pr.click_volume_db = db.clamp(crate::model::MIN_CLICK_DB, 0.0);
    }
    if let Some(on) = params.bool("accent") {
        pr.click_accent = on;
    }
    ok(json!({
        "on": pr.metronome,
        "volume_db": round3(f64::from(pr.click_volume_db)),
        "accent": pr.click_accent,
    }))
}

fn transport_count_in(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let pr = &mut cx.app.project;
    if let Some(on) = params.bool("on") {
        pr.count_in = on;
    }
    if let Some(bars) = params.i64("bars") {
        pr.count_in_bars = bars.clamp(1, i64::from(MAX_COUNT_IN_BARS)) as u8;
    }
    ok(json!({"on": pr.count_in, "bars": pr.count_in_bars}))
}

// ------------------------------------------------------------------ batch

fn batch(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let Some(calls) = params.arr("calls") else {
        return Err(RpcError::invalid(
            "calls",
            "`calls` is needed: a list of {method, params}",
        ));
    };
    if cx.core.in_batch {
        return Err(RpcError::invalid(
            "calls",
            "a batch cannot hold another batch",
        ));
    }
    let rollback = params.bool("rollback") == Some(true);
    let before = rollback.then(|| cx.app.project.clone());
    let mut results = Vec::with_capacity(calls.len());
    let mut failed: Option<(usize, String, RpcError)> = None;
    cx.core.in_batch = true;
    for (i, call) in calls.iter().enumerate() {
        let method = call.get("method").and_then(Value::as_str);
        let empty = serde_json::Map::new();
        let call_params = match call.get("params") {
            None | Some(Value::Null) => Some(&empty),
            Some(Value::Object(o)) => Some(o),
            Some(_) => None,
        };
        let (Some(method), Some(call_params)) = (method, call_params) else {
            failed = Some((
                i,
                method.unwrap_or("").to_string(),
                RpcError::invalid(
                    "calls",
                    format!(
                        "call {} must be an object {{\"method\": \"...\", \"params\": {{...}}}}",
                        i + 1
                    ),
                ),
            ));
            break;
        };
        let refused = matches!(
            method,
            "batch"
                | "events.subscribe"
                | "events.unsubscribe"
                | "ui.screenshot"
                | "app.quit"
                | "rpc.wait"
        );
        let out = if refused {
            Err(RpcError::new(
                Code::NotSupported,
                format!("`{method}` cannot be part of a batch: call it on its own"),
            ))
        } else {
            super::dispatch(cx.app, cx.ctx, cx.core, cx.conn, None, method, call_params)
        };
        match out {
            Ok(Reply::Now(v)) => results.push(v),
            Ok(Reply::Later) => results.push(Value::Null),
            Err(e) => {
                failed = Some((i, method.to_string(), e));
                break;
            }
        }
    }
    cx.core.in_batch = false;
    let Some((index, method, error)) = failed else {
        return ok(json!({"results": results, "count": results.len()}));
    };
    let mut rolled_back = false;
    if let Some(mut old) = before {
        // The project goes back to how it was (a new song revision has the engine follow).
        // What a call did outside the project (a file saved, a window opened) stays done.
        old.song_rev = cx.app.project.song_rev.wrapping_add(1);
        old.playing = cx.app.project.playing;
        old.position_beats = cx.app.project.position_beats;
        cx.app.project = old;
        rolled_back = true;
    }
    Err(RpcError {
        code: error.code,
        message: format!(
            "call {} of {} (`{method}`) failed: {}",
            index + 1,
            calls.len(),
            error.message
        ),
        data: error.data.clone(),
    }
    .with("failed_index", index)
    .with("failed_method", method)
    .with("results", Value::Array(results))
    .with("rolled_back", rolled_back))
}

pub fn methods() -> Vec<Method> {
    let [bar, position, seconds, beat] = place_params("");
    let [end_bar, end_position, end_seconds, end_beat] = place_params("end_");
    vec![
        m("rpc.describe", "Every method with its parameters and result, and the error codes.",
            vec![p("method", Ty::Str, "only this method")],
            "{protocol, app, version, commit, socket, methods: [{name, group, summary, changes_project, params: [{name, type, required, description, unit?, values?, min?, max?}], result}], errors: [{name, number, meaning}]}",
            rpc_describe),
        m("rpc.version", "The app's version and commit, and the protocol's version.",
            vec![], "{app, version, commit, protocol, methods, socket, pid, headless, uptime_seconds}", rpc_version),
        m("rpc.ping", "Answers at once: a check that the app is there and its interface thread is running.",
            vec![], "{pong: true}", rpc_ping),
        m("rpc.stats", "How long the app took over the last requests, and who is connected.",
            vec![], "{handling: {requests, measured, mean_ms, median_ms, p95_ms, max_ms}, clients, subscribers, window_visible}", rpc_stats),
        m("rpc.wait", "Answers after the app has drawn some frames: the time a change needs to reach the engine, or a plug-in to open.",
            vec![
                p("frames", Ty::Int, "frames to wait for (default 2)").range(1.0, 600.0),
                p("timeout_ms", Ty::Num, "answer after this long at the latest (default 5000)").unit("milliseconds").range(1.0, 60000.0),
            ],
            "{waited_ms}", rpc_wait),
        m("state.get", "The whole project and the state of the app in one object.",
            vec![
                p("regions", Ty::Bool, "include each track's regions (default true)"),
                p("notes", Ty::Bool, "include each MIDI region's notes (default true; false keeps a big project's answer small)"),
            ],
            "{project: {name, path, dirty, can_undo, can_redo}, transport, key, tuning_hz, tempo_changes, signatures, markers, tracks: [track.get], stereo_out, selection, busy, status, dialog}",
            state_get),
        m("project.get", "The project's name, file and whether it has unsaved changes.",
            vec![], "{name, path, dirty, can_undo, can_redo}", project_get),
        m("project.new", "Replaces the open project with a new, empty one (File > New).",
            vec![if_unsaved_param()], "the new project as project.get gives it", project_new).changes(),
        m("project.close", "Closes the project, which leaves a new, empty one (File > Close Project).",
            vec![if_unsaved_param()], "the empty project as project.get gives it", project_new).changes(),
        m("project.open", "Opens a .prowave file in place of the open project.",
            vec![p("path", Ty::Str, "the .prowave file: an absolute path, or one starting with ~/").req(), if_unsaved_param()],
            "project.get's fields, with missing_media (audio files that could not be read) and the dialog that lists them when there are any",
            project_open).changes(),
        m("project.save", "Saves the project to its file (File > Save).",
            vec![], "project.get's fields", project_save),
        m("project.save_as", "Saves the project to a new file, which becomes its file (File > Save As).",
            vec![
                p("path", Ty::Str, "the file to write: an absolute path, or one starting with ~/ (.prowave is added when missing)").req(),
                p("overwrite", Ty::Bool, "replace a file that is already there"),
            ],
            "project.get's fields", project_save_as),
        m("project.save_copy", "Writes a copy of the project to another file; the project keeps its own file.",
            vec![
                p("path", Ty::Str, "the file to write").req(),
                p("overwrite", Ty::Bool, "replace a file that is already there"),
            ],
            "{copy, project}", project_save_copy),
        m("project.revert", "Reloads the project from its file, losing every change since the last save.",
            vec![p("discard_changes", Ty::Bool, "must be true: the changes are lost").req()],
            "project.get's fields", project_revert).changes(),
        m("project.undo", "Undoes the last step (Edit > Undo).",
            vec![], "{done, can_undo, can_redo}; done is false when there was nothing to undo", project_undo),
        m("project.redo", "Does the last undone step again (Edit > Redo).",
            vec![], "{done, can_undo, can_redo}", project_redo),
        m("transport.get", "Whether the song plays or records, where the playhead is, the cycle, the tempo and the click.",
            vec![], "{playing, recording, counting_in, playhead: {bar, position, beat, seconds}, tempo, signature, cycle: {on, start, end}, metronome, count_in}", transport_get),
        m("transport.play", "Starts playing, from the playhead or from a place.",
            vec![bar, position, seconds, beat], "the transport as transport.get gives it", transport_play),
        m("transport.stop", "Stops playing and recording.",
            vec![bar, position, seconds, beat], "the transport; a place moves the playhead there after stopping", transport_stop),
        m("transport.record", "Starts recording, as the Record key does: audio on the armed audio tracks, else MIDI or audio on the selected track; with the count-in when it is on.",
            vec![p("on", Ty::Bool, "false stops the recording and leaves the song playing (default true)"), bar, position, seconds, beat],
            "the transport, with armed_tracks and records_on (the selected track)", transport_record),
        m("transport.locate", "Moves the playhead.",
            vec![bar, position, seconds, beat], "the transport", transport_locate),
        m("transport.cycle", "Turns the cycle (loop) on or off and sets its range.",
            vec![p("on", Ty::Bool, "cycle on or off"), bar, position, seconds, beat, end_bar, end_position, end_seconds, end_beat],
            "the transport; the range runs from the start place to the end place (bar 1 to end_bar 9 loops bars 1 to 8)", transport_cycle).changes(),
        m("transport.tempo", "Reads or sets the tempo in effect at the playhead (or at a place), as dragging the tempo in the control bar does.",
            vec![p("bpm", Ty::Num, "the new tempo").unit("bpm").range(f64::from(MIN_BPM), f64::from(MAX_BPM)), bar, position, seconds, beat],
            "{bpm, at, tempo_changes}", transport_tempo).changes(),
        m("transport.signature", "Reads the time signatures, or sets the signature from a bar on.",
            vec![
                p("numerator", Ty::Int, "beats in a bar").range(1.0, 99.0),
                p("denominator", Ty::Int, "the note that is one beat: 1, 2, 4, 8, 16, 32 or 64").range(1.0, 64.0),
                p("from_bar", Ty::Int, "the first bar with this signature (default 1: the song's own)").range(1.0, 9999.0),
            ],
            "{signatures: [{bar, numerator, denominator}]}", transport_signature).changes(),
        m("transport.metronome", "Reads or sets the click.",
            vec![
                p("on", Ty::Bool, "click on or off"),
                p("volume_db", Ty::Num, "its level").unit("dB").range(-60.0, 0.0),
                p("accent", Ty::Bool, "the first beat of each bar clicks higher"),
            ],
            "{on, volume_db, accent}", transport_metronome).changes(),
        m("transport.count_in", "Reads or sets the count-in before recording.",
            vec![p("on", Ty::Bool, "count-in on or off"), p("bars", Ty::Int, "how many bars").range(1.0, 4.0)],
            "{on, bars}", transport_count_in).changes(),
        m("batch", "Runs several calls in order as one Undo step. Stops at the first that fails.",
            vec![
                p("calls", Ty::Arr, "the calls: [{\"method\": \"...\", \"params\": {...}}, ...]").req(),
                p("rollback", Ty::Bool, "when a call fails, put the project back as it was before the batch (default false: what the earlier calls did stays)"),
            ],
            "{results: [each call's result], count}; on a failure the error has failed_index (0-based), failed_method, results so far and rolled_back",
            batch).changes(),
    ]
}
