//! The cycle area: the strip along the top of the ruler (Logic Pro: Tracks area > ruler >
//! cycle area). As in Logic:
//!
//! - A click toggles Cycle mode.
//! - A drag on the middle of the cycle band moves it; a drag on its left or right edge moves
//!   that locator alone; a drag anywhere else in the strip draws a new cycle.
//! - Every position snaps as the Snap menu says (bars, beats, divisions or ticks), not always
//!   to bars.
//!
//! Navigate > Move Locators Forward / Backward by Cycle Length move the whole cycle by its own
//! length (Logic's Shift-Command-Period / Shift-Command-Comma).
use crate::model::Project;
use egui::{Rect, Sense, Ui};

/// How wide each end of the band reacts as an edge, in points (less on a narrow band, so its
/// middle can still be grabbed).
pub(crate) const EDGE: f32 = 6.0;

/// What a drag in the cycle area does, decided where it began.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum CycleGrab {
    /// The whole band: the pointer's beat at the press, and the locators then.
    Move { from: f64, start: f64, end: f64 },
    /// The left locator; the right one stays at `end`.
    Left { end: f64 },
    /// The right locator; the left one stays at `start`.
    Right { start: f64 },
    /// A new cycle from `anchor` (already snapped).
    New { anchor: f64 },
}

/// What a press at `x` (the beat `beat`) begins, for a band drawn from `x0` to `x1`.
pub(crate) fn grab_at(
    p: &Project,
    x: f32,
    beat: f64,
    x0: f32,
    x1: f32,
    snap: &dyn Fn(f64) -> f64,
) -> CycleGrab {
    let edge = EDGE.min((x1 - x0) / 3.0).max(0.0);
    let on_band = x >= x0 - edge && x <= x1 + edge && x1 > x0;
    if on_band && (x - x0).abs() <= edge {
        CycleGrab::Left { end: p.cycle_end }
    } else if on_band && (x - x1).abs() <= edge {
        CycleGrab::Right {
            start: p.cycle_start,
        }
    } else if on_band {
        CycleGrab::Move {
            from: beat,
            start: p.cycle_start,
            end: p.cycle_end,
        }
    } else {
        CycleGrab::New {
            anchor: snap(beat).max(0.0),
        }
    }
}

/// The locators a drag begun as `grab` gives with the pointer at `beat`, or `None` while it
/// would make an empty cycle.
pub(crate) fn drag_to(grab: CycleGrab, beat: f64, snap: &dyn Fn(f64) -> f64) -> Option<(f64, f64)> {
    let span = |a: f64, b: f64| {
        let (s, e) = (a.min(b).max(0.0), a.max(b).max(0.0));
        (e - s > 1e-6).then_some((s, e))
    };
    match grab {
        CycleGrab::Move { from, start, end } => {
            let s = snap(start + (beat - from)).max(0.0);
            Some((s, s + (end - start)))
        }
        CycleGrab::Left { end } => span(snap(beat), end),
        CycleGrab::Right { start } => span(start, snap(beat)),
        CycleGrab::New { anchor } => span(anchor, snap(beat)),
    }
}

fn drag_id() -> egui::Id {
    egui::Id::unique(("cycle_area", "drag"))
}

/// The id of the strip's interaction (tests find it on screen with this).
pub(crate) fn strip_id() -> egui::Id {
    egui::Id::unique(("cycle_area", "strip"))
}

/// The cycle area's clicks and drags, in `strip` (the top of the ruler). `x_of` and `beat_of`
/// convert between beats and screen x as the Tracks area does; `snap` is its Snap setting.
pub(crate) fn interact(
    ui: &Ui,
    p: &mut Project,
    strip: Rect,
    x_of: &dyn Fn(f64) -> f32,
    beat_of: &dyn Fn(f32) -> f64,
    snap: &dyn Fn(f64) -> f64,
) {
    let resp = ui.interact(strip, strip_id(), Sense::click_and_drag());
    let (x0, x1) = (x_of(p.cycle_start), x_of(p.cycle_end));
    let held: Option<CycleGrab> = ui.ctx().data(|d| d.get_temp(drag_id()));
    // The pointer shows what a drag would do: the edges resize, the middle moves.
    if let Some(pt) = resp.hover_pos() {
        let shown = held.unwrap_or_else(|| grab_at(p, pt.x, beat_of(pt.x), x0, x1, snap));
        let icon = match shown {
            CycleGrab::Left { .. } | CycleGrab::Right { .. } => egui::CursorIcon::ResizeHorizontal,
            CycleGrab::Move { .. } if held.is_some() => egui::CursorIcon::Grabbing,
            CycleGrab::Move { .. } => egui::CursorIcon::Grab,
            CycleGrab::New { .. } => egui::CursorIcon::Default,
        };
        if icon != egui::CursorIcon::Default {
            ui.ctx().set_cursor_icon(icon);
        }
    }
    if resp.drag_started() {
        // Where the button went down, not where the drag was recognised a few points later.
        let from = ui
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos());
        if let Some(pt) = from {
            let grab = grab_at(p, pt.x, beat_of(pt.x), x0, x1, snap);
            ui.ctx().data_mut(|d| d.insert_temp(drag_id(), grab));
        }
    }
    let held: Option<CycleGrab> = ui.ctx().data(|d| d.get_temp(drag_id()));
    if let (Some(grab), true, Some(pt)) = (held, resp.dragged(), resp.interact_pointer_pos()) {
        if let Some((start, end)) = drag_to(grab, beat_of(pt.x), snap) {
            p.cycle_start = start;
            p.cycle_end = end;
            // Moving the band leaves Cycle as it was (Logic); setting a new range turns it on.
            if !matches!(grab, CycleGrab::Move { .. }) {
                p.cycle = true;
            }
        }
    }
    if resp.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<CycleGrab>(drag_id()));
    }
    if resp.clicked() {
        p.cycle = !p.cycle;
    }
}

