//! Selection commands of the Tracks area (Logic Pro: Edit > Select, and the arrow keys):
//! the next or previous region on the selected track (Right, Left; with Shift added to the
//! selection), the track below or above (Down, Up; with Shift its regions are added), the
//! regions inside the locators (Shift-L), the overlapped ones and the muted ones (Shift-M).
//!
//! Tracks hidden in a folded folder are never selected by these: the arrows step over them,
//! and the Select commands leave their regions out.
//!
//! A track selected with Up or Down selects its regions, as a click on its header does (Logic:
//! a selected track selects its regions).
use crate::model::Project;
use crate::tracks_area::TracksView;

/// The regions of track `t`, earliest first (equal starts in track order).
fn in_time_order(p: &Project, t: usize) -> Vec<usize> {
    let Some(track) = p.tracks.get(t) else {
        return Vec::new();
    };
    let mut order: Vec<usize> = (0..track.regions.len()).collect();
    order.sort_by(|a, b| {
        let start = |r: &usize| p.region_start_beats(&track.regions[*r]);
        start(a).total_cmp(&start(b)).then(a.cmp(b))
    });
    order
}

/// Right (`next`) or Left: the region after or before the selected one on the selected track;
/// with none of that track's regions the primary one, the first that starts at or after the
/// playhead (Right) or the last that starts before it (Left). `None`: there is none.
pub(crate) fn region_target(p: &Project, v: &TracksView, next: bool) -> Option<(usize, usize)> {
    let t = p.selected_track;
    if t >= p.tracks.len() || crate::track_commands::takes_no_room(p, v.show_hidden, t) {
        return None;
    }
    let order = in_time_order(p, t);
    let start = |r: usize| p.region_start_beats(&p.tracks[t].regions[r]);
    let anchor = v
        .selected
        .filter(|(pt, _)| *pt == t)
        .and_then(|(_, r)| order.iter().position(|x| *x == r));
    let target = match anchor {
        Some(k) if next => order.get(k + 1).copied(),
        Some(k) => k.checked_sub(1).and_then(|k| order.get(k)).copied(),
        None => {
            let at = p.position_beats;
            if next {
                order.iter().copied().find(|r| start(*r) >= at - 1e-9)
            } else {
                order.iter().rev().copied().find(|r| start(*r) < at - 1e-9)
            }
        }
    };
    target.map(|r| (t, r))
}

/// Selects `region_target`; `add` (Shift): it joins the selection and becomes the primary
/// one. Returns whether there was one.
pub(crate) fn step_region(p: &Project, v: &mut TracksView, next: bool, add: bool) -> bool {
    let Some((t, r)) = region_target(p, v, next) else {
        return false;
    };
    if add {
        let mut sel = vec![(t, r)];
        sel.extend(v.selection());
        v.select_many(&sel);
    } else {
        v.select_only(t, r);
    }
    true
}

/// Down (`down`) or Up: the next track below or above that has a row (not in a folded folder,
/// and not hidden unless hidden tracks are shown).
pub(crate) fn track_target(p: &Project, show_hidden: bool, down: bool) -> Option<usize> {
    let shown = |i: usize| !crate::track_commands::takes_no_room(p, show_hidden, i);
    let n = p.tracks.len();
    if n == 0 {
        return None;
    }
    let from = p.selected_track.min(n - 1);
    if down {
        (from + 1..n).find(|&i| shown(i))
    } else {
        (0..from).rev().find(|&i| shown(i))
    }
}

/// Down or Up: `track_target` becomes the selected track, and its regions are selected; `add`
/// (Shift): the track joins the selected tracks (`track_selection.rs`) and its regions are
/// added to the selection. Returns whether the selected track changed.
pub(crate) fn step_track(p: &mut Project, v: &mut TracksView, down: bool, add: bool) -> bool {
    let Some(t) = track_target(p, v.show_hidden, down) else {
        return false;
    };
    let before = crate::track_selection::selected(p, v);
    if add {
        crate::track_selection::add_track(p, v, &before, t);
    } else {
        crate::track_selection::select(p, v, t, &[t]);
    }
    let mut sel = if add { v.selection() } else { Vec::new() };
    sel.extend(in_time_order(p, t).into_iter().map(|r| (t, r)));
    v.select_many(&sel);
    true
}

