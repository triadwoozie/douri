//! Tests of how the arrange window's features work together: each feature has its own tests
//! in its module; these cover what came up when they were put together (the marquee and the
//! other selections, hidden tracks, track numbers that shift, keys while a window is open).

use crate::model::{Note, Project, Region, RegionParams, TrackKind};
use crate::tracks_area::{delete_selected, TracksView};
use crate::ui_test::Harness;
use egui::{Event, Key, Modifiers};

fn region(name: &str, start_bar: f32, len_bars: f32) -> Region {
    Region {
        missing: None,
        name: name.into(),
        start_bar,
        len_bars,
        notes: vec![Note {
            lyric: crate::lyrics::Lyric::NONE,
            mute: false,
            start: 0.0,
            len: 1.0,
            pitch: 60,
            vel: 90,
        }],
        controls: Vec::new(),
        folder: None,
        audio: None,
        params: RegionParams::default(),
        generated: None,
    }
}

/// Three instrument tracks, each with one region over bars 1-5.
fn three() -> Project {
    let mut p = Project::empty();
    p.add_track(TrackKind::Instrument);
    p.add_track(TrackKind::Instrument);
    for (i, t) in p.tracks.iter_mut().enumerate() {
        t.regions = vec![region(&format!("R{i}"), 1.0, 4.0)];
    }
    p
}

/// A marquee over beats 4-8 on tracks `first..=last`.
fn marquee(v: &mut TracksView, first: usize, last: usize) {
    v.range.begin((4.0, first), false);
    v.range.drag_to((8.0, last));
    v.range.end();
    assert!(v.range.active().is_some());
}

#[test]
fn selecting_regions_or_a_marker_takes_the_marquee_away_so_delete_acts_on_the_selection() {
    let mut p = three();
    let mut v = TracksView::default();
    marquee(&mut v, 0, 0);
    // A right-click or a click on a region of track 2 selects it: the marquee goes, and
    // Delete removes that region, not the material under the old marquee.
    v.select_only(2, 0);
    assert!(v.range.active().is_none());
    delete_selected(&mut p, &mut v);
    assert!(p.tracks[2].regions.is_empty());
    assert_eq!(p.tracks[0].regions.len(), 1, "track 0 untouched");
    // Shift-click (toggle) after a marquee, and after a selected marker.
    marquee(&mut v, 0, 0);
    v.selected_marker = Some(0);
    v.toggle(1, 0);
    assert!(v.range.active().is_none());
    assert_eq!(v.selected_marker, None, "Delete must not take the marker");
    // Selecting many (a header click, Select All, the arrow keys).
    marquee(&mut v, 0, 0);
    v.select_many(&[(0, 0)]);
    assert!(v.range.active().is_none());
}

#[test]
fn hidden_tracks_are_left_out_of_the_marquee_the_arrow_keys_and_drags() {
    let mut p = three();
    p.tracks[1].hidden = true;
    let mut v = TracksView::default();
    // A marquee from track 0 to track 2 over a hidden track 1 leaves track 1 alone.
    marquee(&mut v, 0, 2);
    assert!(crate::marquee::delete(&mut p, &mut v));
    assert_eq!(
        p.tracks[1].regions.len(),
        1,
        "the hidden track keeps its region"
    );
    assert_eq!(p.tracks[1].regions[0].len_bars, 4.0);
    assert!(p.tracks[0].regions.len() > 1, "track 0 was cut");
    // Down from track 0 goes to track 2; Up from 2 to 0.
    p.selected_track = 0;
    assert_eq!(
        crate::arrange_select::track_target(&p, false, true),
        Some(2)
    );
    p.selected_track = 2;
    assert_eq!(
        crate::arrange_select::track_target(&p, false, false),
        Some(0)
    );
    // With hidden tracks shown, track 1 is a stop again.
    assert_eq!(
        crate::arrange_select::track_target(&p, true, false),
        Some(1)
    );
    // A drag one track down from track 0 does not land on the hidden track.
    let group = [(0, 0, 0.0)];
    assert!(!crate::region_edit::fits_rows(&p, &group, 1, false));
    assert!(crate::region_edit::fits_rows(&p, &group, 1, true));
    // Select Muted Regions does not pick up a muted region on the hidden track.
    p.tracks[1].regions[0].params.mute = true;
    assert!(crate::arrange_select::muted(&p, false).is_empty());
    assert_eq!(crate::arrange_select::muted(&p, true), vec![(1, 0)]);
}

