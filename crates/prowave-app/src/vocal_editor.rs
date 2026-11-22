//! Vocal Editor (Musical Editor): a dedicated vocal score editor with lyrics, phonetic symbols,
//! pitch curves, and control parameter lanes for Vocal Top Line tracks.
//!
//! Aligned with the Musical Editor specifications and styled strictly in Dōri's Liquid Glass theme
//! with 100% phosphor-egui icons.

use crate::i18n::{tr, trf};
use crate::liquid_glass;
use crate::lyrics::Lyric;
use crate::model::{Note, Project, TrackKind};
use crate::piano_roll::{c_label, is_black, thumb_len, Audition};
use crate::tracks_area::{snap_step, snap_to, Snap};
use crate::vocal::{VocalRole, VocalSettings};
use crate::widgets::{BLUE, TEXT, TEXT_DIM};
use crate::{Bottom, UiState};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Key, Modifiers, Painter, Pos2, Rect, Sense,
    Shape, Stroke, StrokeKind, Ui,
};
use std::collections::HashSet;

const TAB_H: f32 = 26.0;
const EVENT_BAR_H: f32 = 28.0;
const TOOLBAR_H: f32 = 32.0;
const INSPECTOR_W: f32 = 230.0;
const KEYS_W: f32 = 52.0;
const RULER_H: f32 = 22.0;
const PARAM_LANE_H: f32 = 78.0;
const MIN_THUMB: f32 = 24.0;

// Liquid Glass Palette
const WELL_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
const TOOLBAR_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x24);
const BORDER_COLOR: Color32 = Color32::from_rgb(0x2e, 0x31, 0x38);
const ROW_WHITE: Color32 = Color32::from_rgb(0x16, 0x18, 0x1c);
const ROW_BLACK: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x22);
const GRID_LINE_BAR: Color32 = Color32::from_rgb(0x2a, 0x2d, 0x35);
const GRID_LINE_BEAT: Color32 = Color32::from_rgb(0x20, 0x22, 0x27);
const ACCENT_BLUE: Color32 = Color32::from_rgb(0x2b, 0x6c, 0xb0);
const ACCENT_BLUE_LIGHT: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);
const PITCH_CURVE_ORANGE: Color32 = Color32::from_rgb(0xff, 0x95, 0x00);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VocalTool {
    Arrow,
    Pencil,
    Line,
    Scissors,
    PitchPencil,
    PitchEraser,
    Vibrato,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VocalParamType {
    Velocity,
    Dynamics,
    PitchBend,
    PitchBendSensitivity,
    VibratoDepth,
    VibratoRate,
    Brightness,
    Breathiness,
    Gender,
}

impl VocalParamType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Velocity => "Velocity",
            Self::Dynamics => "Dynamics",
            Self::PitchBend => "Pitch Bend",
            Self::PitchBendSensitivity => "Pitch Bend Sens",
            Self::VibratoDepth => "Vibrato Depth",
            Self::VibratoRate => "Vibrato Rate",
            Self::Brightness => "Brightness",
            Self::Breathiness => "Breathiness",
            Self::Gender => "Gender",
        }
    }

    pub const ALL: [Self; 9] = [
        Self::Velocity,
        Self::Dynamics,
        Self::PitchBend,
        Self::PitchBendSensitivity,
        Self::VibratoDepth,
        Self::VibratoRate,
        Self::Brightness,
        Self::Breathiness,
        Self::Gender,
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
    pub custom_pitch: Vec<(f32, f32)>, // (beat, pitch_delta)
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
            show_inspector: true,
            show_parameters: true,
            param_type: VocalParamType::Velocity,
            held: None,
            vel_drag: None,
            dirty: false,
            custom_pitch: Vec::new(),
            drag_state: DragState::None,
        }
    }
}

