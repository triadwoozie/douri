//! `export.bounce` and `render.range`: the project, or a stretch of it, played into an audio
//! file without the speakers.
//!
//! Both run the app's bounce (`bounce.rs`, File > Bounce > Project or Section): a second
//! engine with no sound card is handed a copy of the project by the code that hands the
//! running engine the project, and its blocks are written to the file. So everything the app
//! plays is in the file: audio regions, instruments, Aux strips and sends, the built-in and
//! plug-in effects, automation, the Stereo Out. `export.bounce` is the whole of it;
//! `render.range` is the older, shorter call kept for measuring a stretch (a WAV file of
//! 32-bit floats, a fixed tail).
//!
//! A bounce runs over many frames and on its own thread, so the app goes on answering other
//! requests while it runs. The call is answered when the bounce ends (`Reply::Later`); with
//! `wait: false` it is answered at once and the end comes as the event `bounce.finished`.
use super::params::{place, place_json, round3, P};
use super::protocol::{Code, RpcError};
use super::registry::{m, p, Method, Reply, Ty};
use super::{ok, state, Core, Cx};
use crate::bounce::{self, Depth, Format, Report, Settings};
use crate::App;
use serde_json::{json, Map, Value};

/// Which call a bounce answers, for the shape of its result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Bounce,
    Range,
}

/// A call waiting for its bounce to end.
pub struct Pending {
    pub conn: u64,
    pub id: Option<Value>,
    /// The bounce's number (`BounceUi::serial`).
    pub serial: u64,
    pub kind: Kind,
}

/// What the events last said of the running bounce: its number, the percent, preparing.
#[derive(Default, PartialEq, Clone, Copy)]
pub struct Seen {
    serial: u64,
    percent: i32,
    preparing: bool,
    finished: u64,
}

fn db_json(x: Option<f64>) -> Value {
    x.map_or(Value::Null, |d| json!(round3(d)))
}

/// A bounce's result as the calls give it.
pub fn report_json(app: &App, core: &Core, serial: u64, r: &Report, kind: Kind) -> Value {
    let p = &app.project;
    let files: Vec<Value> = r
        .files
        .iter()
        .map(|f| {
            json!({
                "path": f.path.display().to_string(),
                "track": f.track_index.map(|i| core.ids.track_id(i)),
                "name": f.track,
                "seconds": round3(f.seconds),
                "frames": f.frames,
                "peak_db": db_json(f.peak_db()),
                "rms_db": db_json(f.rms_db()),
                "silent": f.silent(),
                "tail_seconds": round3(f.tail_seconds),
                "gain_db": db_json(bounce::db(f64::from(f.gain))),
                "clipped_samples": f.clipped,
                "speed": round3(f.speed),
            })
        })
        .collect();
    let rendered: Vec<Value> = r
        .rendered
        .iter()
        .map(|(i, name, what)| json!({"track": core.ids.track_id(*i), "name": name, "instrument": what}))
        .collect();
    let first = r.files.first();
    let mut out = json!({
        "bounce": serial,
        "path": first.map(|f| f.path.display().to_string()),
        "files": files,
        "seconds": first.map_or(0.0, |f| round3(f.seconds)),
        "sample_rate": r.sample_rate,
        "channels": 2,
        "format": r.format,
        "from": place_json(p, r.from),
        "to": place_json(p, r.to),
        "range_seconds": round3(r.range_seconds),
        "tail_seconds": first.map_or(0.0, |f| round3(f.tail_seconds)),
        "peak_db": db_json(first.and_then(|f| f.peak_db())),
        "rms_db": db_json(first.and_then(|f| f.rms_db())),
        "silent": first.is_none_or(|f| f.silent()),
        "rendered_tracks": rendered,
        "muted_tracks": r.muted,
        "not_rendered": r.not_rendered,
        "complete": r.not_rendered.is_empty(),
        "speed": round3(r.speed()),
        "took_seconds": round3(r.took_seconds),
    });
    if kind == Kind::Range {
        if let Some(o) = out.as_object_mut() {
            o.remove("bounce");
        }
    }
    out
}

fn error_of(text: &str) -> RpcError {
    if text == "cancelled" {
        RpcError::new(Code::Failed, "the bounce was cancelled")
    } else {
        RpcError::failed(text)
    }
}

