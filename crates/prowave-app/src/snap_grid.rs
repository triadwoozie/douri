//! The Tracks area's grid settings beside the Snap mode (Logic Pro: the Snap menu of the
//! Tracks area and the Division of the LCD): Snap to Grid on or off (Command-G), the length of
//! a division, and whether a dragged region snaps by its start (Absolute Value) or keeps its
//! distance from the grid (Relative Value, Logic's default).
//!
//! With Snap to Grid off, a drag, the Scissors and every other place that snaps go to the
//! tick (1/960 of a quarter note), as with the Ticks mode.
use crate::tracks_area::Snap;

/// The finest place anything goes to: a tick, 1/960 of a quarter note.
pub(crate) const TICK: f64 = 1.0 / 960.0;

/// Logic's Division values: a whole note divided by this (1/4 to 1/192, the triplet ones
/// among them).
pub(crate) const DIVISIONS: [u16; 11] = [4, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grid {
    /// Snap to Grid (Command-G). Off: everything places to the tick.
    pub on: bool,
    /// The division: a whole note divided by this (16: sixteenth notes).
    pub division: u16,
    /// Snap Regions to Relative Value: a region off the grid keeps its distance from it when
    /// dragged. Off: its start goes to the grid (Absolute Value).
    pub relative: bool,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            on: true,
            division: 16,
            relative: true,
        }
    }
}

impl Grid {
    /// One division in quarter notes (0.25 for sixteenth notes).
    pub(crate) fn division_beats(&self) -> f64 {
        4.0 / f64::from(self.division.max(1))
    }

    /// The step everything in the Tracks area snaps to with Snap mode `snap`, at `beat_px`
    /// points per quarter note, in a bar of `bpb` quarter notes whose beat is `unit`. Off:
    /// the tick. Division is this grid's division; Smart takes the finest of division, beat
    /// and bar whose lines are at least 12 points apart.
    pub(crate) fn step(&self, snap: Snap, beat_px: f32, bpb: f64, unit: f64) -> f64 {
        if !self.on {
            return TICK;
        }
        let div = self.division_beats();
        let wide = |len: f64| f64::from(beat_px) * len >= 12.0;
        match snap {
            Snap::Smart if wide(div) => div,
            Snap::Smart if wide(unit) => unit,
            Snap::Smart | Snap::Bar => bpb,
            Snap::Beat => unit,
            Snap::Division => div,
            Snap::Ticks => TICK,
        }
    }

    /// Whether snapping goes to bar lines (which follow the signatures, so are no fixed step).
    pub(crate) fn snaps_to_bars(&self, snap: Snap, beat_px: f32, unit: f64) -> bool {
        let wide = |len: f64| f64::from(beat_px) * len >= 12.0;
        self.on
            && match snap {
                Snap::Bar => true,
                Snap::Smart => !wide(self.division_beats()) && !wide(unit),
                _ => false,
            }
    }

    /// Whether automation points go anywhere (Ticks, or Snap to Grid off).
    pub(crate) fn free(&self, snap: Snap) -> bool {
        !self.on || snap == Snap::Ticks
    }

    /// Where a dragged region whose start was `grabbed` lands when the pointer puts its start
    /// at `raw`, `snap` being the grid. Relative: it moves by whole grid steps from where it
    /// was; Absolute: its start goes to the grid.
    pub(crate) fn region_start(&self, raw: f64, grabbed: f64, snap: impl Fn(f64) -> f64) -> f64 {
        if self.relative {
            let off = grabbed - snap(grabbed);
            (snap(raw - off) + off).max(0.0)
        } else {
            snap(raw)
        }
    }
}

