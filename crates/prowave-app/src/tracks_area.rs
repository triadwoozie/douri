//! Tracks area (Logic Pro: main window "Tracks" area): local toolbar, ruler with cycle area and
//! playhead, track headers and the region lanes.
//!
//! Works today: select/move regions with Snap, Delete and Repeat regions, Set Locators by
//! Regions, drag in the ruler to set the cycle, click to move the playhead, scroll, zoom
//! (sliders and Ctrl+wheel), Catch playhead, add/duplicate tracks, M/S/R/I, volume, pan.
//! Menu entries that are not implemented yet are shown disabled.
use crate::i18n::{tr, trf};
use crate::model::HeaderPart;
use crate::model::{Project, Region, Track, TrackKind, STACK_COLOR};
use crate::track_icons::{draw_icon, track_icon};
use crate::widgets::{
    context_click, context_menu, menu_title, popup_button, stroke, todo_item, BLUE, BTN_BG, TEXT,
    TEXT_DIM,
};
use crate::{Bottom, UiState};
use egui::{
    pos2, vec2, Align2, Color32, FontId, Key, KeyboardShortcut, Modifiers, Painter, Pos2, Rect,
    Sense, Shape, Stroke, StrokeKind, Ui,
};

const TOOLBAR_H: f32 = 30.0;
const RULER_H: f32 = 26.0;
const HEADER_W: f32 = 340.0;
const NUM_W: f32 = 24.0;
/// How far each folder nesting level moves a track header's contents to the right.
const INDENT_W: f32 = 14.0;

const TOOLBAR_BG: Color32 = Color32::from_rgb(0x22, 0x22, 0x25);
const HEADER_BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x2a);
const HEADER_SEL: Color32 = Color32::from_rgb(0x30, 0x30, 0x35);
const LANE_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
const LANE_SEL: Color32 = Color32::from_rgb(0x23, 0x23, 0x27);
/// The band over a track stack's tracks in its parent's lane: purple-grey.
const STACK_BAND: Color32 = Color32::from_rgb(0x3a, 0x38, 0x42);
const RULER_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a);
const CYCLE: Color32 = Color32::from_rgb(0xe5, 0xa9, 0x3b);
const MUTE_ON: Color32 = Color32::from_rgb(0x3a, 0x7b, 0xd5);
const SOLO_ON: Color32 = Color32::from_rgb(0xe5, 0xa9, 0x3b);
const REC_ON: Color32 = Color32::from_rgb(0xe0, 0x48, 0x48);
const MON_ON: Color32 = Color32::from_rgb(0xe5, 0xa9, 0x3b);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Snap {
    Smart,
    Bar,
    Beat,
    Division,
    Ticks,
}

impl Snap {
    pub(crate) const ALL: [Snap; 5] = [
        Snap::Smart,
        Snap::Bar,
        Snap::Beat,
        Snap::Division,
        Snap::Ticks,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            Snap::Smart => tr("Smart"),
            Snap::Bar => tr("Bar"),
            Snap::Beat => tr("Beat"),
            Snap::Division => tr("Division"),
            Snap::Ticks => tr("Ticks"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    Overlap,
    NoOverlap,
    XFade,
    ShuffleR,
    ShuffleL,
}

impl DragMode {
    const ALL: [DragMode; 5] = [
        DragMode::Overlap,
        DragMode::NoOverlap,
        DragMode::XFade,
        DragMode::ShuffleR,
        DragMode::ShuffleL,
    ];
    fn label(self) -> &'static str {
        match self {
            DragMode::Overlap => tr("Overlap"),
            DragMode::NoOverlap => tr("No Overlap"),
            DragMode::XFade => tr("X-Fade"),
            DragMode::ShuffleR => tr("Shuffle R"),
            DragMode::ShuffleL => tr("Shuffle L"),
        }
    }
}

/// Where the Pencil tool is in [`TOOLS`].
const PENCIL_TOOL: usize = 1;
/// Where the Fade tool is in [`TOOLS`].
const FADE_TOOL: usize = 9;
/// The shortest region the Pencil draws, in bars (a 16th note of a 4/4 bar).
const PENCIL_MIN_BARS: f32 = 1.0 / 16.0;

/// What a click with one of the editing tools does, once the regions are drawn.
#[derive(Clone)]
enum ToolAct {
    Erase(usize, usize),
    /// The Scissors tool: (track, region, where, Option held: equal pieces).
    Split(usize, usize, f64, bool),
    /// The Mute tool on a region.
    Mute(usize, usize),
    /// A region's start dragged to a new place (quarter notes).
    Start(usize, usize, f64),
    Join(usize, usize),
    /// A region's end dragged to a new place (quarter notes); `true`: stretch to fit.
    End(usize, usize, f64, bool),
    /// Regions let go after a drag: (track, region, start before), how far later and how many
    /// tracks down they land, and whether Option was held (copies land there instead).
    Drop(Vec<(usize, usize, f64)>, f64, isize, bool),
}

struct MarqueeInput {
    started: Option<Pos2>,
    at: Option<Pos2>,
    stopped: bool,
    clicked: Option<Pos2>,
    other: bool,
}

impl Default for MarqueeInput {
    fn default() -> Self {
        Self {
            started: None,
            at: None,
            stopped: false,
            clicked: None,
            other: false,
        }
    }
}

impl MarqueeInput {
    fn read(&mut self, ui: &Ui, resp: &egui::Response) {
        use egui::PointerButton::Primary;
        if resp.drag_started_by(Primary) {
            self.started = ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos());
        }
        if resp.dragged_by(Primary) {
            self.at = resp.interact_pointer_pos().or(self.at);
        }
        if resp.drag_stopped_by(Primary) {
            self.stopped = true;
        }
        if resp.clicked() {
            self.clicked = resp.interact_pointer_pos();
        }
    }
}

fn draw_marquee(
    g: &Painter,
    rows: &[(f32, f32)],
    r: crate::marquee::Range,
    x_of: &dyn Fn(f64) -> f32,
    lanes: Rect,
    scroll_y: f32,
) {
    let (x0, x1) = (x_of(r.start), x_of(r.end));
    for (_t, &(top, h)) in rows.iter().enumerate().take(r.last + 1).skip(r.first) {
        if h <= 0.0 {
            continue;
        }
        let y = lanes.top() + top - scroll_y;
        let band = Rect::from_min_max(pos2(x0, y), pos2(x1.max(x0 + 1.0), y + h));
        let paint = band.intersect(lanes);
        if paint.width() > 0.0 && paint.height() > 0.0 {
            g.rect_filled(
                paint,
                0.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 38),
            );
            g.line_segment(
                [pos2(x0, paint.top()), pos2(x0, paint.bottom())],
                Stroke::new(1.5, Color32::from_rgb(0xff, 0x9f, 0x40)),
            );
            g.line_segment(
                [pos2(x1, paint.top()), pos2(x1, paint.bottom())],
                Stroke::new(1.5, Color32::from_rgb(0xff, 0x9f, 0x40)),
            );
        }
    }
}

pub(crate) const TOOLS: [&str; 14] = [
    "Pointer Tool",
    "Pencil Tool",
    "Eraser Tool",
    "Text Tool",
    "Scissors Tool",
    "Glue Tool",
    "Solo Tool",
    "Mute Tool",
    "Zoom Tool",
    "Fade Tool",
    "Automation Select Tool",
    "Automation Curve Tool",
    "Marquee Tool",
    "Flex Tool",
];

/// A Normalize Region Gain scan running on its own thread: reading a long recording to find
/// its loudest sample takes seconds, which would freeze the window if done on the UI thread.
pub struct NormalizeJob {
    track: usize,
    region: usize,
    path: std::path::PathBuf,
    /// The file's peak, or why it could not be read.
    result: std::sync::mpsc::Receiver<Result<f32, String>>,
}

/// Each region's (track, region, gain in dB) when a Gain tool drag began, and the pointer's
/// height then.
type GainDrag = (Vec<(usize, usize, f32)>, f32);

/// The tool the pointer has in the Tracks area (Logic's left-click tool).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackTool {
    Pointer,
    /// Click or drag on an empty stretch of a MIDI track to draw an empty MIDI region (Logic:
    /// Pencil tool).
    Pencil,
    Eraser,
    Text,
    Scissors,
    Join,
    Mute,
    Zoom,
    /// Drag on an audio region to set its fade in (left half) or fade out (right half).
    Fade,
    Gain,
    /// Drag to select a stretch of time on one or more tracks (`marquee.rs`).
    Marquee,
    Solo,
}

impl TrackTool {
    pub(crate) const ALL: [(TrackTool, egui::Key, bool, &'static str); 12] = [
        (TrackTool::Pointer, egui::Key::T, false, "T"),
        (TrackTool::Pencil, egui::Key::P, false, "P"),
        (TrackTool::Eraser, egui::Key::E, false, "E"),
        (TrackTool::Marquee, egui::Key::R, false, "R"),
        (TrackTool::Text, egui::Key::T, true, "⇧T"),
        (TrackTool::Scissors, egui::Key::I, false, "I"),
        (TrackTool::Join, egui::Key::J, false, "J"),
        (TrackTool::Mute, egui::Key::M, false, "M"),
        (TrackTool::Solo, egui::Key::S, false, "S"),
        (TrackTool::Zoom, egui::Key::Y, false, "Y"),
        (TrackTool::Fade, egui::Key::A, false, "A"),
        (TrackTool::Gain, egui::Key::G, false, "G"),
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            TrackTool::Pointer => tr("Pointer Tool"),
            TrackTool::Pencil => tr("Pencil Tool"),
            TrackTool::Eraser => tr("Eraser Tool"),
            TrackTool::Text => tr("Text Tool"),
            TrackTool::Scissors => tr("Scissors Tool"),
            TrackTool::Join => tr("Join Tool"),
            TrackTool::Mute => tr("Mute Tool"),
            TrackTool::Zoom => tr("Zoom Tool"),
            TrackTool::Fade => tr("Fade Tool"),
            TrackTool::Gain => tr("Gain Tool"),
            TrackTool::Marquee => tr("Marquee Tool"),
            TrackTool::Solo => tr("Solo Tool"),
        }
    }

    pub(crate) fn moves_regions(self) -> bool {
        matches!(self, TrackTool::Pointer | TrackTool::Pencil)
    }
}

impl crate::piano_tools::MenuTool for TrackTool {
    fn entries() -> &'static [(Self, egui::Key, bool, &'static str)] {
        &Self::ALL
    }
    fn label(self) -> &'static str {
        self.name()
    }
    fn paint(self, g: &egui::Painter, c: egui::Pos2, col: Color32) {
        draw_pointer(g, c, col);
    }
}

pub struct TracksView {
    /// Track > Show Hidden Tracks (H, Logic's Hide view).
    pub show_hidden: bool,
    /// Width of one bar in points (horizontal zoom).
    pub bar_w: f32,
    /// Track height in points (vertical zoom).
    pub row_h: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub snap: Snap,
    pub drag_mode: DragMode,
    pub tool: TrackTool,
    /// The Command-click tool (Logic default: Marquee tool).
    pub cmd_tool: TrackTool,
    pub tool_menu: Option<Pos2>,
    pub held_tool: Option<TrackTool>,
    pub gain_drag: Option<GainDrag>,
    pub fade_drag: Option<crate::fade_tool::Drag>,
    pub edge_drag: Option<(usize, usize, f64, bool)>,
    pub start_drag: Option<(usize, usize, f64)>,
    pub range: crate::marquee::State,
    pub solo_hold: Option<bool>,
    pub drag_origin: Option<Pos2>,
    /// A Pencil drag in progress: the (track, region) it is drawing.
    pub pencil_drag: Option<(usize, usize)>,
    pub catch: bool,
    pub show_tempo: bool,
    pub show_markers: bool,
    pub selected_marker: Option<usize>,
    pub show_signatures: bool,
    pub show_chords: bool,
    pub show_live_loops: bool,
    pub live_loops_split: f32,
    pub selected: Option<(usize, usize)>,
    pub extra: Vec<(usize, usize)>,
    pub track_extra: Vec<usize>,
    pub track_sel: crate::track_selection::TrackSel,
    pub nudge: crate::region_commands::NudgeValue,
    pub zoom: crate::arrange_zoom::ZoomState,
    pub grid: crate::snap_grid::Grid,
    pub drag_by: (f64, isize),
    pub flex_drag: Option<(usize, usize, usize, f64)>,
    pub lanes_w: f32,
    lane_menu: Option<(usize, f32)>,
    grab_beats: f64,
    pub(crate) group_drag: Vec<(usize, usize, f64)>,
    cycle_anchor: Option<f64>,
    swipe: Option<(Vec<crate::model::CompSegment>, f32)>,
    pub normalize_jobs: Vec<NormalizeJob>,
    pub shot_menu: Option<(usize, Option<usize>)>,
    pub tempo_jobs: Vec<crate::smart_tempo::Job>,
    pub tempo_all: Option<crate::smart_tempo::AllJob>,
    pub tempo_editor: crate::smart_tempo::Editor,
    pub waveform_display: crate::project_settings::WaveformDisplay,
}

impl TracksView {
    /// Whether a region is held by the Solo tool: the keys that edit regions wait until it is
    /// let go (they would change the region numbers the hold names).
    pub(crate) fn solo_holding(&self) -> bool {
        self.solo_hold.is_some()
    }

    /// Whether the Tool menu (T) is open: its keys (P, I, A, …) are its own then.
    pub(crate) fn tool_menu_open(&self) -> bool {
        self.tool_menu.is_some()
    }

    /// The tool the pointer has now: the Command-click tool while Command is held.
    pub(crate) fn tool_for(&self, command: bool) -> TrackTool {
        if command {
            self.cmd_tool
        } else {
            self.tool
        }
    }

    /// A press or the start of a drag on region `r` of track `t` with the Pointer tool.
    pub fn grab(&mut self, t: usize, r: usize, add: bool) {
        if self.is_selected(t, r) {
            return;
        }
        if add {
            self.extra.push((t, r));
        } else {
            self.select_only(t, r);
        }
    }

    /// Whether region `r` of track `t` is selected.
    pub fn is_selected(&self, t: usize, r: usize) -> bool {
        self.selected == Some((t, r)) || self.extra.contains(&(t, r))
    }

    /// Every selected region, in track and region order.
    pub fn selection(&self) -> Vec<(usize, usize)> {
        let mut all: Vec<(usize, usize)> = self
            .selected
            .into_iter()
            .chain(self.extra.iter().copied())
            .collect();
        all.sort_unstable();
        all.dedup();
        all
    }

    /// Selects region `r` of track `t` alone.
    pub fn select_only(&mut self, t: usize, r: usize) {
        self.selected = Some((t, r));
        self.extra.clear();
    }

    /// Selects exactly these regions; the first one is the primary one. Nothing when empty.
    pub fn select_many(&mut self, regions: &[(usize, usize)]) {
        self.clear_selection();
        for &x in regions {
            if self.selected.is_none() {
                self.selected = Some(x);
            } else if self.selected != Some(x) && !self.extra.contains(&x) {
                self.extra.push(x);
            }
        }
    }

    /// Sets the selection to one region, or to nothing.
    pub fn set_primary(&mut self, one: Option<(usize, usize)>) {
        match one {
            Some((t, r)) => self.select_only(t, r),
            None => self.clear_selection(),
        }
    }

    /// Selects nothing.
    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.extra.clear();
        self.group_drag.clear();
    }

    /// Shift-click: adds region `r` of track `t` to the selection, or takes it out again.
    pub fn toggle(&mut self, t: usize, r: usize) {
        if self.selected == Some((t, r)) {
            // The primary one goes: the most recently added region takes its place.
            self.selected = self.extra.pop();
        } else if let Some(i) = self.extra.iter().position(|x| *x == (t, r)) {
            self.extra.remove(i);
        } else if self.selected.is_none() {
            self.selected = Some((t, r));
        } else {
            self.extra.push((t, r));
        }
    }

    /// A click on a region that is already selected: it becomes the primary one and the rest of
    /// the selection stays.
    pub fn make_primary(&mut self, t: usize, r: usize) {
        if let Some(i) = self.extra.iter().position(|x| *x == (t, r)) {
            if let Some(old) = self.selected {
                self.extra[i] = old;
            } else {
                self.extra.remove(i);
            }
            self.selected = Some((t, r));
        }
    }

    /// Edit > Select All: every region of every track.
    pub fn select_all(&mut self, p: &crate::model::Project) {
        let all: Vec<(usize, usize)> = p
            .tracks
            .iter()
            .enumerate()
            .flat_map(|(t, tr)| (0..tr.regions.len()).map(move |r| (t, r)))
            .collect();
        self.select_many(&all);
    }

    /// Every selected track, in order: the selected track and the ones picked with Shift or
    /// Command click (Track > Create Track Stack needs two or more).
    pub fn selected_tracks(&self, p: &crate::model::Project) -> Vec<usize> {
        let mut all: Vec<usize> = std::iter::once(p.selected_track)
            .chain(self.track_extra.iter().copied())
            .filter(|&t| t < p.tracks.len())
            .collect();
        all.sort_unstable();
        all.dedup();
        all
    }

    /// Whether track `i` is one of the selected tracks.
    pub fn track_selected(&self, p: &crate::model::Project, i: usize) -> bool {
        p.selected_track == i || self.track_extra.contains(&i)
    }

    /// A click on track `i`'s header, as the Mac and Logic do it: a plain click selects that track
    /// alone, Shift selects every track from the selected one to it, and Command toggles it in or
    /// out of the selection.
    pub fn click_track(
        &mut self,
        p: &mut crate::model::Project,
        i: usize,
        shift: bool,
        toggle: bool,
    ) {
        if i >= p.tracks.len() {
            return;
        }
        if shift {
            let (from, to) = (p.selected_track.min(i), p.selected_track.max(i));
            self.track_extra = (from..=to).filter(|&t| t != p.selected_track).collect();
        } else if toggle {
            if !self.track_selected(p, i) {
                self.track_extra.push(i);
            } else if i == p.selected_track {
                // The selected track goes out: the most recently picked one takes its place.
                if let Some(next) = self.track_extra.pop() {
                    p.selected_track = next;
                }
            } else {
                self.track_extra.retain(|&t| t != i);
            }
        } else {
            self.track_extra.clear();
            p.selected_track = i;
        }
    }

    /// Forgets selected regions the project no longer has (after a delete or an undo).
    pub fn prune(&mut self, p: &crate::model::Project) {
        self.track_extra
            .retain(|&t| t < p.tracks.len() && t != p.selected_track);
        let valid = |(t, r): (usize, usize)| p.tracks.get(t).is_some_and(|tr| r < tr.regions.len());
        self.extra.retain(|x| valid(*x));
        if self.selected.is_some_and(|x| !valid(x)) {
            self.selected = None;
        }
        if self.selected.is_none() {
            self.selected = self.extra.pop();
        }
    }
}

impl Default for TracksView {
    fn default() -> Self {
        Self {
            bar_w: 96.0,
            row_h: 70.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            lanes_w: 1000.0,
            snap: Snap::Smart,
            drag_mode: DragMode::NoOverlap,
            show_hidden: false,
            tool: TrackTool::Pointer,
            cmd_tool: TrackTool::Marquee,
            tool_menu: None,
            held_tool: None,
            gain_drag: None,
            fade_drag: None,
            edge_drag: None,
            start_drag: None,
            range: crate::marquee::State::default(),
            solo_hold: None,
            drag_origin: None,
            pencil_drag: None,
            catch: true,
            show_tempo: false,
            show_markers: false,
            selected_marker: None,
            show_signatures: false,
            show_chords: false,
            show_live_loops: false,
            live_loops_split: 380.0,
            selected: None,
            extra: Vec::new(),
            track_extra: Vec::new(),
            track_sel: Default::default(),
            nudge: crate::region_commands::NudgeValue::load(),
            zoom: Default::default(),
            grid: crate::snap_grid::Grid::default(),
            drag_by: (0.0, 0),
            flex_drag: None,
            grab_beats: 0.0,
            group_drag: Vec::new(),
            cycle_anchor: None,
            swipe: None,
            normalize_jobs: Vec::new(),
            shot_menu: None,
            lane_menu: None,
            tempo_jobs: Vec::new(),
            tempo_all: None,
            tempo_editor: Default::default(),
            waveform_display: crate::project_settings::WaveformDisplay::BipolarStereo,
        }
    }
}

/// True when another track is soloed and `track` is not: it is silent until solo is released
/// (Logic plays only soloed tracks and greys the others' regions).
pub(crate) fn silenced_by_solo(p: &Project, track: usize) -> bool {
    p.tracks.get(track).is_some() && !p.is_soloed(track) && p.any_solo()
}

/// A region while it is being recorded.
const RECORDING_FILL: Color32 = Color32::from_rgb(86, 14, 14);

/// The outline of a selected region.
const SELECTED_EDGE: Color32 = Color32::from_rgb(176, 200, 72);

/// The ruler over a selected region is gold.
const RULER_SELECTION: Color32 = Color32::from_rgb(169, 126, 25);
const RULER_SELECTION_EDGE: Color32 = Color32::from_rgb(95, 69, 20);

/// Modern Logic-style crisp white playhead with subtle grey edge.
pub(crate) const PLAYHEAD_LINE: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub(crate) const PLAYHEAD_HEAD: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub(crate) const PLAYHEAD_EDGE: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xce);
/// While recording the playhead turns red.
const PLAYHEAD_LINE_REC: Color32 = Color32::from_rgb(176, 42, 38);
const PLAYHEAD_HEAD_REC: Color32 = Color32::from_rgb(206, 32, 26);
const PLAYHEAD_EDGE_REC: Color32 = Color32::from_rgb(112, 16, 14);

/// The playhead's line, head and outline colours.
pub(crate) fn playhead_colors(recording: bool) -> (Color32, Color32, Color32) {
    if recording {
        (PLAYHEAD_LINE_REC, PLAYHEAD_HEAD_REC, PLAYHEAD_EDGE_REC)
    } else {
        (PLAYHEAD_LINE, PLAYHEAD_HEAD, PLAYHEAD_EDGE)
    }
}

/// Half the width of the playhead head's grab zone in the ruler.
const PLAYHEAD_GRAB: f32 = 8.0;

/// Beat under screen x `x` while dragging the playhead (never before the start of the song).
fn playhead_beat(x: f32, lanes_left: f32, scroll_x: f32, beat_px: f32) -> f64 {
    ((x - lanes_left + scroll_x) / beat_px.max(1e-3)).max(0.0) as f64
}

/// Half the height of the grab zone on a track's lower edge.
const RESIZE_GRIP: f32 = 3.0;

/// `(top, height)` of every track row in points, from the top of the first track (unscrolled):
/// the global track zoom `row_h` times each track's own height factor.
fn row_layout_with(
    p: &Project,
    row_h: f32,
    show_hidden: bool,
    extra: impl Fn(&Track) -> f32,
) -> Vec<(f32, f32)> {
    let mut top = 0.0;
    p.tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let h = if crate::track_commands::takes_no_room(p, show_hidden, i) {
                0.0
            } else {
                (row_h * t.height).round().max(1.0) + take_rows_height(t) + extra(t)
            };
            let row = (top, h);
            top += h;
            row
        })
        .collect()
}

fn row_layout(p: &Project, row_h: f32) -> Vec<(f32, f32)> {
    row_layout_with(p, row_h, false, |_| 0.0)
}

pub(crate) fn rows_as_drawn(
    p: &Project,
    _auto: &crate::automation::AutoView,
    show_hidden: bool,
    row_h: f32,
) -> Vec<(f32, f32)> {
    row_layout_with(p, row_h, show_hidden, |_| 0.0)
}

/// Index of the track row containing `y` (same origin as [`row_layout`]).
fn row_at(rows: &[(f32, f32)], y: f32) -> Option<usize> {
    rows.iter().position(|&(top, h)| y >= top && y < top + h)
}

fn id(key: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::unique(("tracks_area", key))
}

/// Whether the snap goes to bar lines (which is not a fixed step once signatures change).
fn snaps_to_bars(snap: Snap, beat_px: f32, beat_unit: f64) -> bool {
    match snap {
        Snap::Bar => true,
        Snap::Smart => beat_px * (beat_unit as f32) < 12.0,
        _ => false,
    }
}

/// Fade tool steps per quarter note (1/16 notes, as the Inspector's fades).
const FADE_STEPS_PER_BEAT: f64 = 4.0;

/// The fade (in quarter notes) the Fade tool sets when dragged to `at`, for a region that
/// starts at `start` and is `len` long: from its start for a fade in, from its end for a fade
/// out; in 1/16 notes and never longer than the region.
pub(crate) fn fade_for_drag(at: f64, start: f64, len: f64, fade_in: bool) -> f32 {
    let raw = if fade_in {
        at - start
    } else {
        start + len - at
    };
    ((raw * FADE_STEPS_PER_BEAT).round().max(0.0) / FADE_STEPS_PER_BEAT).min(len) as f32
}

/// How near (in points) the pointer must be to a Flex marker to take it.
const FLEX_GRAB_W: f32 = 6.0;

