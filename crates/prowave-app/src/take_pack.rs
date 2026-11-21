//! Pack Take Folder (Logic Pro: Functions > Folder > Pack Take Folder): the selected regions
//! of one track become the takes of one take folder (see `takes.rs`).
//!
//! What is packed: two or more MIDI regions, all on one track, none of them a take folder, a
//! Pattern or Drummer region, a looped region, and all with the same Region Inspector
//! settings (the folder has one set, so nothing is lost). Audio regions are not packed: a
//! take holds notes and controllers only.
//!
//! The folder runs from the earliest start to the latest end. Each region is one take, the
//! earliest first, and keeps its place in time. The comp plays, in each stretch, the latest
//! take that covers it (as a cycle recording plays its last take), so packing does not change
//! what is heard where the regions did not overlap.

use crate::model::{Project, Take, TakeFolder};
use crate::tracks_area::TracksView;

/// The track and the regions (earliest first) the selection packs into, if it can.
fn plan(p: &Project, v: &TracksView) -> Option<(usize, Vec<usize>)> {
    let sel = v.selection();
    let t = sel.first()?.0;
    if sel.len() < 2 || sel.iter().any(|&(st, _)| st != t) {
        return None;
    }
    let regions = &p.tracks.get(t)?.regions;
    let mut order: Vec<usize> = sel.iter().map(|&(_, r)| r).collect();
    let first = regions.get(order[0])?;
    for &r in &order {
        let reg = regions.get(r)?;
        if reg.is_audio()
            || reg.folder.is_some()
            || reg.generated.is_some()
            || reg.params.loop_beats > 0.0
            || reg.params != first.params
        {
            return None;
        }
    }
    order.sort_by(|&a, &b| {
        regions[a]
            .start_bar
            .total_cmp(&regions[b].start_bar)
            .then(a.cmp(&b))
    });
    Some((t, order))
}

/// Whether Pack Take Folder can apply to the selection.
pub(crate) fn can_pack(p: &Project, v: &TracksView) -> bool {
    plan(p, v).is_some()
}

