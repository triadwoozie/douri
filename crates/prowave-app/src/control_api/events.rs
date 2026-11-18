//! The events a subscribed connection is sent: once a frame the app is compared with how it
//! was a frame ago, and each difference worth waiting for becomes one line.
use super::params::place_json;
use super::{state, Core};
use crate::App;
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

/// Every event name, for `events.subscribe` and the documents.
pub const NAMES: &[&str] = &[
    "transport.started",
    "transport.stopped",
    "transport.recording",
    "transport.bar",
    "transport.tempo",
    "project.changed",
    "project.saved",
    "project.opened",
    "track.added",
    "track.removed",
    "recording.finished",
    "import.fitted",
    "bounce.progress",
    "bounce.finished",
    "dialog.opened",
    "dialog.closed",
    "status",
];

/// How often the slower comparisons are made (the project's content, the track list).
const SLOW: Duration = Duration::from_millis(250);

/// How the app was when it was last looked at.
#[derive(Default)]
pub struct Watch {
    seen: bool,
    playing: bool,
    recording: bool,
    bar: i64,
    tempo: u32,
    status: Option<String>,
    dialog: Option<&'static str>,
    fit_busy: bool,
    audio_rec: bool,
    midi_rec: bool,
    /// The regions being recorded (track, region), while a recording runs.
    takes: Vec<(usize, usize)>,
    path: Option<std::path::PathBuf>,
    name: String,
    slow_at: Option<Instant>,
    doc: Option<prowave_project::ProjectDoc>,
    dirty: bool,
    tracks: Vec<(String, String)>,
}

fn fields(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Compares the app with the last look and sends what changed to the subscribed connections.
pub fn watch(app: &App, core: &mut Core) {
    let p = &app.project;
    let shared = core.shared.clone();
    let emit = |name: &str, v: Value| shared.emit(name, fields(v));
    let w = &mut core.watch;
    let bar = p.beats_to_bar(p.position_beats).floor() as i64 + 1;
    let tempo = p.tempo_at(p.position_beats).to_bits();
    let dialog = super::m_ui::open_dialog(app).map(|d| d.id);
    let audio_rec = app.ui.audio_rec.active();
    let midi_rec = app.ui.rec.active();
    let fit_busy = app.ui.fit.busy();
    if !w.seen {
        // The first look only notes how things are.
        w.seen = true;
        w.playing = p.playing;
        w.recording = p.recording;
        w.bar = bar;
        w.tempo = tempo;
        w.status = app.ui.status.clone();
        w.dialog = dialog;
        w.fit_busy = fit_busy;
        w.audio_rec = audio_rec;
        w.midi_rec = midi_rec;
        w.path = app.path.clone();
        w.name = p.name.clone();
    }
    if p.playing != w.playing {
        w.playing = p.playing;
        let name = if p.playing {
            "transport.started"
        } else {
            "transport.stopped"
        };
        emit(name, json!({"playhead": place_json(p, p.position_beats)}));
    }
    if p.recording != w.recording {
        w.recording = p.recording;
        emit("transport.recording", json!({"on": p.recording}));
    }
    if bar != w.bar {
        w.bar = bar;
        if p.playing {
            emit("transport.bar", json!({"bar": bar}));
        }
    }
    if tempo != w.tempo {
        w.tempo = tempo;
        emit(
            "transport.tempo",
            json!({"bpm": p.tempo_at(p.position_beats)}),
        );
    }
    if app.ui.status != w.status {
        w.status = app.ui.status.clone();
        if let Some(text) = &w.status {
            emit("status", json!({"text": text}));
        }
    }
    if dialog != w.dialog {
        if let Some(d) = super::m_ui::open_dialog(app) {
            emit("dialog.opened", json!({"dialog": d.to_json()}));
        } else if let Some(old) = w.dialog {
            emit("dialog.closed", json!({"id": old}));
        }
        w.dialog = dialog;
    }
    if fit_busy != w.fit_busy {
        w.fit_busy = fit_busy;
        if !fit_busy {
            emit(
                "import.fitted",
                json!({"status": app.ui.status, "tempo": p.tempo}),
            );
        }
    }
    // A recording: the regions it writes are noted while it runs and reported when it ends.
    if audio_rec {
        w.takes = app.ui.audio_rec.regions().collect();
    }
    if midi_rec {
        if let Some(target) = app.ui.rec.target() {
            w.takes = vec![target];
        }
    }
    let was_recording = w.audio_rec || w.midi_rec;
    let kind = if w.audio_rec { "audio" } else { "midi" };
    w.audio_rec = audio_rec;
    w.midi_rec = midi_rec;
    if was_recording && !audio_rec && !midi_rec {
        core.ids.reconcile(p);
        let regions: Vec<Value> = std::mem::take(&mut w.takes)
            .into_iter()
            .filter_map(|(t, r)| {
                let region = p.tracks.get(t)?.regions.get(r)?;
                Some(json!({
                    "id": core.ids.region_id(t, r),
                    "track": core.ids.track_id(t),
                    "name": region.name,
                    "file": region.audio.as_ref().map(|a| a.file.path.display().to_string()),
                }))
            })
            .collect();
        emit(
            "recording.finished",
            json!({"kind": kind, "regions": regions}),
        );
    }
    let w = &mut core.watch;
    if app.path != w.path || p.name != w.name {
        let opened = app.path != w.path;
        w.path = app.path.clone();
        w.name = p.name.clone();
        if opened {
            emit(
                "project.opened",
                json!({"name": p.name, "path": app.path.as_ref().map(|x| x.display().to_string())}),
            );
        }
    }
    // The slower looks.
    if w.slow_at.is_some_and(|t| t.elapsed() < SLOW) {
        return;
    }
    w.slow_at = Some(Instant::now());
    let doc = crate::file_commands::content(p);
    let dirty = doc != app.saved;
    if w.doc.as_ref().is_some_and(|old| *old != doc) {
        emit("project.changed", json!({"dirty": dirty}));
    }
    if w.doc.is_some() && w.dirty && !dirty {
        emit(
            "project.saved",
            json!({"path": app.path.as_ref().map(|x| x.display().to_string())}),
        );
    }
    w.doc = Some(doc);
    w.dirty = dirty;
    core.ids.reconcile(p);
    let now: Vec<(String, String)> = core
        .ids
        .track_ids()
        .into_iter()
        .zip(p.tracks.iter().map(|t| t.name.clone()))
        .collect();
    let w = &mut core.watch;
    if !w.tracks.is_empty() {
        for (i, (id, name)) in now.iter().enumerate() {
            if !w.tracks.iter().any(|(old, _)| old == id) {
                let kind = p.tracks.get(i).map_or("", |t| state::kind_name(t.kind));
                emit(
                    "track.added",
                    json!({"id": id, "name": name, "kind": kind, "number": i + 1}),
                );
            }
        }
        for (id, name) in &w.tracks {
            if !now.iter().any(|(new, _)| new == id) {
                emit("track.removed", json!({"id": id, "name": name}));
            }
        }
    }
    w.tracks = now;
}
