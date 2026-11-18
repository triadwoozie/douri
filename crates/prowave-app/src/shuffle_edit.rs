//! Shuffle R and Shuffle L (the Drag pop-up of the Tracks area, `drag_modes.rs`) beyond a
//! Pointer-tool drop and Delete, as Logic Pro's manual gives them ("Use drag modes"):
//!
//! - **Resize, Shuffle L:** when the right edge of a region is resized, the regions that follow
//!   it on its track move by the change in length.
//! - **Resize, Shuffle R:** when the left edge of a region is resized, the regions before it
//!   move by the change in length. Should that take them past the start of the project, they
//!   stop there and the resized region overlaps them by the rest.
//! - **Delete:** the remaining regions move by the length of what was deleted. Besides Delete
//!   of selected regions (`drag_modes::close_deleted`), this holds for the Eraser tool and for
//!   Delete (or Cut) of a marquee's contents: each piece taken out counts as a region deleted.
//! - **Nudge** (Option-Left, Option-Right) is a move, so it follows the same rules as a drop
//!   in the Drag mode chosen (No Overlap too). Logic's manual does not describe Nudge in these
//!   modes; treating it as a move is this program's reading.
//!
//! Resizing the other edge (the left one in Shuffle L, the right one in Shuffle R) is not
//! described in Logic's manual and is left as a plain resize.
//!
//! Also here, whatever the Drag mode: Edit > Shuffle Left within Selection (Option-[) and
//! Shuffle Right within Selection (Option-]), Logic's Edit > Move commands of those names
//! ("Remove gaps between regions"): on each track, Shuffle Left leaves the first selected
//! region where it is and moves each later selected one to start where the selected one before
//! it ends; Shuffle Right leaves the last one where it is and moves each earlier one to end
//! where the selected one after it starts.
//! Regions that are not selected do not move and do not count.

use crate::drag_modes::{set_start, span};
use crate::model::Project;
use crate::tracks_area::{DragMode, TracksView};

const EPS: f64 = 1e-6;

/// Every region's (start, end) in quarter notes, track by track.
fn spans(p: &Project) -> Vec<Vec<(f64, f64)>> {
    (0..p.tracks.len())
        .map(|t| {
            (0..p.tracks[t].regions.len())
                .map(|r| span(p, t, r))
                .collect()
        })
        .collect()
}

/// Whether the project still has the regions `before` was taken of (a resize does not add or
/// remove any; should one have, nothing is moved).
fn same_regions(p: &Project, before: &[Vec<(f64, f64)>]) -> bool {
    before.len() == p.tracks.len()
        && before
            .iter()
            .zip(&p.tracks)
            .all(|(b, t)| b.len() == t.regions.len())
}

/// Shuffle L after the right edges of some regions were resized: on each track, every region
/// that started at or after a resized region's old end moves by how much that end moved.
fn follow_ends(p: &mut Project, before: &[Vec<(f64, f64)>]) {
    for (t, old) in before.iter().enumerate() {
        let moved: Vec<(f64, f64)> = old
            .iter()
            .enumerate()
            .filter_map(|(r, &(_, e))| {
                let d = span(p, t, r).1 - e;
                (d.abs() > EPS).then_some((e, d))
            })
            .collect();
        if moved.is_empty() {
            continue;
        }
        for (r, &(s, _)) in old.iter().enumerate() {
            let by: f64 = moved
                .iter()
                .filter(|&&(e, _)| s >= e - EPS)
                .map(|&(_, d)| d)
                .sum();
            if by.abs() > EPS {
                let now = span(p, t, r).0;
                set_start(p, t, r, now + by);
            }
        }
    }
}

