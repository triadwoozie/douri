//! Automation that travels with regions: the pieces of a lane under a region, copied to where
//! the region is copied or moved. The lanes themselves are drawn by `automation`.
use crate::automation::AutoView;
use crate::model::{Automation, Project};
use prowave_engine::{lane_value, AutoPoint, AutoTarget};

/// A piece of envelope on the clipboard: breakpoints from its start, and its length.
#[derive(Clone, PartialEq, Debug)]
pub struct Piece {
    pub target: AutoTarget,
    pub len: f64,
    pub points: Vec<AutoPoint>,
}

/// The value just before `beat` (what a jump at `beat` leaves), or `None` without breakpoints.
fn value_before(pts: &[AutoPoint], beat: f64) -> Option<f32> {
    let first = pts.first()?;
    if beat <= first.beat {
        return Some(first.value);
    }
    let i = pts.partition_point(|p| p.beat < beat);
    let (a, b) = (pts[i - 1], pts.get(i).copied().unwrap_or(pts[i - 1]));
    if b.beat <= a.beat {
        return Some(a.value);
    }
    let t = prowave_engine::curve_shape(((beat - a.beat) / (b.beat - a.beat)) as f32, a.curve);
    Some(a.value + (b.value - a.value) * t)
}

/// The indexes of the breakpoints in `from..=to`.
fn in_range(pts: &[AutoPoint], from: f64, to: f64) -> std::ops::Range<usize> {
    let a = pts.partition_point(|p| p.beat < from - 1e-9);
    let b = pts.partition_point(|p| p.beat <= to + 1e-9);
    a..b.max(a)
}

/// Removes breakpoints that repeat the one before them (same beat and value).
fn dedupe(pts: &mut Vec<AutoPoint>) {
    pts.dedup_by(|b, a| (a.beat - b.beat).abs() < 1e-9 && a.value == b.value);
}

/// The envelope from `from` to `to` as a piece starting at 0, with breakpoints at both ends so
/// it keeps its values when pasted.
fn copy_piece(pts: &[AutoPoint], target: AutoTarget, from: f64, to: f64) -> Option<Piece> {
    if pts.is_empty() || to <= from {
        return None;
    }
    let mut out = Vec::new();
    let start = lane_value(pts, from)?;
    out.push(AutoPoint::new(0.0, start));
    for p in &pts[in_range(pts, from, to)] {
        if p.beat > from + 1e-9 && p.beat < to - 1e-9 {
            out.push(AutoPoint {
                beat: p.beat - from,
                ..*p
            });
        }
    }
    let end = value_before(pts, to)?;
    out.push(AutoPoint::new(to - from, end));
    Some(Piece {
        target,
        len: to - from,
        points: out,
    })
}

/// Puts `piece` at `at`, replacing the envelope there; the envelope after it keeps its values.
fn paste_piece(pts: &mut Vec<AutoPoint>, piece: &Piece, at: f64) {
    let at = at.max(0.0);
    let end = at + piece.len;
    let before = pts
        .iter()
        .any(|p| p.beat < at)
        .then(|| value_before(pts, at))
        .flatten();
    let after = pts
        .iter()
        .any(|p| p.beat > end)
        .then(|| lane_value(pts, end + 1e-9))
        .flatten();
    pts.retain(|p| p.beat < at - 1e-9 || p.beat > end + 1e-9);
    let i = pts.partition_point(|p| p.beat < at);
    let mut add = Vec::with_capacity(piece.points.len() + 2);
    if let Some(b) = before {
        add.push(AutoPoint::new(at, b));
    }
    add.extend(piece.points.iter().map(|p| AutoPoint {
        beat: p.beat + at,
        ..*p
    }));
    if let Some(a) = after {
        add.push(AutoPoint::new(end, a));
    }
    pts.splice(i..i, add);
    dedupe(pts);
    pts.truncate(Automation::MAX_POINTS);
}

/// The pieces of every lane of `a` from `from` to `to` (a region's span).
fn pieces_of(a: &Automation, from: f64, to: f64) -> Vec<Piece> {
    a.lanes()
        .filter_map(|(t, pts)| {
            // Only the breakpoints under the region count: a lane without any is left alone.
            let r = in_range(pts, from, to);
            (!r.is_empty())
                .then(|| copy_piece(pts, t, from, to))
                .flatten()
        })
        .collect()
}

/// Regions dropped on other tracks or copied by an Option-drag: (track before, start before,
/// length, track now, start now) in quarter notes. The automation under each is copied to where
/// it landed, or with `lift` moved there (the envelope closes over the hole it left).
pub fn follow_drops(
    p: &mut Project,
    v: &AutoView,
    drops: &[(usize, f64, f64, usize, f64)],
    lift: bool,
) {
    if !v.with_regions {
        return;
    }
    // Lift every piece first, so regions dropped over each other do not carry each other's.
    let mut taken = Vec::new();
    for &(from, was, len, to, now) in drops {
        let Some(track) = p.tracks.get_mut(from) else {
            continue;
        };
        let pieces = pieces_of(&track.auto, was, was + len);
        if lift {
            for piece in &pieces {
                let pts = track.auto.lane_mut(piece.target);
                pts.retain(|x| x.beat < was - 1e-9 || x.beat > was + len + 1e-9);
            }
            track.auto.prune();
        }
        taken.push((to, now, pieces));
    }
    for (to, now, pieces) in taken {
        let Some(track) = p.tracks.get_mut(to) else {
            continue;
        };
        for piece in &pieces {
            paste_piece(track.auto.lane_mut(piece.target), piece, now);
        }
        track.auto.prune();
    }
}
