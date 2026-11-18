//! Edit > Cut, Copy, Paste, Split at Playhead and Join on the selected regions (Logic Pro: Edit
//! menu of the Tracks area). Select All is in `tracks_area.rs`, with the selection itself.
//!
//! Logic behaviour followed here:
//! - Copy and Cut take every selected region; Paste puts them on the selected track at the
//!   playhead, keeping the distances between them.
//! - Split at Playhead cuts every selected region that the playhead is inside of, as the
//!   Scissors tool does (`Project::split_region`): a note that crosses the cut ends there, the
//!   right half of an audio region plays on from the same place in its file, and the right
//!   pieces are what gets selected.
//! - Join merges the selected regions of each track into one region that spans them all, as the
//!   Join tool does (`Project::join_group`): MIDI regions keep each note where it was; audio
//!   regions join when they are consecutive parts of one file. The joined region takes the
//!   earliest region's name and settings.
//!
//! Pattern, Drummer and take-folder regions, and reversed audio, are not cut or joined, because
//! their notes or sound are made from settings or takes.

use crate::model::{Project, Region, TrackKind};
use crate::tracks_area::TracksView;

/// The regions the clipboard holds (Edit > Copy / Cut), earliest first.
pub(crate) type RegionClip = Vec<Region>;

/// A MIDI region with its own notes: the only kind that can be split or joined.
fn plain_midi(r: &Region) -> bool {
    !r.is_audio() && r.folder.is_none() && r.generated.is_none()
}

/// Copies every selected region to the clipboard. Returns whether there was one.
pub(crate) fn copy_selected(p: &Project, v: &TracksView, clip: &mut RegionClip) -> bool {
    let mut picked: Vec<Region> = v
        .selection()
        .into_iter()
        .filter_map(|(t, r)| p.tracks.get(t)?.regions.get(r).cloned())
        .collect();
    if picked.is_empty() {
        return false;
    }
    picked.sort_by(|a, b| a.start_bar.total_cmp(&b.start_bar));
    *clip = picked;
    true
}

/// Copies the selected regions and removes them from their tracks.
pub(crate) fn cut_selected(p: &mut Project, v: &mut TracksView, clip: &mut RegionClip) {
    if copy_selected(p, v, clip) {
        crate::tracks_area::delete_selected(p, v);
    }
}

/// Whether the clipboard regions fit the selected track: audio regions go on audio tracks, the
/// others on tracks that play MIDI.
pub(crate) fn can_paste(p: &Project, clip: &RegionClip) -> bool {
    let Some(track) = p.tracks.get(p.selected_track) else {
        return false;
    };
    !clip.is_empty()
        && track.kind.has_regions()
        && clip
            .iter()
            .all(|r| r.is_audio() == (track.kind == TrackKind::Audio))
}

/// Puts copies of the clipboard regions on the selected track, the earliest at the playhead,
/// and selects them.
pub(crate) fn paste_at_playhead(p: &mut Project, v: &mut TracksView, clip: &RegionClip) {
    if !can_paste(p, clip) {
        return;
    }
    let base = clip.iter().map(|r| r.start_bar).fold(f32::MAX, f32::min);
    let playhead_bar = p.beats_to_bar(p.position_beats) as f32 + 1.0;
    let t = p.selected_track;
    let Some(track) = p.tracks.get_mut(t) else {
        return;
    };
    let mut made = Vec::new();
    for r in clip {
        track.regions.push(Region {
            start_bar: playhead_bar + (r.start_bar - base),
            ..r.clone()
        });
        made.push((t, track.regions.len() - 1));
    }
    v.select_many(&made);
    p.song_rev += 1;
}

/// Whether every region of a drag (track, region, start) would find a track of its kind
/// `rows` tracks further down (up when negative): audio regions go on audio tracks, the others
/// on tracks that play MIDI.
pub(crate) fn fits_rows(
    p: &Project,
    group: &[(usize, usize, f64)],
    rows: isize,
    show_hidden: bool,
) -> bool {
    group.iter().all(|&(t, r, _)| {
        let Some(region) = p.tracks.get(t).and_then(|tr| tr.regions.get(r)) else {
            return false;
        };
        let Some(i) = t.checked_add_signed(rows) else {
            return false;
        };
        let Some(to) = p.tracks.get(i) else {
            return false;
        };
        // Not onto a track that has no row (folded away, or hidden): it would vanish.
        !crate::track_commands::takes_no_room(p, show_hidden, i)
            && to.kind.has_regions()
            && region.is_audio() == (to.kind == TrackKind::Audio)
    })
}

/// A drag of `group` by `delta` quarter notes, stopped where its earliest region reaches the
/// start of the song: the regions keep their distances from each other, as in Logic.
pub(crate) fn clamp_delta(group: &[(usize, usize, f64)], delta: f64) -> f64 {
    let earliest = group.iter().map(|g| g.2).fold(f64::INFINITY, f64::min);
    if earliest.is_finite() {
        delta.max(-earliest)
    } else {
        delta
    }
}

/// The bar (1-based) a region that started at `start` lands on, moved by `delta` quarter notes
/// (never before the start of the song).
pub(crate) fn landing_bar(p: &Project, start: f64, delta: f64) -> f32 {
    p.beats_to_bar((start + delta).max(0.0)) as f32 + 1.0
}

/// The end of a Pointer-tool drag of regions, as in Logic. Each region of `group` (track,
/// region, its start in quarter notes when the drag began) lands `delta` quarter notes later
/// and `rows` tracks further down. With `copy` (Option held when the button is let go) the
/// regions stay where they were and copies of them land there instead, the same in everything
/// but their place. The automation under them goes along (Automation: Move with Regions), and
/// with `xfade` (Drag: X-Fade) an audio region dropped over another crossfades with it.
/// Returns where the landed regions are now, to be selected.
pub(crate) fn drop_regions(
    p: &mut Project,
    auto: &crate::automation::AutoView,
    group: &[(usize, usize, f64)],
    delta: f64,
    rows: isize,
    copy: bool,
    xfade: bool,
) -> Vec<(usize, usize)> {
    let group: Vec<(usize, usize, f64)> = group
        .iter()
        .copied()
        .filter(|&(t, r, _)| p.tracks.get(t).is_some_and(|tr| r < tr.regions.len()))
        .collect();
    // The drag that asked for `rows` already kept to the rows on screen.
    let rows = if fits_rows(p, &group, rows, true) {
        rows
    } else {
        0
    };
    let delta = clamp_delta(&group, delta);
    let mut landed: Vec<(usize, usize)> = Vec::new();
    if !copy && rows == 0 {
        // A plain move: each region takes its new place on its own track.
        for &(t, r, s0) in &group {
            p.tracks[t].regions[r].start_bar = landing_bar(p, s0, delta);
        }
        crate::automation_ui::follow_moves(p, auto, &group);
        landed.extend(group.iter().map(|&(t, r, _)| (t, r)));
    } else {
        let mut made: Vec<(usize, Region)> = Vec::new();
        let mut spans = Vec::new();
        for &(t, r, s0) in &group {
            // The region itself stays where it began (the drag may have moved it along).
            let back = p.beats_to_bar(s0) as f32 + 1.0;
            p.tracks[t].regions[r].start_bar = back;
            let to = t.saturating_add_signed(rows);
            let mut region = p.tracks[t].regions[r].clone();
            region.start_bar = landing_bar(p, s0, delta);
            let len = p.region_len_beats(&p.tracks[t].regions[r]);
            spans.push((t, s0, len, to, p.region_start_beats(&region)));
            made.push((to, region));
        }
        if !copy {
            // Moved to other tracks: taken off their own ones, the highest index first so
            // the lower ones stay valid.
            let mut gone: Vec<(usize, usize)> = group.iter().map(|&(t, r, _)| (t, r)).collect();
            gone.sort_unstable();
            gone.dedup();
            for &(t, r) in gone.iter().rev() {
                p.tracks[t].regions.remove(r);
            }
        }
        for (to, region) in made {
            p.tracks[to].regions.push(region);
            landed.push((to, p.tracks[to].regions.len() - 1));
        }
        crate::automation_ui::follow_drops(p, auto, &spans, !copy);
    }
    for &(t, r) in &landed {
        if p.tracks[t].kind == TrackKind::Audio && crate::tracks_area::overlaps_another(p, t, r) {
            p.tracks[t].regions[r].params.crossfade = xfade;
        }
    }
    p.song_rev += 1;
    landed
}

/// Split at Playhead: every selected region the playhead is inside of is cut. The right pieces
/// become the selection.
pub(crate) fn split_at_playhead(p: &mut Project, v: &mut TracksView) {
    let at = p.position_beats;
    let mut targets: Vec<(usize, usize)> = v
        .selection()
        .into_iter()
        .filter(|&(t, r)| p.can_split_region(t, r, at))
        .collect();
    // Highest index first on each track: a cut puts its right piece right after the left one,
    // so the lower indexes stay valid, and the pieces already made move up by one.
    targets.sort_unstable();
    targets.reverse();
    let mut made: Vec<(usize, usize)> = Vec::new();
    for (t, r) in targets {
        if p.split_region(t, r, at) {
            for m in made.iter_mut().filter(|m| m.0 == t && m.1 > r) {
                m.1 += 1;
            }
            made.push((t, r + 1));
        }
    }
    if !made.is_empty() {
        v.select_many(&made);
    }
}

