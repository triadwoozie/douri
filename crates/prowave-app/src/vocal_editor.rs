//! Musical Vocal Editor (`vocal_editor.rs`).
//!
//! Provides the primary note, pitch curve, and expression editing interface for Vocal / UTAU
//! tracks in Dōri. Rendered strictly in Dōri's Liquid Glass design system with 100% phosphor-egui
//! icons, real interactive piano keyboard (C1-C6), full parameter lane (PITD, Velocity, DYN, etc.),
//! ML Auto Tune contour prediction, and floating Singer Inspector popover card.

use crate::i18n::tr;
use crate::liquid_glass;
use crate::lyrics::Lyric;
use crate::model::{Note, Project, TrackKind};
use crate::piano_roll::Audition;
use crate::tracks_area::Snap;
use crate::vocal::{Singer, VocalRole, VocalSettings};
use crate::widgets::{context_menu, BLUE, TEXT, TEXT_DIM};
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Key, Painter, Pos2, Rect, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};
use std::collections::HashSet;

pub const TOOLBAR_H: f32 = 32.0;
pub const EVENT_BAR_H: f32 = 28.0;
pub const TAB_H: f32 = 32.0;
pub const KEYS_W: f32 = 52.0;
pub const RULER_H: f32 = 20.0;
pub const PARAM_LANE_H: f32 = 120.0;

// Liquid Glass Theme Colors
pub const TOOLBAR_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x24);
pub const WELL_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
pub const ROW_WHITE: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x22);
pub const ROW_BLACK: Color32 = Color32::from_rgb(0x16, 0x18, 0x1c);
pub const BORDER_COLOR: Color32 = Color32::from_rgb(0x2e, 0x31, 0x38);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x2b, 0x6c, 0xb0);
pub const ACCENT_BLUE_LIGHT: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);
pub const GRID_LINE_BAR: Color32 = Color32::from_rgb(0x42, 0x46, 0x50);
pub const GRID_LINE_BEAT: Color32 = Color32::from_rgb(0x28, 0x2b, 0x32);
pub const PITCH_CURVE_ORANGE: Color32 = Color32::from_rgb(0xff, 0x9f, 0x0a);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VocalTool {
    Arrow,
    Pencil,
    Line,
    Scissors,
    Eraser,
    Vibrato,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VocalParamType {
    Pitd,
    Velocity,
    Dyn,
    Vel,
    Vol,
    Atk,
    Dec,
    Genc,
    Brec,
    VibratoDepth,
    VibratoRate,
}

impl VocalParamType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pitd => "PITD (Pitch Cent)",
            Self::Velocity => "Velocity",
            Self::Dyn => "DYN (Dynamics)",
            Self::Vel => "VEL (Consonant)",
            Self::Vol => "VOL (Volume)",
            Self::Atk => "ATK (Attack)",
            Self::Dec => "DEC (Decay)",
            Self::Genc => "GENC (Gender)",
            Self::Brec => "BREC (Breath)",
            Self::VibratoDepth => "Vibrato Depth",
            Self::VibratoRate => "Vibrato Rate",
        }
    }

    pub const ALL: [Self; 11] = [
        Self::Pitd,
        Self::Velocity,
        Self::Dyn,
        Self::Vel,
        Self::Vol,
        Self::Atk,
        Self::Dec,
        Self::Genc,
        Self::Brec,
        Self::VibratoDepth,
        Self::VibratoRate,
    ];
}

#[derive(Clone, Debug)]
pub struct VocalView {
    pub tool: VocalTool,
    pub beat_px: f32,
    pub key_h: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub selected: HashSet<usize>,
    pub editing_lyric: Option<usize>,
    pub lyric_buffer: String,
    pub acoustic_feedback: bool,
    pub catch_playhead: bool,
    pub show_inspector: bool,
    pub show_parameters: bool,
    pub param_type: VocalParamType,
    pub held: Option<u8>,
    pub vel_drag: Option<usize>,
    pub dirty: bool,
    pub custom_pitch: Vec<(f32, f32)>, // (beat, pitch_delta_cents)
    drag_state: DragState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragState {
    None,
    MoveNotes {
        anchor_beat: f32,
        anchor_pitch: i32,
    },
    ResizeNoteRight {
        index: usize,
        anchor_beat: f32,
        orig_len: f32,
    },
    ResizeNoteLeft {
        index: usize,
        anchor_beat: f32,
        orig_start: f32,
        orig_len: f32,
    },
    DrawPitchCurve,
    ErasePitchCurve,
}

impl Default for VocalView {
    fn default() -> Self {
        Self {
            tool: VocalTool::Arrow,
            beat_px: 60.0,
            key_h: 18.0,
            scroll_x: 0.0,
            scroll_y: 50.0 * 18.0, // center around middle C (C3-C4)
            selected: HashSet::new(),
            editing_lyric: None,
            lyric_buffer: String::new(),
            acoustic_feedback: true,
            catch_playhead: true,
            show_inspector: false,
            show_parameters: true,
            param_type: VocalParamType::Pitd,
            held: None,
            vel_drag: None,
            dirty: false,
            custom_pitch: Vec::new(),
            drag_state: DragState::None,
        }
    }
}

fn note_name(pitch: i32) -> &'static str {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let idx = (pitch % 12 + 12) % 12;
    NAMES[idx as usize]
}

fn is_black(pitch: i32) -> bool {
    matches!((pitch % 12 + 12) % 12, 1 | 3 | 6 | 8 | 10)
}

fn c_label(pitch: i32) -> String {
    let octave = pitch / 12 - 2;
    format!("{}{}", note_name(pitch), octave)
}

fn phoneme_alias(kana: &str) -> &'static str {
    match kana {
        "あ" | "a" => "a",
        "い" | "i" => "i",
        "う" | "u" => "u",
        "え" | "e" => "e",
        "お" | "o" => "o",
        "か" | "ka" => "k a",
        "き" | "ki" => "k' i",
        "く" | "ku" => "k u",
        "け" | "ke" => "k e",
        "こ" | "ko" => "k o",
        "さ" | "sa" => "s a",
        "し" | "shi" | "si" => "sh i",
        "す" | "su" => "s u",
        "せ" | "se" => "s e",
        "そ" | "so" => "s o",
        "た" | "ta" => "t a",
        "ち" | "chi" | "ti" => "ch i",
        "つ" | "tsu" | "tu" => "ts u",
        "て" | "te" => "t e",
        "と" | "to" => "t o",
        "な" | "na" => "n a",
        "に" | "ni" => "n' i",
        "ぬ" | "nu" => "n u",
        "ね" | "ne" => "n e",
        "の" | "no" => "n o",
        "は" | "ha" => "h a",
        "ひ" | "hi" => "h' i",
        "ふ" | "fu" | "hu" => "f u",
        "へ" | "he" => "h e",
        "ほ" | "ho" => "h o",
        "ま" | "ma" => "m a",
        "み" | "mi" => "m' i",
        "む" | "mu" => "m u",
        "め" | "me" => "m e",
        "も" | "mo" => "m o",
        "ら" | "ra" => "r a",
        "り" | "ri" => "r' i",
        "る" | "ru" => "r u",
        "れ" | "re" => "r e",
        "ろ" | "ro" => "r o",
        "わ" | "wa" => "w a",
        "を" | "wo" => "o",
        "ん" | "n" => "n",
        "-" => "-",
        _ => "",
    }
}

fn target_vocal_region(p: &Project, s: &UiState) -> Option<(usize, usize)> {
    if let Some((ti, ri)) = s.tracks.selected {
        if ti < p.tracks.len() && ri < p.tracks[ti].regions.len() && p.tracks[ti].vocal.is_some() {
            return Some((ti, ri));
        }
    }
    let sel_ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
    if sel_ti < p.tracks.len()
        && p.tracks[sel_ti].vocal.is_some()
        && !p.tracks[sel_ti].regions.is_empty()
    {
        return Some((sel_ti, 0));
    }
    p.tracks
        .iter()
        .enumerate()
        .find(|(_, t)| t.vocal.is_some() && !t.regions.is_empty())
        .map(|(ti, _)| (ti, 0))
}

