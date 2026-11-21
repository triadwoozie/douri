//! Logic Pro's selection of several tracks in the Tracks area, and the track commands that act
//! on every selected track.
//!
//! A click on a track header selects that track alone; Command-click adds it or takes it out;
//! Shift-click selects the tracks from the last one clicked to this one. `Project::
//! selected_track` stays the primary (focused) track: the Inspector, the editors and every
//! command that works on one track read it. The others are kept here, in the view state: the
//! selection is not saved and is not an undo step, as in Logic.
//!
//! The set is kept together with the primary track and the number of tracks it was made for.
//! Code that changes `selected_track` on its own (a click on a region, a new track, an undo)
//! or adds or removes a track therefore leaves one track selected, the primary one, and never
//! a set pointing at tracks that have moved.
//!
//! Commands on the set (each one undo step): Delete Track, Hide Track, New Track with Duplicate
//! Settings, Create Track Stack, and Logic's Toggle Track Mute (M) and Toggle Track Solo (S).
use crate::model::{Project, Track, TrackKind};
use crate::tracks_area::TracksView;
use crate::UiState;

/// The selected tracks besides the primary one (see the module notes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackSel {
    /// `Project::selected_track` when the set was made.
    primary: usize,
    /// The number of tracks then.
    count: usize,
    /// The other selected tracks, without the primary one.
    others: Vec<usize>,
    /// The track a Shift-click selects from: the last one clicked without Shift.
    anchor: Option<usize>,
}

impl TrackSel {
    fn valid_for(&self, p: &Project) -> bool {
        self.primary == p.selected_track && self.count == p.tracks.len()
    }
}