/// Helper phoneme lookup for Japanese syllables to phonetic symbol display.
fn phoneme_alias(lyric: &str) -> &'static str {
    match lyric.trim() {
        "あ" | "ア" | "a" => "a",
        "い" | "イ" | "i" => "i",
        "う" | "ウ" | "u" => "u",
        "え" | "エ" | "e" => "e",
        "お" | "オ" | "o" => "o",
        "か" | "カ" => "ka",
        "き" | "キ" => "ki",
        "く" | "ク" => "ku",
        "け" | "ケ" => "ke",
        "こ" | "コ" => "ko",
        "さ" | "サ" => "sa",
        "し" | "シ" => "shi",
        "す" | "ス" => "su",
        "せ" | "セ" => "se",
        "そ" | "ソ" => "so",
        "た" | "タ" => "ta",
        "ち" | "チ" => "chi",
        "つ" | "ツ" => "tsu",
        "て" | "テ" => "te",
        "と" | "ト" => "to",
        "な" | "ナ" => "na",
        "に" | "ニ" => "ni",
        "ぬ" | "ヌ" => "nu",
        "ね" | "ネ" => "ne",
        "の" | "ノ" => "no",
        "は" | "ハ" => "ha",
        "ひ" | "ヒ" => "hi",
        "ふ" | "フ" => "fu",
        "へ" | "ヘ" => "he",
        "ほ" | "ホ" => "ho",
        "ま" | "マ" => "ma",
        "み" | "ミ" => "mi",
        "む" | "ム" => "mu",
        "め" | "メ" => "me",
        "も" | "モ" => "mo",
        "や" | "ヤ" => "ya",
        "ゆ" | "ユ" => "yu",
        "よ" | "ヨ" => "yo",
        "ら" | "ラ" => "ra",
        "り" | "リ" => "ri",
        "る" | "ル" => "ru",
        "れ" | "レ" => "re",
        "ろ" | "ロ" => "ro",
        "わ" | "ワ" => "wa",
        "を" | "ヲ" => "o",
        "ん" | "ン" => "n",
        "が" | "ガ" => "ga",
        "ぎ" | "ギ" => "gi",
        "ぐ" | "グ" => "gu",
        "げ" | "ゲ" => "ge",
        "ご" | "ゴ" => "go",
        "ざ" | "ザ" => "za",
        "じ" | "ジ" => "ji",
        "ず" | "ズ" => "zu",
        "ぜ" | "ゼ" => "ze",
        "ぞ" | "ゾ" => "zo",
        "だ" | "ダ" => "da",
        "ぢ" | "ヂ" => "ji",
        "づ" | "ヅ" => "zu",
        "で" | "デ" => "de",
        "ど" | "ド" => "do",
        "ば" | "バ" => "ba",
        "び" | "ビ" => "bi",
        "ぶ" | "ブ" => "bu",
        "べ" | "ベ" => "be",
        "ぼ" | "ボ" => "bo",
        "ぱ" | "パ" => "pa",
        "ぴ" | "ピ" => "pi",
        "ぷ" | "プ" => "pu",
        "ぺ" | "ペ" => "pe",
        "ぽ" | "ポ" => "po",
        "-" => "-",
        "+" => "+",
        _ => "",
    }
}

/// Finds the targeted vocal score region (or fallback MIDI region).
pub fn target_vocal_region(p: &Project, s: &UiState) -> Option<(usize, usize)> {
    if let Some((t, r)) = s.tracks.selected {
        if let Some(tr) = p.tracks.get(t) {
            if tr.kind != TrackKind::Audio && tr.vocal.is_some() && r < tr.regions.len() {
                return Some((t, r));
            }
        }
    }
    if let Some(tr) = p.tracks.get(p.selected_track) {
        if tr.kind != TrackKind::Audio && tr.vocal.is_some() && !tr.regions.is_empty() {
            return Some((p.selected_track, 0));
        }
    }
    for (ti, tr) in p.tracks.iter().enumerate() {
        if tr.kind != TrackKind::Audio && tr.vocal.is_some() && !tr.regions.is_empty() {
            return Some((ti, 0));
        }
    }
    crate::piano_roll::target_region(p, s)
}

/// Displays the top editor tabs (Piano Roll, Vocal Editor, Step Sequencer).
pub fn editor_tabs(
    ui: &Ui,
    g: &Painter,
    bar: Rect,
    active: usize,
) -> Option<usize> {
    let mut clicked = None;
    g.rect_filled(bar, 0.0, TOOLBAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let tabs = [tr("Piano Roll"), "Vocal Editor", tr("Step Sequencer")];
    let font = FontId::proportional(12.0);
    let widths: Vec<f32> = tabs
        .iter()
        .map(|t| g.layout_no_wrap((*t).into(), font.clone(), TEXT).size().x + 24.0)
        .collect();
    let total: f32 = widths.iter().sum();
    let mut x = bar.center().x - total / 2.0;

    for (i, (t, w)) in tabs.iter().zip(&widths).enumerate() {
        let r = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(*w, TAB_H - 6.0));
        let resp = ui.interact(r, egui::Id::unique(("editor_tab_shared", i)), Sense::click());
        if resp.hovered() && i != active {
            g.rect_filled(r, 4.0, Color32::from_white_alpha(15));
        }
        if resp.clicked() {
            clicked = Some(i);
        }
        if i == active {
            g.rect_filled(r, 4.0, ACCENT_BLUE);
            g.text(r.center(), Align2::CENTER_CENTER, *t, font.clone(), Color32::WHITE);
        } else {
            g.text(r.center(), Align2::CENTER_CENTER, *t, font.clone(), TEXT_DIM);
        }
        x += *w;
    }
    clicked
}

