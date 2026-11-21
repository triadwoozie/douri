//! The Fade tool of the Tracks area (Logic Pro: Fade tool, A in the Tool menu), beyond the
//! plain fade of one region:
//! - A drag on a selected audio region sets the same fade on every selected audio region.
//! - A drag across the place where two audio regions touch makes a crossfade there: the
//!   left region is lengthened past the junction and the right one starts before it, as far
//!   as the drag went on each side (and as far as their files reach), and the right one
//!   crossfades with the left over the overlap (`RegionParams::crossfade`, the same crossfade
//!   Drag: X-Fade makes).
//! - Option-click on a fade (or a crossfade) removes it.

use crate::model::{Project, Region, TrackKind};

/// Fade lengths are in sixteenth notes, as the Inspector's.
const STEPS_PER_BEAT: f64 = 4.0;
/// Shortest a region is left by a crossfade (quarter notes).
const MIN_LEN: f64 = 1.0 / 64.0;

fn quantise(beats: f64) -> f64 {
    (beats * STEPS_PER_BEAT).round().max(0.0) / STEPS_PER_BEAT
}

/// A Fade tool drag in progress.
#[derive(Clone)]
pub struct Drag {
    /// A fade in (the drag began in the left half of the region), else a fade out.
    fade_in: bool,
    /// The regions whose fade the drag sets: the one dragged, or every selected audio region
    /// when it is selected.
    targets: Vec<(usize, usize)>,
    /// The track, and the two regions that touch where the drag began (left, right), with
    /// how they were, should the drag cross the junction.
    pair: Option<Pair>,
    /// Where the drag began (quarter notes).
    from: f64,
    /// The start and length of the region it began on (quarter notes).
    origin: (f64, f64),
}

#[derive(Clone)]
struct Pair {
    track: usize,
    left: (usize, Region),
    right: (usize, Region),
    /// Where they touch (quarter notes).
    at: f64,
}

fn is_audio_region(p: &Project, t: usize, r: usize) -> bool {
    p.tracks.get(t).is_some_and(|tr| {
        tr.kind == TrackKind::Audio && tr.regions.get(r).is_some_and(|x| x.is_audio())
    })
}

/// The audio region of track `t` that begins exactly where region `r` ends (`after`), or ends
/// exactly where it begins.
fn touching(p: &Project, t: usize, r: usize, after: bool) -> Option<usize> {
    let track = p.tracks.get(t)?;
    let reg = track.regions.get(r)?;
    let edge = if after {
        p.region_end_beats(reg)
    } else {
        p.region_start_beats(reg)
    };
    track.regions.iter().enumerate().position(|(j, o)| {
        let other = if after {
            p.region_start_beats(o)
        } else {
            p.region_end_beats(o)
        };
        j != r && o.is_audio() && !o.params.reverse && (other - edge).abs() < 1e-6
    })
}

/// A Fade tool drag begins on region `r` of track `t` at `at` (quarter notes). `selected`:
/// the regions selected now. `None` when the region is not an audio region.
pub fn begin(
    p: &Project,
    selected: &[(usize, usize)],
    (t, r): (usize, usize),
    at: f64,
) -> Option<Drag> {
    if !is_audio_region(p, t, r) {
        return None;
    }
    let reg = &p.tracks[t].regions[r];
    let (start, end) = (p.region_start_beats(reg), p.region_end_beats(reg));
    let fade_in = at < (start + end) / 2.0;
    let targets: Vec<(usize, usize)> = if selected.contains(&(t, r)) {
        selected
            .iter()
            .copied()
            .filter(|&(st, sr)| is_audio_region(p, st, sr))
            .collect()
    } else {
        vec![(t, r)]
    };
    let pair = if reg.params.reverse {
        None
    } else {
        touching(p, t, r, !fade_in).map(|o| {
            let (l, rr) = if fade_in { (o, r) } else { (r, o) };
            let tr = &p.tracks[t];
            Pair {
                track: t,
                left: (l, tr.regions[l].clone()),
                right: (rr, tr.regions[rr].clone()),
                at: if fade_in { start } else { end },
            }
        })
    };
    Some(Drag {
        fade_in,
        targets,
        pair,
        from: at,
        origin: (start, end - start),
    })
}

