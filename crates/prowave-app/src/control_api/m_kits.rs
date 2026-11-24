//! Methods for the drum-kit library and the Kit Browser (the Kits key of the Channel Rack):
//! the library folders, a search of the kits, a kit's samples and where a load puts them, and
//! loading a kit or one sample into a rack. A scan and a kit's files are read before the answer
//! here, so a caller needs no waiting of its own.
use super::params::P;
use super::protocol::RpcError;
use super::registry::{m, p, Method, Reply, Ty};
use super::state;
use super::{ok, Cx};
use crate::channel_rack;
use crate::kit_browser;
use crate::kit_files::Kit;
use crate::kit_library::Filter;
use serde_json::{json, Value};

fn status_json(cx: &Cx) -> Value {
    let v = &cx.app.ui.kits;
    let folders: Vec<Value> = v
        .settings
        .folders
        .iter()
        .map(|f| {
            let info = v.index.roots.iter().find(|r| &r.path == f);
            json!({
                "path": f,
                "present": info.is_some_and(|r| r.present),
                "kits": info.map_or(0, |r| r.kits),
            })
        })
        .collect();
    let sources: Vec<Value> = v
        .index
        .sources
        .iter()
        .map(|(name, kits)| json!({ "name": name, "kits": kits }))
        .collect();
    json!({
        "folders": folders,
        "kits": v.index.kits.len(),
        "sources": sources,
        "scanning": v.scanning,
        "scan_ms": v.last_scan.map(|d| super::params::round3(d.as_secs_f64() * 1000.0)),
        "favorites": v.settings.favorites.len(),
    })
}

/// The index is there: scanned once before the first answer.
fn ready(cx: &mut Cx) {
    let v = &mut cx.app.ui.kits;
    if v.last_scan.is_none() {
        v.scan_now();
    }
}

fn status(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    ready(cx);
    ok(status_json(cx))
}

fn rescan(cx: &mut Cx, _: &P) -> Result<Reply, RpcError> {
    cx.app.ui.kits.scan_now();
    ok(status_json(cx))
}

fn folders(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let Some(list) = params.arr("folders") else {
        return Err(RpcError::invalid(
            "folders",
            "`folders` is needed: the library folders, a list of paths",
        ));
    };
    let mut folders = Vec::with_capacity(list.len());
    for f in list {
        let Some(text) = f.as_str() else {
            return Err(RpcError::invalid("folders", "`folders` is a list of paths"));
        };
        let path = super::params::expand_path(text).map_err(|e| RpcError::invalid("folders", e))?;
        folders.push(path.to_string_lossy().into_owned());
    }
    let v = &mut cx.app.ui.kits;
    v.set_folders(folders);
    v.scan_now();
    ok(status_json(cx))
}

fn search(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    ready(cx);
    let v = &mut cx.app.ui.kits;
    let filter = Filter {
        query: params.str("query").unwrap_or("").to_string(),
        source: params.str("source").map(str::to_string),
        best: params.bool("best").unwrap_or(false),
        favorites: params.bool("favorites").unwrap_or(false),
    };
    let hits = crate::kit_library::search(&v.index, &filter, &v.settings.favorites);
    let offset = params.i64("offset").unwrap_or(0).max(0) as usize;
    let limit = params.i64("limit").unwrap_or(50).clamp(1, 1000) as usize;
    let kits: Vec<Value> = hits
        .iter()
        .skip(offset)
        .take(limit)
        .map(|i| {
            let k = &v.index.kits[*i as usize];
            json!({
                "path": k.path,
                "source": k.source,
                "name": k.name,
                "files": k.files,
                "license": k.license,
                "best": k.best,
                "favorite": v.is_favorite(&k.path),
            })
        })
        .collect();
    ok(json!({ "total": hits.len(), "offset": offset, "kits": kits }))
}