/// Standalone entry point when opening the Vocal Editor.
pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());

    let target = target_vocal_region(p, s);
    let (ti, ri) = match target {
        Some(t) => t,
        None => {
            let g = ui.painter_at(area);
            g.rect_filled(area, 0.0, WELL_BG);
            let msg = "Select a Vocal Track with a MIDI region to edit vocal notes and lyrics";
            g.text(
                area.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(14.0),
                TEXT_DIM,
            );
            return;
        }
    };

    show_in_rect(ui, p, s, ti, ri, area);
}

/// Renders the full Musical Vocal Editor inside the provided rect (called directly by Piano Roll tab when track is vocal).
pub fn show_in_rect(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    ri: usize,
    body: Rect,
) {
    let g = ui.painter_at(body);
    g.rect_filled(body, 0.0, WELL_BG);

    // 1. Top Event Inspector Bar (Start, End, Length, Pitch, Velocity, Text)
    let event_bar = Rect::from_min_size(body.min, vec2(body.width(), EVENT_BAR_H));
    event_inspector_bar(ui, p, s, ti, ri, event_bar);

    // 2. Musical Editor Toolbar (100% phosphor-egui icons, ML Auto Tune button)
    let tool_bar = Rect::from_min_size(
        pos2(body.left(), event_bar.bottom()),
        vec2(body.width(), TOOLBAR_H),
    );
    vocal_toolbar(ui, p, s, ti, ri, tool_bar);

    // Main workspace below toolbars
    let work_area = Rect::from_min_max(pos2(body.left(), tool_bar.bottom()), body.max);

    // Optional Collapsible Parameter Lane at bottom
    let param_lane_rect = if s.vocal_ed.show_parameters {
        Rect::from_min_max(
            pos2(work_area.left(), work_area.bottom() - PARAM_LANE_H),
            work_area.max,
        )
    } else {
        Rect::from_min_max(work_area.left_bottom(), work_area.max)
    };

    let editor_body = Rect::from_min_max(
        work_area.min,
        pos2(work_area.right(), param_lane_rect.top()),
    );

    // Canvas spans the full width of editor body (piano keys on left, note grid right)
    let canvas_rect = editor_body;

    // Interactive Piano Keyboard + Note Canvas
    vocal_canvas(ui, p, s, ti, ri, canvas_rect);

    // Floating Popover Card for Singer Inspector when toggled
    if s.vocal_ed.show_inspector {
        singer_inspector_popover(ui, p, s, ti, canvas_rect);
    }

    // Synced Bottom Control Parameters Lane
    if s.vocal_ed.show_parameters {
        vocal_parameter_lane(
            ui,
            p,
            s,
            ti,
            ri,
            param_lane_rect,
            canvas_rect.left() + KEYS_W,
        );
    }

    // Trigger background synthesis if edits were made
    if s.vocal_ed.dirty {
        s.vocal_ed.dirty = false;
        p.song_rev += 1;
        if let Some(VocalRole::Score(set)) = &p.tracks[ti].vocal {
            let link = set.link;
            crate::vocal::trigger_render(s, link);
        }
    }
}

/// Renders the Top Event Inspector Bar with Start, End, Length, Pitch, Velocity, and Lyric Text fields.
fn event_inspector_bar(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    ri: usize,
    bar: Rect,
) {
    let g = ui.painter_at(bar);
    g.rect_filled(bar, 0.0, TOOLBAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let region = &p.tracks[ti].regions[ri];
    let selected_note = s
        .vocal_ed
        .selected
        .iter()
        .min()
        .copied()
        .and_then(|idx| region.notes.get(idx).cloned().map(|n| (idx, n)));

    let mut x = bar.left() + 12.0;
    let cy = bar.center().y;

    let font_label = FontId::proportional(10.0);
    let font_val = FontId::monospace(11.5);

    let draw_cell = |ui: &mut Ui,
                     g: &Painter,
                     label: &str,
                     val: &str,
                     w: f32,
                     cur_x: &mut f32|
     -> (Rect, egui::Response) {
        let r = Rect::from_min_size(pos2(*cur_x, bar.top() + 3.0), vec2(w, bar.height() - 6.0));
        let resp = ui.interact(
            r,
            egui::Id::unique(("event_bar_cell", *cur_x as i32)),
            Sense::click_and_drag(),
        );
        if resp.hovered() {
            g.rect_filled(r, 3.0, Color32::from_white_alpha(12));
        }
        g.text(
            pos2(r.left() + 4.0, cy - 6.0),
            Align2::LEFT_CENTER,
            label,
            font_label.clone(),
            TEXT_DIM,
        );
        g.text(
            pos2(r.left() + 4.0, cy + 5.0),
            Align2::LEFT_CENTER,
            val,
            font_val.clone(),
            Color32::WHITE,
        );
        g.line_segment(
            [
                pos2(r.right(), bar.top() + 5.0),
                pos2(r.right(), bar.bottom() - 5.0),
            ],
            Stroke::new(1.0, BORDER_COLOR),
        );
        *cur_x += w + 6.0;
        (r, resp)
    };

    if let Some((idx, note)) = selected_note {
        let start_beat = p.bar_to_beats(region.start_bar as f64) as f32;
        let (sb, sbt, sd, stk) = p.bbdt((start_beat + note.start) as f64);
        let start_str = format!("{sb}.{sbt}.{sd}.{stk}");
        let (eb, ebt, ed, etk) = p.bbdt((start_beat + note.start + note.len) as f64);
        let end_str = format!("{eb}.{ebt}.{ed}.{etk}");
        let len_ticks = (note.len * 480.0) as u32;
        let len_str = format!("{len_ticks} tks");
        let pitch_str = c_label(note.pitch as i32);
        let vel_str = format!("{}", note.vel);
        let lyric_str = note.lyric.as_str();
        let phoneme_str = phoneme_alias(lyric_str);
        let text_str = if phoneme_str.is_empty() {
            lyric_str.to_string()
        } else {
            format!("{lyric_str} [{phoneme_str}]")
        };

        let (_, start_resp) = draw_cell(ui, &g, "START", &start_str, 95.0, &mut x);
        if start_resp.dragged_by(egui::PointerButton::Primary) {
            let delta = -start_resp.drag_delta().y * 0.05;
            let reg_mut = &mut p.tracks[ti].regions[ri];
            reg_mut.notes[idx].start = (reg_mut.notes[idx].start + delta).max(0.0);
            s.vocal_ed.dirty = true;
        }

        let (_, _) = draw_cell(ui, &g, "END", &end_str, 95.0, &mut x);

        let (_, len_resp) = draw_cell(ui, &g, "LENGTH", &len_str, 75.0, &mut x);
        if len_resp.dragged_by(egui::PointerButton::Primary) {
            let delta = -len_resp.drag_delta().y * 0.05;
            let reg_mut = &mut p.tracks[ti].regions[ri];
            reg_mut.notes[idx].len = (reg_mut.notes[idx].len + delta).max(0.05);
            s.vocal_ed.dirty = true;
        }

        let (_, pitch_resp) = draw_cell(ui, &g, "PITCH", &pitch_str, 60.0, &mut x);
        if pitch_resp.dragged_by(egui::PointerButton::Primary) {
            let delta = (-pitch_resp.drag_delta().y * 0.2) as i32;
            if delta != 0 {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                let new_p = (reg_mut.notes[idx].pitch as i32 + delta).clamp(12, 110) as u8;
                reg_mut.notes[idx].pitch = new_p;
                s.vocal_ed.dirty = true;
            }
        }

        let (_, vel_resp) = draw_cell(ui, &g, "VELOCITY", &vel_str, 65.0, &mut x);
        if vel_resp.dragged_by(egui::PointerButton::Primary) {
            let delta = (-vel_resp.drag_delta().y * 0.5) as i32;
            if delta != 0 {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                let new_v = (reg_mut.notes[idx].vel as i32 + delta).clamp(1, 127) as u8;
                reg_mut.notes[idx].vel = new_v;
                s.vocal_ed.dirty = true;
            }
        }

        // Lyric text input
        let text_rect =
            Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(160.0, bar.height() - 6.0));
        g.text(
            pos2(text_rect.left() + 4.0, cy - 6.0),
            Align2::LEFT_CENTER,
            "LYRIC / PHONEME",
            font_label.clone(),
            TEXT_DIM,
        );
        g.text(
            pos2(text_rect.left() + 4.0, cy + 5.0),
            Align2::LEFT_CENTER,
            &text_str,
            font_val.clone(),
            Color32::WHITE,
        );
    } else {
        g.text(
            pos2(x, cy),
            Align2::LEFT_CENTER,
            "No note selected (Click a note to inspect and edit)",
            font_label,
            TEXT_DIM,
        );
    }
}

