//! Zooming and following the playhead in the Tracks area (Logic Pro: Zoom Horizontal In/Out,
//! Command-Right and Command-Left; Zoom Vertical In/Out, Command-Down and Command-Up; Toggle
//! Zoom to Fit Selection or All Contents, Z; View > Scroll in Play; the waveform zoom).
//!
//! - Horizontal zoom keeps a place still on screen: the start of the selected regions when
//!   there are some, otherwise the playhead; when that place is off screen, the middle of the
//!   view.
//! - Vertical zoom changes the track height and keeps the selected track's top still.
//! - Z with regions selected fits them into the lanes (width and track height); with none
//!   selected it fits everything. Z again, with the view not changed since, returns to the
//!   zoom and scroll from before.
//! - Scroll in Play: while playing, the playhead stays in the middle and the song moves under
//!   it, in place of Catch's paging.
//! - Waveform zoom draws audio waveforms taller (louder-looking) without changing the sound.
//! - The wheel (Logic): Option zooms vertically, Option-Command (and Command, and a pinch)
//!   horizontally, Option-Control both ways, around the pointer. Shift-Option zooms
//!   horizontally too (newer Logic versions are reported to use it).
//! - The Zoom tool (and Control-Option with any tool): a drag draws a rectangle and zooms so
//!   it fills the lanes; a click zooms in two times with the clicked place in the middle,
//!   Option-click zooms out.
use crate::model::Project;
use crate::tracks_area::TracksView;
use crate::UiState;

/// Horizontal zoom: points per bar.
pub(crate) const BAR_W: (f32, f32) = (6.0, 600.0);
/// Vertical zoom: the track height, as the control interface allows it.
pub(crate) const ROW_H: (f32, f32) = (20.0, 400.0);
/// One step of the zoom keys.
pub(crate) const STEP: f32 = 1.25;
/// The waveform zoom: from 1 (as recorded) up to 16 times taller, doubling each step.
pub(crate) const WAVE_MAX: f32 = 16.0;

/// A zoom and scroll of the Tracks area.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct View {
    pub bar_w: f32,
    pub row_h: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
}

impl View {
    fn of(v: &TracksView) -> Self {
        Self {
            bar_w: v.bar_w,
            row_h: v.row_h,
            scroll_x: v.scroll_x,
            scroll_y: v.scroll_y,
        }
    }

    fn put(self, v: &mut TracksView) {
        v.bar_w = self.bar_w;
        v.row_h = self.row_h;
        v.scroll_x = self.scroll_x;
        v.scroll_y = self.scroll_y;
    }

    fn near(self, o: Self) -> bool {
        (self.bar_w - o.bar_w).abs() < 1e-3
            && (self.row_h - o.row_h).abs() < 1e-3
            && (self.scroll_x - o.scroll_x).abs() < 0.5
            && (self.scroll_y - o.scroll_y).abs() < 0.5
    }
}

#[derive(Clone, Debug)]
pub struct ZoomState {
    /// Height of the lanes at the last frame (for fitting tracks).
    pub lanes_h: f32,
    /// What Z left: the view before it and the view it made.
    back: Option<(View, View)>,
    /// View > Scroll in Play.
    pub scroll_in_play: bool,
    /// How many times taller audio waveforms are drawn (1: as recorded).
    pub wave_zoom: f32,
    /// A Zoom tool press in progress: where it went down on screen, and whether it was a
    /// Control-Option press (which only zooms by a drag; its click is the context menu).
    band: Option<(egui::Pos2, bool)>,
}

impl Default for ZoomState {
    fn default() -> Self {
        Self {
            lanes_h: 500.0,
            back: None,
            scroll_in_play: false,
            wave_zoom: 1.0,
            band: None,
        }
    }
}

/// Where the last region (or its loop) of the song ends, in quarter notes.
pub(crate) fn contents_end(p: &Project) -> f64 {
    p.tracks
        .iter()
        .flat_map(|t| t.regions.iter())
        .map(|r| p.region_loop_end_beats(r))
        .fold(0.0, f64::max)
}

fn beat_px(p: &Project, bar_w: f32) -> f32 {
    bar_w / p.bar_len(0).max(1e-6) as f32
}

/// The place horizontal zoom keeps still: the earliest start of the selected regions, or
/// the playhead.
fn anchor(p: &Project, v: &TracksView) -> f64 {
    v.selection()
        .into_iter()
        .filter_map(|(t, r)| Some(p.region_start_beats(p.tracks.get(t)?.regions.get(r)?)))
        .reduce(f64::min)
        .unwrap_or(p.position_beats)
}