/// The drag is at `at`: sets the fades (or the crossfade) for it. Each call starts again from
/// how the regions were when the drag began.
pub fn drag_to(p: &mut Project, d: &Drag, at: f64) {
    // The pair as it was, should an earlier step have made a crossfade.
    if let Some(pair) = &d.pair {
        restore(p, pair);
    }
    if let Some(pair) = &d.pair {
        let (lo, hi) = (d.from.min(at), d.from.max(at));
        if lo < pair.at - 1e-9 && hi > pair.at + 1e-9 && crossfade(p, pair, lo, hi) {
            return;
        }
    }
    // The fade is measured on the region the drag began on; every other one gets the same
    // length, as far as it is long enough to hold it.
    let length = crate::tracks_area::fade_for_drag(at, d.origin.0, d.origin.1, d.fade_in);
    for &(t, r) in &d.targets {
        let Some(reg) = p.tracks.get(t).and_then(|x| x.regions.get(r)) else {
            continue;
        };
        let len = p.region_len_beats(reg) as f32;
        let params = &mut p.tracks[t].regions[r].params;
        if d.fade_in {
            params.fade_in = length.min(len);
        } else {
            params.fade_out = length.min(len);
        }
    }
}

fn restore(p: &mut Project, pair: &Pair) {
    let Some(track) = p.tracks.get_mut(pair.track) else {
        return;
    };
    for (i, reg) in [&pair.left, &pair.right] {
        if let Some(x) = track.regions.get_mut(*i) {
            *x = reg.clone();
        }
    }
}

/// How many quarter notes of its file region `r` has after its end (none when the file is not
/// there, or the region is reversed).
fn room_after(p: &Project, r: &Region) -> f64 {
    let Some(a) = r.audio.as_ref() else {
        return 0.0;
    };
    if r.params.reverse || a.file.sample_rate == 0 {
        return 0.0;
    }
    let map = p.tempo_map();
    let file_secs = a.file.frames as f64 / f64::from(a.file.sample_rate);
    let (start, end) = (p.region_start_beats(r), p.region_end_beats(r));
    let used = r.params.offset_secs + map.seconds_at(end) - map.seconds_at(start);
    let left = (file_secs - used).max(0.0);
    (map.beat_at(map.seconds_at(end) + left) - end).max(0.0)
}

/// Makes the crossfade of `pair` over `lo..hi` (the junction inside it). Returns whether the
/// regions overlap now.
fn crossfade(p: &mut Project, pair: &Pair, lo: f64, hi: f64) -> bool {
    let t = pair.track;
    let (l, r) = (pair.left.0, pair.right.0);
    let (left, right) = (&pair.left.1, &pair.right.1);
    let (l_start, r_end) = (p.region_start_beats(left), p.region_end_beats(right));
    let into_right = quantise(hi - pair.at)
        .min(room_after(p, left))
        .min(r_end - pair.at - MIN_LEN)
        .max(0.0);
    let into_left = quantise(pair.at - lo)
        .min(pair.at - l_start - MIN_LEN)
        .max(0.0);
    // The right region starts earlier: what it plays stays in place, and it reaches back no
    // further than its file begins.
    if into_left > 0.0 {
        p.resize_region_start(t, r, pair.at - into_left);
    }
    if into_right > 0.0 {
        let end_bar = p.beats_to_bar(pair.at + into_right) as f32 + 1.0;
        let reg = &mut p.tracks[t].regions[l];
        reg.len_bars = (end_bar - reg.start_bar).max(1.0 / 64.0);
    }
    let new_start = p.region_start_beats(&p.tracks[t].regions[r]);
    let new_end = p.region_end_beats(&p.tracks[t].regions[l]);
    if new_end - new_start < 1e-6 {
        restore(p, pair);
        return false;
    }
    p.tracks[t].regions[r].params.crossfade = true;
    true
}