/// The kit in the folder `path` names, read now.
fn read(cx: &mut Cx, params: &P) -> Result<(String, std::sync::Arc<Kit>), RpcError> {
    let path = params.req_path("path")?;
    if !path.is_dir() {
        return Err(RpcError::not_found(format!(
            "there is no folder {}",
            path.display()
        )));
    }
    let path = path.to_string_lossy().into_owned();
    let kit = cx.app.ui.kits.select_now(&path);
    Ok((path, kit))
}

fn kit_json(path: &str, kit: &Kit, limit: usize) -> Value {
    let samples: Vec<Value> = kit
        .samples
        .iter()
        .take(limit)
        .enumerate()
        .map(|(i, s)| {
            json!({
                "index": i,
                "file": s.rel,
                "name": s.name,
                "class": s.class.id(),
            })
        })
        .collect();
    let channels: Vec<Value> = kit
        .pads
        .iter()
        .enumerate()
        .map(|(k, pad)| match pad {
            Some(i) => json!({
                "channel": k + 1,
                "file": kit.samples[*i].rel,
                "class": kit.samples[*i].class.id(),
            }),
            None => json!({ "channel": k + 1, "file": null, "class": null }),
        })
        .collect();
    json!({
        "path": path,
        "kind": kit.kind.id(),
        "files": kit.samples.len(),
        "samples": samples,
        "channels": channels,
    })
}

fn get(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let (path, kit) = read(cx, params)?;
    let limit = params.i64("limit").unwrap_or(200).clamp(0, 10_000) as usize;
    ok(kit_json(&path, &kit, limit))
}

fn rack_json(cx: &Cx, ti: usize) -> Value {
    let slot = cx.app.project.tracks[ti].au_instrument.as_ref();
    let channels: Vec<Value> = slot
        .map(|s| channel_rack::decode(&s.state))
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(k, c)| json!({ "channel": k + 1, "name": c.name, "path": c.path }))
        .collect();
    json!({
        "track": state::strip_id(&cx.core.ids, ti),
        "kit": slot.map(|s| channel_rack::kit_of(&s.state)).unwrap_or_default(),
        "channels": channels,
    })
}

/// The rack track a request names, or the selected track when it plays the rack.
fn rack_of(cx: &mut Cx, params: &P) -> Result<Option<usize>, RpcError> {
    if !params.has("track") {
        return Ok(channel_rack::rack_track(&cx.app.project));
    }
    let ti = state::req_track(&cx.app.project, &cx.core.ids, params, "track")?;
    let is_rack = cx.app.project.tracks[ti]
        .au_instrument
        .as_ref()
        .is_some_and(channel_rack::is_rack);
    if !is_rack {
        return Err(RpcError::failed(format!(
            "track \"{}\" does not play the Channel Rack",
            cx.app.project.tracks[ti].name
        )));
    }
    Ok(Some(ti))
}

