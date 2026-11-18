//! Logic Pro's Edit > Cut/Insert Time: edits of the whole song between the left and right
//! locators (the cycle), on every track at once.
//!
//! - **Cut Section Between Locators** (Control-Command-X): the time between the locators is
//!   taken out. Regions are cut at the locators, the part between them is removed, and
//!   everything after the right locator moves left by the section's length.
//! - **Insert Silence Between Locators** (Control-Command-I): the opposite. Regions are cut at
//!   the left locator and everything from it moves right by the section's length.
//! - **Repeat Section Between Locators** (Control-Command-R): what is between the locators is
//!   copied right after the right locator; what came after it moves right to make room.
//!
//! What moves with the time: the regions of every track, the markers, the tempo changes, the
//! signature changes and the automation of every track and of the Stereo Out. The tempo,
//! signature and automation in effect after the edit are kept continuous: a cut keeps the
//! tempo, signature and automation value that were in effect at the right locator from the
//! left locator on; inserted silence keeps those of the left locator; a repeated section starts
//! with those of the left locator and gives back those of the right one after it.
//!
//! Limits, as the model has them:
//! - Signature changes sit on bar lines. When a section is not a whole number of bars, a
//!   signature change that moves goes to the nearest bar line. Regions, markers and automation
//!   keep their exact positions in quarter notes.
//! - Take folders, Pattern and Drummer regions and reversed audio cannot be cut. One that
//!   crosses a locator stays whole: a cut moves it to start at the left locator when it began
//!   inside the section (and removes it when it lies wholly inside); otherwise it moves as its
//!   start does.
//! - A looped region whose loop crosses a locator has its repeats made into regions of their
//!   own first, so the repeats are cut like any region.
//! - The locators and the playhead stay where they are, as in Logic.
use crate::model::{Project, Region, SigChange, SigRef, TempoPoint};
use prowave_engine::{lane_value, AutoPoint};

/// Positions closer than this (quarter notes) are the same.
const EPS: f64 = 1e-6;

/// Which of the three edits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SectionEdit {
    Cut,
    InsertSilence,
    Repeat,
}

impl SectionEdit {
    /// The Edit > Cut/Insert Time entry that runs it.
    pub(crate) fn from_menu(label: &str) -> Option<Self> {
        match label {
            "Cut Section Between Locators" => Some(Self::Cut),
            "Insert Silence Between Locators" => Some(Self::InsertSilence),
            "Repeat Section Between Locators" => Some(Self::Repeat),
            _ => None,
        }
    }
}

/// Whether the locators enclose a section (the edits do nothing otherwise).
pub(crate) fn can_apply(p: &Project) -> bool {
    p.cycle_start.is_finite() && p.cycle_end.is_finite() && p.cycle_end - p.cycle_start > EPS
}

/// Where something at `b` goes: `None` when a cut removes it.
fn moved(edit: SectionEdit, l: f64, r: f64, b: f64) -> Option<f64> {
    let len = r - l;
    match edit {
        SectionEdit::Cut if b >= r - EPS => Some(b - len),
        SectionEdit::Cut if b >= l - EPS => None,
        SectionEdit::Cut => Some(b),
        SectionEdit::InsertSilence if b >= l - EPS => Some(b + len),
        SectionEdit::InsertSilence => Some(b),
        SectionEdit::Repeat if b >= r - EPS => Some(b + len),
        SectionEdit::Repeat => Some(b),
    }
}

/// Where a repeat copies something at `b` to, if it does.
fn copied(edit: SectionEdit, l: f64, r: f64, b: f64) -> Option<f64> {
    (edit == SectionEdit::Repeat && b >= l - EPS && b < r - EPS).then_some(b + (r - l))
}

