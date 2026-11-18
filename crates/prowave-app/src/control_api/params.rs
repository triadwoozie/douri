//! Reading a request's parameters: typed getters, and the ways a place in the song can be
//! given (a bar, a "bar.beat" position, seconds, or quarter notes).
use super::protocol::RpcError;
use super::registry::{p, Param, Ty};
use crate::model::Project;
use serde_json::{Map, Value};

/// The parameters of one request (already checked against the method's list).
pub struct P<'a>(pub &'a Map<String, Value>);

impl<'a> P<'a> {
    pub fn get(&self, key: &str) -> Option<&'a Value> {
        self.0.get(key).filter(|v| !v.is_null())
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn str(&self, key: &str) -> Option<&'a str> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn f64(&self, key: &str) -> Option<f64> {
        self.get(key)
            .and_then(Value::as_f64)
            .filter(|f| f.is_finite())
    }

    pub fn f32(&self, key: &str) -> Option<f32> {
        self.f64(key).map(|f| f as f32)
    }

    pub fn i64(&self, key: &str) -> Option<i64> {
        self.f64(key).map(|f| f as i64)
    }

    pub fn bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Value::as_bool)
    }

    pub fn arr(&self, key: &str) -> Option<&'a Vec<Value>> {
        self.get(key).and_then(Value::as_array)
    }

    fn missing(key: &str) -> RpcError {
        RpcError::invalid(key, format!("the parameter `{key}` is needed"))
    }

    pub fn req_str(&self, key: &str) -> Result<&'a str, RpcError> {
        self.str(key).ok_or_else(|| Self::missing(key))
    }

    pub fn req_f64(&self, key: &str) -> Result<f64, RpcError> {
        self.f64(key).ok_or_else(|| Self::missing(key))
    }

    /// A file path: absolute, or starting with `~/`.
    pub fn path(&self, key: &str) -> Result<Option<std::path::PathBuf>, RpcError> {
        let Some(text) = self.str(key) else {
            return Ok(None);
        };
        expand_path(text)
            .map(Some)
            .map_err(|m| RpcError::invalid(key, format!("`{key}`: {m}")))
    }

    pub fn req_path(&self, key: &str) -> Result<std::path::PathBuf, RpcError> {
        self.path(key)?.ok_or_else(|| Self::missing(key))
    }
}

/// `text` as an absolute path (`~/` is the home folder).
pub fn expand_path(text: &str) -> Result<std::path::PathBuf, String> {
    if text.is_empty() {
        return Err("the path is empty".into());
    }
    if let Some(rest) = text.strip_prefix("~/") {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        return Ok(std::path::PathBuf::from(home).join(rest));
    }
    let path = std::path::PathBuf::from(text);
    if !path.is_absolute() {
        return Err(format!(
            "\"{text}\" is not an absolute path (the app's folder is not yours: give the whole path, or start with ~/)"
        ));
    }
    Ok(path)
}

// ------------------------------------------------------------------ places in the song

/// The four ways to give a place, with `prefix` before each name ("" or e.g. "end_").
/// `what` says which place it is.
pub fn place_params(prefix: &'static str) -> [Param; 4] {
    // The names must be 'static: the few prefixes in use are spelled out.
    let (bar, position, seconds, beat) = match prefix {
        "" => ("bar", "position", "seconds", "beat"),
        "end_" => ("end_bar", "end_position", "end_seconds", "end_beat"),
        "start_" => ("start_bar", "start_position", "start_seconds", "start_beat"),
        "to_" => ("to_bar", "to_position", "to_seconds", "to_beat"),
        _ => ("bar", "position", "seconds", "beat"),
    };
    [
        p(bar, Ty::Num, "a place as a bar, 1-based as the app shows it; a fraction is a part of the bar (5.5 is halfway through bar 5)")
            .unit("bars")
            .range(1.0, 100_000.0),
        p(position, Ty::Str, "a place as \"bar.beat\", \"bar.beat.division\" or \"bar.beat.division.tick\", all 1-based as in the app's display (\"5.3\" is beat 3 of bar 5; divisions are sixteenths, 240 ticks each)"),
        p(seconds, Ty::Num, "a place as seconds from the start of the song")
            .unit("seconds")
            .range(0.0, 1.0e7),
        p(beat, Ty::Num, "a place as quarter notes from the start of the song, 0-based")
            .unit("quarter notes")
            .range(0.0, 1.0e8),
    ]
}

