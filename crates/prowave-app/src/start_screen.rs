//! Project Chooser, shown by File > New from Template… and on startup:
//! start an empty project, reopen a recent one or start from a template, as in Logic.
//! Choosing queues a File command (`UiState::file_cmd`), so unsaved changes are asked about.

use crate::file_commands::FileCmd;
use crate::i18n::tr;
use crate::model::{Project, Track, TrackKind};
use crate::track_icons::{draw_icon, IconKind};
use crate::UiState;
use egui::{pos2, vec2, Align2, Color32, FontId, Key, Rect, Sense, Stroke, StrokeKind};
use std::path::PathBuf;

/// A project to start from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Template {
    /// One software instrument track (File > New).
    Empty,
    HipHop,
    Electronic,
    Songwriter,
    Orchestral,
}

impl Template {
    pub const ALL: [Template; 5] = [
        Template::Empty,
        Template::HipHop,
        Template::Electronic,
        Template::Songwriter,
        Template::Orchestral,
    ];

    pub const FACTORY: [Template; 4] = [
        Template::HipHop,
        Template::Electronic,
        Template::Songwriter,
        Template::Orchestral,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Template::Empty => "Empty Project",
            Template::HipHop => "Hip Hop",
            Template::Electronic => "Electronic",
            Template::Songwriter => "Songwriter",
            Template::Orchestral => "Orchestral",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Template::Empty => "One software instrument track.",
            Template::HipHop => "808 bass, drum machine and synth lead.",
            Template::Electronic => "Synths, drum machine and a reverb aux.",
            Template::Songwriter => "Piano, drummer, bass and vocal tracks.",
            Template::Orchestral => "Strings, brass, woodwinds and percussion.",
        }
    }

    fn icon(self) -> IconKind {
        match self {
            Template::Empty => IconKind::Note,
            Template::HipHop => IconKind::Synth,
            Template::Electronic => IconKind::Synth,
            Template::Songwriter => IconKind::Piano,
            Template::Orchestral => IconKind::Strings,
        }
    }

    /// The new, untitled project this template starts.
    pub fn project(self) -> Project {
        let track = |name: &str, kind, instrument: Option<&'static str>| {
            let mut t = Track::new(name, kind);
            if instrument.is_some() {
                t.instrument = instrument;
            }
            t
        };
        let mut p = Project::empty();
        match self {
            Template::Empty => return p,
            Template::HipHop => {
                p.tempo = 140.0;
                p.tracks = vec![
                    track("808 Bass", TrackKind::Instrument, Some("808 Bass")),
                    track("Drum Machine", TrackKind::Drummer, Some("Drum Machine")),
                    track("Sampler Keys", TrackKind::Instrument, Some("Sampler")),
                    track("Lead Synth", TrackKind::Instrument, Some("Vintage B3")),
                ];
            }
            Template::Electronic => {
                p.tempo = 124.0;
                p.tracks = vec![
                    track("Drum Machine", TrackKind::Drummer, Some("Drum Machine")),
                    track("Synth Bass", TrackKind::Instrument, Some("808 Bass")),
                    track("Analog Lead", TrackKind::Instrument, Some("Electric Piano")),
                    track("Reverb", TrackKind::Aux, None),
                ];
            }
            Template::Songwriter => {
                p.tempo = 96.0;
                p.tracks = vec![
                    track(
                        "Steinway Grand Piano",
                        TrackKind::Instrument,
                        Some("Sampler"),
                    ),
                    track("Drummer", TrackKind::Drummer, Some("Drum Kit")),
                    track("Bass", TrackKind::Audio, None),
                    track("Lead Vocal", TrackKind::Audio, None),
                ];
            }
            Template::Orchestral => {
                p.tempo = 84.0;
                p.tracks = vec![
                    track("Strings Ensemble", TrackKind::Instrument, Some("Sampler")),
                    track("Brass Section", TrackKind::Instrument, Some("Sampler")),
                    track("Woodwinds", TrackKind::Instrument, Some("Sampler")),
                    track("Timpani & Percussion", TrackKind::Audio, None),
                ];
            }
        }
        p.name = "Untitled".into();
        p.selected_track = 0;
        p
    }
}