/// The answer of the call `pending`, once its bounce is over.
pub fn answer(app: &App, core: &Core, pending: &Pending) -> Option<Result<Value, RpcError>> {
    let b = &app.ui.bounce;
    if b.job.is_some() && b.serial == pending.serial {
        return None;
    }
    Some(match &b.last {
        Some((serial, Ok(r))) if *serial == pending.serial => {
            Ok(report_json(app, core, *serial, r, pending.kind))
        }
        Some((serial, Err(e))) if *serial == pending.serial => Err(error_of(e)),
        _ => Err(RpcError::failed(
            "the bounce ended and its result is gone (another one ran after it)",
        )),
    })
}

fn progress_json(serial: u64, pr: &bounce::Progress) -> Map<String, Value> {
    let mut o = Map::new();
    o.insert("bounce".into(), json!(serial));
    o.insert("fraction".into(), json!(round3(f64::from(pr.fraction))));
    o.insert("file".into(), json!(pr.file));
    o.insert("files".into(), json!(pr.files));
    o.insert("track".into(), json!(pr.track));
    o.insert("preparing".into(), json!(pr.preparing));
    o
}

/// Once a frame: answers the calls whose bounce is over, and tells the subscribed clients how
/// far the running one is and when it ends. Returns whether a bounce is running (the frames
/// must go on then, also while the window is hidden).
pub fn tend(app: &App, core: &mut Core) -> bool {
    let b = &app.ui.bounce;
    let mut seen = core.bounce_seen;
    if let Some(pr) = b.progress() {
        let percent = (pr.fraction * 100.0).floor() as i32;
        if (seen.serial, seen.percent, seen.preparing) != (b.serial, percent, pr.preparing) {
            seen.serial = b.serial;
            seen.percent = percent;
            seen.preparing = pr.preparing;
            core.shared
                .emit("bounce.progress", progress_json(b.serial, &pr));
        }
    } else if let Some((serial, result)) = &b.last {
        if seen.finished != *serial {
            seen.finished = *serial;
            let mut o = Map::new();
            o.insert("bounce".into(), json!(serial));
            o.insert("ok".into(), json!(result.is_ok()));
            match result {
                Ok(r) => {
                    o.insert(
                        "result".into(),
                        report_json(app, core, *serial, r, Kind::Bounce),
                    );
                }
                Err(e) => {
                    o.insert("error".into(), json!(error_of(e).message));
                }
            }
            core.shared.emit("bounce.finished", o);
        }
    }
    core.bounce_seen = seen;
    let waiting = std::mem::take(&mut core.bounces);
    for pending in waiting {
        match answer(app, core, &pending) {
            None => core.bounces.push(pending),
            Some(result) => {
                if let Some(id) = &pending.id {
                    core.shared.respond(pending.conn, id, &result);
                }
            }
        }
    }
    b.job.is_some()
}

/// The range the call names (`None`: none was given, so the bounce's own).
fn range_of(cx: &Cx, params: &P) -> Result<Option<(f64, f64)>, RpcError> {
    let pr = &cx.app.project;
    let from = place(pr, params, "")?;
    let to = place(pr, params, "end_")?;
    if from.is_none() && to.is_none() {
        return Ok(None);
    }
    let whole = bounce::whole_range(pr);
    let from = from.unwrap_or(0.0);
    let Some(to) = to.or(whole.map(|w| w.1)) else {
        return Err(RpcError::invalid(
            "end_bar",
            "the project has no regions, so the range needs an end",
        ));
    };
    if to <= from {
        return Err(RpcError::invalid(
            "end_bar",
            "the range must end after it starts (without an end it runs to the end of the last region)",
        ));
    }
    Ok(Some((from, to)))
}

/// Starts the bounce `settings` asks for and keeps the call until it ends (or answers at
/// once when the caller does not wait).
fn begin(cx: &mut Cx, settings: Settings, kind: Kind, wait: bool) -> Result<Reply, RpcError> {
    // A range that is wrong in itself is a mistake in the parameters.
    let serial = crate::bounce_ui::start(cx.app, settings.clone()).map_err(|e| {
        if e.contains("end after it starts") {
            RpcError::invalid("end_bar", e)
        } else {
            RpcError::failed(e)
        }
    })?;
    cx.ctx.request_repaint();
    if !wait {
        return ok(json!({
            "bounce": serial,
            "started": true,
            "path": settings.path.display().to_string(),
        }));
    }
    cx.core.bounces.push(Pending {
        conn: cx.conn,
        id: cx.id.clone(),
        serial,
        kind,
    });
    Ok(Reply::Later)
}