/// Main entry point for the Vocal Editor in the bottom pane.
pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    let g = ui.painter_at(area);
    g.rect_filled(area, 0.0, WELL_BG);

    // 1. Shared Editor Tab Bar
    let tab_bar = Rect::from_min_size(area.min, vec2(area.width(), TAB_H));
    if let Some(i) = editor_tabs(ui, &g, tab_bar, 1) {
        match i {
            0 => {
                s.bottom = Bottom::PianoRoll;
                s.score_tab = false;
                return;
            }
            1 => {
                s.bottom = Bottom::VocalEditor;
            }
            2 => {
                s.bottom = Bottom::PianoRoll;
                s.score_tab = true;
                return;
            }
            _ => {}
        }
    }

    let target = target_vocal_region(p, s);
    let (ti, ri) = match target {
        Some(t) => t,
        None => {
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

    // 2. Top Event Inspector Bar (Start, End, Length, Pitch, Velocity, Text)
    let event_bar = Rect::from_min_size(
        pos2(area.left(), tab_bar.bottom()),
        vec2(area.width(), EVENT_BAR_H),
    );
    event_inspector_bar(ui, p, s, ti, ri, event_bar);

    // 3. Musical Editor Toolbar (100% phosphor-egui icons)
    let tool_bar = Rect::from_min_size(
        pos2(area.left(), event_bar.bottom()),
        vec2(area.width(), TOOLBAR_H),
    );
    vocal_toolbar(ui, p, s, ti, ri, tool_bar);

    // Main workspace below toolbars
    let work_area = Rect::from_min_max(pos2(area.left(), tool_bar.bottom()), area.max);

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

    // Optional Collapsible Singer Inspector on left
    let (inspector_rect, canvas_rect) = if s.vocal_ed.show_inspector {
        let insp = Rect::from_min_size(editor_body.min, vec2(INSPECTOR_W, editor_body.height()));
        let canv = Rect::from_min_max(
            pos2(insp.right(), editor_body.top()),
            editor_body.max,
        );
        (Some(insp), canv)
    } else {
        (None, editor_body)
    };

    if let Some(insp) = inspector_rect {
        singer_inspector_panel(ui, p, s, ti, insp);
    }

    // Interactive Piano Keyboard + Note Canvas
    vocal_canvas(ui, p, s, ti, ri, canvas_rect);

    // Synced Bottom Control Parameters Lane
    if s.vocal_ed.show_parameters {
        vocal_parameter_lane(ui, p, s, ti, ri, param_lane_rect, canvas_rect.left());
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
        let resp = ui.interact(r, egui::Id::unique(("event_bar_cell", *cur_x as i32)), Sense::click_and_drag());
        if resp.hovered() {
            g.rect_filled(r, 3.0, Color32::from_white_alpha(12));
        }
        g.text(pos2(r.left() + 4.0, cy - 6.0), Align2::LEFT_CENTER, label, font_label.clone(), TEXT_DIM);
        g.text(pos2(r.left() + 4.0, cy + 5.0), Align2::LEFT_CENTER, val, font_val.clone(), Color32::WHITE);
        g.line_segment([pos2(r.right(), bar.top() + 5.0), pos2(r.right(), bar.bottom() - 5.0)], Stroke::new(1.0, BORDER_COLOR));
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
        if start_resp.dragged() {
            let delta = -start_resp.drag_delta().y * 0.05;
            let reg_mut = &mut p.tracks[ti].regions[ri];
            reg_mut.notes[idx].start = (reg_mut.notes[idx].start + delta).max(0.0);
            s.vocal_ed.dirty = true;
        }

        let (_, _) = draw_cell(ui, &g, "END", &end_str, 95.0, &mut x);

        let (_, len_resp) = draw_cell(ui, &g, "LENGTH", &len_str, 75.0, &mut x);
        if len_resp.dragged() {
            let delta = -len_resp.drag_delta().y * 0.05;
            let reg_mut = &mut p.tracks[ti].regions[ri];
            reg_mut.notes[idx].len = (reg_mut.notes[idx].len + delta).max(0.05);
            s.vocal_ed.dirty = true;
        }

        let (_, pitch_resp) = draw_cell(ui, &g, "PITCH", &pitch_str, 60.0, &mut x);
        if pitch_resp.dragged() {
            let delta = (-pitch_resp.drag_delta().y * 0.2) as i32;
            if delta != 0 {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                let new_p = (reg_mut.notes[idx].pitch as i32 + delta).clamp(12, 110) as u8;
                reg_mut.notes[idx].pitch = new_p;
                s.vocal_ed.dirty = true;
            }
        }

        let (_, vel_resp) = draw_cell(ui, &g, "VELOCITY", &vel_str, 65.0, &mut x);
        if vel_resp.dragged() {
            let delta = (-vel_resp.drag_delta().y * 0.5) as i32;
            if delta != 0 {
                let reg_mut = &mut p.tracks[ti].regions[ri];
                let new_v = (reg_mut.notes[idx].vel as i32 + delta).clamp(1, 127) as u8;
                reg_mut.notes[idx].vel = new_v;
                s.vocal_ed.dirty = true;
            }
        }

        // Lyric text input
        let text_rect = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(160.0, bar.height() - 6.0));
        g.text(pos2(text_rect.left() + 4.0, cy - 6.0), Align2::LEFT_CENTER, "LYRIC / PHONEME", font_label.clone(), TEXT_DIM);
        g.text(pos2(text_rect.left() + 4.0, cy + 5.0), Align2::LEFT_CENTER, &text_str, font_val.clone(), Color32::from_rgb(0xff, 0xd6, 0x0a));
    } else {
        let mut x_dummy = x;
        draw_cell(ui, &g, "START", "--.--.--.--", 95.0, &mut x_dummy);
        draw_cell(ui, &g, "END", "--.--.--.--", 95.0, &mut x_dummy);
        draw_cell(ui, &g, "LENGTH", "--", 75.0, &mut x_dummy);
        draw_cell(ui, &g, "PITCH", "--", 60.0, &mut x_dummy);
        draw_cell(ui, &g, "VELOCITY", "--", 65.0, &mut x_dummy);
        draw_cell(ui, &g, "LYRIC / PHONEME", "--", 160.0, &mut x_dummy);
    }
}