/// The Flex marker of region `r` of track `t` under screen x `x` (within [`FLEX_GRAB_W`]), and
/// the length of the sound it plays, in seconds. `full` is where the region is.
fn flex_marker_under(p: &Project, t: usize, r: usize, full: Rect, x: f32) -> Option<(usize, f64)> {
    let a = p.tracks.get(t)?.regions.get(r)?.audio.as_ref()?;
    let f = a.flex.as_ref()?;
    // The scale is the length of the Flex result, not of the file it replaces: after a drag
    // the sound is longer (or shorter) than the original.
    let secs = f.length().max(1e-3);
    let per_sec = f64::from(full.width()) / secs;
    if per_sec <= 0.0 {
        return None;
    }
    let dst = f64::from(x - full.left()) / per_sec;
    crate::flex_edit::marker_near(&f.markers, dst, f64::from(FLEX_GRAB_W) / per_sec)
        .map(|i| (i, secs))
}

/// A double-click on region `r` of track `t` with Flex Time: on a marker it removes it,
/// elsewhere it adds one where the pointer is (Logic: click to add a Flex marker,
/// double-click it to remove it). The message to show when the edit changed.
fn flex_double_click(
    p: &mut Project,
    t: usize,
    r: usize,
    full: Rect,
    x: f32,
) -> Option<&'static str> {
    let a = p.tracks.get_mut(t)?.regions.get_mut(r)?.audio.as_mut()?;
    let f = a.flex.as_mut()?;
    let secs = f.length().max(1e-3);
    let per_sec = f64::from(full.width()) / secs;
    if per_sec <= 0.0 {
        return None;
    }
    let dst = (f64::from(x - full.left()) / per_sec).clamp(0.0, secs);
    if let Some(i) =
        crate::flex_edit::marker_near(&f.markers, dst, f64::from(FLEX_GRAB_W) / per_sec)
    {
        return crate::flex_edit::remove_marker(&mut f.markers, i)
            .then_some("Flex: marker removed");
    }
    let src = crate::flex_edit::source_at(&f.markers, dst);
    crate::flex_edit::add_marker(&mut f.markers, src, dst)
        .is_some()
        .then_some("Flex: marker added")
}

/// Whether region `ri` of track `ti` overlaps another region of the track.
pub(crate) fn overlaps_another(p: &Project, ti: usize, ri: usize) -> bool {
    let track = &p.tracks[ti];
    let (s, e) = (
        p.region_start_beats(&track.regions[ri]),
        p.region_end_beats(&track.regions[ri]),
    );
    track.regions.iter().enumerate().any(|(j, o)| {
        j != ri && p.region_start_beats(o) < e - 1e-9 && p.region_end_beats(o) > s + 1e-9
    })
}

/// Snap step in beats (quarter notes) for the current setting and zoom. `beat_px` is pixels
/// per quarter note; `beat_unit` is the signature's beat in quarter notes (0.5 in 6/8), so
/// Beat snaps to eighths in x/8 as in Logic. Divisions stay 16th notes.
pub(crate) fn snap_step(snap: Snap, beat_px: f32, beats_per_bar: f64, beat_unit: f64) -> f64 {
    match snap {
        Snap::Smart if beat_px >= 48.0 => 0.25,
        Snap::Smart if beat_px * beat_unit as f32 >= 12.0 => beat_unit,
        Snap::Smart => beats_per_bar,
        Snap::Bar => beats_per_bar,
        Snap::Beat => beat_unit,
        Snap::Division => 0.25,
        Snap::Ticks => 1.0 / 960.0,
    }
}

pub(crate) fn snap_to(b: f64, step: f64) -> f64 {
    (b / step).round() * step
}

// ------------------------------------------------------------------ small widgets

fn icon_button(
    ui: &Ui,
    g: &Painter,
    rect: Rect,
    key: &str,
    on: bool,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> egui::Response {
    let resp = ui.interact(rect, id(("icon", key)), Sense::click());
    if on {
        g.rect_filled(rect, 5.0, BLUE);
        g.rect_stroke(
            rect,
            5.0,
            Stroke::new(1.0, Color32::from_rgb(0x00, 0xd2, 0xff)),
            StrokeKind::Inside,
        );
    } else {
        g.rect_filled(rect, 5.0, Color32::from_rgb(0x1a, 0x1e, 0x29));
        g.rect_stroke(
            rect,
            5.0,
            Stroke::new(1.0, Color32::from_rgb(0x11, 0x11, 0x13)),
            StrokeKind::Inside,
        );
        if resp.hovered() {
            g.rect_filled(rect, 5.0, Color32::from_white_alpha(15));
        }
    }
    draw(g, rect, if on { Color32::WHITE } else { TEXT });
    resp
}

/// Horizontal mini slider; returns the new 0..1 value.
fn mini_slider(ui: &Ui, g: &Painter, rect: Rect, key: &str, value: f32) -> f32 {
    let resp = ui.interact(
        rect.expand2(vec2(4.0, 6.0)),
        id(("slider", key)),
        Sense::click_and_drag(),
    );
    let mut v = value;
    if let Some(p) = resp.interact_pointer_pos() {
        v = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    }
    let cy = rect.center().y;
    let trough = Rect::from_center_size(pos2(rect.center().x, cy), vec2(rect.width(), 4.0));
    g.rect_filled(trough, 2.0, Color32::from_rgb(0x14, 0x15, 0x18));
    g.rect_stroke(
        trough,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38)),
        StrokeKind::Inside,
    );
    if v > 0.0 {
        let fill = Rect::from_min_max(
            trough.min,
            pos2(trough.left() + trough.width() * v, trough.bottom()),
        );
        g.rect_filled(fill, 2.0, Color32::from_rgb(0x2b, 0x6c, 0xb0));
    }
    let thumb_c = pos2(rect.left() + rect.width() * v, cy);
    g.circle_filled(thumb_c, 5.5, Color32::from_rgb(0xd0, 0xd4, 0xdc));
    g.circle_stroke(
        thumb_c,
        5.5,
        Stroke::new(1.0, Color32::from_rgb(0x3e, 0x42, 0x4c)),
    );
    v
}

fn header_button_bg(selected_row: bool) -> Color32 {
    if selected_row {
        Color32::from_rgb(0x30, 0x30, 0x35)
    } else {
        Color32::from_rgb(0x1e, 0x1e, 0x21)
    }
}

#[allow(clippy::too_many_arguments)]
fn header_button(
    ui: &Ui,
    g: &Painter,
    rect: Rect,
    key: (usize, &str),
    label: &str,
    on: bool,
    on_col: Color32,
    selected_row: bool,
) -> bool {
    let resp = ui.interact(rect, id(("hdrbtn", key)), Sense::click());
    if let Some(text) = crate::tooltips::button_tip(label) {
        resp.clone().on_hover_text(crate::i18n::tr(text));
    }
    if let Some(hid) = match label {
        "M" => Some("M"),
        "S" => Some("S"),
        "R" => Some("R"),
        "I" => Some("I"),
        _ => None,
    } {
        crate::quick_help::hover(ui.ctx(), &resp, hid);
    }
    let pill_r = 3.0;
    if on {
        g.rect_filled(rect, pill_r, on_col);
        g.rect_stroke(
            rect,
            pill_r,
            Stroke::new(1.0, on_col.lerp_to_gamma(Color32::WHITE, 0.3)),
            egui::StrokeKind::Inside,
        );
    } else {
        let bg = if resp.hovered() {
            Color32::from_rgb(0x38, 0x38, 0x3d)
        } else {
            header_button_bg(selected_row)
        };
        g.rect_filled(rect, pill_r, bg);
        g.rect_stroke(
            rect,
            pill_r,
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
            egui::StrokeKind::Inside,
        );
    }
    let txt = if on && on_col == SOLO_ON {
        Color32::BLACK
    } else if on {
        Color32::WHITE
    } else if resp.hovered() {
        Color32::from_rgb(0xea, 0xee, 0xf4)
    } else {
        Color32::from_rgb(0x8a, 0x93, 0xa5)
    };
    g.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        txt,
    );
    resp.clicked()
}

// ------------------------------------------------------------------ toolbar icons

fn draw_grid_icon(g: &Painter, r: Rect, c: Color32) {
    for (dx, dy) in [(-3.5, -3.5), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)] {
        g.rect_filled(
            Rect::from_center_size(r.center() + vec2(dx, dy), vec2(5.0, 5.0)),
            1.0,
            c,
        );
    }
}

fn draw_list_icon(g: &Painter, r: Rect, c: Color32) {
    for i in -1..=1 {
        let y = r.center().y + i as f32 * 4.0;
        g.rect_filled(
            Rect::from_center_size(pos2(r.center().x, y), vec2(13.0, 2.4)),
            1.0,
            c,
        );
    }
}

fn draw_curve_icon(g: &Painter, r: Rect, c: Color32) {
    let p = r.center();
    let pts: Vec<Pos2> = (0..=12)
        .map(|i| {
            let t = i as f32 / 12.0;
            p + vec2(-7.0 + 14.0 * t, 5.0 - 10.0 * (t * t * (3.0 - 2.0 * t)))
        })
        .collect();
    g.add(Shape::line(pts, stroke(c)));
    g.circle_filled(p + vec2(-7.0, 5.0), 1.8, c);
    g.circle_filled(p + vec2(7.0, -5.0), 1.8, c);
}

fn draw_flex_icon(g: &Painter, r: Rect, c: Color32) {
    // Hourglass on its side (Flex).
    let p = r.center();
    g.add(Shape::line(
        vec![
            p + vec2(-7.0, -5.0),
            p + vec2(7.0, 5.0),
            p + vec2(7.0, -5.0),
            p + vec2(-7.0, 5.0),
            p + vec2(-7.0, -5.0),
        ],
        stroke(c),
    ));
}

fn draw_catch_icon(g: &Painter, r: Rect, c: Color32) {
    // Playhead with a follow arrow.
    let p = r.center();
    g.line_segment([p + vec2(-4.0, -7.0), p + vec2(-4.0, 7.0)], stroke(c));
    g.add(Shape::convex_polygon(
        vec![
            p + vec2(-8.0, -7.0),
            p + vec2(0.0, -7.0),
            p + vec2(-4.0, -3.0),
        ],
        c,
        Stroke::NONE,
    ));
    g.line_segment([p + vec2(-1.0, 2.0), p + vec2(6.0, 2.0)], stroke(c));
    g.add(Shape::line(
        vec![p + vec2(3.5, -0.5), p + vec2(6.5, 2.0), p + vec2(3.5, 4.5)],
        stroke(c),
    ));
}

fn draw_pointer(g: &Painter, at: Pos2, c: Color32) {
    let r = Rect::from_center_size(at, vec2(14.0, 14.0));
    crate::icons::paint_regular(g, r, crate::icons::CURSOR, 13.0, c);
}

/// The Fade tool's icon: a ramp down to a baseline.
fn draw_fade_tool(g: &Painter, at: Pos2, c: Color32) {
    g.line_segment([at + vec2(-6.0, 5.0), at + vec2(6.0, 5.0)], stroke(c));
    g.line_segment([at + vec2(-6.0, 5.0), at + vec2(-6.0, -5.0)], stroke(c));
    g.line_segment([at + vec2(-6.0, -5.0), at + vec2(6.0, 5.0)], stroke(c));
}

/// The Pencil tool's icon: a pencil on a diagonal.
fn draw_pencil_tool(g: &Painter, at: Pos2, c: Color32) {
    g.line_segment([at + vec2(-5.0, 5.0), at + vec2(4.0, -4.0)], stroke(c));
    g.line_segment([at + vec2(-5.0, 5.0), at + vec2(-6.0, 6.0)], stroke(c));
    g.line_segment([at + vec2(4.0, -4.0), at + vec2(6.0, -6.0)], stroke(c));
}

fn draw_crosshair(g: &Painter, at: Pos2, c: Color32) {
    g.line_segment([at + vec2(-6.0, 0.0), at + vec2(6.0, 0.0)], stroke(c));
    g.line_segment([at + vec2(0.0, -6.0), at + vec2(0.0, 6.0)], stroke(c));
}

fn draw_zoom_icon(g: &Painter, at: Pos2, c: Color32, vertical: bool) {
    let (a, b) = if vertical {
        (vec2(0.0, -6.0), vec2(0.0, 6.0))
    } else {
        (vec2(-6.0, 0.0), vec2(6.0, 0.0))
    };
    g.line_segment([at + a, at + b], stroke(c));
    let n = if vertical {
        vec2(2.5, 0.0)
    } else {
        vec2(0.0, 2.5)
    };
    let d = if vertical {
        vec2(0.0, 2.5)
    } else {
        vec2(2.5, 0.0)
    };
    g.add(Shape::line(
        vec![at + a + n + d, at + a, at + a - n + d],
        stroke(c),
    ));
    g.add(Shape::line(
        vec![at + b + n - d, at + b, at + b - n - d],
        stroke(c),
    ));
}

fn shortcut(_ui: &Ui, mods: Modifiers, key: Key) -> String {
    crate::key_commands::format_shortcut(
        &KeyboardShortcut::new(mods, key),
        crate::key_commands::is_mac_keyboard(),
    )
}

// ------------------------------------------------------------------ toolbar

fn toolbar(ui: &Ui, p: &mut Project, s: &mut UiState, bar: Rect, lanes_w: f32) {
    let g = ui.painter_at(bar);
    g.rect_filled(bar, 0.0, TOOLBAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(0x11, 0x11, 0x13)),
    );
    let cy = bar.center().y;
    let (can_undo, can_redo) = (s.can_undo, s.can_redo);
    let mut undo_pick = None;
    let v = &mut s.tracks;

    // Back chevron (dim: no previous view to return to).
    let c = pos2(bar.left() + 16.0, cy);
    g.add(Shape::line(
        vec![c + vec2(3.0, -5.0), c + vec2(-2.0, 0.0), c + vec2(3.0, 5.0)],
        stroke(TEXT_DIM),
    ));

    let mut x = bar.left() + 32.0;
    let sel = v.selected;
    let (edit, w) = menu_title(ui, &g, x, cy, tr("Edit"), id(("menu", "Edit")));
    x += w + 2.0;
    egui::Popup::menu(&edit).show(|ui| {
        ui.set_min_width(260.0);
        crate::widgets::undo_items(ui, can_undo, can_redo, &mut undo_pick);
        ui.separator();
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Cut")))
            .clicked()
        {
            crate::region_edit::cut_selected(p, v, &mut s.region_clip);
            ui.close();
        }
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Copy")))
            .clicked()
        {
            crate::region_edit::copy_selected(p, v, &mut s.region_clip);
            ui.close();
        }
        if ui
            .add_enabled(
                crate::region_edit::can_paste(p, &s.region_clip),
                egui::Button::new(tr("Paste")),
            )
            .clicked()
        {
            crate::region_edit::paste_at_playhead(p, v, &s.region_clip);
            ui.close();
        }
        if ui
            .add_enabled(
                sel.is_some(),
                egui::Button::new(tr("Delete")).shortcut_text(tr("Del")),
            )
            .clicked()
        {
            delete_selected(p, v);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                p.tracks.iter().any(|t| !t.regions.is_empty()),
                egui::Button::new(tr("Select All")),
            )
            .clicked()
        {
            v.select_all(p);
            ui.close();
        }
        if ui
            .add_enabled(
                crate::region_edit::can_split(p, v),
                egui::Button::new(tr("Split Regions at Playhead")),
            )
            .clicked()
        {
            crate::region_edit::split_at_playhead(p, v);
            ui.close();
        }
        if ui
            .add_enabled(
                crate::region_edit::can_join(p, v),
                egui::Button::new(tr("Join Regions")),
            )
            .clicked()
        {
            crate::region_edit::join_selected(p, v);
            ui.close();
        }
        let rep = shortcut(ui, Modifiers::COMMAND, Key::R);
        if ui
            .add_enabled(
                sel.is_some(),
                egui::Button::new(tr("Repeat Regions")).shortcut_text(rep),
            )
            .clicked()
        {
            repeat_selected(p, v, &s.auto);
            ui.close();
        }
    });
    if undo_pick.is_some() {
        s.undo_cmd = undo_pick;
    }
    let (func, w) = menu_title(ui, &g, x, cy, tr("Functions"), id(("menu", "Functions")));
    x += w + 2.0;
    egui::Popup::menu(&func).show(|ui| {
        ui.set_min_width(260.0);
        if ui
            .add_enabled(
                sel.is_some(),
                egui::Button::new(tr("Set Locators by Regions")),
            )
            .clicked()
        {
            set_locators_by_region(p, v);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                crate::region_edit::can_quantize(p, v),
                egui::Button::new(tr("Quantize")),
            )
            .clicked()
        {
            crate::region_edit::quantize_selected(p, v);
            ui.close();
        }
        todo_item(ui, tr("Bounce in Place…"));
        todo_item(ui, tr("Convert to New Sampler Track…"));
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr("Move to Playhead")))
            .clicked()
        {
            crate::region_edit::move_to_playhead(p, v);
            ui.close();
        }
    });
    let (view, w) = menu_title(ui, &g, x, cy, tr("View"), id(("menu", "View")));
    x += w + 10.0;
    egui::Popup::menu(&view).show(|ui| {
        ui.set_min_width(260.0);
        if ui.button(tr("Zoom to Fit")).clicked() {
            zoom_to_fit(p, v, lanes_w);
            ui.close();
        }
        if ui.button(tr("Zoom In")).clicked() {
            v.bar_w = (v.bar_w * 1.25).min(600.0);
            ui.close();
        }
        if ui.button(tr("Zoom Out")).clicked() {
            v.bar_w = (v.bar_w / 1.25).max(6.0);
            ui.close();
        }
        ui.separator();
        if ui.checkbox(&mut v.catch, tr("Catch Playhead")).clicked() {
            ui.close();
        }
        todo_item(ui, tr("Show Global Tracks"));
        if ui.button(tr("Configure Track Header…")).clicked() {
            s.track_dialog = Some(crate::track_dialogs::TrackDialog::Header);
            ui.close();
        }
    });

    // View buttons: grid (Live Loops) / list (Tracks, active).
    let r = Rect::from_center_size(pos2(x + 13.0, cy), vec2(26.0, 20.0));
    if icon_button(ui, &g, r, "grid", v.show_live_loops, draw_grid_icon).clicked() {
        v.show_live_loops = !v.show_live_loops;
    }
    let r = Rect::from_center_size(pos2(x + 41.0, cy), vec2(26.0, 20.0));
    let _ = icon_button(ui, &g, r, "list", true, draw_list_icon);
    x += 66.0;
    let r = Rect::from_center_size(pos2(x + 13.0, cy), vec2(26.0, 20.0));
    let _ = icon_button(ui, &g, r, "automation", false, draw_curve_icon);
    let r = Rect::from_center_size(pos2(x + 41.0, cy), vec2(26.0, 20.0));
    let _ = icon_button(ui, &g, r, "flex", false, draw_flex_icon);
    let r = Rect::from_center_size(pos2(x + 69.0, cy), vec2(26.0, 20.0));
    if icon_button(ui, &g, r, "catch", v.catch, draw_catch_icon).clicked() {
        v.catch = !v.catch;
    }
    let left_end = x + 90.0;

    // Right-hand side, laid out from the right edge.
    // Horizontal zoom (log scale, 6..600 points per bar).
    let hz = Rect::from_min_size(pos2(bar.right() - 72.0, cy - 6.0), vec2(64.0, 12.0));
    let (lo, hi) = (6f32.ln(), 600f32.ln());
    let t = ((v.bar_w.ln() - lo) / (hi - lo)).clamp(0.0, 1.0);
    let t2 = mini_slider(ui, &g, hz, "hzoom", t);
    if t2 != t {
        v.bar_w = (lo + t2 * (hi - lo)).exp();
    }
    draw_zoom_icon(&g, pos2(hz.left() - 13.0, cy), TEXT, false);
    // Vertical zoom (track height 36..140).
    let vz = Rect::from_min_size(pos2(hz.left() - 26.0 - 54.0, cy - 6.0), vec2(54.0, 12.0));
    let t = ((v.row_h - 36.0) / (140.0 - 36.0)).clamp(0.0, 1.0);
    let t2 = mini_slider(ui, &g, vz, "vzoom", t);
    if t2 != t {
        v.row_h = 36.0 + t2 * (140.0 - 36.0);
    }
    draw_zoom_icon(&g, pos2(vz.left() - 13.0, cy), TEXT, true);
    // What fits between the two sides; the rest is left out rather than drawn on top.
    let mut rx = vz.left() - 30.0;
    // SNAP_W and DRAG_W allow for the English "Snap:" and "Drag:"; longer translations take
    // the difference from the room left.
    let label_w = |t: &str| {
        g.layout_no_wrap(t.to_owned(), FontId::proportional(12.5), TEXT_DIM)
            .size()
            .x
    };
    let longer = (label_w(tr("Snap:")) - label_w("Snap:")).max(0.0)
        + (label_w(tr("Drag:")) - label_w("Drag:")).max(0.0);
    let show = fit_toolbar(rx - left_end - longer);
    // Waveform zoom / fit buttons (not implemented yet: drawn dim).
    for i in 0..2 {
        if !show.wave {
            break;
        }
        let c = pos2(rx - 10.0 - i as f32 * 26.0, cy);
        g.line_segment(
            [c + vec2(-5.0, -6.0), c + vec2(-5.0, 6.0)],
            stroke(TEXT_DIM),
        );
        g.line_segment([c + vec2(5.0, -6.0), c + vec2(5.0, 6.0)], stroke(TEXT_DIM));
        if i == 0 {
            g.line_segment([c + vec2(-3.0, 0.0), c + vec2(3.0, 0.0)], stroke(TEXT_DIM));
        } else {
            g.line_segment([c + vec2(0.0, -4.0), c + vec2(0.0, 4.0)], stroke(TEXT_DIM));
        }
    }
    if show.wave {
        rx -= 62.0;
    }

    if show.drag {
        rx = drag_menu(ui, &g, v, rx, cy);
    }
    if show.snap {
        rx = snap_menu(ui, &g, v, rx, cy);
    }
    if show.tools {
        tool_menus(ui, &g, v, rx, cy);
    }
}

/// Which optional toolbar items are shown, given the free width between the left-hand group
/// and the zoom sliders. Logic hides toolbar items that do not fit; most useful first: Snap,
/// Drag, the two tool menus, then the waveform zoom buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ToolbarFit {
    snap: bool,
    drag: bool,
    tools: bool,
    wave: bool,
}

/// Widths (with their labels and gaps) of the optional toolbar items.
const SNAP_W: f32 = 204.0;
const DRAG_W: f32 = 160.0;
const TOOLS_W: f32 = 92.0;
const WAVE_W: f32 = 62.0;

fn fit_toolbar(free: f32) -> ToolbarFit {
    let mut left = free;
    let mut take = |w: f32| {
        let fits = left >= w;
        if fits {
            left -= w;
        }
        fits
    };
    let snap = take(SNAP_W);
    let drag = take(DRAG_W);
    let tools = take(TOOLS_W);
    let wave = take(WAVE_W);
    ToolbarFit {
        snap,
        drag,
        tools,
        wave,
    }
}

/// "Drag:" mode menu ending at `rx`; returns where the next item to its left ends.
fn drag_menu(ui: &Ui, g: &Painter, v: &mut TracksView, rx: f32, cy: f32) -> f32 {
    let drag_r = Rect::from_min_size(pos2(rx - 110.0, cy - 10.0), vec2(110.0, 20.0));
    let resp = popup_button(ui, g, drag_r, id(("popup", "drag")), v.drag_mode.label());
    egui::Popup::menu(&resp).show(|ui| {
        for m in DragMode::ALL {
            if ui.selectable_label(v.drag_mode == m, m.label()).clicked() {
                v.drag_mode = m;
                ui.close();
            }
        }
    });
    let label = g.text(
        pos2(drag_r.left() - 6.0, cy),
        Align2::RIGHT_CENTER,
        tr("Drag:"),
        FontId::proportional(12.5),
        TEXT_DIM,
    );
    label.left() - 8.0
}

/// "Snap:" menu ending at `rx`; returns where the next item to its left ends.
fn snap_menu(ui: &Ui, g: &Painter, v: &mut TracksView, rx: f32, cy: f32) -> f32 {
    let snap_r = Rect::from_min_size(pos2(rx - 150.0, cy - 10.0), vec2(150.0, 20.0));
    let shown = if v.grid.on { v.snap.label() } else { tr("Off") };
    let resp = popup_button(ui, g, snap_r, id(("popup", "snap")), shown);
    egui::Popup::menu(&resp).show(|ui| {
        if ui.selectable_label(v.grid.on, tr("Snap to Grid")).clicked() {
            v.grid.on = !v.grid.on;
            ui.close();
        }
        ui.separator();
        for m in Snap::ALL {
            if ui
                .selectable_label(v.grid.on && v.snap == m, m.label())
                .clicked()
            {
                v.snap = m;
                v.grid.on = true;
                ui.close();
            }
        }
        ui.separator();
        for d in crate::snap_grid::DIVISIONS {
            let text = crate::snap_grid::division_label(d);
            if ui.selectable_label(v.grid.division == d, text).clicked() {
                v.grid.division = d;
                ui.close();
            }
        }
        ui.separator();
        if ui
            .selectable_label(v.grid.relative, tr("Snap Regions to Relative Value"))
            .clicked()
        {
            v.grid.relative = true;
            ui.close();
        }
        if ui
            .selectable_label(!v.grid.relative, tr("Snap Regions to Absolute Value"))
            .clicked()
        {
            v.grid.relative = false;
            ui.close();
        }
    });
    let label = g.text(
        pos2(snap_r.left() - 6.0, cy),
        Align2::RIGHT_CENTER,
        tr("Snap:"),
        FontId::proportional(12.5),
        TEXT_DIM,
    );
    label.left() - 12.0
}