/// Zoom Horizontal In (`factor` > 1) or Out, keeping `anchor` where it is on screen.
pub(crate) fn zoom_h(p: &Project, v: &mut TracksView, factor: f32) {
    let beat = anchor(p, v) as f32;
    let w = v.lanes_w.max(1.0);
    let x = beat * beat_px(p, v.bar_w) - v.scroll_x;
    // Off screen: the middle of the view stays instead.
    let (beat, x) = if (0.0..=w).contains(&x) {
        (beat, x)
    } else {
        ((v.scroll_x + w / 2.0) / beat_px(p, v.bar_w), w / 2.0)
    };
    v.bar_w = (v.bar_w * factor).clamp(BAR_W.0, BAR_W.1);
    v.scroll_x = (beat * beat_px(p, v.bar_w) - x).max(0.0);
}

/// Zoom Vertical In (`factor` > 1) or Out: the track height, the selected track's top kept
/// where it is on screen.
pub(crate) fn zoom_v(p: &Project, s: &mut UiState, factor: f32) {
    let t = p.selected_track;
    let before =
        crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, s.tracks.row_h);
    let v = &mut s.tracks;
    v.row_h = (v.row_h * factor).clamp(ROW_H.0, ROW_H.1);
    let after = crate::tracks_area::rows_as_drawn(p, &s.auto, v.show_hidden, v.row_h);
    if let (Some(b), Some(a)) = (before.get(t), after.get(t)) {
        v.scroll_y = (a.0 - (b.0 - v.scroll_y)).max(0.0);
    }
}

/// The view that shows quarter notes `from..to` across the lanes and tracks `rows` (first
/// and last) down them.
fn fitted(p: &Project, s: &UiState, (from, to): (f64, f64), rows: Option<(usize, usize)>) -> View {
    let v = &s.tracks;
    let margin = 20.0;
    let bars = ((to - from) / p.bar_len(0).max(1e-6)).max(1e-3) as f32;
    let bar_w = ((v.lanes_w - 2.0 * margin).max(40.0) / bars).clamp(BAR_W.0, BAR_W.1);
    let scroll_x = (from as f32 * beat_px(p, bar_w) - margin).max(0.0);
    let (row_h, scroll_y) = match rows {
        Some((a, b)) => {
            // The rows' height is a straight line in the track height (rounding aside):
            // measure it at two heights and solve for the height that fills the lanes.
            let span = |h: f32| {
                let r = crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, h);
                (r[b].0 + r[b].1 - r[a].0, r[a].0)
            };
            let (h1, h2) = (100.0, 200.0);
            let (s1, s2) = (span(h1).0, span(h2).0);
            let per = ((s2 - s1) / (h2 - h1)).max(1e-3);
            let want = (v.zoom.lanes_h - 10.0).max(ROW_H.0);
            let row_h = (h1 + (want - s1) / per).clamp(ROW_H.0, ROW_H.1);
            (row_h, span(row_h).1)
        }
        None => (v.row_h, v.scroll_y),
    };
    View {
        bar_w,
        row_h,
        scroll_x,
        scroll_y,
    }
}

/// What Z fits: the selected regions' time and tracks, or (none selected) the whole song and
/// every shown track. `None`: there is nothing to fit.
fn fit_target(p: &Project, v: &TracksView) -> Option<((f64, f64), (usize, usize))> {
    let picked: Vec<(usize, usize)> = {
        let sel = v.selection();
        if sel.is_empty() {
            (0..p.tracks.len())
                .filter(|&t| !p.hidden_by_fold(t))
                .flat_map(|t| (0..p.tracks[t].regions.len()).map(move |r| (t, r)))
                .collect()
        } else {
            sel
        }
    };
    let mut span: Option<(f64, f64, usize, usize)> = None;
    for (t, r) in picked {
        let Some(reg) = p.tracks.get(t).and_then(|x| x.regions.get(r)) else {
            continue;
        };
        if p.hidden_by_fold(t) {
            continue;
        }
        let (a, b) = (p.region_start_beats(reg), p.region_loop_end_beats(reg));
        span = Some(match span {
            None => (a, b, t, t),
            Some((x, y, lo, hi)) => (x.min(a), y.max(b), lo.min(t), hi.max(t)),
        });
    }
    let (from, to, lo, hi) = span?;
    // The whole song is shown from its start, as Zoom to Fit does.
    let from = if v.selection().is_empty() { 0.0 } else { from };
    Some(((from, to.max(from + 1e-3)), (lo, hi)))
}