/// The groups Join merges: per track, the indexes of its selected regions, when they can be
/// joined (two or more; see `Project::can_join_group`).
fn join_groups(p: &Project, v: &TracksView) -> Vec<(usize, Vec<usize>)> {
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for (t, r) in v.selection() {
        match groups.iter_mut().find(|(gt, _)| *gt == t) {
            Some((_, rs)) => rs.push(r),
            None => groups.push((t, vec![r])),
        }
    }
    groups.retain(|(t, rs)| p.can_join_group(*t, rs));
    groups
}

/// Whether Join does anything to the selection.
pub(crate) fn can_join(p: &Project, v: &TracksView) -> bool {
    !join_groups(p, v).is_empty()
}

/// Join: merges each group of selected regions into one region on its track. The joined region
/// is selected.
pub(crate) fn join_selected(p: &mut Project, v: &mut TracksView) {
    let mut made = Vec::new();
    for (t, rs) in join_groups(p, v) {
        if let Some(at) = p.join_group(t, &rs) {
            made.push((t, at));
        }
    }
    if !made.is_empty() {
        v.select_many(&made);
    }
}

/// Functions > Move to Playhead: the earliest selected region starts at the playhead, the others
/// keep their distances from it (each stays on its track).
pub(crate) fn move_to_playhead(p: &mut Project, v: &TracksView) {
    let sel = v.selection();
    let base = sel
        .iter()
        .filter_map(|&(t, r)| p.tracks.get(t)?.regions.get(r))
        .map(|reg| reg.start_bar)
        .fold(f32::MAX, f32::min);
    if sel.is_empty() || base == f32::MAX {
        return;
    }
    let playhead_bar = p.beats_to_bar(p.position_beats) as f32 + 1.0;
    for (t, r) in sel {
        if let Some(reg) = p.tracks.get_mut(t).and_then(|tr| tr.regions.get_mut(r)) {
            reg.start_bar = playhead_bar + (reg.start_bar - base);
        }
    }
    p.song_rev += 1;
}

/// Edit > Invert Selection (Shift-I): every region that was not selected, and none that was.
pub(crate) fn invert_selection(p: &Project, v: &mut TracksView) {
    let all: Vec<(usize, usize)> = p
        .tracks
        .iter()
        .enumerate()
        .flat_map(|(t, tr)| (0..tr.regions.len()).map(move |r| (t, r)))
        .filter(|&(t, r)| !v.is_selected(t, r))
        .collect();
    v.select_many(&all);
}

/// Where the earliest selected region starts, in quarter notes.
pub(crate) fn selection_start(p: &Project, v: &TracksView) -> Option<f64> {
    v.selection()
        .into_iter()
        .filter_map(|(t, r)| p.tracks.get(t)?.regions.get(r))
        .map(|reg| p.region_start_beats(reg))
        .min_by(f64::total_cmp)
}

/// Edit > Select All Following (Shift-F): the selected regions and every region that starts
/// after the earliest of them; with `same_track` (Control-Shift-F) only on the tracks that
/// have a selected region. The selected regions stay selected and the primary one stays so.
pub(crate) fn select_following(p: &Project, v: &mut TracksView, same_track: bool) {
    let Some(from) = selection_start(p, v) else {
        return;
    };
    let was = v.selection();
    let mut all: Vec<(usize, usize)> = v.selected.into_iter().collect();
    for (t, track) in p.tracks.iter().enumerate() {
        if same_track && !was.iter().any(|x| x.0 == t) {
            continue;
        }
        for (r, reg) in track.regions.iter().enumerate() {
            if was.contains(&(t, r)) || p.region_start_beats(reg) >= from - 1e-9 {
                all.push((t, r));
            }
        }
    }
    v.select_many(&all);
}

/// The regions a command on `clicked` acts on: the whole selection when `clicked` is part of
/// it (or there is no `clicked`: a key or a menu), else `clicked` alone, as in Logic.
fn targets(v: &TracksView, clicked: Option<(usize, usize)>) -> Vec<(usize, usize)> {
    match clicked {
        Some((t, r)) if !v.is_selected(t, r) => vec![(t, r)],
        _ => v.selection(),
    }
}

/// Mute On/Off (Control-M, the Mute tool, Mute Region in the right-click menu): the clicked
/// region, or the primary one, changes, and every other region the command acts on takes the
/// same state. Returns whether a region changed.
pub(crate) fn toggle_mute(
    p: &mut Project,
    v: &TracksView,
    clicked: Option<(usize, usize)>,
) -> bool {
    let lead = clicked.or(v.selected);
    let Some(now) = lead
        .and_then(|(t, r)| p.tracks.get(t)?.regions.get(r))
        .map(|reg| reg.params.mute)
    else {
        return false;
    };
    let mut changed = false;
    for (t, r) in targets(v, clicked) {
        if let Some(reg) = p.tracks.get_mut(t).and_then(|tr| tr.regions.get_mut(r)) {
            reg.params.mute = !now;
            changed = true;
        }
    }
    if changed {
        p.song_rev += 1;
    }
    changed
}

/// How many more times a looped region plays when nothing in the project comes after it.
const LOOP_REPEATS_ALONE: f64 = 7.0;

/// Loop On/Off (L): with a looped region in the selection, no selected region loops any more;
/// otherwise each one loops up to the next region on its track, or, with none there, up to
/// the end of the last region of the project (Logic: up to the end of the project). A region
/// with nothing after it anywhere plays [`LOOP_REPEATS_ALONE`] more times.
pub(crate) fn toggle_loop(p: &mut Project, v: &TracksView) -> bool {
    let sel: Vec<(usize, usize)> = v
        .selection()
        .into_iter()
        .filter(|&(t, r)| p.tracks.get(t).is_some_and(|tr| r < tr.regions.len()))
        .collect();
    if sel.is_empty() {
        return false;
    }
    let looped = sel
        .iter()
        .any(|&(t, r)| p.tracks[t].regions[r].params.loop_beats > 0.0);
    let project_end = p
        .tracks
        .iter()
        .flat_map(|t| t.regions.iter())
        .map(|reg| p.region_end_beats(reg))
        .fold(0.0, f64::max);
    for &(t, r) in &sel {
        let beats = if looped {
            0.0
        } else {
            let reg = &p.tracks[t].regions[r];
            let (end, len) = (p.region_end_beats(reg), p.region_len_beats(reg));
            let next = p.tracks[t]
                .regions
                .iter()
                .map(|o| p.region_start_beats(o))
                .filter(|s| *s >= end - 1e-9)
                .min_by(f64::total_cmp);
            match next {
                Some(next) => next - end,
                None if project_end > end + 1e-9 => project_end - end,
                None => len * LOOP_REPEATS_ALONE,
            }
        };
        p.tracks[t].regions[r].params.loop_beats = beats as f32;
    }
    p.song_rev += 1;
    true
}

/// The Scissors tool on region `clicked` at `at` (quarter notes): the region is cut there,
/// and so is every other selected region `at` is inside of when the clicked one is selected
/// (Logic: all selected regions are cut). With `equal` (Option held) the cuts go on after
/// `at` in steps as long as the clicked region's first piece, so the pieces are equal. The
/// pieces to the right of a cut are what gets selected. Returns whether anything was cut.
pub(crate) fn scissors(
    p: &mut Project,
    v: &mut TracksView,
    clicked: (usize, usize),
    at: f64,
    equal: bool,
) -> bool {
    let Some(start) = p
        .tracks
        .get(clicked.0)
        .and_then(|t| t.regions.get(clicked.1))
        .map(|r| p.region_start_beats(r))
    else {
        return false;
    };
    let step = at - start;
    if step < 1e-6 {
        return false;
    }
    let mut regions = targets(v, Some(clicked));
    // Highest index first on each track: the cuts of one region leave the lower ones alone.
    regions.sort_unstable();
    regions.reverse();
    let mut made: Vec<(usize, usize)> = Vec::new();
    for (t, r) in regions {
        let Some(reg) = p.tracks.get(t).and_then(|tr| tr.regions.get(r)) else {
            continue;
        };
        let end = p.region_end_beats(reg);
        let mut cuts = vec![at];
        if equal {
            let mut next = at + step;
            while next < end - 1e-6 {
                cuts.push(next);
                next += step;
            }
        }
        // The last cut first: the piece left of it keeps the region's index.
        let mut n = 0;
        for &cut in cuts.iter().rev() {
            if p.split_region(t, r, cut) {
                n += 1;
            }
        }
        if n > 0 {
            for m in made.iter_mut().filter(|m| m.0 == t && m.1 > r) {
                m.1 += n;
            }
            made.extend((1..=n).map(|k| (t, r + k)));
        }
    }
    if made.is_empty() {
        return false;
    }
    made.sort_unstable();
    v.select_many(&made);
    true
}