/// Shuffle R after the left edges of some regions were resized: on each track, every region
/// that ended at or before a resized region's old start moves by how much that start moved,
/// no further left than the start of the project (the regions that would go past it stop
/// there together, and the resized region overlaps them).
fn follow_starts(p: &mut Project, before: &[Vec<(f64, f64)>]) {
    for (t, old) in before.iter().enumerate() {
        let moved: Vec<(f64, f64)> = old
            .iter()
            .enumerate()
            .filter_map(|(r, &(s, _))| {
                let d = span(p, t, r).0 - s;
                (d.abs() > EPS).then_some((s, d))
            })
            .collect();
        if moved.is_empty() {
            continue;
        }
        let mut by: Vec<f64> = old
            .iter()
            .map(|&(_, e)| {
                moved
                    .iter()
                    .filter(|&&(s, _)| e <= s + EPS)
                    .map(|&(_, d)| d)
                    .sum()
            })
            .collect();
        // How far past the start of the project the furthest one would go: the ones moving
        // left all go that much less far.
        let past = old
            .iter()
            .zip(&by)
            .enumerate()
            .filter(|(_, (_, &b))| b < -EPS)
            .map(|(r, (_, &b))| -(span(p, t, r).0 + b))
            .fold(0.0_f64, f64::max);
        if past > EPS {
            for b in by.iter_mut().filter(|b| **b < -EPS) {
                *b = (*b + past).min(0.0);
            }
        }
        for (r, b) in by.into_iter().enumerate() {
            if b.abs() > EPS {
                let now = span(p, t, r).0;
                set_start(p, t, r, now + b);
            }
        }
    }
}

/// The lower-right corner of region `clicked` let go at `end`: `region_commands::resize_ends`,
/// and in Shuffle L the regions after each resized one follow its end.
pub(crate) fn resize_ends(
    p: &mut Project,
    v: &TracksView,
    clicked: (usize, usize),
    end: f64,
    stretch: bool,
) -> bool {
    let before = (v.drag_mode == DragMode::ShuffleL).then(|| spans(p));
    let ok = crate::region_commands::resize_ends(p, v, clicked, end, stretch);
    if let Some(before) = before.filter(|b| same_regions(p, b)) {
        follow_ends(p, &before);
        p.song_rev += 1;
    }
    ok
}

/// The lower-left corner of region `clicked` let go at `at`:
/// `region_commands::resize_starts`, and in Shuffle R the regions before each resized one
/// follow its start.
pub(crate) fn resize_starts(p: &mut Project, v: &TracksView, clicked: (usize, usize), at: f64) {
    let before = (v.drag_mode == DragMode::ShuffleR).then(|| spans(p));
    crate::region_commands::resize_starts(p, v, clicked, at);
    if let Some(before) = before.filter(|b| same_regions(p, b)) {
        follow_starts(p, &before);
        p.song_rev += 1;
    }
}

/// Nudge Left / Right by `beats` quarter notes in the Drag mode of the Tracks area: the
/// selected regions move together as a Pointer drop of them would (`drag_modes::drop`).
pub(crate) fn nudge(
    p: &mut Project,
    v: &TracksView,
    auto: &crate::automation::AutoView,
    beats: f64,
) {
    let group: Vec<(usize, usize, f64)> = v
        .selection()
        .into_iter()
        .filter_map(|(t, r)| {
            let reg = p.tracks.get(t)?.regions.get(r)?;
            Some((t, r, p.region_start_beats(reg)))
        })
        .collect();
    if group.is_empty() || crate::region_edit::clamp_delta(&group, beats).abs() < 1e-9 {
        return;
    }
    crate::drag_modes::drop(p, auto, &group, beats, 0, false, v.drag_mode);
}

/// The selected regions of each track that has two or more, earliest first.
fn selected_runs(p: &Project, v: &TracksView) -> Vec<(usize, Vec<usize>)> {
    let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
    for (t, r) in v.selection() {
        if p.tracks.get(t).is_none_or(|x| r >= x.regions.len()) {
            continue;
        }
        match out.iter_mut().find(|(ot, _)| *ot == t) {
            Some((_, rs)) => rs.push(r),
            None => out.push((t, vec![r])),
        }
    }
    out.retain(|(_, rs)| rs.len() > 1);
    for (t, rs) in &mut out {
        rs.sort_by(|&a, &b| span(p, *t, a).0.total_cmp(&span(p, *t, b).0));
    }
    out
}

/// Whether Shuffle Left / Right would move anything: some track has two selected regions with
/// a gap (or an overlap) between them.
pub(crate) fn can_shuffle(p: &Project, v: &TracksView) -> bool {
    selected_runs(p, v).iter().any(|(t, rs)| {
        rs.windows(2)
            .any(|w| (span(p, *t, w[1]).0 - span(p, *t, w[0]).1).abs() > EPS)
    })
}