/// Applies phrase-aware ML Auto Tune contour prediction across the selected notes (or whole phrase).
fn apply_ml_autotune(p: &mut Project, s: &mut UiState, ti: usize, ri: usize) {
    let bpm = p.tempo;
    let singer_dir = if let Some(VocalRole::Score(set)) = &p.tracks[ti].vocal {
        if !set.voice.is_empty() {
            Some(crate::vocal::vocal_home().join(&set.voice))
        } else {
            None
        }
    } else {
        None
    };

    let reg = &p.tracks[ti].regions[ri];
    if reg.notes.is_empty() {
        return;
    }

    let ustx_notes: Vec<prowave_vocal::UstxNote> = reg
        .notes
        .iter()
        .map(|n| prowave_vocal::UstxNote {
            position: (n.start * 480.0).round() as i32,
            duration: (n.len * 480.0).round() as i32,
            tone: n.pitch as i32,
            lyric: n.lyric.as_str().to_string(),
            pitch: None,
            vibrato: None,
        })
        .collect();

    let pred =
        prowave_vocal::autotune::predict_contours(&ustx_notes, bpm as f64, singer_dir.as_deref());

    let (span_start_tick, span_end_tick) = if !s.vocal_ed.selected.is_empty() {
        let mut min_t = i32::MAX;
        let mut max_t = i32::MIN;
        for &idx in &s.vocal_ed.selected {
            if let Some(n) = reg.notes.get(idx) {
                let st = (n.start * 480.0).round() as i32;
                let et = st + (n.len * 480.0).round() as i32;
                min_t = min_t.min(st);
                max_t = max_t.max(et);
            }
        }
        (min_t, max_t)
    } else {
        (
            0,
            ((reg.len_bars * p.bar_len_at(p.region_start_beats(reg)) as f32) * 480.0).round()
                as i32,
        )
    };

    let existing_pitd: Vec<prowave_vocal::CurvePoint> = s
        .vocal_ed
        .custom_pitch
        .iter()
        .map(|(b, cents)| prowave_vocal::CurvePoint {
            tick: (b * 480.0).round() as i32,
            val: cents.round() as i32,
        })
        .collect();

    let merged = prowave_vocal::autotune::merge_curve_span(
        &existing_pitd,
        &pred.pitd,
        span_start_tick,
        span_end_tick,
    );

    s.vocal_ed.custom_pitch = merged
        .into_iter()
        .map(|pt| (pt.tick as f32 / 480.0, pt.val as f32))
        .collect();

    s.vocal_ed.dirty = true;
}

/// Renders the primary Musical Editor Toolbar with 100% phosphor-egui icons.
fn vocal_toolbar(ui: &mut Ui, p: &mut Project, s: &mut UiState, ti: usize, ri: usize, bar: Rect) {
    let g = ui.painter_at(bar);
    g.rect_filled(bar, 0.0, TOOLBAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let cy = bar.center().y;
    let mut x = bar.left() + 10.0;

    // Tool Selector Group (Pointer, Pencil, Line, Scissors, Eraser, Vibrato)
    let tools = [
        (
            VocalTool::Arrow,
            phosphor_egui::regular::CURSOR,
            "Arrow Tool (1)",
        ),
        (
            VocalTool::Pencil,
            phosphor_egui::regular::PENCIL_SIMPLE,
            "Pencil Tool (2)",
        ),
        (
            VocalTool::Line,
            phosphor_egui::regular::LINE_SEGMENT,
            "Line Tool",
        ),
        (
            VocalTool::Scissors,
            phosphor_egui::regular::SCISSORS,
            "Scissors Tool (3)",
        ),
        (
            VocalTool::Eraser,
            phosphor_egui::regular::ERASER,
            "Eraser Tool (4)",
        ),
        (
            VocalTool::Vibrato,
            phosphor_egui::regular::WAVE_SINE,
            "Vibrato Tool (5)",
        ),
    ];

    let btn_w = 28.0;
    let btn_h = 24.0;
    let pill_rect = Rect::from_min_size(
        pos2(x, cy - btn_h / 2.0),
        vec2(btn_w * tools.len() as f32, btn_h),
    );
    liquid_glass::paint_glass_pill(ui, pill_rect, 4.0, s.glass_quality, false, None, None);

    for (tool, icon, tip) in tools {
        let r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
        let active = s.vocal_ed.tool == tool;
        let resp = ui
            .interact(
                r,
                egui::Id::unique(("vocal_tool", tool as u8)),
                Sense::click(),
            )
            .on_hover_text(tip);

        if active {
            g.rect_filled(r, 4.0, ACCENT_BLUE);
        } else if resp.hovered() {
            g.rect_filled(r, 4.0, Color32::from_white_alpha(20));
        }
        g.text(
            r.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(14.0),
            if active { Color32::WHITE } else { TEXT_DIM },
        );
        if resp.clicked() {
            s.vocal_ed.tool = tool;
        }
        x += btn_w;
    }

    x += 12.0;
    g.line_segment(
        [pos2(x, bar.top() + 6.0), pos2(x, bar.bottom() - 6.0)],
        Stroke::new(1.0, BORDER_COLOR),
    );
    x += 12.0;

    // Grid Snap Toggle & Division Dropdown
    let snap_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let snap_active = s.tracks.snap == Snap::Division;
    let snap_resp = ui
        .interact(
            snap_r,
            egui::Id::unique("vocal_snap_toggle"),
            Sense::click(),
        )
        .on_hover_text("Toggle Grid Snap");
    if snap_active {
        g.rect_filled(snap_r, 4.0, ACCENT_BLUE);
    } else if snap_resp.hovered() {
        g.rect_filled(snap_r, 4.0, Color32::from_white_alpha(20));
    }
    g.text(
        snap_r.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::GRID_FOUR,
        FontId::proportional(14.0),
        if snap_active {
            Color32::WHITE
        } else {
            TEXT_DIM
        },
    );
    if snap_resp.clicked() {
        s.tracks.snap = if snap_active {
            Snap::Beat
        } else {
            Snap::Division
        };
    }
    x += btn_w + 6.0;

    // Confirmation Sound (Acoustic Feedback audition toggle)
    let feedback_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let feedback_resp = ui
        .interact(
            feedback_r,
            egui::Id::unique("vocal_feedback_toggle"),
            Sense::click(),
        )
        .on_hover_text("Acoustic Feedback: audition sound on clicks and edits");
    if s.vocal_ed.acoustic_feedback {
        g.rect_filled(feedback_r, 4.0, ACCENT_BLUE);
    } else if feedback_resp.hovered() {
        g.rect_filled(feedback_r, 4.0, Color32::from_white_alpha(20));
    }
    g.text(
        feedback_r.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::SPEAKER_HIGH,
        FontId::proportional(14.0),
        if s.vocal_ed.acoustic_feedback {
            Color32::WHITE
        } else {
            TEXT_DIM
        },
    );
    if feedback_resp.clicked() {
        s.vocal_ed.acoustic_feedback = !s.vocal_ed.acoustic_feedback;
    }
    x += btn_w + 6.0;

    // Catch Playhead toggle
    let catch_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let catch_resp = ui
        .interact(
            catch_r,
            egui::Id::unique("vocal_catch_toggle"),
            Sense::click(),
        )
        .on_hover_text("Catch Playhead: auto-scroll view during playback");
    if s.vocal_ed.catch_playhead {
        g.rect_filled(catch_r, 4.0, ACCENT_BLUE);
    } else if catch_resp.hovered() {
        g.rect_filled(catch_r, 4.0, Color32::from_white_alpha(20));
    }
    g.text(
        catch_r.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::CURSOR_CLICK,
        FontId::proportional(14.0),
        if s.vocal_ed.catch_playhead {
            Color32::WHITE
        } else {
            TEXT_DIM
        },
    );
    if catch_resp.clicked() {
        s.vocal_ed.catch_playhead = !s.vocal_ed.catch_playhead;
    }
    x += btn_w + 6.0;

    // Singer Inspector Floating Popover Toggle
    let insp_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let insp_resp = ui
        .interact(
            insp_r,
            egui::Id::unique("vocal_insp_toggle"),
            Sense::click(),
        )
        .on_hover_text("Toggle Singer / Part Inspector Card");
    if s.vocal_ed.show_inspector {
        g.rect_filled(insp_r, 4.0, ACCENT_BLUE);
    } else if insp_resp.hovered() {
        g.rect_filled(insp_r, 4.0, Color32::from_white_alpha(20));
    }
    g.text(
        insp_r.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::SLIDERS,
        FontId::proportional(14.0),
        if s.vocal_ed.show_inspector {
            Color32::WHITE
        } else {
            TEXT_DIM
        },
    );
    if insp_resp.clicked() {
        s.vocal_ed.show_inspector = !s.vocal_ed.show_inspector;
    }
    x += btn_w + 12.0;

    // ML Auto Tune action button
    let tune_w = 110.0;
    let tune_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(tune_w, btn_h));
    let tune_resp = ui
        .interact(
            tune_r,
            egui::Id::unique("vocal_ml_autotune_btn"),
            Sense::click(),
        )
        .on_hover_text("ML Auto Tune: Predict phrase-aware pitch and dynamics contours");
    if tune_resp.hovered() {
        g.rect_filled(tune_r, 4.0, Color32::from_white_alpha(20));
    } else {
        g.rect_filled(tune_r, 4.0, Color32::from_rgb(0x20, 0x24, 0x2c));
    }
    g.rect_stroke(
        tune_r,
        4.0,
        Stroke::new(1.0, BORDER_COLOR),
        StrokeKind::Inside,
    );
    g.text(
        pos2(tune_r.left() + 14.0, cy),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::SPARKLE,
        FontId::proportional(13.0),
        Color32::from_rgb(0x3a, 0x7b, 0xd5),
    );
    g.text(
        pos2(tune_r.left() + 28.0, cy),
        Align2::LEFT_CENTER,
        "ML Auto Tune",
        FontId::proportional(11.0),
        Color32::WHITE,
    );
    if tune_resp.clicked() {
        apply_ml_autotune(p, s, ti, ri);
    }
}