/// Toggle Zoom to Fit Selection or All Contents (Z).
pub(crate) fn toggle_fit(p: &Project, s: &mut UiState) {
    let now = View::of(&s.tracks);
    if let Some((before, made)) = s.tracks.zoom.back.take() {
        if made.near(now) {
            before.put(&mut s.tracks);
            return;
        }
    }
    let Some((time, rows)) = fit_target(p, &s.tracks) else {
        return;
    };
    let made = fitted(p, s, time, Some(rows));
    made.put(&mut s.tracks);
    s.tracks.zoom.back = Some((now, made));
}

/// Whether Z has something to do: a way back, or something to fit.
pub(crate) fn can_fit(p: &Project, v: &TracksView) -> bool {
    v.zoom.back.is_some() || fit_target(p, v).is_some()
}

/// Scroll in Play: the scroll that keeps the playhead in the middle of the lanes, `None`
/// when it is off or the song is not playing. Called by the Tracks area each frame.
pub(crate) fn scroll_in_play(p: &Project, v: &TracksView, beat_px: f32) -> Option<f32> {
    (v.zoom.scroll_in_play && p.playing)
        .then(|| (p.position_beats as f32 * beat_px - v.lanes_w / 2.0).max(0.0))
}

/// Waveform Zoom In or Out: doubles or halves the waveform zoom (1 to [`WAVE_MAX`]).
pub(crate) fn zoom_wave(v: &mut TracksView, bigger: bool) {
    let z = v.zoom.wave_zoom;
    v.zoom.wave_zoom = (if bigger { z * 2.0 } else { z / 2.0 }).clamp(1.0, WAVE_MAX);
}

/// The tool a press with `mods` takes instead of the chosen one: the Zoom tool while Control
/// and Option are held (Logic), whatever tool is chosen.
pub(crate) fn pressed_tool(mods: egui::Modifiers) -> Option<crate::tracks_area::TrackTool> {
    (mods.ctrl && mods.alt && !mods.command).then_some(crate::tracks_area::TrackTool::Zoom)
}

/// Fewer points than this between press and release make a click, not a rectangle.
const CLICK_SLOP: f32 = 4.0;

/// The Zoom tool over the lanes `lanes` (`tool` is the tool the pointer has now). Returns the
/// rectangle being dragged, to be drawn over the regions.
pub(crate) fn zoom_tool(
    ui: &egui::Ui,
    p: &Project,
    s: &mut UiState,
    lanes: egui::Rect,
    tool: crate::tracks_area::TrackTool,
) -> Option<egui::Rect> {
    use crate::tracks_area::TrackTool;
    let (pressed, released, origin, now, mods) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.press_origin(),
            i.pointer.interact_pos(),
            i.modifiers,
        )
    });
    if pressed && tool == TrackTool::Zoom {
        let mine =
            |pt: egui::Pos2| lanes.contains(pt) && ui.ctx().layer_id_at(pt) == Some(ui.layer_id());
        s.tracks.zoom.band = origin
            .filter(|&o| mine(o))
            .map(|o| (o, pressed_tool(mods).is_some()));
    }
    let (from, control_option) = s.tracks.zoom.band?;
    let to = now.unwrap_or(from);
    let rect = egui::Rect::from_two_pos(from, to).intersect(lanes);
    let dragged = (to - from).abs().max_elem() >= CLICK_SLOP;
    if !released {
        return dragged.then_some(rect);
    }
    s.tracks.zoom.band = None;
    if dragged {
        zoom_to_rect(p, s, lanes, rect);
    } else if !control_option {
        zoom_at(p, &mut s.tracks, lanes, from.x, !mods.alt);
    }
    None
}

/// The quarter note at screen x `x` over the lanes `lanes`.
fn beat_at(p: &Project, v: &TracksView, lanes: egui::Rect, x: f32) -> f64 {
    f64::from(((x - lanes.left() + v.scroll_x) / beat_px(p, v.bar_w)).max(0.0))
}

/// A Zoom tool click at screen x `x`: two times in (`zoom_in`) or out, that place in the
/// middle of the lanes.
fn zoom_at(p: &Project, v: &mut TracksView, lanes: egui::Rect, x: f32, zoom_in: bool) {
    let beat = beat_at(p, v, lanes, x) as f32;
    let f = if zoom_in { 2.0 } else { 0.5 };
    v.bar_w = (v.bar_w * f).clamp(BAR_W.0, BAR_W.1);
    v.scroll_x = (beat * beat_px(p, v.bar_w) - lanes.width() / 2.0).max(0.0);
}