/// Navigate > Move Locators Forward / Backward by Cycle Length: the cycle moves by its own
/// length (not before the start of the song).
pub(crate) fn move_by_length(p: &mut Project, forward: bool) {
    let len = p.cycle_end - p.cycle_start;
    if len <= 0.0 {
        return;
    }
    let start = if forward {
        p.cycle_end
    } else {
        (p.cycle_start - len).max(0.0)
    };
    p.cycle_start = start;
    p.cycle_end = start + len;
}

/// Whether Move Locators Backward by Cycle Length can move the cycle (it is not at the start).
pub(crate) fn can_move_back(p: &Project) -> bool {
    p.cycle_start > 1e-9 && p.cycle_end > p.cycle_start
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks_area::Snap;
    use crate::ui_test::Harness;
    use egui::{pos2, PointerButton, Pos2};

    fn show(h: &mut Harness, p: &mut Project, s: &mut crate::UiState) {
        let meters = crate::mixer::Meters::default();
        h.settle(|ui| crate::tracks_area::show(ui, p, s, &meters));
    }

    /// A drag along the cycle strip from `from` to `to` (x), at the strip's height.
    fn drag(h: &mut Harness, p: &mut Project, s: &mut crate::UiState, from: f32, to: f32) {
        let meters = crate::mixer::Meters::default();
        let y = strip(h).center().y;
        let path: Vec<Pos2> = [from, from + (to - from) * 0.1, from + (to - from) * 0.5, to]
            .into_iter()
            .map(|x| pos2(x, y))
            .collect();
        h.drag(&path, PointerButton::Primary, |ui| {
            crate::tracks_area::show(ui, p, s, &meters)
        });
    }

    fn strip(h: &Harness) -> Rect {
        h.ctx
            .read_response(strip_id())
            .expect("the strip is on screen")
            .rect
    }

    /// The screen x of `beat` (a bar is 96 points at the start, 24 per quarter note).
    fn x_at(h: &Harness, beat: f64) -> f32 {
        strip(h).left() + beat as f32 * 24.0
    }

    fn rig() -> (Project, crate::UiState, Harness) {
        let mut p = Project::empty();
        (p.cycle, p.cycle_start, p.cycle_end) = (true, 8.0, 16.0);
        let mut s = crate::UiState::new();
        s.tracks.snap = Snap::Bar;
        (p, s, Harness::new(1400.0, 600.0))
    }

    #[test]
    fn dragging_the_middle_of_the_band_moves_the_cycle() {
        let (mut p, mut s, mut h) = rig();
        show(&mut h, &mut p, &mut s);
        // Grab bar 4 (the middle of bars 3-5) and drag two bars right.
        let x = x_at(&h, 12.0);
        drag(&mut h, &mut p, &mut s, x, x + 2.0 * 96.0);
        assert_eq!((p.cycle_start, p.cycle_end), (16.0, 24.0));
        assert!(p.cycle, "moving it leaves Cycle on");
        // With Cycle off, the band moves and Cycle stays off.
        p.cycle = false;
        let x = x_at(&h, 20.0);
        drag(&mut h, &mut p, &mut s, x, x - 96.0);
        assert_eq!((p.cycle_start, p.cycle_end, p.cycle), (12.0, 20.0, false));
    }

    #[test]
    fn dragging_an_edge_moves_that_locator_alone() {
        let (mut p, mut s, mut h) = rig();
        show(&mut h, &mut p, &mut s);
        let left = x_at(&h, 8.0) + 1.0;
        drag(&mut h, &mut p, &mut s, left, left - 96.0);
        assert_eq!((p.cycle_start, p.cycle_end), (4.0, 16.0));
        let right = x_at(&h, 16.0) - 1.0;
        drag(&mut h, &mut p, &mut s, right, right + 2.0 * 96.0);
        assert_eq!((p.cycle_start, p.cycle_end), (4.0, 24.0));
        // The pointer over an edge shows the resize arrows, over the middle a hand.
        let meters = crate::mixer::Meters::default();
        let y = strip(&h).center().y;
        h.move_to(pos2(x_at(&h, 24.0) - 1.0, y), |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        assert_eq!(h.cursor, egui::CursorIcon::ResizeHorizontal);
        h.move_to(pos2(x_at(&h, 14.0), y), |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        assert_eq!(h.cursor, egui::CursorIcon::Grab);
    }

    #[test]
    fn a_drag_outside_the_band_draws_a_new_cycle_snapped_as_the_snap_menu_says() {
        let (mut p, mut s, mut h) = rig();
        p.cycle = false;
        show(&mut h, &mut p, &mut s);
        // Bars: from bar 7 to bar 9.
        let (x, to) = (x_at(&h, 24.0) + 5.0, x_at(&h, 32.0) - 6.0);
        drag(&mut h, &mut p, &mut s, x, to);
        assert_eq!((p.cycle, p.cycle_start, p.cycle_end), (true, 24.0, 32.0));
        // Snap to beats: the locators land on beats, not bars.
        s.tracks.snap = Snap::Beat;
        let (x, to) = (x_at(&h, 33.0) + 3.0, x_at(&h, 38.0) - 4.0);
        drag(&mut h, &mut p, &mut s, x, to);
        assert_eq!((p.cycle_start, p.cycle_end), (33.0, 38.0));
        // Divisions: sixteenth notes.
        s.tracks.snap = Snap::Division;
        let (x, to) = (x_at(&h, 6.25) + 1.0, x_at(&h, 7.75) + 1.0);
        drag(&mut h, &mut p, &mut s, x, to);
        assert_eq!((p.cycle_start, p.cycle_end), (6.25, 7.75));
    }

    #[test]
    fn a_click_still_toggles_cycle() {
        let (mut p, mut s, mut h) = rig();
        show(&mut h, &mut p, &mut s);
        let meters = crate::mixer::Meters::default();
        let at = pos2(x_at(&h, 30.0), strip(&h).center().y);
        h.click(at, |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        assert!(!p.cycle);
        h.time_passes(1.0);
        h.click(at, |ui| {
            crate::tracks_area::show(ui, &mut p, &mut s, &meters)
        });
        assert!(p.cycle);
        assert_eq!(
            (p.cycle_start, p.cycle_end),
            (8.0, 16.0),
            "a click moves nothing"
        );
    }

    #[test]
    fn grabs_follow_the_band() {
        let p = {
            let mut p = Project::empty();
            (p.cycle_start, p.cycle_end) = (8.0, 16.0);
            p
        };
        let snap = |b: f64| (b / 4.0).round() * 4.0;
        // The band from x 100 to 200.
        assert!(matches!(
            grab_at(&p, 102.0, 8.1, 100.0, 200.0, &snap),
            CycleGrab::Left { .. }
        ));
        assert!(matches!(
            grab_at(&p, 199.0, 15.9, 100.0, 200.0, &snap),
            CycleGrab::Right { .. }
        ));
        assert!(matches!(
            grab_at(&p, 150.0, 12.0, 100.0, 200.0, &snap),
            CycleGrab::Move { .. }
        ));
        assert_eq!(
            grab_at(&p, 300.0, 21.0, 100.0, 200.0, &snap),
            CycleGrab::New { anchor: 20.0 }
        );
        // Dragging the left locator past the right one swaps their roles; an empty cycle is
        // not made.
        assert_eq!(
            drag_to(CycleGrab::Left { end: 16.0 }, 23.0, &snap),
            Some((16.0, 24.0))
        );
        assert_eq!(drag_to(CycleGrab::New { anchor: 20.0 }, 21.0, &snap), None);
        // The band is not moved before the start of the song.
        let mv = CycleGrab::Move {
            from: 12.0,
            start: 8.0,
            end: 16.0,
        };
        assert_eq!(drag_to(mv, 0.0, &snap), Some((0.0, 8.0)));
    }

    #[test]
    fn move_locators_by_cycle_length() {
        let mut p = Project::empty();
        (p.cycle_start, p.cycle_end) = (8.0, 14.0);
        move_by_length(&mut p, true);
        assert_eq!((p.cycle_start, p.cycle_end), (14.0, 20.0));
        move_by_length(&mut p, false);
        move_by_length(&mut p, false);
        assert_eq!((p.cycle_start, p.cycle_end), (2.0, 8.0));
        move_by_length(&mut p, false);
        assert_eq!(
            (p.cycle_start, p.cycle_end),
            (0.0, 6.0),
            "not before the start"
        );
        assert!(!can_move_back(&p));
    }
}
