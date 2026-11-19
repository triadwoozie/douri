//! The Marquee tool of the Tracks area (Logic Pro: Marquee tool, R in the Tool menu, and
//! Logic's default Command-click tool): a drag in the lanes selects a stretch of time on one
//! or more neighbouring tracks, on the Snap grid. What Logic does with it, and what is done
//! here:
//! - Delete removes the material inside it; regions that cross its edges are cut there first.
//! - Edit > Cut and Copy cut or copy that material (the pieces inside, as regions); Paste puts
//!   them at the playhead as it pastes regions.
//! - Split at Playhead (Command-T) with a marquee cuts the regions at both of its edges, and the
//!   pieces inside become the selected regions.
//! - Shift with the Marquee tool moves the nearer edge of the marquee (and takes in more
//!   tracks) instead of starting a new one; a click elsewhere in the lanes clears it.
//!
//! Regions that cannot be cut (take folders, Pattern and Drummer regions, reversed audio) are
//! left alone where they cross an edge; ones wholly inside are removed, copied or selected.

use crate::model::{Project, Region};
use crate::tracks_area::TracksView;

/// A marquee selection: from `start` to `end` (quarter notes, `start < end`) on the tracks
/// `first..=last`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Range {
    pub start: f64,
    pub end: f64,
    pub first: usize,
    pub last: usize,
}

impl Range {
    /// The range between two corners: (time, track) where a drag began and where it is now.
    pub fn between(a: (f64, usize), b: (f64, usize)) -> Self {
        Self {
            start: a.0.min(b.0),
            end: a.0.max(b.0),
            first: a.1.min(b.1),
            last: a.1.max(b.1),
        }
    }

    /// Whether it covers any time at all.
    pub fn is_empty(&self) -> bool {
        self.end - self.start < 1e-9
    }

    /// The corner a Shift-drag or Shift-click at (`beat`, `track`) keeps: the edge further
    /// from the pointer, so the nearer one follows it, and the end track further from it.
    pub fn far_corner(&self, beat: f64, track: usize) -> (f64, usize) {
        let t = if track.abs_diff(self.first) >= track.abs_diff(self.last) {
            self.first
        } else {
            self.last
        };
        let b = if (beat - self.start).abs() > (beat - self.end).abs() {
            self.start
        } else {
            self.end
        };
        (b, t)
    }

    /// The tracks it covers that have a row on screen: a track hidden in a folded stack
    /// lies between its first and last track but is not in it (nothing of the marquee is drawn there).
    fn tracks(&self, p: &Project) -> Vec<usize> {
        (self.first.min(p.tracks.len())..(self.last + 1).min(p.tracks.len()))
            .filter(|&t| !p.hidden_by_fold(t))
            .collect()
    }
}

/// The marquee and the drag that is making it.
#[derive(Default)]
pub struct State {
    pub range: Option<Range>,
    /// While a Marquee drag is in progress: the corner it is anchored at.
    pub anchor: Option<(f64, usize)>,
    /// A Shift-drag: the tracks the marquee had, which it keeps (it only takes in more).
    keep: Option<(usize, usize)>,
}

impl State {
    /// A Marquee drag (or a click) begins at `at`; with `adjust` (Shift) an existing marquee
    /// keeps its far corner and the near one follows the pointer.
    pub fn begin(&mut self, at: (f64, usize), adjust: bool) {
        let (anchor, keep) = match (adjust, self.range) {
            (true, Some(r)) => (r.far_corner(at.0, at.1), Some((r.first, r.last))),
            _ => (at, None),
        };
        self.anchor = Some(anchor);
        self.keep = keep;
        self.drag_to(at);
    }

    /// The pointer of a Marquee drag is at `at`.
    pub fn drag_to(&mut self, at: (f64, usize)) {
        if let Some(a) = self.anchor {
            let mut r = Range::between(a, at);
            if let Some((first, last)) = self.keep {
                r.first = r.first.min(first);
                r.last = r.last.max(last);
            }
            self.range = Some(r);
        }
    }