/// A Zoom tool rectangle `rect` (on screen over `lanes`): the time and tracks it covers fill
/// the lanes.
fn zoom_to_rect(p: &Project, s: &mut UiState, lanes: egui::Rect, rect: egui::Rect) {
    let v = &s.tracks;
    let time = (
        beat_at(p, v, lanes, rect.left()),
        beat_at(p, v, lanes, rect.right()),
    );
    let rows = crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, v.row_h);
    let (y0, y1) = (
        rect.top() - lanes.top() + v.scroll_y,
        rect.bottom() - lanes.top() + v.scroll_y,
    );
    let hit: Vec<usize> = (0..rows.len())
        .filter(|&t| rows[t].1 > 0.0 && rows[t].0 < y1 && rows[t].0 + rows[t].1 > y0)
        .collect();
    let span = hit.first().zip(hit.last()).map(|(a, b)| (*a, *b));
    let made = fitted(p, s, time, span);
    made.put(&mut s.tracks);
}

/// The wheel over the Tracks area with `mods` held: `delta` its scroll this frame, `zoom` the
/// zoom egui made of it (Command-scroll or a pinch; 1: none). Zooms around `pointer` and
/// returns whether it did (so the wheel does not scroll as well).
pub(crate) fn wheel_zoom(
    p: &Project,
    v: &mut TracksView,
    lanes: egui::Rect,
    pointer: egui::Pos2,
    (delta, zoom, mods): (egui::Vec2, f32, egui::Modifiers),
) -> bool {
    // egui turns Command-scroll (and on some systems Control-scroll) into `zoom`;
    // Option-scroll stays a (vertical) scroll.
    let scrolled = (delta.y / 200.0).exp();
    let (h, vert) = if mods.alt && mods.ctrl && (zoom != 1.0 || delta.y != 0.0) {
        let f = if zoom != 1.0 { zoom } else { scrolled };
        (f, f)
    } else if zoom != 1.0 {
        (zoom, 1.0)
    } else if mods.alt && mods.shift && delta.x + delta.y != 0.0 {
        // Shift-Option: across, as newer Logic versions are reported to do (egui may turn the
        // Shift-scroll into a sideways one).
        (((delta.x + delta.y) / 200.0).exp(), 1.0)
    } else if mods.alt && delta.y != 0.0 {
        (1.0, scrolled)
    } else {
        return false;
    };
    if h != 1.0 {
        let x = (pointer.x - lanes.left()).max(0.0);
        let beat = (x + v.scroll_x) / beat_px(p, v.bar_w);
        v.bar_w = (v.bar_w * h).clamp(BAR_W.0, BAR_W.1);
        v.scroll_x = (beat * beat_px(p, v.bar_w) - x).max(0.0);
    }
    if vert != 1.0 {
        let y = (pointer.y - lanes.top()).max(0.0);
        let old = v.row_h;
        v.row_h = (v.row_h * vert).clamp(ROW_H.0, ROW_H.1);
        v.scroll_y = ((y + v.scroll_y) * v.row_h / old - y).max(0.0);
    }
    true
}

fn wave_id() -> egui::Id {
    egui::Id::unique("arrange_zoom_wave")
}

/// Publishes this frame's waveform zoom for the region drawing, which has no view at hand.
pub(crate) fn publish_wave_zoom(ctx: &egui::Context, v: &TracksView) {
    let z = v.zoom.wave_zoom;
    ctx.data_mut(|d| d.insert_temp(wave_id(), z));
}