/// Floating glass popover card over the canvas displaying Singer profile and macro sliders.
fn singer_inspector_popover(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    canvas_rect: Rect,
) {
    let card_w = 260.0;
    let card_h = 280.0;
    let card_rect = Rect::from_min_size(
        pos2(canvas_rect.left() + KEYS_W + 16.0, canvas_rect.top() + 12.0),
        vec2(card_w, card_h),
    );

    let g = ui.painter_at(card_rect);
    // Dark obsidian glass background with border and shadow
    g.rect_filled(card_rect, 8.0, Color32::from_rgb(0x18, 0x1a, 0x1f));
    g.rect_stroke(
        card_rect,
        8.0,
        Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3d, 0x46)),
        StrokeKind::Inside,
    );

    // Header with title and close button
    let cy_top = card_rect.top() + 16.0;
    g.text(
        pos2(card_rect.left() + 12.0, cy_top),
        Align2::LEFT_CENTER,
        "SINGER / PART INSPECTOR",
        FontId::proportional(11.0),
        Color32::from_rgb(0x3a, 0x7b, 0xd5),
    );

    let close_rect = Rect::from_min_size(
        pos2(card_rect.right() - 24.0, card_rect.top() + 6.0),
        vec2(18.0, 18.0),
    );
    let close_resp = ui.interact(
        close_rect,
        egui::Id::unique("vocal_insp_close"),
        Sense::click(),
    );
    if close_resp.hovered() {
        g.rect_filled(close_rect, 3.0, Color32::from_white_alpha(20));
    }
    g.text(
        close_rect.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::X,
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    if close_resp.clicked() {
        s.vocal_ed.show_inspector = false;
    }

    let Some(VocalRole::Score(set)) = p.tracks[ti].vocal.as_mut() else {
        g.text(
            pos2(card_rect.left() + 12.0, cy_top + 24.0),
            Align2::LEFT_CENTER,
            "Track is not a vocal score track",
            FontId::proportional(11.0),
            TEXT_DIM,
        );
        return;
    };

    let mut y = cy_top + 24.0;

    // Voicebank display
    g.text(
        pos2(card_rect.left() + 12.0, y),
        Align2::LEFT_CENTER,
        "Voicebank:",
        FontId::proportional(11.0),
        TEXT_DIM,
    );
    let voice_label = if set.voice.is_empty() {
        "Default Voice"
    } else {
        &set.voice
    };
    g.text(
        pos2(card_rect.left() + 85.0, y),
        Align2::LEFT_CENTER,
        voice_label,
        FontId::proportional(11.5),
        Color32::from_rgb(0xff, 0xd6, 0x0a),
    );
    y += 24.0;

    // 4 Primary Macro Sliders: EXP, AIR, CHA, ATK
    let mut draw_knob =
        |label: &str, tip: &str, val: &mut f32, min_v: f32, max_v: f32, cur_y: &mut f32| {
            let label_rect =
                Rect::from_min_size(pos2(card_rect.left() + 12.0, *cur_y), vec2(40.0, 18.0));
            g.text(
                pos2(label_rect.left(), label_rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(11.0),
                Color32::WHITE,
            );

            let slider_rect = Rect::from_min_size(
                pos2(card_rect.left() + 55.0, *cur_y + 2.0),
                vec2(130.0, 14.0),
            );
            let resp = ui
                .interact(
                    slider_rect,
                    egui::Id::unique(("insp_param", *cur_y as i32)),
                    Sense::drag(),
                )
                .on_hover_text(tip);

            // Groove rail background
            g.rect_filled(slider_rect, 3.0, Color32::from_rgb(0x16, 0x18, 0x1c));
            g.rect_stroke(
                slider_rect,
                3.0,
                Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38)),
                StrokeKind::Inside,
            );

            let t = ((*val - min_v) / (max_v - min_v)).clamp(0.0, 1.0);
            let fill_w = slider_rect.width() * t;
            if fill_w > 0.0 {
                let fill_r =
                    Rect::from_min_size(slider_rect.min, vec2(fill_w, slider_rect.height()));
                g.rect_filled(fill_r, 3.0, ACCENT_BLUE);
            }

            if resp.dragged_by(egui::PointerButton::Primary) {
                let delta = resp.drag_delta().x / slider_rect.width() * (max_v - min_v);
                *val = (*val + delta).clamp(min_v, max_v);
                s.vocal_ed.dirty = true;
            }

            let val_text = format!("{:.0}", *val);
            g.text(
                pos2(card_rect.left() + 195.0, *cur_y + 8.0),
                Align2::LEFT_CENTER,
                &val_text,
                FontId::monospace(10.5),
                TEXT_DIM,
            );

            *cur_y += 22.0;
        };

    draw_knob(
        "EXP",
        "Dynamics / Tension (-100..100)",
        &mut set.tension,
        -100.0,
        100.0,
        &mut y,
    );
    draw_knob(
        "AIR",
        "Breathiness (-100..100)",
        &mut set.breathiness,
        -100.0,
        100.0,
        &mut y,
    );
    draw_knob(
        "CHA",
        "Character / Gender Tone (-100..100)",
        &mut set.gender,
        -100.0,
        100.0,
        &mut y,
    );
    draw_knob(
        "ATK",
        "Attack / Consonant Velocity (0..200)",
        &mut set.velocity,
        0.0,
        200.0,
        &mut y,
    );

    y += 6.0;
    g.line_segment(
        [
            pos2(card_rect.left() + 12.0, y),
            pos2(card_rect.right() - 12.0, y),
        ],
        Stroke::new(1.0, BORDER_COLOR),
    );
    y += 10.0;

    draw_knob(
        "Depth",
        "Vibrato Depth in cents",
        &mut set.vibrato_depth,
        0.0,
        100.0,
        &mut y,
    );
    draw_knob(
        "Rate",
        "Vibrato Rate in Hz",
        &mut set.vibrato_rate,
        1.0,
        10.0,
        &mut y,
    );
    draw_knob(
        "Glide",
        "Pitch Transition time (ms)",
        &mut set.transition_ms,
        20.0,
        200.0,
        &mut y,
    );
}