/// Runs `edit` on the section between the locators. Does nothing without a section.
pub(crate) fn apply(p: &mut Project, edit: SectionEdit) {
    if !can_apply(p) {
        return;
    }
    let (l, r) = (p.cycle_start.max(0.0), p.cycle_end);
    if r - l <= EPS {
        return;
    }
    let cuts: Vec<f64> = match edit {
        SectionEdit::InsertSilence => vec![l],
        SectionEdit::Cut | SectionEdit::Repeat => vec![l, r],
    };
    unloop_across(p, &cuts);
    split_at(p, &cuts);
    let regions = place_regions(p, edit, l, r);
    let sigs_changed = move_signatures(p, edit, l, r);
    move_tempo(p, edit, l, r);
    move_markers(p, edit, l, r);
    for t in &mut p.tracks {
        move_automation(&mut t.auto, edit, l, r);
    }
    move_automation(&mut p.master_auto, edit, l, r);
    // The regions, at their new places in the new bars.
    let new_sigs = p.sigs().to_owned();
    for (t, list) in regions.into_iter().enumerate() {
        p.tracks[t].regions = list
            .into_iter()
            .map(|(mut reg, old_start, start, len)| {
                if sigs_changed || (start - old_start).abs() > EPS {
                    place(&mut reg, new_sigs.as_ref(), start, len);
                }
                reg
            })
            .collect();
    }
    p.song_rev += 1;
}

/// Sets `reg` to start at `start` and last `len` quarter notes in the bars of `sigs`.
fn place(reg: &mut Region, sigs: SigRef<'_>, start: f64, len: f64) {
    let b0 = sigs.beats_to_bar(start.max(0.0));
    let b1 = sigs.beats_to_bar((start + len).max(0.0));
    reg.start_bar = (b0 + 1.0) as f32;
    reg.len_bars = (b1 - b0) as f32;
}

/// Makes the repeats of every looped region whose loop crosses one of `cuts` into regions of
/// their own (the last one cut where the loop ended), so that they can be cut and moved.
fn unloop_across(p: &mut Project, cuts: &[f64]) {
    for t in 0..p.tracks.len() {
        let mut i = 0;
        while i < p.tracks[t].regions.len() {
            let reg = &p.tracks[t].regions[i];
            let (end, stop) = (p.region_end_beats(reg), p.region_loop_end_beats(reg));
            let crosses = cuts.iter().any(|&c| c > end + EPS && c < stop - EPS);
            let cuttable = reg.folder.is_none() && reg.generated.is_none() && !reg.params.reverse;
            if reg.params.loop_beats <= 0.0 || !crosses || !cuttable {
                i += 1;
                continue;
            }
            let repeats = p.loop_repeats(reg);
            let len = p.region_len_beats(reg);
            let mut body = reg.clone();
            body.params.loop_beats = 0.0;
            p.tracks[t].regions[i].params.loop_beats = 0.0;
            let mut at_index = i + 1;
            for (at, plays) in repeats {
                let mut copy = body.clone();
                place(&mut copy, p.sigs(), at, len);
                p.tracks[t].regions.insert(at_index, copy);
                if plays < len - EPS && p.split_region(t, at_index, at + plays) {
                    p.tracks[t].regions.remove(at_index + 1);
                }
                at_index += 1;
            }
            i = at_index;
        }
    }
}

/// Cuts every region that crosses one of `cuts` there.
fn split_at(p: &mut Project, cuts: &[f64]) {
    for t in 0..p.tracks.len() {
        // From the last region down: a cut puts the right piece after the left one.
        for i in (0..p.tracks[t].regions.len()).rev() {
            // The later cut first: the left piece keeps index `i` for the earlier one.
            for &c in cuts.iter().rev() {
                if p.can_split_region(t, i, c) {
                    p.split_region(t, i, c);
                }
            }
        }
    }
}

/// Every track's regions after the edit: (region, start before, start after, length), in
/// quarter notes measured in the bars before the edit.
#[allow(clippy::type_complexity)]
fn place_regions(
    p: &mut Project,
    edit: SectionEdit,
    l: f64,
    r: f64,
) -> Vec<Vec<(Region, f64, f64, f64)>> {
    let mut out = Vec::with_capacity(p.tracks.len());
    for t in 0..p.tracks.len() {
        let regions = std::mem::take(&mut p.tracks[t].regions);
        let mut list = Vec::with_capacity(regions.len());
        let mut copies = Vec::new();
        for reg in regions {
            let start = p.region_start_beats(&reg);
            let len = p.region_len_beats(&reg);
            let end = start + len;
            let inside = start >= l - EPS && end <= r + EPS;
            let new_start = match edit {
                SectionEdit::Cut if inside => continue,
                // Starts in the section but could not be cut at the right locator.
                SectionEdit::Cut if start >= l - EPS && start < r - EPS => l,
                _ => moved(edit, l, r, start).unwrap_or(l),
            };
            if edit == SectionEdit::Repeat && inside {
                copies.push((reg.clone(), start, start + (r - l), len));
            }
            list.push((reg, start, new_start, len));
        }
        list.extend(copies);
        list.sort_by(|a, b| a.2.total_cmp(&b.2));
        out.push(list);
    }
    out
}