/// The place given with `prefix`, in quarter notes from the start. `None`: none was given.
pub fn place(project: &Project, params: &P, prefix: &str) -> Result<Option<f64>, RpcError> {
    let key = |name: &str| format!("{prefix}{name}");
    let given: Vec<String> = ["bar", "position", "seconds", "beat"]
        .iter()
        .map(|n| key(n))
        .filter(|k| params.has(k))
        .collect();
    if given.len() > 1 {
        return Err(RpcError::invalid(
            &given[1],
            format!("give one of {} , not several", given.join(", ")),
        ));
    }
    if let Some(bar) = params.f64(&key("bar")) {
        return Ok(Some(project.bar_to_beats((bar - 1.0).max(0.0))));
    }
    if let Some(text) = params.str(&key("position")) {
        return position_beats(project, text)
            .map(Some)
            .map_err(|m| RpcError::invalid(&key("position"), m));
    }
    if let Some(secs) = params.f64(&key("seconds")) {
        return Ok(Some(project.tempo_map().beat_at(secs.max(0.0))));
    }
    if let Some(beat) = params.f64(&key("beat")) {
        return Ok(Some(beat.max(0.0)));
    }
    Ok(None)
}

pub fn req_place(project: &Project, params: &P, prefix: &str) -> Result<f64, RpcError> {
    place(project, params, prefix)?.ok_or_else(|| {
        RpcError::invalid(
            &format!("{prefix}bar"),
            format!(
                "a place is needed: give {prefix}bar (1-based), {prefix}position (\"bar.beat\"), {prefix}seconds or {prefix}beat (quarter notes, 0-based)"
            ),
        )
    })
}

/// "bar.beat[.division[.tick]]" (1-based, as the display shows a place) in quarter notes.
pub fn position_beats(project: &Project, text: &str) -> Result<f64, String> {
    let bad = || {
        format!(
            "\"{text}\" is not a position: write bar.beat, bar.beat.division or bar.beat.division.tick with whole numbers from 1 (for example \"5.3\" or \"5.3.2.120\")"
        )
    };
    let parts: Vec<u32> = text
        .trim()
        .split('.')
        .map(|s| s.trim().parse::<u32>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    if parts.is_empty() || parts.len() > 4 || parts.contains(&0) {
        return Err(bad());
    }
    if parts[0] > 100_000 {
        return Err(format!("\"{text}\": bars go from 1 to 100000"));
    }
    let bar0 = parts[0] - 1;
    let (num, den) = project.signature_of_bar(bar0);
    // The signature's beat in quarter notes (an eighth in 6/8).
    let unit = 4.0 / f64::from(den.max(1));
    let beat = parts.get(1).copied().unwrap_or(1);
    if beat > u32::from(num) {
        return Err(format!(
            "\"{text}\": bar {} is in {num}/{den}, so its beats go from 1 to {num}",
            parts[0]
        ));
    }
    let divisions = (unit * 4.0).max(1.0);
    let div = parts.get(2).copied().unwrap_or(1);
    if f64::from(div) > divisions.ceil() {
        return Err(format!(
            "\"{text}\": a beat of bar {} has {} divisions (sixteenths)",
            parts[0],
            divisions.ceil()
        ));
    }
    let tick = parts.get(3).copied().unwrap_or(1);
    if tick > 240 {
        return Err(format!("\"{text}\": ticks go from 1 to 240"));
    }
    let in_beat = (f64::from(div - 1) + f64::from(tick - 1) / 240.0) / divisions;
    Ok(project.bar_to_beats(f64::from(bar0)) + (f64::from(beat - 1) + in_beat) * unit)
}

/// A place in quarter notes as the result of a call shows it.
pub fn place_json(project: &Project, beats: f64) -> Value {
    let (bar, beat, div, tick) = project.bbdt(beats);
    serde_json::json!({
        "bar": round6(project.beats_to_bar(beats) + 1.0),
        "position": format!("{bar}.{beat}.{div}.{tick}"),
        "beat": round6(beats),
        "seconds": round6(project.tempo_map().seconds_at(beats)),
    })
}

/// Six decimals: what a JSON reader needs of a float that came from an `f32`.
pub fn round6(x: f64) -> f64 {
    (x * 1.0e6).round() / 1.0e6
}

pub fn round3(x: f64) -> f64 {
    (x * 1.0e3).round() / 1.0e3
}