fn export_bounce(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let range = range_of(cx, params)?;
    let only = match params.get("track") {
        Some(v) => Some(state::track_ref(&cx.app.project, &cx.core.ids, v, "track")?),
        None => None,
    };
    let given = params.path("path")?;
    let format = match params.str("format") {
        Some("wav") => Format::Wav,
        Some("aiff") => Format::Aiff,
        Some(other) => {
            return Err(RpcError::invalid(
                "format",
                format!("`format` must be wav or aiff; got \"{other}\""),
            ))
        }
        None => match &given {
            Some(path) => Format::of_path(path).ok_or_else(|| {
                RpcError::invalid(
                    "path",
                    "the file's name must end in .wav or .aiff (or give `format`)",
                )
            })?,
            None => Format::Wav,
        },
    };
    let depth = match params.i64("bit_depth") {
        None | Some(24) => Depth::Pcm24,
        Some(16) => Depth::Pcm16,
        Some(32) => Depth::Float32,
        Some(other) => {
            return Err(RpcError::invalid(
                "bit_depth",
                format!("`bit_depth` must be 16, 24 or 32 (32 is float); got {other}"),
            ))
        }
    };
    let path = match given {
        Some(path) => path,
        None => super::m_ui::scratch_file(cx.core, "bounce", format.extension()),
    };
    let tail = params.f64("tail_seconds").unwrap_or(1.0).clamp(0.0, 600.0);
    // A tail that was asked for is kept as asked; without one it follows the sound.
    let longest = match (params.f64("max_tail_seconds"), params.has("tail_seconds")) {
        (Some(m), _) => m.clamp(tail, 600.0),
        (None, true) => tail,
        (None, false) => 20.0,
    };
    let stems = params.bool("stems").unwrap_or(false);
    let settings = Settings {
        path,
        range,
        format,
        depth,
        normalize: params.bool("normalize").unwrap_or(false),
        tail_seconds: tail,
        max_tail_seconds: longest,
        mix: params.bool("mix").unwrap_or(true),
        stems,
        only,
        overwrite: params.bool("overwrite").unwrap_or(false),
        strip: None,
    };
    begin(
        cx,
        settings,
        Kind::Bounce,
        params.bool("wait").unwrap_or(true),
    )
}

fn bounce_status(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    let b = &cx.app.ui.bounce;
    let mut out = Map::new();
    out.insert("running".into(), json!(b.job.is_some()));
    if let Some(pr) = b.progress() {
        out.extend(progress_json(b.serial, &pr));
    } else if let Some((serial, result)) = &b.last {
        out.insert("bounce".into(), json!(serial));
        out.insert("ok".into(), json!(result.is_ok()));
        match result {
            Ok(r) => {
                out.insert(
                    "result".into(),
                    report_json(cx.app, cx.core, *serial, r, Kind::Bounce),
                );
            }
            Err(e) => {
                out.insert("error".into(), json!(error_of(e).message));
            }
        }
    }
    ok(Value::Object(out))
}

fn bounce_cancel(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    match cx.app.ui.bounce.job.as_ref() {
        Some(job) => {
            job.cancel();
            cx.ctx.request_repaint();
            ok(json!({"cancelled": true, "bounce": cx.app.ui.bounce.serial}))
        }
        None => ok(json!({"cancelled": false})),
    }
}

fn render_range(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let range = match range_of(cx, params)? {
        Some(r) => Some(r),
        // As before the bounce: the whole song, whatever Cycle says.
        None => bounce::whole_range(&cx.app.project),
    };
    let only = match params.get("track") {
        Some(v) => Some(state::track_ref(&cx.app.project, &cx.core.ids, v, "track")?),
        None => None,
    };
    let tail = params.f64("tail_seconds").unwrap_or(1.0).clamp(0.0, 60.0);
    let path = match params.path("path")? {
        Some(path) => path,
        None => super::m_ui::scratch_file(cx.core, "render", "wav"),
    };
    if Format::of_path(&path) != Some(Format::Wav) {
        return Err(RpcError::invalid(
            "path",
            "the sound is written as a WAV file: give a path that ends in .wav",
        ));
    }
    let settings = Settings {
        path,
        range,
        format: Format::Wav,
        depth: Depth::Float32,
        normalize: false,
        tail_seconds: tail,
        max_tail_seconds: tail,
        mix: true,
        stems: false,
        only,
        overwrite: params.bool("overwrite").unwrap_or(false),
        strip: None,
    };
    begin(cx, settings, Kind::Range, true)
}