/// Moves the markers: a cut removes the ones in the section, a repeat copies them.
fn move_markers(p: &mut Project, edit: SectionEdit, l: f64, r: f64) {
    let mut out = Vec::with_capacity(p.markers.len());
    let mut copies = Vec::new();
    for m in p.markers.drain(..) {
        if let Some(b) = copied(edit, l, r, m.beat) {
            let mut c = m.clone();
            c.beat = b;
            copies.push(c);
        }
        if let Some(b) = moved(edit, l, r, m.beat) {
            out.push(crate::model::Marker { beat: b, ..m });
        }
    }
    out.extend(copies);
    p.markers = out;
    p.tidy_markers();
}

/// Whether the tempo is on a curve (ramping) at `beat`.
fn ramp_at(p: &Project, beat: f64) -> bool {
    let i = p.tempo_changes.partition_point(|t| t.beat <= beat);
    let ramp = match i.checked_sub(1) {
        Some(j) => p.tempo_changes[j].ramp,
        None => p.tempo_ramp,
    };
    ramp && i < p.tempo_changes.len()
}

/// Moves the tempo changes, keeping the tempo continuous around the edit.
fn move_tempo(p: &mut Project, edit: SectionEdit, l: f64, r: f64) {
    let len = r - l;
    let (at_l, at_r) = (p.tempo_at(l), p.tempo_at(r));
    let (ramp_l, ramp_r) = (ramp_at(p, l), ramp_at(p, r));
    // Points made here go first, so a real change on the same beat wins (see `tidy_tempo`).
    let mut made: Vec<TempoPoint> = Vec::new();
    let point = |beat, bpm, ramp| TempoPoint { beat, bpm, ramp };
    match edit {
        SectionEdit::Cut => {
            let changes_inside = p
                .tempo_changes
                .iter()
                .any(|t| t.beat >= l - EPS && t.beat <= r + EPS);
            if changes_inside || at_l != at_r || ramp_l != ramp_r {
                made.push(point(l, at_r, ramp_r));
            }
        }
        SectionEdit::InsertSilence => {
            if ramp_l {
                // The curve waits through the silence and goes on after it.
                made.push(point(l, at_l, false));
                made.push(point(l + len, at_l, true));
            }
        }
        SectionEdit::Repeat => {
            made.push(point(r, at_l, ramp_l));
            made.push(point(r + len, at_r, ramp_r));
        }
    }
    let mut out = Vec::with_capacity(p.tempo_changes.len() + made.len());
    let mut copies = Vec::new();
    for t in &p.tempo_changes {
        if let Some(b) = copied(edit, l, r, t.beat) {
            copies.push(TempoPoint { beat: b, ..*t });
        }
        if let Some(b) = moved(edit, l, r, t.beat) {
            out.push(TempoPoint { beat: b, ..*t });
        }
    }
    // A point made at the very start is the start tempo.
    let mut all: Vec<TempoPoint> = made.into_iter().chain(out).chain(copies).collect();
    all.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    if let Some(first) = all.iter().rev().find(|t| t.beat <= EPS) {
        p.tempo = first.bpm;
        p.tempo_ramp = first.ramp;
    }
    p.tempo_changes = all;
    p.tidy_tempo();
    // A change that only repeats the tempo before it changes nothing.
    let mut prev = (p.tempo, p.tempo_ramp);
    p.tempo_changes.retain(|t| {
        let same = t.bpm == prev.0 && !t.ramp && !prev.1;
        prev = (t.bpm, t.ramp);
        !same
    });
    p.tidy_tempo();
}

