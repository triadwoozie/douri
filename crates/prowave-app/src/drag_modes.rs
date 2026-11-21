//! The Drag modes of the Tracks area beyond Overlap and X-Fade (Logic Pro: the Drag pop-up
//! menu in the Tracks area's menu bar): what happens to the other regions of a track when
//! regions are dropped on it (Pointer-tool drag), or deleted from it.
//!
//! - **No Overlap:** where two regions overlap after a drop, the earlier one is shortened so
//!   that it ends where the later one begins (Logic: "the overlapped area of the left
//!   (earlier) region is shortened"). Two regions that start at the same place are left as
//!   they are: neither is earlier.
//! - **Shuffle L:** the regions of the track close up to the left. A region taken away (moved
//!   off, or deleted) leaves no gap: the regions after it move left by its length. A region
//!   dropped in a gap starts where the region before it ends; dropped on the first half of a
//!   region it goes in front of it, on the second half behind it, and the regions from there
//!   on move right by its length (so two regions dragged over each other swap places).
//! - **Shuffle R:** the same, mirrored: regions before a region taken away move right by its
//!   length; a region dropped in a gap ends where the region after it starts, and the regions
//!   before it move left by its length, no further than the start of the song.
//!
//! Resizing, Nudge, the Eraser and deleting a marquee's contents in these modes are in
//! `shuffle_edit.rs`.

use crate::model::Project;
use crate::tracks_area::DragMode;

const EPS: f64 = 1e-6;

/// Region `r` of track `t` starts at `beats`.
pub(crate) fn set_start(p: &mut Project, t: usize, r: usize, beats: f64) {
    p.tracks[t].regions[r].start_bar = p.beats_to_bar(beats.max(0.0)) as f32 + 1.0;
}

/// Region `r` of track `t` ends at `beats` (its start stays).
fn set_end(p: &mut Project, t: usize, r: usize, beats: f64) {
    let end_bar = p.beats_to_bar(beats) as f32 + 1.0;
    let reg = &mut p.tracks[t].regions[r];
    reg.len_bars = (end_bar - reg.start_bar).max(1.0 / 64.0);
}

/// Where region `r` of track `t` starts and ends (quarter notes).
pub(crate) fn span(p: &Project, t: usize, r: usize) -> (f64, f64) {
    let reg = &p.tracks[t].regions[r];
    (p.region_start_beats(reg), p.region_end_beats(reg))
}

/// Moves every region of track `t` not in `skip` that `pick` takes (by its start and end) by
/// `by` quarter notes.
fn shift(p: &mut Project, t: usize, skip: &[usize], by: f64, pick: impl Fn(f64, f64) -> bool) {
    for r in 0..p.tracks[t].regions.len() {
        if skip.contains(&r) {
            continue;
        }
        let (s, e) = span(p, t, r);
        if pick(s, e) {
            set_start(p, t, r, s + by);
        }
    }
}

/// The end of a Pointer-tool drag of regions (see `region_edit::drop_regions`) in Drag mode
/// `mode`. Returns where the landed regions are now.
pub(crate) fn drop(
    p: &mut Project,
    auto: &crate::automation::AutoView,
    group: &[(usize, usize, f64)],
    delta: f64,
    rows: isize,
    copy: bool,
    mode: DragMode,
) -> Vec<(usize, usize)> {
    // Where each region was, and how long it is, before the drop.
    let left: Vec<(usize, f64, f64)> = group
        .iter()
        .filter_map(|&(t, r, s0)| {
            let reg = p.tracks.get(t)?.regions.get(r)?;
            Some((t, s0, p.region_len_beats(reg)))
        })
        .collect();
    let landed = crate::region_edit::drop_regions(
        p,
        group,
        delta,
        rows,
        copy,
        mode == DragMode::XFade,
    );
    match mode {
        DragMode::Overlap | DragMode::XFade => {}
        DragMode::NoOverlap => no_overlap(p, &landed),
        DragMode::ShuffleL | DragMode::ShuffleR => {
            let right = mode == DragMode::ShuffleR;
            if !copy {
                close_gaps(p, &left, &landed, right);
            }
            insert(p, &landed, right);
        }
    }
    p.song_rev += 1;
    landed
}