/// Edit > Shuffle Left (`right` false) or Shuffle Right: the selected regions of each track
/// close up to the first (or the last) of them, in one step.
pub(crate) fn shuffle_selected(p: &mut Project, v: &TracksView, right: bool) {
    let runs = selected_runs(p, v);
    if runs.is_empty() {
        return;
    }
    for (t, rs) in runs {
        if right {
            for k in (0..rs.len() - 1).rev() {
                let next_start = span(p, t, rs[k + 1]).0;
                let (s, e) = span(p, t, rs[k]);
                set_start(p, t, rs[k], next_start - (e - s));
            }
        } else {
            for k in 1..rs.len() {
                let prev_end = span(p, t, rs[k - 1]).1;
                set_start(p, t, rs[k], prev_end);
            }
        }
    }
    p.song_rev += 1;
}

/// The Eraser tool on region `r` of track `t`: it is removed, and in a Shuffle mode the others
/// close up as for Delete.
pub(crate) fn erase(p: &mut Project, v: &mut TracksView, (t, r): (usize, usize)) {
    if p.tracks.get(t).is_none_or(|x| r >= x.regions.len()) {
        return;
    }
    crate::drag_modes::close_deleted(p, &[(t, r)], v.drag_mode);
    p.tracks[t].regions.remove(r);
    v.clear_selection();
    p.song_rev += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marquee::rig::{midi, Rig};
    use crate::tracks_area::TrackTool;
    use crate::undo::History;
    use egui::{vec2, Key, Modifiers, Pos2};

    /// One instrument track: "A" bars 2-3, "B" bars 3-4, "C" bars 6-7 (4/4), and a second
    /// track with "D" bars 3-4, which no edit on the first one moves.
    fn rig() -> Rig {
        let mut p = Project::empty();
        p.add_track(crate::model::TrackKind::Instrument);
        p.tracks[0].regions = vec![
            midi("A", 2.0, 1.0),
            midi("B", 3.0, 1.0),
            midi("C", 6.0, 1.0),
        ];
        p.tracks[1].regions = vec![midi("D", 3.0, 1.0)];
        p.selected_track = 0;
        p.position_beats = 0.0;
        Rig::new(p)
    }

    /// Each region of track `t` by name, as (name, start bar, end bar), earliest first.
    fn places(r: &Rig, t: usize) -> Vec<(String, f32, f32)> {
        let mut out: Vec<(String, f32, f32)> = r.p.tracks[t]
            .regions
            .iter()
            .map(|x| (x.name.clone(), x.start_bar, x.start_bar + x.len_bars))
            .collect();
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out
    }

    fn at(v: &[(&str, f32, f32)]) -> Vec<(String, f32, f32)> {
        v.iter().map(|&(n, s, e)| (n.to_string(), s, e)).collect()
    }

    /// The lower half of track `t`'s row, just inside the edge at `beat` (quarter notes): the
    /// grip of a region's right end (`right`) or left end.
    fn grip(r: &Rig, beat: f32, t: usize, right: bool) -> Pos2 {
        r.at(beat, t) + vec2(if right { -2.0 } else { 2.0 }, 20.0)
    }

    /// Records the edit just made as one undo step, undoes it and checks there was only one.
    fn one_step(r: &mut Rig, hist: &mut History, before: Vec<(String, f32, f32)>) {
        assert!(hist.record(&r.p));
        assert!(hist.undo(&mut r.p));
        assert!(!hist.can_undo(), "one step");
        assert_eq!(places(r, 0), before);
    }

    #[test]
    fn shuffle_l_resizing_a_right_edge_moves_the_following_regions_by_the_change() {
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleL;
        let mut hist = History::new(&r.p);
        let before = places(&r, 0);
        // A's end (beat 8) a bar later, to beat 12: B and C move a bar later.
        let (from, to) = (grip(&r, 8.0, 0, true), grip(&r, 12.0, 0, true));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 4.0), ("B", 4.0, 5.0), ("C", 7.0, 8.0)])
        );
        assert_eq!(places(&r, 1), at(&[("D", 3.0, 4.0)]), "another track");
        one_step(&mut r, &mut hist, before);
        // Shorter by half a bar: the ones after it move half a bar earlier.
        let (from, to) = (grip(&r, 8.0, 0, true), grip(&r, 6.0, 0, true));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 2.5), ("B", 2.5, 3.5), ("C", 5.5, 6.5)])
        );
        // The left edge in Shuffle L is a plain resize (Logic's manual gives no rule for it).
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleL;
        let (from, to) = (grip(&r, 20.0, 0, false), grip(&r, 22.0, 0, false));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 3.0), ("B", 3.0, 4.0), ("C", 6.5, 7.0)])
        );
    }

    #[test]
    fn shuffle_r_resizing_a_left_edge_moves_the_regions_before_it_no_further_than_the_start() {
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleR;
        let mut hist = History::new(&r.p);
        let before = places(&r, 0);
        // C's start (beat 20) two beats earlier: A and B move two beats earlier.
        let (from, to) = (grip(&r, 20.0, 0, false), grip(&r, 18.0, 0, false));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 1.5, 2.5), ("B", 2.5, 3.5), ("C", 5.5, 7.0)])
        );
        assert_eq!(places(&r, 1), at(&[("D", 3.0, 4.0)]), "another track");
        one_step(&mut r, &mut hist, before);
        // B's start two bars earlier: A can go only one bar, to the start of the project, and
        // B overlaps it by the other bar.
        let (from, to) = (grip(&r, 8.0, 0, false), grip(&r, 0.0, 0, false));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 1.0, 2.0), ("B", 1.0, 4.0), ("C", 6.0, 7.0)])
        );
        // Shorter at the start: the ones before it move later, so they stay butted.
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleR;
        let (from, to) = (grip(&r, 8.0, 0, false), grip(&r, 10.0, 0, false));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.5, 3.5), ("B", 3.5, 4.0), ("C", 6.0, 7.0)])
        );
        // In Overlap nothing else moves.
        let mut r = rig();
        let (from, to) = (grip(&r, 20.0, 0, false), grip(&r, 18.0, 0, false));
        r.drag(from, to, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 3.0), ("B", 3.0, 4.0), ("C", 5.5, 7.0)])
        );
    }

    #[test]
    fn nudge_in_a_shuffle_mode_moves_as_a_drop_does() {
        // Shuffle L: C nudged a bar earlier (Option-Left, nudge value Bar) lands in the gap
        // and closes up to B.
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleL;
        r.s.tracks.nudge = crate::region_commands::NudgeValue::Bar;
        r.s.tracks.select_only(0, 2);
        let mut hist = History::new(&r.p);
        let before = places(&r, 0);
        r.tap(Key::ArrowLeft, Modifiers::ALT);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 3.0), ("B", 3.0, 4.0), ("C", 4.0, 5.0)])
        );
        one_step(&mut r, &mut hist, before);
        // B nudged a bar later lands in the gap, so it closes up to A again: nothing moves.
        r.s.tracks.select_only(0, 1);
        r.tap(Key::ArrowRight, Modifiers::ALT);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 3.0), ("B", 3.0, 4.0), ("C", 6.0, 7.0)])
        );
        // Shuffle R: A nudged a bar later goes onto B's end half and swaps with it.
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleR;
        r.s.tracks.nudge = crate::region_commands::NudgeValue::Bar;
        r.s.tracks.select_only(0, 0);
        r.tap(Key::ArrowRight, Modifiers::ALT);
        assert_eq!(
            places(&r, 0),
            at(&[("B", 2.0, 3.0), ("A", 3.0, 4.0), ("C", 6.0, 7.0)])
        );
        // Overlap: a plain nudge.
        let mut r = rig();
        r.s.tracks.nudge = crate::region_commands::NudgeValue::Bar;
        r.s.tracks.select_only(0, 2);
        r.tap(Key::ArrowLeft, Modifiers::ALT);
        assert_eq!(places(&r, 0)[2], ("C".to_string(), 5.0, 6.0));
    }

    #[test]
    fn the_eraser_in_shuffle_l_closes_the_gap() {
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleL;
        r.s.tracks.tool = TrackTool::Eraser;
        let mut hist = History::new(&r.p);
        let before = places(&r, 0);
        let pt = r.at(5.0, 0);
        r.click(pt, Modifiers::NONE);
        assert_eq!(places(&r, 0), at(&[("B", 2.0, 3.0), ("C", 5.0, 6.0)]));
        one_step(&mut r, &mut hist, before);
        // Shuffle R: the ones before it move later.
        r.s.tracks.drag_mode = DragMode::ShuffleR;
        let pt = r.at(9.0, 0);
        r.click(pt, Modifiers::NONE);
        assert_eq!(places(&r, 0), at(&[("A", 3.0, 4.0), ("C", 6.0, 7.0)]));
    }

    #[test]
    fn deleting_a_marquee_in_a_shuffle_mode_closes_the_gap_it_leaves() {
        // A marquee from beat 6 to beat 10 over both tracks: half of A, half of B and half of
        // D go.
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleL;
        r.s.tracks.tool = TrackTool::Marquee;
        let mut hist = History::new(&r.p);
        let before = places(&r, 0);
        let (from, to) = (r.at(6.0, 0), r.at(10.0, 1));
        r.drag(from, to, Modifiers::NONE);
        r.tap(Key::Delete, Modifiers::NONE);
        // Track 0: A's first half, B's second half closed up to it, C a bar earlier.
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 2.5), ("B", 2.5, 3.0), ("C", 5.0, 6.0)])
        );
        assert_eq!(places(&r, 1), at(&[("D", 3.0, 3.5)]), "its own gap closes");
        one_step(&mut r, &mut hist, before);
        // Shuffle R: what was before the marquee moves later by what went.
        let mut r = rig();
        r.s.tracks.drag_mode = DragMode::ShuffleR;
        r.s.tracks.tool = TrackTool::Marquee;
        let (from, to) = (r.at(6.0, 0), r.at(10.0, 0));
        r.drag(from, to, Modifiers::NONE);
        r.tap(Key::Delete, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 3.0, 3.5), ("B", 3.5, 4.0), ("C", 6.0, 7.0)])
        );
        // Overlap: the gap stays.
        let mut r = rig();
        r.s.tracks.tool = TrackTool::Marquee;
        let (from, to) = (r.at(6.0, 0), r.at(10.0, 0));
        r.drag(from, to, Modifiers::NONE);
        r.tap(Key::Delete, Modifiers::NONE);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 2.5), ("B", 3.5, 4.0), ("C", 6.0, 7.0)])
        );
    }

    #[test]
    fn option_brackets_shuffle_the_selected_regions_left_and_right_in_one_step() {
        let mut r = rig();
        // A, C and D selected; B is not.
        r.s.tracks.select_many(&[(0, 0), (0, 2), (1, 0)]);
        assert!(crate::menus::enabled(
            "Shuffle Left within Selection",
            &r.p,
            &r.s
        ));
        assert!(crate::menus::enabled(
            "Shuffle Right within Selection",
            &r.p,
            &r.s
        ));
        let mut hist = History::new(&r.p);
        let before = places(&r, 0);
        // Option-[: C starts where A ends (over B, which stays); D is alone on its track.
        r.tap(Key::OpenBracket, Modifiers::ALT);
        assert_eq!(
            places(&r, 0),
            at(&[("A", 2.0, 3.0), ("B", 3.0, 4.0), ("C", 3.0, 4.0)])
        );
        assert_eq!(places(&r, 1), at(&[("D", 3.0, 4.0)]));
        one_step(&mut r, &mut hist, before);
        // Option-]: A ends where C starts.
        r.tap(Key::CloseBracket, Modifiers::ALT);
        assert_eq!(
            places(&r, 0),
            at(&[("B", 3.0, 4.0), ("A", 5.0, 6.0), ("C", 6.0, 7.0)])
        );
        // Butted now: nothing left to shuffle, so the entries are off.
        assert!(!crate::menus::enabled(
            "Shuffle Right within Selection",
            &r.p,
            &r.s
        ));
        assert!(!crate::menus::enabled(
            "Shuffle Left within Selection",
            &r.p,
            &r.s
        ));
        // One region selected on a track: off; the menu path does the same as the key.
        r.s.tracks.select_only(0, 2);
        assert!(!crate::menus::enabled(
            "Shuffle Left within Selection",
            &r.p,
            &r.s
        ));
        r.s.tracks.select_many(&[(0, 0), (0, 1)]);
        crate::menus::apply("Shuffle Left within Selection", &mut r.p, &mut r.s);
        assert_eq!(
            places(&r, 0),
            at(&[("B", 3.0, 4.0), ("A", 4.0, 5.0), ("C", 6.0, 7.0)])
        );
    }
}