/// Renders the Musical Editor toolbar using 100% phosphor-egui icons.
fn vocal_toolbar(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    _ri: usize,
    bar: Rect,
) {
    let g = ui.painter_at(bar);
    g.rect_filled(bar, 0.0, TOOLBAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let cy = bar.center().y;
    let mut x = bar.left() + 10.0;

    // Tool Selector Group (Arrow, Pencil, Line, Scissors, Pitch Pencil, Pitch Eraser, Vibrato)
    let tools = [
        (VocalTool::Arrow, phosphor_egui::regular::CURSOR, "Arrow Tool (1)"),
        (VocalTool::Pencil, phosphor_egui::regular::PENCIL_SIMPLE, "Pencil Tool (2)"),
        (VocalTool::Line, phosphor_egui::regular::LINE_SEGMENT, "Line Tool"),
        (VocalTool::Scissors, phosphor_egui::regular::SCISSORS, "Scissors Tool (3)"),
        (VocalTool::PitchPencil, phosphor_egui::regular::WAVEFORM, "Pitch Pencil (4)"),
        (VocalTool::PitchEraser, phosphor_egui::regular::ERASER, "Pitch Eraser"),
        (VocalTool::Vibrato, phosphor_egui::regular::WAVE_SINE, "Vibrato Tool (5)"),
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
        let resp = ui.interact(r, egui::Id::unique(("vocal_tool", tool as u8)), Sense::click()).on_hover_text(tip);

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
    g.line_segment([pos2(x, bar.top() + 6.0), pos2(x, bar.bottom() - 6.0)], Stroke::new(1.0, BORDER_COLOR));
    x += 12.0;

    // Grid Snap Toggle & Division Dropdown
    let snap_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let snap_active = s.tracks.snap == Snap::Division;
    let snap_resp = ui.interact(snap_r, egui::Id::unique("vocal_snap_toggle"), Sense::click()).on_hover_text("Toggle Grid Snap");
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
        if snap_active { Color32::WHITE } else { TEXT_DIM },
    );
    if snap_resp.clicked() {
        s.tracks.snap = if snap_active { Snap::Beat } else { Snap::Division };
    }
    x += btn_w + 6.0;

    // Confirmation Sound (Acoustic Feedback audition toggle)
    let feedback_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let feedback_resp = ui.interact(feedback_r, egui::Id::unique("vocal_feedback_toggle"), Sense::click())
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
        if s.vocal_ed.acoustic_feedback { Color32::WHITE } else { TEXT_DIM },
    );
    if feedback_resp.clicked() {
        s.vocal_ed.acoustic_feedback = !s.vocal_ed.acoustic_feedback;
    }
    x += btn_w + 6.0;

    // Catch Playhead toggle
    let catch_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let catch_resp = ui.interact(catch_r, egui::Id::unique("vocal_catch_toggle"), Sense::click())
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
        if s.vocal_ed.catch_playhead { Color32::WHITE } else { TEXT_DIM },
    );
    if catch_resp.clicked() {
        s.vocal_ed.catch_playhead = !s.vocal_ed.catch_playhead;
    }
    x += btn_w + 6.0;

    // Part / Singer Inspector Toggle
    let insp_r = Rect::from_min_size(pos2(x, cy - btn_h / 2.0), vec2(btn_w, btn_h));
    let insp_resp = ui.interact(insp_r, egui::Id::unique("vocal_insp_toggle"), Sense::click())
        .on_hover_text("Toggle Singer / Part Inspector");
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
        if s.vocal_ed.show_inspector { Color32::WHITE } else { TEXT_DIM },
    );
    if insp_resp.clicked() {
        s.vocal_ed.show_inspector = !s.vocal_ed.show_inspector;
    }

    // Right side: Render Vocal button and quiet non-intrusive status
    let right_x = bar.right() - 110.0;
    let render_r = Rect::from_min_size(pos2(right_x, cy - btn_h / 2.0), vec2(100.0, btn_h));
    let is_rendering = s.vocal.is_rendering(None);
    let render_resp = ui.interact(render_r, egui::Id::unique("vocal_render_btn"), Sense::click())
        .on_hover_text("Render Vocal Score with OpenUtau / Voicebank Engine");

    liquid_glass::paint_glass_pill(ui, render_r, 4.0, s.glass_quality, false, None, None);
    if render_resp.hovered() {
        g.rect_filled(render_r, 4.0, Color32::from_white_alpha(20));
    }
    let icon_render = if is_rendering {
        phosphor_egui::regular::ARROWS_CLOCKWISE
    } else {
        phosphor_egui::regular::MICROPHONE_STAGE
    };
    g.text(
        pos2(render_r.left() + 14.0, cy),
        Align2::CENTER_CENTER,
        icon_render,
        FontId::proportional(13.0),
        if is_rendering { Color32::from_rgb(0xff, 0xd6, 0x0a) } else { Color32::WHITE },
    );
    g.text(
        pos2(render_r.left() + 28.0, cy),
        Align2::LEFT_CENTER,
        if is_rendering { "Rendering…" } else { "Render Vocal" },
        FontId::proportional(11.0),
        Color32::WHITE,
    );
    if render_resp.clicked() {
        if let Some(VocalRole::Score(set)) = &p.tracks[ti].vocal {
            let link = set.link;
            crate::vocal::trigger_render(s, link);
        }
    }
}