/// Left-click and Command-click tool menus ending at `rx`.
fn tool_menus(ui: &Ui, g: &Painter, v: &mut TracksView, rx: f32, cy: f32) {
    for (i, key) in ["tool_l", "tool_r"].iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rx - 40.0 - (1 - i) as f32 * 44.0, cy - 10.0),
            vec2(40.0, 20.0),
        );
        let resp = ui.interact(r, id(*key), Sense::click());
        g.rect_filled(r, 5.0, Color32::from_rgb(0x26, 0x26, 0x2a));
        g.rect_stroke(
            r,
            5.0,
            Stroke::new(1.0, Color32::from_rgb(0x11, 0x11, 0x13)),
            StrokeKind::Inside,
        );
        if resp.hovered() {
            g.rect_filled(r, 5.0, Color32::from_white_alpha(15));
        }
        let icon_at = pos2(r.left() + 13.0, cy);
        let active_tool = if i == 0 { v.tool } else { v.cmd_tool };
        match active_tool {
            TrackTool::Pencil => draw_pencil_tool(g, icon_at, TEXT),
            TrackTool::Fade => draw_fade_tool(g, icon_at, TEXT),
            TrackTool::Pointer => draw_pointer(g, icon_at, TEXT),
            TrackTool::Marquee => draw_crosshair(g, icon_at, TEXT),
            _ => draw_pointer(g, icon_at, TEXT),
        }
        let c = pos2(r.right() - 9.0, cy + 1.0);
        g.add(Shape::line(
            vec![
                c + vec2(-3.0, -1.5),
                c + vec2(0.0, 1.5),
                c + vec2(3.0, -1.5),
            ],
            stroke(TEXT_DIM),
        ));
        egui::Popup::menu(&resp).show(|ui| {
            ui.set_min_width(220.0);
            for (n, t) in TOOLS.iter().enumerate() {
                let tool = match n {
                    0 => Some(TrackTool::Pointer),
                    1 => Some(TrackTool::Pencil),
                    2 => Some(TrackTool::Eraser),
                    3 => Some(TrackTool::Text),
                    4 => Some(TrackTool::Scissors),
                    5 => Some(TrackTool::Join),
                    6 => Some(TrackTool::Solo),
                    7 => Some(TrackTool::Mute),
                    8 => Some(TrackTool::Zoom),
                    FADE_TOOL => Some(TrackTool::Fade),
                    12 => Some(TrackTool::Marquee),
                    _ => None,
                };
                match tool {
                    Some(tool) => {
                        let selected = if i == 0 {
                            v.tool == tool
                        } else {
                            v.cmd_tool == tool
                        };
                        if ui.selectable_label(selected, tr(t)).clicked() {
                            if i == 0 {
                                v.tool = tool;
                            } else {
                                v.cmd_tool = tool;
                            }
                            ui.close();
                        }
                    }
                    None => todo_item(ui, t),
                }
            }
        });
    }
}

// ------------------------------------------------------------------ right-click menus

/// An entry picked in one of the Tracks area's right-click menus; run once the menus are drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
enum CtxAction {
    NewTrack(TrackKind),
    ZoomToFit,
    OpenPianoRoll,
    /// A Pattern or Drummer region becomes an ordinary MIDI region.
    ConvertToMidi(usize, usize),
    /// An empty MIDI region at `bar` on `track`, opened in the Piano Roll.
    CreateMidiRegion {
        track: usize,
        bar: f32,
    },
    /// A generated Pattern region at `bar`.
    CreatePatternRegion {
        track: usize,
        bar: f32,
    },
    /// A Drummer region at `bar`.
    CreateDrummerRegion {
        track: usize,
        bar: f32,
    },
    ToggleRegionMute(usize, usize),
    ToggleReverse(usize, usize),
    ToggleTakeFolder(usize, usize),
    FlattenTakeFolder(usize, usize),
    Normalize(usize, usize),
    /// Smart Tempo: say the region's tempo (false) or make it the project's (true).
    DetectTempo(usize, usize, bool),
    /// Smart Tempo of every audio region together (multitrack).
    AdaptTempoToAll,
    SmartTempoEditor(usize, usize),
    PitchEditor(usize, usize),
    FlexMode(usize, usize, prowave_engine::FlexMode),
    Flex(usize, usize, crate::flex_edit::Command),
    FlexRemove(usize, usize),
    RepeatRegion,
    LocatorsByRegion,
    DeleteRegion,
    CreateStack(usize),
    ReleaseStack(usize),
    ToggleMute(usize),
    ToggleSolo(usize),
    ResetHeight(usize),
    DeleteTrack(usize),
    /// Makes the track the Groove Track, or (if it is) no track.
    ToggleGrooveTrack(usize),
    ToggleFollowGroove(usize),
}

/// What the track menu needs to know about the track that was right-clicked.
struct TrackInfo {
    i: usize,
    folder: bool,
    /// Two or more tracks are selected, so they can be made into a track stack.
    stackable: bool,
    mute: bool,
    solo: bool,
    custom_height: bool,
    last: bool,
    /// The Groove Track, and whether this track follows it (`None`: there is none).
    groove: bool,
    follow_groove: Option<bool>,
}

fn pick(ui: &mut Ui, label: &str, key: &str, action: CtxAction, picked: &mut Option<CtxAction>) {
    if ui
        .add(egui::Button::new(label).shortcut_text(key))
        .clicked()
    {
        *picked = Some(action);
    }
}

fn new_track_items(ui: &mut Ui, picked: &mut Option<CtxAction>) {
    for (label, kind) in [
        (tr("New Software Instrument Track"), TrackKind::Instrument),
        (tr("New Audio Track"), TrackKind::Audio),
        (tr("New Aux Track"), TrackKind::Aux),
        (tr("New VCA Track"), TrackKind::Vca),
        (tr("New External MIDI Track"), TrackKind::ExternalMidi),
    ] {
        pick(ui, label, "", CtxAction::NewTrack(kind), picked);
    }
}

#[allow(clippy::too_many_arguments)]
fn region_items(
    ui: &mut Ui,
    is_audio: bool,
    muted: bool,
    folder: Option<bool>,
    generated: bool,
    ti: usize,
    ri: usize,
    picked: &mut Option<CtxAction>,
) {
    if !is_audio {
        let open = if generated {
            tr("Open Editor")
        } else {
            tr("Open in Piano Roll")
        };
        pick(ui, open, "", CtxAction::OpenPianoRoll, picked);
        if generated {
            pick(
                ui,
                tr("Convert to MIDI Region"),
                "",
                CtxAction::ConvertToMidi(ti, ri),
                picked,
            );
        }
        ui.separator();
    }
    if let Some(open) = folder {
        let toggle = if open {
            tr("Close Take Folder")
        } else {
            tr("Open Take Folder")
        };
        pick(ui, toggle, "", CtxAction::ToggleTakeFolder(ti, ri), picked);
        pick(
            ui,
            tr("Flatten Take Folder"),
            "",
            CtxAction::FlattenTakeFolder(ti, ri),
            picked,
        );
        ui.separator();
    }
    let mute = if muted {
        tr("Unmute Region")
    } else {
        tr("Mute Region")
    };
    pick(ui, mute, "", CtxAction::ToggleRegionMute(ti, ri), picked);
    if is_audio {
        pick(
            ui,
            tr("Normalize Region Gain"),
            "",
            CtxAction::Normalize(ti, ri),
            picked,
        );
        ui.menu_button(tr("Smart Tempo"), |ui| {
            pick(
                ui,
                tr("Smart Tempo Editor…"),
                "",
                CtxAction::SmartTempoEditor(ti, ri),
                picked,
            );
            pick(
                ui,
                tr("Detect Tempo"),
                "",
                CtxAction::DetectTempo(ti, ri, false),
                picked,
            );
            pick(
                ui,
                tr("Adapt Project Tempo to Region"),
                "",
                CtxAction::DetectTempo(ti, ri, true),
                picked,
            );
            pick(
                ui,
                tr("Adapt Project Tempo to All Audio Regions"),
                "",
                CtxAction::AdaptTempoToAll,
                picked,
            );
            pick(
                ui,
                tr("Region Follows Project Tempo"),
                "",
                CtxAction::Flex(ti, ri, crate::flex_edit::Command::FollowProjectTempo),
                picked,
            );
        });
        ui.menu_button(tr("Flex Time"), |ui| {
            ui.menu_button(tr("Flex Mode"), |ui| {
                for mode in prowave_engine::FlexMode::ALL {
                    pick(
                        ui,
                        crate::flex_edit::mode_label(mode),
                        "",
                        CtxAction::FlexMode(ti, ri, mode),
                        picked,
                    );
                }
            });
            pick(
                ui,
                tr("Quantize Audio to 1/16"),
                "",
                CtxAction::Flex(ti, ri, crate::flex_edit::Command::Quantize { grid: 0.25 }),
                picked,
            );
            pick(
                ui,
                tr("Quantize Audio to 1/8"),
                "",
                CtxAction::Flex(ti, ri, crate::flex_edit::Command::Quantize { grid: 0.5 }),
                picked,
            );
            pick(
                ui,
                tr("Remove Flex Edits"),
                "",
                CtxAction::FlexRemove(ti, ri),
                picked,
            );
        });
        ui.menu_button(tr("Flex Pitch"), |ui| {
            pick(
                ui,
                tr("Open in Pitch Editor"),
                "",
                CtxAction::PitchEditor(ti, ri),
                picked,
            );
            pick(
                ui,
                tr("Correct Pitch to Key"),
                "",
                CtxAction::Flex(ti, ri, crate::flex_edit::Command::CorrectPitch),
                picked,
            );
            pick(
                ui,
                tr("Create MIDI Track from Flex Pitch"),
                "",
                CtxAction::Flex(ti, ri, crate::flex_edit::Command::MidiFromPitch),
                picked,
            );
        });
        pick(
            ui,
            tr("Reverse Region"),
            "",
            CtxAction::ToggleReverse(ti, ri),
            picked,
        );
    }
    let repeat_key = crate::key_commands::format_shortcut(
        &egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::R),
        crate::key_commands::is_mac_keyboard(),
    );
    pick(
        ui,
        tr("Repeat Region"),
        &repeat_key,
        CtxAction::RepeatRegion,
        picked,
    );
    pick(
        ui,
        tr("Set Locators by Region"),
        "",
        CtxAction::LocatorsByRegion,
        picked,
    );
    ui.separator();
    pick(ui, tr("Delete"), "⌫", CtxAction::DeleteRegion, picked);
}

fn track_items(ui: &mut Ui, t: &TrackInfo, picked: &mut Option<CtxAction>) {
    new_track_items(ui, picked);
    ui.separator();
    if ui
        .add_enabled(t.stackable, egui::Button::new(tr("Create Track Stack")))
        .clicked()
    {
        *picked = Some(CtxAction::CreateStack(t.i));
    }
    if t.folder {
        pick(
            ui,
            tr("Release Track Stack"),
            "",
            CtxAction::ReleaseStack(t.i),
            picked,
        );
    }
    ui.separator();
    let mute = if t.mute {
        tr("Unmute Track")
    } else {
        tr("Mute Track")
    };
    pick(ui, mute, "", CtxAction::ToggleMute(t.i), picked);
    let solo = if t.solo {
        tr("Unsolo Track")
    } else {
        tr("Solo Track")
    };
    pick(ui, solo, "", CtxAction::ToggleSolo(t.i), picked);
    if t.custom_height {
        pick(
            ui,
            tr("Reset Track Height"),
            "",
            CtxAction::ResetHeight(t.i),
            picked,
        );
    }
    ui.separator();
    let groove = if t.groove {
        tr("Clear Groove Track")
    } else {
        tr("Set as Groove Track")
    };
    pick(ui, groove, "", CtxAction::ToggleGrooveTrack(t.i), picked);
    if let Some(follows) = t.follow_groove {
        let label = if follows {
            tr("Stop Following Groove Track")
        } else {
            tr("Follow Groove Track")
        };
        pick(ui, label, "", CtxAction::ToggleFollowGroove(t.i), picked);
    }
    ui.separator();
    let delete = ui.add_enabled(!t.last, egui::Button::new(tr("Delete Track")));
    if delete.clicked() {
        *picked = Some(CtxAction::DeleteTrack(t.i));
    }
}

/// Runs a right-click menu entry.
fn apply_ctx(action: CtxAction, p: &mut Project, s: &mut UiState, lanes_w: f32) {
    let v = &mut s.tracks;
    match action {
        CtxAction::NewTrack(kind) => {
            p.add_track(kind);
        }
        CtxAction::ZoomToFit => zoom_to_fit(p, v, lanes_w),
        CtxAction::OpenPianoRoll => s.bottom = Bottom::PianoRoll,
        CtxAction::ConvertToMidi(t, r) => p.convert_to_midi_region(t, r),
        CtxAction::CreateMidiRegion { track, bar } => {
            if let Some(ri) = p.add_empty_region(track, bar) {
                p.selected_track = track;
                v.select_only(track, ri);
                s.bottom = Bottom::PianoRoll;
            }
        }
        CtxAction::CreatePatternRegion { track, bar } => {
            if let Some(ri) = p.add_generated_region(track, false, bar) {
                p.selected_track = track;
                v.select_only(track, ri);
                s.bottom = Bottom::PianoRoll;
            }
        }
        CtxAction::CreateDrummerRegion { track, bar } => {
            if let Some(ri) = p.add_generated_region(track, true, bar) {
                p.selected_track = track;
                v.select_only(track, ri);
            }
        }
        CtxAction::ToggleRegionMute(t, r) => {
            if let Some(region) = p.tracks.get_mut(t).and_then(|t| t.regions.get_mut(r)) {
                region.params.mute = !region.params.mute;
                p.song_rev += 1;
            }
        }
        CtxAction::ToggleTakeFolder(t, r) => {
            if let Some(f) = p
                .tracks
                .get_mut(t)
                .and_then(|t| t.regions.get_mut(r))
                .and_then(|r| r.folder.as_mut())
            {
                f.open = !f.open;
            }
        }
        // The comp as it is becomes an ordinary region: the other takes are let go.
        CtxAction::FlattenTakeFolder(t, r) => {
            if let Some(region) = p.tracks.get_mut(t).and_then(|t| t.regions.get_mut(r)) {
                region.folder = None;
            }
        }
        CtxAction::ToggleReverse(t, r) => {
            if let Some(region) = p.tracks.get_mut(t).and_then(|t| t.regions.get_mut(r)) {
                region.params.reverse = !region.params.reverse;
                // The engine plays a clip's edits only after the song is rebuilt.
                p.song_rev += 1;
            }
        }
        CtxAction::Normalize(t, r) => start_normalize(p, s, t, r),
        CtxAction::DetectTempo(t, r, adapt) => crate::smart_tempo::start(p, s, t, r, adapt),
        CtxAction::AdaptTempoToAll => crate::smart_tempo::start_all(p, s),
        CtxAction::ToggleGrooveTrack(i) => {
            let was = p.tracks.get(i).is_some_and(|t| t.params.groove);
            crate::groove::set_groove_track(p, (!was).then_some(i));
        }
        CtxAction::ToggleFollowGroove(i) => {
            if let Some(t) = p.tracks.get_mut(i) {
                t.params.follow_groove = !t.params.follow_groove;
                p.song_rev += 1;
            }
        }
        CtxAction::SmartTempoEditor(t, r) => crate::smart_tempo::open_editor(p, s, t, r),
        CtxAction::PitchEditor(t, r) => crate::pitch_editor::open(s, t, r),
        CtxAction::FlexMode(t, r, mode) => {
            crate::flex_edit::set_mode(p, t, r, mode);
            p.song_rev += 1;
        }
        CtxAction::Flex(t, r, command) => crate::flex_edit::start(p, s, t, r, command),
        CtxAction::FlexRemove(t, r) => s.flex.remove_later(t, r),
        CtxAction::RepeatRegion => repeat_selected(p, v, &s.auto),
        CtxAction::LocatorsByRegion => set_locators_by_region(p, v),
        CtxAction::DeleteRegion => delete_selected(p, v),
        CtxAction::CreateStack(_) => s.stack_dialog.open(),
        CtxAction::ReleaseStack(i) => {
            if p.release_track_stack(i) {
                forget_track_indexes(s);
            }
        }
        CtxAction::ToggleMute(i) => {
            let mute = !p.tracks[i].mute;
            p.set_folder_flags(i, Some(mute), None);
        }
        CtxAction::ToggleSolo(i) => {
            let solo = !p.tracks[i].solo;
            p.set_folder_flags(i, None, Some(solo));
        }
        CtxAction::ResetHeight(i) => p.tracks[i].height = 1.0,
        CtxAction::DeleteTrack(i) => {
            if p.delete_track(i) {
                forget_track_indexes(s);
            }
        }
    }
}

/// Logic's Normalize Region Gain: finds the region's loudest sample on a background thread,
/// then [`poll_normalize`] sets the gain that brings it to 0 dBFS. The file is not changed.
fn start_normalize(p: &Project, s: &mut UiState, track: usize, region: usize) {
    let Some(audio) = p
        .tracks
        .get(track)
        .and_then(|t| t.regions.get(region))
        .and_then(|r| r.audio.as_ref())
    else {
        s.status = Some(tr("Normalize: this region has no recorded audio").into());
        return;
    };
    // The loudest sample is looked for in the whole file, which a piece does not play.
    if audio.plays_part() {
        s.status = Some(tr("This region plays only part of its recording, which this command cannot handle yet").into());
        return;
    }
    let jobs = &mut s.tracks.normalize_jobs;
    if jobs.iter().any(|j| (j.track, j.region) == (track, region)) {
        return;
    }
    let file = audio.file.clone();
    let path = file.path.clone();
    let (tx, result) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("prowave-normalize".into())
        .spawn(move || {
            let peak = prowave_engine::peak_of(&file).map_err(|e| e.to_string());
            let _ = tx.send(peak);
        });
    match spawned {
        Ok(_) => {
            jobs.push(NormalizeJob {
                track,
                region,
                path,
                result,
            });
            s.status = Some(tr("Normalizing…").into());
        }
        Err(e) => s.status = Some(format!("cannot start the normalize scan: {e}")),
    }
}