pub fn methods() -> Vec<Method> {
    let [bar, position, seconds, beat] = super::params::place_params("");
    let [end_bar, end_position, end_seconds, end_beat] = super::params::place_params("end_");
    const RESULT: &str = "{path, files: [{path, track, name, seconds, frames, peak_db, rms_db, silent, tail_seconds, gain_db, clipped_samples, speed}], seconds, sample_rate, channels, format, from, to, range_seconds, tail_seconds, peak_db, rms_db, silent, rendered_tracks, muted_tracks, not_rendered, complete, speed, took_seconds}";
    vec![
        m(
            "export.bounce",
            "Bounces the project to an audio file (File > Bounce > Project or Section): everything the app plays, through a second engine that has no sound card, faster than it plays. Audio regions, instruments, Aux strips and sends, effects, automation, tempo changes and the Stereo Out are in it; Mute and Solo are as in the app. Without a range: the cycle range while Cycle is on, else from bar 1 to the end of the last region. With `stems`, every track that is heard is also written to a file of its own beside the mix (\"<mix> - 03 <track>.wav\"), each from the same start and through the Stereo Out. `not_rendered` lists what should be in the file and is not, with the reason; `complete` says there is none. The call is answered when the bounce ends: pass `--timeout 0` to prowavectl for a long song.",
            vec![
                p("path", Ty::Str, "the file of the mix, ending in .wav or .aiff (default: a numbered WAV file in the session's folder)"),
                bar, position, seconds, beat,
                end_bar, end_position, end_seconds, end_beat,
                p("format", Ty::Str, "the file format (default: what the path's ending names)").vals(&["wav", "aiff"]),
                p("bit_depth", Ty::Int, "16 or 24 (whole numbers) or 32 (float, WAV only); default 24"),
                p("normalize", Ty::Bool, "raise or lower each file so its loudest sample is at full scale (default false)"),
                p("tail_seconds", Ty::Num, "sound kept after the end, for reverb and release. Given: exactly this much. Not given: one second, and on while the output is above -80 dB, up to `max_tail_seconds`").unit("seconds").range(0.0, 600.0),
                p("max_tail_seconds", Ty::Num, "the longest tail when it follows the sound (default 20; with `tail_seconds` given, the tail may grow up to here)").unit("seconds").range(0.0, 600.0),
                p("mix", Ty::Bool, "write the mix (default true)"),
                p("stems", Ty::Bool, "also write every track that is heard into a file of its own (default false)"),
                p("track", Ty::Any, "only this track, into `path` (the others play nothing; `mix` and `stems` are not looked at)"),
                p("overwrite", Ty::Bool, "replace files that are already there"),
                p("wait", Ty::Bool, "answer when the bounce ends (default true); false answers at once with {bounce, started, path} and the end comes as the event bounce.finished, or ask export.bounce.status"),
            ],
            "{bounce, path, files: [{path, track, name, seconds, frames, peak_db, rms_db, silent, tail_seconds, gain_db, clipped_samples, speed}], seconds, sample_rate, channels, format, from, to, range_seconds, tail_seconds, peak_db, rms_db, silent, rendered_tracks, muted_tracks, not_rendered, complete, speed, took_seconds}; the top-level path, seconds and levels are the first file's (the mix); clipped_samples counts samples at full scale (the output is cut off at 0 dBFS, as the app's is); speed is seconds of sound for each second of rendering",
            export_bounce,
        ),
        m(
            "export.bounce.status",
            "How far the running bounce is, or how the last one ended.",
            vec![],
            "{running, bounce, fraction, file, files, track, preparing} while one runs; {running: false, bounce, ok, result | error} after",
            bounce_status,
        ),
        m(
            "export.bounce.cancel",
            "Stops the running bounce; the file being written is removed and the waiting export.bounce call gets an error.",
            vec![],
            "{cancelled, bounce}",
            bounce_cancel,
        ),
        m(
            "render.range",
            "Plays a stretch of the song into a WAV file of 32-bit floats, without the speakers, for measuring what was made. It is the app's bounce (see export.bounce) with a fixed tail: audio regions, instruments, effects, sends, automation and the Stereo Out are in it. Without a range it runs from bar 1 to the end of the last region. The call is answered when the file is written.",
            vec![
                bar, position, seconds, beat,
                end_bar, end_position, end_seconds, end_beat,
                p("track", Ty::Any, "only this track (default: the whole mix)"),
                p("path", Ty::Str, "the .wav file to write (default: a numbered file in the session's folder, deleted when the app quits)"),
                p("overwrite", Ty::Bool, "replace a file that is already there"),
                p("tail_seconds", Ty::Num, "sound kept after the end, for reverb and release (default 1)").unit("seconds").range(0.0, 60.0),
            ],
            RESULT,
            render_range,
        ),
    ]
}