/// Renders the Singer / Part Inspector panel with voicebank picker and EXP, AIR, CHA, ATK controls.
fn singer_inspector_panel(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    rect: Rect,
) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 0.0, TOOLBAR_BG);
    g.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, BORDER_COLOR),
    );

    let cy_top = rect.top() + 16.0;
    g.text(
        pos2(rect.left() + 12.0, cy_top),
        Align2::LEFT_CENTER,
        "SINGER / PART INSPECTOR",
        FontId::proportional(11.0),
        TEXT_DIM,
    );

    let Some(VocalRole::Score(set)) = p.tracks[ti].vocal.as_mut() else {
        g.text(
            pos2(rect.left() + 12.0, cy_top + 24.0),
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
        pos2(rect.left() + 12.0, y),
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
        pos2(rect.left() + 85.0, y),
        Align2::LEFT_CENTER,
        voice_label,
        FontId::proportional(11.5),
        Color32::from_rgb(0xff, 0xd6, 0x0a),
    );
    y += 26.0;

    // 4 Primary Knobs / Sliders: EXP, AIR, CHA, ATK
    let mut draw_knob = |label: &str, tip: &str, val: &mut f32, min_v: f32, max_v: f32, cur_y: &mut f32| {
        let label_rect = Rect::from_min_size(pos2(rect.left() + 12.0, *cur_y), vec2(40.0, 18.0));
        g.text(
            pos2(label_rect.left(), label_rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            Color32::WHITE,
        );

        let slider_rect = Rect::from_min_size(pos2(rect.left() + 55.0, *cur_y + 2.0), vec2(110.0, 14.0));
        let resp = ui.interact(slider_rect, egui::Id::unique(("insp_param", *cur_y as i32)), Sense::drag()).on_hover_text(tip);

        // Groove rail background
        g.rect_filled(slider_rect, 3.0, Color32::from_rgb(0x16, 0x18, 0x1c));
        g.rect_stroke(slider_rect, 3.0, Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38)), StrokeKind::Inside);

        let t = ((*val - min_v) / (max_v - min_v)).clamp(0.0, 1.0);
        let fill_w = slider_rect.width() * t;
        if fill_w > 0.0 {
            let fill_r = Rect::from_min_size(slider_rect.min, vec2(fill_w, slider_rect.height()));
            g.rect_filled(fill_r, 3.0, ACCENT_BLUE);
        }

        if resp.dragged() {
            let delta = resp.drag_delta().x / slider_rect.width() * (max_v - min_v);
            *val = (*val + delta).clamp(min_v, max_v);
            s.vocal_ed.dirty = true;
        }

        let val_text = format!("{:.0}", *val);
        g.text(
            pos2(rect.left() + 175.0, *cur_y + 8.0),
            Align2::LEFT_CENTER,
            &val_text,
            FontId::monospace(10.5),
            TEXT_DIM,
        );

        *cur_y += 24.0;
    };

    draw_knob("EXP", "Dynamics / Tension (-100..100)", &mut set.tension, -100.0, 100.0, &mut y);
    draw_knob("AIR", "Breathiness (-100..100)", &mut set.breathiness, -100.0, 100.0, &mut y);
    draw_knob("CHA", "Character / Gender Tone (-100..100)", &mut set.gender, -100.0, 100.0, &mut y);
    draw_knob("ATK", "Attack / Consonant Velocity (0..200)", &mut set.velocity, 0.0, 200.0, &mut y);

    y += 10.0;
    g.line_segment([pos2(rect.left() + 12.0, y), pos2(rect.right() - 12.0, y)], Stroke::new(1.0, BORDER_COLOR));
    y += 14.0;

    g.text(
        pos2(rect.left() + 12.0, y),
        Align2::LEFT_CENTER,
        "Vibrato & Transition",
        FontId::proportional(11.0),
        TEXT_DIM,
    );
    y += 20.0;

    draw_knob("Depth", "Vibrato Depth in cents", &mut set.vibrato_depth, 0.0, 100.0, &mut y);
    draw_knob("Rate", "Vibrato Rate in Hz", &mut set.vibrato_rate, 1.0, 10.0, &mut y);
    draw_knob("Glide", "Pitch Transition time (ms)", &mut set.transition_ms, 20.0, 200.0, &mut y);
}