/// Sidebar sections of the chooser.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    NewProject,
    Recent,
    DemoProjects,
    MyTemplates,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::NewProject,
        Section::Recent,
        Section::DemoProjects,
        Section::MyTemplates,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::NewProject => tr("New Project"),
            Section::Recent => tr("Recent"),
            Section::DemoProjects => tr("Demo Projects"),
            Section::MyTemplates => tr("My Templates"),
        }
    }
}

/// Working project configuration details chosen in the collapsible section.
#[derive(Clone, Debug, PartialEq)]
pub struct ChooserDetails {
    pub show_details: bool,
    pub use_musical_grid: bool,
    pub tempo: f64,
    pub tap_history: Vec<std::time::Instant>,
    pub key_idx: usize,
    pub is_major: bool,
    pub sig_idx: usize,
    pub sample_rate_idx: usize,
    pub frame_rate_idx: usize,
    pub surround_idx: usize,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}

impl Default for ChooserDetails {
    fn default() -> Self {
        Self {
            show_details: true,
            use_musical_grid: true,
            tempo: 120.0,
            tap_history: Vec::new(),
            key_idx: 0,
            is_major: true,
            sig_idx: 0,
            sample_rate_idx: 1, // 48 kHz
            frame_rate_idx: 1,  // 24 fps
            surround_idx: 0,    // Stereo
            input_device: None,
            output_device: None,
        }
    }
}

impl ChooserDetails {
    pub fn apply(&self, p: &mut Project) {
        p.tempo = self.tempo as f32;
        p.sig = self.time_signature();
        p.key = self.key_str();
        p.smpte_rate = self.smpte_rate();
        p.song_rev += 1;
    }

    pub fn key_str(&self) -> &'static str {
        const MAJOR_KEYS: [&str; 12] = [
            "Cmaj", "C#maj", "Dmaj", "D#maj", "Emaj", "Fmaj", "F#maj", "Gmaj", "G#maj", "Amaj",
            "A#maj", "Bmaj",
        ];
        const MINOR_KEYS: [&str; 12] = [
            "Cmin", "C#min", "Dmin", "D#min", "Emin", "Fmin", "F#min", "Gmin", "G#min", "Amin",
            "A#min", "Bmin",
        ];
        if self.is_major {
            MAJOR_KEYS[self.key_idx.min(11)]
        } else {
            MINOR_KEYS[self.key_idx.min(11)]
        }
    }

    pub fn smpte_rate(&self) -> prowave_engine::SmpteRate {
        match self.frame_rate_idx {
            0 => prowave_engine::SmpteRate::Fps24,
            1 => prowave_engine::SmpteRate::Fps24,
            2 => prowave_engine::SmpteRate::Fps25,
            3 => prowave_engine::SmpteRate::Fps29_97NonDrop,
            4 => prowave_engine::SmpteRate::Fps30,
            _ => prowave_engine::SmpteRate::Fps24,
        }
    }

    pub fn time_signature(&self) -> (u8, u8) {
        match self.sig_idx {
            0 => (4, 4),
            1 => (3, 4),
            2 => (2, 4),
            3 => (6, 8),
            4 => (12, 8),
            _ => (4, 4),
        }
    }

    pub fn tap(&mut self) {
        let now = std::time::Instant::now();
        if let Some(&last) = self.tap_history.last() {
            if now.duration_since(last).as_secs_f64() > 2.5 {
                self.tap_history.clear();
            }
        }
        self.tap_history.push(now);
        if self.tap_history.len() > 6 {
            self.tap_history.remove(0);
        }
        if self.tap_history.len() >= 2 {
            let mut sum = 0.0;
            for i in 1..self.tap_history.len() {
                sum += self.tap_history[i]
                    .duration_since(self.tap_history[i - 1])
                    .as_secs_f64();
            }
            let avg = sum / (self.tap_history.len() - 1) as f64;
            if avg > 0.0 {
                let bpm = (60.0 / avg).clamp(40.0, 300.0);
                self.tempo = (bpm * 10.0).round() / 10.0;
            }
        }
    }
}