    /// The button is let go: an empty marquee (a click) is none.
    pub fn end(&mut self) {
        self.anchor = None;
        self.keep = None;
        if self.range.is_some_and(|r| r.is_empty()) {
            self.range = None;
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The marquee when it covers some time.
    pub fn active(&self) -> Option<Range> {
        self.range.filter(|r| !r.is_empty())
    }
}

const EPS: f64 = 1e-6;

/// The indexes of the regions of track `t` that lie wholly inside `a..b`.
fn inside(p: &Project, t: usize, a: f64, b: f64) -> Vec<usize> {
    p.tracks[t]
        .regions
        .iter()
        .enumerate()
        .filter(|(_, r)| p.region_start_beats(r) >= a - EPS && p.region_end_beats(r) <= b + EPS)
        .map(|(i, _)| i)
        .collect()
}

/// Cuts every region of track `t` that crosses `a` or `b` there (where it can be cut).
fn cut_at_edges(p: &mut Project, t: usize, a: f64, b: f64) {
    // Highest index first: a cut puts its right piece right after the left one, so the
    // regions still to look at keep their indexes.
    for ri in (0..p.tracks[t].regions.len()).rev() {
        if p.can_split_region(t, ri, b) {
            p.split_region(t, ri, b);
        }
        if p.can_split_region(t, ri, a) {
            p.split_region(t, ri, a);
        }
    }
}

/// Whether the marquee holds any material: a region inside it, or one crossing it.
pub fn has_material(p: &Project, v: &TracksView) -> bool {
    let Some(r) = v.range.active() else {
        return false;
    };
    r.tracks(p).into_iter().any(|t| {
        p.tracks[t]
            .regions
            .iter()
            .any(|x| p.region_start_beats(x) < r.end - EPS && p.region_end_beats(x) > r.start + EPS)
    })
}

/// Delete with a marquee: removes the material inside it, cutting the regions that cross its
/// edges there. The marquee stays; no region is selected after. Returns whether anything
/// changed.
pub fn delete(p: &mut Project, v: &mut TracksView) -> bool {
    let Some(r) = v.range.active() else {
        return false;
    };
    let mut changed = false;
    for t in r.tracks(p) {
        cut_at_edges(p, t, r.start, r.end);
        let gone = inside(p, t, r.start, r.end);
        for ri in gone.into_iter().rev() {
            p.tracks[t].regions.remove(ri);
            changed = true;
        }
    }
    v.clear_selection();
    p.song_rev += 1;
    changed
}

/// The material inside the marquee as regions (the pieces of the ones crossing its edges),
/// earliest first, without changing the project.
pub fn pieces(p: &Project, v: &TracksView) -> Vec<Region> {
    let Some(r) = v.range.active() else {
        return Vec::new();
    };
    let mut scratch = p.clone();
    let mut out = Vec::new();
    for t in r.tracks(&scratch) {
        cut_at_edges(&mut scratch, t, r.start, r.end);
        out.extend(
            inside(&scratch, t, r.start, r.end)
                .into_iter()
                .map(|ri| scratch.tracks[t].regions[ri].clone()),
        );
    }
    out.sort_by(|a, b| a.start_bar.total_cmp(&b.start_bar));
    out
}

/// Edit > Copy with a marquee: the material inside it goes to the clipboard. Returns whether
/// there was any.
pub fn copy(p: &Project, v: &TracksView, clip: &mut crate::region_edit::RegionClip) -> bool {
    let got = pieces(p, v);
    if got.is_empty() {
        return false;
    }
    *clip = got;
    true
}

/// Edit > Cut with a marquee: Copy, then Delete.
pub fn cut(p: &mut Project, v: &mut TracksView, clip: &mut crate::region_edit::RegionClip) {
    if copy(p, v, clip) {
        delete(p, v);
    }
}

/// Whether Split at Playhead with this marquee cuts anything.
pub fn can_split(p: &Project, v: &TracksView) -> bool {
    let Some(r) = v.range.active() else {
        return false;
    };
    r.tracks(p).into_iter().any(|t| {
        (0..p.tracks[t].regions.len())
            .any(|ri| p.can_split_region(t, ri, r.start) || p.can_split_region(t, ri, r.end))
    })
}

/// Split at Playhead with a marquee: the regions on its tracks are cut at both of its edges,
/// and the pieces inside it are selected. Returns whether anything was cut.
pub fn split(p: &mut Project, v: &mut TracksView) -> bool {
    let Some(r) = v.range.active() else {
        return false;
    };
    if !can_split(p, v) {
        return false;
    }
    let mut picked = Vec::new();
    for t in r.tracks(p) {
        cut_at_edges(p, t, r.start, r.end);
        picked.extend(inside(p, t, r.start, r.end).into_iter().map(|ri| (t, ri)));
    }
    v.select_many(&picked);
    true
}

/// Split at Playhead: at the marquee's edges when there is one, else at the playhead.
pub fn can_split_any(p: &Project, v: &TracksView) -> bool {
    if v.range.active().is_some() {
        can_split(p, v)
    } else {
        crate::region_edit::can_split(p, v)
    }
}

/// Runs Split at Playhead (see [`can_split_any`]).
pub fn split_any(p: &mut Project, v: &mut TracksView) {
    if v.range.active().is_some() {
        split(p, v);
    } else {
        crate::region_edit::split_at_playhead(p, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Note, RegionParams, TrackKind};

    fn note(start: f32, len: f32) -> Note {
        Note {
            start,
            len,
            pitch: 60,
            vel: 100,
        }
    }

    fn region(name: &str, start_bar: f32, len_bars: f32) -> Region {
        Region {
            name: name.into(),
            start_bar,
            len_bars,
            notes: (0..(len_bars as usize * 4))
                .map(|k| note(k as f32, 0.5))
                .collect(),
            controls: Vec::new(),
            folder: None,
            audio: None,
            params: RegionParams::default(),
            generated: None,
        }
    }

    /// Three instrument tracks in 4/4: "A" bars 1-4 on track 0, "B" bars 2-3 on track 1,
    /// "C" bars 1-2 on track 2.
    fn rig() -> (Project, TracksView) {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        p.add_track(TrackKind::Instrument);
        p.tracks[0].regions = vec![region("A", 1.0, 4.0)];
        p.tracks[1].regions = vec![region("B", 2.0, 2.0)];
        p.tracks[2].regions = vec![region("C", 1.0, 2.0)];
        (p, TracksView::default())
    }

    fn spans(p: &Project, t: usize) -> Vec<(f32, f32)> {
        p.tracks[t]
            .regions
            .iter()
            .map(|r| (r.start_bar, r.start_bar + r.len_bars))
            .collect()
    }

    /// Beats 6..10 (bar 2.5 to bar 3.5) on tracks 0 and 1.
    fn marquee(v: &mut TracksView) {
        v.range.begin((6.0, 0), false);
        v.range.drag_to((10.0, 1));
        v.range.end();
    }

    #[test]
    fn delete_removes_the_material_inside_and_cuts_at_the_edges() {
        let (mut p, mut v) = rig();
        marquee(&mut v);
        assert!(delete(&mut p, &mut v));
        assert_eq!(spans(&p, 0), vec![(1.0, 2.5), (3.5, 5.0)]);
        assert_eq!(spans(&p, 1), vec![(2.0, 2.5), (3.5, 4.0)]);
        assert_eq!(spans(&p, 2), vec![(1.0, 3.0)], "track 2 is outside");
        // The notes that played inside are gone; the right piece keeps its notes in place.
        assert_eq!(p.tracks[0].regions[1].notes[0].start, 0.0);
        assert_eq!(p.tracks[0].regions[1].notes.len(), 6);
    }

    #[test]
    fn copy_takes_the_pieces_inside_and_leaves_the_project_alone() {
        let (mut p, mut v) = rig();
        marquee(&mut v);
        let before = spans(&p, 0);
        let mut clip = Vec::new();
        assert!(copy(&p, &v, &mut clip));
        assert_eq!(spans(&p, 0), before);
        let got: Vec<(f32, f32)> = clip
            .iter()
            .map(|r| (r.start_bar, r.start_bar + r.len_bars))
            .collect();
        assert_eq!(got, vec![(2.5, 3.5), (2.5, 3.5)]);
        assert_eq!(clip[0].notes.len(), 4);
        // Cut: the same pieces, and they are gone from the tracks.
        let mut clip2 = Vec::new();
        cut(&mut p, &mut v, &mut clip2);
        assert_eq!(clip2.len(), 2);
        assert_eq!(spans(&p, 0), vec![(1.0, 2.5), (3.5, 5.0)]);
    }

    #[test]
    fn split_cuts_at_both_edges_and_selects_the_inside() {
        let (mut p, mut v) = rig();
        marquee(&mut v);
        assert!(can_split(&p, &v));
        assert!(split(&mut p, &mut v));
        assert_eq!(spans(&p, 0), vec![(1.0, 2.5), (2.5, 3.5), (3.5, 5.0)]);
        assert_eq!(spans(&p, 1), vec![(2.0, 2.5), (2.5, 3.5), (3.5, 4.0)]);
        assert_eq!(v.selection(), vec![(0, 1), (1, 1)]);
    }

    #[test]
    fn shift_moves_the_nearer_edge_and_a_click_leaves_no_marquee() {
        let (_, mut v) = rig();
        marquee(&mut v);
        // Near the right edge, on track 2: the left edge and track 0 stay.
        v.range.begin((11.0, 2), true);
        v.range.end();
        let r = v.range.active().unwrap();
        assert_eq!((r.start, r.end, r.first, r.last), (6.0, 11.0, 0, 2));
        // Near the left edge.
        v.range.begin((4.0, 1), true);
        v.range.end();
        let r = v.range.active().unwrap();
        assert_eq!((r.start, r.end, r.first, r.last), (4.0, 11.0, 0, 2));
        // A click without Shift: nothing.
        v.range.begin((4.0, 1), false);
        v.range.end();
        assert!(v.range.active().is_none());
    }

}