/// The Pencil on an empty stretch of track `t` (Logic): an empty MIDI region of one bar from
/// `start` (quarter notes), named after the track. Only instrument and external MIDI tracks
/// take one. Returns the new region's index.
pub(crate) fn pencil_region(p: &mut Project, t: usize, start: f64) -> Option<usize> {
    let track = p.tracks.get(t)?;
    if !matches!(track.kind, TrackKind::Instrument | TrackKind::ExternalMidi) {
        return None;
    }
    let name = track.name.clone();
    let start_bar = p.beats_to_bar(start) as f32 + 1.0;
    let end_bar = p.beats_to_bar(start + p.bar_len_at(start)) as f32 + 1.0;
    let region = crate::model::Region {
        name,
        start_bar,
        len_bars: (end_bar - start_bar).max(1.0 / 64.0),
        notes: Vec::new(),
        controls: Vec::new(),
        folder: None,
        audio: None,
        missing: None,
        params: crate::model::RegionParams::default(),
        generated: None,
    };
    let regions = &mut p.tracks[t].regions;
    regions.push(region);
    p.song_rev += 1;
    Some(p.tracks[t].regions.len() - 1)
}

/// Grid used by Functions > Quantize when the region's Quantize setting is Off: 1/16 note.
pub const DEFAULT_QUANTIZE_GRID: f32 = 0.25;

/// Whether Quantize does anything to the selection: a selected region with notes of its own.
pub(crate) fn can_quantize(p: &Project, v: &TracksView) -> bool {
    v.selection().into_iter().any(|(t, r)| {
        p.tracks
            .get(t)
            .and_then(|tr| tr.regions.get(r))
            .is_some_and(|reg| plain_midi(reg) && !reg.notes.is_empty())
    })
}