#[test]
fn plain_delete_with_nothing_selected_keeps_a_stack_an_aux_and_a_track_with_regions() {
    let mut p = three();
    let aux = p.add_track(TrackKind::Aux);
    let mut v = TracksView::default();
    let n = p.tracks.len();
    // An Aux track has no regions of its own but is not an empty track to delete.
    p.selected_track = aux;
    delete_selected(&mut p, &mut v);
    assert_eq!(p.tracks.len(), n);
    // A track stack whose tracks hold regions.
    p.create_track_stack(0);
    let folder = (0..p.tracks.len())
        .find(|&i| p.tracks[i].kind == TrackKind::Folder)
        .unwrap();
    p.selected_track = folder;
    let n = p.tracks.len();
    delete_selected(&mut p, &mut v);
    assert_eq!(p.tracks.len(), n, "the stack stays");
    // An empty instrument track does go.
    let empty = p.add_track(TrackKind::Instrument);
    p.selected_track = empty;
    delete_selected(&mut p, &mut v);
    assert_eq!(p.tracks.len(), n);
}

#[test]
fn adding_or_removing_tracks_forgets_every_selection_that_names_a_track() {
    let mut p = three();
    let mut s = crate::UiState::new();
    s.tracks.select_only(2, 0);
    s.pitch_ed.region = Some((2, 0));
    marquee(&mut s.tracks, 1, 2);
    p.selected_track = 0;
    crate::menus::apply("New Track with Duplicate Settings", &mut p, &mut s);
    assert_eq!(p.tracks.len(), 4);
    assert!(s.tracks.selection().is_empty());
    assert!(
        s.tracks.range.active().is_none(),
        "the marquee named tracks 1-2"
    );
    assert_eq!(s.pitch_ed.region, None);
    // Delete Track too.
    marquee(&mut s.tracks, 1, 2);
    crate::menus::apply("Delete Track", &mut p, &mut s);
    assert!(s.tracks.range.active().is_none());
}

/// One frame as the app runs it: the View keys, the Tracks area, the key commands.
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

fn key(k: Key) -> Vec<Event> {
    let ev = |pressed| Event::Key {
        key: k,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    vec![ev(true), ev(false)]
}

#[test]
fn delete_and_the_automation_keys_wait_while_a_window_is_open() {
    let mut p = three();
    let mut s = crate::UiState::new();
    let mut h = Harness::new(1400.0, 600.0);
    app_frame(&mut h, &mut p, &mut s, vec![]);
    s.tracks.select_only(0, 0);
    // The New Tracks window has no text field, so egui does not hold the keyboard for it.
    s.track_dialog = Some(crate::track_dialogs::TrackDialog::NewTracks {
        kind: TrackKind::Instrument,
        count: 1,
    });
    let mode = s.auto.mode;
    app_frame(&mut h, &mut p, &mut s, key(Key::Backspace));
    app_frame(&mut h, &mut p, &mut s, key(Key::A));
    assert_eq!(
        p.tracks[0].regions.len(),
        1,
        "Backspace did not delete behind the window"
    );
    assert_eq!(s.auto.mode, mode, "A did not switch Automation Mode");
    // Closed, Backspace deletes the selected region again.
    s.track_dialog = None;
    app_frame(&mut h, &mut p, &mut s, key(Key::Backspace));
    assert!(p.tracks[0].regions.is_empty());
}

#[test]
fn a_window_that_takes_another_ones_place_gives_its_field_the_keyboard() {
    let mut p = three();
    let mut s = crate::UiState::new();
    s.tracks.select_only(0, 0);
    let mut h = Harness::new(1400.0, 600.0);
    let opened = |h: &Harness, k: &str| {
        h.ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::unique(("region_dialog_opened", k)))
                .unwrap_or(false)
        })
    };
    let show = |h: &mut Harness, p: &mut Project, s: &mut crate::UiState| {
        for _ in 0..2 {
            h.frame(vec![], |ui| crate::track_dialogs::show(ui.ctx(), p, s));
        }
    };
    crate::menus::apply("Repeat…", &mut p, &mut s);
    show(&mut h, &mut p, &mut s);
    assert!(
        opened(&h, "repeat"),
        "the Repeat window's field took the keyboard"
    );
    // Rename Regions from the menu takes the Repeat window's place.
    crate::menus::apply("Rename Regions", &mut p, &mut s);
    show(&mut h, &mut p, &mut s);
    assert!(
        !opened(&h, "repeat"),
        "so the next Repeat window focuses its field again"
    );
    assert!(opened(&h, "rename"));
}