/// Every selected track, in track order: the primary one and the others.
pub(crate) fn selected(p: &Project, v: &TracksView) -> Vec<usize> {
    let n = p.tracks.len();
    let mut out: Vec<usize> = Vec::new();
    if p.selected_track < n {
        out.push(p.selected_track);
    }
    if v.track_sel.valid_for(p) {
        out.extend(v.track_sel.others.iter().copied().filter(|&i| i < n));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Whether track `i` is selected.
pub(crate) fn is_selected(p: &Project, v: &TracksView, i: usize) -> bool {
    i == p.selected_track || v.track_sel.valid_for(p) && v.track_sel.others.contains(&i)
}

/// Makes `primary` the primary track and `tracks` (with it) the selected ones.
pub(crate) fn select(p: &mut Project, v: &mut TracksView, primary: usize, tracks: &[usize]) {
    p.selected_track = primary;
    let anchor = v.track_sel.anchor;
    let mut others: Vec<usize> = tracks
        .iter()
        .copied()
        .filter(|&i| i != primary && i < p.tracks.len())
        .collect();
    others.sort_unstable();
    others.dedup();
    v.track_sel = TrackSel {
        primary,
        count: p.tracks.len(),
        others,
        anchor,
    };
}

/// Every region on the selected tracks (Logic: a selected track selects its regions).
pub(crate) fn regions_of(p: &Project, tracks: &[usize]) -> Vec<(usize, usize)> {
    tracks
        .iter()
        .filter_map(|&t| p.tracks.get(t).map(|tr| (t, tr.regions.len())))
        .flat_map(|(t, n)| (0..n).map(move |r| (t, r)))
        .collect()
}

/// How a track header was clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Click {
    /// A plain click: this track alone.
    Plain,
    /// Command-click: this track added to the selection, or taken out of it.
    Toggle,
    /// Shift-click: the tracks from the last one clicked to this one.
    Range,
}

impl Click {
    pub(crate) fn from_mods(m: egui::Modifiers) -> Self {
        if m.shift {
            Click::Range
        } else if m.command {
            Click::Toggle
        } else {
            Click::Plain
        }
    }
}

/// A click on track `i`'s header: changes the selected tracks and selects their regions.
/// `show_hidden`: hidden tracks have a row (Show Hidden Tracks), so a range takes them in.
pub(crate) fn click(p: &mut Project, v: &mut TracksView, i: usize, how: Click, show_hidden: bool) {
    if i >= p.tracks.len() {
        return;
    }
    let mut set = selected(p, v);
    let primary = match how {
        Click::Plain => {
            set = vec![i];
            v.track_sel.anchor = Some(i);
            i
        }
        Click::Toggle => {
            let primary = match set.iter().position(|&t| t == i) {
                // One track is always selected.
                Some(_) if set.len() == 1 => i,
                Some(k) => {
                    set.remove(k);
                    if i == p.selected_track {
                        // The nearest selected track takes the focus.
                        *set.iter()
                            .min_by_key(|&&t| t.abs_diff(i))
                            .unwrap_or(&set[0])
                    } else {
                        p.selected_track
                    }
                }
                None => {
                    set.push(i);
                    i
                }
            };
            // A range goes from the track in focus.
            v.track_sel.anchor = Some(primary);
            primary
        }
        Click::Range => {
            let from = v
                .track_sel
                .anchor
                .filter(|&a| a < p.tracks.len())
                .unwrap_or(p.selected_track.min(p.tracks.len() - 1));
            let (a, b) = (from.min(i), from.max(i));
            set = (a..=b)
                .filter(|&t| t == i || !crate::track_commands::takes_no_room(p, show_hidden, t))
                .collect();
            if v.track_sel.anchor.is_none() {
                v.track_sel.anchor = Some(from);
            }
            i
        }
    };
    select(p, v, primary, &set);
    let regions = regions_of(p, &selected(p, v));
    v.select_many(&regions);
}

/// A right-click on track `i`'s header: a track already selected keeps the others and takes
/// the focus; another one is selected alone (its regions stay as they are, for the menu).
pub(crate) fn context_click(p: &mut Project, v: &mut TracksView, i: usize) {
    if i >= p.tracks.len() {
        return;
    }
    if is_selected(p, v, i) {
        let set = selected(p, v);
        select(p, v, i, &set);
    } else {
        select(p, v, i, &[i]);
        v.track_sel.anchor = Some(i);
    }
}

/// Up or Down with Shift (Add Next/Previous Track to Selection): `t` joins the selected
/// tracks, which were `before`, and becomes the primary one.
pub(crate) fn add_track(p: &mut Project, v: &mut TracksView, before: &[usize], t: usize) {
    let mut set = before.to_vec();
    set.push(t);
    select(p, v, t, &set);
}

// ------------------------------------------------------------------ Delete Track

/// Track > Delete Track (Command-Delete): deletes every selected track, from the last up. A
/// track stack goes as Delete Track takes one (its tracks stay, unless they are selected too).
/// One track always stays. Returns how many went.
pub(crate) fn delete_tracks(p: &mut Project, s: &mut UiState) -> usize {
    let set = selected(p, &s.tracks);
    let mut gone = 0;
    for &i in set.iter().rev() {
        if p.delete_track(i) {
            gone += 1;
        }
    }
    if gone > 0 {
        crate::tracks_area::forget_track_indexes(s);
    }
    gone
}

// ------------------------------------------------------------------ Hide Track

/// Track > Hide Track (Control-H): hides every selected track (or shows them again when the
/// primary one is hidden), then selects the next track below them that still has a row. Their
/// regions leave the selection.
pub(crate) fn hide_tracks(p: &mut Project, s: &mut UiState) {
    let set = selected(p, &s.tracks);
    let Some(&primary) = set.iter().find(|&&i| i == p.selected_track) else {
        return;
    };
    let hide = !p.tracks[primary].hidden;
    for &i in &set {
        p.tracks[i].hidden = hide;
    }
    if !hide {
        return;
    }
    let keep: Vec<(usize, usize)> = s
        .tracks
        .selection()
        .into_iter()
        .filter(|&(ti, _)| !crate::track_commands::hidden(p, ti))
        .collect();
    s.tracks.select_many(&keep);
    let show = s.tracks.show_hidden;
    let last = set
        .iter()
        .map(|&i| p.folder_end(i))
        .max()
        .unwrap_or(primary + 1);
    let first = set[0];
    let next = (last..p.tracks.len())
        .chain((0..first).rev())
        .find(|&j| !crate::track_commands::takes_no_room(p, show, j));
    if let Some(j) = next {
        select(p, &mut s.tracks, j, &[j]);
    }
}

// ------------------------------------------------------------------ Duplicate Settings

/// Whether New Track with Duplicate Settings has a track to copy: a selected track that is not
/// a track stack.
pub(crate) fn can_duplicate(p: &Project, v: &TracksView) -> bool {
    selected(p, v)
        .into_iter()
        .any(|i| p.tracks[i].kind != TrackKind::Folder)
}

/// Track > New Track with Duplicate Settings (Command-D): a copy of each selected track (not of
/// a track stack) right below it, as Logic makes them; the copies are selected, the primary
/// one's copy (or the first) in focus. Returns how many were made.
pub(crate) fn duplicate_tracks(p: &mut Project, s: &mut UiState) -> usize {
    let set = selected(p, &s.tracks);
    let primary = p.selected_track;
    let originals: Vec<usize> = set
        .into_iter()
        .filter(|&i| p.tracks[i].kind != TrackKind::Folder)
        .collect();
    // From the last up, so the tracks still to copy keep their places.
    for &i in originals.iter().rev() {
        crate::track_commands::duplicate_settings_of(p, i);
    }
    // The copy of the k-th original (in track order) is now at its place + k + 1.
    let copies: Vec<usize> = originals
        .iter()
        .enumerate()
        .map(|(k, &i)| i + k + 1)
        .collect();
    if let Some(&first) = copies.first() {
        let focus = originals
            .iter()
            .position(|&i| i == primary)
            .map_or(first, |k| copies[k]);
        select(p, &mut s.tracks, focus, &copies);
        crate::tracks_area::forget_track_indexes(s);
    }
    copies.len()
}

// ------------------------------------------------------------------ Mute and Solo

/// Logic's Toggle Track Mute (M): mutes the selected tracks, or unmutes them when the primary
/// one is muted. A track stack's M reaches the tracks inside it, as its key does; a mix group
/// with Mute takes its tracks along, as a track's M key does.
pub(crate) fn toggle_mute(p: &mut Project, v: &TracksView) {
    let set = selected(p, v);
    let Some(t) = p.tracks.get(p.selected_track) else {
        return;
    };
    let on = !t.mute;
    for i in set {
        if p.tracks[i].kind == TrackKind::Folder {
            p.set_folder_flags(i, Some(on), None);
        } else {
            p.set_mute(i, on);
        }
    }
}

/// Logic's Toggle Track Solo (S): solos the selected tracks, or takes their solo off when the
/// primary one is soloed. Track stacks and groups as [`toggle_mute`].
pub(crate) fn toggle_solo(p: &mut Project, v: &TracksView) {
    let set = selected(p, v);
    let Some(t) = p.tracks.get(p.selected_track) else {
        return;
    };
    let on = !t.solo;
    for i in set {
        if p.tracks[i].kind == TrackKind::Folder {
            p.set_folder_flags(i, None, Some(on));
        } else {
            p.set_solo(i, on);
        }
    }
}

// ------------------------------------------------------------------ Create Track Stack

/// The selected tracks as the blocks a stack takes: each selected track with everything inside
/// it when it is a stack itself; a selected track inside a selected stack goes with that stack.
fn blocks(p: &Project, set: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &i in set {
        if out.last().is_some_and(|&(_, end)| i < end) {
            continue;
        }
        out.push((i, p.folder_end(i)));
    }
    out
}

/// The track stack a block sits in directly (`None`: at the top level).
fn parent(p: &Project, i: usize) -> Option<usize> {
    let level = p.tracks[i].indent;
    (0..i).rev().find(|&j| p.tracks[j].indent < level)
}

/// The new order of the tracks for a stack made of `blocks`: the tracks before the first
/// block, the new stack (`None`), the blocks, then the rest as they were.
fn stack_order(p: &Project, blocks: &[(usize, usize)]) -> Vec<Option<usize>> {
    let first = blocks[0].0;
    let inside = |i: usize| blocks.iter().any(|&(a, b)| (a..b).contains(&i));
    let mut order: Vec<Option<usize>> = (0..first).map(Some).collect();
    order.push(None);
    order.extend(blocks.iter().flat_map(|&(a, b)| (a..b).map(Some)));
    order.extend((first..p.tracks.len()).filter(|&i| !inside(i)).map(Some));
    order
}

/// Whether Create Track Stack can put the selected tracks into one stack: they sit at the
/// same level in the same stack (or all at the top), and gathering them does not move one
/// Aux track past another (an Aux track's bus follows its place among the Aux tracks, so that
/// would change what it plays). Not during a recording.
pub(crate) fn can_create_stack(p: &Project, v: &TracksView) -> bool {
    if p.recording {
        return false;
    }
    let set = selected(p, v);
    let b = blocks(p, &set);
    let Some(&(first, _)) = b.first() else {
        return false;
    };
    let (level, up) = (p.tracks[first].indent, parent(p, first));
    if b.iter()
        .any(|&(i, _)| p.tracks[i].indent != level || parent(p, i) != up)
    {
        return false;
    }
    let aux: Vec<usize> = stack_order(p, &b)
        .into_iter()
        .flatten()
        .filter(|&i| p.tracks[i].kind == TrackKind::Aux)
        .collect();
    aux.windows(2).all(|w| w[0] < w[1])
}

/// Track > Create Track Stack (Shift-Command-D): one new track stack around every selected
/// track, in their order, at the place of the first. Selected tracks that are apart are moved
/// together under it (Logic); the tracks between them stay where they were. The stack is
/// selected. Returns its index, or `None` when [`can_create_stack`] says no.
pub(crate) fn create_stack(p: &mut Project, s: &mut UiState) -> Option<usize> {
    let at = create_stack_in(p, &mut s.tracks)?;
    crate::tracks_area::forget_track_indexes(s);
    Some(at)
}

/// [`create_stack`] with only the Tracks area's view: the caller forgets the selections of the
/// editors (track numbers moved).
pub(crate) fn create_stack_in(p: &mut Project, v: &mut TracksView) -> Option<usize> {
    if !can_create_stack(p, v) {
        return None;
    }
    let set = selected(p, v);
    let b = blocks(p, &set);
    let level = p.tracks[b[0].0].indent;
    let order = stack_order(p, &b);
    let n = p
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Folder)
        .count()
        + 1;
    let mut folder = Track::new(format!("Folder {n}"), TrackKind::Folder);
    folder.indent = level;
    // Where each old track goes, for the VCA assignments.
    let mut new_of = vec![0usize; p.tracks.len()];
    for (k, o) in order.iter().enumerate() {
        if let Some(i) = o {
            new_of[*i] = k;
        }
    }
    let at = b[0].0;
    let in_block = |i: usize| b.iter().any(|&(a, e)| (a..e).contains(&i));
    let mut old: Vec<Option<Track>> = std::mem::take(&mut p.tracks)
        .into_iter()
        .map(Some)
        .collect();
    let mut tracks = Vec::with_capacity(order.len());
    for o in &order {
        match o {
            None => tracks.push(folder.clone()),
            Some(i) => {
                let Some(mut t) = old[*i].take() else {
                    continue;
                };
                if in_block(*i) {
                    t.indent = t.indent.saturating_add(1);
                }
                t.vca = t.vca.and_then(|v| new_of.get(v).copied());
                tracks.push(t);
            }
        }
    }
    p.tracks = tracks;
    select(p, v, at, &[at]);
    p.song_rev += 1;
    v.clear_selection();
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrange_select::tests::{press, region};
    use crate::ui_test::Harness;
    use egui::{Key, Modifiers};

    /// Six instrument tracks "T0".."T5"; each has one region "R<i>".
    fn rig() -> (Project, UiState) {
        let mut p = Project::empty();
        while p.tracks.len() < 6 {
            p.add_track(TrackKind::Instrument);
        }
        for (i, t) in p.tracks.iter_mut().enumerate() {
            t.name = format!("T{i}");
            t.regions = vec![region(&format!("R{i}"), 1.0 + i as f32, 1.0)];
        }
        p.selected_track = 0;
        (p, UiState::new())
    }

    fn names(p: &Project) -> Vec<String> {
        p.tracks.iter().map(|t| t.name.clone()).collect()
    }

    fn sel(p: &Project, s: &UiState) -> Vec<usize> {
        selected(p, &s.tracks)
    }

    fn regions(s: &UiState) -> Vec<(usize, usize)> {
        let mut r = s.tracks.selection();
        r.sort_unstable();
        r
    }

    /// One undo step: `f` changes the project and the history records exactly one step.
    fn one_step(p: &mut Project, f: impl FnOnce(&mut Project)) {
        let mut h = crate::undo::History::new(p);
        f(p);
        assert!(h.record(p), "an undo step");
        assert!(!h.record(p), "only one");
    }

    #[test]
    fn plain_command_and_shift_clicks_select_tracks_and_their_regions() {
        let (mut p, mut s) = rig();
        let (p, v) = (&mut p, &mut s.tracks);
        click(p, v, 1, Click::Plain, false);
        assert_eq!((selected(p, v), p.selected_track), (vec![1], 1));
        assert_eq!(v.selection(), vec![(1, 0)]);
        // Command-click adds, and the added track takes the focus.
        click(p, v, 3, Click::Toggle, false);
        assert_eq!((selected(p, v), p.selected_track), (vec![1, 3], 3));
        click(p, v, 5, Click::Toggle, false);
        let mut r = v.selection();
        r.sort_unstable();
        assert_eq!(r, vec![(1, 0), (3, 0), (5, 0)]);
        // Command-click on a selected one takes it out; the focus goes to the nearest.
        click(p, v, 5, Click::Toggle, false);
        assert_eq!((selected(p, v), p.selected_track), (vec![1, 3], 3));
        // The last one cannot be taken out.
        click(p, v, 1, Click::Toggle, false);
        click(p, v, 3, Click::Toggle, false);
        assert_eq!(selected(p, v), vec![3]);
        click(p, v, 3, Click::Toggle, false);
        assert_eq!(selected(p, v), vec![3]);
        // Shift-click: from the last one clicked to this one.
        click(p, v, 1, Click::Plain, false);
        click(p, v, 4, Click::Range, false);
        assert_eq!((selected(p, v), p.selected_track), (vec![1, 2, 3, 4], 4));
        click(p, v, 0, Click::Range, false);
        assert_eq!(selected(p, v), vec![0, 1], "still from track 1");
        let mut r = v.selection();
        r.sort_unstable();
        assert_eq!(r, vec![(0, 0), (1, 0)]);
        // A plain click: one track again.
        click(p, v, 2, Click::Plain, false);
        assert_eq!(selected(p, v), vec![2]);
    }

    #[test]
    fn a_range_steps_over_tracks_without_a_row() {
        let (mut p, mut s) = rig();
        p.tracks[2].hidden = true;
        let (p, v) = (&mut p, &mut s.tracks);
        click(p, v, 1, Click::Plain, false);
        click(p, v, 3, Click::Range, false);
        assert_eq!(selected(p, v), vec![1, 3]);
        click(p, v, 3, Click::Range, true);
        assert_eq!(
            selected(p, v),
            vec![1, 2, 3],
            "shown with Show Hidden Tracks"
        );
    }

    #[test]
    fn the_set_falls_back_to_the_primary_track_when_it_is_changed_elsewhere() {
        let (mut p, mut s) = rig();
        let (p, v) = (&mut p, &mut s.tracks);
        click(p, v, 1, Click::Plain, false);
        click(p, v, 2, Click::Toggle, false);
        assert_eq!(selected(p, v), vec![1, 2]);
        // A click on a region elsewhere sets the selected track by itself.
        p.selected_track = 4;
        assert_eq!(selected(p, v), vec![4]);
        // A track added (the count changed): the set does not point at moved tracks.
        click(p, v, 1, Click::Plain, false);
        click(p, v, 2, Click::Toggle, false);
        p.tracks.insert(0, Track::new("X", TrackKind::Audio));
        assert_eq!(selected(p, v), vec![2]);
        assert!(!is_selected(p, v, 1));
    }

    /// A click on track `i`'s number key (part of its header) with `mods` held.
    fn tap_header(h: &mut Harness, p: &mut Project, s: &mut UiState, i: usize, mods: Modifiers) {
        let meters = crate::mixer::Meters::default();
        h.frame(vec![], |ui| crate::tracks_area::show(ui, p, s, &meters));
        let hdr = h
            .ctx
            .read_response(egui::Id::unique(("tracks_area", ("hdr", i))))
            .expect("the header is on screen")
            .rect;
        let at = egui::pos2(hdr.left() + 22.0, hdr.center().y);
        h.time_passes(1.0);
        h.modifiers = mods;
        h.click(at, |ui| crate::tracks_area::show(ui, p, s, &meters));
        h.modifiers = Modifiers::NONE;
    }

    #[test]
    fn header_clicks_select_tracks_in_the_tracks_area() {
        let (mut p, mut s) = rig();
        let (p, s) = (&mut p, &mut s);
        let h = &mut Harness::new(1200.0, 900.0);
        tap_header(h, p, s, 1, Modifiers::NONE);
        assert_eq!(sel(p, s), vec![1]);
        assert_eq!(regions(s), vec![(1, 0)]);
        tap_header(h, p, s, 3, Modifiers::COMMAND);
        assert_eq!((sel(p, s), p.selected_track), (vec![1, 3], 3));
        assert_eq!(regions(s), vec![(1, 0), (3, 0)]);
        tap_header(h, p, s, 3, Modifiers::COMMAND);
        assert_eq!((sel(p, s), p.selected_track), (vec![1], 1));
        tap_header(h, p, s, 4, Modifiers::SHIFT);
        assert_eq!((sel(p, s), p.selected_track), (vec![1, 2, 3, 4], 4));
        assert_eq!(regions(s), vec![(1, 0), (2, 0), (3, 0), (4, 0)]);
        // Selection is not an undo step.
        let mut hist = crate::undo::History::new(p);
        tap_header(h, p, s, 0, Modifiers::NONE);
        assert_eq!(sel(p, s), vec![0]);
        assert!(!hist.record(p));
    }

    #[test]
    fn shift_up_and_down_add_tracks_to_the_selection() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        click(p, &mut s.tracks, 1, Click::Plain, false);
        press(h, p, s, Key::ArrowDown, Modifiers::SHIFT);
        press(h, p, s, Key::ArrowDown, Modifiers::SHIFT);
        assert_eq!((sel(p, s), p.selected_track), (vec![1, 2, 3], 3));
        assert_eq!(regions(s), vec![(1, 0), (2, 0), (3, 0)]);
        // Down alone: one track.
        press(h, p, s, Key::ArrowDown, Modifiers::NONE);
        assert_eq!(sel(p, s), vec![4]);
        press(h, p, s, Key::ArrowUp, Modifiers::SHIFT);
        assert_eq!((sel(p, s), p.selected_track), (vec![3, 4], 3));
        assert!(crate::menus::enabled(
            "Add Previous Track to Selection",
            p,
            s
        ));
    }

    fn pick(p: &mut Project, s: &mut UiState, tracks: &[usize]) {
        click(p, &mut s.tracks, tracks[0], Click::Plain, false);
        for &t in &tracks[1..] {
            click(p, &mut s.tracks, t, Click::Toggle, false);
        }
    }

    #[test]
    fn command_delete_deletes_every_selected_track_in_one_step() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        pick(p, s, &[1, 3, 4]);
        let mut hist = crate::undo::History::new(p);
        press(h, p, s, Key::Backspace, Modifiers::COMMAND);
        assert_eq!(names(p), vec!["T0", "T2", "T5"]);
        assert!(hist.record(p) && !hist.record(p), "one undo step");
        assert!(hist.undo(p));
        assert_eq!(p.tracks.len(), 6);
        // Every track selected: one stays.
        let all: Vec<usize> = (0..6).collect();
        pick(p, s, &all);
        crate::menus::apply("Delete Track", p, s);
        assert_eq!(names(p), vec!["T0"]);
        assert!(!crate::menus::enabled("Delete Track", p, s));
    }

    #[test]
    fn control_h_hides_every_selected_track_and_shows_them_again() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        pick(p, s, &[1, 2]);
        let mut hist = crate::undo::History::new(p);
        press(h, p, s, Key::H, Modifiers::CTRL);
        assert!(p.tracks[1].hidden && p.tracks[2].hidden && !p.tracks[3].hidden);
        assert_eq!(sel(p, s), vec![3], "the next track with a row");
        assert!(hist.record(p) && !hist.record(p), "one undo step");
        // Shown again with Show Hidden Tracks on: the primary one is hidden, so both show.
        s.tracks.show_hidden = true;
        pick(p, s, &[1, 2]);
        one_step(p, |p| crate::menus::apply("Hide Track", p, s));
        assert!(!p.tracks[1].hidden && !p.tracks[2].hidden);
    }

    #[test]
    fn command_d_duplicates_each_selected_track_below_it() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        pick(p, s, &[1, 3]);
        p.tracks[3].volume_db = -6.0;
        let mut hist = crate::undo::History::new(p);
        press(h, p, s, Key::D, Modifiers::COMMAND);
        assert_eq!(
            names(p),
            vec!["T0", "T1", "T1", "T2", "T3", "T3", "T4", "T5"]
        );
        assert!(p.tracks[2].regions.is_empty() && p.tracks[5].regions.is_empty());
        assert_eq!(p.tracks[5].volume_db, -6.0);
        assert_eq!((sel(p, s), p.selected_track), (vec![2, 5], 5));
        assert!(hist.record(p) && !hist.record(p), "one undo step");
    }

    #[test]
    fn m_and_s_mute_and_solo_every_selected_track() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        pick(p, s, &[0, 2]);
        let mut hist = crate::undo::History::new(p);
        press(h, p, s, Key::M, Modifiers::NONE);
        let muted: Vec<bool> = p.tracks.iter().map(|t| t.mute).collect();
        assert_eq!(muted, vec![true, false, true, false, false, false]);
        assert!(hist.record(p) && !hist.record(p), "one undo step");
        press(h, p, s, Key::M, Modifiers::NONE);
        assert!(p.tracks.iter().all(|t| !t.mute));
        press(h, p, s, Key::S, Modifiers::NONE);
        let solo: Vec<bool> = p.tracks.iter().map(|t| t.solo).collect();
        assert_eq!(solo, vec![true, false, true, false, false, false]);
        assert!(hist.record(p));
        press(h, p, s, Key::S, Modifiers::NONE);
        assert!(!p.any_solo());
        // The primary track decides: muted, so M unmutes every selected one.
        p.tracks[2].mute = true;
        press(h, p, s, Key::M, Modifiers::NONE);
        assert!(p.tracks.iter().all(|t| !t.mute));
        // Over the Piano Roll M and S are not these.
        s.piano_focus = true;
        press(h, p, s, Key::M, Modifiers::NONE);
        assert!(p.tracks.iter().all(|t| !t.mute));
    }

    #[test]
    fn a_track_stack_mute_reaches_its_tracks() {
        let (mut p, mut s) = rig();
        let f = p.create_track_stack(&[1], false).unwrap();
        click(&mut p, &mut s.tracks, f, Click::Plain, false);
        toggle_mute(&mut p, &s.tracks);
        assert!(p.tracks[f].mute && p.tracks[f + 1].mute && !p.tracks[f + 2].mute);
    }

    #[test]
    fn create_track_stack_gathers_the_selected_tracks_under_one_stack() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        // Adjacent tracks: the stack goes around them.
        pick(p, s, &[1, 2]);
        let mut hist = crate::undo::History::new(p);
        press(h, p, s, Key::D, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(
            names(p),
            vec!["T0", "Folder 1", "T1", "T2", "T3", "T4", "T5"]
        );
        let indents: Vec<u8> = p.tracks.iter().map(|t| t.indent).collect();
        assert_eq!(indents, vec![0, 0, 1, 1, 0, 0, 0]);
        assert_eq!(sel(p, s), vec![1], "the stack is selected");
        assert!(hist.record(p) && !hist.record(p), "one undo step");
        assert!(hist.undo(p));
        // Apart: moved together under the stack at the first one's place, in their order.
        p.tracks[5].vca = None;
        let vca = p.add_track(TrackKind::Vca);
        p.tracks[vca].name = "V".into();
        p.tracks[4].vca = Some(vca);
        pick(p, s, &[0, 2, 4]);
        one_step(p, |p| {
            create_stack(p, s);
        });
        assert_eq!(
            names(p),
            vec!["Folder 1", "T0", "T2", "T4", "T1", "T3", "T5", "V"]
        );
        let indents: Vec<u8> = p.tracks.iter().map(|t| t.indent).collect();
        assert_eq!(indents, vec![0, 1, 1, 1, 0, 0, 0, 0]);
        assert_eq!(
            p.tracks[3].vca,
            Some(7),
            "the VCA assignment follows both tracks"
        );
    }

    #[test]
    fn create_track_stack_is_greyed_when_it_cannot_gather_the_tracks() {
        let (mut p, mut s) = rig();
        // Track 1 in a stack, track 4 at the top level: not in the same place.
        p.create_track_stack(&[1], false);
        // Tracks: T0, Folder 1, T1, T2, T3, T4, T5.
        pick(&mut p, &mut s, &[2, 5]);
        assert!(!crate::menus::enabled("Create Track Stack…", &p, &s));
        // Moving an Aux track past another would change its bus.
        let (mut p, mut s) = rig();
        p.tracks[1].kind = TrackKind::Aux;
        p.tracks[2].kind = TrackKind::Aux;
        pick(&mut p, &mut s, &[0, 2]);
        assert!(!crate::menus::enabled("Create Track Stack…", &p, &s));
        pick(&mut p, &mut s, &[0, 1, 2]);
        assert!(crate::menus::enabled("Create Track Stack…", &p, &s));
        // A selected track inside a selected stack goes with it.
        let (mut p, mut s) = rig();
        let f = p.create_track_stack(&[1], false).unwrap();
        pick(&mut p, &mut s, &[f, f + 1, 4]);
        assert!(create_stack(&mut p, &mut s).is_some());
        assert_eq!(
            names(&p),
            vec!["T0", "Folder 2", "Folder 1", "T1", "T3", "T2", "T4", "T5"]
        );
        let indents: Vec<u8> = p.tracks.iter().map(|t| t.indent).collect();
        assert_eq!(indents, vec![0, 0, 1, 2, 1, 0, 0, 0]);
    }
}