/// State of the open chooser.
#[derive(Clone, Debug, PartialEq)]
pub struct Chooser {
    pub section: Section,
    /// Highlighted tile or row in the section.
    pub selected: usize,
    /// Detailed project options.
    pub details: ChooserDetails,
}

impl Default for Chooser {
    fn default() -> Self {
        Self {
            section: Section::NewProject,
            selected: 0,
            details: ChooserDetails::default(),
        }
    }
}

impl Chooser {
    /// File > New from Template…: over the open project, which Cancel goes back to.
    pub fn templates() -> Self {
        Self {
            section: Section::NewProject,
            selected: 0,
            details: ChooserDetails::default(),
        }
    }
}

/// Dynamically calculates the Project Chooser viewport size based on monitor dimensions.
pub fn chooser_viewport_size(mon: Option<egui::Vec2>) -> egui::Vec2 {
    let m = mon.unwrap_or(egui::vec2(1600.0, 960.0));
    let w = (m.x * 0.62).clamp(780.0, 1180.0);
    let h = (m.y * 0.68).clamp(520.0, 780.0);
    egui::vec2(w, h)
}

/// What the highlighted item does when chosen.
fn choice(c: &Chooser, recent: &[PathBuf]) -> Option<Choice> {
    match c.section {
        Section::NewProject => {
            if c.selected == 0 {
                Some(Choice::Template(Template::Empty))
            } else {
                Template::FACTORY
                    .get(c.selected - 1)
                    .copied()
                    .map(Choice::Template)
            }
        }
        Section::Recent => recent.get(c.selected).cloned().map(Choice::Recent),
        Section::DemoProjects => Some(Choice::Template(Template::Electronic)),
        Section::MyTemplates => Some(Choice::Template(Template::Empty)),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Choice {
    Template(Template),
    Recent(PathBuf),
}

/// Queues the File command for `choice`.
fn choose(choice: Choice, c: &Chooser, s: &mut UiState) {
    match choice {
        Choice::Template(t) => {
            s.chooser_details = c.details.clone();
            s.file_cmd = Some(FileCmd::NewFrom(t));
        }
        Choice::Recent(path) => {
            s.recent_pick = Some(path);
            s.file_cmd = Some(FileCmd::OpenRecent);
        }
    }
}

const BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x21);
const SIDEBAR: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a);
const TEXT: Color32 = Color32::from_rgb(0xea, 0xee, 0xf4);
const DIM: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xa2);
const ACCENT: Color32 = Color32::from_rgb(0x38, 0x39, 0x3e);
const TILE: Color32 = Color32::from_rgb(0x26, 0x26, 0x2a);
const BORDER: Color32 = Color32::from_rgb(0x34, 0x34, 0x39);

/// A tile in the New Project grid. Returns (clicked, double-clicked).
fn tile(ui: &egui::Ui, r: Rect, t: Template, selected: bool, key: usize) -> (bool, bool) {
    let resp = ui.interact(r, egui::Id::unique(("chooser_tile", key)), Sense::click());
    let g = ui.painter();
    let fill = if resp.hovered() {
        Color32::from_rgb(0x30, 0x30, 0x35)
    } else {
        TILE
    };
    g.rect_filled(r, 6.0, fill);
    g.rect_stroke(
        r,
        6.0,
        Stroke::new(
            if selected { 1.5 } else { 1.0 },
            if selected {
                Color32::from_rgba_unmultiplied(255, 255, 255, 89)
            } else {
                BORDER
            },
        ),
        StrokeKind::Inside,
    );

    // Compact 36x36px icon container with 24px Phosphor glyph
    let icon_box = Rect::from_center_size(pos2(r.center().x, r.top() + 28.0), vec2(36.0, 36.0));
    g.rect_filled(icon_box, 6.0, Color32::from_rgb(0x1a, 0x1a, 0x1d));
    g.rect_stroke(
        icon_box,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        StrokeKind::Inside,
    );
    draw_icon(g, icon_box.shrink(6.0), t.icon());

    g.text(
        pos2(r.center().x, r.top() + 56.0),
        Align2::CENTER_CENTER,
        tr(t.name()),
        FontId::proportional(12.5),
        if selected { Color32::WHITE } else { TEXT },
    );
    let about = g.layout(
        tr(t.about()).to_string(),
        FontId::proportional(10.5),
        DIM,
        r.width() - 14.0,
    );
    let at = pos2(r.center().x - about.size().x / 2.0, r.top() + 72.0);
    g.galley(at, about, DIM);
    (resp.clicked(), resp.double_clicked())
}