/// Functions > Quantize: moves the notes of every selected MIDI region onto the region's
/// Quantize grid (1/16 when it is Off). Destructive, like Logic's Functions > Quantize.
pub(crate) fn quantize_selected(p: &mut Project, v: &TracksView) {
    let mut changed = false;
    for (t, r) in v.selection() {
        let Some(reg) = p.tracks.get_mut(t).and_then(|tr| tr.regions.get_mut(r)) else {
            continue;
        };
        if !plain_midi(reg) || reg.notes.is_empty() {
            continue;
        }
        let grid = reg
            .params
            .quantize
            .filter(|g| *g > 0.0)
            .unwrap_or(DEFAULT_QUANTIZE_GRID);
        let all: Vec<usize> = (0..reg.notes.len()).collect();
        crate::note_edit::quantize(&mut reg.notes, &all, grid, 1.0, 0.0);
        changed = true;
    }
    if changed {
        p.song_rev += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AudioRegion, Note};
    use crate::region_commands::can_split;

    fn test_audio() -> AudioRegion {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../prowave-engine/tests/fixtures");
        AudioRegion {
            file: prowave_engine::AudioFile::open(dir.join("tone.caf")).unwrap(),
            peaks: Default::default(),
            flex: None,
        }
    }

    fn note(start: f32, len: f32) -> Note {
        Note {
            lyric: crate::lyrics::Lyric::NONE,
            mute: false,
            start,
            len,
            pitch: 60,
            vel: 90,
        }
    }

    /// The demo project with one region on track 0: 4 bars with notes at beats 0, 3 and 8.
    fn rig() -> (Project, TracksView) {
        let mut p = Project::demo();
        let region = &mut p.tracks[0].regions[0];
        region.start_bar = 1.0;
        region.len_bars = 4.0;
        region.notes = vec![note(0.0, 1.0), note(3.0, 3.0), note(8.0, 1.0)];
        region.controls.clear();
        p.tracks[0].regions.truncate(1);
        let mut v = TracksView::default();
        v.select_only(0, 0);
        p.selected_track = 0;
        (p, v)
    }

    /// Adds a second region on track 0 at `bar` with the same length and one note.
    fn second_region(p: &mut Project, bar: f32) {
        let mut r = p.tracks[0].regions[0].clone();
        r.start_bar = bar;
        r.len_bars = 2.0;
        r.notes = vec![note(1.0, 1.0)];
        p.tracks[0].regions.push(r);
    }

    #[test]
    fn split_cuts_the_region_and_shortens_the_note_that_crosses() {
        let (mut p, mut v) = rig();
        p.position_beats = 4.0; // bar 2 in 4/4
        assert!(can_split(&p, &v));
        split_at_playhead(&mut p, &mut v);
        let regions = &p.tracks[0].regions;
        assert_eq!(regions.len(), 2);
        let (left, right) = (&regions[0], &regions[1]);
        assert_eq!((left.start_bar, left.len_bars), (1.0, 1.0));
        assert_eq!((right.start_bar, right.len_bars), (2.0, 3.0));
        // Notes at 0 and 3 stay left; the one at 3 (length 3) ends at the cut.
        assert_eq!(left.notes.len(), 2);
        assert!((left.notes[1].len - 1.0).abs() < 1e-6);
        // The note at 8 moves right and starts 4 beats into the right piece.
        assert_eq!(right.notes.len(), 1);
        assert!((right.notes[0].start - 4.0).abs() < 1e-6);
        assert_eq!(v.selected, Some((0, 1)));
    }

    #[test]
    fn split_needs_the_playhead_inside_a_selected_region() {
        let (mut p, v) = rig();
        for at in [0.0, 16.0, 40.0] {
            p.position_beats = at;
            assert!(!can_split(&p, &v), "at {at}");
        }
    }

    #[test]
    fn split_cuts_every_selected_region_under_the_playhead() {
        let (mut p, mut v) = rig();
        second_region(&mut p, 1.0); // also under bar 1
        v.select_many(&[(0, 0), (0, 1)]);
        p.position_beats = 2.0;
        split_at_playhead(&mut p, &mut v);
        assert_eq!(p.tracks[0].regions.len(), 4, "both regions were cut");
        assert_eq!(v.selection().len(), 2, "the two right pieces are selected");
    }

    #[test]
    fn audio_regions_are_split_but_reversed_audio_is_not() {
        let (mut p, mut v) = rig();
        p.position_beats = 4.0;
        p.tracks[0].regions[0].audio = Some(test_audio());
        assert!(can_split(&p, &v));
        p.tracks[0].regions[0].params.reverse = true;
        assert!(!can_split(&p, &v));
        p.tracks[0].regions[0].params.reverse = false;
        split_at_playhead(&mut p, &mut v);
        let right = &p.tracks[0].regions[1];
        assert!(
            right.params.offset_secs > 1.9,
            "the right half plays on from the cut (2 s at 120 bpm): {}",
            right.params.offset_secs
        );
    }

    #[test]
    fn copy_paste_puts_the_regions_at_the_playhead_keeping_their_spacing() {
        let (mut p, mut v) = rig();
        second_region(&mut p, 3.0); // two bars after the first
        v.select_many(&[(0, 0), (0, 1)]);
        let mut clip = Vec::new();
        assert!(!can_paste(&p, &clip));
        assert!(copy_selected(&p, &v, &mut clip));
        assert_eq!(clip.len(), 2);
        p.position_beats = 16.0; // bar 5
        paste_at_playhead(&mut p, &mut v, &clip);
        let regions = &p.tracks[0].regions;
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[2].start_bar, 5.0);
        assert_eq!(regions[3].start_bar, 7.0, "the same two bars apart");
        assert_eq!(v.selection().len(), 2);
    }

    #[test]
    fn cut_removes_the_regions_and_paste_brings_them_back() {
        let (mut p, mut v) = rig();
        let mut clip = Vec::new();
        cut_selected(&mut p, &mut v, &mut clip);
        assert!(p.tracks[0].regions.is_empty() && v.selected.is_none());
        paste_at_playhead(&mut p, &mut v, &clip);
        assert_eq!(p.tracks[0].regions.len(), 1);
    }

    #[test]
    fn an_audio_region_does_not_paste_onto_a_midi_track() {
        let (p, _) = rig();
        let mut audio = p.tracks[0].regions[0].clone();
        audio.audio = Some(test_audio());
        assert!(!can_paste(&p, &vec![audio]));
    }

    #[test]
    fn join_merges_two_regions_and_keeps_each_notes_place() {
        let (mut p, mut v) = rig();
        second_region(&mut p, 5.0); // starts at bar 5, after the first (bars 1-4)
        v.select_many(&[(0, 0), (0, 1)]);
        assert!(can_join(&p, &v));
        join_selected(&mut p, &mut v);
        let regions = &p.tracks[0].regions;
        assert_eq!(regions.len(), 1, "one region is left");
        let joined = &regions[0];
        assert_eq!(
            (joined.start_bar, joined.len_bars),
            (1.0, 6.0),
            "bars 1 to 6"
        );
        // The first region's notes at 0, 3 and 8; the second's note at 1, moved on by the 16 beats
        // before bar 5, to 17.
        let mut starts: Vec<f32> = joined.notes.iter().map(|n| n.start).collect();
        starts.sort_by(f32::total_cmp);
        assert_eq!(starts, vec![0.0, 3.0, 8.0, 17.0]);
        assert_eq!(v.selected, Some((0, 0)));
    }

    #[test]
    fn join_needs_two_plain_midi_regions_on_one_track() {
        let (mut p, mut v) = rig();
        assert!(!can_join(&p, &v), "one region");
        second_region(&mut p, 5.0);
        p.tracks[0].regions[1].generated = None;
        p.tracks[0].regions[1].audio = Some(test_audio());
        v.select_many(&[(0, 0), (0, 1)]);
        assert!(!can_join(&p, &v), "an audio region is in it");
    }
    #[test]
    fn move_to_playhead_keeps_the_distances_between_the_regions() {
        let (mut p, mut v) = rig();
        second_region(&mut p, 3.0); // two bars after the first
        v.select_many(&[(0, 0), (0, 1)]);
        p.position_beats = 32.0; // bar 9
        move_to_playhead(&mut p, &v);
        let starts: Vec<f32> = p.tracks[0].regions.iter().map(|r| r.start_bar).collect();
        assert_eq!(starts, vec![9.0, 11.0]);
    }

    #[test]
    fn move_to_playhead_does_nothing_without_a_selection() {
        let (mut p, mut v) = rig();
        v.clear_selection();
        p.position_beats = 32.0;
        move_to_playhead(&mut p, &v);
        assert_eq!(p.tracks[0].regions[0].start_bar, 1.0);
    }

    #[test]
    fn quantize_snaps_the_notes_of_selected_regions_to_the_grid() {
        let (mut p, mut v) = rig();
        // Notes at 0, 3 and 8 are on the 1/16 grid already; move one off it.
        p.tracks[0].regions[0].notes[1].start = 3.1;
        v.select_only(0, 0);
        assert!(can_quantize(&p, &v));
        quantize_selected(&mut p, &v);
        assert!((p.tracks[0].regions[0].notes[1].start - 3.0).abs() < 1e-5);
    }

    #[test]
    fn quantize_uses_the_regions_grid_and_skips_generated_regions() {
        let (mut p, mut v) = rig();
        p.tracks[0].regions[0].notes = vec![note(0.3, 1.0)];
        p.tracks[0].regions[0].params.quantize = Some(1.0); // whole beats
        v.select_only(0, 0);
        quantize_selected(&mut p, &v);
        assert_eq!(p.tracks[0].regions[0].notes[0].start, 0.0);
        // A generated region's notes are made from its settings, so they are left alone.
        p.tracks[0].regions[0].generated = Some(crate::model::Generated::new(
            crate::model::Generator::Drummer(crate::pattern::DrummerParams::new(
                crate::pattern::DrumStyle::Rock,
            )),
        ));
        assert!(!can_quantize(&p, &v));
    }

    // ---- Option-drag copies (Logic: Option held when the region is let go)

    use crate::ui_test::Harness;
    use egui::{vec2, Modifiers, PointerButton, Pos2, Rect};

    /// Two instrument tracks; track 0 has a two-bar region at bar 2 with notes, a controller
    /// and its own settings, and track 1 a region far right (only there to find the row).
    fn drag_rig() -> (Project, crate::UiState) {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        let mut r = Region {
            missing: None,
            name: "Riff".into(),
            start_bar: 2.0,
            len_bars: 2.0,
            notes: vec![note(0.0, 1.0), note(2.5, 0.5)],
            controls: vec![crate::model::Control {
                start: 1.0,
                num: 64,
                val: 127,
            }],
            folder: None,
            audio: None,
            params: crate::model::RegionParams {
                transpose: 3,
                velocity: -5,
                quantize: Some(0.25),
                ..Default::default()
            },
            generated: None,
        };
        p.tracks[0].regions.push(r.clone());
        r.start_bar = 9.0;
        r.name = "Far".into();
        p.tracks[1].regions.push(r);
        p.selected_track = 0;
        (p, crate::UiState::new())
    }

    fn show(h: &mut Harness, p: &mut Project, s: &mut crate::UiState) {
        let meters = crate::mixer::Meters::default();
        h.settle(|ui| crate::tracks_area::show(ui, p, s, &meters));
    }

    /// Where region `r` of track `t` is on screen (the last frame).
    fn rect_of(h: &Harness, t: usize, r: usize) -> Rect {
        h.ctx
            .read_response(egui::Id::unique(("tracks_area", ("region", t, r))))
            .expect("the region is on screen")
            .rect
    }

    /// Drags from `from` by `by`, with `mods` held at each step (the last ones when let go).
    /// The drag begins with a small step (a drag starts only once the pointer has moved a
    /// little, and the region is held where it was then); `by` is counted from there.
    fn drag_with(
        h: &mut Harness,
        p: &mut Project,
        s: &mut crate::UiState,
        from: Pos2,
        by: egui::Vec2,
        mods: &[Modifiers],
    ) {
        let meters = crate::mixer::Meters::default();
        let mut ui = |ui: &mut egui::Ui| crate::tracks_area::show(ui, p, s, &meters);
        h.modifiers = mods[0];
        h.press(from, PointerButton::Primary, &mut ui);
        let from = from + vec2(10.0, 0.0);
        h.move_to(from, &mut ui);
        for (i, m) in mods.iter().enumerate() {
            h.modifiers = *m;
            let k = (i + 1) as f32 / mods.len() as f32;
            h.move_to(from + by * k, &mut ui);
        }
        h.release(from + by, PointerButton::Primary, &mut ui);
        h.frame(vec![], &mut ui);
        h.modifiers = Modifiers::NONE;
    }

    /// Everything about two regions but where they are.
    fn same_content(a: &Region, b: &Region) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.len_bars, b.len_bars);
        assert_eq!(a.notes, b.notes);
        assert_eq!(a.controls, b.controls);
        assert!(a.params == b.params);
        assert_eq!(a.generated, b.generated);
        assert_eq!(
            a.audio.as_ref().map(|x| &x.file.path),
            b.audio.as_ref().map(|x| &x.file.path)
        );
    }

    /// Points of one bar in the tracks area (a bar is 96 points wide at the start).
    const BAR: f32 = 96.0;

    #[test]
    fn option_drag_leaves_the_region_and_drops_an_identical_copy_where_it_ends() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let original = p.tracks[0].regions[0].clone();
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        let meters = crate::mixer::Meters::default();
        h.modifiers = Modifiers::ALT;
        h.press(grab, PointerButton::Primary, |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        let grab = grab + vec2(10.0, 0.0);
        h.move_to(grab, |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        h.move_to(grab + vec2(BAR, 0.0), |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        h.move_to(grab + vec2(2.0 * BAR, 0.0), |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        // On the way the region stays where it was (its copy follows the pointer).
        assert_eq!(
            p.tracks[0].regions.len(),
            1,
            "nothing lands before the button is let go"
        );
        assert_eq!(
            p.tracks[0].regions[0].start_bar, 2.0,
            "the original stays in place"
        );
        h.release(grab + vec2(2.0 * BAR, 0.0), PointerButton::Primary, |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        h.modifiers = Modifiers::NONE;
        let regions = &p.tracks[0].regions;
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].start_bar, 2.0, "the original is unchanged");
        same_content(&regions[0], &original);
        assert_eq!(regions[1].start_bar, 4.0, "the copy is two bars later");
        same_content(&regions[1], &original);
        assert_eq!(s.tracks.selection(), vec![(0, 1)], "the copy is selected");
    }

    #[test]
    fn a_plain_drag_still_moves_and_option_counts_only_when_the_button_is_let_go() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        let (none, alt) = (Modifiers::NONE, Modifiers::ALT);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            grab,
            vec2(BAR, 0.0),
            &[none, none, none],
        );
        assert_eq!(p.tracks[0].regions.len(), 1);
        assert_eq!(p.tracks[0].regions[0].start_bar, 3.0, "moved a bar");

        // Option pressed on the way and let go before the drop: a move.
        show(&mut h, &mut p, &mut s);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            grab,
            vec2(BAR, 0.0),
            &[none, alt, none],
        );
        assert_eq!(p.tracks[0].regions.len(), 1);
        assert_eq!(p.tracks[0].regions[0].start_bar, 4.0);

        // Option pressed only at the end: a copy.
        show(&mut h, &mut p, &mut s);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            grab,
            vec2(-BAR, 0.0),
            &[none, none, alt],
        );
        let starts: Vec<f32> = p.tracks[0].regions.iter().map(|r| r.start_bar).collect();
        assert_eq!(starts, vec![4.0, 3.0]);
    }

    #[test]
    fn option_drag_copies_a_whole_selection_keeping_its_spacing() {
        let (mut p, mut s) = drag_rig();
        let mut extra = p.tracks[0].regions[0].clone();
        extra.start_bar = 5.0;
        extra.name = "Second".into();
        p.tracks[0].regions.push(extra);
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        s.tracks.select_many(&[(0, 0), (0, 1)]);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        let alt = Modifiers::ALT;
        drag_with(&mut h, &mut p, &mut s, grab, vec2(BAR, 0.0), &[alt, alt]);
        let regions = &p.tracks[0].regions;
        let at: Vec<(&str, f32)> = regions
            .iter()
            .map(|r| (r.name.as_str(), r.start_bar))
            .collect();
        assert_eq!(
            at,
            vec![
                ("Riff", 2.0),
                ("Second", 5.0),
                ("Riff", 3.0),
                ("Second", 6.0)
            ]
        );
        same_content(&regions[2], &regions[0]);
        same_content(&regions[3], &regions[1]);
        assert_eq!(s.tracks.selection(), vec![(0, 2), (0, 3)]);
    }

    #[test]
    fn option_drag_copies_to_another_track_and_a_plain_drag_moves_there() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        let down = rect_of(&h, 1, 0).center().y - rect_of(&h, 0, 0).center().y;
        assert!(down > 10.0);
        let alt = Modifiers::ALT;
        drag_with(&mut h, &mut p, &mut s, grab, vec2(BAR, down), &[alt, alt]);
        assert_eq!(
            p.tracks[0].regions.len(),
            1,
            "the original stays on its track"
        );
        assert_eq!(p.tracks[0].regions[0].start_bar, 2.0);
        let copy = &p.tracks[1].regions[1];
        assert_eq!(copy.start_bar, 3.0);
        same_content(copy, &p.tracks[0].regions[0]);
        assert_eq!(s.tracks.selection(), vec![(1, 1)]);
        assert_eq!(p.selected_track, 1);

        // Without Option the region goes to the other track.
        show(&mut h, &mut p, &mut s);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        let none = Modifiers::NONE;
        drag_with(&mut h, &mut p, &mut s, grab, vec2(0.0, down), &[none, none]);
        assert!(p.tracks[0].regions.is_empty());
        let names: Vec<&str> = p.tracks[1]
            .regions
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(names, vec!["Far", "Riff", "Riff"]);
    }

    #[test]
    fn an_audio_region_is_not_copied_onto_a_midi_track() {
        let (mut p, _) = drag_rig();
        p.tracks[0].kind = TrackKind::Audio;
        p.tracks[0].regions[0].audio = Some(test_audio());
        let auto = crate::automation::AutoView::default();
        let landed = drop_regions(&mut p, &auto, &[(0, 0, 4.0)], 4.0, 1, true, false);
        assert_eq!(landed, vec![(0, 1)], "copied on its own track instead");
        assert_eq!(p.tracks[1].regions.len(), 1);
    }

    #[test]
    fn a_copied_audio_region_keeps_its_file_offset_gain_and_fades() {
        let mut p = Project::empty();
        p.tracks[0].kind = TrackKind::Audio;
        p.add_track(TrackKind::Audio);
        p.tracks[0].regions.push(Region {
            missing: None,
            name: "Take".into(),
            start_bar: 1.0,
            len_bars: 1.0,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            audio: Some(test_audio()),
            params: crate::model::RegionParams {
                gain_db: -4.5,
                fade_in: 0.5,
                fade_out: 0.75,
                fade_curve: prowave_engine::FadeCurve::SCurve,
                offset_secs: 0.25,
                reverse: true,
                ..Default::default()
            },
            generated: None,
        });
        let auto = crate::automation::AutoView::default();
        let landed = drop_regions(&mut p, &auto, &[(0, 0, 0.0)], 8.0, 1, true, false);
        assert_eq!(landed, vec![(1, 0)]);
        let (a, b) = (&p.tracks[0].regions[0], &p.tracks[1].regions[0]);
        same_content(a, b);
        assert_eq!((a.start_bar, b.start_bar), (1.0, 3.0));
    }

    #[test]
    fn undo_takes_the_copy_away_in_one_step() {
        let (mut p, mut s) = drag_rig();
        let mut history = crate::undo::History::new(&p);
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let grab = rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0);
        let alt = Modifiers::ALT;
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            grab,
            vec2(2.0 * BAR, 0.0),
            &[alt, alt, alt],
        );
        assert_eq!(p.tracks[0].regions.len(), 2);
        assert!(history.record(&p));
        assert!(history.undo(&mut p));
        assert_eq!(p.tracks[0].regions.len(), 1);
        assert_eq!(p.tracks[0].regions[0].start_bar, 2.0);
        assert!(!history.can_undo(), "one step");
    }

    // ---- Shift-click, group moves and the rubber band (Logic: select and move regions)

    fn click_with(
        h: &mut Harness,
        p: &mut Project,
        s: &mut crate::UiState,
        at: Pos2,
        mods: Modifiers,
    ) {
        let meters = crate::mixer::Meters::default();
        // Not a double-click of the click before.
        h.time_passes(1.0);
        h.modifiers = mods;
        h.click(at, |ui| crate::tracks_area::show(ui, p, s, &meters));
        h.modifiers = Modifiers::NONE;
    }

    /// The middle of region `r` of track `t` on screen.
    fn middle(
        h: &mut Harness,
        p: &mut Project,
        s: &mut crate::UiState,
        t: usize,
        r: usize,
    ) -> Pos2 {
        show(h, p, s);
        rect_of(h, t, r).left_center() + vec2(20.0, 0.0)
    }

    fn places(p: &Project) -> Vec<Vec<(&str, f32)>> {
        p.tracks
            .iter()
            .map(|t| {
                t.regions
                    .iter()
                    .map(|r| (r.name.as_str(), r.start_bar))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn shift_click_adds_a_region_on_another_track_and_a_drag_moves_them_all_in_one_undo_step() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        let far = middle(&mut h, &mut p, &mut s, 1, 0);
        click_with(&mut h, &mut p, &mut s, riff, Modifiers::NONE);
        assert_eq!(s.tracks.selection(), vec![(0, 0)]);
        click_with(&mut h, &mut p, &mut s, far, Modifiers::SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (1, 0)]);

        let mut history = crate::undo::History::new(&p);
        let none = Modifiers::NONE;
        // Dragged by the region on the first track: the one on the second goes along.
        drag_with(&mut h, &mut p, &mut s, riff, vec2(BAR, 0.0), &[none, none]);
        assert_eq!(
            places(&p),
            vec![vec![("Riff", 3.0)], vec![("Far", 10.0)]],
            "both a bar later"
        );
        assert_eq!(
            s.tracks.selection(),
            vec![(0, 0), (1, 0)],
            "still both selected"
        );
        assert!(history.record(&p));
        assert!(history.undo(&mut p));
        assert_eq!(places(&p), vec![vec![("Riff", 2.0)], vec![("Far", 9.0)]]);
        assert!(!history.can_undo(), "the whole move was one step");
    }

    #[test]
    fn shift_click_takes_a_region_out_again_and_a_plain_click_selects_one_alone() {
        let (mut p, mut s) = drag_rig();
        let mut third = p.tracks[0].regions[0].clone();
        third.start_bar = 6.0;
        third.name = "Third".into();
        p.tracks[0].regions.push(third);
        let mut h = Harness::new(1400.0, 600.0);
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        let third = middle(&mut h, &mut p, &mut s, 0, 1);
        let far = middle(&mut h, &mut p, &mut s, 1, 0);
        click_with(&mut h, &mut p, &mut s, riff, Modifiers::NONE);
        click_with(&mut h, &mut p, &mut s, far, Modifiers::SHIFT);
        click_with(&mut h, &mut p, &mut s, far, Modifiers::SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 0)], "taken out again");
        click_with(&mut h, &mut p, &mut s, far, Modifiers::SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (1, 0)]);
        // A plain click on a region outside the selection selects it alone.
        click_with(&mut h, &mut p, &mut s, third, Modifiers::NONE);
        assert_eq!(s.tracks.selection(), vec![(0, 1)]);
    }

    #[test]
    fn a_drag_that_begins_with_shift_held_adds_the_region_and_moves_the_selection() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        let far = middle(&mut h, &mut p, &mut s, 1, 0);
        click_with(&mut h, &mut p, &mut s, riff, Modifiers::NONE);
        let shift = Modifiers::SHIFT;
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            far,
            vec2(-BAR, 0.0),
            &[shift, shift],
        );
        assert_eq!(places(&p), vec![vec![("Riff", 1.0)], vec![("Far", 8.0)]]);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (1, 0)]);
    }

    #[test]
    fn a_group_stops_at_the_start_of_the_song_and_keeps_its_spacing() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        // Dragged by the later region, four bars to the left: the earlier one has only one
        // bar of room, so both go one bar.
        let far = middle(&mut h, &mut p, &mut s, 1, 0);
        let none = Modifiers::NONE;
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            far,
            vec2(-4.0 * BAR, 0.0),
            &[none, none, none],
        );
        assert_eq!(places(&p), vec![vec![("Riff", 1.0)], vec![("Far", 8.0)]]);
        // The same when the regions are placed without a drag on screen.
        assert_eq!(clamp_delta(&[(0, 0, 4.0), (1, 0, 32.0)], -12.0), -4.0);
        assert_eq!(clamp_delta(&[(0, 0, 4.0), (1, 0, 32.0)], 3.0), 3.0);
    }

    #[test]
    fn a_group_goes_to_other_tracks_together_or_not_at_all() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let down = rect_of(&h, 1, 0).center().y - rect_of(&h, 0, 0).center().y;
        let none = Modifiers::NONE;
        // Two tracks: the lower region has no track under it, so nothing changes track.
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            riff,
            vec2(-10.0, down),
            &[none, none],
        );
        assert_eq!(places(&p), vec![vec![("Riff", 2.0)], vec![("Far", 9.0)]]);

        // A third instrument track: both go down one track, a bar later.
        p.add_track(TrackKind::Instrument);
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            riff,
            vec2(BAR - 10.0, down),
            &[none, none],
        );
        assert_eq!(
            places(&p),
            vec![vec![], vec![("Riff", 3.0)], vec![("Far", 10.0)]]
        );
        assert_eq!(s.tracks.selection(), vec![(1, 0), (2, 0)]);

        // An audio track under them: a MIDI region cannot land there, so neither moves down.
        p.add_track(TrackKind::Audio);
        let riff = middle(&mut h, &mut p, &mut s, 1, 0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            riff,
            vec2(-10.0, down),
            &[none, none],
        );
        assert_eq!(
            places(&p),
            vec![vec![], vec![("Riff", 3.0)], vec![("Far", 10.0)], vec![]]
        );
    }

    #[test]
    fn a_rubber_band_on_the_background_selects_what_it_touches_and_shift_switches() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let (riff, far) = (rect_of(&h, 0, 0), rect_of(&h, 1, 0));
        let meters = crate::mixer::Meters::default();
        let band = |h: &mut Harness,
                    p: &mut Project,
                    s: &mut crate::UiState,
                    from: Pos2,
                    to: Pos2,
                    mods: Modifiers| {
            h.time_passes(1.0);
            h.modifiers = mods;
            let mid = from + (to - from) * 0.5;
            h.drag(&[from, mid, to], PointerButton::Primary, |ui| {
                crate::tracks_area::show(ui, p, s, &meters)
            });
            h.modifiers = Modifiers::NONE;
        };
        let none = Modifiers::NONE;
        // From the empty first bar of the first track to just inside the region on the second.
        let before = riff.left_center() - vec2(40.0, 0.0);
        let into_far = far.left_center() + vec2(10.0, 0.0);
        band(&mut h, &mut p, &mut s, before, into_far, none);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (1, 0)]);
        assert_eq!(places(&p), vec![vec![("Riff", 2.0)], vec![("Far", 9.0)]]);

        // A band over nothing selects nothing.
        let gap = pos2_between(riff, far);
        band(&mut h, &mut p, &mut s, gap, gap + vec2(60.0, 0.0), none);
        assert!(s.tracks.selection().is_empty());

        // Over the first region only; then Shift adds the second, and takes it out again.
        let into_riff = riff.left_center() + vec2(10.0, 0.0);
        band(&mut h, &mut p, &mut s, before, into_riff, none);
        assert_eq!(s.tracks.selection(), vec![(0, 0)]);
        let far_from = far.left_center() - vec2(40.0, 0.0);
        band(&mut h, &mut p, &mut s, far_from, into_far, Modifiers::SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (1, 0)]);
        band(&mut h, &mut p, &mut s, far_from, into_far, Modifiers::SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 0)]);
    }

    /// A point on the second track's background, between the two regions of the rig.
    fn pos2_between(riff: Rect, far: Rect) -> Pos2 {
        egui::pos2(riff.right() + 60.0, far.center().y)
    }

    // ---- tools and keys on the whole selection, the left edge, drag modifiers

    #[test]
    fn the_scissors_cut_every_selected_region_and_option_cuts_equal_pieces() {
        let (mut p, mut s) = drag_rig();
        // A second region under the same bars on the other track.
        let mut under = p.tracks[0].regions[0].clone();
        under.name = "Under".into();
        p.tracks[1].regions.insert(0, under);
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        s.tracks.tool = crate::tracks_area::TrackTool::Scissors;
        // A click a bar into "Riff" (bars 2-3): both selected regions are cut at bar 3.
        let at = rect_of(&h, 0, 0).left_center() + vec2(BAR, 0.0);
        click_with(&mut h, &mut p, &mut s, at, Modifiers::NONE);
        assert_eq!(
            places(&p),
            vec![
                vec![("Riff", 2.0), ("Riff", 3.0)],
                vec![("Under", 2.0), ("Under", 3.0), ("Far", 9.0)]
            ]
        );
        assert_eq!(s.tracks.selection(), vec![(0, 1), (1, 1)]);

        // Option: a region of two bars clicked half a bar in is cut into four equal pieces.
        let (mut p, mut s) = drag_rig();
        show(&mut h, &mut p, &mut s);
        s.tracks.tool = crate::tracks_area::TrackTool::Scissors;
        let at = rect_of(&h, 0, 0).left_center() + vec2(BAR / 2.0, 0.0);
        click_with(&mut h, &mut p, &mut s, at, Modifiers::ALT);
        let pieces: Vec<(f32, f32)> = p.tracks[0]
            .regions
            .iter()
            .map(|r| (r.start_bar, r.len_bars))
            .collect();
        assert_eq!(pieces, vec![(2.0, 0.5), (2.5, 0.5), (3.0, 0.5), (3.5, 0.5)]);
    }

    #[test]
    fn the_mute_tool_mutes_every_selected_region() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        s.tracks.tool = crate::tracks_area::TrackTool::Mute;
        click_with(&mut h, &mut p, &mut s, riff, Modifiers::NONE);
        assert!(p.tracks[0].regions[0].params.mute && p.tracks[1].regions[0].params.mute);
        click_with(&mut h, &mut p, &mut s, riff, Modifiers::NONE);
        assert!(!p.tracks[0].regions[0].params.mute && !p.tracks[1].regions[0].params.mute);
        // A region outside the selection is muted alone.
        s.tracks.select_only(1, 0);
        click_with(&mut h, &mut p, &mut s, riff, Modifiers::NONE);
        assert!(p.tracks[0].regions[0].params.mute && !p.tracks[1].regions[0].params.mute);
    }

    #[test]
    fn dragging_the_left_edge_moves_the_start_and_the_notes_stay_where_they_were() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let meters = crate::mixer::Meters::default();
        // "Riff": bars 2-3, notes at beats 0 and 2.5 of the region (beats 4 and 6.5 of the song).
        let grip = rect_of(&h, 0, 0).left_bottom() + vec2(2.0, -4.0);
        let to = grip + vec2(BAR / 2.0, 0.0);
        h.drag(
            &[grip, grip + vec2(20.0, 0.0), to],
            PointerButton::Primary,
            |ui| crate::tracks_area::show(ui, &mut p, &mut s, &meters),
        );
        let r = &p.tracks[0].regions[0];
        assert_eq!((r.start_bar, r.len_bars), (2.5, 1.5), "half a bar shorter");
        assert_eq!(r.notes.len(), 1, "the note before the new start is gone");
        assert_eq!(r.notes[0].start, 0.5, "still at beat 6.5 of the song");
        assert_eq!(
            r.controls.len(),
            0,
            "the controller before the new start too"
        );

        // Back out to bar 1.5: the end stays, the note is still at beat 6.5.
        show(&mut h, &mut p, &mut s);
        let grip = rect_of(&h, 0, 0).left_bottom() + vec2(2.0, -4.0);
        let to = grip + vec2(-BAR, 0.0);
        h.time_passes(1.0);
        h.drag(
            &[grip, grip + vec2(-20.0, 0.0), to],
            PointerButton::Primary,
            |ui| crate::tracks_area::show(ui, &mut p, &mut s, &meters),
        );
        let r = &p.tracks[0].regions[0];
        assert_eq!((r.start_bar, r.len_bars), (1.5, 2.5));
        assert_eq!(r.notes[0].start, 4.5);
    }

    #[test]
    fn the_left_edge_of_an_audio_region_moves_through_its_file() {
        let mut p = Project::empty();
        p.tracks[0].kind = TrackKind::Audio;
        let (mut riff, _) = (drag_rig().0.tracks[0].regions[0].clone(), ());
        riff.notes.clear();
        riff.controls.clear();
        riff.audio = Some(test_audio());
        riff.params = crate::model::RegionParams {
            offset_secs: 1.0,
            ..Default::default()
        };
        p.tracks[0].regions.push(riff);
        // 120 bpm: two beats are a second. Two beats later: a second further into the file.
        assert!(p.resize_region_start(0, 0, 6.0));
        let r = &p.tracks[0].regions[0];
        assert_eq!((r.start_bar, r.len_bars), (2.5, 1.5));
        assert!((r.params.offset_secs - 2.0).abs() < 1e-9);
        // Far to the left: no further than the start of the file (two seconds: four beats).
        assert!(p.resize_region_start(0, 0, 0.0));
        let r = &p.tracks[0].regions[0];
        assert_eq!((r.start_bar, r.len_bars), (1.5, 2.5));
        assert!(r.params.offset_secs.abs() < 1e-9);
        assert!(!p.resize_region_start(0, 0, 0.0), "nothing more to show");
    }

    #[test]
    fn shift_keeps_a_drag_to_one_direction_and_control_moves_in_divisions() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let down = rect_of(&h, 1, 0).center().y - rect_of(&h, 0, 0).center().y;
        assert!(down < BAR, "the drag below goes further along than down");
        let (none, shift, ctrl) = (Modifiers::NONE, Modifiers::SHIFT, Modifiers::CTRL);
        // A bar later and a track down with Shift pressed on the way: only the bar.
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            riff,
            vec2(BAR, down),
            &[none, shift, shift],
        );
        assert_eq!(places(&p), vec![vec![("Riff", 3.0)], vec![("Far", 9.0)]]);
        // Mostly down: only the track changes.
        let riff = middle(&mut h, &mut p, &mut s, 0, 0);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            riff,
            vec2(down / 2.0, down),
            &[none, shift, shift],
        );
        assert_eq!(places(&p), vec![vec![], vec![("Far", 9.0), ("Riff", 3.0)]]);
        // Control: sixteenth notes whatever Snap says (30 points are 1.25 beats; Snap alone
        // would go to the beat).
        let riff = middle(&mut h, &mut p, &mut s, 1, 1);
        drag_with(
            &mut h,
            &mut p,
            &mut s,
            riff,
            vec2(30.0, 0.0),
            &[none, ctrl, ctrl],
        );
        assert_eq!(p.tracks[1].regions[1].start_bar, 3.0 + 1.25 / 4.0);
    }

    #[test]
    fn a_click_on_a_track_header_selects_the_tracks_regions() {
        let (mut p, mut s) = drag_rig();
        let mut third = p.tracks[0].regions[0].clone();
        third.start_bar = 6.0;
        p.tracks[0].regions.push(third);
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        let (first, second) = (rect_of(&h, 0, 0), rect_of(&h, 1, 0));
        let header = |row: Rect| egui::pos2(crate::tracks_area::HEADER_W * 0.62, row.top() + 6.0);
        click_with(&mut h, &mut p, &mut s, header(second), Modifiers::NONE);
        assert_eq!(p.selected_track, 1);
        assert_eq!(s.tracks.selection(), vec![(1, 0)]);
        click_with(&mut h, &mut p, &mut s, header(first), Modifiers::NONE);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (0, 1)]);
        click_with(&mut h, &mut p, &mut s, header(second), Modifiers::SHIFT);
        assert_eq!(s.tracks.selection(), vec![(0, 0), (0, 1), (1, 0)]);
    }

    #[test]
    fn delete_removes_every_selected_region() {
        let (mut p, mut s) = drag_rig();
        s.tracks.select_many(&[(0, 0), (1, 0)]);
        crate::tracks_area::delete_selected(&mut p, &mut s.tracks);
        assert_eq!(places(&p), vec![vec![], vec![]]);
        assert_eq!(p.tracks.len(), 2, "the tracks stay");
        assert!(s.tracks.selection().is_empty());
    }

    // ---- the Tool menu (T), the Command-click tool, the Pencil, the Gain tool, the pointer

    use crate::tracks_area::{Snap, TrackTool};
    use egui::{pos2, Event, Key};

    /// One frame as the app runs the Tracks area: the View keys, the area, the key commands.
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

    /// `key` pressed and let go with `mods` held, the pointer where it last was.
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

    #[test]
    fn t_opens_the_tool_menu_at_the_pointer_and_logics_letters_pick_the_tool() {
        let (mut p, mut s) = drag_rig();
        let (p, s) = (&mut p, &mut s);
        let mut h = Harness::new(1400.0, 600.0);
        let over = middle(&mut h, p, s, 0, 0);
        app_frame(&mut h, p, s, vec![Event::PointerMoved(over)]);
        let none = Modifiers::NONE;
        let (inspector, bottom, auto_mode, recording) =
            (s.show_inspector, s.bottom, s.auto.mode, p.recording);
        for (key, shift, tool) in [
            (Key::P, false, TrackTool::Pencil),
            (Key::E, false, TrackTool::Eraser),
            (Key::T, true, TrackTool::Text),
            (Key::I, false, TrackTool::Scissors),
            (Key::J, false, TrackTool::Join),
            (Key::M, false, TrackTool::Mute),
            (Key::Y, false, TrackTool::Zoom),
            (Key::A, false, TrackTool::Fade),
            (Key::G, false, TrackTool::Gain),
            (Key::R, false, TrackTool::Marquee),
            (Key::S, false, TrackTool::Solo),
            (Key::T, false, TrackTool::Pointer),
        ] {
            tap(&mut h, p, s, Key::T, none);
            assert!(s.tracks.tool_menu_open(), "T opens the Tool menu");
            let mods = if shift { Modifiers::SHIFT } else { none };
            tap(&mut h, p, s, key, mods);
            assert_eq!(s.tracks.tool, tool, "{key:?}");
            assert!(!s.tracks.tool_menu_open(), "a tool's key closes the menu");
        }
        // The letters were the menu's only: I did not show the Inspector, P the Piano Roll,
        // A Automation Mode; and T twice gave the Pointer (the last round above).
        assert_eq!(s.show_inspector, inspector);
        assert!(s.bottom == bottom);
        assert_eq!(s.auto.mode, auto_mode);
        assert_eq!(p.recording, recording);
        // M and S picked tools, not Toggle Track Mute and Toggle Track Solo.
        assert!(p.tracks.iter().all(|t| !t.mute && !t.solo));
        // Escape closes the menu and keeps the tool.
        s.tracks.tool = TrackTool::Mute;
        tap(&mut h, p, s, Key::T, none);
        tap(&mut h, p, s, Key::Escape, none);
        assert!(!s.tracks.tool_menu_open());
        assert_eq!(s.tracks.tool, TrackTool::Mute);
        // Outside the menu the letters do what they did: I is Show Inspector.
        tap(&mut h, p, s, Key::I, none);
        assert_eq!(s.show_inspector, !inspector);
        // Command-T is still Split at Playhead, not the menu.
        tap(&mut h, p, s, Key::T, Modifiers::COMMAND);
        assert!(!s.tracks.tool_menu_open());
    }

    #[test]
    fn t_does_nothing_while_a_text_field_has_the_keyboard_or_the_piano_roll_has_the_keys() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        let over = middle(&mut h, &mut p, &mut s, 0, 0);
        let meters = crate::mixer::Meters::default();
        let mut text = String::new();
        let field = egui::Id::unique("a text field");
        let mut with_field = |h: &mut Harness, p: &mut Project, s: &mut crate::UiState, ev| {
            h.frame(ev, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut text).id(field));
                r.request_focus();
                crate::tracks_area::show(ui, p, s, &meters);
            })
        };
        with_field(&mut h, &mut p, &mut s, vec![Event::PointerMoved(over)]);
        let t = |pressed| Event::Key {
            key: Key::T,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        with_field(&mut h, &mut p, &mut s, vec![t(true), t(false)]);
        with_field(&mut h, &mut p, &mut s, vec![]);
        assert!(
            h.ctx.memory(|m| m.has_focus(field)),
            "the field has the keyboard"
        );
        assert!(!s.tracks.tool_menu_open());
        // The Piano Roll has the keys (the pointer is over it): its own T, not this one.
        s.piano_focus = true;
        tap(&mut h, &mut p, &mut s, Key::T, Modifiers::NONE);
        assert!(!s.tracks.tool_menu_open());
    }

    #[test]
    fn command_click_uses_the_command_click_tool_and_the_left_click_tool_stays() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        assert_eq!(s.tracks.tool, TrackTool::Pointer);
        assert_eq!(
            s.tracks.cmd_tool,
            TrackTool::Marquee,
            "Logic's Marquee on Command"
        );
        // The Scissors as the Command-click tool, picked in the toolbar's second menu.
        s.tracks.cmd_tool = TrackTool::Scissors;
        // "Riff" is bars 2-3 (beat 4 on); 1.4 beats in, Smart snap at this zoom is a beat.
        let at = rect_of(&h, 0, 0).left_center() + vec2(1.4 * BAR / 4.0, 0.0);
        click_with(&mut h, &mut p, &mut s, at, Modifiers::COMMAND);
        assert_eq!(places(&p)[0], vec![("Riff", 2.0), ("Riff", 2.25)]);
        assert_eq!(s.tracks.tool, TrackTool::Pointer);
        // A plain click selects and cuts nothing.
        click_with(&mut h, &mut p, &mut s, at, Modifiers::NONE);
        assert_eq!(p.tracks[0].regions.len(), 2);
        // Another Command-click tool: the Mute tool, picked in the toolbar's second menu.
        s.tracks.cmd_tool = TrackTool::Mute;
        click_with(&mut h, &mut p, &mut s, at, Modifiers::COMMAND);
        assert!(p.tracks[0].regions.iter().any(|r| r.params.mute));
    }

    #[test]
    fn the_scissors_cut_on_the_snap_grid() {
        // A click 2.6 beats into "Riff" (bar 2 = beat 4): beat 6.6 of the song.
        for (snap, cut_bar) in [
            (Snap::Bar, 3.0),        // the nearest bar line, beat 8
            (Snap::Beat, 2.75),      // beat 7
            (Snap::Division, 2.625), // beat 6.5
            (Snap::Smart, 2.75),     // a beat at this zoom
        ] {
            let (mut p, mut s) = drag_rig();
            let mut h = Harness::new(1400.0, 600.0);
            s.tracks.snap = snap;
            s.tracks.tool = TrackTool::Scissors;
            show(&mut h, &mut p, &mut s);
            let at = rect_of(&h, 0, 0).left_center() + vec2(2.6 * BAR / 4.0, 0.0);
            click_with(&mut h, &mut p, &mut s, at, Modifiers::NONE);
            assert_eq!(
                places(&p)[0],
                vec![("Riff", 2.0), ("Riff", cut_bar)],
                "{}",
                snap.label()
            );
        }
    }

    #[test]
    fn each_tool_action_is_one_undo_step() {
        use crate::undo::History;
        // Option-Scissors: four pieces, one step back to one region.
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        let mut hist = History::new(&p);
        show(&mut h, &mut p, &mut s);
        s.tracks.tool = TrackTool::Scissors;
        let at = rect_of(&h, 0, 0).left_center() + vec2(BAR / 2.0, 0.0);
        click_with(&mut h, &mut p, &mut s, at, Modifiers::ALT);
        assert_eq!(p.tracks[0].regions.len(), 4);
        assert!(hist.record(&p));
        assert!(hist.undo(&mut p));
        assert_eq!(places(&p)[0], vec![("Riff", 2.0)]);
        assert!(!hist.can_undo(), "one step");
        // The Pencil, the Eraser, the Mute tool: each click one step.
        for tool in [TrackTool::Pencil, TrackTool::Eraser, TrackTool::Mute] {
            let (mut p, mut s) = drag_rig();
            let mut hist = History::new(&p);
            let before = places(&p).concat().len();
            show(&mut h, &mut p, &mut s);
            s.tracks.tool = tool;
            let at = if tool == TrackTool::Pencil {
                rect_of(&h, 0, 0).right_center() + vec2(2.0 * BAR, 0.0)
            } else {
                rect_of(&h, 0, 0).left_center() + vec2(20.0, 0.0)
            };
            click_with(&mut h, &mut p, &mut s, at, Modifiers::NONE);
            assert!(hist.record(&p), "{tool:?} changed the project");
            assert!(!hist.record(&p));
            assert!(hist.undo(&mut p));
            assert!(!hist.can_undo());
            assert_eq!(places(&p).concat().len(), before);
            assert!(!p.tracks[0].regions[0].params.mute);
        }
    }

    #[test]
    fn the_pencil_makes_an_empty_bar_on_an_instrument_track_from_the_grid_line_before() {
        let (mut p, mut s) = drag_rig();
        let audio = p.add_track(TrackKind::Audio);
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        s.tracks.tool = TrackTool::Pencil;
        let riff = rect_of(&h, 0, 0);
        // Bar 6.7 of the first track (bars 4 on are empty there): at this zoom the snap is a
        // beat, so the region starts on beat 3 of bar 6 and is a bar long.
        let x = |bar: f32| riff.left() + (bar - 2.0) * BAR;
        click_with(
            &mut h,
            &mut p,
            &mut s,
            pos2(x(6.7), riff.center().y),
            Modifiers::NONE,
        );
        let made = &p.tracks[0].regions[1];
        assert_eq!((made.start_bar, made.len_bars), (6.5, 1.0));
        assert!(made.notes.is_empty() && made.audio.is_none());
        assert_eq!(made.name, p.tracks[0].name);
        assert_eq!(s.tracks.selection(), vec![(0, 1)]);
        // With Snap set to Bar it starts on the bar.
        s.tracks.snap = Snap::Bar;
        click_with(
            &mut h,
            &mut p,
            &mut s,
            pos2(x(9.7), riff.center().y),
            Modifiers::NONE,
        );
        assert_eq!(p.tracks[0].regions[2].start_bar, 9.0);
        // On a region the Pencil is the Pointer: no new region.
        click_with(&mut h, &mut p, &mut s, riff.center(), Modifiers::NONE);
        assert_eq!(p.tracks[0].regions.len(), 3);
        assert_eq!(s.tracks.selection(), vec![(0, 0)]);
        // An audio track takes no MIDI region.
        show(&mut h, &mut p, &mut s);
        let row = h
            .ctx
            .read_response(egui::Id::unique(("tracks_area", "lanes")))
            .unwrap()
            .rect;
        let y = row.top() + 70.0 * audio as f32 + 35.0;
        click_with(&mut h, &mut p, &mut s, pos2(x(6.2), y), Modifiers::NONE);
        assert!(p.tracks[audio].regions.is_empty());
    }

    #[test]
    fn the_gain_tool_drags_audio_regions_up_and_down_in_half_decibels() {
        let (mut p, mut s) = drag_rig();
        let t = p.add_track(TrackKind::Audio);
        let mut take = p.tracks[0].regions[0].clone();
        take.notes.clear();
        take.missing = Some(crate::model::MissingAudio {
            source: "/prowave-test/take.wav".into(),
            flex: None,
        });
        p.tracks[t].regions.push(take);
        let mut h = Harness::new(1400.0, 600.0);
        show(&mut h, &mut p, &mut s);
        s.tracks.tool = TrackTool::Gain;
        let meters = crate::mixer::Meters::default();
        let drag = |h: &mut Harness, p: &mut Project, s: &mut crate::UiState, ti, up: f32| {
            let from = rect_of(h, ti, 0).center();
            h.time_passes(1.0);
            h.drag(
                &[from, from - vec2(0.0, up / 2.0), from - vec2(0.0, up)],
                PointerButton::Primary,
                |ui| crate::tracks_area::show(ui, p, s, &meters),
            );
        };
        drag(&mut h, &mut p, &mut s, t, 30.0);
        assert_eq!(p.tracks[t].regions[0].params.gain_db, 3.0);
        drag(&mut h, &mut p, &mut s, t, -55.0);
        assert_eq!(p.tracks[t].regions[0].params.gain_db, -2.5);
        assert_eq!(p.tracks[t].regions[0].start_bar, 2.0, "it does not move");
        // A MIDI region has no gain to change, and the Gain tool does not move it.
        drag(&mut h, &mut p, &mut s, 0, 30.0);
        assert_eq!(p.tracks[0].regions[0].params.gain_db, 0.0);
        assert_eq!(places(&p)[0], vec![("Riff", 2.0)]);
    }

    #[test]
    fn the_pointer_shows_the_tool_over_the_lanes() {
        let (mut p, mut s) = drag_rig();
        let mut h = Harness::new(1400.0, 600.0);
        let over = middle(&mut h, &mut p, &mut s, 0, 0);
        let meters = crate::mixer::Meters::default();
        let look = |h: &mut Harness, p: &mut Project, s: &mut crate::UiState, mods| {
            h.modifiers = mods;
            h.move_to(over, |ui| crate::tracks_area::show(ui, p, s, &meters));
            h.modifiers = Modifiers::NONE;
            h.cursor
        };
        assert_eq!(
            look(&mut h, &mut p, &mut s, Modifiers::NONE),
            egui::CursorIcon::Default
        );
        // Command held: the Command-click tool (the Marquee), drawn in place of the arrow.
        let cmd = look(&mut h, &mut p, &mut s, Modifiers::COMMAND);
        assert_eq!(cmd, egui::CursorIcon::None);
        for (tool, _, _, _) in TrackTool::ALL.into_iter().skip(1) {
            s.tracks.tool = tool;
            assert_eq!(
                look(&mut h, &mut p, &mut s, Modifiers::NONE),
                egui::CursorIcon::None,
                "{tool:?}"
            );
        }
    }
}