/// No Overlap: each landed region and the regions it overlaps on its track; the earlier of
/// two is shortened to where the later begins.
fn no_overlap(p: &mut Project, landed: &[(usize, usize)]) {
    for &(t, x) in landed {
        for o in 0..p.tracks[t].regions.len() {
            if o == x {
                continue;
            }
            let (xs, xe) = span(p, t, x);
            let (os, oe) = span(p, t, o);
            if os < xs - EPS && oe > xs + EPS {
                set_end(p, t, o, xs);
            } else if xs < os - EPS && xe > os + EPS {
                set_end(p, t, x, os);
            }
        }
        p.tracks[t].regions[x].params.crossfade = false;
    }
}

/// Shuffle: the places the regions were taken from close up. `left`: (track, start, length)
/// of each region before the drop.
fn close_gaps(p: &mut Project, left: &[(usize, f64, f64)], landed: &[(usize, usize)], right: bool) {
    let mut gone = left.to_vec();
    // Shuffle L: the latest first, so the regions after an earlier one are still after it
    // when it is done; Shuffle R: the earliest first.
    gone.sort_by(|a, b| a.1.total_cmp(&b.1));
    if !right {
        gone.reverse();
    }
    for (t, s0, len) in gone {
        let skip: Vec<usize> = landed.iter().filter(|l| l.0 == t).map(|l| l.1).collect();
        if right {
            shift(p, t, &skip, len, |_, e| e <= s0 + EPS);
        } else {
            shift(p, t, &skip, -len, |s, _| s >= s0 + len - EPS);
        }
    }
}

/// Shuffle: each landed region takes its place among the others on its track.
fn insert(p: &mut Project, landed: &[(usize, usize)], right: bool) {
    let mut order = landed.to_vec();
    order.sort_by(|a, b| span(p, a.0, a.1).0.total_cmp(&span(p, b.0, b.1).0));
    if right {
        order.reverse();
    }
    for (k, &(t, x)) in order.iter().enumerate() {
        // The landed regions still to place do not count yet.
        let skip: Vec<usize> = order[k..]
            .iter()
            .filter(|l| l.0 == t)
            .map(|l| l.1)
            .collect();
        let others: Vec<(f64, f64)> = (0..p.tracks[t].regions.len())
            .filter(|r| !skip.contains(r))
            .map(|r| span(p, t, r))
            .collect();
        let (xs, xe) = span(p, t, x);
        let len = xe - xs;
        if right {
            // Where its end lands: in a region, before or after it; in a gap, at the start of
            // the region that follows.
            let at = match others.iter().find(|&&(s, e)| s < xe - EPS && xe <= e + EPS) {
                Some(&(s, e)) if xe > (s + e) / 2.0 => e,
                Some(&(s, _)) => s,
                None => others
                    .iter()
                    .map(|o| o.0)
                    .filter(|&s| s >= xe - EPS)
                    .fold(f64::INFINITY, f64::min),
            };
            let at = if at.is_finite() { at } else { xe };
            // The regions before move left by its length, no further than the song's start.
            let earliest = others
                .iter()
                .filter(|o| o.1 <= at + EPS)
                .map(|o| o.0)
                .fold(f64::INFINITY, f64::min);
            let by = if earliest.is_finite() {
                len.min(earliest)
            } else {
                0.0
            };
            shift(p, t, &skip, -by, |_, e| e <= at + EPS);
            set_start(p, t, x, (at - len).max(0.0));
        } else {
            let at = match others.iter().find(|&&(s, e)| s - EPS <= xs && xs < e - EPS) {
                Some(&(s, e)) if xs < (s + e) / 2.0 => s,
                Some(&(_, e)) => e,
                None => others
                    .iter()
                    .map(|o| o.1)
                    .filter(|&e| e <= xs + EPS)
                    .fold(f64::NEG_INFINITY, f64::max),
            };
            let at = if at.is_finite() { at } else { xs };
            shift(p, t, &skip, len, |s, _| s >= at - EPS);
            set_start(p, t, x, at);
        }
        p.tracks[t].regions[x].params.crossfade = false;
    }
}