/// Option-click with the Fade tool at `at` on region `r` of track `t`: the fade (or the
/// crossfade) there is removed. Returns whether there was one.
pub fn remove_at(p: &mut Project, (t, r): (usize, usize), at: f64) -> bool {
    if !is_audio_region(p, t, r) {
        return false;
    }
    let (fade_in, fade_out) = p.effective_fades(t, r);
    let reg = &p.tracks[t].regions[r];
    let (start, end) = (p.region_start_beats(reg), p.region_end_beats(reg));
    if fade_in > 0.0 && at <= start + f64::from(fade_in) + 1e-9 {
        let reg = &mut p.tracks[t].regions[r];
        reg.params.fade_in = 0.0;
        reg.params.crossfade = false;
        return true;
    }
    if fade_out > 0.0 && at >= end - f64::from(fade_out) - 1e-9 {
        p.tracks[t].regions[r].params.fade_out = 0.0;
        // A later region crossfading into this one's end: its crossfade goes too.
        let crossing: Vec<usize> = p.tracks[t]
            .regions
            .iter()
            .enumerate()
            .filter(|(j, o)| {
                let os = p.region_start_beats(o);
                *j != r && o.params.crossfade && os > start && os < end
            })
            .map(|(j, _)| j)
            .collect();
        for j in crossing {
            p.tracks[t].regions[j].params.crossfade = false;
        }
        return true;
    }
    false
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{RegionParams, Track};
    use crate::tracks_area::TrackTool;
    use crate::undo::History;
    use egui::Modifiers;

    /// An audio region of the engine's test tone (a fifth of a second: the regions are
    /// longer than their file, so a crossfade cannot lengthen the left one).
    fn audio(start_bar: f32, len_bars: f32) -> Region {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../prowave-engine/tests/fixtures");
        Region {
            missing: None,
            name: "Take".into(),
            start_bar,
            len_bars,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            audio: Some(crate::model::AudioRegion {
                file: prowave_engine::AudioFile::open(dir.join("tone.caf")).expect("the fixture"),
                peaks: Default::default(),
                flex: None,
                start_frame: 0,
                len_frames: None,
            }),
            params: RegionParams::default(),
            generated: None,
        }
    }

    /// Two audio tracks: track 0 has one region over bars 1-5, cut at bar 3 into two that
    /// touch; track 1 has one region over bars 1-3.
    pub(crate) fn project() -> Project {
        let mut p = Project::empty();
        p.tracks.clear();
        let mut a = Track::new("Audio 1", TrackKind::Audio);
        a.regions.push(audio(1.0, 4.0));
        let mut b = Track::new("Audio 2", TrackKind::Audio);
        b.regions.push(audio(1.0, 2.0));
        p.tracks.push(a);
        p.tracks.push(b);
        assert!(p.split_region(0, 0, 8.0));
        p
    }

    #[test]
    fn a_drag_on_a_selected_region_fades_every_selected_audio_region() {
        let mut p = project();
        let sel = [(0, 0), (1, 0)];
        // Bar 1-3 region of track 1, left half: a fade in, one beat long.
        let d = begin(&p, &sel, (1, 0), 0.5).expect("an audio region");
        drag_to(&mut p, &d, 1.0);
        assert_eq!(p.tracks[1].regions[0].params.fade_in, 1.0);
        assert_eq!(
            p.tracks[0].regions[0].params.fade_in, 1.0,
            "the other selected one"
        );
        assert_eq!(p.tracks[0].regions[1].params.fade_in, 0.0, "not selected");
        // Not selected: only the one dragged.
        let mut p = project();
        let d = begin(&p, &[(0, 0)], (1, 0), 0.5).expect("an audio region");
        drag_to(&mut p, &d, 1.0);
        assert_eq!(p.tracks[1].regions[0].params.fade_in, 1.0);
        assert_eq!(p.tracks[0].regions[0].params.fade_in, 0.0);
    }

    #[test]
    fn a_drag_across_where_two_regions_touch_crossfades_them() {
        let mut p = project();
        // From beat 7 in the left region (its right half: a fade out) across the junction at
        // beat 8 to beat 9.
        let d = begin(&p, &[], (0, 0), 7.0).expect("an audio region");
        drag_to(&mut p, &d, 9.0);
        let (l, r) = (&p.tracks[0].regions[0], &p.tracks[0].regions[1]);
        // The file ends before the left one does, so it cannot reach further; the right one
        // starts a beat earlier, playing the same audio in place.
        assert_eq!((l.start_bar, l.len_bars), (1.0, 2.0));
        assert_eq!(r.start_bar, 2.75);
        let map = p.tempo_map();
        let secs = map.seconds_at(7.0) - map.seconds_at(0.0);
        assert!((r.params.offset_secs - secs).abs() < 1e-9);
        assert!(r.params.crossfade);
        assert_eq!(p.effective_fades(0, 0).1, 1.0, "the left one fades out");
        assert_eq!(p.effective_fades(0, 1).0, 1.0, "the right one fades in");
        // Back to this side of the junction: a plain fade out again, the regions as they were.
        drag_to(&mut p, &d, 7.5);
        let (l, r) = (&p.tracks[0].regions[0], &p.tracks[0].regions[1]);
        assert_eq!(r.start_bar, 3.0);
        assert!(!r.params.crossfade);
        assert_eq!(l.params.fade_out, 0.5);
    }

    #[test]
    fn option_click_on_a_fade_removes_it() {
        let mut p = project();
        p.tracks[1].regions[0].params.fade_in = 1.0;
        p.tracks[1].regions[0].params.fade_out = 2.0;
        assert!(!remove_at(&mut p, (1, 0), 3.0), "between the fades");
        assert!(remove_at(&mut p, (1, 0), 0.5));
        assert_eq!(p.tracks[1].regions[0].params.fade_in, 0.0);
        assert_eq!(p.tracks[1].regions[0].params.fade_out, 2.0);
        assert!(remove_at(&mut p, (1, 0), 7.0));
        assert_eq!(p.tracks[1].regions[0].params.fade_out, 0.0);
        // A crossfade: Option-click in it on the left region takes it away.
        let d = begin(&p, &[], (0, 0), 7.0).expect("an audio region");
        drag_to(&mut p, &d, 9.0);
        assert!(remove_at(&mut p, (0, 0), 7.5));
        assert!(!p.tracks[0].regions[1].params.crossfade);
        assert_eq!(p.effective_fades(0, 0), (0.0, 0.0));
    }

    #[test]
    fn the_fade_tool_in_the_tracks_area_crossfades_and_option_click_removes_it() {
        let mut r = crate::marquee::rig::Rig::new(project());
        r.s.tracks.tool = TrackTool::Fade;
        let mut hist = History::new(&r.p);
        let (from, to) = (r.at(7.0, 0), r.at(9.0, 0));
        r.drag(from, to, Modifiers::NONE);
        assert!(r.p.tracks[0].regions[1].params.crossfade);
        assert_eq!(r.p.tracks[0].regions[1].start_bar, 2.75);
        assert!(hist.record(&r.p));
        assert!(hist.undo(&mut r.p));
        assert!(!r.p.tracks[0].regions[1].params.crossfade);
        assert!(!hist.can_undo(), "one step");
        // A fade in on track 1, then Option-click on it.
        let (from, to) = (r.at(0.2, 1), r.at(1.0, 1));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(r.p.tracks[1].regions[0].params.fade_in, 1.0);
        let at = r.at(0.5, 1);
        r.click(at, Modifiers::ALT);
        assert_eq!(r.p.tracks[1].regions[0].params.fade_in, 0.0);
        assert_eq!(r.spans(1), vec![(1.0, 3.0)], "nothing moved");
    }
}