/// Primary Empty Project card
fn empty_project_card(ui: &egui::Ui, r: Rect, selected: bool) -> (bool, bool) {
    let resp = ui.interact(r, egui::Id::unique("empty_project_tile"), Sense::click());
    let g = ui.painter();
    let fill = if resp.hovered() {
        Color32::from_rgb(0x30, 0x30, 0x35)
    } else {
        TILE
    };
    g.rect_filled(r, 6.0, fill);
    g.rect_stroke(
        r,
        6.0,
        Stroke::new(
            if selected { 1.5 } else { 1.0 },
            if selected {
                Color32::from_rgba_unmultiplied(255, 255, 255, 89)
            } else {
                BORDER
            },
        ),
        StrokeKind::Inside,
    );

    // Compact 36x36px icon container with 22px Phosphor PLUS glyph
    let icon_box = Rect::from_center_size(pos2(r.center().x, r.top() + 28.0), vec2(36.0, 36.0));
    g.rect_filled(icon_box, 6.0, Color32::from_rgb(0x1a, 0x1a, 0x1d));
    g.rect_stroke(
        icon_box,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        StrokeKind::Inside,
    );
    crate::icons::paint_regular(
        g,
        icon_box,
        crate::icons::PLUS,
        22.0,
        Color32::from_rgb(0xf0, 0xf0, 0xf4),
    );

    g.text(
        pos2(r.center().x, r.top() + 56.0),
        Align2::CENTER_CENTER,
        tr("Empty Project"),
        FontId::proportional(13.0),
        if selected { Color32::WHITE } else { TEXT },
    );
    let about = g.layout(
        tr("One software instrument track.").to_string(),
        FontId::proportional(10.5),
        DIM,
        r.width() - 14.0,
    );
    let at = pos2(r.center().x - about.size().x / 2.0, r.top() + 72.0);
    g.galley(at, about, DIM);

    (resp.clicked(), resp.double_clicked())
}