fn load(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let track = rack_of(cx, params)?;
    let (path, kit) = read(cx, params)?;
    if kit.pads.is_empty() {
        return Err(RpcError::failed(format!(
            "{path} has no drum samples to put on the channels"
        )));
    }
    let name = cx
        .app
        .ui
        .kits
        .index
        .kits
        .iter()
        .find(|k| k.path == path)
        .map(|k| k.name.clone())
        .unwrap_or_else(|| {
            std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    let ti = channel_rack::load_kit(
        &mut cx.app.project,
        track,
        &name,
        &kit_browser::pad_paths(&kit),
    );
    cx.core.ids.reconcile(&cx.app.project);
    ok(rack_json(cx, ti))
}

fn assign(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let track = rack_of(cx, params)?;
    let file = params.req_path("file")?;
    if !file.is_file() {
        return Err(RpcError::not_found(format!(
            "there is no file {}",
            file.display()
        )));
    }
    let Some(channel) = params.i64("channel") else {
        return Err(RpcError::invalid(
            "channel",
            "`channel` is needed: the rack's channel, from 1",
        ));
    };
    let ti = match track {
        Some(ti) => ti,
        None => channel_rack::load_kit(&mut cx.app.project, None, "", &[]),
    };
    if !channel_rack::assign_sample(&mut cx.app.project, ti, channel.max(1) as usize - 1, &file) {
        return Err(RpcError::failed("the sample could not be put there"));
    }
    cx.core.ids.reconcile(&cx.app.project);
    ok(rack_json(cx, ti))
}

fn favorite(cx: &mut Cx, params: &P) -> Result<Reply, RpcError> {
    let path = params.req_path("path")?.to_string_lossy().into_owned();
    let on = params.bool("on").unwrap_or(true);
    let v = &mut cx.app.ui.kits;
    v.start();
    v.set_favorite(&path, on);
    ok(json!({ "path": path, "favorite": v.is_favorite(&path) }))
}

pub fn methods() -> Vec<Method> {
    let track = || {
        p(
            "track",
            Ty::Any,
            "the rack's track: its id (\"t3\"), its name, or its number in the list (from 1); left out: the selected track when it plays the Channel Rack, else a new Channel Rack track",
        )
    };
    let status_result = "{folders: [{path, present, kits}], kits, sources: [{name, kits}], scanning, scan_ms, favorites}";
    let rack_result = "{track, kit, channels: [{channel, name, path}]}";
    vec![
        m("kits.status", "The drum-kit library: its folders (present: the folder is there now), the number of kits and the sources. The first call scans the folders.",
            vec![], status_result, status),
        m("kits.rescan", "Scans the library folders again and waits for it; scan_ms is how long it took.",
            vec![], status_result, rescan),
        m("kits.folders", "Sets the library folders (saved), then scans them. A folder that is not there is kept and shows present: false.",
            vec![p("folders", Ty::Arr, "the folders, a list of paths").req()],
            status_result, folders),
        m("kits.search", "The kits whose source or name holds every word of the query, as the Kit Browser lists them.",
            vec![
                p("query", Ty::Str, "words to look for (left out: every kit)"),
                p("source", Ty::Str, "only kits of this source"),
                p("best", Ty::Bool, "only kits the library's BEST.md ranks, in its order"),
                p("favorites", Ty::Bool, "only the favourites"),
                p("limit", Ty::Int, "most kits to return (default 50)").range(1.0, 1000.0),
                p("offset", Ty::Int, "kits to skip (default 0)").range(0.0, 10_000_000.0),
            ],
            "{total, offset, kits: [{path, source, name, files, license, best, favorite}]}", search),
        m("kits.get", "Reads a kit's folder: its audio files with the class of each (kick, snare, clap, closed_hat, open_hat, rim, tom_low, tom_mid, tom_high, perc, shaker, crash, ride, 808, fx, loop, other) and the channels a load fills.",
            vec![
                p("path", Ty::Str, "the kit's folder").req(),
                p("limit", Ty::Int, "most samples to list (default 200)").range(0.0, 10_000.0),
            ],
            "{path, kind (files, sfz, hydrogen, drumgizmo), files, samples: [{index, file, name, class}], channels: [{channel, file, class}]}", get),
        m("kits.load", "Loads a kit into a Channel Rack: its samples go on the channels kits.get lists; levels, pans, mutes and steps stay.",
            vec![p("path", Ty::Str, "the kit's folder").req(), track()],
            rack_result, load).changes(),
        m("kits.assign", "Puts one audio file on one channel of a Channel Rack (empty channels are added up to it).",
            vec![
                p("file", Ty::Str, "the audio file").req(),
                p("channel", Ty::Int, "the channel, from 1").req().range(1.0, 64.0),
                track(),
            ],
            rack_result, assign).changes(),
        m("kits.favorite", "Marks a kit as a favourite, or takes the mark off.",
            vec![
                p("path", Ty::Str, "the kit's folder").req(),
                p("on", Ty::Bool, "false takes the mark off (default true)"),
            ],
            "{path, favorite}", favorite),
    ]
}