/// Pack Take Folder: the selected regions become one take folder in the place of the earliest,
/// which is then selected. Does nothing when [`can_pack`] is false.
pub(crate) fn pack(p: &mut Project, v: &mut TracksView) {
    let Some((t, order)) = plan(p, v) else {
        return;
    };
    let regions = &p.tracks[t].regions;
    let base = p.region_start_beats(&regions[order[0]]);
    let end = order
        .iter()
        .map(|&r| p.region_end_beats(&regions[r]))
        .fold(base, f64::max);
    let len = (end - base) as f32;
    let mut takes = Vec::new();
    let mut spans = Vec::new();
    for &r in &order {
        let reg = &regions[r];
        let shift = (p.region_start_beats(reg) - base) as f32;
        let own = p.region_len_beats(reg) as f32;
        // What the region plays: what starts inside it.
        let inside = |s: f32| (0.0..own).contains(&s);
        takes.push(Take {
            notes: reg
                .notes
                .iter()
                .filter(|n| inside(n.start))
                .map(|n| crate::model::Note {
                    start: n.start + shift,
                    ..*n
                })
                .collect(),
            controls: reg
                .controls
                .iter()
                .filter(|c| inside(c.start))
                .map(|c| crate::model::Control {
                    start: c.start + shift,
                    ..*c
                })
                .collect(),
        });
        spans.push((shift, shift + own));
    }
    let mut folder = TakeFolder::new(takes, len);
    // Later takes over earlier ones where they overlap.
    for (k, &(a, b)) in spans.iter().enumerate() {
        folder.swipe(k, a, b);
    }
    let mut packed = regions[order[0]].clone();
    let end_bar = p.beats_to_bar(end) as f32 + 1.0;
    packed.len_bars = end_bar - packed.start_bar;
    folder.apply(&mut packed.notes, &mut packed.controls);
    packed.folder = Some(folder);
    let mut sorted = order;
    sorted.sort_unstable();
    let regions = &mut p.tracks[t].regions;
    for &r in sorted.iter().rev() {
        regions.remove(r);
    }
    let at = sorted[0];
    regions.insert(at, packed);
    p.song_rev += 1;
    v.select_only(t, at);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marquee::rig::{midi, note, Rig};
    use crate::model::{Project, TrackKind};
    use crate::undo::History;

    /// Track 0: "A" bars 1-3, "B" bars 2-4 (overlapping A), "C" bars 6-7; track 1: "D" bars
    /// 1-2.
    fn project() -> Project {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        p.tracks[0].regions = vec![
            midi("A", 1.0, 2.0),
            midi("B", 2.0, 2.0),
            midi("C", 6.0, 1.0),
        ];
        p.tracks[1].regions = vec![midi("D", 1.0, 1.0)];
        p.selected_track = 0;
        p
    }

    fn starts(notes: &[crate::model::Note]) -> Vec<f32> {
        notes.iter().map(|n| n.start).collect()
    }

    #[test]
    fn pack_makes_one_take_per_region_and_plays_the_latest_where_they_overlap() {
        let mut p = project();
        let mut v = TracksView::default();
        v.select_many(&[(0, 2), (0, 0), (0, 1)]);
        assert!(can_pack(&p, &v));
        pack(&mut p, &mut v);
        let regs = &p.tracks[0].regions;
        assert_eq!(regs.len(), 1);
        let f = &regs[0];
        assert_eq!((f.start_bar, f.len_bars), (1.0, 6.0));
        assert_eq!(f.name, "A");
        let folder = f.folder.as_ref().expect("a take folder");
        assert_eq!(folder.takes.len(), 3);
        assert_eq!(folder.len, 24.0);
        // Each take keeps its place: B starts at beat 4, C at beat 20.
        assert_eq!(folder.takes[1].notes[0].start, 4.0);
        assert_eq!(starts(&folder.takes[2].notes), vec![20.0, 21.0, 22.0, 23.0]);
        // A plays until B begins, B to its end, C after (the stretch before C plays nothing).
        let comp: Vec<(f32, f32, usize)> = folder
            .comp
            .iter()
            .map(|c| (c.start, c.end, c.take))
            .collect();
        assert_eq!(comp, vec![(0.0, 4.0, 0), (4.0, 12.0, 1), (12.0, 24.0, 2)]);
        // The region plays the comp: A's first bar, all of B, all of C.
        let expected: Vec<f32> = (0..4)
            .chain(4..12)
            .chain(20..24)
            .map(|b| b as f32)
            .collect();
        assert_eq!(starts(&f.notes), expected);
        assert_eq!(v.selection(), vec![(0, 0)]);
        assert_eq!(
            p.tracks[1].regions.len(),
            1,
            "the other track is left alone"
        );
    }

    #[test]
    fn pack_needs_two_midi_regions_on_one_track_with_the_same_settings() {
        let p = project();
        let mut v = TracksView::default();
        v.select_many(&[(0, 0)]);
        assert!(!can_pack(&p, &v), "one region");
        v.select_many(&[(0, 0), (1, 0)]);
        assert!(!can_pack(&p, &v), "two tracks");
        let mut q = project();
        q.tracks[0].regions[1].params.transpose = 12;
        v.select_many(&[(0, 0), (0, 1)]);
        assert!(!can_pack(&q, &v), "different settings");
        let mut q = project();
        q.tracks[0].regions[1].params.loop_beats = 4.0;
        assert!(!can_pack(&q, &v), "a looped region");
        let mut q = project();
        q.tracks[0].regions[1].folder = Some(TakeFolder::new(
            vec![Take {
                notes: vec![note(0.0)],
                controls: Vec::new(),
            }],
            8.0,
        ));
        assert!(!can_pack(&q, &v), "already a folder");
        let mut q = project();
        q.tracks[0].regions[1].missing = Some(crate::model::MissingAudio {
            source: "gone.wav".into(),
            flex: None,
        });
        assert!(!can_pack(&q, &v), "an audio region");
        assert!(can_pack(&p, &v));
    }

    #[test]
    fn the_edit_menu_entry_packs_in_one_undo_step() {
        let mut r = Rig::new(project());
        let mut hist = History::new(&r.p);
        assert!(!crate::menus::enabled("Pack Take Folder", &r.p, &r.s));
        r.s.tracks.select_many(&[(0, 0), (0, 2)]);
        assert!(crate::menus::enabled("Pack Take Folder", &r.p, &r.s));
        crate::menus::apply("Pack Take Folder", &mut r.p, &mut r.s);
        r.frame(vec![]);
        assert_eq!(r.p.tracks[0].regions.len(), 2);
        assert!(r.p.tracks[0].regions[0].folder.is_some());
        assert!(hist.record(&r.p));
        assert!(hist.undo(&mut r.p));
        assert!(!hist.can_undo(), "one step");
        assert_eq!(r.p.tracks[0].regions.len(), 3);
        assert!(r.p.tracks[0].regions.iter().all(|x| x.folder.is_none()));
    }
}