/// Draws the chooser over the whole window.
pub fn show(ui: &mut egui::Ui, s: &mut UiState) {
    let Some(mut c) = s.chooser.clone() else {
        return;
    };
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    let g = ui.painter().clone();

    // Standalone dialog window (or startup chooser) fills the frame 100% edge-to-edge
    let standalone = !s.has_path || (full.width() <= 920.0 && full.height() <= 620.0);
    let area = if standalone {
        full
    } else {
        let modal_w = 840.0f32.min(full.width() - 32.0);
        let modal_h = 540.0f32.min(full.height() - 32.0);
        g.rect_filled(full, 0.0, Color32::from_black_alpha(160));
        Rect::from_center_size(full.center(), vec2(modal_w, modal_h))
    };
    // Modal card with 12px rounded corners and 1px #2E3138 border
    let cr = 12.0;
    g.rect_filled(area, cr, BG);
    g.rect_stroke(
        area,
        cr,
        Stroke::new(1.0, Color32::from_rgb(46, 49, 56)),
        StrokeKind::Inside,
    );

    // Sidebar: width 170px with rounded left corners matching dialog card
    let side_w = 170.0f32;
    let side = Rect::from_min_size(area.min, vec2(side_w, area.height()));
    g.rect_filled(
        side,
        egui::CornerRadius {
            nw: 12,
            ne: 0,
            sw: 12,
            se: 0,
        },
        SIDEBAR,
    );
    g.line_segment(
        [side.right_top(), side.right_bottom()],
        Stroke::new(1.0, BORDER),
    );

    g.text(
        side.left_top() + vec2(18.0, 24.0),
        Align2::LEFT_CENTER,
        "Dōri",
        FontId::proportional(18.0),
        TEXT,
    );

    for (k, sec) in Section::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(
            side.left_top() + vec2(8.0, 52.0 + k as f32 * 32.0),
            vec2(side.width() - 16.0, 28.0),
        );
        let resp = ui.interact(r, egui::Id::unique(("chooser_section", k)), Sense::click());
        if c.section == sec {
            g.rect_filled(r, 6.0, ACCENT);
        } else if resp.hovered() {
            g.rect_filled(r, 6.0, Color32::from_white_alpha(10));
        }
        g.text(
            r.left_center() + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            sec.label(),
            FontId::proportional(12.5),
            TEXT,
        );
        if resp.clicked() && c.section != sec {
            c.section = sec;
            c.selected = 0;
        }
    }

    // Content area.
    let main = Rect::from_min_max(pos2(side.right(), area.top()), area.max);
    let footer_h = 50.0;
    let body = Rect::from_min_max(
        main.min + vec2(20.0, 16.0),
        pos2(main.right() - 20.0, main.bottom() - footer_h),
    );

    let recent: Vec<PathBuf> = s.recent.paths().to_vec();
    let mut chosen = None;
    let mut cancel = false;

    // Top drag handles for standalone borderless window
    if standalone {
        let side_drag_rect = Rect::from_min_size(side.left_top(), vec2(side.width(), 48.0));
        let s_drag = ui.interact(
            side_drag_rect,
            egui::Id::unique("chooser_side_drag"),
            Sense::drag(),
        );
        if s_drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let top_drag_rect = Rect::from_min_size(main.left_top(), vec2(main.width() - 40.0, 40.0));
        let t_drag = ui.interact(
            top_drag_rect,
            egui::Id::unique("chooser_top_drag"),
            Sense::drag(),
        );
        if t_drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }

    // Top-right clean close 'x' button
    let close_r = Rect::from_center_size(
        pos2(area.right() - 16.0, area.top() + 16.0),
        vec2(20.0, 20.0),
    );
    let close_resp = ui.interact(close_r, egui::Id::unique("chooser_close_x"), Sense::click());
    if close_resp.hovered() {
        g.circle_filled(close_r.center(), 10.0, Color32::from_white_alpha(20));
    }
    let stroke_color = if close_resp.hovered() {
        Color32::WHITE
    } else {
        Color32::from_rgb(0x9a, 0x9a, 0xa2)
    };
    let x_stroke = Stroke::new(1.2, stroke_color);
    g.line_segment(
        [
            close_r.center() + vec2(-4.0, -4.0),
            close_r.center() + vec2(4.0, 4.0),
        ],
        x_stroke,
    );
    g.line_segment(
        [
            close_r.center() + vec2(4.0, -4.0),
            close_r.center() + vec2(-4.0, 4.0),
        ],
        x_stroke,
    );
    if close_resp.clicked() {
        cancel = true;
    }

    match c.section {
        Section::NewProject => {
            let cols = if body.width() >= 640.0 { 4 } else { 2 };
            let tgap = 14.0f32;
            let tw =
                ((body.width() - (cols as f32 - 1.0) * tgap) / cols as f32).clamp(146.0, 190.0);
            let th = 114.0f32;

            // Header / Primary tile: Empty Project
            let top_card = Rect::from_min_size(body.left_top(), vec2(tw, th));
            let (clicked_empty, dbl_empty) = empty_project_card(ui, top_card, c.selected == 0);
            if clicked_empty {
                c.selected = 0;
            }
            if dbl_empty {
                chosen = Some(Choice::Template(Template::Empty));
            }

            // Factory Templates section
            let f_top = body.top() + th + 12.0;
            g.text(
                pos2(body.left(), f_top + 4.0),
                Align2::LEFT_TOP,
                tr("Factory Templates"),
                FontId::proportional(12.0),
                DIM,
            );

            let t_y = f_top + 22.0;
            for (k, t) in Template::FACTORY.iter().enumerate() {
                let col = k % cols;
                let row = k / cols;
                let r = Rect::from_min_size(
                    pos2(
                        body.left() + col as f32 * (tw + tgap),
                        t_y + row as f32 * (th + tgap),
                    ),
                    vec2(tw, th),
                );
                let (clicked, double) = tile(ui, r, *t, c.selected == k + 1, k + 1);
                if clicked {
                    c.selected = k + 1;
                }
                if double {
                    chosen = Some(Choice::Template(*t));
                }
            }

            // Collapsible Details section
            let num_rows = (Template::FACTORY.len() + cols - 1) / cols;
            let d_top = t_y + num_rows as f32 * th + (num_rows as f32 - 1.0).max(0.0) * tgap + 14.0;
            let d_rect =
                Rect::from_min_max(pos2(body.left(), d_top), pos2(body.right(), body.bottom()));
            let d_resp = ui.interact(
                Rect::from_min_size(pos2(d_rect.left(), d_rect.top()), vec2(100.0, 20.0)),
                egui::Id::unique("chooser_details_toggle"),
                Sense::click(),
            );
            if d_resp.clicked() {
                c.details.show_details = !c.details.show_details;
            }
            let glyph = if c.details.show_details { "▼" } else { "▶" };
            g.text(
                pos2(d_rect.left(), d_rect.top() + 10.0),
                Align2::LEFT_CENTER,
                format!("{glyph} {}", tr("Details")),
                FontId::proportional(12.5),
                TEXT,
            );

            if c.details.show_details {
                let inner = d_rect.translate(vec2(0.0, 24.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 6.0);

                    // Row 1: Grid, Tempo, Tap Tempo
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut c.details.use_musical_grid, tr("Use musical grid"));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(tr("Tempo:")).color(DIM));
                        ui.add(
                            egui::DragValue::new(&mut c.details.tempo)
                                .range(40.0..=300.0)
                                .speed(0.5),
                        );
                        if ui.button(tr("Tap Tempo")).clicked() {
                            c.details.tap();
                        }
                    });

                    // Row 2: Key Signature & Major/Minor & Time Signature
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Key Signature:")).color(DIM));
                        const KEYS: [&str; 12] = [
                            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                        ];
                        egui::ComboBox::from_id_salt("chooser_key")
                            .selected_text(KEYS[c.details.key_idx])
                            .show_ui(ui, |ui| {
                                for (i, k) in KEYS.iter().enumerate() {
                                    ui.selectable_value(&mut c.details.key_idx, i, *k);
                                }
                            });
                        ui.radio_value(&mut c.details.is_major, true, tr("Major"));
                        ui.radio_value(&mut c.details.is_major, false, tr("Minor"));

                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(tr("Time Signature:")).color(DIM));
                        const SIGS: [&str; 5] = ["4/4", "3/4", "2/4", "6/8", "12/8"];
                        egui::ComboBox::from_id_salt("chooser_sig")
                            .selected_text(SIGS[c.details.sig_idx])
                            .show_ui(ui, |ui| {
                                for (i, s) in SIGS.iter().enumerate() {
                                    ui.selectable_value(&mut c.details.sig_idx, i, *s);
                                }
                            });
                    });

                    // Row 3: Audio Devices
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Input Device:")).color(DIM));
                        let in_text = c
                            .details
                            .input_device
                            .clone()
                            .unwrap_or_else(|| tr("Default").to_string());
                        egui::ComboBox::from_id_salt("chooser_in_dev")
                            .selected_text(in_text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut c.details.input_device,
                                    None,
                                    tr("Default"),
                                );
                                for dev in &s.mixer.inputs.devices {
                                    ui.selectable_value(
                                        &mut c.details.input_device,
                                        Some(dev.name.clone()),
                                        &dev.name,
                                    );
                                }
                            });

                        ui.label(egui::RichText::new(tr("Output Device:")).color(DIM));
                        let out_text = c
                            .details
                            .output_device
                            .clone()
                            .unwrap_or_else(|| tr("Default").to_string());
                        egui::ComboBox::from_id_salt("chooser_out_dev")
                            .selected_text(out_text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut c.details.output_device,
                                    None,
                                    tr("Default"),
                                );
                                for dev in &s.audio_devices {
                                    ui.selectable_value(
                                        &mut c.details.output_device,
                                        Some(dev.name.clone()),
                                        &dev.name,
                                    );
                                }
                            });
                    });

                    // Row 4: Sample Rate, Frame Rate, Surround
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Sample Rate:")).color(DIM));
                        const RATES: [&str; 3] = ["44.1 kHz", "48 kHz", "96 kHz"];
                        egui::ComboBox::from_id_salt("chooser_sample_rate")
                            .selected_text(RATES[c.details.sample_rate_idx])
                            .show_ui(ui, |ui| {
                                for (i, r) in RATES.iter().enumerate() {
                                    ui.selectable_value(&mut c.details.sample_rate_idx, i, *r);
                                }
                            });

                        ui.label(egui::RichText::new(tr("Frame Rate:")).color(DIM));
                        const FRAMES: [&str; 5] =
                            ["23.976 fps", "24 fps", "25 fps", "29.97 fps", "30 fps"];
                        egui::ComboBox::from_id_salt("chooser_frame_rate")
                            .selected_text(FRAMES[c.details.frame_rate_idx])
                            .show_ui(ui, |ui| {
                                for (i, f) in FRAMES.iter().enumerate() {
                                    ui.selectable_value(&mut c.details.frame_rate_idx, i, *f);
                                }
                            });

                        ui.label(egui::RichText::new(tr("Surround Format:")).color(DIM));
                        const SURR: [&str; 2] = ["Stereo", "5.1 Surround"];
                        egui::ComboBox::from_id_salt("chooser_surround")
                            .selected_text(SURR[c.details.surround_idx])
                            .show_ui(ui, |ui| {
                                for (i, sr) in SURR.iter().enumerate() {
                                    ui.selectable_value(&mut c.details.surround_idx, i, *sr);
                                }
                            });
                    });
                });
            }
        }
        Section::Recent => {
            if recent.is_empty() {
                g.text(
                    pos2(body.left(), body.top() + 20.0),
                    Align2::LEFT_TOP,
                    tr("No recent projects."),
                    FontId::proportional(13.0),
                    DIM,
                );
            }
            for (k, path) in recent.iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(body.left(), body.top() + k as f32 * 40.0),
                    vec2(body.width(), 36.0),
                );
                let resp = ui.interact(r, egui::Id::unique(("chooser_recent", k)), Sense::click());
                if c.selected == k {
                    g.rect_filled(r, 6.0, ACCENT);
                } else if resp.hovered() {
                    g.rect_filled(r, 6.0, Color32::from_white_alpha(10));
                }
                let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("?");
                let folder = path.parent().map(|d| d.display().to_string());
                g.text(
                    r.left_center() + vec2(12.0, -7.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(13.5),
                    TEXT,
                );
                g.text(
                    r.left_center() + vec2(12.0, 9.0),
                    Align2::LEFT_CENTER,
                    folder.unwrap_or_default(),
                    FontId::proportional(11.0),
                    if c.selected == k { TEXT } else { DIM },
                );
                if resp.clicked() {
                    c.selected = k;
                }
                if resp.double_clicked() {
                    chosen = Some(Choice::Recent(path.clone()));
                }
            }
        }
        Section::DemoProjects | Section::MyTemplates => {
            g.text(
                pos2(body.left(), body.top() + 20.0),
                Align2::LEFT_TOP,
                format!("No {} found.", c.section.label()),
                FontId::proportional(13.0),
                DIM,
            );
        }
    }

    // Footer: Open… on the left; Cancel and Choose on the right.
    let footer = Rect::from_min_max(pos2(side.right(), area.bottom() - footer_h), area.max);
    g.line_segment(
        [footer.left_top(), footer.right_top()],
        Stroke::new(1.0, BORDER),
    );
    let footer_bg = Rect::from_min_max(pos2(side.right(), area.bottom() - footer_h), area.max);
    g.rect_filled(
        footer_bg,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: 0,
            se: 12,
        },
        SIDEBAR,
    );
    // Ensure the 1px #2E3138 outer border is crisp over all edges
    g.rect_stroke(
        area,
        12.0,
        Stroke::new(1.0, Color32::from_rgb(46, 49, 56)),
        StrokeKind::Inside,
    );

    let footer_ui = footer.shrink2(vec2(20.0, 10.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(footer_ui), |ui| {
        ui.horizontal_centered(|ui| {
            if ui.button(tr("Open an existing project…")).clicked() {
                s.file_cmd = Some(FileCmd::Open);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let can_choose = choice(&c, &recent).is_some();
                let choose_btn = egui::Button::new(
                    egui::RichText::new(tr("Choose"))
                        .color(if can_choose {
                            Color32::from_rgb(0x11, 0x12, 0x15)
                        } else {
                            Color32::from_rgb(0x80, 0x82, 0x8a)
                        })
                        .strong(),
                )
                .fill(if can_choose {
                    Color32::from_rgb(0xf2, 0xf3, 0xf5)
                } else {
                    Color32::from_rgb(0x32, 0x33, 0x38)
                })
                .stroke(Stroke::new(
                    1.0,
                    if can_choose {
                        Color32::from_rgb(0xe0, 0xe2, 0xe8)
                    } else {
                        Color32::from_rgb(0x40, 0x41, 0x46)
                    },
                ))
                .corner_radius(6.0)
                .min_size(vec2(80.0, 26.0));
                if ui.add_enabled(can_choose, choose_btn).clicked() {
                    chosen = choice(&c, &recent);
                }
                let cancel_btn = egui::Button::new(tr("Cancel"))
                    .corner_radius(6.0)
                    .min_size(vec2(80.0, 26.0));
                if ui.add(cancel_btn).clicked() {
                    cancel = true;
                }
            });
        });
    });

    // Return chooses, Escape goes back to the open project.
    if !ui.ctx().egui_wants_keyboard_input() {
        if ui.input(|i| i.key_pressed(Key::Enter)) {
            chosen = chosen.or_else(|| choice(&c, &recent));
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
    }

    if cancel {
        s.chooser = None;
        if !s.has_path {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        return;
    }
    s.chooser = Some(c.clone());
    if let Some(choice) = chosen {
        choose(choice, &c, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_start_untitled_projects_with_their_tracks() {
        for t in Template::ALL {
            let p = t.project();
            assert_eq!(p.name, "Untitled", "{t:?}");
            assert!(!p.tracks.is_empty(), "{t:?}");
            assert_eq!(p.selected_track, 0);
        }
        assert_eq!(Template::Empty.project().tracks.len(), 1);
        let kinds: Vec<_> = Template::Electronic
            .project()
            .tracks
            .iter()
            .map(|t| t.kind)
            .collect();
        assert!(kinds.contains(&TrackKind::Aux) && kinds.contains(&TrackKind::Drummer));
    }

    #[test]
    fn choosing_queues_the_matching_file_command() {
        let recent = vec![PathBuf::from("/music/Night Drive.prowave")];
        let mut c = Chooser::templates();
        c.selected = 1;
        assert_eq!(
            choice(&c, &recent),
            Some(Choice::Template(Template::HipHop))
        );
        c.section = Section::Recent;
        c.selected = 0;
        let mut s = UiState::new();
        choose(choice(&c, &recent).unwrap(), &c, &mut s);
        assert_eq!(s.file_cmd, Some(FileCmd::OpenRecent));
        assert_eq!(s.recent_pick, Some(recent[0].clone()));

        c.selected = 3;
        assert_eq!(choice(&c, &recent), None, "nothing to choose past the list");
        c.section = Section::NewProject;
        c.selected = 0;
        choose(choice(&c, &[]).unwrap(), &c, &mut s);
        assert_eq!(s.file_cmd, Some(FileCmd::NewFrom(Template::Empty)));
    }
}