/// Every region on a track that has a row (not inside a folded folder, not hidden unless
/// hidden tracks are shown) for which `keep` holds.
fn shown_regions(
    p: &Project,
    show_hidden: bool,
    keep: impl Fn(usize, usize) -> bool,
) -> Vec<(usize, usize)> {
    (0..p.tracks.len())
        .filter(|&t| !crate::track_commands::takes_no_room(p, show_hidden, t))
        .flat_map(|t| (0..p.tracks[t].regions.len()).map(move |r| (t, r)))
        .filter(|&(t, r)| keep(t, r))
        .collect()
}

/// Edit > Select > Inside Locators (Shift-L): the regions that lie wholly between the left and
/// the right locator; nothing else stays selected.
pub(crate) fn select_inside_locators(p: &Project, v: &mut TracksView) {
    let (from, to) = (p.cycle_start, p.cycle_end);
    let picked = shown_regions(p, v.show_hidden, |t, r| {
        let reg = &p.tracks[t].regions[r];
        p.region_start_beats(reg) >= from - 1e-9 && p.region_end_beats(reg) <= to + 1e-9
    });
    v.select_many(&picked);
}

/// Edit > Select > Overlapped Regions: every region that overlaps another on its track.
pub(crate) fn select_overlapped(p: &Project, v: &mut TracksView) {
    v.select_many(&overlapped(p, v.show_hidden));
}

/// The shown regions that overlap another on their track.
pub(crate) fn overlapped(p: &Project, show_hidden: bool) -> Vec<(usize, usize)> {
    shown_regions(p, show_hidden, |t, r| {
        crate::tracks_area::overlaps_another(p, t, r)
    })
}

/// Edit > Select > Muted Regions (Shift-M): every muted region.
pub(crate) fn select_muted(p: &Project, v: &mut TracksView) {
    v.select_many(&muted(p, v.show_hidden));
}

/// The shown regions that are muted.
pub(crate) fn muted(p: &Project, show_hidden: bool) -> Vec<(usize, usize)> {
    shown_regions(p, show_hidden, |t, r| p.tracks[t].regions[r].params.mute)
}