/// Moves the signature changes. Returns whether the signatures changed.
fn move_signatures(p: &mut Project, edit: SectionEdit, l: f64, r: f64) -> bool {
    let len = r - l;
    let old = p.sigs().to_owned();
    let sigs = old.as_ref();
    let sig_at = |beat: f64| sigs.signature_of_bar(sigs.beats_to_bar(beat.max(0.0)) as u32);
    // (beat, signature), made ones first so a real change on the same bar wins.
    let mut events: Vec<(f64, (u8, u8))> = Vec::new();
    match edit {
        SectionEdit::Cut => {
            let inside = old.changes.iter().any(|c| {
                let b = sigs.bar_to_beats(f64::from(c.bar - 1));
                b >= l - EPS && b <= r + EPS
            });
            if inside {
                events.push((l, sig_at(r)));
            }
        }
        SectionEdit::InsertSilence => {}
        SectionEdit::Repeat => {
            events.push((r, sig_at(l)));
            events.push((r + len, sig_at(r)));
        }
    }
    let mut copies = Vec::new();
    for c in &old.changes {
        let b = sigs.bar_to_beats(f64::from(c.bar - 1));
        if let Some(to) = copied(edit, l, r, b) {
            copies.push((to, (c.num, c.den)));
        }
        if let Some(to) = moved(edit, l, r, b) {
            events.push((to, (c.num, c.den)));
        }
    }
    events.extend(copies);
    events.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Each change on the bar line nearest to it, in the bars the changes before it make.
    let mut first = old.first;
    let mut changes: Vec<SigChange> = Vec::new();
    for (beat, (num, den)) in events {
        let map = SigRef {
            first,
            changes: &changes,
        };
        let bar = map.beats_to_bar(beat.max(0.0)).round() as u32 + 1;
        if bar <= 1 {
            first = (num, den);
            continue;
        }
        while changes.last().is_some_and(|c| c.bar >= bar) {
            changes.pop();
        }
        changes.push(SigChange { bar, num, den });
    }
    // A change to the signature already in effect changes nothing.
    let mut prev = first;
    changes.retain(|c| {
        let same = (c.num, c.den) == prev;
        prev = (c.num, c.den);
        !same
    });
    let changed = first != old.first || changes != old.changes;
    p.sig = first;
    p.sig_changes = changes;
    p.tidy_signatures();
    changed
}

/// Every lane of breakpoints of a strip's automation.
fn lanes_mut(a: &mut crate::automation::Automation) -> Vec<&mut Vec<AutoPoint>> {
    let mut out: Vec<&mut Vec<AutoPoint>> = vec![&mut a.volume, &mut a.pan, &mut a.mute];
    out.extend(a.sends.iter_mut().map(|l| &mut l.points));
    out.extend(a.fx.iter_mut().map(|l| &mut l.points));
    out.extend(a.plugins.iter_mut().map(|l| &mut l.points));
    out
}

/// The curve of the stretch of `pts` that `beat` is in.
fn curve_at(pts: &[AutoPoint], beat: f64) -> f32 {
    let i = pts.partition_point(|p| p.beat <= beat);
    i.checked_sub(1).map_or(0.0, |j| pts[j].curve)
}