/// Interactive Piano Keyboard (Left) & Canvas (Notes, Lyrics, Phonemes, Pitch Curve).
fn vocal_canvas(ui: &mut Ui, p: &mut Project, s: &mut UiState, ti: usize, ri: usize, rect: Rect) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 0.0, WELL_BG);

    let keys_rect = Rect::from_min_size(rect.min, vec2(KEYS_W, rect.height()));
    let ruler_rect = Rect::from_min_size(
        pos2(keys_rect.right(), rect.top()),
        vec2(rect.width() - KEYS_W, RULER_H),
    );
    let grid_rect = Rect::from_min_max(pos2(keys_rect.right(), ruler_rect.bottom()), rect.max);

    let beat_px = s.vocal_ed.beat_px;
    let key_h = s.vocal_ed.key_h;
    let scroll_x = s.vocal_ed.scroll_x;
    let scroll_y = s.vocal_ed.scroll_y;

    let region = &p.tracks[ti].regions[ri];
    let start_beat = p.bar_to_beats(region.start_bar as f64) as f32;

    // Coordinate conversions
    let x_of = |beat: f32| grid_rect.left() + (beat - scroll_x) * beat_px;
    let beat_of = |x: f32| ((x - grid_rect.left()) / beat_px + scroll_x).max(0.0);
    let y_of_pitch = |pitch: u8| grid_rect.bottom() - (pitch as f32 * key_h) + scroll_y;
    let pitch_of_y = |y: f32| -> i32 {
        let p = (grid_rect.bottom() + scroll_y - y) / key_h;
        p.floor() as i32
    };

    // Auto-scroll follow playhead if catch_playhead is active
    if s.vocal_ed.catch_playhead && p.playing {
        let cur_x = x_of(p.position_beats as f32);
        if cur_x > grid_rect.right() - 50.0 || cur_x < grid_rect.left() {
            s.vocal_ed.scroll_x = (p.position_beats as f32 - 2.0).max(0.0);
        }
    }

    // Middle-click 2D Panning (smooth navigation)
    let (middle_down, middle_delta, hover) = ui.input(|i| {
        (
            i.pointer.middle_down(),
            i.pointer.delta(),
            i.pointer.hover_pos(),
        )
    });
    let in_canvas = hover
        .or_else(|| ui.input(|i| i.pointer.interact_pos()))
        .is_some_and(|pt| rect.contains(pt));
    if middle_down && in_canvas {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        s.vocal_ed.scroll_x = (s.vocal_ed.scroll_x - middle_delta.x / beat_px).max(0.0);
        s.vocal_ed.scroll_y = (s.vocal_ed.scroll_y + middle_delta.y).clamp(0.0, 127.0 * key_h);
    }

    // Handle scroll wheel on grid
    let grid_resp = ui.interact(
        grid_rect,
        egui::Id::unique("vocal_grid_interact"),
        Sense::click_and_drag(),
    );
    if grid_resp.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta.x.abs() > 0.1 {
            s.vocal_ed.scroll_x = (s.vocal_ed.scroll_x - scroll_delta.x / beat_px).max(0.0);
        }
        if scroll_delta.y.abs() > 0.1 {
            if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                // Zoom
                s.vocal_ed.beat_px =
                    (s.vocal_ed.beat_px * (1.0 + scroll_delta.y * 0.005)).clamp(20.0, 300.0);
            } else {
                s.vocal_ed.scroll_y =
                    (s.vocal_ed.scroll_y + scroll_delta.y).clamp(0.0, 127.0 * key_h);
            }
        }
    }

    // 1. Draw Semitone Rows & Grid Lines
    let min_pitch = pitch_of_y(grid_rect.bottom()).max(0) as u8;
    let max_pitch = pitch_of_y(grid_rect.top()).min(127) as u8;

    for pitch in min_pitch..=max_pitch {
        let y_top = y_of_pitch(pitch);
        let r = Rect::from_min_size(
            pos2(grid_rect.left(), y_top - key_h),
            vec2(grid_rect.width(), key_h),
        );
        if r.bottom() < grid_rect.top() || r.top() > grid_rect.bottom() {
            continue;
        }
        let fill = if is_black(pitch as i32) {
            ROW_BLACK
        } else {
            ROW_WHITE
        };
        g.rect_filled(r, 0.0, fill);
        g.line_segment(
            [r.left_bottom(), r.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x23, 0x26, 0x2c)),
        );
    }

    // 2. Vertical Bars / Beat Grid Lines
    let first_beat = s.vocal_ed.scroll_x.floor() as i32;
    let last_beat = (beat_of(grid_rect.right()) + 1.0).ceil() as i32;

    for beat in first_beat..=last_beat {
        let x = x_of(beat as f32);
        if x < grid_rect.left() || x > grid_rect.right() {
            continue;
        }
        let is_bar = beat % 4 == 0;
        let stroke = if is_bar {
            Stroke::new(1.0, GRID_LINE_BAR)
        } else {
            Stroke::new(1.0, GRID_LINE_BEAT)
        };
        g.line_segment(
            [pos2(x, grid_rect.top()), pos2(x, grid_rect.bottom())],
            stroke,
        );
    }

    // 3. Ruler Bar
    let gr = ui.painter_at(ruler_rect);
    gr.rect_filled(ruler_rect, 0.0, TOOLBAR_BG);
    gr.line_segment(
        [ruler_rect.left_bottom(), ruler_rect.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    for beat in first_beat..=last_beat {
        let x = x_of(beat as f32);
        if x < ruler_rect.left() || x > ruler_rect.right() {
            continue;
        }
        if beat % 4 == 0 {
            let bar_num = beat / 4 + 1;
            gr.line_segment(
                [
                    pos2(x, ruler_rect.top() + 6.0),
                    pos2(x, ruler_rect.bottom()),
                ],
                Stroke::new(1.0, Color32::from_gray(0x80)),
            );
            gr.text(
                pos2(x + 4.0, ruler_rect.top() + 4.0),
                Align2::LEFT_TOP,
                format!("{bar_num}"),
                FontId::proportional(11.0),
                Color32::WHITE,
            );
        } else {
            gr.line_segment(
                [
                    pos2(x, ruler_rect.top() + 12.0),
                    pos2(x, ruler_rect.bottom()),
                ],
                Stroke::new(1.0, Color32::from_gray(0x40)),
            );
        }
    }

    // 4. Notes & Pitch Curve Rendering
    let notes = &p.tracks[ti].regions[ri].notes;
    let mut note_rects = Vec::with_capacity(notes.len());

    for (i, note) in notes.iter().enumerate() {
        let nx0 = x_of(start_beat + note.start);
        let nx1 = x_of(start_beat + note.start + note.len);
        let ny = y_of_pitch(note.pitch) - key_h;
        let nr = Rect::from_min_max(pos2(nx0, ny), pos2(nx1.max(nx0 + 4.0), ny + key_h));
        note_rects.push((i, nr));

        if nr.right() < grid_rect.left() || nr.left() > grid_rect.right() {
            continue;
        }

        let is_sel = s.vocal_ed.selected.contains(&i);
        let base_col = Color32::from_rgb(0x38, 0x8e, 0x3c); // clean vocal note green
        let fill_col = if is_sel { ACCENT_BLUE } else { base_col };
        let stroke_col = if is_sel {
            ACCENT_BLUE_LIGHT
        } else {
            Color32::from_rgb(0x2e, 0x6e, 0x32)
        };

        g.rect_filled(nr, CornerRadius::same(3), fill_col);
        g.rect_stroke(
            nr,
            CornerRadius::same(3),
            Stroke::new(1.0, stroke_col),
            StrokeKind::Inside,
        );

        // Note Label: Pitch "Lyric [phoneme]"
        let pitch_label = c_label(note.pitch as i32);
        let lyric_str = note.lyric.as_str();
        let phoneme_str = phoneme_alias(lyric_str);
        let label = if lyric_str.is_empty() {
            pitch_label
        } else if phoneme_str.is_empty() {
            format!("{pitch_label} \"{lyric_str}\"")
        } else {
            format!("{pitch_label} \"{lyric_str} [{phoneme_str}]\"")
        };

        let text_font = FontId::proportional(11.0);
        g.text(
            pos2(nr.left() + 4.0, nr.center().y),
            Align2::LEFT_CENTER,
            &label,
            text_font,
            Color32::WHITE,
        );
    }

    // 5. Smooth Pitch Curve Overlay (Vocal Pitch glides + Vibrato curves)
    if notes.len() >= 2 {
        let mut curve_pts: Vec<Pos2> = Vec::new();
        for i in 0..notes.len() {
            let n = &notes[i];
            let bx0 = x_of(start_beat + n.start);
            let bx1 = x_of(start_beat + n.start + n.len);
            let by = y_of_pitch(n.pitch) - key_h / 2.0;

            // Transition from previous note
            if i > 0 {
                let prev = &notes[i - 1];
                let p_by = y_of_pitch(prev.pitch) - key_h / 2.0;
                let glide_steps = 6;
                let glide_w = (bx0 - x_of(start_beat + prev.start + prev.len)).clamp(4.0, 24.0);
                for step in 0..=glide_steps {
                    let t = step as f32 / glide_steps as f32;
                    let x = bx0 - glide_w * (1.0 - t);
                    let y = p_by + (by - p_by) * (0.5 - 0.5 * (t * std::f32::consts::PI).cos());
                    curve_pts.push(pos2(x, y));
                }
            }

            // Vibrato on sustained note portion
            let steps = (n.len * 16.0).max(4.0) as usize;
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let x = bx0 + (bx1 - bx0) * t;
                let vib_offset = if n.len > 0.5 && t > 0.3 {
                    let vib_t = (t - 0.3) / 0.7;
                    (vib_t * 6.0 * std::f32::consts::PI).sin() * 3.5 * vib_t
                } else {
                    0.0
                };
                curve_pts.push(pos2(x, by + vib_offset));
            }
        }

        // Draw pitch curve
        for pair in curve_pts.windows(2) {
            let p0 = pair[0];
            let p1 = pair[1];
            if (p0.x >= grid_rect.left() && p0.x <= grid_rect.right())
                || (p1.x >= grid_rect.left() && p1.x <= grid_rect.right())
            {
                g.line_segment([p0, p1], Stroke::new(1.5, PITCH_CURVE_ORANGE));
            }
        }
    }

    // 6. Inline Lyric Text Editor on Double Click
    if let Some(edit_idx) = s.vocal_ed.editing_lyric {
        if let Some((_, nr)) = note_rects.iter().find(|(idx, _)| *idx == edit_idx) {
            let edit_rect = Rect::from_min_size(
                pos2(nr.left(), nr.bottom() + 2.0),
                vec2(nr.width().max(60.0), 20.0),
            );
            g.rect_filled(edit_rect, 3.0, Color32::from_rgb(0x22, 0x25, 0x2a));
            g.rect_stroke(
                edit_rect,
                3.0,
                Stroke::new(1.0, ACCENT_BLUE_LIGHT),
                StrokeKind::Inside,
            );

            let mut edit_resp = ui.allocate_rect(edit_rect, Sense::click_and_drag());
            let te = egui::TextEdit::singleline(&mut s.vocal_ed.lyric_buffer)
                .font(FontId::proportional(11.5))
                .text_color(Color32::WHITE);
            let te_resp = ui.put(edit_rect, te);
            te_resp.request_focus();

            // Handle Tab, Shift+Tab, Enter navigation
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                reg_mut.notes[edit_idx].lyric = Lyric::new(&s.vocal_ed.lyric_buffer);
                s.vocal_ed.editing_lyric = None;
                s.vocal_ed.dirty = true;
            } else if ui.input(|i| i.key_pressed(Key::Tab)) {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                reg_mut.notes[edit_idx].lyric = Lyric::new(&s.vocal_ed.lyric_buffer);
                s.vocal_ed.dirty = true;
                if ui.input(|i| i.modifiers.shift) {
                    if edit_idx > 0 {
                        s.vocal_ed.editing_lyric = Some(edit_idx - 1);
                        s.vocal_ed.lyric_buffer =
                            reg_mut.notes[edit_idx - 1].lyric.as_str().to_string();
                    } else {
                        s.vocal_ed.editing_lyric = None;
                    }
                } else {
                    if edit_idx + 1 < reg_mut.notes.len() {
                        s.vocal_ed.editing_lyric = Some(edit_idx + 1);
                        s.vocal_ed.lyric_buffer =
                            reg_mut.notes[edit_idx + 1].lyric.as_str().to_string();
                    } else {
                        s.vocal_ed.editing_lyric = None;
                    }
                }
            } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                s.vocal_ed.editing_lyric = None;
            }
        }
    }

    // 7. Interactive Click & Drag Handling on Notes
    let pointer_pos = ui.input(|i| i.pointer.hover_pos());
    if let Some(pos) = pointer_pos {
        if grid_rect.contains(pos) {
            // Scissors tool click
            if s.vocal_ed.tool == VocalTool::Scissors && ui.input(|i| i.pointer.primary_clicked()) {
                let click_beat = beat_of(pos.x) - start_beat;
                let reg_mut = &mut p.tracks[ti].regions[ri];
                for idx in 0..reg_mut.notes.len() {
                    let n = reg_mut.notes[idx];
                    if click_beat > n.start + 0.1 && click_beat < n.start + n.len - 0.1 {
                        let len1 = click_beat - n.start;
                        let len2 = n.len - len1;
                        reg_mut.notes[idx].len = len1;
                        let mut new_note = n;
                        new_note.start = click_beat;
                        new_note.len = len2;
                        new_note.lyric = Lyric::new("-"); // melisma extension
                        reg_mut.notes.insert(idx + 1, new_note);
                        s.vocal_ed.dirty = true;
                        break;
                    }
                }
            }
            // Pencil tool click to create note
            else if s.vocal_ed.tool == VocalTool::Pencil
                && ui.input(|i| i.pointer.primary_clicked())
            {
                let click_beat = beat_of(pos.x) - start_beat;
                let click_pitch = pitch_of_y(pos.y).clamp(12, 110) as u8;
                let reg_mut = &mut p.tracks[ti].regions[ri];
                let mut note = Note::default();
                note.start = (click_beat / 0.25).round().max(0.0) * 0.25;
                note.len = 1.0;
                note.pitch = click_pitch;
                note.vel = 100;
                note.lyric = Lyric::new("a");
                reg_mut.notes.push(note);
                s.vocal_ed.selected.clear();
                s.vocal_ed.selected.insert(reg_mut.notes.len() - 1);
                s.vocal_ed.dirty = true;
            }
        }
    }

    // Double-click on note to edit lyric
    for (idx, nr) in &note_rects {
        let note_resp = ui.interact(
            *nr,
            egui::Id::unique(("vocal_note_resp", *idx)),
            Sense::click_and_drag(),
        );
        if note_resp.double_clicked() {
            s.vocal_ed.editing_lyric = Some(*idx);
            s.vocal_ed.lyric_buffer = p.tracks[ti].regions[ri].notes[*idx]
                .lyric
                .as_str()
                .to_string();
        } else if note_resp.clicked() {
            if ui.input(|i| i.modifiers.shift) {
                if s.vocal_ed.selected.contains(idx) {
                    s.vocal_ed.selected.remove(idx);
                } else {
                    s.vocal_ed.selected.insert(*idx);
                }
            } else {
                s.vocal_ed.selected.clear();
                s.vocal_ed.selected.insert(*idx);
            }
            if s.vocal_ed.acoustic_feedback {
                let note = p.tracks[ti].regions[ri].notes[*idx];
                s.audition.push(Audition::On {
                    pitch: note.pitch,
                    vel: note.vel,
                    track: ti as u16,
                });
            }
        }
    }

    // 8. Keyboard Shortcuts
    if !ui.ctx().egui_wants_keyboard_input() && s.vocal_ed.editing_lyric.is_none() {
        let notes_len = p.tracks[ti].regions[ri].notes.len();
        if notes_len > 0 {
            let cur_idx = s.vocal_ed.selected.iter().min().copied().unwrap_or(0);
            if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
                if ui.input(|i| i.modifiers.shift) {
                    if cur_idx + 1 < notes_len {
                        s.vocal_ed.selected.insert(cur_idx + 1);
                    }
                } else {
                    s.vocal_ed.selected.clear();
                    s.vocal_ed.selected.insert((cur_idx + 1).min(notes_len - 1));
                }
            } else if ui.input(|i| i.key_pressed(Key::ArrowLeft)) {
                if ui.input(|i| i.modifiers.shift) {
                    if cur_idx > 0 {
                        s.vocal_ed.selected.insert(cur_idx - 1);
                    }
                } else {
                    s.vocal_ed.selected.clear();
                    s.vocal_ed.selected.insert(cur_idx.saturating_sub(1));
                }
            } else if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                let shift = if ui.input(|i| i.modifiers.shift) {
                    12
                } else {
                    1
                };
                let reg_mut = &mut p.tracks[ti].regions[ri];
                for sel in &s.vocal_ed.selected {
                    if let Some(n) = reg_mut.notes.get_mut(*sel) {
                        n.pitch = (n.pitch as i32 + shift).clamp(12, 110) as u8;
                    }
                }
                s.vocal_ed.dirty = true;
            } else if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                let shift = if ui.input(|i| i.modifiers.shift) {
                    12
                } else {
                    1
                };
                let reg_mut = &mut p.tracks[ti].regions[ri];
                for sel in &s.vocal_ed.selected {
                    if let Some(n) = reg_mut.notes.get_mut(*sel) {
                        n.pitch = (n.pitch as i32 - shift).clamp(12, 110) as u8;
                    }
                }
                s.vocal_ed.dirty = true;
            } else if ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
                let mut sel_vec: Vec<usize> = s.vocal_ed.selected.iter().copied().collect();
                sel_vec.sort_unstable();
                let reg_mut = &mut p.tracks[ti].regions[ri];
                for idx in sel_vec.into_iter().rev() {
                    if idx < reg_mut.notes.len() {
                        reg_mut.notes.remove(idx);
                    }
                }
                s.vocal_ed.selected.clear();
                s.vocal_ed.dirty = true;
            } else if ui.input(|i| i.key_pressed(Key::Q)) {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                for sel in &s.vocal_ed.selected {
                    if let Some(n) = reg_mut.notes.get_mut(*sel) {
                        n.start = (n.start / 0.25).round() * 0.25;
                    }
                }
                s.vocal_ed.dirty = true;
            }
        }
    }

    // 9. Interactive Left Piano Keyboard
    let gk = ui.painter_at(keys_rect);
    gk.rect_filled(keys_rect, 0.0, TOOLBAR_BG);
    gk.line_segment(
        [keys_rect.right_top(), keys_rect.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    for pitch in min_pitch..=max_pitch {
        let y_top = y_of_pitch(pitch);
        let key_r = Rect::from_min_size(
            pos2(keys_rect.left(), y_top - key_h),
            vec2(keys_rect.width(), key_h),
        );
        if key_r.bottom() < keys_rect.top() || key_r.top() > keys_rect.bottom() {
            continue;
        }

        let black = is_black(pitch as i32);
        let col = if black {
            Color32::from_rgb(0x18, 0x19, 0x1c)
        } else {
            Color32::from_rgb(0x2a, 0x2d, 0x33)
        };
        gk.rect_filled(key_r, 0.0, col);
        gk.line_segment(
            [key_r.left_bottom(), key_r.right_bottom()],
            Stroke::new(1.0, Color32::from_rgb(0x1a, 0x1c, 0x20)),
        );

        if pitch % 12 == 0 {
            gk.text(
                pos2(key_r.left() + 4.0, key_r.center().y),
                Align2::LEFT_CENTER,
                c_label(pitch as i32),
                FontId::proportional(10.0),
                Color32::from_gray(0xa0),
            );
        }

        let resp = ui.interact(
            key_r,
            egui::Id::unique(("vocal_key", pitch)),
            Sense::click_and_drag(),
        );
        if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down()) {
            if s.vocal_ed.held != Some(pitch) {
                s.audition.push(Audition::On {
                    pitch,
                    vel: 100,
                    track: ti as u16,
                });
                s.vocal_ed.held = Some(pitch);
            }
        }
    }
    if !ui.input(|i| i.pointer.primary_down()) {
        s.vocal_ed.held = None;
    }
}