/// A division as the menu prints it ("1/16").
pub(crate) fn division_label(n: u16) -> String {
    format!("1/{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_division_is_a_whole_note_divided() {
        let mut g = Grid::default();
        assert_eq!(g.division_beats(), 0.25);
        g.division = 192;
        assert!((g.division_beats() - 1.0 / 48.0).abs() < 1e-12);
        g.division = 12;
        assert!((g.division_beats() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(division_label(4), "1/4");
    }

    #[test]
    fn the_step_follows_the_mode_the_division_and_snap_to_grid() {
        let mut g = Grid::default();
        // 4/4, 20 points a quarter note: a sixteenth is 5 points, a beat 20.
        assert_eq!(g.step(Snap::Smart, 20.0, 4.0, 1.0), 1.0);
        assert_eq!(g.step(Snap::Division, 20.0, 4.0, 1.0), 0.25);
        assert_eq!(g.step(Snap::Bar, 20.0, 4.0, 1.0), 4.0);
        assert!(!g.snaps_to_bars(Snap::Smart, 20.0, 1.0) && g.snaps_to_bars(Snap::Smart, 5.0, 1.0));
        // Eighth-note division: Division and Smart (24 points an eighth) take it.
        g.division = 8;
        assert_eq!(g.step(Snap::Division, 30.0, 4.0, 1.0), 0.5);
        assert_eq!(g.step(Snap::Smart, 30.0, 4.0, 1.0), 0.5);
        // Off: everything to the tick, nothing to bars, automation free.
        g.on = false;
        for m in Snap::ALL {
            assert_eq!(g.step(m, 20.0, 4.0, 1.0), TICK);
            assert!(!g.snaps_to_bars(m, 1.0, 1.0) && g.free(m));
        }
    }

    #[test]
    fn relative_keeps_an_off_grid_region_off_the_grid_and_absolute_puts_it_on() {
        let beat = |b: f64| b.round();
        let g = Grid::default();
        // A region at 2.3 dragged so its start would be at 4.6: two beats on, still .3 off.
        assert!((g.region_start(4.6, 2.3, beat) - 4.3).abs() < 1e-9);
        let abs = Grid {
            relative: false,
            ..g
        };
        assert_eq!(abs.region_start(4.6, 2.3, beat), 5.0);
        // On the grid both are the same.
        assert_eq!(g.region_start(4.6, 2.0, beat), 5.0);
    }

    // ---- in the Tracks area: Command-G, the Snap menu, drags and the Scissors

    use crate::model::{Project, TrackKind};
    use crate::tracks_area::TrackTool;
    use crate::ui_test::Harness;
    use crate::UiState;
    use egui::{vec2, Key, Modifiers, PointerButton, Pos2, Rect};

    /// Two instrument tracks; track 0 has "R" (two bars at bar 2), track 1 is empty.
    fn rig() -> (Project, UiState) {
        let mut p = Project::empty();
        p.add_track(TrackKind::Instrument);
        p.tracks[0].regions = vec![crate::arrange_select::tests::region("R", 2.0, 2.0)];
        p.selected_track = 0;
        (p, UiState::new())
    }

    fn frame(h: &mut Harness, p: &mut Project, s: &mut UiState, ev: Vec<egui::Event>) {
        let meters = crate::mixer::Meters::default();
        h.frame(ev, |ui| {
            let ctx = ui.ctx().clone();
            crate::key_commands::begin_frame(&ctx);
            crate::menus::handle_keys(&ctx, p, s);
            crate::tracks_area::show(ui, p, s, &meters);
            crate::key_commands::handle(&ctx, p, s);
        });
    }

    fn rect_of(h: &Harness) -> Rect {
        h.ctx
            .read_response(egui::Id::unique((
                "tracks_area",
                ("region", 0usize, 0usize),
            )))
            .expect("the region is on screen")
            .rect
    }

    /// Drags region 0 of track 0 from 20 points into it by `dx` points; returns its start in
    /// quarter notes after, and the drag as one undo step.
    fn drag_by(h: &mut Harness, p: &mut Project, s: &mut UiState, dx: f32) -> f64 {
        let mut history = crate::undo::History::new(p);
        let meters = crate::mixer::Meters::default();
        let from: Pos2 = rect_of(h).left_center() + vec2(20.0, 0.0);
        let mut ui = |ui: &mut egui::Ui| crate::tracks_area::show(ui, p, s, &meters);
        h.press(from, PointerButton::Primary, &mut ui);
        h.move_to(from + vec2(10.0, 0.0), &mut ui);
        h.move_to(from + vec2(10.0 + dx / 2.0, 0.0), &mut ui);
        h.move_to(from + vec2(10.0 + dx, 0.0), &mut ui);
        h.release(from + vec2(10.0 + dx, 0.0), PointerButton::Primary, &mut ui);
        h.frame(vec![], &mut ui);
        assert!(history.record(p), "the drag is one undo step");
        let at = p.region_start_beats(&p.tracks[0].regions[0]);
        assert!(history.undo(p) && !history.can_undo());
        assert!(history.redo(p));
        at
    }

    /// Off the grid: not within a hundredth of a quarter note of a sixteenth.
    fn off_grid(b: f64) -> bool {
        let q = b * 4.0;
        (q - q.round()).abs() > 0.04
    }

    #[test]
    fn command_g_turns_snap_to_grid_off_and_on_and_the_snap_menu_shows_it() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(1400.0, 600.0));
        frame(h, p, s, vec![]);
        assert!(s.tracks.grid.on && crate::menus::checked("Snap to Grid", p, s));
        let c = crate::key_commands::combo_of("Snap to Grid").unwrap();
        assert_eq!(
            c.shortcut(),
            egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::G)
        );
        crate::arrange_select::tests::press(h, p, s, Key::G, Modifiers::COMMAND);
        assert!(!s.tracks.grid.on && !crate::menus::checked("Snap to Grid", p, s));
        crate::arrange_select::tests::press(h, p, s, Key::G, Modifiers::COMMAND);
        assert!(s.tracks.grid.on);
        // In the Edit menu's Snap submenu, with Relative (Logic's default) checked.
        let edit = crate::menus::MENUS
            .iter()
            .find(|(m, _)| *m == "Edit")
            .unwrap()
            .1;
        assert!(edit.contains(&"Snap"));
        assert!(crate::menus::checked(
            "Snap Regions to Relative Value",
            p,
            s
        ));
        crate::menus::apply("Snap Regions to Absolute Value", p, s);
        assert!(!s.tracks.grid.relative);
        assert!(crate::menus::checked(
            "Snap Regions to Absolute Value",
            p,
            s
        ));
        // G alone is not the toggle.
        crate::arrange_select::tests::press(h, p, s, Key::G, Modifiers::NONE);
        assert!(s.tracks.grid.on);
    }

    #[test]
    fn with_snap_off_a_drag_and_the_scissors_go_anywhere() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(1400.0, 600.0));
        frame(h, p, s, vec![]);
        frame(h, p, s, vec![]);
        // On (Smart at 24 points a quarter note: beats): the drag lands on a beat.
        let on = drag_by(h, p, s, 31.0);
        assert_eq!(on, on.round(), "{on}");
        assert!(on > 4.0);
        // Off: wherever the pointer put it.
        s.tracks.grid.on = false;
        let off = drag_by(h, p, s, 31.0);
        assert!(off_grid(off), "{off}");
        // The Scissors cut where clicked, not on the grid.
        s.tracks.tool = TrackTool::Scissors;
        let start = p.region_start_beats(&p.tracks[0].regions[0]);
        let at = rect_of(h).left_center() + vec2(29.0, 0.0);
        h.time_passes(1.0);
        let meters = crate::mixer::Meters::default();
        h.click(at, |ui| crate::tracks_area::show(ui, p, s, &meters));
        assert_eq!(p.tracks[0].regions.len(), 2, "cut");
        let cut = p.region_start_beats(&p.tracks[0].regions[1]);
        assert!(off_grid(cut - start) || off_grid(cut), "{cut}");
        assert!((cut - start - 29.0 / 24.0).abs() < 0.1, "{cut}");
    }

    #[test]
    fn a_dragged_off_grid_region_keeps_its_offset_unless_snapping_is_absolute() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(1400.0, 600.0));
        // A quarter note is 24 points; the region starts 0.3 of one after beat 4.
        p.tracks[0].regions[0].start_bar = 2.075;
        frame(h, p, s, vec![]);
        frame(h, p, s, vec![]);
        let rel = drag_by(h, p, s, 48.0);
        assert!(((rel - 0.3) - (rel - 0.3).round()).abs() < 1e-6, "{rel}");
        assert!(rel > 5.0, "{rel}");
        s.tracks.grid.relative = false;
        let abs = drag_by(h, p, s, 48.0);
        assert_eq!(abs, abs.round(), "{abs}");
    }
}
