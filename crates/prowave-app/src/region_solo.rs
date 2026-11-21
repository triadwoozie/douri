//! The Solo tool of the Tracks area (Logic Pro: Solo tool, S in the Tool menu): while the
//! button is held down on a region, the song plays from where it was pressed with only that
//! region heard; letting go stops it again (or goes on playing everything, when the song was
//! playing before).
//!
//! The region is kept in `Project::region_solo` (not saved, not an undo step). The engine is
//! not told anything new: `engine_sync` leaves every other region out of the song it sends
//! while it is set, as it leaves out muted regions ([`heard`]).

use crate::model::{Project, Region};

/// Whether region `r` (index `ri`) of track `ti` is heard: it is not muted, and no other
/// region is held with the Solo tool.
pub fn heard(p: &Project, ti: usize, ri: usize, r: &Region) -> bool {
    !r.params.mute && p.region_solo.is_none_or(|x| x == (ti, ri))
}

/// The Solo tool is pressed on region `r` of track `t` at `at` (quarter notes): the song plays
/// from there with that region alone. Returns whether the song was playing before, to be
/// given back to [`end`]; `None` while recording, when nothing changes.
pub fn begin(p: &mut Project, (t, r): (usize, usize), at: f64) -> Option<bool> {
    if p.recording || p.tracks.get(t).and_then(|x| x.regions.get(r)).is_none() {
        return None;
    }
    let was = p.playing;
    p.region_solo = Some((t, r));
    p.position_beats = at.max(0.0);
    p.playing = true;
    p.song_rev += 1;
    Some(was)
}

/// The button is let go: every region is heard again, and the song stops unless it was
/// playing before the press.
pub fn end(p: &mut Project, was_playing: bool) {
    p.region_solo = None;
    p.playing = was_playing;
    p.song_rev += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackKind;

    #[test]
    fn only_the_held_region_is_heard_and_letting_go_gives_everything_back() {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        let r = p.tracks[0]
            .regions
            .first()
            .cloned()
            .unwrap_or_else(|| crate::model::Region {
                name: "A".into(),
                start_bar: 1.0,
                len_bars: 1.0,
                notes: vec![crate::model::Note {
                    start: 0.0,
                    len: 1.0,
                    pitch: 60,
                    vel: 100,
                    mute: false,
                    lyric: crate::lyrics::Lyric::NONE,
                }],
                controls: Vec::new(),
                folder: None,
                audio: None,
                missing: None,
                params: Default::default(),
                generated: None,
            });
        p.tracks[0].regions = vec![r.clone(), r.clone()];
        p.tracks[1].regions = vec![r.clone()];
        let notes = |p: &Project| crate::engine_sync::song_notes_for_test(p).len();
        assert_eq!(notes(&p), 3);
        let rev = p.song_rev;
        let was = begin(&mut p, (0, 1), 6.0).expect("pressed");
        assert!(!was);
        assert!(p.playing);
        assert_eq!(p.position_beats, 6.0);
        assert!(p.song_rev > rev, "the engine gets the song again");
        assert_eq!(notes(&p), 1, "the held region alone");
        assert!(heard(&p, 0, 1, &p.tracks[0].regions[1]));
        assert!(!heard(&p, 0, 0, &p.tracks[0].regions[0]));
        assert!(!heard(&p, 1, 0, &p.tracks[1].regions[0]));
        end(&mut p, was);
        assert!(!p.playing);
        assert_eq!(notes(&p), 3);
        // While recording the tool does nothing.
        p.recording = true;
        assert!(begin(&mut p, (0, 0), 0.0).is_none());
        assert!(p.region_solo.is_none());
    }
}

#[cfg(test)]
mod ui_tests {
    use crate::marquee::rig::{midi, Rig};
    use crate::model::{Project, TrackKind};
    use crate::tracks_area::TrackTool;
    use egui::Modifiers;

    #[test]
    fn holding_the_solo_tool_on_a_region_plays_it_alone_from_there() {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        p.tracks[0].regions = vec![midi("A", 1.0, 4.0)];
        p.tracks[1].regions = vec![midi("B", 1.0, 4.0)];
        let mut r = Rig::new(p);
        r.s.tracks.tool = TrackTool::Solo;
        let all = crate::engine_sync::song_notes_for_test(&r.p).len();
        let at = r.at(6.0, 1);
        r.press(at, Modifiers::NONE);
        assert_eq!(r.p.region_solo, Some((1, 0)));
        assert!(r.p.playing);
        assert_eq!(r.p.position_beats, 6.0);
        assert_eq!(crate::engine_sync::song_notes_for_test(&r.p).len(), all / 2);
        assert!(
            r.s.tracks.selection().is_empty(),
            "the Solo tool selects nothing"
        );
        r.release(at);
        assert_eq!(r.p.region_solo, None);
        assert!(!r.p.playing, "it stops when let go");
        assert_eq!(crate::engine_sync::song_notes_for_test(&r.p).len(), all);
        assert_eq!(r.spans(1), vec![(1.0, 5.0)], "nothing moved");
    }
}