/// Interactive Piano Keyboard (Left) & Canvas (Notes, Lyrics, Phonemes, Pitch Curve).
fn vocal_canvas(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    ri: usize,
    rect: Rect,
) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 0.0, WELL_BG);

    let keys_rect = Rect::from_min_size(rect.min, vec2(KEYS_W, rect.height()));
    let ruler_rect = Rect::from_min_size(
        pos2(keys_rect.right(), rect.top()),
        vec2(rect.width() - KEYS_W, RULER_H),
    );
    let grid_rect = Rect::from_min_max(
        pos2(keys_rect.right(), ruler_rect.bottom()),
        rect.max,
    );

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

    // Handle scroll wheel on grid
    let grid_resp = ui.interact(grid_rect, egui::Id::unique("vocal_grid_interact"), Sense::click_and_drag());
    if grid_resp.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta.x.abs() > 0.1 {
            s.vocal_ed.scroll_x = (s.vocal_ed.scroll_x - scroll_delta.x / beat_px).max(0.0);
        }
        if scroll_delta.y.abs() > 0.1 {
            if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
                // Zoom
                s.vocal_ed.beat_px = (s.vocal_ed.beat_px * (1.0 + scroll_delta.y * 0.005)).clamp(20.0, 300.0);
            } else {
                s.vocal_ed.scroll_y = (s.vocal_ed.scroll_y + scroll_delta.y).clamp(0.0, 127.0 * key_h);
            }
        }
    }

    // 1. Draw Semitone Rows & Grid Lines
    let min_pitch = pitch_of_y(grid_rect.bottom()).max(0) as u8;
    let max_pitch = pitch_of_y(grid_rect.top()).min(127) as u8;

    for pitch in min_pitch..=max_pitch {
        let y_top = y_of_pitch(pitch);
        let r = Rect::from_min_size(pos2(grid_rect.left(), y_top - key_h), vec2(grid_rect.width(), key_h));
        if r.bottom() < grid_rect.top() || r.top() > grid_rect.bottom() {
            continue;
        }
        let fill = if is_black(pitch as i32) { ROW_BLACK } else { ROW_WHITE };
        g.rect_filled(r, 0.0, fill);
        g.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, Color32::from_rgb(0x23, 0x26, 0x2c)));
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
        g.line_segment([pos2(x, grid_rect.top()), pos2(x, grid_rect.bottom())], stroke);
    }

    // 3. Ruler Bar
    let gr = ui.painter_at(ruler_rect);
    gr.rect_filled(ruler_rect, 0.0, TOOLBAR_BG);
    gr.line_segment([ruler_rect.left_bottom(), ruler_rect.right_bottom()], Stroke::new(1.0, BORDER_COLOR));

    for beat in first_beat..=last_beat {
        let x = x_of(beat as f32);
        if x < ruler_rect.left() || x > ruler_rect.right() {
            continue;
        }
        if beat % 4 == 0 {
            let bar_num = beat / 4 + 1;
            gr.line_segment([pos2(x, ruler_rect.top() + 6.0), pos2(x, ruler_rect.bottom())], Stroke::new(1.0, Color32::from_gray(0x80)));
            gr.text(pos2(x + 4.0, ruler_rect.top() + 4.0), Align2::LEFT_TOP, format!("{bar_num}"), FontId::proportional(11.0), Color32::WHITE);
        } else {
            gr.line_segment([pos2(x, ruler_rect.top() + 12.0), pos2(x, ruler_rect.bottom())], Stroke::new(1.0, Color32::from_gray(0x40)));
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
        let stroke_col = if is_sel { ACCENT_BLUE_LIGHT } else { Color32::from_rgb(0x2e, 0x6e, 0x32) };

        g.rect_filled(nr, CornerRadius::same(3), fill_col);
        g.rect_stroke(nr, CornerRadius::same(3), Stroke::new(1.0, stroke_col), StrokeKind::Inside);

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
            g.rect_stroke(edit_rect, 3.0, Stroke::new(1.0, ACCENT_BLUE_LIGHT), StrokeKind::Inside);

            let edit_id = egui::Id::unique("inline_lyric_input");
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
                    // Shift+Tab: previous note
                    if edit_idx > 0 {
                        s.vocal_ed.editing_lyric = Some(edit_idx - 1);
                        s.vocal_ed.lyric_buffer = reg_mut.notes[edit_idx - 1].lyric.as_str().to_string();
                    } else {
                        s.vocal_ed.editing_lyric = None;
                    }
                } else {
                    // Tab: next note
                    if edit_idx + 1 < reg_mut.notes.len() {
                        s.vocal_ed.editing_lyric = Some(edit_idx + 1);
                        s.vocal_ed.lyric_buffer = reg_mut.notes[edit_idx + 1].lyric.as_str().to_string();
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
            else if s.vocal_ed.tool == VocalTool::Pencil && ui.input(|i| i.pointer.primary_clicked()) {
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
        let note_resp = ui.interact(*nr, egui::Id::unique(("vocal_note_resp", *idx)), Sense::click_and_drag());
        if note_resp.double_clicked() {
            s.vocal_ed.editing_lyric = Some(*idx);
            s.vocal_ed.lyric_buffer = p.tracks[ti].regions[ri].notes[*idx].lyric.as_str().to_string();
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
            // Acoustic Feedback audition
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

    // 8. Keyboard Shortcuts (Right/Left navigation, Up/Down semitone, Shift+Up/Down octave)
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
                let shift = if ui.input(|i| i.modifiers.shift) { 12 } else { 1 };
                let reg_mut = &mut p.tracks[ti].regions[ri];
                for sel in &s.vocal_ed.selected {
                    if let Some(n) = reg_mut.notes.get_mut(*sel) {
                        n.pitch = (n.pitch as i32 + shift).clamp(12, 110) as u8;
                    }
                }
                s.vocal_ed.dirty = true;
            } else if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                let shift = if ui.input(|i| i.modifiers.shift) { 12 } else { 1 };
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
                // Quantize selected notes
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
    gk.line_segment([keys_rect.right_top(), keys_rect.right_bottom()], Stroke::new(1.0, BORDER_COLOR));

    for pitch in min_pitch..=max_pitch {
        let y_top = y_of_pitch(pitch);
        let key_r = Rect::from_min_size(pos2(keys_rect.left(), y_top - key_h), vec2(keys_rect.width(), key_h));
        if key_r.bottom() < keys_rect.top() || key_r.top() > keys_rect.bottom() {
            continue;
        }

        let black = is_black(pitch as i32);
        let col = if black { Color32::from_rgb(0x18, 0x19, 0x1c) } else { Color32::from_rgb(0x2a, 0x2d, 0x33) };
        let resp = ui.interact(key_r, egui::Id::unique(("piano_key", pitch)), Sense::click());

        if resp.is_pointer_button_down_on() {
            gk.rect_filled(key_r, 0.0, ACCENT_BLUE);
            if s.vocal_ed.acoustic_feedback {
                s.audition.push(Audition::On { pitch, vel: 100, track: ti as u16 });
            }
        } else if resp.hovered() {
            gk.rect_filled(key_r, 0.0, Color32::from_white_alpha(15));
        } else {
            gk.rect_filled(key_r, 0.0, col);
        }
        gk.line_segment([key_r.left_bottom(), key_r.right_bottom()], Stroke::new(1.0, Color32::from_rgb(0x1a, 0x1c, 0x20)));

        // C labels
        if pitch % 12 == 0 {
            gk.text(
                pos2(key_r.right() - 6.0, key_r.center().y),
                Align2::RIGHT_CENTER,
                c_label(pitch as i32),
                FontId::proportional(10.0),
                Color32::WHITE,
            );
        }
    }

    // Playhead line
    let px = x_of(p.position_beats as f32);
    if px >= grid_rect.left() && px <= grid_rect.right() {
        g.line_segment(
            [pos2(px, grid_rect.top()), pos2(px, grid_rect.bottom())],
            Stroke::new(1.5, crate::tracks_area::playhead_colors(p.recording).0),
        );
    }
}

/// Synced Bottom Control Parameters Lane with Velocity Stems and Zoom Controls.
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
    g.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, BORDER_COLOR));

    // Left Parameter Selector Area
    let left_w = canvas_left - rect.left();
    let left_rect = Rect::from_min_size(rect.min, vec2(left_w, rect.height()));
    g.rect_filled(left_rect, 0.0, TOOLBAR_BG);
    g.line_segment([left_rect.right_top(), left_rect.right_bottom()], Stroke::new(1.0, BORDER_COLOR));

    // Parameter type label and toggle
    let cy = left_rect.center().y;
    let param_label = s.vocal_ed.param_type.label();
    let label_rect = Rect::from_min_size(pos2(left_rect.left() + 8.0, cy - 12.0), vec2(left_w - 16.0, 24.0));
    let resp = ui.interact(label_rect, egui::Id::unique("vocal_param_selector"), Sense::click())
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
        let cur = all.iter().position(|&p| p == s.vocal_ed.param_type).unwrap_or(0);
        s.vocal_ed.param_type = all[(cur + 1) % all.len()];
    }

    // Velocity Stems Lane
    let lane_rect = Rect::from_min_max(pos2(canvas_left, rect.top()), rect.max);
    g.rect_filled(lane_rect, 0.0, WELL_BG);

    let beat_px = s.vocal_ed.beat_px;
    let scroll_x = s.vocal_ed.scroll_x;
    let x_of = |beat: f32| lane_rect.left() + (beat - scroll_x) * beat_px;

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
            let col = if is_sel { ACCENT_BLUE_LIGHT } else { Color32::from_rgb(0x38, 0x8e, 0x3c) };

            let stem_bot = lane_rect.bottom() - 4.0;
            let stem_top = stem_bot - (n_vel as f32 / 127.0) * (lane_rect.height() - 14.0);

            // Stem line
            g.line_segment([pos2(nx, stem_bot), pos2(nx, stem_top)], Stroke::new(1.5, col));
            // Handle circle
            g.circle_filled(pos2(nx, stem_top), 3.5, if is_sel { Color32::WHITE } else { col });

            let handle_rect = Rect::from_center_size(pos2(nx, stem_top), vec2(10.0, 10.0));
            let stem_resp = ui.interact(handle_rect, egui::Id::unique(("vel_stem", idx)), Sense::drag());
            if stem_resp.dragged() {
                let delta = (-stem_resp.drag_delta().y / (lane_rect.height() - 14.0) * 127.0) as i32;
                let reg_mut = &mut p.tracks[ti].regions[ri];
                reg_mut.notes[idx].vel = (reg_mut.notes[idx].vel as i32 + delta).clamp(1, 127) as u8;
                s.vocal_ed.dirty = true;
            }
        }
    } else {
        // Continuous parameter midline
        let mid_y = lane_rect.center().y;
        g.line_segment([pos2(lane_rect.left(), mid_y), pos2(lane_rect.right(), mid_y)], Stroke::new(1.0, BORDER_COLOR));
    }

    // Zoom buttons in bottom right
    let zoom_w = 20.0;
    let zoom_h = 18.0;
    let right_y = lane_rect.bottom() - zoom_h - 4.0;

    let z_in_rect = Rect::from_min_size(pos2(lane_rect.right() - 26.0, right_y), vec2(zoom_w, zoom_h));
    let z_in_resp = ui.interact(z_in_rect, egui::Id::unique("vocal_zoom_in"), Sense::click()).on_hover_text("Zoom In");
    if z_in_resp.hovered() {
        g.rect_filled(z_in_rect, 3.0, Color32::from_white_alpha(20));
    }
    g.text(z_in_rect.center(), Align2::CENTER_CENTER, phosphor_egui::regular::MAGNIFYING_GLASS_PLUS, FontId::proportional(12.0), TEXT_DIM);
    if z_in_resp.clicked() {
        s.vocal_ed.beat_px = (s.vocal_ed.beat_px * 1.25).min(300.0);
    }

    let z_out_rect = Rect::from_min_size(pos2(lane_rect.right() - 50.0, right_y), vec2(zoom_w, zoom_h));
    let z_out_resp = ui.interact(z_out_rect, egui::Id::unique("vocal_zoom_out"), Sense::click()).on_hover_text("Zoom Out");
    if z_out_resp.hovered() {
        g.rect_filled(z_out_rect, 3.0, Color32::from_white_alpha(20));
    }
    g.text(z_out_rect.center(), Align2::CENTER_CENTER, phosphor_egui::regular::MAGNIFYING_GLASS_MINUS, FontId::proportional(12.0), TEXT_DIM);
    if z_out_resp.clicked() {
        s.vocal_ed.beat_px = (s.vocal_ed.beat_px / 1.25).max(20.0);
    }
}
