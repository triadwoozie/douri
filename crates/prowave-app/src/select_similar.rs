//! Edit > Select > Similar Regions (Shift-S), Equal Regions (Shift-E) and Same Colored Regions
//! (Shift-C), as Logic Pro has them (Logic: Select Similar / Equal / Equal Colored
//! Regions/Events). Each one takes the selected regions as the model and selects, on every
//! track that has a row in the Tracks area, each region that matches one of them; the models
//! stay selected, and the first of them stays the primary one.
//!
//! What matches, following Apple's table of similar and identical regions:
//! - Similar: the same type of region, audio or MIDI. (Pattern and Drummer regions are MIDI
//!   regions; a region whose file is missing is an audio region.)
//! - Equal: a MIDI region with the same notes and controller events and the same length; an
//!   audio region that plays the same part of the same file (the same file, the same start in
//!   it, the same length).
//! - Same Colored: the same colour as shown, the region's own or its track's.
//!
//! The selection is not part of the project, so none of these makes an undo step.
use crate::model::{Project, Region};
use crate::tracks_area::TracksView;
use crate::UiState;

/// The entries run here.
const LABELS: &[&str] = &[
    "Select Similar Regions",
    "Select Equal Regions",
    "Select Same Colored Regions",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Match {
    Similar,
    Equal,
    Colored,
}

fn match_of(label: &str) -> Option<Match> {
    match label {
        "Select Similar Regions" => Some(Match::Similar),
        "Select Equal Regions" => Some(Match::Equal),
        "Select Same Colored Regions" => Some(Match::Colored),
        _ => None,
    }
}

/// Whether `label` is one of the entries run here.
pub(crate) fn handles(label: &str) -> bool {
    LABELS.contains(&label)
}

/// The selected regions that are there.
fn models(p: &Project, v: &TracksView) -> Vec<(usize, usize)> {
    v.selection()
        .into_iter()
        .filter(|&(t, r)| p.tracks.get(t).is_some_and(|x| r < x.regions.len()))
        .collect()
}

/// Whether the entry can be chosen now (a region is selected); `None` when it is not one of
/// these.
pub(crate) fn enabled(label: &str, p: &Project, s: &UiState) -> Option<bool> {
    match_of(label)?;
    Some(!models(p, &s.tracks).is_empty())
}

/// Lengths and positions within this are the same.
const EPS: f64 = 1e-6;

/// Whether two regions have the same content (Logic: identical regions).
fn equal(a: &Region, b: &Region) -> bool {
    if (f64::from(a.len_bars) - f64::from(b.len_bars)).abs() > EPS {
        return false;
    }
    match (a.is_audio(), b.is_audio()) {
        (true, true) => {
            let source = |r: &Region| {
                r.audio
                    .as_ref()
                    .map(|x| x.file.path.clone())
                    .or_else(|| r.missing.as_ref().map(|m| m.source.clone()))
            };
            source(a).is_some()
                && source(a) == source(b)
                && (a.params.offset_secs - b.params.offset_secs).abs() <= EPS
        }
        (false, false) => a.notes == b.notes && a.controls == b.controls,
        _ => false,
    }
}

/// Whether region `(t, r)` matches the model `(mt, mr)`.
fn matches(p: &Project, how: Match, (mt, mr): (usize, usize), (t, r): (usize, usize)) -> bool {
    let (mtrack, track) = (&p.tracks[mt], &p.tracks[t]);
    let (m, x) = (&mtrack.regions[mr], &track.regions[r]);
    match how {
        Match::Similar => m.is_audio() == x.is_audio(),
        Match::Equal => equal(m, x),
        Match::Colored => {
            m.params.color.unwrap_or(mtrack.color) == x.params.color.unwrap_or(track.color)
        }
    }
}

/// The regions `how` selects: the models first, then every region on a track with a row that
/// matches one of them, in track and region order.
fn picked(p: &Project, v: &TracksView, show_hidden: bool, how: Match) -> Vec<(usize, usize)> {
    let models = models(p, v);
    let mut out = models.clone();
    // The primary one first, so it stays the primary one.
    if let Some(primary) = v.selected.filter(|x| models.contains(x)) {
        out.retain(|x| *x != primary);
        out.insert(0, primary);
    }
    for t in 0..p.tracks.len() {
        if crate::track_commands::takes_no_room(p, show_hidden, t) {
            continue;
        }
        for r in 0..p.tracks[t].regions.len() {
            if !out.contains(&(t, r)) && models.iter().any(|&m| matches(p, how, m, (t, r))) {
                out.push((t, r));
            }
        }
    }
    out
}

/// Runs `label` if it is one of these; returns whether it was.
pub(crate) fn apply(label: &str, p: &mut Project, s: &mut UiState) -> bool {
    let Some(how) = match_of(label) else {
        return false;
    };
    let sel = picked(p, &s.tracks, s.tracks.show_hidden, how);
    if !sel.is_empty() {
        s.tracks.select_many(&sel);
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::arrange_select::tests::{press, region};
    use crate::model::{Note, Project, TrackKind};
    use crate::ui_test::Harness;
    use crate::UiState;
    use egui::{Key, Modifiers};
    use std::path::Path;

    fn note(pitch: u8, start: f32) -> Note {
        Note {
            pitch,
            start,
            len: 0.5,
            vel: 100,
            ..Note::default()
        }
    }

    /// Track 0 (MIDI): "M1" and "M2" with the same notes and length, "M3" with other notes,
    /// "M4" with M1's notes but longer. Track 1 (audio): "A1" and "A2" playing the same part of
    /// a.wav, "A3" a later part of it, "B1" b.wav. Track 2 (MIDI, hidden): "M5", a copy of M1.
    fn rig(dir: &Path) -> (Project, UiState) {
        let mut p = Project::empty();
        p.tracks.clear();
        p.add_track(TrackKind::Instrument);
        p.add_track(TrackKind::Audio);
        p.add_track(TrackKind::Instrument);
        let notes = vec![note(60, 0.0), note(64, 1.0)];
        let midi = |name: &str, bar: f32, len: f32, notes: &[Note]| {
            let mut r = region(name, bar, len);
            r.notes = notes.to_vec();
            r
        };
        p.tracks[0].regions = vec![
            midi("M1", 1.0, 1.0, &notes),
            midi("M2", 5.0, 1.0, &notes),
            midi("M3", 9.0, 1.0, &[note(62, 0.0)]),
            midi("M4", 13.0, 2.0, &notes),
        ];
        p.tracks[2].regions = vec![midi("M5", 1.0, 1.0, &notes)];
        p.tracks[2].hidden = true;
        let file = |name: &str| {
            let path = dir.join(name);
            prowave_engine::write_wav(&path, &vec![0.1; 48_000 * 2 * 4], 48_000).unwrap();
            prowave_engine::AudioFile::open(&path).unwrap()
        };
        let (a, b) = (file("a.wav"), file("b.wav"));
        let audio = |name: &str, bar: f32, f: &prowave_engine::AudioFile, offset: f64| {
            let mut r = region(name, bar, 1.0);
            r.audio = Some(crate::model::AudioRegion {
                file: f.clone(),
                peaks: Default::default(),
                flex: None,
                start_frame: 0,
                len_frames: None,
            });
            r.params.offset_secs = offset;
            r
        };
        p.tracks[1].regions = vec![
            audio("A1", 1.0, &a, 0.0),
            audio("A2", 3.0, &a, 0.0),
            audio("A3", 5.0, &a, 2.0),
            audio("B1", 7.0, &b, 0.0),
        ];
        p.selected_track = 0;
        (p, UiState::new())
    }

    fn names(p: &Project, s: &UiState) -> Vec<String> {
        let mut out: Vec<String> = s
            .tracks
            .selection()
            .into_iter()
            .map(|(t, r)| p.tracks[t].regions[r].name.clone())
            .collect();
        out.sort();
        out
    }

    const SHIFT: Modifiers = Modifiers::SHIFT;

    #[test]
    fn shift_e_selects_the_regions_equal_to_the_selected_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, mut s) = rig(dir.path());
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        // Nothing selected: greyed, and the key does nothing.
        assert!(!crate::menus::enabled("Select Equal Regions", p, s));
        press(h, p, s, Key::E, SHIFT);
        assert!(s.tracks.selection().is_empty());
        // M1: the same notes and length (M2); not M3 (other notes), M4 (longer), or M5 (its
        // track is hidden).
        s.tracks.select_only(0, 0);
        assert!(crate::menus::enabled("Select Equal Regions", p, s));
        press(h, p, s, Key::E, SHIFT);
        assert_eq!(names(p, s), vec!["M1", "M2"]);
        assert_eq!(s.tracks.selected, Some((0, 0)), "the model stays primary");
        // With hidden tracks shown, M5 is there to select.
        s.tracks.show_hidden = true;
        s.tracks.select_only(0, 0);
        press(h, p, s, Key::E, SHIFT);
        assert_eq!(names(p, s), vec!["M1", "M2", "M5"]);
        // A2: the same part of the same file (A1); not A3 (later in it) or B1 (another file).
        s.tracks.select_only(1, 1);
        press(h, p, s, Key::E, SHIFT);
        assert_eq!(names(p, s), vec!["A1", "A2"]);
        assert_eq!(s.tracks.selected, Some((1, 1)));
        // Two models: the regions equal to either.
        s.tracks.select_many(&[(0, 2), (1, 3)]);
        press(h, p, s, Key::E, SHIFT);
        assert_eq!(names(p, s), vec!["B1", "M3"]);
    }

    #[test]
    fn shift_s_selects_the_regions_of_the_same_type() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, mut s) = rig(dir.path());
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        s.tracks.select_only(0, 2);
        press(h, p, s, Key::S, SHIFT);
        assert_eq!(names(p, s), vec!["M1", "M2", "M3", "M4"]);
        s.tracks.select_only(1, 3);
        press(h, p, s, Key::S, SHIFT);
        assert_eq!(names(p, s), vec!["A1", "A2", "A3", "B1"]);
        // A region whose file is missing is an audio region too.
        let gone = p.tracks[1].regions[0].clone();
        let mut missing = gone.clone();
        missing.name = "X".into();
        missing.missing = Some(crate::model::MissingAudio {
            source: gone.audio.as_ref().unwrap().file.path.clone(),
            flex: None,
        });
        missing.audio = None;
        p.tracks[1].regions.push(missing);
        s.tracks.select_only(1, 0);
        press(h, p, s, Key::S, SHIFT);
        assert_eq!(names(p, s), vec!["A1", "A2", "A3", "B1", "X"]);
    }

    #[test]
    fn shift_c_selects_the_regions_of_the_same_colour_and_c_is_still_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, mut s) = rig(dir.path());
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        let orange = crate::theme::TRACK_PALETTE[3];
        p.tracks[0].regions[2].params.color = Some(orange);
        p.tracks[1].regions[3].params.color = Some(orange);
        // A region of track 1 without a colour of its own shows its track's colour.
        p.tracks[1].color = crate::theme::TRACK_PALETTE[5];
        p.tracks[0].color = crate::theme::TRACK_PALETTE[5];
        s.tracks.select_only(0, 2);
        let cycle = p.cycle;
        press(h, p, s, Key::C, SHIFT);
        assert_eq!(names(p, s), vec!["B1", "M3"]);
        assert_eq!(p.cycle, cycle, "Shift-C is not Cycle");
        s.tracks.select_only(0, 0);
        press(h, p, s, Key::C, SHIFT);
        assert_eq!(names(p, s), vec!["A1", "A2", "A3", "M1", "M2", "M4"]);
        press(h, p, s, Key::C, Modifiers::NONE);
        assert_eq!(p.cycle, !cycle, "C is still Cycle");
    }

    #[test]
    fn they_are_in_edit_select_wait_over_the_piano_roll_and_make_no_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, mut s) = rig(dir.path());
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        let select = crate::menus::submenu("Select").unwrap();
        for label in [
            "Select Similar Regions",
            "Select Equal Regions",
            "Select Same Colored Regions",
        ] {
            assert!(select.contains(&label), "{label}");
            assert!(crate::menus::implemented(label), "{label}");
        }
        for (label, key) in [
            ("Select Similar Regions", Key::S),
            ("Select Equal Regions", Key::E),
            ("Select Same Colored Regions", Key::C),
        ] {
            let c = crate::key_commands::combo_of(label).unwrap();
            assert!(c.key == key && c.shift && !c.cmd && !c.alt && !c.ctrl);
        }
        // Over the Piano Roll the keys wait (Shift-E is the Piano Roll's own there).
        s.tracks.select_only(0, 0);
        s.piano_focus = true;
        press(h, p, s, Key::S, SHIFT);
        assert_eq!(names(p, s), vec!["M1"]);
        s.piano_focus = false;
        let mut history = crate::undo::History::new(p);
        press(h, p, s, Key::S, SHIFT);
        assert_eq!(names(p, s).len(), 4);
        assert!(!history.record(p));
        // The menu runs the same thing.
        s.tracks.select_only(1, 0);
        crate::menus::apply("Select Equal Regions", p, s);
        assert_eq!(names(p, s), vec!["A1", "A2"]);
    }
}