/// Whether there is a region on a shown track at all (for the Select entries).
pub(crate) fn any_shown_region(p: &Project, show_hidden: bool) -> bool {
    !shown_regions(p, show_hidden, |_, _| true).is_empty()
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::model::{Project, Region, RegionParams, TrackKind};
    use crate::ui_test::Harness;
    use crate::UiState;
    use egui::{Event, Key, Modifiers};

    pub(crate) fn region(name: &str, start_bar: f32, len_bars: f32) -> Region {
        Region {
            missing: None,
            name: name.into(),
            start_bar,
            len_bars,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            audio: None,
            params: RegionParams::default(),
            generated: None,
        }
    }

    /// Five tracks in 4/4: 0 has "C" (bar 5), "A" (bar 2, muted) and "B" (bar 3); 1 has
    /// nothing; 2 is a folded folder around 3 ("H", muted, hidden); 4 has "D" (bar 2) and "E"
    /// (bar 3, overlapping "D").
    pub(crate) fn rig() -> (Project, UiState) {
        let mut p = Project::empty();
        for _ in 0..3 {
            p.add_track(TrackKind::Instrument);
        }
        let folder = p.create_track_stack(&[2], false).unwrap();
        p.tracks[folder].folded = true;
        assert_eq!(p.tracks.len(), 5);
        p.tracks[0].regions = vec![
            region("C", 5.0, 2.0),
            region("A", 2.0, 1.0),
            region("B", 3.0, 1.0),
        ];
        p.tracks[0].regions[1].params.mute = true;
        p.tracks[3].regions = vec![region("H", 2.0, 1.0)];
        p.tracks[3].regions[0].params.mute = true;
        p.tracks[4].regions = vec![region("D", 2.0, 2.0), region("E", 3.0, 2.0)];
        p.selected_track = 0;
        assert!(p.hidden_by_fold(3) && !p.hidden_by_fold(2) && !p.hidden_by_fold(4));
        (p, UiState::new())
    }

    /// `key` with `mods`, run as the app runs its keys.
    pub(crate) fn press(
        h: &mut Harness,
        p: &mut Project,
        s: &mut UiState,
        key: Key,
        mods: Modifiers,
    ) {
        h.modifiers = mods;
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: mods,
        };
        h.frame(vec![ev(true), ev(false)], |ui| {
            let ctx = ui.ctx().clone();
            crate::key_commands::begin_frame(&ctx);
            crate::menus::handle_keys(&ctx, p, s);
            crate::key_commands::handle(&ctx, p, s);
        });
        h.modifiers = Modifiers::NONE;
    }

    fn names(p: &Project, s: &UiState) -> Vec<String> {
        s.tracks
            .selection()
            .into_iter()
            .map(|(t, r)| p.tracks[t].regions[r].name.clone())
            .collect()
    }

    fn primary(p: &Project, s: &UiState) -> Option<String> {
        s.tracks
            .selected
            .map(|(t, r)| p.tracks[t].regions[r].name.clone())
    }

    const NONE: Modifiers = Modifiers::NONE;
    const SHIFT: Modifiers = Modifiers::SHIFT;

    #[test]
    fn right_and_left_step_through_the_selected_tracks_regions_in_time_order() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        // Nothing selected: Right takes the first region at or after the playhead (bar 3).
        p.position_beats = 8.0;
        press(h, p, s, Key::ArrowRight, NONE);
        assert_eq!(names(p, s), vec!["B"]);
        press(h, p, s, Key::ArrowRight, NONE);
        assert_eq!(names(p, s), vec!["C"]);
        press(h, p, s, Key::ArrowRight, NONE);
        assert_eq!(names(p, s), vec!["C"], "nothing after the last one");
        press(h, p, s, Key::ArrowLeft, NONE);
        press(h, p, s, Key::ArrowLeft, NONE);
        assert_eq!(names(p, s), vec!["A"]);
        // Shift: added, and the one added is the primary one.
        press(h, p, s, Key::ArrowRight, SHIFT);
        assert_eq!(primary(p, s).as_deref(), Some("B"));
        press(h, p, s, Key::ArrowRight, SHIFT);
        assert_eq!(primary(p, s).as_deref(), Some("C"));
        let mut all = names(p, s);
        all.sort();
        assert_eq!(all, vec!["A", "B", "C"]);
        // Nothing selected and the playhead after them all: Left takes the last one.
        s.tracks.clear_selection();
        p.position_beats = 100.0;
        press(h, p, s, Key::ArrowLeft, NONE);
        assert_eq!(names(p, s), vec!["C"]);
        // Option-Right is still Nudge, not this.
        press(h, p, s, Key::ArrowRight, Modifiers::ALT);
        assert_eq!(p.tracks[0].regions[0].start_bar, 5.0 + 1.0 / 16.0);
    }

    #[test]
    fn up_and_down_select_the_next_shown_track_and_its_regions() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        press(h, p, s, Key::ArrowDown, NONE);
        assert_eq!(p.selected_track, 1);
        assert!(s.tracks.selection().is_empty(), "track 1 has no regions");
        press(h, p, s, Key::ArrowDown, NONE);
        assert_eq!(p.selected_track, 2, "the folder");
        press(h, p, s, Key::ArrowDown, NONE);
        assert_eq!(
            p.selected_track, 4,
            "the track in the folded folder is stepped over"
        );
        assert_eq!(names(p, s), vec!["D", "E"]);
        press(h, p, s, Key::ArrowDown, NONE);
        assert_eq!(p.selected_track, 4, "the last track stays");
        // Right and Left now go through this track's regions.
        press(h, p, s, Key::ArrowRight, NONE);
        assert_eq!(names(p, s), vec!["E"]);
        // Shift-Up: up to the folder and then to track 0, whose regions are added.
        press(h, p, s, Key::ArrowUp, NONE);
        assert_eq!(p.selected_track, 2);
        press(h, p, s, Key::ArrowUp, NONE);
        assert_eq!(p.selected_track, 1);
        press(h, p, s, Key::ArrowUp, SHIFT);
        assert_eq!(p.selected_track, 0);
        assert_eq!(names(p, s), vec!["C", "A", "B"]);
        press(h, p, s, Key::ArrowDown, NONE);
        press(h, p, s, Key::ArrowDown, NONE);
        press(h, p, s, Key::ArrowDown, SHIFT);
        assert_eq!(p.selected_track, 4);
        assert_eq!(
            names(p, s),
            vec!["D", "E"],
            "track 2 had none, track 4's are added"
        );
        // A selected track hidden in a folded folder has no regions to step through.
        p.selected_track = 3;
        s.tracks.clear_selection();
        press(h, p, s, Key::ArrowRight, NONE);
        assert!(s.tracks.selection().is_empty());
        assert!(!crate::menus::enabled("Select Next Region", p, s));
        p.selected_track = 4;
        s.tracks.select_only(4, 1);
        assert!(
            !crate::menus::enabled("Select Next Region", p, s),
            "E is the last"
        );
        assert!(crate::menus::enabled("Select Previous Region", p, s));
        assert!(!crate::menus::enabled("Select Next Track", p, s));
    }

    #[test]
    fn arrows_wait_over_the_piano_roll_and_change_nothing_undo_keeps() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        s.piano_focus = true;
        press(h, p, s, Key::ArrowDown, NONE);
        press(h, p, s, Key::ArrowRight, NONE);
        assert_eq!(p.selected_track, 0);
        assert!(s.tracks.selection().is_empty());
        // The selection is not saved, so stepping makes no undo step.
        s.piano_focus = false;
        let mut history = crate::undo::History::new(p);
        press(h, p, s, Key::ArrowDown, NONE);
        press(h, p, s, Key::ArrowRight, NONE);
        assert!(!history.record(p));
    }

    #[test]
    fn select_inside_locators_overlapped_and_muted_leave_hidden_tracks_out() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        // Locators bar 2 to bar 4: "A" and "B" lie inside, "D" (bars 2-3) too, "E" ends at 5.
        (p.cycle_start, p.cycle_end) = (4.0, 12.0);
        press(h, p, s, Key::L, SHIFT);
        assert_eq!(names(p, s), vec!["A", "B", "D"]);
        assert!(
            p.tracks[0]
                .regions
                .iter()
                .all(|r| r.params.loop_beats == 0.0),
            "not L"
        );
        // Shift-M: the muted ones; "H" is in the folded folder.
        press(h, p, s, Key::M, SHIFT);
        assert_eq!(names(p, s), vec!["A"]);
        // Shift-O: the overlapped ones.
        press(h, p, s, Key::O, SHIFT);
        assert_eq!(names(p, s), vec!["D", "E"]);
        // With nothing muted or overlapped they cannot be chosen.
        p.tracks[0].regions[1].params.mute = false;
        p.tracks[4].regions.pop();
        assert!(!crate::menus::enabled("Select Muted Regions", p, s));
        assert!(!crate::menus::enabled("Select Overlapped Regions", p, s));
        // Every one of them is in Edit > Select.
        let select = crate::menus::submenu("Select").unwrap();
        for label in [
            "Select All",
            "Select All Following",
            "Select All Following of Same Track",
            "Select Inside Locators",
            "Select Overlapped Regions",
            "Select Muted Regions",
            "Invert Selection",
            "Deselect All",
            "Select Next Region",
            "Select Previous Track",
        ] {
            assert!(select.contains(&label), "{label}");
            assert!(crate::menus::implemented(label), "{label}");
        }
        assert!(crate::menus::MENUS
            .iter()
            .any(|(m, items)| *m == "Edit" && items.contains(&"Select")));
    }
}