/// Delete in a Shuffle mode: before the regions `sel` are removed, the others on their tracks
/// move by each one's length to close its place (Shuffle L: the ones after it move left;
/// Shuffle R: the ones before it move right).
pub(crate) fn close_deleted(p: &mut Project, sel: &[(usize, usize)], mode: DragMode) {
    let right = match mode {
        DragMode::ShuffleL => false,
        DragMode::ShuffleR => true,
        _ => return,
    };
    let left: Vec<(usize, f64, f64)> = sel
        .iter()
        .filter(|&&(t, r)| p.tracks.get(t).is_some_and(|x| r < x.regions.len()))
        .map(|&(t, r)| {
            let (s, e) = span(p, t, r);
            (t, s, e - s)
        })
        .collect();
    close_gaps(p, &left, sel, right);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marquee::rig::{midi, Rig};
    use crate::model::Project;
    use crate::undo::History;
    use egui::{Key, Modifiers};

    /// One instrument track with "A" bars 1-2, "B" bars 2-3 and "C" bars 5-6 (4/4).
    fn project() -> Project {
        let mut p = Project::empty();
        p.tracks[0].regions = vec![
            midi("A", 1.0, 1.0),
            midi("B", 2.0, 1.0),
            midi("C", 5.0, 1.0),
        ];
        p.selected_track = 0;
        p
    }

    /// Each region of track `t` by name, as (name, start bar, end bar), earliest first.
    fn places(p: &Project, t: usize) -> Vec<(String, f32, f32)> {
        let mut out: Vec<(String, f32, f32)> = p.tracks[t]
            .regions
            .iter()
            .map(|r| (r.name.clone(), r.start_bar, r.start_bar + r.len_bars))
            .collect();
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out
    }

    fn at(v: &[(&str, f32, f32)]) -> Vec<(String, f32, f32)> {
        v.iter().map(|&(n, s, e)| (n.to_string(), s, e)).collect()
    }

    /// Drops region `r` of track 0 `delta` quarter notes on.
    fn drop_one(p: &mut Project, r: usize, delta: f64, mode: DragMode) {
        let s0 = p.region_start_beats(&p.tracks[0].regions[r]);
        let auto = crate::automation::AutoView::default();
        drop(p, &auto, &[(0, r, s0)], delta, 0, false, mode);
    }

    #[test]
    fn no_overlap_shortens_the_earlier_region() {
        // C dropped at bar 2.5: B (bars 2-3) ends at 2.5.
        let mut p = project();
        drop_one(&mut p, 2, -10.0, DragMode::NoOverlap);
        assert_eq!(
            places(&p, 0),
            at(&[("A", 1.0, 2.0), ("B", 2.0, 2.5), ("C", 2.5, 3.5)])
        );
        // A dropped at bar 1.5: it is the earlier one, so it ends where B begins.
        let mut p = project();
        drop_one(&mut p, 0, 2.0, DragMode::NoOverlap);
        assert_eq!(
            places(&p, 0),
            at(&[("A", 1.5, 2.0), ("B", 2.0, 3.0), ("C", 5.0, 6.0)])
        );
        // Overlap leaves both.
        let mut p = project();
        drop_one(&mut p, 2, -10.0, DragMode::Overlap);
        assert_eq!(
            places(&p, 0),
            at(&[("A", 1.0, 2.0), ("B", 2.0, 3.0), ("C", 2.5, 3.5)])
        );
    }

    #[test]
    fn shuffle_l_closes_up_to_the_left_and_swaps_regions_dragged_over_each_other() {
        // C dropped in the gap at bar 4: it starts where B ends.
        let mut p = project();
        drop_one(&mut p, 2, -4.0, DragMode::ShuffleL);
        assert_eq!(
            places(&p, 0),
            at(&[("A", 1.0, 2.0), ("B", 2.0, 3.0), ("C", 3.0, 4.0)])
        );
        // B dragged onto the first half of A: they swap.
        let mut p = project();
        drop_one(&mut p, 1, -3.0, DragMode::ShuffleL);
        assert_eq!(
            places(&p, 0),
            at(&[("B", 1.0, 2.0), ("A", 2.0, 3.0), ("C", 5.0, 6.0)])
        );
        // A moved away to bar 9: B and C move left by its length; A goes after C.
        let mut p = project();
        drop_one(&mut p, 0, 32.0, DragMode::ShuffleL);
        assert_eq!(
            places(&p, 0),
            at(&[("B", 1.0, 2.0), ("C", 4.0, 5.0), ("A", 5.0, 6.0)])
        );
    }

    #[test]
    fn shuffle_r_closes_up_to_the_right() {
        // A dropped in the gap at bar 3.5: it ends where C starts, and B (before it) moves
        // left by its length (to the song's start).
        let mut p = project();
        drop_one(&mut p, 0, 10.0, DragMode::ShuffleR);
        assert_eq!(
            places(&p, 0),
            at(&[("B", 1.0, 2.0), ("A", 4.0, 5.0), ("C", 5.0, 6.0)])
        );
        // B dragged so that its end is on the second half of C: they swap.
        let mut p = project();
        drop_one(&mut p, 1, 12.0, DragMode::ShuffleR);
        assert_eq!(
            places(&p, 0),
            at(&[("A", 1.0, 2.0), ("C", 4.0, 5.0), ("B", 5.0, 6.0)])
        );
    }

    #[test]
    fn delete_in_shuffle_l_moves_the_following_regions_left() {
        let mut p = project();
        close_deleted(&mut p, &[(0, 0)], DragMode::ShuffleL);
        p.tracks[0].regions.remove(0);
        assert_eq!(places(&p, 0), at(&[("B", 1.0, 2.0), ("C", 4.0, 5.0)]));
        let mut p = project();
        close_deleted(&mut p, &[(0, 2)], DragMode::ShuffleR);
        p.tracks[0].regions.remove(2);
        assert_eq!(places(&p, 0), at(&[("A", 2.0, 3.0), ("B", 3.0, 4.0)]));
        let mut p = project();
        close_deleted(&mut p, &[(0, 0)], DragMode::Overlap);
        assert_eq!(places(&p, 0)[1], ("B".to_string(), 2.0, 3.0));
    }

    #[test]
    fn the_drag_menu_mode_applies_to_a_pointer_drag_and_delete_in_one_step() {
        let mut r = Rig::new(project());
        r.s.tracks.drag_mode = DragMode::ShuffleL;
        let mut hist = History::new(&r.p);
        // Drag C (bar 5, beat 16) into the gap before it: it closes up to B.
        let (from, to) = (r.at(17.0, 0), r.at(13.0, 0));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r.p, 0),
            at(&[("A", 1.0, 2.0), ("B", 2.0, 3.0), ("C", 3.0, 4.0)])
        );
        assert!(hist.record(&r.p));
        assert!(hist.undo(&mut r.p));
        assert!(!hist.can_undo(), "one step");
        assert_eq!(places(&r.p, 0)[2], ("C".to_string(), 5.0, 6.0));
        // Delete A: B and C close up.
        r.s.tracks.select_only(0, 0);
        r.tap(Key::Delete, Modifiers::NONE);
        assert_eq!(places(&r.p, 0), at(&[("B", 1.0, 2.0), ("C", 4.0, 5.0)]));
        // No Overlap through the toolbar's mode.
        r.s.tracks.drag_mode = DragMode::NoOverlap;
        // (The rig's drag starts a quarter of the way along, where the region is grabbed: it
        // lands with its start at beat 2.)
        let (from, to) = (r.at(13.5, 0), r.at(0.5, 0));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(places(&r.p, 0), at(&[("B", 1.0, 1.5), ("C", 1.5, 2.5)]));
    }
}