/// The waveform zoom the region drawing multiplies the waveform's height by.
pub(crate) fn wave_zoom(ctx: &egui::Context) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(wave_id())).unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrange_select::tests::{press, rig};
    use crate::ui_test::Harness;
    use egui::{Key, Modifiers};

    const CMD: Modifiers = Modifiers::COMMAND;

    /// Where quarter note `beat` is on screen, from the lanes' left edge.
    fn x_of(p: &Project, v: &TracksView, beat: f64) -> f32 {
        beat as f32 * beat_px(p, v.bar_w) - v.scroll_x
    }

    #[test]
    fn command_arrows_zoom_and_keep_the_playhead_or_the_selection_still() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        s.tracks.lanes_w = 1000.0;
        p.position_beats = 16.0;
        s.tracks.scroll_x = 100.0;
        let x = x_of(p, &s.tracks, 16.0);
        let w = s.tracks.bar_w;
        press(h, p, s, Key::ArrowRight, CMD);
        assert!((s.tracks.bar_w - w * STEP).abs() < 1e-3);
        assert!(
            (x_of(p, &s.tracks, 16.0) - x).abs() < 0.01,
            "the playhead stays"
        );
        press(h, p, s, Key::ArrowLeft, CMD);
        press(h, p, s, Key::ArrowLeft, CMD);
        assert!((s.tracks.bar_w - w / STEP).abs() < 1e-3);
        assert!((x_of(p, &s.tracks, 16.0) - x).abs() < 0.01);
        // With regions selected, their start stays ("B" at bar 3 is beat 8).
        s.tracks.select_only(0, 2);
        let x = x_of(p, &s.tracks, 8.0);
        press(h, p, s, Key::ArrowRight, CMD);
        assert!((x_of(p, &s.tracks, 8.0) - x).abs() < 0.01);
        // At the limit the key does nothing and the entry is greyed.
        s.tracks.bar_w = BAR_W.1;
        assert!(!crate::menus::enabled("Zoom Horizontal In", p, s));
        // Plain Right is still Select Next Region, not zoom.
        press(h, p, s, Key::ArrowRight, Modifiers::NONE);
        assert_eq!(s.tracks.bar_w, BAR_W.1);
    }

    #[test]
    fn command_down_and_up_change_the_track_height_and_keep_the_selected_track_still() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        p.selected_track = 4;
        s.tracks.scroll_y = 30.0;
        let top = |p: &Project, s: &UiState| {
            crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, s.tracks.row_h)[4].0
                - s.tracks.scroll_y
        };
        let (h0, y0) = (s.tracks.row_h, top(p, s));
        press(h, p, s, Key::ArrowDown, CMD);
        assert!((s.tracks.row_h - h0 * STEP).abs() < 1e-3);
        assert!((top(p, s) - y0).abs() <= 2.0, "rows are whole points");
        press(h, p, s, Key::ArrowUp, CMD);
        press(h, p, s, Key::ArrowUp, CMD);
        assert!((s.tracks.row_h - h0 / STEP).abs() < 1e-3);
        assert_eq!(
            p.selected_track, 4,
            "Command-Up is not Select Previous Track"
        );
        for _ in 0..30 {
            press(h, p, s, Key::ArrowUp, CMD);
        }
        assert_eq!(s.tracks.row_h, ROW_H.0);
        assert!(!crate::menus::enabled("Zoom Vertical Out", p, s));
        // All four are in Navigate with their keys.
        let nav = crate::menus::MENUS
            .iter()
            .find(|(m, _)| *m == "Navigate")
            .unwrap()
            .1;
        for (label, key) in [
            ("Zoom Horizontal In", Key::ArrowRight),
            ("Zoom Horizontal Out", Key::ArrowLeft),
            ("Zoom Vertical In", Key::ArrowDown),
            ("Zoom Vertical Out", Key::ArrowUp),
        ] {
            assert!(nav.contains(&label), "{label}");
            let c = crate::key_commands::combo_of(label).unwrap();
            assert_eq!(c.shortcut(), egui::KeyboardShortcut::new(CMD, key));
        }
    }

    #[test]
    fn z_fits_the_selection_or_everything_and_z_again_goes_back() {
        let (mut p, mut s) = rig();
        let (p, s, h) = (&mut p, &mut s, &mut Harness::new(800.0, 400.0));
        (s.tracks.lanes_w, s.tracks.zoom.lanes_h) = (1000.0, 300.0);
        s.tracks.scroll_x = 333.0;
        let before = View::of(&s.tracks);
        // "D" and "E" on track 4: beats 4 to 16 (E ends at bar 5).
        s.tracks.select_many(&[(4, 0), (4, 1)]);
        press(h, p, s, Key::Z, Modifiers::NONE);
        let v = &s.tracks;
        assert!(
            (x_of(p, v, 4.0) - 20.0).abs() < 0.5,
            "the start at the left margin"
        );
        assert!(
            (x_of(p, v, 16.0) - (v.lanes_w - 20.0)).abs() < 0.5,
            "the end at the right"
        );
        let rows = crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, v.row_h);
        assert!(
            (rows[4].1 - (v.zoom.lanes_h - 10.0)).abs() <= 2.0,
            "the track fills the lanes"
        );
        assert!((rows[4].0 - v.scroll_y).abs() < 0.5, "and is at the top");
        // Z again: the view from before.
        press(h, p, s, Key::Z, Modifiers::NONE);
        assert_eq!(View::of(&s.tracks), before);
        // Nothing selected: the whole song from bar 1; a loop counts to its end.
        s.tracks.clear_selection();
        p.tracks[0].regions[0].params.loop_beats = 8.0;
        let end = contents_end(p);
        assert_eq!(end, 32.0, "C ends at beat 24, its loop at 32");
        press(h, p, s, Key::Z, Modifiers::NONE);
        assert_eq!(s.tracks.scroll_x, 0.0);
        assert!((x_of(p, &s.tracks, end) - (s.tracks.lanes_w - 40.0)).abs() < 0.5);
        // A zoom in between: Z fits again rather than going back.
        press(h, p, s, Key::ArrowRight, CMD);
        let zoomed = View::of(&s.tracks);
        press(h, p, s, Key::Z, Modifiers::NONE);
        assert_ne!(View::of(&s.tracks), zoomed);
        assert_eq!(s.tracks.scroll_x, 0.0);
        press(h, p, s, Key::Z, Modifiers::NONE);
        assert_eq!(View::of(&s.tracks), zoomed);
        // An empty song has nothing to fit.
        let mut empty = Project::empty();
        empty.tracks[0].regions.clear();
        assert!(!crate::menus::enabled(
            "Toggle Zoom to Fit Selection or All Contents",
            &empty,
            &UiState::new()
        ));
    }

    #[test]
    fn zoom_to_fit_counts_loops_and_signatures() {
        let (mut p, mut s) = rig();
        s.tracks.lanes_w = 1020.0;
        p.tracks[0].regions[0].params.loop_beats = 8.0;
        crate::tracks_area::zoom_to_fit(&p, &mut s.tracks, 1020.0);
        // 32 quarter notes are 8 bars of 4/4 across 1000 points.
        assert!((s.tracks.bar_w - 125.0).abs() < 1e-3);
    }

    #[test]
    fn scroll_in_play_keeps_the_playhead_in_the_middle() {
        let (mut p, mut s) = rig();
        let (p, s) = (&mut p, &mut s);
        let meters = crate::mixer::Meters::default();
        let mut h = Harness::new(1400.0, 600.0);
        let show = |h: &mut Harness, p: &mut Project, s: &mut UiState| {
            h.frame(vec![], |ui| crate::tracks_area::show(ui, p, s, &meters));
        };
        show(&mut h, p, s);
        assert!(!crate::menus::checked("Scroll in Play", p, s));
        crate::menus::apply("Scroll in Play", p, s);
        assert!(crate::menus::checked("Scroll in Play", p, s));
        p.playing = true;
        for beat in [40.0, 41.5, 60.0] {
            p.position_beats = beat;
            show(&mut h, p, s);
            let x = x_of(p, &s.tracks, beat);
            assert!((x - s.tracks.lanes_w / 2.0).abs() < 0.5, "{beat}: {x}");
        }
        // Off: Catch pages again, so the playhead is not held in the middle.
        crate::menus::apply("Scroll in Play", p, s);
        p.position_beats = 61.0;
        show(&mut h, p, s);
        assert!((x_of(p, &s.tracks, 61.0) - s.tracks.lanes_w / 2.0).abs() > 5.0);
    }

    #[test]
    fn waveform_zoom_doubles_up_to_sixteen_and_back() {
        let (mut p, mut s) = rig();
        let (p, s) = (&mut p, &mut s);
        assert!(!crate::menus::enabled("Waveform Zoom Out", p, s));
        for want in [2.0, 4.0, 8.0, 16.0, 16.0] {
            crate::menus::apply("Waveform Zoom In", p, s);
            assert_eq!(s.tracks.zoom.wave_zoom, want);
        }
        assert!(!crate::menus::enabled("Waveform Zoom In", p, s));
        crate::menus::apply("Waveform Zoom Out", p, s);
        assert_eq!(s.tracks.zoom.wave_zoom, 8.0);
        // The region drawing reads it from the frame the Tracks area drew.
        let ctx = egui::Context::default();
        assert_eq!(wave_zoom(&ctx), 1.0);
        publish_wave_zoom(&ctx, &s.tracks);
        assert_eq!(wave_zoom(&ctx), 8.0);
    }

    #[test]
    fn the_toolbars_zoom_in_and_out_keys_zoom_horizontally() {
        let (mut p, mut s) = rig();
        let w = s.tracks.bar_w;
        assert!(crate::menus::enabled("Zoom In", &p, &s));
        crate::menus::apply("Zoom In", &mut p, &mut s);
        assert!((s.tracks.bar_w - w * STEP).abs() < 1e-3);
        crate::menus::apply("Zoom Out", &mut p, &mut s);
        assert!((s.tracks.bar_w - w).abs() < 1e-3);
        s.tracks.bar_w = BAR_W.0;
        assert!(!crate::menus::enabled("Zoom Out", &p, &s));
    }

    // ---- the Zoom tool

    fn show(h: &mut Harness, p: &mut Project, s: &mut UiState) {
        let meters = crate::mixer::Meters::default();
        h.frame(vec![], |ui| crate::tracks_area::show(ui, p, s, &meters));
    }

    fn lanes(h: &Harness) -> egui::Rect {
        h.ctx
            .read_response(egui::Id::unique(("tracks_area", "lanes")))
            .expect("the lanes are on screen")
            .rect
    }

    /// A press at `from`, moves to `to` and a let go there, with `mods` held.
    fn gesture(
        h: &mut Harness,
        p: &mut Project,
        s: &mut UiState,
        (from, to): (egui::Pos2, egui::Pos2),
        mods: Modifiers,
    ) {
        let meters = crate::mixer::Meters::default();
        let mut ui = |ui: &mut egui::Ui| crate::tracks_area::show(ui, p, s, &meters);
        h.time_passes(1.0);
        h.modifiers = mods;
        h.press(from, egui::PointerButton::Primary, &mut ui);
        h.move_to(from + (to - from) / 2.0, &mut ui);
        h.move_to(to, &mut ui);
        h.release(to, egui::PointerButton::Primary, &mut ui);
        h.modifiers = Modifiers::NONE;
        h.frame(vec![], &mut ui);
    }

    fn zoom_rig() -> (Project, UiState, Harness) {
        let (mut p, mut s) = rig();
        // Lanes low enough that one track can fill them (track height is 400 at the most).
        let mut h = Harness::new(1400.0, 420.0);
        show(&mut h, &mut p, &mut s);
        show(&mut h, &mut p, &mut s);
        (p, s, h)
    }

    #[test]
    fn a_zoom_tool_click_zooms_in_on_the_place_and_option_click_zooms_out() {
        let (mut p, mut s, mut h) = zoom_rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        s.tracks.tool = crate::tracks_area::TrackTool::Zoom;
        let l = lanes(h);
        let at = egui::pos2(l.left() + 300.0, l.top() + 10.0);
        let beat = beat_at(p, &s.tracks, l, at.x);
        let (w, before) = (s.tracks.bar_w, p.clone());
        gesture(h, p, s, (at, at), Modifiers::NONE);
        assert!((s.tracks.bar_w - w * 2.0).abs() < 1e-3);
        let mid = beat_at(p, &s.tracks, l, l.center().x);
        assert!((mid - beat).abs() < 0.05, "{mid} {beat}");
        let at = egui::pos2(l.center().x, l.top() + 10.0);
        gesture(h, p, s, (at, at), Modifiers::ALT);
        assert!((s.tracks.bar_w - w).abs() < 1e-3);
        // The place in the middle, unless that would scroll before the song's start.
        let want = (beat as f32 * beat_px(p, w) - l.width() / 2.0).max(0.0);
        assert!((s.tracks.scroll_x - want).abs() < 0.5);
        // The song is as it was: the Zoom tool changes only the view.
        let places = |p: &Project| -> Vec<Vec<(f32, f32)>> {
            p.tracks
                .iter()
                .map(|t| {
                    t.regions
                        .iter()
                        .map(|r| (r.start_bar, r.len_bars))
                        .collect()
                })
                .collect()
        };
        assert_eq!(places(p), places(&before));
    }

    #[test]
    fn a_zoom_tool_rectangle_fills_the_lanes_with_its_time_and_tracks() {
        let (mut p, mut s, mut h) = zoom_rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        s.tracks.tool = crate::tracks_area::TrackTool::Zoom;
        let l = lanes(h);
        let rows =
            crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, s.tracks.row_h);
        // Over track 4 only, from beat 4 to beat 12.
        let bp = beat_px(p, s.tracks.bar_w);
        let y = l.top() + rows[4].0 - s.tracks.scroll_y + rows[4].1 / 2.0;
        let x = |b: f32| l.left() + b * bp - s.tracks.scroll_x;
        gesture(
            h,
            p,
            s,
            (egui::pos2(x(4.0), y - 5.0), egui::pos2(x(12.0), y + 5.0)),
            Modifiers::NONE,
        );
        let v = &s.tracks;
        assert!((x_of(p, v, 4.0) - 20.0).abs() < 1.0, "{}", x_of(p, v, 4.0));
        assert!(
            (x_of(p, v, 12.0) - (v.lanes_w - 20.0)).abs() < 1.0,
            "{} {} {:?}",
            x_of(p, v, 12.0),
            v.lanes_w,
            l
        );
        let rows = crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, v.row_h);
        assert!(
            (rows[4].1 - (v.zoom.lanes_h - 10.0)).abs() <= 2.0,
            "track 4 fills the lanes"
        );
        assert!((rows[4].0 - v.scroll_y).abs() < 0.5);
    }

    #[test]
    fn control_option_drag_zooms_with_any_tool_and_moves_nothing() {
        let (mut p, mut s, mut h) = zoom_rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        assert_eq!(s.tracks.tool, crate::tracks_area::TrackTool::Pointer);
        let ctrl_opt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        };
        let l = lanes(h);
        let rows =
            crate::tracks_area::rows_as_drawn(p, &s.auto, s.tracks.show_hidden, s.tracks.row_h);
        let bp = beat_px(p, s.tracks.bar_w);
        // From inside region "D" (track 4, beats 4 to 12): no move, a zoom to beats 6..8.
        let y = l.top() + rows[4].0 - s.tracks.scroll_y + rows[4].1 / 2.0;
        let x = |b: f32| l.left() + b * bp - s.tracks.scroll_x;
        let places = |p: &Project| -> Vec<(f32, f32)> {
            p.tracks[4]
                .regions
                .iter()
                .map(|r| (r.start_bar, r.len_bars))
                .collect()
        };
        let before = places(p);
        let w = s.tracks.bar_w;
        gesture(
            h,
            p,
            s,
            (egui::pos2(x(6.0), y), egui::pos2(x(8.0), y + 4.0)),
            ctrl_opt,
        );
        assert_eq!(places(p), before, "nothing moved");
        assert!(s.tracks.bar_w > w * 3.0, "zoomed in: {}", s.tracks.bar_w);
        assert!((x_of(p, &s.tracks, 6.0) - 20.0).abs() < 1.0);
        // A Control-Option click is not a zoom (the Control-click menu is).
        let w = s.tracks.bar_w;
        let at = egui::pos2(l.left() + 400.0, l.top() + 5.0);
        gesture(h, p, s, (at, at), ctrl_opt);
        assert_eq!(s.tracks.bar_w, w);
    }

    // ---- the wheel

    /// One wheel step of `dy` points with `mods` held over `at`.
    fn wheel(
        h: &mut Harness,
        p: &mut Project,
        s: &mut UiState,
        at: egui::Pos2,
        dy: f32,
        mods: Modifiers,
    ) {
        let meters = crate::mixer::Meters::default();
        h.modifiers = mods;
        h.move_to(at, |ui| crate::tracks_area::show(ui, p, s, &meters));
        let ev = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: mods,
        };
        h.frame(vec![ev], |ui| crate::tracks_area::show(ui, p, s, &meters));
        h.modifiers = Modifiers::NONE;
        h.time_passes(1.0);
        h.frame(vec![], |ui| crate::tracks_area::show(ui, p, s, &meters));
    }

    #[test]
    fn option_wheel_zooms_vertically_option_command_horizontally_option_control_both() {
        let (mut p, mut s, mut h) = zoom_rig();
        let (p, s, h) = (&mut p, &mut s, &mut h);
        let l = lanes(h);
        let at = egui::pos2(l.left() + 300.0, l.top() + 40.0);
        let (w, rh) = (s.tracks.bar_w, s.tracks.row_h);
        // Option: taller tracks, the same zoom across.
        wheel(h, p, s, at, 5.0, Modifiers::ALT);
        assert!(s.tracks.row_h > rh, "{} {rh}", s.tracks.row_h);
        assert_eq!(s.tracks.bar_w, w);
        let rh = s.tracks.row_h;
        // Option-Command: wider bars around the pointer, the same track height.
        let beat = beat_at(p, &s.tracks, l, at.x);
        wheel(h, p, s, at, 5.0, Modifiers::ALT | Modifiers::COMMAND);
        assert!(s.tracks.bar_w > w, "{} {w}", s.tracks.bar_w);
        assert_eq!(s.tracks.row_h, rh);
        assert!(
            (beat_at(p, &s.tracks, l, at.x) - beat).abs() < 0.01,
            "the pointer's place stays"
        );
        // Command alone zooms across too (it did before).
        let w = s.tracks.bar_w;
        wheel(h, p, s, at, -5.0, Modifiers::COMMAND);
        assert!(s.tracks.bar_w < w);
        // Option-Control: both.
        let (w, rh) = (s.tracks.bar_w, s.tracks.row_h);
        let ctrl_opt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        };
        wheel(h, p, s, at, 5.0, ctrl_opt);
        assert!(
            s.tracks.bar_w > w && s.tracks.row_h > rh,
            "{} {w} {} {rh}",
            s.tracks.bar_w,
            s.tracks.row_h
        );
        // Shift-Option: across only.
        let (w, rh) = (s.tracks.bar_w, s.tracks.row_h);
        wheel(h, p, s, at, 5.0, Modifiers::ALT | Modifiers::SHIFT);
        assert!(s.tracks.bar_w > w, "{} {w}", s.tracks.bar_w);
        assert_eq!(s.tracks.row_h, rh);
        // No modifier: a scroll, no zoom.
        let (w, rh) = (s.tracks.bar_w, s.tracks.row_h);
        s.tracks.scroll_y = 0.0;
        wheel(h, p, s, at, -5.0, Modifiers::NONE);
        assert_eq!((s.tracks.bar_w, s.tracks.row_h), (w, rh));
    }
}