/// Moves the breakpoints of every lane of `a`, keeping each envelope's values at the edges.
fn move_automation(a: &mut crate::automation::Automation, edit: SectionEdit, l: f64, r: f64) {
    let len = r - l;
    for pts in lanes_mut(a) {
        // A lane that ends before the section is flat from then on: nothing to move.
        if pts.is_empty() || pts.iter().all(|p| p.beat < l - EPS) {
            continue;
        }
        let (Some(vl), Some(vr)) = (lane_value(pts, l), lane_value(pts, r)) else {
            continue;
        };
        let (cl, cr) = (curve_at(pts, l), curve_at(pts, r));
        let at = |beat, value, curve| AutoPoint { beat, value, curve };
        let mut out: Vec<AutoPoint> = pts.iter().copied().filter(|p| p.beat < l - EPS).collect();
        match edit {
            SectionEdit::Cut => {
                out.push(at(l, vl, cl));
                out.push(at(l, vr, cr));
                out.extend(
                    pts.iter()
                        .filter(|p| p.beat > r + EPS)
                        .map(|p| at(p.beat - len, p.value, p.curve)),
                );
            }
            SectionEdit::InsertSilence => {
                out.push(at(l, vl, 0.0));
                out.push(at(l + len, vl, cl));
                out.extend(
                    pts.iter()
                        .filter(|p| p.beat > l + EPS)
                        .map(|p| at(p.beat + len, p.value, p.curve)),
                );
            }
            SectionEdit::Repeat => {
                let inner: Vec<AutoPoint> = pts
                    .iter()
                    .copied()
                    .filter(|p| p.beat > l + EPS && p.beat < r - EPS)
                    .collect();
                // The section, then its copy from the right locator, then the rest.
                out.push(at(l, vl, cl));
                out.extend(inner.iter().copied());
                out.push(at(r, vr, 0.0));
                out.push(at(r, vl, cl));
                out.extend(inner.iter().map(|p| at(p.beat + len, p.value, p.curve)));
                out.push(at(r + len, vr, cr));
                out.extend(
                    pts.iter()
                        .filter(|p| p.beat > r + EPS)
                        .map(|p| at(p.beat + len, p.value, p.curve)),
                );
            }
        }
        // Two points with the same place and value are one.
        out.dedup_by(|b, a| (a.beat - b.beat).abs() <= EPS && a.value == b.value);
        *pts = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Note, RegionParams};

    fn note(start: f32) -> Note {
        Note {
            lyric: crate::lyrics::Lyric::NONE,
            mute: false,
            start,
            len: 1.0,
            pitch: 60,
            vel: 90,
        }
    }

    fn region(name: &str, start_bar: f32, len_bars: f32, notes: Vec<Note>) -> Region {
        Region {
            missing: None,
            name: name.into(),
            start_bar,
            len_bars,
            notes,
            controls: Vec::new(),
            folder: None,
            audio: None,
            params: RegionParams::default(),
            generated: None,
        }
    }

    /// (name, start, end) of every region of track 0, in quarter notes.
    fn spans(p: &Project) -> Vec<(String, f64, f64)> {
        p.tracks[0]
            .regions
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    p.region_start_beats(r),
                    p.region_end_beats(r),
                )
            })
            .collect()
    }

    fn span(name: &str, a: f64, b: f64) -> (String, f64, f64) {
        (name.into(), a, b)
    }

    fn pt(beat: f64, value: f32) -> AutoPoint {
        AutoPoint {
            beat,
            value,
            curve: 0.0,
        }
    }

    /// A 4/4 song: A over bars 1-2, B over bars 3-6 (a note in each bar), C in bar 9; a
    /// marker in bar 5 and one in bar 7, a tempo change in bar 8, a signature change at bar 10,
    /// volume automation; the locators on bars 4 and 6 (beats 12 to 20).
    fn song() -> Project {
        let mut p = Project::empty();
        p.tracks[0].regions = vec![
            region("A", 1.0, 2.0, vec![note(0.0)]),
            region(
                "B",
                3.0,
                4.0,
                vec![note(0.0), note(4.0), note(8.0), note(12.0)],
            ),
            region("C", 9.0, 1.0, vec![note(0.0)]),
        ];
        p.add_marker(16.0);
        p.add_marker(24.0);
        p.tempo = 120.0;
        p.tempo_changes = vec![TempoPoint {
            beat: 28.0,
            bpm: 90.0,
            ramp: false,
        }];
        p.sig_changes = vec![SigChange {
            bar: 10,
            num: 3,
            den: 4,
        }];
        p.tracks[0].auto.volume = vec![pt(0.0, -6.0), pt(16.0, 0.0), pt(28.0, -12.0)];
        (p.cycle_start, p.cycle_end) = (12.0, 20.0);
        p
    }

    #[test]
    fn a_cut_takes_the_section_out_of_every_track_and_closes_the_gap() {
        let mut p = song();
        apply(&mut p, SectionEdit::Cut);
        assert_eq!(
            spans(&p),
            vec![
                span("A", 0.0, 8.0),
                span("B", 8.0, 12.0),
                span("B", 12.0, 16.0),
                span("C", 24.0, 28.0)
            ]
        );
        // B's notes in bars 4 and 5 are gone: one note in each piece that is left.
        let notes: Vec<usize> = p.tracks[0].regions.iter().map(|r| r.notes.len()).collect();
        assert_eq!(notes, vec![1, 1, 1, 1]);
        let markers: Vec<f64> = p.markers.iter().map(|m| m.beat).collect();
        assert_eq!(
            markers,
            vec![16.0],
            "the one inside goes, the one after moves left"
        );
        assert_eq!(p.tempo_changes.len(), 1);
        assert_eq!(
            (p.tempo_changes[0].beat, p.tempo_changes[0].bpm),
            (20.0, 90.0)
        );
        assert_eq!(
            p.sig_changes,
            vec![SigChange {
                bar: 8,
                num: 3,
                den: 4
            }]
        );
        // The automation after the section moves left; the value at the right locator (it was
        // ramping from 0 dB at beat 16 to -12 dB at 28: -4 dB at 20) carries on from the left one.
        let vol = &p.tracks[0].auto.volume;
        assert_eq!(vol.last().map(|q| (q.beat, q.value)), Some((20.0, -12.0)));
        assert_eq!(lane_value(vol, 12.0 + 1e-3).map(|v| v.round()), Some(-4.0));
        assert_eq!(
            (p.cycle_start, p.cycle_end),
            (12.0, 20.0),
            "the locators stay"
        );
    }

    #[test]
    fn insert_silence_opens_a_gap_at_the_left_locator() {
        let mut p = song();
        apply(&mut p, SectionEdit::InsertSilence);
        assert_eq!(
            spans(&p),
            vec![
                span("A", 0.0, 8.0),
                span("B", 8.0, 12.0),
                span("B", 20.0, 32.0),
                span("C", 40.0, 44.0)
            ]
        );
        let markers: Vec<f64> = p.markers.iter().map(|m| m.beat).collect();
        assert_eq!(markers, vec![24.0, 32.0]);
        assert_eq!(p.tempo_changes[0].beat, 36.0);
        assert_eq!(
            p.sig_changes,
            vec![SigChange {
                bar: 12,
                num: 3,
                den: 4
            }]
        );
        let vol = &p.tracks[0].auto.volume;
        assert_eq!(vol.last().map(|q| q.beat), Some(36.0));
        // The value at the left locator holds through the silence.
        let at_l = lane_value(vol, 12.0);
        assert_eq!(lane_value(vol, 19.0), at_l);
    }

    #[test]
    fn repeat_copies_the_section_after_the_right_locator() {
        let mut p = song();
        apply(&mut p, SectionEdit::Repeat);
        assert_eq!(
            spans(&p),
            vec![
                span("A", 0.0, 8.0),
                span("B", 8.0, 12.0),
                span("B", 12.0, 20.0),
                span("B", 20.0, 28.0),
                span("B", 28.0, 32.0),
                span("C", 40.0, 44.0),
            ]
        );
        let markers: Vec<f64> = p.markers.iter().map(|m| m.beat).collect();
        assert_eq!(
            markers,
            vec![16.0, 24.0, 32.0],
            "the marker inside is copied"
        );
        assert_eq!(
            p.tempo_changes.last().map(|t| (t.beat, t.bpm)),
            Some((36.0, 90.0))
        );
        assert_eq!(
            p.sig_changes,
            vec![SigChange {
                bar: 12,
                num: 3,
                den: 4
            }]
        );
        // The automation of the section plays again in the copy.
        let vol = &p.tracks[0].auto.volume;
        assert_eq!(lane_value(vol, 24.0), lane_value(vol, 16.0));
    }

    #[test]
    fn a_looped_region_crossing_a_locator_has_its_repeats_cut_too() {
        let mut p = Project::empty();
        let mut r = region("Loop", 1.0, 1.0, vec![note(0.0)]);
        r.params.loop_beats = 16.0;
        p.tracks[0].regions = vec![r];
        (p.cycle_start, p.cycle_end) = (8.0, 12.0);
        apply(&mut p, SectionEdit::Cut);
        // The loop ran 16 quarter notes past the region (bars 2-5): bar 3 goes, bars 4 and 5
        // move to bars 3 and 4.
        assert_eq!(
            spans(&p),
            vec![
                span("Loop", 0.0, 4.0),
                span("Loop", 4.0, 8.0),
                span("Loop", 8.0, 12.0),
                span("Loop", 12.0, 16.0)
            ]
        );
        assert!(p.tracks[0]
            .regions
            .iter()
            .all(|r| r.params.loop_beats == 0.0));
    }

    #[test]
    fn nothing_happens_without_a_section() {
        let mut p = song();
        p.cycle_end = p.cycle_start;
        let before = spans(&p);
        assert!(!can_apply(&p));
        apply(&mut p, SectionEdit::Cut);
        assert_eq!(spans(&p), before);
    }

    // ---- in the app: the menu, the keys, one undo step

    use crate::ui_test::Harness;
    use egui::{Event, Key, Modifiers};

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

    /// Control-Command: on macOS two keys, as egui reports them there.
    fn ctrl_cmd() -> Modifiers {
        Modifiers {
            ctrl: true,
            mac_cmd: true,
            command: true,
            ..Modifiers::NONE
        }
    }

    fn press(h: &mut Harness, p: &mut Project, s: &mut crate::UiState, ev: Event) {
        h.modifiers = ctrl_cmd();
        app_frame(h, p, s, vec![ev]);
        h.modifiers = Modifiers::NONE;
        app_frame(h, p, s, vec![]);
    }

    fn key(key: Key) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl_cmd(),
        }
    }

    #[test]
    fn the_menu_entries_run_the_edits_as_one_undo_step() {
        let mut p = song();
        let mut s = crate::UiState::new();
        let mut history = crate::undo::History::new(&p);
        for label in [
            "Cut Section Between Locators",
            "Insert Silence Between Locators",
            "Repeat Section Between Locators",
        ] {
            assert!(crate::menus::enabled(label, &p, &s), "{label}");
        }
        s.tracks.select_only(0, 1);
        crate::menus::apply("Cut Section Between Locators", &mut p, &mut s);
        assert_eq!(p.tracks[0].regions.len(), 4);
        assert!(
            s.tracks.selection().is_empty(),
            "the region indexes changed"
        );
        assert!(history.record(&p));
        assert!(history.undo(&mut p));
        assert_eq!(spans(&p), spans(&song()));
        // No section: the entries are off.
        p.cycle_end = p.cycle_start;
        assert!(!crate::menus::enabled(
            "Repeat Section Between Locators",
            &p,
            &s
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn control_command_x_i_and_r_cut_insert_and_repeat() {
        let mut p = song();
        let mut s = crate::UiState::new();
        let mut h = Harness::new(1400.0, 600.0);
        let (p, s, h) = (&mut p, &mut s, &mut h);
        app_frame(h, p, s, vec![]);
        let mut history = crate::undo::History::new(p);
        // Control-Command-X comes from the system as Cut.
        press(h, p, s, Event::Cut);
        assert_eq!(p.tracks[0].regions.len(), 4, "cut, not the Cut of regions");
        assert_eq!(p.markers.len(), 1);
        assert!(history.record(p));
        assert_eq!(s.undo_cmd, None);
        let mut q = song();
        apply(&mut q, SectionEdit::Cut);
        assert_eq!(spans(p), spans(&q));
        // Control-Command-I: Insert Silence, not Show Inspector.
        let inspector = s.show_inspector;
        press(h, p, s, key(Key::I));
        assert_eq!(s.show_inspector, inspector, "not Show Inspector");
        apply(&mut q, SectionEdit::InsertSilence);
        assert_eq!(spans(p), spans(&q));
        // Control-Command-R: Repeat Section, not Repeat of the selected regions.
        s.tracks.select_only(0, 0);
        press(h, p, s, key(Key::R));
        apply(&mut q, SectionEdit::Repeat);
        assert_eq!(spans(p), spans(&q));
    }
}