/// Bottom Parameter Lane for PITD (Pitch Bend Cent), Velocity, DYN, etc.
fn vocal_parameter_lane(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    ri: usize,
    rect: Rect,
    canvas_left: f32,
) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 0.0, TOOLBAR_BG);
    g.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    // Left Parameter Selector Area
    let left_w = canvas_left - rect.left();
    let left_rect = Rect::from_min_size(rect.min, vec2(left_w, rect.height()));
    g.rect_filled(left_rect, 0.0, TOOLBAR_BG);
    g.line_segment(
        [left_rect.right_top(), left_rect.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    // Parameter type label and toggle
    let cy = left_rect.center().y;
    let param_label = s.vocal_ed.param_type.label();
    let label_rect = Rect::from_min_size(
        pos2(left_rect.left() + 6.0, cy - 12.0),
        vec2(left_w - 12.0, 24.0),
    );
    let resp = ui
        .interact(
            label_rect,
            egui::Id::unique("vocal_param_selector"),
            Sense::click(),
        )
        .on_hover_text("Click to cycle parameter lane");
    if resp.hovered() {
        g.rect_filled(label_rect, 4.0, Color32::from_white_alpha(15));
    }
    g.text(
        label_rect.center(),
        Align2::CENTER_CENTER,
        param_label,
        FontId::proportional(11.0),
        Color32::WHITE,
    );
    if resp.clicked() {
        let all = VocalParamType::ALL;
        let cur = all
            .iter()
            .position(|&p| p == s.vocal_ed.param_type)
            .unwrap_or(0);
        s.vocal_ed.param_type = all[(cur + 1) % all.len()];
    }

    // Lane Drawing Canvas
    let lane_rect = Rect::from_min_max(pos2(canvas_left, rect.top()), rect.max);
    g.rect_filled(lane_rect, 0.0, WELL_BG);

    let beat_px = s.vocal_ed.beat_px;
    let scroll_x = s.vocal_ed.scroll_x;
    let x_of = |beat: f32| lane_rect.left() + (beat - scroll_x) * beat_px;
    let beat_of = |x: f32| ((x - lane_rect.left()) / beat_px + scroll_x).max(0.0);

    let start_beat = p.bar_to_beats(p.tracks[ti].regions[ri].start_bar as f64) as f32;

    if s.vocal_ed.param_type == VocalParamType::Velocity {
        let note_data: Vec<(usize, f32, u8)> = p.tracks[ti].regions[ri]
            .notes
            .iter()
            .enumerate()
            .map(|(idx, n)| (idx, n.start, n.vel))
            .collect();

        for (idx, n_start, n_vel) in note_data {
            let nx = x_of(start_beat + n_start);
            if nx < lane_rect.left() || nx > lane_rect.right() {
                continue;
            }
            let is_sel = s.vocal_ed.selected.contains(&idx);
            let col = if is_sel {
                ACCENT_BLUE_LIGHT
            } else {
                Color32::from_rgb(0x38, 0x8e, 0x3c)
            };

            let stem_bot = lane_rect.bottom() - 4.0;
            let stem_top = stem_bot - (n_vel as f32 / 127.0) * (lane_rect.height() - 14.0);

            g.line_segment(
                [pos2(nx, stem_bot), pos2(nx, stem_top)],
                Stroke::new(1.5, col),
            );
            g.circle_filled(
                pos2(nx, stem_top),
                3.5,
                if is_sel { Color32::WHITE } else { col },
            );

            let handle_rect = Rect::from_center_size(pos2(nx, stem_top), vec2(10.0, 10.0));
            let stem_resp = ui.interact(
                handle_rect,
                egui::Id::unique(("vel_stem", idx)),
                Sense::drag(),
            );
            if stem_resp.dragged_by(egui::PointerButton::Primary) {
                let delta =
                    (-stem_resp.drag_delta().y / (lane_rect.height() - 14.0) * 127.0) as i32;
                let reg_mut = &mut p.tracks[ti].regions[ri];
                reg_mut.notes[idx].vel =
                    (reg_mut.notes[idx].vel as i32 + delta).clamp(1, 127) as u8;
                s.vocal_ed.dirty = true;
            }
        }
    } else if s.vocal_ed.param_type == VocalParamType::Pitd {
        // Continuous Pitch Bend Cent Lane (-1200..+1200 cents, baseline 0)
        let mid_y = lane_rect.center().y;
        let scale_y = (lane_rect.height() - 20.0) * 0.5; // distance from center to +/- 200 cents

        // Baseline 0 cents
        g.line_segment(
            [
                pos2(lane_rect.left(), mid_y),
                pos2(lane_rect.right(), mid_y),
            ],
            Stroke::new(1.0, Color32::from_rgb(0x3e, 0x43, 0x50)),
        );
        // +100 cents guide line
        let plus_100_y = mid_y - (100.0 / 200.0) * scale_y;
        g.line_segment(
            [
                pos2(lane_rect.left(), plus_100_y),
                pos2(lane_rect.right(), plus_100_y),
            ],
            Stroke::new(0.5, Color32::from_rgb(0x22, 0x25, 0x2c)),
        );
        // -100 cents guide line
        let minus_100_y = mid_y + (100.0 / 200.0) * scale_y;
        g.line_segment(
            [
                pos2(lane_rect.left(), minus_100_y),
                pos2(lane_rect.right(), minus_100_y),
            ],
            Stroke::new(0.5, Color32::from_rgb(0x22, 0x25, 0x2c)),
        );

        g.text(
            pos2(lane_rect.left() + 4.0, mid_y - 8.0),
            Align2::LEFT_CENTER,
            "0",
            FontId::monospace(9.5),
            TEXT_DIM,
        );
        g.text(
            pos2(lane_rect.left() + 4.0, plus_100_y - 8.0),
            Align2::LEFT_CENTER,
            "+100c",
            FontId::monospace(9.5),
            TEXT_DIM,
        );
        g.text(
            pos2(lane_rect.left() + 4.0, minus_100_y - 8.0),
            Align2::LEFT_CENTER,
            "-100c",
            FontId::monospace(9.5),
            TEXT_DIM,
        );

        // Render curve points
        if s.vocal_ed.custom_pitch.len() >= 2 {
            let pts: Vec<Pos2> = s
                .vocal_ed
                .custom_pitch
                .iter()
                .map(|(b, cents)| {
                    let px = x_of(start_beat + b);
                    let py = mid_y - (cents / 200.0) * scale_y;
                    pos2(px, py)
                })
                .collect();

            for pair in pts.windows(2) {
                let p0 = pair[0];
                let p1 = pair[1];
                if (p0.x >= lane_rect.left() && p0.x <= lane_rect.right())
                    || (p1.x >= lane_rect.left() && p1.x <= lane_rect.right())
                {
                    g.line_segment([p0, p1], Stroke::new(1.5, PITCH_CURVE_ORANGE));
                }
            }

            for (idx, pt) in pts.iter().enumerate() {
                if pt.x >= lane_rect.left() && pt.x <= lane_rect.right() {
                    g.circle_filled(*pt, 2.5, Color32::WHITE);
                }
            }
        }

        // Draw / edit pitch bend curve with mouse
        let lane_resp = ui.interact(
            lane_rect,
            egui::Id::unique("pitd_lane_interact"),
            Sense::click_and_drag(),
        );
        if lane_resp.dragged_by(egui::PointerButton::Primary)
            || (lane_resp.clicked() && ui.input(|i| i.pointer.primary_clicked()))
        {
            if let Some(pos) = lane_resp.interact_pointer_pos() {
                let click_beat = (beat_of(pos.x) - start_beat).max(0.0);
                let cents = -((pos.y - mid_y) / scale_y) * 200.0;
                let cents = cents.clamp(-1200.0, 1200.0);

                let mut cur = s.vocal_ed.custom_pitch.clone();
                cur.retain(|(b, _)| (*b - click_beat).abs() > 0.05);
                cur.push((click_beat, cents));
                cur.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                s.vocal_ed.custom_pitch = cur;
                s.vocal_ed.dirty = true;
            }
        }
    } else {
        // Other expression parameter lanes
        let mid_y = lane_rect.center().y;
        g.line_segment(
            [
                pos2(lane_rect.left(), mid_y),
                pos2(lane_rect.right(), mid_y),
            ],
            Stroke::new(1.0, BORDER_COLOR),
        );
    }

    // Zoom buttons in bottom right
    let zoom_w = 20.0;
    let zoom_h = 18.0;
    let right_y = lane_rect.bottom() - zoom_h - 4.0;

    let z_in_rect = Rect::from_min_size(
        pos2(lane_rect.right() - 26.0, right_y),
        vec2(zoom_w, zoom_h),
    );
    let z_in_resp = ui
        .interact(z_in_rect, egui::Id::unique("vocal_zoom_in"), Sense::click())
        .on_hover_text("Zoom In");
    if z_in_resp.hovered() {
        g.rect_filled(z_in_rect, 3.0, Color32::from_white_alpha(20));
    }
    g.text(
        z_in_rect.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::MAGNIFYING_GLASS_PLUS,
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    if z_in_resp.clicked() {
        s.vocal_ed.beat_px = (s.vocal_ed.beat_px * 1.25).min(300.0);
    }

    let z_out_rect = Rect::from_min_size(
        pos2(lane_rect.right() - 50.0, right_y),
        vec2(zoom_w, zoom_h),
    );
    let z_out_resp = ui
        .interact(
            z_out_rect,
            egui::Id::unique("vocal_zoom_out"),
            Sense::click(),
        )
        .on_hover_text("Zoom Out");
    if z_out_resp.hovered() {
        g.rect_filled(z_out_rect, 3.0, Color32::from_white_alpha(20));
    }
    g.text(
        z_out_rect.center(),
        Align2::CENTER_CENTER,
        phosphor_egui::regular::MAGNIFYING_GLASS_MINUS,
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    if z_out_resp.clicked() {
        s.vocal_ed.beat_px = (s.vocal_ed.beat_px / 1.25).max(20.0);
    }
}
