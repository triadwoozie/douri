//! Logic's marker commands (Navigate menu and key commands): Create Marker (rounded to the
//! nearest bar, or without rounding), Create Marker for Selected Regions, Delete Marker, Go to
//! Next / Previous Marker and Set Locators by Marker and Enable Cycle. The selected marker (a
//! click on the Marker track selects it) is `TracksView::selected_marker`.
use crate::model::Project;
use crate::tracks_area::TracksView;

/// Create Marker: a marker at `at`, at the nearest bar line when `round` (Logic's Create
/// Marker) or exactly there (Create Marker without Rounding). Returns its index; a marker
/// already there is returned as it is.
pub(crate) fn create(p: &mut Project, at: f64, round: bool) -> usize {
    let beat = if round {
        p.sigs().nearest_bar_line(at)
    } else {
        at
    };
    p.add_marker(beat)
}

/// Create Marker for Selected Regions: a marker at the start of each selected region, named
/// after it. Returns the index of the last one made.
pub(crate) fn create_for_regions(p: &mut Project, v: &TracksView) -> Option<usize> {
    let mut starts: Vec<(f64, String)> = v
        .selection()
        .into_iter()
        .filter_map(|(t, r)| {
            let reg = p.tracks.get(t)?.regions.get(r)?;
            Some((p.region_start_beats(reg), reg.name.clone()))
        })
        .collect();
    starts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut last = None;
    for (beat, name) in starts {
        let i = p.add_marker(beat);
        if !name.trim().is_empty() {
            p.markers[i].name = name;
        }
        last = Some(i);
    }
    last
}

/// The marker the marker commands act on: the selected one, else the one the playhead is in.
pub(crate) fn target(p: &Project, v: &TracksView) -> Option<usize> {
    v.selected_marker
        .filter(|i| *i < p.markers.len())
        .or_else(|| p.marker_at(p.position_beats))
}

/// Delete Marker: the selected marker, else the one the playhead is in.
pub(crate) fn delete(p: &mut Project, v: &mut TracksView) {
    if let Some(i) = target(p, v) {
        p.markers.remove(i);
    }
    v.selected_marker = None;
}

/// Where marker `i` runs: from its start to the next marker, the last one to the end of the
/// song (at least one bar).
pub(crate) fn span(p: &Project, i: usize) -> Option<(f64, f64)> {
    let start = p.markers.get(i)?.beat;
    let end = match p.markers.get(i + 1) {
        Some(next) => next.beat,
        None => {
            let song_end = p
                .tracks
                .iter()
                .flat_map(|t| t.regions.iter())
                .map(|r| p.region_loop_end_beats(r))
                .fold(0.0, f64::max);
            song_end.max(start + p.bar_len_at(start))
        }
    };
    Some((start, end))
}