/// Applies the Normalize Region Gain scans that have finished. Call once per frame.
pub fn poll_normalize(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    use std::sync::mpsc::TryRecvError;
    if s.tracks.normalize_jobs.is_empty() {
        return;
    }
    let mut finished = Vec::new();
    let mut message = None;
    s.tracks
        .normalize_jobs
        .retain(|job| match job.result.try_recv() {
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
            Ok(peak) => {
                finished.push((job.track, job.region, job.path.clone(), peak));
                false
            }
        });
    for (track, region, path, peak) in finished {
        // The project may have changed while the file was being read.
        let Some(r) = p
            .tracks
            .get_mut(track)
            .and_then(|t| t.regions.get_mut(region))
            .filter(|r| r.audio.as_ref().is_some_and(|a| a.file.path == path))
        else {
            continue;
        };
        match peak {
            Ok(peak) => match crate::model::RegionParams::normalize_gain(peak) {
                Some(gain) => r.params.gain_db = gain,
                None => message = Some(tr("Normalize: the region is silent").to_string()),
            },
            Err(e) => {
                message = Some(trf("Normalize: cannot read {}: {}", &[&path.display(), &e]));
            }
        }
    }
    if s.tracks.normalize_jobs.is_empty() {
        // Clears "Normalizing…" (or shows what went wrong).
        s.status = message;
    } else {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Track numbers shifted, so a remembered region or note selection may point at another track.
pub(crate) fn forget_track_indexes(s: &mut UiState) {
    s.tracks.clear_selection();
    s.tracks.track_extra.clear();
    s.piano.selected.clear();
}

// ------------------------------------------------------------------ actions

pub(crate) fn delete_selected(p: &mut Project, v: &mut TracksView) {
    if v.range.active().is_some() {
        crate::marquee::delete(p, v);
        return;
    }
    if v.selected_marker.is_some_and(|i| i < p.markers.len()) {
        crate::marker_edit::delete(p, v);
        return;
    }
    let mut sel = v.selection();
    if sel.is_empty() {
        if p.tracks
            .get(p.selected_track)
            .is_some_and(|t| t.kind.has_regions() && t.regions.is_empty())
        {
            p.delete_track(p.selected_track);
        }
        return;
    }
    crate::drag_modes::close_deleted(p, &sel, v.drag_mode);
    // Highest index first on each track, so the ones still to go keep their numbers.
    sel.sort_unstable();
    sel.reverse();
    for (t, r) in sel {
        if let Some(track) = p.tracks.get_mut(t) {
            if r < track.regions.len() {
                track.regions.remove(r);
            }
        }
    }
    v.clear_selection();
    p.song_rev += 1;
}

fn repeat_selected(p: &mut Project, v: &mut TracksView, auto: &crate::automation::AutoView) {
    crate::region_commands::repeat(p, v, auto, 1, crate::region_commands::RepeatAdjust::Auto);
}

pub(crate) fn set_locators_by_region(p: &mut Project, v: &TracksView) {
    // The span from the start of the first selected region to the end of the last one.
    let span = v
        .selection()
        .into_iter()
        .filter_map(|(t, r)| p.tracks.get(t)?.regions.get(r))
        .map(|reg| (p.region_start_beats(reg), p.region_end_beats(reg)))
        .fold(None, |acc: Option<(f64, f64)>, (s, e)| match acc {
            None => Some((s, e)),
            Some((a, b)) => Some((a.min(s), b.max(e))),
        });
    if let Some((start, end)) = span {
        p.cycle_start = start;
        p.cycle_end = end;
        p.cycle = true;
    }
}

pub(crate) fn zoom_to_fit(p: &Project, v: &mut TracksView, lanes_w: f32) {
    let end = crate::arrange_zoom::contents_end(p);
    if end > 0.0 {
        let bars = end / p.bar_len(0).max(1e-6);
        v.bar_w = ((lanes_w - 20.0) / bars as f32).clamp(6.0, 600.0);
        v.scroll_x = 0.0;
    }
}

// ------------------------------------------------------------------ drawing

/// The body colour of a region. Logic draws regions deeper than the track colour; these four
/// were measured from Logic Pro itself (yellow is estimated from the others).
const LOGIC_REGION_COLORS: [([u8; 3], [u8; 3]); 4] = [
    ([0x4e, 0xb0, 0x4a], [43, 150, 53]),
    ([0x3f, 0x5f, 0xc0], [60, 102, 176]),
    ([0xd9, 0xaa, 0x2a], [196, 158, 26]),
    ([0x7a, 0x4f, 0xc0], [97, 63, 175]),
];

/// A selected region keeps its colour but gets deeper and more saturated (Logic: a green
/// region goes from 43, 150, 53 to 15, 136, 29).
fn selected_fill(c: Color32) -> Color32 {
    let (r, g, b) = (c.r() as f32, c.g() as f32, c.b() as f32);
    let lum = 0.3 * r + 0.59 * g + 0.11 * b;
    let k = |v: f32| ((lum + (v - lum) * 1.45) * 0.8).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(k(r), k(g), k(b))
}

fn region_fill(c: Color32) -> Color32 {
    if let Some((_, [r, g, b])) = LOGIC_REGION_COLORS
        .iter()
        .find(|(track, _)| *track == [c.r(), c.g(), c.b()])
    {
        return Color32::from_rgb(*r, *g, *b);
    }
    // Any other colour gets the same kind of darkening.
    let ch = |v: u8| v as f32 * 0.72;
    let (r, g, b) = (ch(c.r()), ch(c.g()), ch(c.b()));
    let lum = 0.3 * r + 0.59 * g + 0.11 * b;
    let sat = |v: f32| (lum + (v - lum) * 1.1).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(sat(r), sat(g), sat(b))
}

// ------------------------------------------------------------------ take folders

/// Height of one take's row in an opened take folder.
const TAKE_ROW_H: f32 = 22.0;
/// The yellow header of a take folder.
const FOLDER_HEAD_H: f32 = 14.0;
/// Logic's take folder header is yellow.
const FOLDER_YELLOW: Color32 = Color32::from_rgb(253, 197, 0);

/// Extra height a track's row needs for its opened take folders (one row per take).
fn take_rows_height(t: &Track) -> f32 {
    t.regions
        .iter()
        .filter_map(|r| r.folder.as_ref())
        .filter(|f| f.open)
        .map(|f| f.takes.len() as f32 * TAKE_ROW_H)
        .fold(0.0, f32::max)
}

/// The triangle that opens and closes a take folder, at the left of its header.
fn folder_toggle_rect(full: Rect) -> Rect {
    Rect::from_min_size(full.min, vec2(16.0, FOLDER_HEAD_H))
}

/// The row of take number `take` (0 = first take) in an opened folder: the latest take is on
/// top, as in Logic. `full` is the folder's own rectangle.
fn take_row_rect(full: Rect, takes: usize, take: usize) -> Rect {
    let k = takes - 1 - take;
    Rect::from_min_size(
        pos2(full.left(), full.bottom() + 1.0 + k as f32 * TAKE_ROW_H),
        vec2(full.width(), TAKE_ROW_H - 2.0),
    )
}

/// Thin note lines (pitch up, time right) of `notes` in `inner`, clipped to `clip`.
#[allow(clippy::too_many_arguments)]
fn draw_note_lines<'a>(
    g: &Painter,
    inner: Rect,
    left: f32,
    beat_px: f32,
    notes: impl Iterator<Item = &'a crate::model::Note>,
    lo: f32,
    hi: f32,
    ink: Color32,
) {
    let step = (2.0f32).min((inner.height() - 4.0) / (hi - lo).max(1.0));
    let mid = (lo + hi) / 2.0;
    for n in notes {
        let nx = left + n.start * beat_px;
        if nx > inner.right() || nx + n.len * beat_px < inner.left() {
            continue;
        }
        let ny = inner.center().y - (n.pitch as f32 - mid) * step;
        let nr = Rect::from_min_max(
            pos2(nx.max(inner.left()), ny - 0.75),
            pos2(
                (nx + (n.len * beat_px).max(2.0)).min(inner.right()),
                ny + 0.75,
            ),
        );
        g.rect_filled(nr, 0.0, ink);
    }
}

/// A take folder as Logic draws it: a yellow header with the name and a triangle, and under
/// it the comp, the stretches of the takes that play, side by side with thin lines between.
/// Opened, a row for each take follows, the stretches the comp uses lit in the region colour.
#[allow(clippy::too_many_arguments)]
fn draw_take_folder(
    g: &Painter,
    full: Rect,
    track: &Track,
    region: &Region,
    folder: &crate::model::TakeFolder,
    beat_px: f32,
    selected: bool,
    silenced: bool,
) {
    let base = if region.params.mute {
        Color32::from_gray(0x6a)
    } else {
        track.color
    };
    let fill = if silenced {
        Color32::from_rgb(0x2e, 0x2e, 0x30)
    } else if selected {
        selected_fill(region_fill(base))
    } else {
        region_fill(base)
    };
    let ink = if silenced {
        Color32::from_gray(0x8c)
    } else {
        fill.lerp_to_gamma(Color32::WHITE, if selected { 0.75 } else { 0.6 })
    };
    let head_h = FOLDER_HEAD_H.min(full.height() * 0.4);
    let round = 4;
    let body = Rect::from_min_max(pos2(full.left(), full.top() + head_h), full.max);
    // The comp: one block per stretch, rounded at the ends of the folder only.
    let last = folder.comp.len().saturating_sub(1);
    for (i, seg) in folder.comp.iter().enumerate() {
        let x0 = (full.left() + seg.start * beat_px).max(full.left());
        let x1 = (full.left() + seg.end * beat_px).min(full.right());
        if x1 <= x0 {
            continue;
        }
        g.rect_filled(
            Rect::from_min_max(pos2(x0, body.top()), pos2(x1, body.bottom())),
            egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: if i == 0 { round } else { 0 },
                se: if i == last { round } else { 0 },
            },
            fill,
        );
    }
    // The header.
    let head = Rect::from_min_size(full.min, vec2(full.width(), head_h));
    let head_fill = if silenced {
        Color32::from_gray(0x70)
    } else {
        FOLDER_YELLOW
    };
    g.rect_filled(
        head,
        egui::CornerRadius {
            nw: round,
            ne: round,
            sw: 0,
            se: 0,
        },
        head_fill,
    );
    let dark = Color32::from_rgb(40, 30, 0);
    // The triangle: pointing right when closed, down when opened.
    let c = pos2(full.left() + 8.0, head.center().y);
    let tri = if folder.open {
        vec![
            c + vec2(-3.5, -2.0),
            c + vec2(3.5, -2.0),
            c + vec2(0.0, 3.0),
        ]
    } else {
        vec![
            c + vec2(-2.0, -3.5),
            c + vec2(-2.0, 3.5),
            c + vec2(3.0, 0.0),
        ]
    };
    g.add(Shape::convex_polygon(tri, dark, Stroke::NONE));
    g.text(
        head.left_center() + vec2(16.0, 0.0),
        Align2::LEFT_CENTER,
        &region.name,
        FontId::proportional(10.5),
        dark,
    );
    // Where one stretch gives way to the next: a notch in the header and a line below it.
    for seg in folder.comp.iter().skip(1) {
        let x = full.left() + seg.start * beat_px;
        if x <= full.left() || x >= full.right() {
            continue;
        }
        g.line_segment(
            [pos2(x, head.bottom() - 4.0), pos2(x, head.bottom())],
            Stroke::new(1.0, dark),
        );
        g.line_segment(
            [pos2(x, body.top()), pos2(x, body.bottom())],
            Stroke::new(1.0, Color32::from_black_alpha(150)),
        );
    }
    // What the comp plays.
    let (lo, hi) = pitch_range(&region.notes);
    let inner = Rect::from_min_max(
        pos2(body.left(), body.top() + 2.0),
        body.max - vec2(0.0, 2.0),
    );
    draw_note_lines(
        g,
        inner,
        full.left(),
        beat_px,
        region.notes.iter(),
        lo,
        hi,
        ink,
    );
    let border = if selected {
        SELECTED_EDGE
    } else {
        Color32::from_black_alpha(110)
    };
    g.rect_stroke(
        full,
        egui::CornerRadius::same(round),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    // The takes, newest on top.
    if folder.open {
        for (take_idx, take) in folder.takes.iter().enumerate() {
            let row = take_row_rect(full, folder.takes.len(), take_idx);
            g.rect_filled(row, 3.0, Color32::from_gray(0x3a));
            let row_inner =
                Rect::from_min_max(pos2(row.left(), row.top() + 2.0), row.max - vec2(0.0, 2.0));
            let (lo, hi) = pitch_range(&take.notes);
            // The stretches of this take that the comp plays are lit.
            for seg in folder.comp.iter().filter(|s| s.take == take_idx) {
                let x0 = (full.left() + seg.start * beat_px).max(full.left());
                let x1 = (full.left() + seg.end * beat_px).min(full.right());
                if x1 > x0 {
                    g.rect_filled(
                        Rect::from_min_max(pos2(x0, row.top()), pos2(x1, row.bottom())),
                        3.0,
                        fill,
                    );
                }
            }
            let used = |start: f32| {
                folder
                    .comp
                    .iter()
                    .any(|s| s.take == take_idx && start >= s.start && start < s.end)
            };
            draw_note_lines(
                g,
                row_inner,
                full.left(),
                beat_px,
                take.notes.iter().filter(|n| used(n.start)),
                lo,
                hi,
                ink,
            );
            draw_note_lines(
                g,
                row_inner,
                full.left(),
                beat_px,
                take.notes.iter().filter(|n| !used(n.start)),
                lo,
                hi,
                Color32::from_gray(0x86),
            );
            g.text(
                row.left_top() + vec2(5.0, 1.0),
                Align2::LEFT_TOP,
                trf("Take {}", &[&(take_idx + 1)]),
                FontId::proportional(9.5),
                Color32::from_white_alpha(200),
            );
        }
    }
}

/// Lowest and highest pitch of `notes` (a default span when there are none).
fn pitch_range(notes: &[crate::model::Note]) -> (f32, f32) {
    let lo = notes.iter().map(|n| n.pitch).min().unwrap_or(48) as f32;
    let hi = notes.iter().map(|n| n.pitch).max().unwrap_or(72) as f32;
    (lo, hi)
}

#[allow(clippy::too_many_arguments)]
fn draw_region(
    g: &Painter,
    rr: Rect,
    full: Rect,
    content_origin: f32,
    orig_width: f32,
    track: &Track,
    region: &Region,
    fades: (f32, f32),
    beat_px: f32,
    selected: bool,
    recording: bool,
    silenced: bool,
    seed: u32,
    waveform_display: crate::project_settings::WaveformDisplay,
) {
    // Muted regions are drawn grey, like Logic.
    let base = if region.params.mute {
        Color32::from_gray(0x6a)
    } else {
        track.color
    };
    let body = if recording {
        // The region being recorded is red until the recording stops.
        RECORDING_FILL
    } else if silenced {
        // Another track is soloed: dark grey body, grey content, name in the track colour.
        Color32::from_rgb(0x2e, 0x2e, 0x30)
            .lerp_to_gamma(Color32::WHITE, if selected { 0.06 } else { 0.0 })
    } else if selected {
        selected_fill(region_fill(base))
    } else {
        region_fill(base)
    };
    g.rect_filled(rr, 3.0, body);
    let head_h = 14.0f32.min(rr.height() * 0.4);
    let head = Rect::from_min_size(rr.min, vec2(rr.width(), head_h));
    g.rect_filled(
        head,
        egui::CornerRadius {
            nw: 3,
            ne: 3,
            sw: 0,
            se: 0,
        },
        body.lerp_to_gamma(Color32::WHITE, if silenced { 0.03 } else { 0.0 }),
    );
    let name_color = if silenced {
        base.lerp_to_gamma(Color32::WHITE, 0.15)
    } else {
        body.lerp_to_gamma(Color32::WHITE, if selected { 0.7 } else { 0.85 })
    };
    if let Some(generated) = &region.generated {
        if head.width() > 44.0 {
            draw_generated_mark(
                g,
                head,
                matches!(generated.source, crate::model::Generator::Drummer(_)),
                name_color,
            );
        }
    }
    g.text(
        head.left_center() + vec2(5.0, 0.0),
        Align2::LEFT_CENTER,
        &region.name,
        FontId::proportional(10.5),
        name_color,
    );
    let inner = Rect::from_min_max(
        pos2(rr.left(), head.bottom() + 2.0),
        rr.max - vec2(0.0, 2.0),
    );
    // A Flex edit: its mode in the corner of the name bar and its markers over the waveform.
    if let Some(f) = region.audio.as_ref().and_then(|a| a.flex.as_ref()) {
        if head.width() > 90.0 {
            let mark = if f.pitch.is_empty() {
                crate::flex_edit::mode_label(f.mode).to_string()
            } else {
                format!("{} ♪", crate::flex_edit::mode_label(f.mode))
            };
            g.text(
                head.right_center() - vec2(5.0, 0.0),
                Align2::RIGHT_CENTER,
                mark,
                FontId::proportional(9.5),
                name_color,
            );
        }
        // Markers are placed on the result's length, as the pointer is mapped to it.
        let secs = f.length().max(1e-3);
        for m in &f.markers {
            let x = content_origin + (m.dst / secs) as f32 * orig_width;
            if x > inner.left() && x < inner.right() {
                g.line_segment(
                    [pos2(x, inner.top()), pos2(x, inner.bottom())],
                    Stroke::new(1.0, Color32::from_rgb(0xff, 0xb0, 0x30).gamma_multiply(0.7)),
                );
            }
        }
    }
    let ink = if recording {
        Color32::from_rgb(236, 152, 155)
    } else if silenced {
        Color32::from_gray(0x8c)
    } else {
        // Logic draws the content in a pale tint of the region colour, not pure white.
        body.lerp_to_gamma(Color32::WHITE, if selected { 0.75 } else { 0.6 })
    };
    let edit = region.audio.is_some() || track.kind == TrackKind::Audio;
    if let Some((a, peaks)) = region
        .audio
        .as_ref()
        .and_then(|a| Some((a, a.peaks.get()?)))
    {
        let gain = 10f32.powf(region.params.gain_db / 20.0);
        let full_x = (content_origin, orig_width);
        // A piece of a split draws only the part of the file it plays.
        draw_peaks(
            g,
            inner,
            full_x,
            a.peaks_part(peaks),
            gain,
            region.params.reverse,
            ink,
            waveform_display,
        );
    } else if region.audio.is_some() {
        g.line_segment(
            [
                pos2(inner.left(), inner.center().y),
                pos2(inner.right(), inner.center().y),
            ],
            Stroke::new(1.0, ink),
        );
    } else if track.kind == TrackKind::Audio {
        fake_waveform(g, inner, content_origin, beat_px, ink, seed);
    } else if !region.notes.is_empty() {
        let lo = region.notes.iter().map(|n| n.pitch).min().unwrap_or(48) as f32;
        let hi = region.notes.iter().map(|n| n.pitch).max().unwrap_or(72) as f32;
        // Logic keeps a fixed, small step per semitone and centres the notes in the region.
        let step = (2.0f32).min((inner.height() - 4.0) / (hi - lo).max(1.0));
        let mid = (lo + hi) / 2.0;
        for n in &region.notes {
            let nx = content_origin + n.start * beat_px;
            if nx > inner.right() || nx + n.len * beat_px < inner.left() {
                continue;
            }
            let ny = inner.center().y - (n.pitch as f32 - mid) * step;
            let left_x = nx.max(inner.left());
            let right_x = (nx + (n.len * beat_px).max(2.0)).min(inner.right());
            if right_x > left_x {
                let nr = Rect::from_min_max(pos2(left_x, ny - 0.75), pos2(right_x, ny + 0.75));
                g.rect_filled(nr, 0.0, ink);
            }
        }
    }
    if edit {
        draw_fades(g, rr, inner, full, region, fades, beat_px);
    }
    let border = if recording {
        Color32::from_rgb(120, 22, 22)
    } else if selected {
        SELECTED_EDGE
    } else if silenced {
        Color32::from_gray(0x44)
    } else {
        Color32::from_black_alpha(110)
    };
    if selected && !recording {
        // Logic outlines a selected region in yellow-green with a dark line around it.
        g.rect_stroke(
            rr,
            3.0,
            Stroke::new(1.0, Color32::from_black_alpha(220)),
            StrokeKind::Outside,
        );
    }
    g.rect_stroke(rr, 3.0, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// A small mark at the right of a region's header that tells a Drummer region (a drum with
/// its sticks) from a Pattern region (a grid of steps).
fn draw_generated_mark(g: &Painter, head: Rect, drummer: bool, color: Color32) {
    let c = pos2(head.right() - 10.0, head.center().y);
    let stroke = Stroke::new(1.0, color);
    if drummer {
        g.rect_stroke(
            Rect::from_center_size(c + vec2(0.0, 1.5), vec2(8.0, 4.5)),
            1.0,
            stroke,
            StrokeKind::Middle,
        );
        g.line_segment([c + vec2(-4.0, -2.0), c + vec2(-1.0, -4.5)], stroke);
        g.line_segment([c + vec2(4.0, -2.0), c + vec2(1.0, -4.5)], stroke);
    } else {
        for row in 0..2 {
            for col in 0..3 {
                let at = c + vec2(col as f32 * 3.6 - 3.6, row as f32 * 3.6 - 1.8);
                g.rect_filled(Rect::from_center_size(at, vec2(2.6, 2.6)), 0.5, color);
            }
        }
    }
}

/// Height (0.3..1.0) of a demo audio track's pseudo-waveform `beats` into its region. It
/// depends only on the position in the region, so the waveform moves with the region when the
/// view scrolls or zooms.
fn fake_level(seed: u32, beats: f32) -> f32 {
    let step = (beats * 16.0).floor() as u32;
    let mut s = seed ^ step.wrapping_mul(0x9e37_79b9);
    s = s.wrapping_mul(1664525).wrapping_add(1013904223);
    s ^= s >> 15;
    s = s.wrapping_mul(0x2c1b_3c6d);
    s ^= s >> 12;
    ((beats * 1.6).sin().abs() * 0.6 + 0.3) * ((s >> 24) as f32 / 255.0 * 0.6 + 0.4)
}

/// Demo audio tracks without a file get a stable pseudo-waveform. `rect` is the visible part
/// of the region; `x0` is where the whole region starts on screen.
fn fake_waveform(g: &Painter, rect: Rect, x0: f32, beat_px: f32, col: Color32, seed: u32) {
    let mid = rect.center().y;
    // Start on the same 2-px grid as the region's left edge, so columns do not shimmer.
    let mut x = x0 + ((rect.left() - x0) / 2.0).floor().max(0.0) * 2.0;
    while x < rect.right() {
        let h = fake_level(seed, (x - x0) / beat_px.max(1e-3)) * rect.height() * 0.45;
        g.line_segment([pos2(x, mid - h), pos2(x, mid + h)], Stroke::new(1.0, col));
        x += 2.0;
    }
}

/// Real waveform from min/max peaks. `x0`/`full_w` describe the unclipped region.
/// Shades what the region's fades take away: a triangle from the region's start up to where
/// the fade in ends, and one down to the region's end for the fade out, with the curve's line.
fn draw_fades(
    g: &Painter,
    rr: Rect,
    inner: Rect,
    full: Rect,
    region: &Region,
    (fin, fout): (f32, f32),
    beat_px: f32,
) {
    if fin <= 0.0 && fout <= 0.0 {
        return;
    }
    let g = g.with_clip_rect(g.clip_rect().intersect(rr));
    let shade = Color32::from_black_alpha(110);
    let line = Stroke::new(1.2, Color32::from_white_alpha(220));
    let curve = |x: f32| match region.params.fade_curve {
        prowave_engine::FadeCurve::Linear => x,
        prowave_engine::FadeCurve::EqualPower => (x * std::f32::consts::FRAC_PI_2).sin(),
        prowave_engine::FadeCurve::SCurve => x * x * (3.0 - 2.0 * x),
    };
    // `x0` where the fade is silent, `x1` where it reaches full level.
    let fade = |x0: f32, x1: f32| {
        const STEPS: usize = 16;
        let pts: Vec<Pos2> = (0..=STEPS)
            .map(|k| {
                let t = k as f32 / STEPS as f32;
                let y = inner.bottom() - curve(t) * inner.height();
                pos2(x0 + (x1 - x0) * t, y)
            })
            .collect();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            g.add(Shape::convex_polygon(
                vec![pos2(a.x, inner.top()), pos2(b.x, inner.top()), b, a],
                shade,
                Stroke::NONE,
            ));
        }
        g.add(Shape::line(pts, line));
    };
    if fin > 0.0 {
        fade(full.left(), full.left() + fin * beat_px);
    }
    if fout > 0.0 {
        fade(full.right(), full.right() - fout * beat_px);
    }
}

/// Draws a recorded waveform: `full_x` is the left edge and width of the whole region on
/// screen, `gain` its region gain (linear) and `reverse` plays it back to front.
/// How tall a waveform sample of size `a` (0 to 1, or more when the region is boosted) is
/// drawn, as a fraction of the full height: a soft logarithmic curve, so a quiet recording (a
/// microphone peaks at 0.05 to 0.2) shows its shape instead of a flat line, while silence stays
/// a line and full scale stays full height. It only changes the picture, never the audio.
pub(crate) fn display_amp(a: f32) -> f32 {
    const K: f32 = 40.0;
    if !a.is_finite() {
        return 0.0;
    }
    let m = a.abs().min(4.0);
    (1.0 + K * m).ln() / (1.0 + K).ln() * a.signum()
}

#[cfg(test)]
pub(crate) fn region_look_for_test(track: &Track, region: &Region) -> crate::theme::RegionLook {
    let color = region.params.color.unwrap_or(track.color);
    crate::theme::region_look(
        color,
        region.is_audio() || track.kind == TrackKind::Audio,
        false,
        false,
    )
}

fn draw_peaks(
    g: &Painter,
    body: Rect,
    (x0, full_w): (f32, f32),
    peaks: &[(f32, f32)],
    gain: f32,
    reverse: bool,
    col: Color32,
    waveform_display: crate::project_settings::WaveformDisplay,
) {
    if peaks.is_empty() || full_w <= 0.0 {
        return;
    }
    let mid = body.center().y;
    let half = body.height() * 0.48;
    let mut x = body.left().floor();
    while x < body.right() {
        let a = (((x - x0) / full_w) * peaks.len() as f32).max(0.0) as usize;
        if a >= peaks.len() {
            break;
        }
        let b =
            ((((x + 1.0 - x0) / full_w) * peaks.len() as f32) as usize).clamp(a + 1, peaks.len());
        let (a, b) = if reverse {
            (peaks.len() - b, peaks.len() - a)
        } else {
            (a, b)
        };
        let (lo, hi) = peaks[a..b]
            .iter()
            .fold((0f32, 0f32), |(l, h), p| (l.min(p.0), h.max(p.1)));
        let (lo, hi) = (display_amp(lo * gain), display_amp(hi * gain));
        match waveform_display {
            crate::project_settings::WaveformDisplay::BipolarStereo => {
                g.line_segment(
                    [
                        pos2(x, mid - hi.min(1.0) * half),
                        pos2(x, mid - lo.max(-1.0) * half),
                    ],
                    Stroke::new(1.0, col),
                );
            }
            crate::project_settings::WaveformDisplay::RectifiedLinear => {
                let peak = hi.abs().max(lo.abs()).min(1.0);
                g.line_segment(
                    [
                        pos2(x, body.bottom()),
                        pos2(x, body.bottom() - peak * body.height() * 0.92),
                    ],
                    Stroke::new(1.0, col),
                );
            }
            crate::project_settings::WaveformDisplay::LogarithmicRms => {
                let peak = hi.abs().max(lo.abs()).min(1.0);
                let rms = peak.sqrt();
                g.line_segment(
                    [pos2(x, mid - rms * half), pos2(x, mid + rms * half)],
                    Stroke::new(1.0, col),
                );
            }
        }
        x += 1.0;
    }
}

/// Triangle pointing down (open) or right (folded), drawn as a shape so it needs no font glyph.
fn disclosure_triangle(g: &Painter, c: Pos2, open: bool, col: Color32) {
    let pts = if open {
        vec![
            c + vec2(-4.0, -2.5),
            c + vec2(4.0, -2.5),
            c + vec2(0.0, 3.5),
        ]
    } else {
        vec![
            c + vec2(-2.5, -4.0),
            c + vec2(-2.5, 4.0),
            c + vec2(3.5, 0.0),
        ]
    };
    g.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// A folder's M and S buttons: they set the same state on every track inside the folder.
fn folder_buttons(ui: &Ui, g: &Painter, p: &mut Project, i: usize, x: f32, y: f32, selected: bool) {
    let size = vec2(16.0, 16.0);
    let (mute, solo) = (p.tracks[i].mute, p.tracks[i].solo);
    if header_button(
        ui,
        g,
        Rect::from_min_size(pos2(x, y), size),
        (i, "m"),
        "M",
        mute,
        MUTE_ON,
        selected,
    ) {
        p.set_folder_flags(i, Some(!mute), None);
    }
    if header_button(
        ui,
        g,
        Rect::from_min_size(pos2(x + 19.0, y), size),
        (i, "s"),
        "S",
        solo,
        SOLO_ON,
        selected,
    ) {
        p.set_folder_flags(i, None, Some(!solo));
    }
}

const SLIDER_W: f32 = 76.0;
const SLIDER_H: f32 = 14.0;

/// Where 0 dB sits on a track header's volume slider (Logic puts unity about two thirds up).
const SLIDER_UNITY: f32 = 0.65;

/// Slider position (0..1) for a volume in dB (-60..+6): most of the travel is below unity,
/// the last third is the boost to +6 dB.
fn db_to_slider(db: f32) -> f32 {
    let db = db.clamp(-60.0, 6.0);
    if db >= 0.0 {
        SLIDER_UNITY + (1.0 - SLIDER_UNITY) * db / 6.0
    } else {
        SLIDER_UNITY * ((db + 60.0) / 60.0).sqrt()
    }
}

fn slider_to_db(f: f32) -> f32 {
    let f = f.clamp(0.0, 1.0);
    if f >= SLIDER_UNITY {
        (f - SLIDER_UNITY) / (1.0 - SLIDER_UNITY) * 6.0
    } else {
        -60.0 + 60.0 * (f / SLIDER_UNITY).powi(2)
    }
}

/// Draws track `i`'s header and returns the response of its background (for the right-click
/// menu; the buttons on it handle their own clicks).
/// The volume slider of a track header (dB, -60..+6; double-click resets to 0 dB) with the level
/// meters inside it.
fn volume_slider(
    ui: &Ui,
    g: &Painter,
    t: &mut Track,
    i: usize,
    hdr: Rect,
    peaks: (f32, f32),
    silenced: bool,
) {
    // Volume slider with embedded green level meter (right-aligned, 10px margin)
    let sl = Rect::from_min_size(
        pos2(
            hdr.right() - 10.0 - SLIDER_W,
            hdr.center().y - SLIDER_H / 2.0,
        ),
        vec2(SLIDER_W, SLIDER_H),
    );
    let travel = (sl.left() + SLIDER_H / 2.0, sl.right() - SLIDER_H / 2.0);
    let resp = ui.interact(
        sl.expand2(vec2(4.0, 6.0)),
        id(("vol", i)),
        Sense::click_and_drag(),
    );
    if let Some(pt) = resp.interact_pointer_pos() {
        let f = (pt.x - travel.0) / (travel.1 - travel.0);
        t.volume_db = slider_to_db(f.clamp(0.0, 1.0));
    }
    if resp.double_clicked() {
        t.volume_db = 0.0;
    }
    g.rect_filled(sl, SLIDER_H / 2.0, Color32::from_rgb(0x18, 0x18, 0x1a));
    g.rect_stroke(
        sl,
        SLIDER_H / 2.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        egui::StrokeKind::Inside,
    );
    // Level meter inside the pill, as in Logic: two thin bars (L over R) that follow the
    // slider's own scale, green, yellow from -6 dB and red above 0 dB.
    let silent = t.mute || silenced;
    for (k, peak) in [peaks.0, peaks.1].into_iter().enumerate() {
        let db = crate::mixer::lin_to_db(peak);
        if db <= -60.0 {
            continue;
        }
        let y = sl.center().y - 3.0 + k as f32 * 3.5;
        let x0 = sl.left() + 3.0;
        let x_at = |db: f32| x0 + (sl.right() - 3.0 - x0) * db_to_slider(db);
        let mut from = -60.0f32;
        for edge in [-6.0f32, 0.0, 6.0] {
            let to = db.min(edge);
            if to > from {
                let r = Rect::from_min_max(pos2(x_at(from), y), pos2(x_at(to), y + 2.5));
                let color = if silent {
                    Color32::from_gray(0x70)
                } else {
                    crate::mixer::meter_colour(to - 0.01)
                };
                g.rect_filled(r, 0.5, color);
            }
            from = edge;
        }
    }
    let knob = pos2(
        travel.0 + (travel.1 - travel.0) * db_to_slider(t.volume_db),
        sl.center().y,
    );
    // White/silver tactile fader thumb
    g.circle_filled(
        knob + vec2(0.5, 1.0),
        SLIDER_H / 2.0 - 0.5,
        Color32::from_black_alpha(100),
    );
    g.circle_filled(
        knob,
        SLIDER_H / 2.0 - 1.0,
        Color32::from_rgb(0xea, 0xee, 0xf4),
    );
    g.circle_stroke(
        knob,
        SLIDER_H / 2.0 - 1.0,
        Stroke::new(1.0, Color32::from_rgb(0x8a, 0x93, 0xa5)),
    );
    g.circle_filled(knob, 1.5, Color32::from_rgb(0x25, 0x2a, 0x37));
}

/// The pan knob of a track header (drag up or down; double-click centres it).
fn pan_knob(ui: &Ui, g: &Painter, t: &mut Track, i: usize, hdr: Rect) {
    let kc = pos2(hdr.right() - 22.0, hdr.center().y);
    let kresp = ui.interact(
        Rect::from_center_size(kc, vec2(22.0, 22.0)),
        id(("pan", i)),
        Sense::click_and_drag(),
    );
    if kresp.dragged() {
        t.pan = (t.pan - kresp.drag_delta().y * 0.01).clamp(-1.0, 1.0);
    }
    if kresp.double_clicked() {
        t.pan = 0.0;
    }
    crate::widgets::draw_radial_pan(g, kc, 8.5, t.pan);
}

/// The lane band of a track stack: the span, in quarter notes, of the regions of the tracks
/// inside it. `None` when none of them has a region.
fn stack_band(p: &Project, i: usize) -> Option<(f64, f64)> {
    let end = p.folder_end(i);
    let mut span: Option<(f64, f64)> = None;
    for t in &p.tracks[i + 1..end] {
        for r in &t.regions {
            let (a, b) = (p.region_start_beats(r), p.region_end_beats(r));
            span = Some(span.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
        }
    }
    span
}

/// Synchronizes the bottom editor panel to match the clicked track's type:
/// Session Player (Drummer, Keyboard, Bass), Piano Roll, or Step Sequencer.
pub(crate) fn sync_bottom_editor_to_track(
    p: &crate::model::Project,
    bottom: &mut crate::Bottom,
    score_tab: &mut bool,
    ti: usize,
) {
    if ti >= p.tracks.len() {
        return;
    }
    let track = &p.tracks[ti];
    let mut has_drummer = track.kind == TrackKind::Drummer;
    let mut has_keyboard = false;
    let mut has_bass = false;
    let mut has_step_seq = false;
    let mut has_midi = false;

    for r in &track.regions {
        if let Some(g) = &r.generated {
            match &g.source {
                crate::model::Generator::Drummer(_) => has_drummer = true,
                crate::model::Generator::Session(sp) => match sp.kind {
                    crate::session_player::SessionPlayerKind::Drummer => has_drummer = true,
                    crate::session_player::SessionPlayerKind::Keyboard => has_keyboard = true,
                    crate::session_player::SessionPlayerKind::Bass => has_bass = true,
                },
                crate::model::Generator::Pattern(_) => has_step_seq = true,
            }
        } else if track.kind.has_regions() && track.kind != TrackKind::Audio {
            has_midi = true;
        }
    }

    let name_lower = track.name.to_lowercase();
    if !has_drummer && !has_keyboard && !has_bass && !has_step_seq {
        if name_lower.contains("drum") || track.kind == TrackKind::Drummer {
            has_drummer = true;
        } else if name_lower.contains("bass") {
            has_bass = true;
        } else if name_lower.contains("piano") || name_lower.contains("key") {
            has_keyboard = true;
        } else if name_lower.contains("pattern") || name_lower.contains("step") {
            has_step_seq = true;
        }
    }

    if has_drummer || has_keyboard || has_bass {
        *bottom = crate::Bottom::SessionPlayer;
    } else if has_step_seq {
        *bottom = crate::Bottom::PianoRoll;
        *score_tab = true;
    } else if has_midi || track.kind == TrackKind::Instrument {
        *bottom = crate::Bottom::PianoRoll;
        *score_tab = false;
    }
}

/// What a track header shows besides the project: the level meters, whether solo silences the
/// track, and whether the track is among the selected ones (Shift- or Command-click).
#[derive(Clone, Copy)]
struct HeaderState {
    peaks: (f32, f32),
    silenced: bool,
    picked: bool,
    hide_view: bool,
}

fn header(
    ui: &Ui,
    g: &Painter,
    p: &mut Project,
    i: usize,
    hdr: Rect,
    state: HeaderState,
) -> egui::Response {
    let HeaderState {
        peaks,
        silenced,
        picked,
        hide_view,
    } = state;
    let selected = p.selected_track == i || picked;
    // Track > Configure Track Header: the parts the user hid are not drawn (their space stays).
    let (show_number, show_icon, show_name, show_buttons, show_volume, show_pan) = (
        !p.header_hidden.contains(&HeaderPart::Number),
        !p.header_hidden.contains(&HeaderPart::Icon),
        !p.header_hidden.contains(&HeaderPart::Name),
        !p.header_hidden.contains(&HeaderPart::Buttons),
        !p.header_hidden.contains(&HeaderPart::Volume),
        !p.header_hidden.contains(&HeaderPart::Pan),
    );
    g.rect_filled(hdr, 0.0, if selected { HEADER_SEL } else { HEADER_BG });
    g.line_segment(
        [hdr.left_bottom(), hdr.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
    );
    // 4px flush left vertical track color bar
    let color_bar = Rect::from_min_size(hdr.min, vec2(4.0, hdr.height()));
    let track_color = p.tracks.get(i).map_or(Color32::TRANSPARENT, |t| t.color);
    g.rect_filled(color_bar, 0.0, track_color);
    let num = Rect::from_min_size(
        pos2(hdr.left() + 4.0, hdr.top()),
        vec2(NUM_W - 4.0, hdr.height()),
    );
    if show_number {
        g.text(
            num.center(),
            Align2::CENTER_CENTER,
            (i + 1).to_string(),
            FontId::proportional(12.0),
            TEXT_DIM,
        );
    }
    // The click that selects the track is handled by the caller, which knows the modifier keys.
    let resp = ui.interact(hdr, id(("hdr", i)), Sense::click());

    let h = hdr.height();
    let indent = f32::from(p.tracks[i].indent) * INDENT_W;
    // A track stack's parent: the arrow folds its tracks, and its level and mute reach them.
    let folder = p.tracks[i].is_stack_parent();
    let content_x = hdr.left() + NUM_W + 6.0 + indent;
    if folder {
        // Fold arrow: click to hide or show the tracks inside.
        let arrow = Rect::from_center_size(pos2(content_x + 5.0, hdr.center().y), vec2(14.0, 20.0));
        if ui
            .interact(arrow, id(("fold", i)), Sense::click())
            .clicked()
        {
            p.tracks[i].folded = !p.tracks[i].folded;
        }
        disclosure_triangle(g, arrow.center(), !p.tracks[i].folded, TEXT_DIM);
    }
    if p.tracks[i].indent > 0 {
        // The tracks inside a stack are marked with a thin bar at the left of their block.
        let x = hdr.left() + NUM_W + 6.0 + f32::from(p.tracks[i].indent - 1) * INDENT_W + 5.0;
        g.line_segment(
            [pos2(x, hdr.top()), pos2(x, hdr.bottom())],
            Stroke::new(1.0, STACK_COLOR),
        );
    }
    let badge_size = 24.0f32;
    let badge_r = Rect::from_center_size(
        pos2(
            content_x + if folder { 14.0 } else { 0.0 } + badge_size / 2.0,
            hdr.center().y,
        ),
        vec2(badge_size, badge_size),
    );
    if show_icon {
        g.rect_filled(badge_r, 5.0, Color32::from_rgb(0x18, 0x18, 0x1a));
        g.rect_stroke(
            badge_r,
            5.0,
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
            egui::StrokeKind::Inside,
        );
        draw_icon(g, badge_r.shrink(4.5), track_icon(&p.tracks[i]));
    }

    let name_x = if show_icon {
        badge_r.right() + 8.0
    } else {
        content_x + if folder { 14.0 } else { 0.0 }
    };
    let compact = h < 46.0;
    let name_y = if compact {
        hdr.center().y
    } else {
        hdr.center().y - 9.0
    };
    let name_w = hdr.right() - 122.0 - name_x;
    let name_galley = g.layout_no_wrap(p.tracks[i].name.clone(), FontId::proportional(13.0), TEXT);
    let clip = g.with_clip_rect(
        Rect::from_min_size(pos2(name_x, hdr.top()), vec2(name_w.max(20.0), h))
            .intersect(g.clip_rect()),
    );
    if show_name {
        clip.galley(
            pos2(name_x, name_y - name_galley.size().y / 2.0),
            name_galley,
            TEXT,
        );
    }

    // A folder only has M and S, which act on everything inside it, and no volume or pan.
    if folder {
        if !compact && show_buttons {
            folder_buttons(ui, g, p, i, name_x, hdr.center().y + 3.0, selected);
        }
        // A stack's level scales the tracks inside it; a Summing Stack is a strip, so it has a
        // pan knob too. A folder track has no pan.
        if show_volume {
            volume_slider(ui, g, &mut p.tracks[i], i, hdr, peaks, silenced);
        }
        return resp;
    }

    // M S R (I) 16x16 boxed micro-buttons under the name
    let audio = p.tracks[i].kind == TrackKind::Audio;
    let t = &mut p.tracks[i];
    if !compact && show_buttons {
        let by = hdr.center().y + 3.0;
        let mut bx = name_x;
        let size = vec2(16.0, 16.0);
        if header_button(
            ui,
            g,
            Rect::from_min_size(pos2(bx, by), size),
            (i, "m"),
            "M",
            t.mute,
            MUTE_ON,
            selected,
        ) {
            t.mute = !t.mute;
        }
        bx += 19.0;
        if header_button(
            ui,
            g,
            Rect::from_min_size(pos2(bx, by), size),
            (i, "s"),
            "S",
            t.solo,
            SOLO_ON,
            selected,
        ) {
            t.solo = !t.solo;
        }
        bx += 19.0;
        // Aux and Folder tracks hold no regions, so there is nothing to record into.
        if t.kind.has_regions()
            && header_button(
                ui,
                g,
                Rect::from_min_size(pos2(bx, by), size),
                (i, "r"),
                "R",
                t.rec,
                REC_ON,
                selected,
            )
        {
            t.rec = !t.rec;
        }
        if audio {
            bx += 19.0;
            if header_button(
                ui,
                g,
                Rect::from_min_size(pos2(bx, by), size),
                (i, "i"),
                "I",
                t.monitor,
                MON_ON,
                selected,
            ) {
                t.monitor = !t.monitor;
            }
        }
        if hide_view {
            bx += 19.0;
            if header_button(
                ui,
                g,
                Rect::from_min_size(pos2(bx, by), size),
                (i, "h"),
                "H",
                t.hidden,
                Color32::from_rgb(0xe0, 0x90, 0x20),
                selected,
            ) {
                t.hidden = !t.hidden;
            }
        }
    }

    if show_volume {
        volume_slider(ui, g, t, i, hdr, peaks, silenced);
    }
    resp
}

// ------------------------------------------------------------------ main view

/// Height of the Master strip (Track > Show Master Track).
const MASTER_STRIP_H: f32 = 36.0;

/// The Master strip: its name and the master fader, the same control as the transport's.
fn master_strip(
    ui: &mut Ui,
    p: &mut Project,
    rect: Rect,
    meters: &crate::mixer::Meters,
    quality: crate::liquid_glass::GlassQuality,
) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 0.0, HEADER_BG);
    g.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, Color32::from_gray(0x2a)),
    );
    g.text(
        pos2(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        tr("Master"),
        FontId::proportional(13.0),
        TEXT,
    );
    let slider = Rect::from_min_max(
        pos2(rect.left() + 96.0, rect.center().y - 11.0),
        pos2(rect.right() - 16.0, rect.center().y + 11.0),
    );
    crate::transport::master_volume(
        ui,
        &g,
        p,
        slider,
        Some(meters.master),
        id(("tracks", "master")),
        quality,
    );
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState, meters: &crate::mixer::Meters) {
    s.tracks.waveform_display = s.app_settings.waveform_display;
    // Regions removed or moved by an undo or a track change are no longer selected.
    s.tracks.prune(p);
    let rec_targets: Vec<(usize, usize)> = s
        .rec
        .target()
        .into_iter()
        .chain(s.audio_rec.regions())
        .collect();
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    // Track > Show Master Track: a strip along the bottom, outside the scrolling tracks.
    let master_h = if p.show_master_track {
        MASTER_STRIP_H
    } else {
        0.0
    };
    let area = Rect::from_min_max(full.min, pos2(full.right(), full.bottom() - master_h));
    if p.show_master_track {
        let strip = Rect::from_min_max(pos2(full.left(), area.bottom()), full.max);
        master_strip(ui, p, strip, meters, s.glass_quality);
    }
    let tb = Rect::from_min_size(area.min, vec2(area.width(), TOOLBAR_H));
    let ruler_y = tb.bottom();
    // Global tracks between the ruler and the tracks: Marker, then Tempo, as in Logic.
    let marker_h = if s.tracks.show_markers {
        crate::marker_track::HEIGHT
    } else {
        0.0
    };
    let tempo_h = if s.tracks.show_tempo {
        crate::tempo_track::HEIGHT
    } else {
        0.0
    };
    let signature_h = if s.tracks.show_signatures {
        crate::signature_track::HEIGHT
    } else {
        0.0
    };
    let chord_h = if s.tracks.show_chords { 26.0 } else { 0.0 };
    let tracks_y = ruler_y + RULER_H + marker_h + signature_h + tempo_h + chord_h;
    let header_w = if area.width() < 1180.0 { 280.0 } else { 340.0 };
    let headers = Rect::from_min_max(
        pos2(area.left(), tracks_y),
        pos2(area.left() + header_w, area.bottom()),
    );
    s.tracks_headers_rect = Some(headers);
    let full_lanes = Rect::from_min_max(pos2(area.left() + header_w, tracks_y), area.max);
    let (loops_area, splitter_r, lanes) = if s.tracks.show_live_loops {
        let max_w = (full_lanes.width() - 250.0).max(180.0);
        let split_w = s.tracks.live_loops_split.clamp(180.0, max_w);
        let loops_r = Rect::from_min_size(full_lanes.min, vec2(split_w, full_lanes.height()));
        let split_r = Rect::from_min_size(
            pos2(loops_r.right(), full_lanes.top()),
            vec2(6.0, full_lanes.height()),
        );
        let lanes_r = Rect::from_min_max(pos2(split_r.right(), full_lanes.top()), full_lanes.max);
        (Some(loops_r), Some(split_r), lanes_r)
    } else {
        (None, None, full_lanes)
    };
    let ruler = Rect::from_min_max(
        pos2(lanes.left(), ruler_y),
        pos2(area.right(), ruler_y + RULER_H),
    );
    let corner = Rect::from_min_max(
        pos2(area.left(), ruler_y),
        pos2(headers.right(), ruler.bottom()),
    );

    // The Tool menu (T) takes its keys before anything else here looks at them.
    if let Some(at) = s.tracks.tool_menu {
        let (picked, open) = crate::piano_tools::tool_menu(ui.ctx(), "tracks", at, s.tracks.tool);
        if let Some(t) = picked {
            s.tracks.tool = t;
        }
        if !open {
            s.tracks.tool_menu = None;
        }
    }
    toolbar(ui, p, s, tb, lanes.width());

    // Zoom is counted in pixels per bar of the first bar's length.
    let bpb = p.bar_len(0);
    let v = &mut s.tracks;
    v.lanes_w = lanes.width();
    v.zoom.lanes_h = lanes.height();
    crate::arrange_zoom::publish_wave_zoom(ui.ctx(), v);

    // ---- wheel: scroll; Shift = horizontal; the zooms (Command, Option, pinch) are in
    // `arrange_zoom::wheel_zoom`
    let hover = ui.input(|i| i.pointer.hover_pos());
    // Middle-click 2D Panning (Logic Pro Hand tool navigation)
    let (middle_down, middle_delta) = ui.input(|i| (i.pointer.middle_down(), i.pointer.delta()));
    let pointer_in_lanes = hover
        .or_else(|| ui.input(|i| i.pointer.interact_pos()))
        .is_some_and(|pt| lanes.contains(pt) || headers.contains(pt));
    if middle_down && pointer_in_lanes {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        v.scroll_x = (v.scroll_x - middle_delta.x).max(0.0);
        v.scroll_y = (v.scroll_y - middle_delta.y).max(0.0);
    }
    if let Some(pt) =
        hover.filter(|&pt| lanes.contains(pt) || headers.contains(pt) || ruler.contains(pt))
    {
        let (delta, mods, zoom) =
            ui.input(|i| (i.smooth_scroll_delta, i.modifiers, i.zoom_delta()));
        if crate::arrange_zoom::wheel_zoom(p, v, lanes, pt, (delta, zoom, mods)) {
            // the zoom took the wheel
        } else if mods.shift {
            v.scroll_x -= delta.y + delta.x;
        } else {
            v.scroll_x -= delta.x;
            v.scroll_y -= delta.y;
        }
    }
    let rows = rows_as_drawn(p, &s.auto, v.show_hidden, v.row_h);
    let content_h = rows.last().map_or(0.0, |(top, h)| top + h);
    v.scroll_y = v
        .scroll_y
        .clamp(0.0, (content_h - lanes.height() + 40.0).max(0.0));
    v.scroll_x = v.scroll_x.clamp(0.0, 1000.0 * v.bar_w);

    let beat_px = v.bar_w / bpb as f32;
    // Scroll in Play: the playhead stays in the middle and the song moves; else Catch: page
    // the view when the playhead leaves it.
    if let Some(x) = crate::arrange_zoom::scroll_in_play(p, v, beat_px) {
        v.scroll_x = x;
    } else if v.catch && p.playing {
        let px = p.position_beats as f32 * beat_px - v.scroll_x;
        if px > lanes.width() - 30.0 || px < 0.0 {
            v.scroll_x = (p.position_beats as f32 * beat_px - lanes.width() * 0.1).max(0.0);
        }
    }
    let (scroll_x, scroll_y) = (v.scroll_x, v.scroll_y);
    let x_of = |beat: f64| lanes.left() + beat as f32 * beat_px - scroll_x;
    let beat_of = |x: f32| ((x - lanes.left() + scroll_x) / beat_px).max(0.0) as f64;

    // ---- lanes background + grid
    let g = ui.painter_at(lanes);
    g.rect_filled(lanes, 0.0, LANE_BG);
    let bars = p.bars_in(beat_of(lanes.left()), beat_of(lanes.right()));
    for (i, &(top, row_h)) in rows.iter().enumerate() {
        if row_h <= 0.0 {
            continue;
        }
        let y = lanes.top() + top - scroll_y;
        let row = Rect::from_min_size(pos2(lanes.left(), y), vec2(lanes.width(), row_h));
        if v.track_selected(p, i) {
            g.rect_filled(row, 0.0, LANE_SEL);
        }
        if p.tracks[i].kind == TrackKind::Folder {
            g.rect_filled(row, 0.0, Color32::from_white_alpha(10));
        }
        // A stack's lane shows a band over the span its tracks use, named at both ends.
        if p.tracks[i].is_stack_parent() {
            if let Some((from, to)) = stack_band(p, i) {
                let band = Rect::from_min_max(
                    pos2(x_of(from), row.top() + 3.0),
                    pos2(x_of(to), row.bottom() - 3.0),
                )
                .intersect(lanes);
                if band.width() > 0.0 && band.height() > 0.0 {
                    g.rect_filled(band, 3.0, STACK_BAND);
                    let name = p.tracks[i].name.as_str();
                    g.text(
                        pos2(band.left() + 5.0, band.center().y),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                    g.text(
                        pos2(band.right() - 5.0, band.center().y),
                        Align2::RIGHT_CENTER,
                        name,
                        FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                }
            }
        }
        g.line_segment(
            [row.left_bottom(), row.right_bottom()],
            Stroke::new(1.0, Color32::from_gray(0x22)),
        );
    }
    for bar in &bars {
        let x = x_of(bar.start);
        g.line_segment(
            [pos2(x, lanes.top()), pos2(x, lanes.bottom())],
            Stroke::new(1.0, Color32::from_gray(0x38)),
        );
        if beat_px * bar.unit as f32 >= 14.0 {
            for k in 1..bar.num {
                let xb = x_of(bar.start + k as f64 * bar.unit);
                g.line_segment(
                    [pos2(xb, lanes.top()), pos2(xb, lanes.bottom())],
                    Stroke::new(1.0, Color32::from_gray(0x30)),
                );
            }
        }
    }

    // Click on an empty lane: select that track, deselect regions.
    // Drags on empty lanes are for the Pencil; regions are drawn over the lanes and take their
    // own drags.
    let lane_resp = ui.interact(lanes, id("lanes"), Sense::click_and_drag());
    let mut picked: Option<CtxAction> = None;
    // The tool the pointer has: the Command-click tool while Command is held, and while a
    // button is down the one it went down with.
    // Control-Option held: the Zoom tool, whatever tool is chosen (Logic).
    let (command, mods) = ui.input(|i| (i.modifiers.command, i.modifiers));
    let pointer_tool = crate::arrange_zoom::pressed_tool(mods).unwrap_or(v.tool_for(command));
    if v.held_tool.is_none() && ui.input(|i| i.pointer.any_pressed()) {
        v.held_tool = Some(pointer_tool);
    }
    let tool = v.held_tool.unwrap_or(pointer_tool);
    // The Zoom tool: a rectangle to zoom to, or a click to zoom in or out (`arrange_zoom`).
    let zoom_band = crate::arrange_zoom::zoom_tool(ui, p, s, lanes, tool);
    let v = &mut s.tracks;
    if lane_resp.clicked() || context_click(&lane_resp) {
        if let Some(pt) = lane_resp.interact_pointer_pos() {
            if let Some(row) = row_at(&rows, pt.y - lanes.top() + scroll_y) {
                p.selected_track = row;
                sync_bottom_editor_to_track(p, &mut s.bottom, &mut s.score_tab, row);
                if context_click(&lane_resp) {
                    v.lane_menu = Some((
                        row,
                        p.beats_to_bar(beat_of(pt.x)).floor().max(0.0) as f32 + 1.0,
                    ));
                }
            }
        }
        if tool != TrackTool::Marquee {
            v.clear_selection();
        }
    }
    // Double-click on an empty stretch: creates a 4-bar MIDI region (or Drummer region) and opens it
    if lane_resp.double_clicked() {
        if let Some(pt) = lane_resp.interact_pointer_pos() {
            if let Some(row) = row_at(&rows, pt.y - lanes.top() + scroll_y) {
                let bar = p.beats_to_bar(beat_of(pt.x)).floor() as f32 + 1.0;
                if p.tracks[row].kind == TrackKind::Drummer {
                    if let Some(ri) = p.add_generated_region(row, true, bar) {
                        p.selected_track = row;
                        v.select_only(row, ri);
                        s.bottom = Bottom::SessionPlayer;
                    }
                } else if p.tracks[row].kind.has_regions() && p.tracks[row].kind != TrackKind::Audio
                {
                    if let Some(ri) = p.add_empty_region(row, bar) {
                        p.selected_track = row;
                        v.select_only(row, ri);
                        s.bottom = Bottom::PianoRoll;
                    }
                }
            }
        }
    }
    context_menu(&lane_resp, |ui| {
        if let Some((row, bar)) = v.lane_menu {
            if let Some(track) = p.tracks.get(row) {
                if track.kind.has_regions() && track.kind != TrackKind::Audio {
                    if ui.button("Create MIDI Region").clicked() {
                        picked = Some(CtxAction::CreateMidiRegion { track: row, bar });
                        ui.close();
                    }
                    if track.kind == TrackKind::Instrument
                        && ui.button("Create Pattern Region").clicked()
                    {
                        picked = Some(CtxAction::CreatePatternRegion { track: row, bar });
                        ui.close();
                    }
                    if track.kind == TrackKind::Drummer
                        && ui.button("Create Drummer Region").clicked()
                    {
                        picked = Some(CtxAction::CreateDrummerRegion { track: row, bar });
                        ui.close();
                    }
                    ui.separator();
                }
            }
        }
        new_track_items(ui, &mut picked);
        ui.separator();
        if ui.button(tr("Zoom to Fit")).clicked() {
            picked = Some(CtxAction::ZoomToFit);
        }
    });

    // ---- regions
    let unit = p.beat_unit_at(beat_of(lanes.left()));
    let step = v.grid.step(v.snap, beat_px, bpb, unit);
    // Snapping to bars follows the signature, so it goes to the nearest bar line.
    let snap_bars = v.grid.snaps_to_bars(v.snap, beat_px, unit);
    let sig_owned = p.sigs().to_owned();
    let grid_on = v.grid.on;
    let snap_pos = move |b: f64| -> f64 {
        if !grid_on {
            b
        } else if snap_bars {
            sig_owned.as_ref().nearest_bar_line(b)
        } else {
            snap_to(b, step)
        }
    };
    let mut moved = false;
    let mut tool_act: Option<ToolAct> = None;
    let mut solo_press: Option<(usize, usize, f64)> = None;
    let mut cut_preview: Option<(f32, Rect)> = None;
    let mut mq = MarqueeInput::default();
    if tool == TrackTool::Marquee {
        mq.read(ui, &lane_resp);
    } else {
        mq.other |= lane_resp.clicked() || lane_resp.drag_started_by(egui::PointerButton::Primary);
    }
    // Pencil tool (Logic: draw a MIDI region): a click on an empty stretch of a MIDI track
    // makes a region one bar long; a drag sets its length, both snapped as the grid is.
    if tool == TrackTool::Pencil {
        let pencil_row = |pt: egui::Pos2| {
            row_at(&rows, pt.y - lanes.top() + scroll_y)
                .filter(|row| p.tracks[*row].kind.has_regions())
                .filter(|row| p.tracks[*row].kind != TrackKind::Audio)
        };
        if lane_resp.drag_started() || lane_resp.clicked() {
            if let Some(pt) = lane_resp.interact_pointer_pos() {
                if let Some(row) = pencil_row(pt) {
                    let at = snap_pos(beat_of(pt.x).max(0.0));
                    let start_bar = p.beats_to_bar(at) as f32 + 1.0;
                    let bar_len = if lane_resp.clicked() {
                        1.0
                    } else {
                        PENCIL_MIN_BARS
                    };
                    if let Some(ri) = p.add_midi_region(row, start_bar, bar_len) {
                        p.selected_track = row;
                        v.selected = Some((row, ri));
                        if lane_resp.drag_started() {
                            v.pencil_drag = Some((row, ri));
                        } else {
                            moved = true;
                        }
                    }
                }
            }
        }
        if let (Some((row, ri)), true, Some(pt)) = (
            v.pencil_drag,
            lane_resp.dragged(),
            lane_resp.interact_pointer_pos(),
        ) {
            let start = p.region_start_beats(&p.tracks[row].regions[ri]);
            let end = snap_pos(beat_of(pt.x).max(0.0));
            let len = (p.beats_to_bar(end) - p.beats_to_bar(start)) as f32;
            p.tracks[row].regions[ri].len_bars = len.max(PENCIL_MIN_BARS);
            moved = true;
        }
        if lane_resp.drag_stopped() {
            v.pencil_drag = None;
            moved = true;
        }
    }
    for (ti, &(top, row_h)) in rows.iter().enumerate() {
        if row_h <= 0.0 {
            continue;
        }
        let y = lanes.top() + top - scroll_y;
        if y + row_h < lanes.top() || y > lanes.bottom() {
            continue;
        }
        let base_h = row_h - take_rows_height(&p.tracks[ti]);
        for ri in 0..p.tracks[ti].regions.len() {
            let orig_start = p.region_start_beats(&p.tracks[ti].regions[ri]);
            let orig_len = p.region_len_beats(&p.tracks[ti].regions[ri]);
            let (start_beat, len_beats, content_start, orig_len_b) =
                if let Some((dt, dr, live_end, _)) = v.edge_drag {
                    if dt == ti && dr == ri {
                        (
                            orig_start,
                            (live_end - orig_start).max(1.0 / 64.0),
                            orig_start,
                            orig_len,
                        )
                    } else {
                        (orig_start, orig_len, orig_start, orig_len)
                    }
                } else if let Some((dt, dr, live_start)) = v.start_drag {
                    if dt == ti && dr == ri {
                        let end = orig_start + orig_len;
                        let s = live_start.min(end - 1.0 / 64.0);
                        (s, end - s, orig_start, orig_len)
                    } else {
                        (orig_start, orig_len, orig_start, orig_len)
                    }
                } else {
                    (orig_start, orig_len, orig_start, orig_len)
                };
            let full = Rect::from_min_max(
                pos2(x_of(start_beat), y + 1.0),
                pos2(x_of(start_beat + len_beats), y + base_h - 1.0),
            );
            let content_origin_x = x_of(content_start);
            let orig_full_w = (orig_len_b as f32 * beat_px).max(1.0);
            let rr = full.intersect(lanes);
            if rr.width() <= 0.0 || rr.height() <= 0.0 {
                continue;
            }
            let resp = ui.interact(rr, id(("region", ti, ri)), Sense::click_and_drag());
            if tool == TrackTool::Marquee {
                mq.read(ui, &resp);
            } else {
                mq.other |= resp.clicked() || resp.drag_started_by(egui::PointerButton::Primary);
            }
            // Shift or Command click adds to the selection (or takes the region out of it). A
            // plain click or drag on a region that is selected keeps the whole selection.
            // The Marquee tool's clicks and drags are the marquee's, and the Solo tool's hold
            // selects nothing (Logic).
            let selects = tool != TrackTool::Marquee;
            if selects && resp.clicked() && ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                v.toggle(ti, ri);
                p.selected_track = ti;
                sync_bottom_editor_to_track(p, &mut s.bottom, &mut s.score_tab, ti);
            } else if (selects && resp.clicked())
                || (resp.drag_started_by(egui::PointerButton::Primary)
                    && !matches!(tool, TrackTool::Marquee | TrackTool::Solo))
                || context_click(&resp)
            {
                if !v.is_selected(ti, ri) {
                    v.select_only(ti, ri);
                } else if resp.clicked() {
                    v.make_primary(ti, ri);
                }
                p.selected_track = ti;
                sync_bottom_editor_to_track(p, &mut s.bottom, &mut s.score_tab, ti);
            }
            if resp.double_clicked() {
                if p.tracks[ti].kind != TrackKind::Audio {
                    let is_session = matches!(
                        p.tracks[ti].regions[ri]
                            .generated
                            .as_ref()
                            .map(|g| &g.source),
                        Some(
                            crate::model::Generator::Session(_)
                                | crate::model::Generator::Drummer(_)
                        )
                    );
                    s.bottom = if is_session {
                        Bottom::SessionPlayer
                    } else {
                        Bottom::PianoRoll
                    };
                } else if v.tool != TrackTool::Fade {
                    if let Some(pt) = resp.interact_pointer_pos() {
                        if let Some(message) = flex_double_click(p, ti, ri, full, pt.x) {
                            s.status = Some(tr(message).into());
                            moved = true;
                        } else {
                            crate::pitch_editor::open_in(&mut s.pitch_ed, &mut s.bottom, ti, ri);
                        }
                    } else {
                        crate::pitch_editor::open_in(&mut s.pitch_ed, &mut s.bottom, ti, ri);
                    }
                }
            }
            let click_tool = !tool.moves_regions()
                && !matches!(
                    tool,
                    TrackTool::Fade | TrackTool::Gain | TrackTool::Marquee | TrackTool::Solo
                );
            // The Scissors show where they would cut, on the grid.
            if tool == TrackTool::Scissors {
                if let Some(pt) = resp.hover_pos() {
                    let at = snap_pos(beat_of(pt.x));
                    if at > start_beat + 1e-6 && at < start_beat + len_beats - 1e-6 {
                        cut_preview = Some((x_of(at), full));
                    }
                }
            }
            if tool == TrackTool::Solo
                && v.solo_hold.is_none()
                && resp.is_pointer_button_down_on()
                && ui.input(|i| i.pointer.primary_down())
            {
                let from = ui.input(|i| i.pointer.press_origin());
                if let Some(pt) = from.or(resp.interact_pointer_pos()) {
                    solo_press = Some((ti, ri, beat_of(pt.x)));
                }
            }
            if tool.moves_regions() && (full.right() - rr.right()).abs() < 0.5 {
                let grip = Rect::from_min_max(pos2(rr.right() - 8.0, full.center().y), rr.max);
                let er = ui.interact(grip, id(("region_end", ti, ri)), Sense::drag());
                if er.hovered() || er.dragged_by(egui::PointerButton::Primary) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if let (true, Some(pt)) = (
                    er.dragged_by(egui::PointerButton::Primary),
                    er.interact_pointer_pos(),
                ) {
                    let end = snap_pos(beat_of(pt.x)).max(orig_start + 1.0 / 64.0);
                    let stretch = ui.input(|i| i.modifiers.alt);
                    v.edge_drag = Some((ti, ri, end, stretch));
                }
                if er.drag_stopped() {
                    if let Some((t, r, end, stretch)) = v.edge_drag.take() {
                        tool_act = Some(ToolAct::End(t, r, end, stretch));
                    }
                }
            }
            if tool.moves_regions() && (full.left() - rr.left()).abs() < 0.5 {
                let grip = Rect::from_min_max(
                    pos2(rr.left(), full.top()),
                    pos2(rr.left() + 8.0, rr.bottom()),
                );
                let sr = ui.interact(grip, id(("region_start", ti, ri)), Sense::drag());
                if sr.hovered() || sr.dragged_by(egui::PointerButton::Primary) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                }
                if let (true, Some(pt)) = (
                    sr.dragged_by(egui::PointerButton::Primary),
                    sr.interact_pointer_pos(),
                ) {
                    let at = snap_pos(beat_of(pt.x)).min(orig_start + orig_len - 1.0 / 64.0);
                    v.start_drag = Some((ti, ri, at));
                }
                if sr.drag_stopped() {
                    if let Some((t, r, at)) = v.start_drag.take() {
                        tool_act = Some(ToolAct::Start(t, r, at));
                    }
                }
            }
            if click_tool {
                if resp.clicked() {
                    let at = resp.interact_pointer_pos().map(|pt| beat_of(pt.x));
                    let alt = ui.input(|i| i.modifiers.alt);
                    match tool {
                        TrackTool::Eraser => tool_act = Some(ToolAct::Erase(ti, ri)),
                        TrackTool::Scissors => {
                            if let Some(b) = at {
                                tool_act = Some(ToolAct::Split(ti, ri, snap_pos(b), alt));
                            }
                        }
                        TrackTool::Join => tool_act = Some(ToolAct::Join(ti, ri)),
                        TrackTool::Mute => tool_act = Some(ToolAct::Mute(ti, ri)),
                        _ => {}
                    }
                }
            }
            let fade_tool = v.tool == TrackTool::Fade && p.tracks[ti].kind == TrackKind::Audio;
            // A Flex marker under the pointer: grabbing it stretches the sound; it does not
            // move the region (Logic's Flex Time).
            let flex_hover = if fade_tool {
                None
            } else {
                ui.input(|i| i.pointer.hover_pos())
                    .and_then(|pt| flex_marker_under(p, ti, ri, full, pt.x))
            };
            if flex_hover.is_some() && resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            let flex_hit = if fade_tool {
                None
            } else {
                ui.input(|i| i.pointer.press_origin())
                    .or_else(|| resp.interact_pointer_pos())
                    .and_then(|pt| flex_marker_under(p, ti, ri, full, pt.x))
            };
            if let (Some((marker, secs)), true) =
                (flex_hit, resp.drag_started_by(egui::PointerButton::Primary))
            {
                v.flex_drag = Some((ti, ri, marker, secs));
                s.flex.dragging = true;
            }
            let flex_own = v
                .flex_drag
                .is_some_and(|(dt, dr, _, _)| (dt, dr) == (ti, ri));
            if flex_own {
                // The dragged marker follows the pointer; the sound is made on release.
                if let (true, Some((_, _, marker, secs)), Some(pt)) = (
                    resp.dragged_by(egui::PointerButton::Primary),
                    v.flex_drag,
                    resp.interact_pointer_pos(),
                ) {
                    if let Some(f) = p.tracks[ti].regions[ri]
                        .audio
                        .as_mut()
                        .and_then(|a| a.flex.as_mut())
                    {
                        let dst = f64::from((pt.x - full.left()) / full.width()) * secs;
                        crate::flex_edit::move_marker(&mut f.markers, marker, dst);
                    }
                }
                if resp.drag_stopped() {
                    v.flex_drag = None;
                    s.flex.dragging = false;
                    moved = true;
                }
            } else if fade_tool {
                let shape_keys = ui.input(|i| i.modifiers.ctrl && i.modifiers.shift);
                if crate::fade_curve::on_region(ui, p, v, &resp, (ti, ri), shape_keys, &beat_of) {
                    moved = true;
                } else if resp.drag_started_by(egui::PointerButton::Primary) && !shape_keys {
                    let from = ui.input(|i| i.pointer.press_origin());
                    if let Some(pt) = from.or(resp.interact_pointer_pos()) {
                        v.fade_drag =
                            crate::fade_tool::begin(p, &v.selection(), (ti, ri), beat_of(pt.x));
                    }
                }
                if let (true, Some(d), Some(pt)) = (
                    resp.dragged_by(egui::PointerButton::Primary),
                    v.fade_drag.as_ref(),
                    resp.interact_pointer_pos(),
                ) {
                    crate::fade_tool::drag_to(p, d, beat_of(pt.x));
                }
                if resp.drag_stopped() {
                    v.fade_drag = None;
                    moved = true;
                }
                if resp.clicked() && ui.input(|i| i.modifiers.alt) {
                    if let Some(pt) = resp.interact_pointer_pos() {
                        moved |= crate::fade_tool::remove_at(p, (ti, ri), beat_of(pt.x));
                    }
                }
            } else if tool.moves_regions() {
                let press_pt = ui
                    .input(|i| i.pointer.press_origin())
                    .or_else(|| resp.interact_pointer_pos());
                let on_edge_handle = press_pt.map_or(false, |pt| {
                    ((full.left() - rr.left()).abs() < 0.5 && pt.x <= rr.left() + 8.0)
                        || ((full.right() - rr.right()).abs() < 0.5 && pt.x >= rr.right() - 8.0)
                });
                if resp.drag_started_by(egui::PointerButton::Primary)
                    && !on_edge_handle
                    && v.start_drag.is_none()
                    && v.edge_drag.is_none()
                {
                    if let Some(pt) = resp.interact_pointer_pos() {
                        v.grab_beats = beat_of(pt.x) - start_beat;
                    }
                    // Every selected region moves with the one being dragged: each remembers
                    // where it started.
                    let mut group: Vec<(usize, usize, f64)> = v
                        .selection()
                        .into_iter()
                        .filter_map(|(gt, gr)| {
                            let reg = p.tracks.get(gt)?.regions.get(gr)?;
                            Some((gt, gr, p.region_start_beats(reg)))
                        })
                        .collect();
                    if !group.iter().any(|g| (g.0, g.1) == (ti, ri)) {
                        group.push((ti, ri, start_beat));
                    }
                    v.group_drag = group;
                }
                if resp.dragged_by(egui::PointerButton::Primary)
                    && !v.group_drag.is_empty()
                    && v.start_drag.is_none()
                    && v.edge_drag.is_none()
                {
                    if let Some(&(gt, gr, orig_beats)) = v.group_drag.first() {
                        if let Some(reg) = p.tracks.get(gt).and_then(|t| t.regions.get(gr)) {
                            let payload = crate::track_drop_slice::DraggedRegionPayload {
                                track_index: gt,
                                region_index: gr,
                                region_name: reg.name.clone(),
                                is_midi: reg.audio.is_none(),
                                original_start_beats: orig_beats,
                            };
                            egui::DragAndDrop::set_payload(ui.ctx(), payload.clone());
                            s.dragged_region = Some(payload);
                        }
                    }
                    let copy = ui.input(|i| i.modifiers.alt);
                    if let Some(pt) = ui
                        .ctx()
                        .input(|i| i.pointer.latest_pos())
                        .or_else(|| resp.interact_pointer_pos())
                    {
                        if lanes.contains(pt) {
                            let raw = (beat_of(pt.x) - v.grab_beats).max(0.0);
                            let grabbed = v
                                .group_drag
                                .iter()
                                .find(|g| (g.0, g.1) == (ti, ri))
                                .map_or(start_beat, |g| g.2);
                            let nb = v.grid.region_start(raw, grabbed, &snap_pos);
                            let delta = nb - grabbed;
                            let now_y = pt.y - lanes.top() + scroll_y;
                            let row_under = row_at(&rows, now_y).unwrap_or(ti);
                            let down = row_under as isize - ti as isize;
                            v.drag_by = (delta, down);
                            if !copy && down == 0 {
                                let moves: Vec<(usize, usize, f32)> = v
                                    .group_drag
                                    .iter()
                                    .map(|&(gt, gr, s0)| {
                                        (gt, gr, crate::region_edit::landing_bar(p, s0, delta))
                                    })
                                    .collect();
                                for (gt, gr, bar) in moves {
                                    if let Some(reg) = p.tracks[gt].regions.get_mut(gr) {
                                        reg.start_bar = bar;
                                    }
                                }
                            }
                        } else {
                            // Pointer is outside the timeline: suspend snapping and restore original positions.
                            v.drag_by = (0.0, 0);
                            let restores: Vec<(usize, usize, f32)> = v
                                .group_drag
                                .iter()
                                .map(|&(gt, gr, s0)| {
                                    (gt, gr, crate::region_edit::landing_bar(p, s0, 0.0))
                                })
                                .collect();
                            for (gt, gr, bar) in restores {
                                if let Some(reg) = p.tracks[gt].regions.get_mut(gr) {
                                    reg.start_bar = bar;
                                }
                            }
                        }
                    }
                }
                if resp.drag_stopped() && !v.group_drag.is_empty() {
                    let release_pos = ui
                        .ctx()
                        .input(|i| i.pointer.latest_pos().or(i.pointer.interact_pos()));
                    let drop_target = release_pos.and_then(|pos| {
                        crate::track_drop_slice::cross_panel_drop_target(
                            pos,
                            Some(headers),
                            s.library_panel_rect,
                        )
                    });
                    if drop_target.is_some() {
                        // Cross-panel drop: restore timeline positions and let App::ui's
                        // single end-of-frame evaluator consume `s.dragged_region`.
                        let restores: Vec<(usize, usize, f32)> = v
                            .group_drag
                            .iter()
                            .map(|&(gt, gr, s0)| {
                                (gt, gr, crate::region_edit::landing_bar(p, s0, 0.0))
                            })
                            .collect();
                        for (gt, gr, bar) in restores {
                            if let Some(reg) = p.tracks[gt].regions.get_mut(gr) {
                                reg.start_bar = bar;
                            }
                        }
                        v.group_drag.clear();
                        v.drag_by = (0.0, 0);
                    } else {
                        let copy = ui.input(|i| i.modifiers.alt);
                        let group = std::mem::take(&mut v.group_drag);
                        let (delta, down) = std::mem::take(&mut v.drag_by);
                        tool_act = Some(ToolAct::Drop(group, delta, down, copy));
                        s.dragged_region = None;
                        egui::DragAndDrop::clear_payload(ui.ctx());
                    }
                }
            }
            let (is_audio, muted) = (
                p.tracks[ti].kind == TrackKind::Audio,
                p.tracks[ti].regions[ri].params.mute,
            );
            let folder_open = p.tracks[ti].regions[ri].folder.as_ref().map(|f| f.open);
            let generated = p.tracks[ti].regions[ri].generated.is_some();
            let shown = v.shot_menu == Some((ti, Some(ri)));
            crate::widgets::context_menu_shown(&resp, shown, |ui| {
                region_items(
                    ui,
                    is_audio,
                    muted,
                    folder_open,
                    generated,
                    ti,
                    ri,
                    &mut picked,
                );
            });
            let selected = v.is_selected(ti, ri);
            if folder_open.is_some() {
                // The triangle opens and closes the folder; the rows of an opened one are
                // swiped to pick which take plays where.
                let tri = ui.interact(
                    folder_toggle_rect(full).intersect(lanes),
                    id(("take_toggle", ti, ri)),
                    Sense::click(),
                );
                if tri.clicked() {
                    picked = Some(CtxAction::ToggleTakeFolder(ti, ri));
                    v.select_only(ti, ri);
                }
                if folder_open == Some(true) {
                    let n = p.tracks[ti].regions[ri]
                        .folder
                        .as_ref()
                        .map_or(0, |f| f.takes.len());
                    for take in 0..n {
                        let row = take_row_rect(full, n, take).intersect(lanes);
                        if row.width() <= 0.0 || row.height() <= 0.0 {
                            continue;
                        }
                        let resp = ui.interact(
                            row,
                            id(("take_row", ti, ri, take)),
                            Sense::click_and_drag(),
                        );
                        let Some(region) = p.tracks[ti].regions.get_mut(ri) else {
                            continue;
                        };
                        let Some(folder) = region.folder.as_mut() else {
                            continue;
                        };
                        let grab =
                            |pt: egui::Pos2| snap_pos((beat_of(pt.x) - start_beat).max(0.0)) as f32;
                        if resp.drag_started_by(egui::PointerButton::Primary) {
                            if let Some(pt) = resp.interact_pointer_pos() {
                                v.swipe = Some((folder.comp.clone(), grab(pt)));
                            }
                        }
                        if resp.dragged_by(egui::PointerButton::Primary) {
                            if let (Some((orig, anchor)), Some(pt)) =
                                (v.swipe.as_ref(), resp.interact_pointer_pos())
                            {
                                folder.comp = orig.clone();
                                folder.swipe(take, *anchor, grab(pt));
                                folder.apply(&mut region.notes, &mut region.controls);
                            }
                        }
                        if resp.drag_stopped() {
                            v.swipe = None;
                            moved = true;
                        }
                        if resp.clicked() {
                            // A click picks the whole take.
                            let len = folder.len;
                            folder.swipe(take, 0.0, len);
                            folder.apply(&mut region.notes, &mut region.controls);
                            moved = true;
                        }
                    }
                }
                if let Some(folder) = p.tracks[ti].regions[ri].folder.as_ref() {
                    draw_take_folder(
                        &g,
                        full,
                        &p.tracks[ti],
                        &p.tracks[ti].regions[ri],
                        folder,
                        beat_px,
                        selected,
                        silenced_by_solo(p, ti),
                    );
                    continue;
                }
            }
            let tr = &p.tracks[ti];
            draw_region(
                &g,
                rr,
                full,
                content_origin_x,
                orig_full_w,
                tr,
                &tr.regions[ri],
                p.effective_fades(ti, ri),
                beat_px,
                selected,
                rec_targets.contains(&(ti, ri)),
                silenced_by_solo(p, ti),
                ti as u32 * 77 + ri as u32 * 13 + 3,
                v.waveform_display,
            );
        }
    }
    // Ghost preview of Alt/Option copy or cross-track move
    let copying = ui.input(|i| i.modifiers.alt);
    if !v.group_drag.is_empty() && (copying || v.drag_by.1 != 0) {
        let (delta, down) = v.drag_by;
        for &(gt, gr, s0) in &v.group_drag {
            let Some(region) = p.tracks.get(gt).and_then(|t| t.regions.get(gr)) else {
                continue;
            };
            let to = gt.saturating_add_signed(down);
            let Some(&(top, row_h)) = rows.get(to) else {
                continue;
            };
            let y = lanes.top() + top - scroll_y;
            let base_h = row_h - take_rows_height(&p.tracks[to]);
            let start = (s0 + delta).max(0.0);
            let full = Rect::from_min_max(
                pos2(x_of(start), y + 1.0),
                pos2(x_of(start + p.region_len_beats(region)), y + base_h - 1.0),
            );
            let paint = full.intersect(lanes);
            if paint.width() > 0.0 && paint.height() > 0.0 {
                draw_region(
                    &g,
                    paint,
                    full,
                    full.left(),
                    full.width(),
                    &p.tracks[to],
                    region,
                    p.effective_fades(gt, gr),
                    beat_px,
                    true,
                    false,
                    false,
                    gt as u32 * 77 + gr as u32 * 13 + 3,
                    v.waveform_display,
                );
            }
        }
    }
    // Active edge drag boundary highlight
    if let Some((dt, _dr, live_end, _)) = v.edge_drag {
        let x = x_of(live_end);
        if let Some(&(top, row_h)) = rows.get(dt) {
            let y = lanes.top() + top - scroll_y;
            let base_h = row_h - take_rows_height(&p.tracks[dt]);
            let bar_rect =
                Rect::from_min_max(pos2(x - 1.5, y + 1.0), pos2(x + 1.5, y + base_h - 1.0));
            if bar_rect.intersects(lanes) {
                let p = ui.painter_at(lanes);
                p.rect_filled(bar_rect.intersect(lanes), 1.0, crate::theme::ACCENT_BLUE);
                p.line_segment(
                    [pos2(x, y + 1.0), pos2(x, y + base_h - 1.0)],
                    Stroke::new(1.0, Color32::WHITE),
                );
            }
        }
    }
    if let Some((dt, _dr, live_start)) = v.start_drag {
        let x = x_of(live_start);
        if let Some(&(top, row_h)) = rows.get(dt) {
            let y = lanes.top() + top - scroll_y;
            let base_h = row_h - take_rows_height(&p.tracks[dt]);
            let bar_rect =
                Rect::from_min_max(pos2(x - 1.5, y + 1.0), pos2(x + 1.5, y + base_h - 1.0));
            if bar_rect.intersects(lanes) {
                let p = ui.painter_at(lanes);
                p.rect_filled(bar_rect.intersect(lanes), 1.0, crate::theme::ACCENT_BLUE);
                p.line_segment(
                    [pos2(x, y + 1.0), pos2(x, y + base_h - 1.0)],
                    Stroke::new(1.0, Color32::WHITE),
                );
            }
        }
    }
    // Drag audio/Drummer/MIDI region or OS file onto Track Header gutter -> Create New Track using:
    let ptr_pos = ui.ctx().input(|i| {
        i.pointer
            .latest_pos()
            .or(i.pointer.hover_pos())
            .or(i.pointer.interact_pos())
    });
    s.tracks_timeline_rect = Some(lanes);
    s.tracks_drop_beat = ptr_pos.map(|pt| beat_of(pt.x).max(0.0));
    let os_hover = ui.input(|i| !i.raw.hovered_files.is_empty());
    let has_region_payload = !v.group_drag.is_empty() || s.dragged_region.is_some();
    if let Some(pos) = ptr_pos {
        if headers.contains(pos) && (os_hover || has_region_payload) {
            let hl = ui.painter_at(headers);
            hl.rect_filled(
                headers,
                4.0,
                Color32::from_rgba_unmultiplied(0x12, 0x69, 0xcc, 28),
            );
            hl.rect_stroke(
                headers,
                4.0,
                Stroke::new(2.0, Color32::from_rgb(0x12, 0x69, 0xcc)),
                egui::StrokeKind::Inside,
            );
            hl.text(
                pos2(headers.center().x, headers.top() + 14.0),
                Align2::CENTER_CENTER,
                tr("Drop Audio to Create Track / Drum Kit"),
                FontId::proportional(11.0),
                Color32::WHITE,
            );
        } else if lanes.contains(pos) && s.dragged_user_loop.is_some() {
            let hl = ui.painter_at(lanes);
            hl.rect_filled(
                lanes,
                4.0,
                Color32::from_rgba_unmultiplied(0x51, 0xee, 0xfc, 18),
            );
            hl.rect_stroke(
                lanes,
                4.0,
                Stroke::new(2.0, Color32::from_rgb(0x51, 0xee, 0xfc)),
                egui::StrokeKind::Inside,
            );
        }
    }

    match tool_act {
        Some(ToolAct::Drop(group, delta, down, copy)) => {
            let landed =
                crate::drag_modes::drop(p, &s.auto, &group, delta, down, copy, v.drag_mode);
            if copy || down != 0 {
                v.select_many(&landed);
                if let Some(&(t, _)) = landed.first() {
                    p.selected_track = t;
                }
            }
        }
        Some(ToolAct::Erase(ti, ri)) => crate::shuffle_edit::erase(p, v, (ti, ri)),
        Some(ToolAct::Split(ti, ri, at, equal)) => {
            if !crate::region_edit::scissors(p, v, (ti, ri), at, equal) {
                s.status = Some(tr("Cannot cut this region here").into());
            }
        }
        Some(ToolAct::Mute(ti, ri)) => {
            crate::region_edit::toggle_mute(p, v, Some((ti, ri)));
        }
        Some(ToolAct::Start(ti, ri, at)) => {
            crate::shuffle_edit::resize_starts(p, v, (ti, ri), at);
        }
        Some(ToolAct::Join(ti, ri)) => {
            if p.join_regions(ti, ri) {
                v.select_only(ti, ri);
            } else {
                s.status = Some(tr("These regions cannot be joined").into());
            }
        }
        Some(ToolAct::End(ti, ri, end, stretch))
            if !crate::shuffle_edit::resize_ends(p, v, (ti, ri), end, stretch) =>
        {
            s.status = Some(tr("This region cannot be stretched").into());
        }
        Some(ToolAct::End(..)) | None => {}
    }
    // Marquee tool drag, click, and rendering
    let adding = ui.input(|i| i.modifiers.shift);
    let cell = |pt: Pos2| {
        let row = row_at(&rows, pt.y - lanes.top() + scroll_y).unwrap_or_else(|| {
            if pt.y < lanes.top() {
                0
            } else {
                rows.len().saturating_sub(1)
            }
        });
        (snap_pos(beat_of(pt.x)), row)
    };
    if let Some(pt) = mq.started {
        v.range.begin(cell(pt), adding);
    }
    if let Some(pt) = mq.at {
        v.range.drag_to(cell(pt));
    }
    if mq.stopped {
        v.range.end();
    }
    if let Some(pt) = mq.clicked {
        if adding && v.range.active().is_some() {
            v.range.begin(cell(pt), true);
            v.range.end();
        } else {
            v.range.clear();
        }
    }
    if let Some((ti, ri, at)) = solo_press {
        if v.solo_hold.is_none() {
            v.solo_hold = crate::region_solo::begin(p, (ti, ri), at);
        }
    }
    if mq.other {
        v.range.clear();
    }
    if let Some(r) = v
        .range
        .range
        .filter(|r| !r.is_empty() || v.range.anchor.is_some())
    {
        draw_marquee(&g, &rows, r, &x_of, lanes, scroll_y);
    }
    // The Zoom tool's rectangle, over the regions it will fill the lanes with.
    if let Some(r) = zoom_band {
        g.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 24));
        g.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, Color32::from_gray(0xe0)),
            egui::StrokeKind::Inside,
        );
    }
    if moved {
        p.song_rev += 1;
    }

    if marker_h > 0.0 {
        let top = ruler_y + RULER_H;
        let picked_marker = crate::marker_track::show(
            ui,
            p,
            Rect::from_min_max(
                pos2(area.left(), top),
                pos2(headers.right(), top + marker_h),
            ),
            Rect::from_min_max(pos2(lanes.left(), top), pos2(area.right(), top + marker_h)),
            x_of,
            beat_of,
            &snap_pos,
            &mut v.selected_marker,
        );
        if picked_marker {
            v.clear_selection();
        }
    }
    if signature_h > 0.0 {
        let top = ruler_y + RULER_H + marker_h;
        crate::signature_track::show(
            ui,
            p,
            Rect::from_min_max(
                pos2(area.left(), top),
                pos2(headers.right(), top + signature_h),
            ),
            Rect::from_min_max(
                pos2(lanes.left(), top),
                pos2(area.right(), top + signature_h),
            ),
            x_of,
            beat_of,
        );
    }
    if tempo_h > 0.0 {
        let top = ruler_y + RULER_H + marker_h + signature_h;
        crate::tempo_track::show(
            ui,
            p,
            Rect::from_min_max(pos2(area.left(), top), pos2(headers.right(), top + tempo_h)),
            Rect::from_min_max(pos2(lanes.left(), top), pos2(area.right(), top + tempo_h)),
            x_of,
            beat_of,
            &snap_pos,
        );
    }
    if chord_h > 0.0 {
        let top = ruler_y + RULER_H + marker_h + signature_h + tempo_h;
        let chord_hdr =
            Rect::from_min_max(pos2(area.left(), top), pos2(headers.right(), top + chord_h));
        let chord_lane =
            Rect::from_min_max(pos2(lanes.left(), top), pos2(area.right(), top + chord_h));
        let gh = ui.painter_at(chord_hdr);
        gh.rect_filled(chord_hdr, 0.0, HEADER_BG);
        gh.line_segment(
            [chord_hdr.left_bottom(), chord_hdr.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        );
        gh.text(
            pos2(chord_hdr.left() + 32.0, chord_hdr.center().y),
            Align2::LEFT_CENTER,
            tr("Chords"),
            FontId::proportional(12.0),
            TEXT_DIM,
        );

        let gl = ui.painter_at(chord_lane);
        gl.rect_filled(chord_lane, 0.0, LANE_BG);
        gl.line_segment(
            [chord_lane.left_bottom(), chord_lane.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        );

        for chord in &p.chords {
            let x0 = x_of(chord.start_beat);
            let x1 = x_of(chord.start_beat + chord.len_beats);
            if x1 < chord_lane.left() || x0 > chord_lane.right() {
                continue;
            }
            let block_r = Rect::from_min_max(
                pos2(x0.max(chord_lane.left()) + 1.0, chord_lane.top() + 2.0),
                pos2(x1.min(chord_lane.right()) - 1.0, chord_lane.bottom() - 2.0),
            );
            if block_r.width() > 12.0 {
                gl.rect_filled(block_r, 4.0, Color32::from_rgb(0x26, 0x26, 0x2a));
                gl.rect_stroke(
                    block_r,
                    4.0,
                    Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
                    egui::StrokeKind::Inside,
                );
                gl.text(
                    block_r.center(),
                    Align2::CENTER_CENTER,
                    &chord.name,
                    FontId::proportional(11.0),
                    Color32::from_rgb(0xea, 0xee, 0xf4),
                );
            }
        }
    }

    if let (Some(loops_r), Some(split_r)) = (loops_area, splitter_r) {
        let s_resp = ui.interact(split_r, id("live_loops_splitter"), Sense::click_and_drag());
        if s_resp.dragged() {
            v.live_loops_split = (v.live_loops_split + s_resp.drag_delta().x).max(150.0);
        }
        let gp = ui.painter_at(split_r);
        gp.rect_filled(split_r, 0.0, Color32::from_rgb(0x18, 0x18, 0x1a));
        gp.line_segment(
            [
                pos2(split_r.center().x, split_r.top()),
                pos2(split_r.center().x, split_r.bottom()),
            ],
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        );

        let loops_hdr = Rect::from_min_max(
            pos2(loops_r.left(), ruler_y),
            pos2(loops_r.right(), tracks_y),
        );
        let gh_l = ui.painter_at(loops_hdr);
        gh_l.rect_filled(loops_hdr, 0.0, Color32::from_rgb(0x18, 0x18, 0x1a));
        gh_l.line_segment(
            [loops_hdr.left_bottom(), loops_hdr.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        );
        let num_scenes = 4;
        let col_w = loops_hdr.width() / num_scenes as f32;
        for sc in 0..num_scenes {
            let sc_r = Rect::from_min_size(
                pos2(loops_hdr.left() + sc as f32 * col_w, loops_hdr.top()),
                vec2(col_w, loops_hdr.height()),
            );
            gh_l.line_segment(
                [sc_r.right_top(), sc_r.right_bottom()],
                Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2a, 0x2e)),
            );
            gh_l.text(
                sc_r.center(),
                Align2::CENTER_CENTER,
                format!("Scene {}", sc + 1),
                FontId::proportional(11.0),
                TEXT_DIM,
            );
        }

        let gl = ui.painter_at(loops_r);
        gl.rect_filled(loops_r, 0.0, Color32::from_rgb(0x18, 0x18, 0x1a));
        for (i, &(top, row_h)) in rows.iter().enumerate() {
            if row_h <= 0.0 {
                continue;
            }
            let y = loops_r.top() + top - scroll_y;
            let row = Rect::from_min_size(pos2(loops_r.left(), y), vec2(loops_r.width(), row_h))
                .intersect(loops_r);
            if row.height() <= 0.0 {
                continue;
            }
            gl.line_segment(
                [row.left_bottom(), row.right_bottom()],
                Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2a, 0x2e)),
            );
            for sc in 0..num_scenes {
                let cell = Rect::from_min_size(
                    pos2(loops_r.left() + sc as f32 * col_w + 3.0, y + 4.0),
                    vec2(col_w - 6.0, row_h - 8.0),
                )
                .intersect(loops_r);
                if cell.width() > 10.0 && cell.height() > 10.0 {
                    let cell_resp = ui.interact(cell, id(("ll_cell", i, sc)), Sense::click());
                    let has_cell = (i + sc) % 3 != 0;
                    let bg = if has_cell {
                        if cell_resp.hovered() {
                            Color32::from_rgb(0x30, 0x30, 0x35)
                        } else {
                            Color32::from_rgb(0x26, 0x26, 0x2a)
                        }
                    } else {
                        Color32::from_rgb(0x1e, 0x1e, 0x21)
                    };
                    gl.rect_filled(cell, 4.0, bg);
                    gl.rect_stroke(
                        cell,
                        4.0,
                        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
                        egui::StrokeKind::Inside,
                    );
                    if has_cell {
                        let c = cell.center();
                        gl.circle_stroke(
                            c,
                            7.0,
                            Stroke::new(
                                1.5,
                                if cell_resp.hovered() {
                                    Color32::from_rgb(0x2e, 0xbd, 0x6b)
                                } else {
                                    Color32::from_rgb(0x3a, 0x7b, 0xd5)
                                },
                            ),
                        );
                        let tri = vec![
                            c + vec2(-2.5, -3.5),
                            c + vec2(4.0, 0.0),
                            c + vec2(-2.5, 3.5),
                        ];
                        gl.add(Shape::convex_polygon(tri, Color32::WHITE, Stroke::NONE));
                    }
                }
            }
        }
    }

    // ---- keyboard: Delete (Command-R, Repeat, is a key command: `key_commands`)
    let keys_free = !ui.ctx().egui_wants_keyboard_input()
        && !s.piano_focus
        && !s.typing.open
        && s.track_dialog.is_none()
        && s.settings_tab.is_none();
    if keys_free && v.tool_menu.is_none() {
        if let Some(pt) = hover.filter(|pt| full.contains(*pt)) {
            if take_plain_key(ui, Key::T) {
                v.tool_menu = Some(pt);
            }
        }
    }
    if !ui.ctx().egui_wants_keyboard_input() && !s.piano_focus {
        let del = ui.input(|i| {
            !i.modifiers.command
                && !i.modifiers.ctrl
                && (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
        });
        if del {
            let tracks = p.tracks.len();
            delete_selected(p, v);
            if p.tracks.len() != tracks {
                ui.ctx().request_repaint();
                return;
            }
        }
    }

    // ---- headers
    let gh = ui.painter_at(headers);
    gh.rect_filled(headers, 0.0, Color32::from_rgb(0x18, 0x19, 0x1b));
    for (i, &(top, row_h)) in rows.iter().enumerate() {
        if row_h <= 0.0 {
            continue;
        }
        let y = headers.top() + top - scroll_y;
        if y + row_h < headers.top() || y > headers.bottom() {
            continue;
        }
        let hdr = Rect::from_min_size(pos2(headers.left(), y), vec2(headers.width(), row_h));
        let peaks = meters.tracks.get(i).copied().unwrap_or((0.0, 0.0));
        let silenced = silenced_by_solo(p, i);
        let state = HeaderState {
            peaks,
            silenced,
            picked: v.track_extra.contains(&i),
            hide_view: v.show_hidden,
        };
        let resp = header(ui, &gh, p, i, hdr, state);
        // A click on a track header selects the track, Command-click adds it or takes it out,
        // Shift-click selects the tracks from the last one clicked; the regions of every
        // selected track are selected (Logic: Select Regions on Track Selection).
        if crate::widgets::context_click(&resp) {
            crate::track_selection::context_click(p, v, i);
        } else if resp.clicked() {
            let how = crate::track_selection::Click::from_mods(ui.input(|i| i.modifiers));
            crate::track_selection::click(p, v, i, how, v.show_hidden);
            if how == crate::track_selection::Click::Plain {
                sync_bottom_editor_to_track(p, &mut s.bottom, &mut s.score_tab, i);
            }
        }
        let stackable = v.selected_tracks(p).len() >= 2;
        let t = &p.tracks[i];
        let info = TrackInfo {
            i,
            folder: t.is_stack_parent(),
            stackable,
            mute: t.mute,
            solo: t.solo,
            custom_height: t.height != 1.0,
            last: p.tracks.len() == 1,
            groove: t.params.groove,
            follow_groove: crate::groove::groove_track(p)
                .filter(|g| *g != i)
                .map(|_| t.params.follow_groove),
        };
        let shown = v.shot_menu == Some((i, None));
        crate::widgets::context_menu_shown(&resp, shown, |ui| track_items(ui, &info, &mut picked));
    }
    // Crisp 1.5px divider separating headers column and the void below from the timeline
    gh.line_segment(
        [
            pos2(headers.right(), headers.top()),
            pos2(headers.right(), headers.bottom()),
        ],
        Stroke::new(1.5, Color32::from_rgb(0x10, 0x11, 0x13)),
    );
    if let Some(action) = picked {
        apply_ctx(action, p, s, lanes.width());
    }
    let v = &mut s.tracks;

    // ---- individual track height: drag the lower edge of a track (header or lane), as in
    // Logic. Registered after headers and regions so the edge wins over them.
    let edges = Rect::from_min_max(pos2(headers.left(), lanes.top()), lanes.right_bottom());
    for (i, &(top, row_h)) in rows.iter().enumerate() {
        if row_h <= 0.0 {
            continue;
        }
        let bottom = lanes.top() + top + row_h - scroll_y;
        if bottom < edges.top() || bottom > edges.bottom() {
            continue;
        }
        let edge = Rect::from_min_max(
            pos2(edges.left(), bottom - RESIZE_GRIP),
            pos2(edges.right(), bottom + RESIZE_GRIP),
        )
        .intersect(edges);
        let resp = ui
            .interact(edge, id(("track_edge", i)), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if resp.dragged() {
            if let (Some(pt), Some(t)) = (resp.interact_pointer_pos(), p.tracks.get_mut(i)) {
                let wanted = pt.y - (bottom - row_h) - take_rows_height(t);
                t.height = (wanted / v.row_h).clamp(Track::MIN_HEIGHT, Track::MAX_HEIGHT);
            }
        }
        if resp.double_clicked() {
            if let Some(t) = p.tracks.get_mut(i) {
                t.height = 1.0;
            }
        }
    }
    gh.line_segment(
        [headers.right_top(), headers.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(0x22)),
    );

    // ---- corner above the headers: "+" (new track), duplicate track, global tracks
    let gc = ui.painter_at(corner);
    gc.rect_filled(corner, 0.0, crate::theme::CARD_BG);
    gc.line_segment(
        [corner.left_bottom(), corner.right_bottom()],
        Stroke::new(1.0, crate::theme::BORDER),
    );
    let plus = Rect::from_center_size(
        pos2(corner.left() + 18.0, corner.center().y),
        vec2(24.0, 18.0),
    );
    let pr = ui.interact(plus, id(("icon", "plus")), Sense::click());
    gc.rect_filled(
        plus,
        4.0,
        if pr.hovered() {
            crate::theme::CARD_HOVER
        } else {
            BTN_BG
        },
    );
    crate::icons::paint_regular(&gc, plus, crate::icons::PLUS, 13.0, TEXT);
    if pr.clicked() {
        s.new_track.open();
    }
    let dup = Rect::from_center_size(
        pos2(plus.right() + 18.0, corner.center().y),
        vec2(24.0, 18.0),
    );
    let dr = ui.interact(dup, id(("icon", "dup")), Sense::click());
    let dup_bg = if dr.hovered() {
        crate::theme::CARD_HOVER
    } else {
        BTN_BG
    };
    gc.rect_filled(dup, 4.0, dup_bg);
    crate::icons::paint_regular(&gc, dup, crate::icons::COPY, 13.0, TEXT);
    if dr.clicked() {
        if let Some(copy) = crate::track_commands::duplicate_settings_of(p, p.selected_track) {
            p.selected_track = copy;
            p.song_rev += 1;
        }
    }
    let gt = Rect::from_center_size(
        pos2(corner.right() - 18.0, corner.center().y),
        vec2(24.0, 18.0),
    );
    gc.rect_stroke(
        Rect::from_center_size(gt.center(), vec2(14.0, 10.0)),
        1.5,
        stroke(TEXT_DIM),
        StrokeKind::Middle,
    );
    gc.line_segment(
        [
            gt.center() + vec2(-7.0, -1.5),
            gt.center() + vec2(7.0, -1.5),
        ],
        stroke(TEXT_DIM),
    );

    // ---- ruler
    let gr = ui.painter_at(ruler);
    gr.rect_filled(ruler, 0.0, RULER_BG);
    gr.line_segment(
        [ruler.left_bottom(), ruler.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(0x24)),
    );
    // The stretch of the ruler over the selected region turns gold, as in Logic.
    let selection = v
        .selected
        .and_then(|(t, r)| p.tracks.get(t)?.regions.get(r))
        .map(|r| (p.region_start_beats(r), p.region_end_beats(r)));
    if let Some((start, end)) = selection {
        let band = Rect::from_min_max(
            pos2(x_of(start).max(ruler.left()), ruler.top()),
            pos2(x_of(end).min(ruler.right()), ruler.bottom()),
        );
        if band.width() > 0.0 {
            gr.rect_filled(band, 0.0, RULER_SELECTION);
            gr.line_segment(
                [band.left_bottom(), band.right_bottom()],
                Stroke::new(1.0, RULER_SELECTION_EDGE),
            );
        }
    }
    if p.cycle {
        let band = Rect::from_min_max(
            pos2(x_of(p.cycle_start), ruler.top() + 2.0),
            pos2(x_of(p.cycle_end), ruler.bottom() - 1.0),
        );
        gr.rect_filled(band, 2.0, CYCLE);
    }
    let label_every = [1i64, 2, 4, 8, 16, 32]
        .into_iter()
        .find(|n| *n as f32 * v.bar_w >= 34.0)
        .unwrap_or(64);
    for bar in &bars {
        let b = bar.bar0 as i64;
        let x = x_of(bar.start);
        let beat = bar.start;
        let in_cycle = p.cycle && beat >= p.cycle_start && beat < p.cycle_end;
        let in_selection = selection.is_some_and(|(s0, s1)| beat >= s0 && beat < s1);
        let col = if in_cycle {
            Color32::from_gray(0x2a)
        } else if in_selection {
            Color32::WHITE
        } else {
            TEXT
        };
        if b % label_every == 0 {
            gr.line_segment(
                [pos2(x, ruler.top() + 4.0), pos2(x, ruler.bottom())],
                Stroke::new(1.0, col.gamma_multiply(0.7)),
            );
            gr.text(
                pos2(x + 3.0, ruler.top() + 4.0),
                Align2::LEFT_TOP,
                (b + 1).to_string(),
                FontId::proportional(11.5),
                col,
            );
        } else {
            gr.line_segment(
                [pos2(x, ruler.bottom() - 7.0), pos2(x, ruler.bottom())],
                Stroke::new(1.0, col.gamma_multiply(0.6)),
            );
        }
        if beat_px * bar.unit as f32 >= 10.0 {
            for k in 1..bar.num {
                let xb = x_of(bar.start + k as f64 * bar.unit);
                gr.line_segment(
                    [pos2(xb, ruler.bottom() - 4.0), pos2(xb, ruler.bottom())],
                    Stroke::new(1.0, col.gamma_multiply(0.45)),
                );
            }
        }
    }
    // Upper half: the cycle area (click toggles Cycle mode; drag moves or draws it). Lower
    // half: locate.
    let upper = Rect::from_min_max(ruler.min, pos2(ruler.right(), ruler.center().y));
    let lower = Rect::from_min_max(pos2(ruler.left(), ruler.center().y), ruler.max);
    crate::cycle_area::interact(ui, p, upper, &x_of, &beat_of, &snap_pos);
    let low = ui.interact(lower, id("ruler_locate"), Sense::click_and_drag());
    if let Some(pt) = low.interact_pointer_pos() {
        if low.clicked() || low.dragged() {
            p.position_beats = beat_of(pt.x);
        }
    }

    // ---- playhead
    let px = x_of(p.position_beats);
    // Grab the playhead's head in the ruler and drag it anywhere in the song (back to the
    // start, past the end of the view, ...). Registered after the ruler so the head wins over
    // the cycle area; kept alive while dragging even when the head leaves the view.
    let head_id = id("playhead_head");
    let visible = px >= lanes.left() && px <= lanes.right();
    if visible || ui.ctx().is_being_dragged(head_id) {
        let grab = Rect::from_min_max(
            pos2(px - PLAYHEAD_GRAB, ruler.top()),
            pos2(px + PLAYHEAD_GRAB, ruler.bottom()),
        );
        let resp = ui
            .interact(grab, head_id, Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if resp.dragged() {
            if let Some(pt) = resp.interact_pointer_pos() {
                p.position_beats = playhead_beat(pt.x, lanes.left(), scroll_x, beat_px);
            }
        }
    }
    if visible {
        let (line_c, head_c, edge_c) = playhead_colors(p.recording);
        let gp = ui.painter_at(Rect::from_min_max(ruler.min, area.max));
        gp.line_segment(
            [pos2(px, ruler.top() + 6.0), pos2(px, area.bottom())],
            Stroke::new(1.0, line_c),
        );
        let head = vec![
            pos2(px - 6.0, ruler.top() + 2.0),
            pos2(px + 6.0, ruler.top() + 2.0),
            pos2(px + 6.0, ruler.top() + 9.0),
            pos2(px, ruler.top() + 15.0),
            pos2(px - 6.0, ruler.top() + 9.0),
        ];
        gp.add(Shape::convex_polygon(
            head,
            head_c,
            Stroke::new(1.0, edge_c),
        ));
    }

    // A held tool and the Solo tool last only as long as the button is down.
    if ui.input(|i| i.pointer.any_released()) {
        v.held_tool = None;
        if let Some(was) = v.solo_hold.take() {
            crate::region_solo::end(p, was);
        }
    }
}

/// Takes a press of `key` with no modifier held out of this frame's input; returns whether
/// there was one.
fn take_plain_key(ui: &Ui, key: Key) -> bool {
    let mut found = false;
    ui.input_mut(|i| {
        i.events.retain(|e| match e {
            egui::Event::Key {
                key: k,
                pressed,
                modifiers,
                ..
            } if *k == key && modifiers.is_none() => {
                found |= *pressed;
                false
            }
            _ => true,
        });
    });
    found
}

#[cfg(test)]
mod row_tests {
    use super::*;

    #[test]
    fn volume_slider_round_trips_and_puts_unity_two_thirds_up() {
        assert_eq!(db_to_slider(0.0), SLIDER_UNITY);
        assert_eq!(db_to_slider(-60.0), 0.0);
        assert_eq!(db_to_slider(6.0), 1.0);
        for db in [-60.0, -30.0, -12.5, -0.1, 0.0, 2.0, 6.0] {
            assert!((slider_to_db(db_to_slider(db)) - db).abs() < 1e-3, "{db}");
        }
    }

    #[test]
    fn the_toolbar_drops_its_least_used_items_first_when_narrow() {
        let all = SNAP_W + DRAG_W + TOOLS_W + WAVE_W;
        let f = fit_toolbar(all);
        assert!(f.snap && f.drag && f.tools && f.wave);
        let f = fit_toolbar(all - 1.0);
        assert!(
            f.snap && f.drag && f.tools && !f.wave,
            "waveform buttons go first"
        );
        let f = fit_toolbar(SNAP_W + DRAG_W);
        assert!(f.snap && f.drag && !f.tools && !f.wave);
        let f = fit_toolbar(SNAP_W);
        assert!(f.snap && !f.drag);
        assert_eq!(
            fit_toolbar(-50.0),
            ToolbarFit {
                snap: false,
                drag: false,
                tools: false,
                wave: false
            }
        );
    }

    #[test]
    fn fake_waveforms_are_tied_to_the_region_not_the_screen() {
        // Same place in the region, same height, however the view is scrolled.
        assert_eq!(fake_level(7, 3.25), fake_level(7, 3.25));
        let levels: Vec<f32> = (0..64).map(|k| fake_level(7, k as f32 / 16.0)).collect();
        assert!(levels.iter().all(|l| (0.0..=1.0).contains(l)));
        assert!(
            levels.windows(2).any(|w| w[0] != w[1]),
            "the waveform varies"
        );
        assert_ne!(
            fake_level(5, 1.0),
            fake_level(6, 1.0),
            "each track has its own"
        );
    }

    #[test]
    fn rows_follow_each_tracks_height() {
        let mut p = Project::demo();
        p.tracks.truncate(3);
        p.tracks[1].height = 2.0;
        p.tracks[2].height = 0.5;
        let rows = row_layout(&p, 56.0);
        assert_eq!(rows, vec![(0.0, 56.0), (56.0, 112.0), (168.0, 28.0)]);
        assert_eq!(row_at(&rows, 0.0), Some(0));
        assert_eq!(row_at(&rows, 167.9), Some(1));
        assert_eq!(row_at(&rows, 168.0), Some(2));
        assert_eq!(row_at(&rows, 196.0), None);
        assert_eq!(row_at(&rows, -1.0), None);
    }

    #[test]
    fn the_tracks_of_a_folded_folder_take_no_room() {
        let mut p = Project::demo();
        p.tracks.truncate(4);
        let folder = p.create_track_stack(&[1], false); // wraps track 1: [t0, F, t1, t2, t3]
        assert_eq!(folder, Some(1));
        let open = row_layout(&p, 56.0);
        assert_eq!(open.len(), 5);
        assert!(open.iter().all(|&(_, h)| h == 56.0));

        p.tracks[1].folded = true;
        let folded = row_layout(&p, 56.0);
        assert_eq!(folded[2].1, 0.0, "the wrapped track is hidden");
        assert_eq!(
            folded[3],
            (112.0, 56.0),
            "the next track moves up under the folder"
        );
        assert_eq!(row_at(&folded, 100.0), Some(1));
        assert_eq!(row_at(&folded, 112.0), Some(3), "hidden rows are never hit");
    }

    #[test]
    fn dragging_the_playhead_follows_the_pointer_and_stops_at_the_start() {
        // Lanes start at x = 100, scrolled by 50 px, 20 px per beat.
        assert_eq!(playhead_beat(150.0, 100.0, 50.0, 20.0), 5.0);
        assert_eq!(playhead_beat(100.0, 100.0, 0.0, 20.0), 0.0);
        // Dragged left of the lanes: pinned to the beginning of the song.
        assert_eq!(playhead_beat(20.0, 100.0, 0.0, 20.0), 0.0);
    }

    #[test]
    fn solo_silences_every_other_track_until_released() {
        let mut p = Project::demo();
        assert!((0..p.tracks.len()).all(|i| !silenced_by_solo(&p, i)));
        p.tracks[2].solo = true;
        assert!(!silenced_by_solo(&p, 2));
        assert!((0..p.tracks.len())
            .filter(|&i| i != 2)
            .all(|i| silenced_by_solo(&p, i)));
        // Soloing a second track adds it to what plays.
        p.tracks[4].solo = true;
        assert!(!silenced_by_solo(&p, 4) && silenced_by_solo(&p, 0));
        p.tracks[2].solo = false;
        p.tracks[4].solo = false;
        assert!((0..p.tracks.len()).all(|i| !silenced_by_solo(&p, i)));
    }

    #[test]
    fn beat_snap_follows_the_signature_beat() {
        // 4/4: a beat is a quarter note, as before.
        assert_eq!(snap_step(Snap::Beat, 20.0, 4.0, 1.0), 1.0);
        assert_eq!(snap_step(Snap::Smart, 20.0, 4.0, 1.0), 1.0);
        assert_eq!(snap_step(Snap::Bar, 20.0, 4.0, 1.0), 4.0);
        // 6/8: a beat is an eighth (0.5 quarter notes), a bar 3 quarter notes.
        assert_eq!(snap_step(Snap::Beat, 20.0, 3.0, 0.5), 0.5);
        assert_eq!(snap_step(Snap::Bar, 20.0, 3.0, 0.5), 3.0);
        // Smart snaps to eighths once an eighth is at least 12 px wide, else to bars.
        assert_eq!(snap_step(Snap::Smart, 24.0, 3.0, 0.5), 0.5);
        assert_eq!(snap_step(Snap::Smart, 20.0, 3.0, 0.5), 3.0);
        // Divisions are 16th notes in any signature.
        assert_eq!(snap_step(Snap::Division, 20.0, 3.0, 0.5), 0.25);
        assert_eq!(snap_step(Snap::Smart, 60.0, 3.0, 0.5), 0.25);
    }

    /// A 0.5 s mono 16-bit 44.1 kHz WAV: a square wave at `amp` (0..1), or silence for 0.
    fn write_square_wav(path: &std::path::Path, amp: f32) {
        let n = 22_050u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&44_100u32.to_le_bytes());
        b.extend_from_slice(&88_200u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let s = if (i / 50) % 2 == 0 { amp } else { -amp };
            b.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// Runs `poll_normalize` until the scan is over (it reads the file on another thread).
    fn wait_for_normalize(p: &mut Project, s: &mut UiState) {
        let ctx = egui::Context::default();
        let began = std::time::Instant::now();
        while !s.tracks.normalize_jobs.is_empty() {
            assert!(began.elapsed().as_secs() < 10, "the scan never finished");
            poll_normalize(&ctx, p, s);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn project_with_wav(amp: f32) -> (Project, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("take.wav");
        write_square_wav(&wav, amp);
        let mut p = Project::empty();
        crate::import::import_audio(&mut p, &wav, &egui::Context::default()).unwrap();
        (p, dir)
    }

    #[test]
    fn normalize_reads_the_file_in_the_background_and_sets_the_gain() {
        let (mut p, _dir) = project_with_wav(0.25);
        let (t, mut s) = (p.tracks.len() - 1, UiState::new());
        start_normalize(&p, &mut s, t, 0);
        assert_eq!(s.tracks.normalize_jobs.len(), 1);
        assert_eq!(s.status.as_deref(), Some("Normalizing…"));
        start_normalize(&p, &mut s, t, 0);
        assert_eq!(s.tracks.normalize_jobs.len(), 1, "one scan per region");
        wait_for_normalize(&mut p, &mut s);
        // A peak of 0.25 needs +12.04 dB to reach full scale.
        let gain = p.tracks[t].regions[0].params.gain_db;
        assert!((gain - 12.04).abs() < 0.05, "gain {gain}");
        assert_eq!(s.status, None, "the message goes away when it is done");
    }

    #[test]
    fn normalize_reports_silence_and_regions_without_audio() {
        let (mut p, _dir) = project_with_wav(0.0);
        let (t, mut s) = (p.tracks.len() - 1, UiState::new());
        start_normalize(&p, &mut s, t, 0);
        wait_for_normalize(&mut p, &mut s);
        assert_eq!(p.tracks[t].regions[0].params.gain_db, 0.0);
        assert_eq!(s.status.as_deref(), Some("Normalize: the region is silent"));
        // A MIDI region has nothing to scan.
        let mut s = UiState::new();
        start_normalize(&Project::demo(), &mut s, 0, 0);
        assert!(s.tracks.normalize_jobs.is_empty());
        assert_eq!(
            s.status.as_deref(),
            Some("Normalize: this region has no recorded audio")
        );
    }

    #[test]
    fn a_scan_for_a_region_that_has_gone_changes_nothing() {
        let (mut p, _dir) = project_with_wav(0.25);
        let (t, mut s) = (p.tracks.len() - 1, UiState::new());
        start_normalize(&p, &mut s, t, 0);
        // The user deletes the region while the file is being read.
        p.tracks[t].regions.clear();
        wait_for_normalize(&mut p, &mut s);
        assert!(p.tracks[t].regions.is_empty());
        assert_eq!(s.status, None);
    }

    /// Runs one frame of the Tracks area with `events` and returns nothing: the project and
    /// the UI state are changed in place, as the app does each frame.
    fn run_tracks_frame(
        ctx: &egui::Context,
        p: &mut Project,
        s: &mut UiState,
        meters: &crate::mixer::Meters,
        events: Vec<egui::Event>,
    ) {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| show(ui, p, s, meters));
        out.textures_delta.clear();
    }

    #[test]
    fn the_pencil_draws_a_midi_region_by_click_and_drag_in_the_tracks_area() {
        use egui::{Event, PointerButton};
        let mut p = Project::empty();
        p.tracks.clear();
        let ti = p.add_track(TrackKind::Instrument);
        let mut s = UiState::new();
        s.tracks.bar_w = 62.0;
        s.tracks.tool = TrackTool::Pencil;
        let meters = crate::mixer::Meters::default();
        let ctx = egui::Context::default();
        // The first track's lane: its top is the top of the lanes (56 points down), and a bar is
        // 62 points wide from the lanes' left edge (340 points).
        let lane_y = 76.0;
        let bar_x = |bar: f32| 340.0 + bar * 62.0 + 4.0;
        let press = |pos: Pos2, pressed: bool| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        run_tracks_frame(&ctx, &mut p, &mut s, &meters, vec![]);

        // A click on empty lane space: one bar long, from where the pointer is.
        let click = pos2(bar_x(2.0), lane_y);
        run_tracks_frame(
            &ctx,
            &mut p,
            &mut s,
            &meters,
            vec![Event::PointerMoved(click)],
        );
        run_tracks_frame(&ctx, &mut p, &mut s, &meters, vec![press(click, true)]);
        run_tracks_frame(&ctx, &mut p, &mut s, &meters, vec![press(click, false)]);
        let regions = &p.tracks[ti].regions;
        assert_eq!(regions.len(), 1, "a click adds one region");
        assert_eq!(regions[0].len_bars, 1.0, "a click makes a one-bar region");
        assert!(
            regions[0].notes.is_empty(),
            "the region is empty, ready to draw in"
        );
        let clicked_start = regions[0].start_bar;
        assert!(
            (clicked_start - 3.0).abs() < 0.5,
            "starts where clicked: {clicked_start}"
        );

        // A drag across empty lane space further along: the region grows with the pointer.
        let (from, to) = (pos2(bar_x(6.0), lane_y), pos2(bar_x(9.0), lane_y));
        run_tracks_frame(
            &ctx,
            &mut p,
            &mut s,
            &meters,
            vec![Event::PointerMoved(from)],
        );
        run_tracks_frame(&ctx, &mut p, &mut s, &meters, vec![press(from, true)]);
        run_tracks_frame(
            &ctx,
            &mut p,
            &mut s,
            &meters,
            vec![Event::PointerMoved(pos2(from.x + 10.0, lane_y))],
        );
        run_tracks_frame(&ctx, &mut p, &mut s, &meters, vec![Event::PointerMoved(to)]);
        run_tracks_frame(&ctx, &mut p, &mut s, &meters, vec![press(to, false)]);
        let regions = &p.tracks[ti].regions;
        assert_eq!(regions.len(), 2, "the drag adds a second region");
        let dragged = &regions[1];
        assert!(
            dragged.len_bars > 2.0,
            "the drag sets the length: {}",
            dragged.len_bars
        );
        assert!(
            dragged.start_bar > regions[0].start_bar,
            "the drag starts later"
        );
        assert!(s.tracks.pencil_drag.is_none(), "the drag is finished");
    }

    #[test]
    fn the_pencil_and_fade_tools_sit_where_their_menu_entries_are() {
        assert_eq!(TOOLS[PENCIL_TOOL], "Pencil Tool");
        assert_eq!(TOOLS[FADE_TOOL], "Fade Tool");
        assert_eq!(TOOLS[0], "Pointer Tool");
    }

    #[test]
    fn reverse_region_tells_the_engine_to_rebuild_the_song() {
        let (mut p, _dir) = project_with_wav(0.5);
        let mut s = UiState::new();
        let (ti, ri) = p
            .tracks
            .iter()
            .enumerate()
            .find_map(|(ti, t)| {
                t.regions
                    .iter()
                    .position(|r| r.audio.is_some())
                    .map(|ri| (ti, ri))
            })
            .expect("the imported audio region");
        let rev = p.song_rev;
        apply_ctx(CtxAction::ToggleReverse(ti, ri), &mut p, &mut s, 1000.0);
        assert!(p.tracks[ti].regions[ri].params.reverse);
        assert_ne!(p.song_rev, rev, "the engine must hear the reversed clip");
        apply_ctx(CtxAction::ToggleReverse(ti, ri), &mut p, &mut s, 1000.0);
        assert!(
            !p.tracks[ti].regions[ri].params.reverse,
            "a second pick turns it off"
        );
    }

    #[test]
    fn the_fade_tool_measures_from_the_edge_in_sixteenths() {
        // A region from beat 8, 4 beats long.
        assert_eq!(fade_for_drag(9.0, 8.0, 4.0, true), 1.0);
        assert_eq!(
            fade_for_drag(9.1, 8.0, 4.0, true),
            1.0,
            "snaps to 1/16 notes"
        );
        assert_eq!(fade_for_drag(9.2, 8.0, 4.0, true), 1.25);
        assert_eq!(fade_for_drag(11.0, 8.0, 4.0, false), 1.0, "from the end");
        assert_eq!(
            fade_for_drag(7.0, 8.0, 4.0, true),
            0.0,
            "not before the start"
        );
        assert_eq!(
            fade_for_drag(13.0, 8.0, 4.0, false),
            0.0,
            "not past the end"
        );
        assert_eq!(
            fade_for_drag(30.0, 8.0, 4.0, true),
            4.0,
            "not longer than the region"
        );
        assert_eq!(fade_for_drag(-5.0, 8.0, 4.0, false), 4.0);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    #[test]
    fn shift_click_adds_and_takes_out_and_the_primary_is_always_selected() {
        let mut v = TracksView::default();
        v.toggle(0, 0);
        assert_eq!(v.selected, Some((0, 0)), "the first one is primary");
        v.toggle(1, 2);
        v.toggle(0, 1);
        assert_eq!(v.selection(), vec![(0, 0), (0, 1), (1, 2)]);
        // Taking the primary out makes the most recent one primary.
        v.toggle(0, 0);
        assert_eq!(v.selected, Some((0, 1)));
        assert_eq!(v.selection(), vec![(0, 1), (1, 2)]);
        v.toggle(0, 1);
        v.toggle(1, 2);
        assert_eq!(v.selected, None);
        assert!(v.selection().is_empty());
    }

    #[test]
    fn a_click_inside_a_selection_makes_that_region_primary_and_keeps_the_rest() {
        let mut v = TracksView::default();
        v.select_many(&[(0, 0), (0, 1), (1, 0)]);
        v.make_primary(1, 0);
        assert_eq!(v.selected, Some((1, 0)));
        assert_eq!(v.selection(), vec![(0, 0), (0, 1), (1, 0)]);
        // A region that is not selected is ignored here (the caller selects it alone instead).
        v.make_primary(5, 5);
        assert_eq!(v.selected, Some((1, 0)));
    }

    #[test]
    fn select_many_keeps_the_first_as_primary_and_drops_repeats() {
        let mut v = TracksView::default();
        v.select_many(&[(2, 1), (0, 0), (2, 1)]);
        assert_eq!(v.selected, Some((2, 1)));
        assert_eq!(v.extra, vec![(0, 0)]);
        v.select_many(&[]);
        assert!(v.selected.is_none() && v.extra.is_empty());
    }

    #[test]
    fn select_all_selects_every_region_of_every_track() {
        let mut p = Project::demo();
        let count: usize = p.tracks.iter().map(|t| t.regions.len()).sum();
        let mut v = TracksView::default();
        v.select_all(&p);
        assert_eq!(v.selection().len(), count);
        assert!(count > 1, "the demo project has several regions");
        // An empty project selects nothing.
        p.tracks.iter_mut().for_each(|t| t.regions.clear());
        v.select_all(&p);
        assert!(v.selected.is_none());
    }

    #[test]
    fn prune_forgets_regions_that_are_gone() {
        let mut p = Project::demo();
        let mut v = TracksView::default();
        v.select_many(&[(0, 0), (1, 0), (99, 0)]);
        v.prune(&p);
        assert_eq!(
            v.selection(),
            vec![(0, 0), (1, 0)],
            "track 99 does not exist"
        );
        // The primary one goes: another selected region takes its place.
        p.tracks[0].regions.clear();
        v.prune(&p);
        assert_eq!(v.selected, Some((1, 0)));
        assert!(v.extra.is_empty());
    }

    #[test]
    fn a_selected_region_is_drawn_as_selected() {
        let mut v = TracksView::default();
        v.select_many(&[(0, 1), (2, 0)]);
        assert!(v.is_selected(0, 1) && v.is_selected(2, 0));
        assert!(!v.is_selected(0, 0));
    }
}

#[cfg(test)]
mod flex_marker_tests {
    use super::*;
    use crate::model::{AudioRegion, RegionParams};
    use crate::ui_test::Harness;
    use egui::PointerButton;
    use prowave_engine::{AudioFile, FlexMode, WarpPoint};
    use std::sync::{Arc, OnceLock};

    /// A project with one audio track: a two-second sound on a two-bar region at bar 1, with
    /// Flex Time on and one marker halfway (one second in).
    fn project_with_marker() -> Project {
        let mut p = Project::empty();
        p.tracks.clear();
        let mut track = Track::new("Audio 1", TrackKind::Audio);
        let file = AudioFile {
            path: "drums.wav".into(),
            sample_rate: 44_100,
            frames: 88_200,
            channels: 2,
        };
        let mut flex = crate::flex_edit::FlexEdit::new(file.clone(), FlexMode::Rhythmic);
        flex.markers = vec![WarpPoint { src: 1.0, dst: 1.0 }];
        track.regions.push(Region {
            name: "drums".into(),
            start_bar: 1.0,
            len_bars: 2.0,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            audio: Some(AudioRegion {
                flex: Some(flex),
                ..AudioRegion::whole(file, Arc::new(OnceLock::new()))
            }),
            missing: None,
            params: RegionParams::default(),
            generated: None,
        });
        p.tracks.push(track);
        p
    }

    #[test]
    fn a_flex_marker_follows_the_pointer_and_leaves_the_region_alone() {
        let mut p = project_with_marker();
        let mut s = UiState::new();
        s.tracks.bar_w = 62.0;
        let mut h = Harness::new(1200.0, 600.0);
        let meters = crate::mixer::Meters::default();
        for _ in 0..2 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        // The lanes start at x 340 (the track headers); at the default zoom a bar is 62
        // points wide, so the two-bar region runs to x 464 and the marker is halfway, at 402.
        let marker = pos2(402.0, 84.0);
        h.drag(
            &[marker, marker + vec2(15.0, 0.0), marker + vec2(30.0, 0.0)],
            PointerButton::Primary,
            |ui| show(ui, &mut p, &mut s, &meters),
        );
        let region = &p.tracks[0].regions[0];
        let flex = region.audio.as_ref().unwrap().flex.as_ref().unwrap();
        assert_eq!(flex.markers.len(), 1);
        assert_eq!(flex.markers[0].src, 1.0, "the recording moment stays put");
        let dst = flex.markers[0].dst;
        assert!((dst - 1.484).abs() < 0.02, "moved to {dst} seconds");
        assert_eq!(region.start_bar, 1.0, "the region itself does not move");
    }

    #[test]
    fn a_second_drag_and_a_double_click_use_the_result_length() {
        let mut p = project_with_marker();
        let mut s = UiState::new();
        s.tracks.bar_w = 62.0;
        let mut h = Harness::new(1200.0, 600.0);
        let meters = crate::mixer::Meters::default();
        for _ in 0..2 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        // The region runs from x 340 to 464 (two bars at the default zoom).
        let (left, width) = (340.0, 124.0);
        let marker = pos2(left + width / 2.0, 84.0);
        h.drag(
            &[marker, marker + vec2(10.0, 0.0)],
            PointerButton::Primary,
            |ui| show(ui, &mut p, &mut s, &meters),
        );
        let (first, length) = {
            let f = p.tracks[0].regions[0]
                .audio
                .as_ref()
                .unwrap()
                .flex
                .as_ref()
                .unwrap();
            (f.markers[0].dst, f.length())
        };
        assert!((first - 1.161).abs() < 0.02, "moved to {first}");
        assert!((length - 2.161).abs() < 0.02, "result is {length} s long");

        // The marker is drawn on the result's scale now. Grabbing it there and dragging
        // again must move it, not the region.
        let at = pos2(left + (first / length) as f32 * width, 84.0);
        for _ in 0..2 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        h.drag(&[at, at + vec2(10.0, 0.0)], PointerButton::Primary, |ui| {
            show(ui, &mut p, &mut s, &meters)
        });
        let (second, length) = {
            let f = p.tracks[0].regions[0]
                .audio
                .as_ref()
                .unwrap()
                .flex
                .as_ref()
                .unwrap();
            (f.markers[0].dst, f.length())
        };
        assert!(second > first + 0.1, "{second} after {first}");
        assert_eq!(p.tracks[0].regions[0].start_bar, 1.0);

        // A double-click maps by the new length: half a second into the result.
        let spot = pos2(left + (0.5 / length) as f32 * width, 84.0);
        for _ in 0..30 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        for _ in 0..2 {
            h.click(spot, |ui| show(ui, &mut p, &mut s, &meters));
        }
        let f = p.tracks[0].regions[0]
            .audio
            .as_ref()
            .unwrap()
            .flex
            .as_ref()
            .unwrap();
        assert!(
            f.markers.iter().any(|m| (m.dst - 0.5).abs() < 0.02),
            "no marker at half a second: {:?}",
            f.markers.iter().map(|m| m.dst).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_double_click_adds_a_flex_marker_and_another_removes_it() {
        let mut p = project_with_marker();
        let mut s = UiState::new();
        s.tracks.bar_w = 62.0;
        let mut h = Harness::new(1200.0, 600.0);
        let meters = crate::mixer::Meters::default();
        for _ in 0..2 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        // A quarter into the region (half a second), away from the marker at one second.
        let empty = pos2(371.0, 84.0);
        for _ in 0..2 {
            h.click(empty, |ui| show(ui, &mut p, &mut s, &meters));
        }
        let flex = p.tracks[0].regions[0]
            .audio
            .as_ref()
            .unwrap()
            .flex
            .as_ref()
            .unwrap();
        assert_eq!(flex.markers.len(), 2, "a marker was added");
        let added = flex
            .markers
            .iter()
            .find(|m| (m.dst - 0.5).abs() < 0.02)
            .expect("the new marker at half a second");
        assert_eq!(added.src, added.dst, "nothing is moved by adding it");
        // Long enough that the next two clicks are a new double-click.
        for _ in 0..30 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        // The same double-click on the marker takes it out.
        for _ in 0..2 {
            h.click(empty, |ui| show(ui, &mut p, &mut s, &meters));
        }
        let flex = p.tracks[0].regions[0]
            .audio
            .as_ref()
            .unwrap()
            .flex
            .as_ref()
            .unwrap();
        assert_eq!(flex.markers.len(), 1, "the marker was removed");
    }
}

#[cfg(test)]
mod waveform_scale_tests {
    use super::display_amp;

    #[test]
    fn silence_stays_flat_and_full_scale_stays_full_height() {
        assert_eq!(display_amp(0.0), 0.0);
        assert!((display_amp(1.0) - 1.0).abs() < 1e-6);
        assert!((display_amp(-1.0) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_quiet_recording_is_drawn_tall_enough_to_see() {
        // A microphone that peaks at 0.05 used to be 5 % of the height (a flat line).
        assert!(display_amp(0.05) > 0.25, "{}", display_amp(0.05));
        assert!(display_amp(0.2) > 0.5);
        // Still in order: louder is taller, and the curve is symmetric.
        let mut last = 0.0;
        for i in 1..=100 {
            let v = display_amp(i as f32 / 100.0);
            assert!(v > last);
            last = v;
        }
        assert_eq!(display_amp(-0.3), -display_amp(0.3));
    }

    #[test]
    fn bad_values_draw_as_silence_and_boosted_regions_stay_finite() {
        assert_eq!(display_amp(f32::NAN), 0.0);
        assert_eq!(display_amp(f32::INFINITY), 0.0);
        assert!(display_amp(100.0).is_finite() && display_amp(100.0) > 1.0);
    }
}