/// Set Locators by Marker and Enable Cycle: the cycle over the selected marker (or the one the
/// playhead is in).
pub(crate) fn set_locators(p: &mut Project, v: &TracksView) {
    if let Some((start, end)) = target(p, v).and_then(|i| span(p, i)) {
        p.cycle_start = start;
        p.cycle_end = end;
        p.cycle = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Region, RegionParams, TrackKind};

    fn region(name: &str, start_bar: f32) -> Region {
        Region {
            missing: None,
            name: name.into(),
            start_bar,
            len_bars: 1.0,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            audio: None,
            params: RegionParams::default(),
            generated: None,
        }
    }

    #[test]
    fn markers_are_made_rounded_or_exactly_at_the_playhead() {
        let mut p = Project::empty();
        for (at, round, want) in [(5.5, true, 4.0), (6.5, true, 8.0), (5.5, false, 5.5)] {
            let i = create(&mut p, at, round);
            assert_eq!(p.markers[i].beat, want);
        }
        assert_eq!(p.markers.len(), 3);
    }

    #[test]
    fn markers_for_the_selected_regions_take_their_names() {
        let mut p = Project::empty();
        p.tracks[0].regions = vec![region("Verse", 5.0), region("Intro", 1.0)];
        p.add_track(TrackKind::Instrument);
        p.tracks[1].regions = vec![region("Chorus", 9.0)];
        let mut v = TracksView::default();
        v.select_many(&[(0, 0), (0, 1), (1, 0)]);
        create_for_regions(&mut p, &v);
        let made: Vec<(f64, &str)> = p
            .markers
            .iter()
            .map(|m| (m.beat, m.name.as_str()))
            .collect();
        assert_eq!(
            made,
            vec![(0.0, "Intro"), (16.0, "Verse"), (32.0, "Chorus")]
        );
    }

    #[test]
    fn locators_by_marker_span_it_up_to_the_next() {
        let mut p = Project::empty();
        p.tracks[0].regions = vec![region("A", 10.0)];
        p.add_marker(8.0);
        p.add_marker(20.0);
        let mut v = TracksView::default();
        p.position_beats = 10.0;
        set_locators(&mut p, &v);
        assert_eq!((p.cycle, p.cycle_start, p.cycle_end), (true, 8.0, 20.0));
        // The selected marker wins over the playhead; the last one runs to the end of the song.
        v.selected_marker = Some(1);
        set_locators(&mut p, &v);
        assert_eq!((p.cycle_start, p.cycle_end), (20.0, 40.0));
        // Delete Marker takes the selected one.
        delete(&mut p, &mut v);
        assert_eq!(p.markers.len(), 1);
        assert_eq!(p.markers[0].beat, 8.0);
        assert_eq!(v.selected_marker, None);
    }

    // ---- in the app: the keys, the menus, the Marker track

    use crate::ui_test::Harness;
    use egui::{Event, Key, Modifiers, Pos2};

    fn app_frame(h: &mut Harness, p: &mut Project, s: &mut crate::UiState, events: Vec<Event>) {
        let meters = crate::mixer::Meters::default();
        h.frame(events, |ui| {
            let ctx = ui.ctx().clone();
            crate::key_commands::begin_frame(&ctx);
            crate::menus::handle_keys(&ctx, p, s);
            crate::tracks_area::show(ui, p, s, &meters);
            crate::key_commands::handle(&ctx, p, s);
        });
    }

    fn tap(h: &mut Harness, p: &mut Project, s: &mut crate::UiState, key: Key, mods: Modifiers) {
        h.modifiers = mods;
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: mods,
        };
        app_frame(h, p, s, vec![ev(true), ev(false)]);
        h.modifiers = Modifiers::NONE;
        app_frame(h, p, s, vec![]);
    }

    fn click(h: &mut Harness, p: &mut Project, s: &mut crate::UiState, at: Pos2) {
        h.time_passes(1.0);
        let meters = crate::mixer::Meters::default();
        h.click(at, |ui| crate::tracks_area::show(ui, p, s, &meters));
    }

    fn on_screen(h: &Harness, id: egui::Id) -> egui::Rect {
        h.ctx.read_response(id).expect("on screen").rect
    }

    fn rig() -> (Project, crate::UiState, Harness) {
        let mut p = Project::empty();
        p.tracks[0].regions = vec![region("Intro", 1.0), region("Verse", 5.0)];
        let mut s = crate::UiState::new();
        s.tracks.show_markers = true;
        let mut h = Harness::new(1400.0, 600.0);
        app_frame(&mut h, &mut p, &mut s, vec![]);
        (p, s, h)
    }

    #[test]
    fn option_quote_makes_a_marker_and_option_period_and_comma_go_through_them() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        p.position_beats = 5.5;
        tap(h, p, s, Key::Quote, Modifiers::ALT);
        p.position_beats = 13.0;
        tap(h, p, s, Key::Quote, Modifiers::ALT);
        let beats: Vec<f64> = p.markers.iter().map(|m| m.beat).collect();
        assert_eq!(beats, vec![4.0, 12.0], "each at the nearest bar line");
        // Quote alone is not Create Marker.
        tap(h, p, s, Key::Quote, Modifiers::NONE);
        assert_eq!(p.markers.len(), 2);
        p.position_beats = 0.0;
        tap(h, p, s, Key::Period, Modifiers::ALT);
        assert_eq!(p.position_beats, 4.0);
        tap(h, p, s, Key::Period, Modifiers::ALT);
        assert_eq!(p.position_beats, 12.0);
        tap(h, p, s, Key::Comma, Modifiers::ALT);
        assert_eq!(p.position_beats, 4.0);
        // Create Marker Without Rounding (menu only): exactly at the playhead.
        p.position_beats = 9.25;
        crate::menus::apply("Create Marker Without Rounding", p, s);
        assert!(p.markers.iter().any(|m| m.beat == 9.25));
    }

    #[test]
    fn logics_other_marker_keys_make_markers_and_set_the_locators() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        let ctrl_alt = Modifiers::CTRL | Modifiers::ALT;
        // Control-Option-': Create Marker Without Rounding.
        p.position_beats = 9.25;
        tap(h, p, s, Key::Quote, ctrl_alt);
        let beats: Vec<f64> = p.markers.iter().map(|m| m.beat).collect();
        assert_eq!(beats, vec![9.25]);
        // Option-Shift-': Create Markers by Regions.
        s.tracks.select_many(&[(0, 0), (0, 1)]);
        tap(h, p, s, Key::Quote, Modifiers::ALT | Modifiers::SHIFT);
        let made: Vec<(f64, &str)> = p
            .markers
            .iter()
            .map(|m| (m.beat, m.name.as_str()))
            .collect();
        assert_eq!(made[0], (0.0, "Intro"));
        assert_eq!(made[2], (16.0, "Verse"));
        // Control-Option-C: the locators span the marker the playhead is in, Cycle on.
        s.tracks.clear_selection();
        p.cycle = false;
        p.position_beats = 10.0;
        tap(h, p, s, Key::C, ctrl_alt);
        assert_eq!((p.cycle, p.cycle_start, p.cycle_end), (true, 9.25, 16.0));
    }

    #[test]
    fn the_plus_key_makes_a_selected_marker_and_delete_removes_the_selected_one() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        let mut history = crate::undo::History::new(p);
        p.position_beats = 18.5;
        let plus = on_screen(h, crate::marker_track::add_key_id()).center();
        click(h, p, s, plus);
        assert_eq!(p.markers.len(), 1);
        assert_eq!(p.markers[0].beat, 20.0, "bar 6, the nearest bar line");
        assert_eq!(s.tracks.selected_marker, Some(0));
        assert!(history.record(p));
        // A second marker before it; a click selects it (and moves the playhead there).
        p.add_marker(8.0);
        assert!(history.record(p));
        // Two frames: what is on screen is read from the frame before.
        app_frame(h, p, s, vec![]);
        app_frame(h, p, s, vec![]);
        let first =
            on_screen(h, crate::marker_track::marker_id(0)).left_center() + egui::vec2(12.0, 0.0);
        s.tracks.select_only(0, 1);
        click(h, p, s, first);
        assert_eq!(s.tracks.selected_marker, Some(0));
        assert!(
            s.tracks.selection().is_empty(),
            "the regions are deselected"
        );
        assert_eq!(p.position_beats, 8.0);
        // Delete: that marker goes, the regions stay.
        tap(h, p, s, Key::Delete, Modifiers::NONE);
        let beats: Vec<f64> = p.markers.iter().map(|m| m.beat).collect();
        assert_eq!(beats, vec![20.0]);
        assert_eq!(p.tracks[0].regions.len(), 2);
        assert_eq!(s.tracks.selected_marker, None);
        // One undo step brings it back.
        assert!(history.record(p));
        assert!(history.undo(p));
        assert_eq!(p.markers.len(), 2);
    }

    #[test]
    fn the_menus_make_markers_by_regions_and_set_the_locators_by_a_marker() {
        let (mut p, mut s, _h) = rig();
        let (p, s) = (&mut p, &mut s);
        assert!(!crate::menus::enabled("Create Markers by Regions", p, s));
        assert!(!crate::menus::enabled(
            "Set Locators by Marker and Enable Cycle",
            p,
            s
        ));
        s.tracks.select_many(&[(0, 0), (0, 1)]);
        crate::menus::apply("Create Markers by Regions", p, s);
        let made: Vec<(f64, &str)> = p
            .markers
            .iter()
            .map(|m| (m.beat, m.name.as_str()))
            .collect();
        assert_eq!(made, vec![(0.0, "Intro"), (16.0, "Verse")]);
        s.tracks.selected_marker = Some(0);
        p.cycle = false;
        crate::menus::apply("Set Locators by Marker and Enable Cycle", p, s);
        assert_eq!((p.cycle, p.cycle_start, p.cycle_end), (true, 0.0, 16.0));
        // Delete Marker (menu): the selected one.
        crate::menus::apply("Delete Marker", p, s);
        assert_eq!(p.markers.len(), 1);
        assert_eq!(p.markers[0].name, "Verse");
    }

    #[test]
    fn shift_command_period_and_comma_move_the_locators_by_the_cycle_length() {
        let (mut p, mut s, mut h) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        (p.cycle_start, p.cycle_end) = (8.0, 16.0);
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        tap(h, p, s, Key::Period, cmd_shift);
        assert_eq!((p.cycle_start, p.cycle_end), (16.0, 24.0));
        tap(h, p, s, Key::Comma, cmd_shift);
        tap(h, p, s, Key::Comma, cmd_shift);
        assert_eq!((p.cycle_start, p.cycle_end), (0.0, 8.0));
        assert!(!crate::menus::enabled(
            "Move Locators Backward by Cycle Length",
            p,
            s
        ));
        // The menu entries do the same.
        crate::menus::apply("Move Locators Forward by Cycle Length", p, s);
        assert_eq!((p.cycle_start, p.cycle_end), (8.0, 16.0));
    }
}
