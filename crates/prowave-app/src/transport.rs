//! Control bar (Logic Pro: "Control Bar"): view buttons, transport, LCD, count-in/metronome,
//! master volume, and the editor/browser buttons on the right.
//!
//! Every icon is drawn with vector shapes here (no bitmap or font glyphs), so it renders the
//! same on Windows, Linux and macOS.
use crate::file_browser::Tab;
use crate::i18n::{tr, trf};
use crate::project_settings::LcdMode;
use crate::{
    model::{Project, SigPart},
    Bottom, UiState,
};
use egui::epaint::text::{LayoutJob, TextFormat};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui,
};

const BAR_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a);
const ICON: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
const ICON_ON: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xeb);
const ICON_DIM: Color32 = Color32::from_rgb(0x55, 0x55, 0x5a);
const LCD_BG: Color32 = Color32::from_rgb(0x11, 0x11, 0x13);
const LCD_TEXT: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xeb);
const LCD_MID: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
const LCD_DIM: Color32 = Color32::from_rgb(0x42, 0x42, 0x48);
const PLAY_GREEN: Color32 = Color32::from_rgb(0x30, 0xd1, 0x58);
/// The LCD while recording turns dark crimson, as in Logic.
const LCD_REC_BG: Color32 = Color32::from_rgb(32, 14, 16);
const LCD_REC_TEXT: Color32 = Color32::from_rgb(244, 98, 86);
const LCD_REC_MID: Color32 = Color32::from_rgb(170, 62, 54);
const LCD_REC_DIM: Color32 = Color32::from_rgb(84, 30, 28);
/// The record button's background while recording.
const REC_BUTTON_ON: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);

/// Stop (0, the Stop key): stops the song. When it has stopped already, the playhead goes to
/// the start of the project, or to the left locator while Cycle is on, as in Logic.
pub(crate) fn stop(p: &mut Project) {
    if p.playing {
        p.playing = false;
    } else if p.cycle {
        p.position_beats = p.cycle_start;
    } else {
        p.position_beats = 0.0;
    }
}

/// Bars that Fast Rewind and Fast Forward move by (Logic: eight).
pub(crate) const FAST_BARS: u32 = 8;

/// Rewind (Comma): back to the start of this bar, or of the one before when already there;
/// `bars` times.
pub(crate) fn rewind(p: &mut Project, bars: u32) {
    for _ in 0..bars {
        let bar = p.beats_to_bar(p.position_beats);
        let back = if bar.fract() < 1e-6 {
            bar - 1.0
        } else {
            bar.floor()
        };
        p.position_beats = p.bar_to_beats(back.max(0.0));
    }
}

/// Forward (Period): on to the start of the next bar; `bars` times.
pub(crate) fn forward(p: &mut Project, bars: u32) {
    for _ in 0..bars {
        p.position_beats = p.next_bar_start(p.position_beats);
    }
}

/// The LCD's colours: normal or recording.
struct LcdColors {
    bg: Color32,
    text: Color32,
    mid: Color32,
    dim: Color32,
}

impl LcdColors {
    fn new(recording: bool) -> Self {
        if recording {
            Self {
                bg: LCD_REC_BG,
                text: LCD_REC_TEXT,
                mid: LCD_REC_MID,
                dim: LCD_REC_DIM,
            }
        } else {
            Self {
                bg: LCD_BG,
                text: LCD_TEXT,
                mid: LCD_MID,
                dim: LCD_DIM,
            }
        }
    }
}

const REC_RED: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);
const CYCLE_YELLOW: Color32 = Color32::from_rgb(0xff, 0xd6, 0x0a);
const PURPLE: Color32 = Color32::from_rgb(0x3a, 0x7b, 0xd5);

fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.4, c)
}

fn clicked(ui: &Ui, rect: Rect, key: &str) -> bool {
    let resp = ui.interact(rect, egui::Id::unique(("control_bar", key)), Sense::click());
    tip(&resp, key);
    resp.clicked()
}

/// Shows what the button with `key` does when the pointer rests on it (issue #118).
fn tip(resp: &egui::Response, key: &str) {
    if let Some(id) = quick_help_id(key) {
        crate::quick_help::hover(&resp.ctx, resp, id);
    }
    if let Some(text) = crate::tooltips::bar_tip(key) {
        resp.clone().on_hover_text(tr(text));
    }
}

fn quick_help_id(key: &str) -> Option<&'static str> {
    Some(match key {
        "view0" => "view0",
        "view1" => "view1",
        "view2" => "view2",
        "view3" => "view3",
        "panes0" => "panes0",
        "panes1" => "panes1",
        "panes2" => "panes2",
        "right0" => "right0",
        "right1" => "right1",
        "right2" => "right2",
        "right3" => "right3",
        "rew" => "rew",
        "ffw" => "ffw",
        "start" => "start",
        "play" => "play",
        "rec" => "rec",
        "capture" => "capture",
        "cycle" => "cycle",
        "lowlat" => "lowlat",
        "tuner" => "tuner",
        "countin" => "countin",
        "metronome" => "metronome",
        _ => return None,
    })
}

// ---------------------------------------------------------------- icons

type IconFn = fn(&Painter, Rect, Color32, bool);

fn icon_library(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::ARCHIVE_BOX, 15.0, c, on);
}

fn icon_inspector(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::INFO, 15.0, c, on);
}

fn icon_help(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::QUESTION, 15.0, c, on);
}

fn icon_toolbar(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::APP_WINDOW, 15.0, c, on);
}

fn icon_smart_controls(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::SLIDERS, 15.0, c, on);
}

fn icon_mixer(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::FADERS, 15.0, c, on);
}

fn icon_editors(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::SCISSORS, 15.0, c, on);
}

fn icon_rewind(g: &Painter, r: Rect, c: Color32) {
    crate::icons::paint_fill(g, r, crate::icons::REWIND, 15.0, c);
}

fn icon_forward(g: &Painter, r: Rect, c: Color32) {
    crate::icons::paint_fill(g, r, crate::icons::FAST_FORWARD, 15.0, c);
}

fn icon_to_start(g: &Painter, r: Rect, c: Color32) {
    crate::icons::paint_fill(g, r, crate::icons::SKIP_BACK, 15.0, c);
}

fn icon_stop(g: &Painter, r: Rect, c: Color32) {
    crate::icons::paint_fill(g, r, crate::icons::STOP, 15.0, c);
}

fn icon_play(g: &Painter, r: Rect, c: Color32) {
    crate::icons::paint_fill(g, r, crate::icons::PLAY, 15.0, c);
}

fn icon_record(g: &Painter, r: Rect, c: Color32) {
    crate::icons::paint_fill(g, r, crate::icons::RECORD, 15.0, c);
}

fn icon_cycle(g: &Painter, r: Rect, c: Color32, on: bool) {
    if on {
        crate::icons::paint_bold(g, r, crate::icons::ARROWS_CLOCKWISE, 15.0, c);
    } else {
        crate::icons::paint_regular(g, r, crate::icons::ARROWS_CLOCKWISE, 15.0, c);
    }
}

fn icon_tuner(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::SLIDERS_HORIZONTAL, 14.0, c, on);
}

fn icon_metronome(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::METRONOME, 15.0, c, on);
}

fn icon_list_editors(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::LIST_BULLETS, 15.0, c, on);
}

fn icon_note_pad(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::NOTE_BLANK, 15.0, c, on);
}

fn icon_loops(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::REPEAT, 15.0, c, on);
}

fn icon_browsers(g: &Painter, r: Rect, c: Color32, on: bool) {
    crate::icons::paint(g, r, crate::icons::FOLDER_OPEN, 15.0, c, on);
}

// ---------------------------------------------------------------- widgets

/// A joined segmented control (Logic's grouped toolbar buttons). Returns the clicked index.
fn segmented(
    ui: &Ui,
    g: &Painter,
    left: f32,
    cy: f32,
    key: &str,
    items: &[(IconFn, bool)],
    quality: crate::liquid_glass::GlassQuality,
) -> Option<usize> {
    let seg = vec2(30.0, 24.0);
    let pad_x = 4.0;
    let full = Rect::from_min_size(
        pos2(left, cy - 14.0),
        vec2(pad_x * 2.0 + seg.x * items.len() as f32, 28.0),
    );
    crate::liquid_glass::paint_glass_pill(ui, full, 14.0, quality, false, None, None);
    let mut hit = None;
    for (i, (draw, on)) in items.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(full.left() + pad_x + i as f32 * seg.x, cy - seg.y / 2.0),
            seg,
        );
        let cr = CornerRadius {
            nw: if i == 0 { 10 } else { 3 },
            sw: if i == 0 { 10 } else { 3 },
            ne: if i + 1 == items.len() { 10 } else { 3 },
            se: if i + 1 == items.len() { 10 } else { 3 },
        };
        let resp = ui.interact(
            r,
            egui::Id::unique(("control_bar_hover", key, i)),
            Sense::click(),
        );
        tip(&resp, &format!("{key}{i}"));
        if *on {
            g.rect_filled(r.shrink(1.0), cr, Color32::from_rgb(0x3a, 0x3a, 0x3e));
            g.rect_stroke(
                r.shrink(1.0),
                cr,
                Stroke::new(1.0, Color32::from_rgb(0x52, 0x52, 0x58)),
                StrokeKind::Inside,
            );
        } else if resp.hovered() {
            g.rect_filled(
                r.shrink(1.0),
                cr,
                Color32::from_rgba_unmultiplied(255, 255, 255, 18),
            );
        }
        if i > 0 {
            g.line_segment(
                [
                    pos2(r.left(), r.top() + 4.0),
                    pos2(r.left(), r.bottom() - 4.0),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 15)),
            );
        }
        let icon_col = if *on {
            Color32::from_rgb(0xf5, 0xf5, 0xf7)
        } else {
            Color32::from_rgb(0x8e, 0x8e, 0x93)
        };
        draw(g, r, icon_col, *on);
        if resp.clicked() {
            hit = Some(i);
        }
    }
    hit
}

/// Transport-style button: icon only, or icon on a coloured rounded square when active.
fn tbutton(
    ui: &Ui,
    g: &Painter,
    center: Pos2,
    key: &str,
    draw: IconFn,
    active_bg: Option<Color32>,
    icon: Color32,
) -> bool {
    let r = Rect::from_center_size(center, vec2(30.0, 26.0));
    let resp = ui.interact(r, egui::Id::unique(("control_bar", key)), Sense::click());
    tip(&resp, key);
    if let Some(bg) = active_bg {
        g.rect_filled(r, 6.0, bg);
        g.rect_stroke(
            r,
            6.0,
            Stroke::new(1.0, bg.lerp_to_gamma(Color32::WHITE, 0.35)),
            StrokeKind::Inside,
        );
    } else if resp.hovered() {
        g.rect_filled(r, 6.0, Color32::from_white_alpha(16));
    }
    draw(g, r, icon, active_bg.is_some());
    resp.clicked()
}

/// Appends `text`, drawing the first `dim` characters in the dim LCD colour.
fn lcd_seg(
    job: &mut LayoutJob,
    text: &str,
    dim: usize,
    size: f32,
    color: Color32,
    dim_color: Color32,
) {
    let split = text.char_indices().nth(dim).map_or(text.len(), |(i, _)| i);
    let font = FontId::monospace(size);
    if split > 0 {
        job.append(
            &text[..split],
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: dim_color,
                ..Default::default()
            },
        );
    }
    job.append(
        &text[split..],
        0.0,
        TextFormat {
            font_id: font,
            color,
            ..Default::default()
        },
    );
}

/// "009 1 1 001"-style position with dim leading zeros.
fn position_job(
    bbdt: (u32, u32, u32, u32),
    size: f32,
    color: Color32,
    dim_color: Color32,
) -> LayoutJob {
    let (bar, beat, div, tick) = bbdt;
    let mut job = LayoutJob::default();
    let bar_s = format!("{bar:03}");
    let bar_dim = bar_s.len() - bar.to_string().len();
    lcd_seg(&mut job, &bar_s, bar_dim, size, color, dim_color);
    lcd_seg(
        &mut job,
        &format!("  {beat}  {div}  "),
        0,
        size,
        color,
        dim_color,
    );
    let tick_s = format!("{tick:03}");
    let tick_dim = tick_s.len() - tick.to_string().len();
    lcd_seg(&mut job, &tick_s, tick_dim, size, color, dim_color);
    job
}

/// Pixels of vertical drag per step when editing the time signature.
const SIG_PX_PER_STEP: f32 = 10.0;

/// Adds `delta_px` to the drag in progress and takes out the whole steps it now holds, keeping
/// the remainder for the next frame so slow drags still add up.
fn take_steps(leftover: &mut f32, delta_px: f32, px_per_step: f32) -> i32 {
    *leftover += delta_px;
    let steps = (*leftover / px_per_step).trunc();
    *leftover -= steps * px_per_step;
    steps as i32
}

fn lcd(
    ui: &mut Ui,
    g: &Painter,
    p: &mut Project,
    s: &mut UiState,
    rect: Rect,
    quality: crate::liquid_glass::GlassQuality,
) {
    let pal = LcdColors::new(p.recording);
    crate::liquid_glass::paint_glass_pill(
        ui,
        rect,
        10.0,
        quality,
        true,
        Some(pal.bg),
        Some(Color32::from_rgb(0x28, 0x28, 0x2c)),
    );
    let top = rect.top() + 4.0;
    let bot = rect.center().y + 1.0;
    let x0 = rect.left();

    // Display area according to active LcdMode
    match s.lcd_mode {
        LcdMode::BeatsAndProject => {
            // Column 1: Time display (clickable to toggle between Bar/Beat and SMPTE Timecode)
            let time_rect = Rect::from_min_size(
                pos2(x0 + 6.0, rect.top() + 2.0),
                vec2(184.0, rect.height() - 4.0),
            );
            let time_resp = ui
                .interact(
                    time_rect,
                    egui::Id::unique(("control_bar", "time_display")),
                    Sense::click(),
                )
                .on_hover_text(tr("Right-click for the SMPTE and synchronization settings"));
            crate::widgets::context_menu(&time_resp, |ui| crate::sync_menu::items(ui, p));
            if time_resp.clicked() {
                s.lcd_smpte = !s.lcd_smpte;
            }
            crate::quick_help::hover(ui.ctx(), &time_resp, "lcd_pos");

            if s.lcd_smpte {
                let mut job = LayoutJob::default();
                lcd_seg(&mut job, &p.timecode(), 0, 18.0, pal.text, pal.dim);
                g.galley(pos2(x0 + 12.0, top + 1.0), g.layout_job(job), pal.text);
                g.text(
                    pos2(x0 + 14.0, bot + 2.0),
                    Align2::LEFT_TOP,
                    "SMPTE TIMECODE (CLICK FOR BARS)",
                    FontId::proportional(8.5),
                    pal.dim,
                );
            } else {
                let (bar, beat, div, tick) = p.bbdt(p.position_beats);
                let cols = [
                    (format!("{bar}"), "BAR"),
                    (format!("{beat}"), "BEAT"),
                    (format!("{div}"), "DIV"),
                    (format!("{tick}"), "TICK"),
                ];
                let col_w = time_rect.width() / 4.0;
                for (i, (val, label)) in cols.iter().enumerate() {
                    let cx = time_rect.left() + (i as f32 + 0.5) * col_w;
                    g.text(
                        pos2(cx, top + 1.0),
                        Align2::CENTER_TOP,
                        val,
                        FontId::monospace(17.0),
                        pal.text,
                    );
                    g.text(
                        pos2(cx, bot + 2.0),
                        Align2::CENTER_TOP,
                        *label,
                        FontId::proportional(8.5),
                        pal.dim,
                    );
                }
            }

            // Divider line 1
            g.line_segment(
                [
                    pos2(x0 + 196.0, rect.top() + 7.0),
                    pos2(x0 + 196.0, rect.bottom() - 7.0),
                ],
                Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
            );

            // Column 2: Tempo (drag vertically to adjust)
            let tempo_rect = Rect::from_min_size(
                pos2(x0 + 204.0, rect.top() + 2.0),
                vec2(92.0, rect.height() - 4.0),
            );
            let resp = ui.interact(
                tempo_rect,
                egui::Id::unique(("control_bar", "tempo")),
                Sense::drag(),
            );
            crate::quick_help::hover(ui.ctx(), &resp, "lcd_tempo");
            let tempo_now = p.tempo_at(p.position_beats);
            if resp.dragged() {
                p.set_tempo_at(p.position_beats, tempo_now - resp.drag_delta().y * 0.5);
            }
            g.text(
                pos2(tempo_rect.center().x, top + 1.0),
                Align2::CENTER_TOP,
                format!("{:.1}", p.tempo_at(p.position_beats)),
                FontId::monospace(17.0),
                pal.text,
            );
            g.text(
                pos2(tempo_rect.center().x, bot + 2.0),
                Align2::CENTER_TOP,
                tr("Keep Tempo"),
                FontId::proportional(9.0),
                pal.mid,
            );

            // Divider line 2
            g.line_segment(
                [
                    pos2(x0 + 304.0, rect.top() + 7.0),
                    pos2(x0 + 304.0, rect.bottom() - 7.0),
                ],
                Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
            );

            // Column 3: Time Signature (drag numerator or denominator to adjust)
            let sig_x = x0 + 352.0;
            let half = vec2(22.0, bot - rect.top());
            for (part, left) in [
                (SigPart::Numerator, sig_x - 22.0),
                (SigPart::Denominator, sig_x),
            ] {
                let id = egui::Id::unique(("control_bar", "signature", part as u8));
                let resp = ui
                    .interact(
                        Rect::from_min_size(pos2(left, rect.top() + 2.0), half),
                        id,
                        Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                    .on_hover_text(tr("Drag to change the time signature"));
                crate::quick_help::hover(ui.ctx(), &resp, "lcd_sig");
                let mut leftover = ui.data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0);
                if resp.drag_started() {
                    leftover = 0.0;
                }
                let steps = if resp.dragged() {
                    take_steps(&mut leftover, -resp.drag_delta().y, SIG_PX_PER_STEP)
                } else {
                    0
                };
                ui.data_mut(|d| d.insert_temp(id, leftover));
                if steps != 0 {
                    let bar0 = p.beats_to_bar(p.position_beats) as u32;
                    p.step_signature_at(bar0, part, steps);
                }
            }
            let (num, den) = p.signature_of_bar(p.beats_to_bar(p.position_beats) as u32);
            g.text(
                pos2(sig_x, top + 1.0),
                Align2::CENTER_TOP,
                format!("{num}/{den}"),
                FontId::monospace(17.0),
                pal.text,
            );
            let (active_root, active_scale) = p.key_at(p.position_beats);
            let root_str = crate::note_edit::ROOT_SHORTS[active_root.min(11) as usize];
            let scale_str = crate::note_edit::MODE_SHORTS[active_scale.min(11) as usize];
            let key_display = format!("{root_str} {scale_str} /16");
            let key_rect = Rect::from_min_max(
                pos2(sig_x - 22.0, bot),
                pos2(sig_x + 22.0, rect.bottom() - 2.0),
            );
            let key_resp = ui
                .interact(
                    key_rect,
                    egui::Id::unique(("control_bar", "key_scale_sig")),
                    Sense::click(),
                )
                .on_hover_text(format!(
                    "Key Signature: {} {}\nClick to change root and scale",
                    crate::note_edit::ROOTS[active_root.min(11) as usize],
                    crate::note_edit::MODES[active_scale.min(11) as usize].0
                ));
            if key_resp.hovered() {
                g.rect_filled(key_rect, 2.0, Color32::from_white_alpha(18));
            }
            g.text(
                pos2(sig_x, bot + 2.0),
                Align2::CENTER_TOP,
                key_display,
                FontId::proportional(9.0),
                if key_resp.hovered() {
                    pal.text
                } else {
                    pal.mid
                },
            );
            key_scale_popup_menu(ui, &key_resp, p, s);
        }
        LcdMode::BeatsAndTime => {
            // Left: Bars & Beats
            let time_rect = Rect::from_min_size(
                pos2(x0 + 6.0, rect.top() + 2.0),
                vec2(188.0, rect.height() - 4.0),
            );
            let (bar, beat, div, tick) = p.bbdt(p.position_beats);
            let cols = [
                (format!("{bar}"), "BAR"),
                (format!("{beat}"), "BEAT"),
                (format!("{div}"), "DIV"),
                (format!("{tick}"), "TICK"),
            ];
            let col_w = time_rect.width() / 4.0;
            for (i, (val, label)) in cols.iter().enumerate() {
                let cx = time_rect.left() + (i as f32 + 0.5) * col_w;
                g.text(
                    pos2(cx, top + 1.0),
                    Align2::CENTER_TOP,
                    val,
                    FontId::monospace(17.0),
                    pal.text,
                );
                g.text(
                    pos2(cx, bot + 2.0),
                    Align2::CENTER_TOP,
                    *label,
                    FontId::proportional(8.5),
                    pal.dim,
                );
            }

            // Divider
            g.line_segment(
                [
                    pos2(x0 + 198.0, rect.top() + 7.0),
                    pos2(x0 + 198.0, rect.bottom() - 7.0),
                ],
                Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
            );

            // Right: Timecode
            let mut job = LayoutJob::default();
            lcd_seg(&mut job, &p.timecode(), 0, 18.0, pal.text, pal.dim);
            g.galley(pos2(x0 + 208.0, top + 1.0), g.layout_job(job), pal.text);
            g.text(
                pos2(x0 + 210.0, bot + 2.0),
                Align2::LEFT_TOP,
                "SMPTE TIMECODE",
                FontId::proportional(8.5),
                pal.dim,
            );
        }
        LcdMode::Beats => {
            // Full width musical beats
            let full_rect = Rect::from_min_size(
                pos2(x0 + 10.0, rect.top() + 2.0),
                vec2(380.0, rect.height() - 4.0),
            );
            let (bar, beat, div, tick) = p.bbdt(p.position_beats);
            let cols = [
                (format!("{bar}"), "BAR"),
                (format!("{beat}"), "BEAT"),
                (format!("{div}"), "DIV"),
                (format!("{tick}"), "TICK"),
            ];
            let col_w = full_rect.width() / 4.0;
            for (i, (val, label)) in cols.iter().enumerate() {
                let cx = full_rect.left() + (i as f32 + 0.5) * col_w;
                g.text(
                    pos2(cx, top),
                    Align2::CENTER_TOP,
                    val,
                    FontId::monospace(19.0),
                    pal.text,
                );
                g.text(
                    pos2(cx, bot + 2.0),
                    Align2::CENTER_TOP,
                    *label,
                    FontId::proportional(9.0),
                    pal.dim,
                );
            }
        }
        LcdMode::Time => {
            // Full width SMPTE timecode
            let mut job = LayoutJob::default();
            lcd_seg(&mut job, &p.timecode(), 0, 20.0, pal.text, pal.dim);
            let galley = g.layout_job(job);
            let gx = x0 + 200.0 - galley.size().x * 0.5;
            g.galley(pos2(gx, top), galley, pal.text);
            g.text(
                pos2(x0 + 200.0, bot + 2.0),
                Align2::CENTER_TOP,
                "SMPTE TIMECODE",
                FontId::proportional(9.0),
                pal.dim,
            );
        }
        LcdMode::Custom => {
            // Custom dynamic columns
            let opt = s.lcd_custom;
            let (bar, beat, div, tick) = p.bbdt(p.position_beats);
            let cols = if opt.show_division_ticks {
                vec![
                    (format!("{bar}"), "BAR"),
                    (format!("{beat}"), "BEAT"),
                    (format!("{div}"), "DIV"),
                    (format!("{tick}"), "TICK"),
                ]
            } else {
                vec![(format!("{bar}"), "BAR"), (format!("{beat}"), "BEAT")]
            };
            let left_w = if opt.show_division_ticks { 156.0 } else { 96.0 };
            let col_w = left_w / cols.len() as f32;
            for (i, (val, label)) in cols.iter().enumerate() {
                let cx = x0 + 8.0 + (i as f32 + 0.5) * col_w;
                g.text(
                    pos2(cx, top + 1.0),
                    Align2::CENTER_TOP,
                    val,
                    FontId::monospace(16.0),
                    pal.text,
                );
                g.text(
                    pos2(cx, bot + 2.0),
                    Align2::CENTER_TOP,
                    *label,
                    FontId::proportional(8.0),
                    pal.dim,
                );
            }

            let mut cur_x = x0 + left_w + 10.0;

            if opt.show_tempo {
                g.line_segment(
                    [
                        pos2(cur_x, rect.top() + 7.0),
                        pos2(cur_x, rect.bottom() - 7.0),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
                );
                cur_x += 6.0;
                let tempo_w = 60.0;
                let tempo_rect = Rect::from_min_size(
                    pos2(cur_x, rect.top() + 2.0),
                    vec2(tempo_w, rect.height() - 4.0),
                );
                let resp = ui.interact(
                    tempo_rect,
                    egui::Id::unique(("lcd", "tempo_custom")),
                    Sense::drag(),
                );
                let tempo_now = p.tempo_at(p.position_beats);
                if resp.dragged() {
                    p.set_tempo_at(p.position_beats, tempo_now - resp.drag_delta().y * 0.5);
                }
                g.text(
                    pos2(cur_x + tempo_w * 0.5, top + 1.0),
                    Align2::CENTER_TOP,
                    format!("{:.1}", p.tempo_at(p.position_beats)),
                    FontId::monospace(15.0),
                    pal.text,
                );
                g.text(
                    pos2(cur_x + tempo_w * 0.5, bot + 2.0),
                    Align2::CENTER_TOP,
                    "BPM",
                    FontId::proportional(8.5),
                    pal.mid,
                );
                cur_x += tempo_w + 6.0;
            }

            if opt.show_time_signature || opt.show_key_signature {
                g.line_segment(
                    [
                        pos2(cur_x, rect.top() + 7.0),
                        pos2(cur_x, rect.bottom() - 7.0),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
                );
                cur_x += 6.0;
                let sig_w = 46.0;
                let (num, den) = p.signature_of_bar(p.beats_to_bar(p.position_beats) as u32);
                if opt.show_time_signature {
                    g.text(
                        pos2(cur_x + sig_w * 0.5, top + 1.0),
                        Align2::CENTER_TOP,
                        format!("{num}/{den}"),
                        FontId::monospace(15.0),
                        pal.text,
                    );
                }
                if opt.show_key_signature {
                    let (active_root, active_scale) = p.key_at(p.position_beats);
                    let root_str = crate::note_edit::ROOT_SHORTS[active_root.min(11) as usize];
                    let scale_str = crate::note_edit::MODE_SHORTS[active_scale.min(11) as usize];
                    let key_display = format!("{root_str} {scale_str}");
                    let key_rect = Rect::from_min_max(
                        pos2(cur_x, bot),
                        pos2(cur_x + sig_w, rect.bottom() - 2.0),
                    );
                    let key_resp = ui
                        .interact(
                            key_rect,
                            egui::Id::unique(("control_bar", "key_scale_custom")),
                            Sense::click(),
                        )
                        .on_hover_text(format!(
                            "Key Signature: {} {}\nClick to change root and scale",
                            crate::note_edit::ROOTS[active_root.min(11) as usize],
                            crate::note_edit::MODES[active_scale.min(11) as usize].0
                        ));
                    if key_resp.hovered() {
                        g.rect_filled(key_rect, 2.0, Color32::from_white_alpha(18));
                    }
                    g.text(
                        pos2(cur_x + sig_w * 0.5, bot + 2.0),
                        Align2::CENTER_TOP,
                        key_display,
                        FontId::proportional(8.5),
                        if key_resp.hovered() {
                            pal.text
                        } else {
                            pal.mid
                        },
                    );
                    key_scale_popup_menu(ui, &key_resp, p, s);
                }
                cur_x += sig_w + 6.0;
            }

            if opt.show_performance_meters {
                g.line_segment(
                    [
                        pos2(cur_x, rect.top() + 7.0),
                        pos2(cur_x, rect.bottom() - 7.0),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
                );
                cur_x += 6.0;
                let meter_w = 42.0;
                // CPU meter
                g.text(
                    pos2(cur_x, top + 1.0),
                    Align2::LEFT_TOP,
                    "CPU",
                    FontId::proportional(8.0),
                    pal.mid,
                );
                let cpu_bar_r = Rect::from_min_size(pos2(cur_x + 20.0, top + 4.0), vec2(20.0, 5.0));
                g.rect_filled(cpu_bar_r, 1.0, Color32::from_rgb(0x20, 0x20, 0x24));
                let cpu_fill_w = cpu_bar_r.width() * s.cpu_load.clamp(0.0, 1.0);
                if cpu_fill_w > 0.5 {
                    let col = if s.cpu_load > 0.85 {
                        Color32::from_rgb(0xff, 0x45, 0x3a)
                    } else {
                        Color32::from_rgb(0x30, 0xd1, 0x58)
                    };
                    g.rect_filled(
                        Rect::from_min_size(cpu_bar_r.min, vec2(cpu_fill_w, 5.0)),
                        1.0,
                        col,
                    );
                }

                // HD meter
                g.text(
                    pos2(cur_x, bot + 2.0),
                    Align2::LEFT_TOP,
                    "HD",
                    FontId::proportional(8.0),
                    pal.mid,
                );
                let hd_bar_r = Rect::from_min_size(pos2(cur_x + 20.0, bot + 5.0), vec2(20.0, 5.0));
                g.rect_filled(hd_bar_r, 1.0, Color32::from_rgb(0x20, 0x20, 0x24));
                let hd_fill_w = hd_bar_r.width() * s.hd_load.clamp(0.0, 1.0);
                if hd_fill_w > 0.5 {
                    let col = if s.hd_load > 0.85 {
                        Color32::from_rgb(0xff, 0x45, 0x3a)
                    } else {
                        Color32::from_rgb(0x0a, 0x84, 0xff)
                    };
                    g.rect_filled(
                        Rect::from_min_size(hd_bar_r.min, vec2(hd_fill_w, 5.0)),
                        1.0,
                        col,
                    );
                }
                cur_x += meter_w + 6.0;
            }

            if opt.show_midi_activity {
                g.line_segment(
                    [
                        pos2(cur_x, rect.top() + 7.0),
                        pos2(cur_x, rect.bottom() - 7.0),
                    ],
                    Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
                );
                cur_x += 6.0;
                // MIDI In
                let in_active = s.midi_in_activity > 0.05;
                let in_dot = pos2(cur_x + 4.0, top + 6.0);
                g.circle_filled(
                    in_dot,
                    3.0,
                    if in_active {
                        Color32::from_rgb(0x30, 0xd1, 0x58)
                    } else {
                        Color32::from_rgb(0x28, 0x28, 0x2c)
                    },
                );
                g.text(
                    pos2(cur_x + 10.0, top + 1.0),
                    Align2::LEFT_TOP,
                    "IN",
                    FontId::proportional(7.5),
                    pal.mid,
                );

                // MIDI Out
                let out_active = s.midi_out_activity > 0.05;
                let out_dot = pos2(cur_x + 4.0, bot + 7.0);
                g.circle_filled(
                    out_dot,
                    3.0,
                    if out_active {
                        Color32::from_rgb(0x30, 0xd1, 0x58)
                    } else {
                        Color32::from_rgb(0x28, 0x28, 0x2c)
                    },
                );
                g.text(
                    pos2(cur_x + 10.0, bot + 2.0),
                    Align2::LEFT_TOP,
                    "OUT",
                    FontId::proportional(7.5),
                    pal.mid,
                );
            }
        }
    }

    // Divider line 3
    g.line_segment(
        [
            pos2(x0 + 400.0, rect.top() + 7.0),
            pos2(x0 + 400.0, rect.bottom() - 7.0),
        ],
        Stroke::new(1.0, Color32::from_rgb(0x28, 0x28, 0x2c)),
    );

    // Column 4: Docked mode buttons (Count-in, Metronome, Tuner / Display Mode)
    let ci_r = Rect::from_center_size(pos2(x0 + 430.0, rect.center().y), vec2(36.0, 24.0));
    let ci_resp = ui.interact(ci_r, egui::Id::unique(("lcd", "ci")), Sense::click());
    if p.count_in {
        g.rect_filled(
            ci_r,
            4.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 36),
        );
        g.rect_stroke(
            ci_r,
            4.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
            StrokeKind::Inside,
        );
    } else if ci_resp.hovered() {
        g.rect_filled(
            ci_r,
            4.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 18),
        );
    }
    g.text(
        ci_r.center(),
        Align2::CENTER_CENTER,
        "1234",
        FontId::proportional(11.0),
        if p.count_in {
            Color32::from_rgb(0xf5, 0xf5, 0xf7)
        } else {
            pal.mid
        },
    );
    if ci_resp.clicked() {
        p.count_in = !p.count_in;
    }

    let met_r = Rect::from_center_size(pos2(x0 + 472.0, rect.center().y), vec2(34.0, 24.0));
    let met_resp = ui.interact(met_r, egui::Id::unique(("lcd", "met")), Sense::click());
    if p.metronome {
        g.rect_filled(
            met_r,
            4.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 36),
        );
        g.rect_stroke(
            met_r,
            4.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
            StrokeKind::Inside,
        );
    } else if met_resp.hovered() {
        g.rect_filled(
            met_r,
            4.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 18),
        );
    }
    icon_metronome(
        g,
        met_r,
        if p.metronome {
            Color32::from_rgb(0xf5, 0xf5, 0xf7)
        } else {
            pal.mid
        },
        p.metronome,
    );
    if met_resp.clicked() {
        p.metronome = !p.metronome;
    }

    let tun_r = Rect::from_center_size(pos2(x0 + 508.0, rect.center().y), vec2(28.0, 24.0));
    let tun_resp = ui
        .interact(
            tun_r,
            egui::Id::unique(("lcd", "display_mode")),
            Sense::click(),
        )
        .on_hover_text("Display Mode & Customization");
    if tun_resp.hovered() {
        g.rect_filled(tun_r, 4.0, Color32::from_white_alpha(15));
    }
    let is_custom_or_alt = s.lcd_mode != LcdMode::BeatsAndProject;
    let mode_color = if is_custom_or_alt {
        Color32::from_rgb(0x3B, 0x82, 0xF6)
    } else {
        pal.dim
    };
    icon_tuner(g, tun_r, mode_color, is_custom_or_alt);

    // Varispeed toggle button (-+)
    let vs_r = Rect::from_center_size(pos2(x0 + 538.0, rect.center().y), vec2(28.0, 24.0));
    let vs_resp = ui
        .interact(
            vs_r,
            egui::Id::unique(("lcd", "varispeed_btn")),
            Sense::click(),
        )
        .on_hover_text(tr("Varispeed (-50% to +100%)"));
    let vs_amber = Color32::from_rgb(0xf5, 0x9e, 0x0b);
    if p.varispeed {
        g.rect_filled(
            vs_r,
            4.0,
            Color32::from_rgba_unmultiplied(0xf5, 0x9e, 0x0b, 48),
        );
        g.rect_stroke(
            vs_r,
            4.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xf5, 0x9e, 0x0b, 140)),
            StrokeKind::Inside,
        );
    } else if vs_resp.hovered() {
        g.rect_filled(vs_r, 4.0, Color32::from_white_alpha(15));
    }
    g.text(
        vs_r.center(),
        Align2::CENTER_CENTER,
        "-+",
        FontId::monospace(11.5),
        if p.varispeed { vs_amber } else { pal.mid },
    );
    if vs_resp.clicked() {
        p.varispeed = !p.varispeed;
    }

    // Varispeed two-part readout when enabled or customized into the Control Bar
    if p.varispeed || s.lcd_custom.show_varispeed {
        let readout_r = Rect::from_min_size(
            pos2(x0 + 556.0, rect.top() + 2.0),
            vec2(104.0, rect.height() - 4.0),
        );
        let upper_r =
            Rect::from_min_max(readout_r.min, pos2(readout_r.right(), readout_r.center().y));
        let lower_r =
            Rect::from_min_max(pos2(readout_r.left(), readout_r.center().y), readout_r.max);
        let upper_resp = ui
            .interact(
                upper_r,
                egui::Id::unique(("lcd", "vs_mode")),
                Sense::click(),
            )
            .on_hover_text(tr("Varispeed Mode"));
        let lower_resp = ui
            .interact(
                lower_r,
                egui::Id::unique(("lcd", "vs_val")),
                Sense::click_and_drag(),
            )
            .on_hover_text(tr(
                "Drag vertically or double-click to adjust Varispeed; click to choose unit",
            ));

        let rate = prowave_engine::varispeed_rate(p.varispeed_pct);
        let val_str = match p.varispeed_unit {
            prowave_engine::VarispeedUnit::Percentage => format!("{:+.2}%", p.varispeed_pct),
            prowave_engine::VarispeedUnit::ResultingTempo => {
                format!("{:.2} bpm", p.tempo_at(p.position_beats) as f64 * rate)
            }
            prowave_engine::VarispeedUnit::Detune => {
                format!("{:+.2} st", prowave_engine::rate_to_semitones(rate))
            }
            prowave_engine::VarispeedUnit::TuningReference => {
                format!("{:.1} Hz", f64::from(p.tuning_hz) * rate)
            }
        };
        let col = if p.varispeed { vs_amber } else { pal.dim };
        g.text(
            upper_r.center(),
            Align2::CENTER_CENTER,
            p.varispeed_mode.short_label(),
            FontId::proportional(9.5),
            col,
        );
        let edit_id = egui::Id::unique(("lcd", "vs_edit_open"));
        let buf_id = egui::Id::unique(("lcd", "vs_edit_buf"));
        let mut editing = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(edit_id).unwrap_or(false));
        if lower_resp.double_clicked() {
            editing = true;
            ui.ctx().data_mut(|d| {
                d.insert_temp(edit_id, true);
                d.insert_temp(buf_id, format!("{:.2}", p.varispeed_pct));
            });
        } else if lower_resp.dragged() {
            p.varispeed = true;
            p.varispeed_pct =
                (p.varispeed_pct - lower_resp.drag_delta().y * 0.4).clamp(-50.0, 100.0);
        }
        if editing {
            let mut buf = ui
                .ctx()
                .data(|d| d.get_temp::<String>(buf_id).unwrap_or_default());
            let te = ui.put(
                lower_r.shrink2(vec2(6.0, 1.0)),
                egui::TextEdit::singleline(&mut buf).font(FontId::monospace(10.5)),
            );
            ui.ctx().data_mut(|d| d.insert_temp(buf_id, buf.clone()));
            if te.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Ok(v) = buf.trim().trim_end_matches('%').parse::<f32>() {
                    p.varispeed_pct = v.clamp(-50.0, 100.0);
                    p.varispeed = true;
                }
                ui.ctx().data_mut(|d| d.insert_temp(edit_id, false));
            }
        } else {
            g.text(
                lower_r.center(),
                Align2::CENTER_CENTER,
                val_str,
                FontId::monospace(11.0),
                col,
            );
            egui::Popup::menu(&lower_resp).show(|ui| {
                for u in prowave_engine::VarispeedUnit::ALL {
                    if ui
                        .selectable_label(p.varispeed_unit == u, u.label())
                        .clicked()
                    {
                        p.varispeed_unit = u;
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tr("Reset Varispeed (0.00%)")).clicked() {
                    p.varispeed_pct = 0.0;
                    ui.close();
                }
            });
        }
        egui::Popup::menu(&upper_resp).show(|ui| {
            for m in prowave_engine::VarispeedMode::ALL {
                if ui
                    .selectable_label(p.varispeed_mode == m, m.label())
                    .clicked()
                {
                    p.varispeed_mode = m;
                    p.varispeed = true;
                    ui.close();
                }
            }
        });
    }

    // Display mode popover menu
    egui::Popup::menu(&tun_resp).show(|ui| {
        ui.set_min_width(220.0);
        for (m, lbl) in [
            (LcdMode::BeatsAndProject, "Beats & Project"),
            (LcdMode::BeatsAndTime, "Beats & Time"),
            (LcdMode::Beats, "Beats"),
            (LcdMode::Time, "Time"),
            (LcdMode::Custom, "Custom"),
        ] {
            let active = s.lcd_mode == m;
            let title = if active {
                format!("✓ {lbl}")
            } else {
                format!("   {lbl}")
            };
            if ui.selectable_label(active, title).clicked() {
                s.lcd_mode = m;
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Customize Control Bar and Display...").clicked() {
            s.show_lcd_customize = true;
            ui.close();
        }
    });
}

fn key_scale_popup_menu(ui: &Ui, resp: &egui::Response, p: &mut Project, s: &mut UiState) {
    egui::Popup::menu(resp).show(|ui| {
        ui.set_max_width(280.0);
        ui.set_min_width(240.0);
        let cur_bar = (p.beats_to_bar(p.position_beats).floor() as u32 + 1).max(1);
        let (active_root, active_scale) = p.key_at(p.position_beats);

        egui::ScrollArea::vertical()
            .max_height(240.0)
            .auto_shrink([true, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.spacing_mut().button_padding = egui::vec2(4.0, 1.5);

                ui.horizontal_top(|ui| {
                    // Left column: Root Note (compact ~88px)
                    ui.vertical(|ui| {
                        ui.set_max_width(88.0);
                        ui.set_min_width(80.0);
                        ui.label(egui::RichText::new("Root note").strong().small());
                        ui.separator();
                        for (i, name) in crate::note_edit::ROOTS.iter().enumerate() {
                            let is_active = active_root == i as u8;
                            let text = if is_active {
                                format!("✓ {name}")
                            } else {
                                format!("  {name}")
                            };
                            if ui
                                .selectable_label(is_active, egui::RichText::new(text).small())
                                .clicked()
                            {
                                p.set_key_at(cur_bar, i as u8, active_scale);
                                s.piano.scale_root = Some(i as u8);
                                s.piano.scale_mode = active_scale as usize;
                            }
                        }
                    });

                    ui.separator();

                    // Right column: Scale / Mode (compact ~165px)
                    ui.vertical(|ui| {
                        ui.set_max_width(165.0);
                        ui.set_min_width(135.0);
                        ui.label(egui::RichText::new("Scale / Mode").strong().small());
                        ui.separator();
                        for (i, (name, _)) in crate::note_edit::MODES.iter().enumerate() {
                            let is_active = active_scale == i as u8;
                            let text = if is_active {
                                format!("✓ {name}")
                            } else {
                                format!("  {name}")
                            };
                            if ui
                                .selectable_label(is_active, egui::RichText::new(text).small())
                                .clicked()
                            {
                                p.set_key_at(cur_bar, active_root, i as u8);
                                s.piano.scale_root = Some(active_root);
                                s.piano.scale_mode = i;
                            }
                        }
                    });
                });
            });

        ui.separator();
        let display = format!(
            "{} {}",
            crate::note_edit::ROOTS[active_root.min(11) as usize],
            crate::note_edit::MODES[active_scale.min(11) as usize].0
        );
        let bar_info = if cur_bar == 1 {
            "Project Key (Bar 1)".to_string()
        } else {
            format!("At Bar {cur_bar}")
        };
        ui.label(
            egui::RichText::new(format!("{bar_info}: {display}"))
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
}

/// Level (0..1) for a peak amplitude: -60 dB at the left end, +6 dB at the right end.
fn meter_frac(peak: f32) -> f32 {
    if peak <= 1e-6 {
        return 0.0;
    }
    ((20.0 * peak.log10() + 60.0) / 66.0).clamp(0.0, 1.0)
}

/// Master volume: compact 72px neutral charcoal pill track (#141416), subtle cool-grey fill (#6E7178),
/// small speaker icon, and a clean 11px matte silver-grey thumb (#C8CACF).
pub(crate) fn master_volume(
    ui: &Ui,
    g: &Painter,
    p: &mut Project,
    rect: Rect,
    _peaks: Option<(f32, f32)>,
    id: egui::Id,
    _quality: crate::liquid_glass::GlassQuality,
) {
    let cy = rect.center().y;
    // Small speaker icon on the left (14px, #9A9CA2)
    let spk_r = Rect::from_center_size(pos2(rect.left() + 7.0, cy), vec2(14.0, 14.0));
    crate::icons::paint_regular(
        g,
        spk_r,
        crate::icons::SPEAKER_HIGH,
        14.0,
        Color32::from_rgb(0x9a, 0x9c, 0xa2),
    );

    // 104px neutral charcoal pill track (#141416) with 8px gap after speaker icon
    let track_left = rect.left() + 14.0 + 8.0;
    let track = Rect::from_min_size(pos2(track_left, cy - 4.5), vec2(104.0, 9.0));
    let resp = ui.interact(track.expand2(vec2(6.0, 8.0)), id, Sense::click_and_drag());
    crate::quick_help::hover(ui.ctx(), &resp, "master_vol");
    let travel = (track.left() + 5.5, track.right() - 5.5);
    if let Some(pt) = resp.interact_pointer_pos() {
        p.master_vol = ((pt.x - travel.0) / (travel.1 - travel.0)).clamp(0.0, 1.0);
    }

    // Track background
    g.rect_filled(track, 4.5, Color32::from_rgb(0x14, 0x14, 0x16));
    g.rect_stroke(
        track,
        4.5,
        Stroke::new(1.0, Color32::from_rgb(0x26, 0x26, 0x2a)),
        StrokeKind::Inside,
    );

    // Subtle cool-grey fill (#6E7178)
    let thumb_x = travel.0 + (travel.1 - travel.0) * p.master_vol;
    if thumb_x > track.left() + 4.0 {
        let fill_rect = Rect::from_min_max(track.min, pos2(thumb_x, track.max.y));
        g.rect_filled(
            fill_rect,
            CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0,
            },
            Color32::from_rgb(0x6e, 0x71, 0x78),
        );
    }

    // Clean 11px matte silver-grey thumb (#C8CACF)
    let thumb_c = pos2(thumb_x, cy);
    g.circle_filled(
        thumb_c + vec2(0.0, 1.0),
        5.5,
        Color32::from_black_alpha(100),
    );
    g.circle_filled(thumb_c, 5.5, Color32::from_rgb(0xc8, 0xca, 0xcf));
    g.circle_stroke(
        thumb_c,
        5.5,
        Stroke::new(1.0, Color32::from_rgb(0xe0, 0xe2, 0xe6)),
    );
}

/// Where the Smart Controls / Mixer / Editors buttons start, from the bar's left edge, and how
/// wide they are together (three 30-point buttons).
const PANES_LEFT: f32 = 140.0;
const PANES_WIDTH: f32 = 90.0;
/// The LCD's width, and how far the transport buttons reach to its left (the Rewind button's
/// left edge is 213 points before the LCD).
const LCD_WIDTH: f32 = 555.0;
const TRANSPORT_REACH: f32 = 213.0;
/// The least space between the Editors button and the Rewind button.
const PANES_GAP: f32 = 16.0;

/// What the buttons right of the LCD need: the metronome's right edge is 156 points after it,
/// and the four editor icons start 121 points before the bar's right edge.
const RIGHT_REACH: f32 = 156.0;
const RIGHT_ICONS: f32 = 121.0;

/// The LCD's centre. Usually a little right of the bar's middle; moved right when the Rewind
/// button would crowd the Editors (scissors) button (issue #118), but never so far that the
/// metronome meets the icons on the right. When the bar is too narrow for both, the room that is
/// missing is shared between the two sides.
fn lcd_center_x(bar: Rect) -> f32 {
    let centred = bar.center().x + 20.0;
    let half = LCD_WIDTH / 2.0;
    let least = bar.left() + PANES_LEFT + PANES_WIDTH + PANES_GAP + TRANSPORT_REACH + half;
    let most = bar.right() - RIGHT_ICONS - PANES_GAP - RIGHT_REACH - half;
    if least <= most {
        centred.clamp(least, most)
    } else {
        (least + most) / 2.0
    }
}

pub fn control_bar(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    master_peaks: Option<(f32, f32)>,
) {
    let bar = ui.max_rect();
    let g = ui.painter_at(bar);
    g.rect_filled(bar, 0.0, BAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(0x11, 0x11, 0x13)),
    );
    let cy = bar.center().y;

    // Left: Library / Inspector / Quick Help / Toolbar (Pill 1)
    let left = [
        (icon_library as IconFn, s.show_library),
        (icon_inspector, s.show_inspector),
        (icon_help, s.show_quick_help),
        (icon_toolbar, s.show_toolbar),
    ];
    match segmented(ui, &g, bar.left() + 8.0, cy, "view", &left, s.glass_quality) {
        Some(0) => s.show_library = !s.show_library,
        Some(1) => s.show_inspector = !s.show_inspector,
        Some(2) => s.show_quick_help = !s.show_quick_help,
        Some(3) => s.show_toolbar = !s.show_toolbar,
        _ => {}
    }
    // Macro Controls / Mixer / Editors (Pill 2)
    let macro_on = s.bottom == Bottom::MacroControls;
    let mixer_on = s.show_mixer && s.bottom == Bottom::Mixer;
    let editors_on = matches!(
        s.bottom,
        Bottom::PianoRoll | Bottom::PitchEditor | Bottom::SessionPlayer
    );
    let mid = [
        (icon_smart_controls as IconFn, macro_on),
        (icon_mixer, mixer_on),
        (icon_editors, editors_on),
    ];
    match segmented(
        ui,
        &g,
        bar.left() + PANES_LEFT,
        cy,
        "panes",
        &mid,
        s.glass_quality,
    ) {
        Some(0) => {
            if macro_on {
                s.bottom = Bottom::Mixer;
                s.show_mixer = false;
            } else {
                s.bottom = Bottom::MacroControls;
            }
        }
        Some(1) => {
            if mixer_on {
                s.show_mixer = false;
            } else {
                s.bottom = Bottom::Mixer;
                s.show_mixer = true;
            }
        }
        Some(2) => crate::track_commands::toggle_editor(p, s),
        _ => {}
    }

    // Centre: LCD, transport to its left, master volume & editors to its right.
    let lcd_w = if p.varispeed || s.lcd_custom.show_varispeed {
        668.0
    } else {
        566.0
    };
    let lcd_rect = Rect::from_center_size(pos2(lcd_center_x(bar), cy), vec2(lcd_w, 42.0));
    lcd(ui, &g, p, s, lcd_rect, s.glass_quality);

    // Center Transport Pill capsule: |<, <<, >>, ⏹, ▶, ●, ↻
    let transport_w = 204.0;
    let transport_capsule = Rect::from_min_max(
        pos2(lcd_rect.left() - transport_w - 8.0, cy - 14.0),
        pos2(lcd_rect.left() - 8.0, cy + 14.0),
    );
    crate::liquid_glass::paint_glass_pill(
        ui,
        transport_capsule,
        14.0,
        s.glass_quality,
        false,
        None,
        None,
    );

    let btn_w = 28.0;
    let pad_x = 4.0;
    for i in 0..7 {
        let r = Rect::from_min_size(
            pos2(
                transport_capsule.left() + pad_x + i as f32 * btn_w,
                cy - 12.0,
            ),
            vec2(btn_w, 24.0),
        );
        let cr = CornerRadius {
            nw: if i == 0 { 10 } else { 3 },
            sw: if i == 0 { 10 } else { 3 },
            ne: if i == 6 { 10 } else { 3 },
            se: if i == 6 { 10 } else { 3 },
        };
        if i > 0 {
            g.line_segment(
                [
                    pos2(r.left(), r.top() + 4.0),
                    pos2(r.left(), r.bottom() - 4.0),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 15)),
            );
        }
        match i {
            // 0: |< (Go to Beginning)
            0 => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "start")), Sense::click());
                tip(&resp, "start");
                if resp.hovered() {
                    g.rect_filled(
                        r.shrink(1.0),
                        cr,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                    );
                }
                icon_to_start(&g, r, if resp.hovered() { Color32::WHITE } else { ICON });
                if resp.clicked() {
                    p.position_beats = 0.0;
                }
            }
            // 1: << (Rewind)
            1 => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "rew")), Sense::click());
                tip(&resp, "rew");
                if resp.hovered() {
                    g.rect_filled(
                        r.shrink(1.0),
                        cr,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                    );
                }
                icon_rewind(&g, r, if resp.hovered() { Color32::WHITE } else { ICON });
                if resp.clicked() {
                    let bar = p.beats_to_bar(p.position_beats);
                    let back = if bar.fract() < 1e-6 {
                        bar - 1.0
                    } else {
                        bar.floor()
                    };
                    p.position_beats = p.bar_to_beats(back.max(0.0));
                }
            }
            // 2: >> (Fast Forward)
            2 => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "ffw")), Sense::click());
                tip(&resp, "ffw");
                if resp.hovered() {
                    g.rect_filled(
                        r.shrink(1.0),
                        cr,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                    );
                }
                icon_forward(&g, r, if resp.hovered() { Color32::WHITE } else { ICON });
                if resp.clicked() {
                    p.position_beats = p.next_bar_start(p.position_beats);
                }
            }
            // 3: ⏹ (Stop)
            3 => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "stop")), Sense::click());
                tip(&resp, "stop");
                if resp.hovered() {
                    g.rect_filled(
                        r.shrink(1.0),
                        cr,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                    );
                }
                icon_stop(&g, r, if resp.hovered() { Color32::WHITE } else { ICON });
                if resp.clicked() {
                    if p.playing {
                        p.playing = false;
                    } else {
                        p.position_beats = 0.0;
                    }
                }
            }
            // 4: ▶ (Play, #30D158 when active)
            4 => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "play")), Sense::click());
                tip(&resp, "play");
                if p.playing {
                    g.rect_filled(r.shrink(1.0), cr, Color32::from_rgb(0x30, 0xd1, 0x58));
                    g.rect_stroke(
                        r.shrink(1.0),
                        cr,
                        Stroke::new(1.0, Color32::from_rgb(0x50, 0xe1, 0x78)),
                        StrokeKind::Inside,
                    );
                    icon_play(&g, r, Color32::WHITE);
                } else {
                    if resp.hovered() {
                        g.rect_filled(
                            r.shrink(1.0),
                            cr,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                        );
                    }
                    icon_play(&g, r, if resp.hovered() { Color32::WHITE } else { ICON });
                }
                if resp.clicked() {
                    crate::start_or_toggle_playback(p, s);
                }
                crate::widgets::context_menu(&resp, |ui| {
                    let mut changed = false;
                    if ui
                        .checkbox(
                            &mut s.app_settings.play_from_marquee,
                            "Play From Marquee Selection",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .checkbox(&mut s.app_settings.play_from_cycle, "Play From Cycle")
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut s.app_settings.play_from_selected_region,
                            "Play From Selected Region",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    if ui
                        .checkbox(
                            &mut s.app_settings.play_from_last_locate,
                            "Play From Last Locate Position",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.separator();
                    if ui.button("Apply Defaults").clicked() {
                        s.app_settings.play_from_marquee = true;
                        s.app_settings.play_from_cycle = true;
                        s.app_settings.play_from_selected_region = false;
                        s.app_settings.play_from_last_locate = true;
                        changed = true;
                        ui.close();
                    }
                    if ui.button("Save As Defaults").clicked() {
                        s.app_settings.save();
                        ui.close();
                    }
                    if changed {
                        s.app_settings.save();
                    }
                });
            }
            // 5: ● (Record, #FF453A when active)
            5 => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "rec")), Sense::click());
                tip(&resp, "rec");
                if p.recording {
                    g.rect_filled(r.shrink(1.0), cr, Color32::from_rgb(0xff, 0x45, 0x3a));
                    g.rect_stroke(
                        r.shrink(1.0),
                        cr,
                        Stroke::new(1.0, Color32::from_rgb(0xff, 0x65, 0x5a)),
                        StrokeKind::Inside,
                    );
                    icon_record(&g, r, Color32::WHITE);
                } else {
                    if resp.hovered() {
                        g.rect_filled(
                            r.shrink(1.0),
                            cr,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                        );
                    }
                    icon_record(&g, r, Color32::from_rgb(0xff, 0x45, 0x3a));
                }
                if resp.clicked() {
                    crate::menus::trigger_record(p, s);
                }
            }
            // 6: ↻ (Cycle, #FFD60A when active)
            _ => {
                let resp = ui.interact(r, egui::Id::unique(("transport", "cycle")), Sense::click());
                tip(&resp, "cycle");
                if p.cycle {
                    g.rect_filled(r.shrink(1.0), cr, Color32::from_rgb(0xff, 0xd6, 0x0a));
                    g.rect_stroke(
                        r.shrink(1.0),
                        cr,
                        Stroke::new(1.0, Color32::from_rgb(0xff, 0xe6, 0x3a)),
                        StrokeKind::Inside,
                    );
                    icon_cycle(&g, r, Color32::from_rgb(0x11, 0x11, 0x13), true);
                } else {
                    if resp.hovered() {
                        g.rect_filled(
                            r.shrink(1.0),
                            cr,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                        );
                    }
                    icon_cycle(
                        &g,
                        r,
                        if resp.hovered() { Color32::WHITE } else { ICON },
                        false,
                    );
                }
                if resp.clicked() {
                    p.cycle = !p.cycle;
                }
            }
        }
    }

    // Right of LCD: master volume, then List Editors / Note Pads / Loop Browser / Browsers.
    let rx = |dx: f32| pos2(lcd_rect.right() + dx, cy);
    let (vol_left, vol_w) = volume_span(rx(10.0).x, bar.right() - 136.0);
    if vol_w > 0.0 {
        let vol = Rect::from_min_size(pos2(vol_left, cy - 11.0), vec2(vol_w, 22.0));
        master_volume(
            ui,
            &g,
            p,
            vol,
            master_peaks,
            egui::Id::unique(("control_bar", "master")),
            s.glass_quality,
        );
    }

    // Right panel toggles floating glass pill (zero MCP badges)
    let right_items = [
        (icon_list_editors as IconFn, s.show_lists),
        (icon_note_pad, s.show_notes),
        (icon_loops, s.browser.open == Some(Tab::Loops)),
        (icon_browsers, s.browser.open == Some(Tab::Files)),
    ];
    let pad_x = 4.0;
    let seg_w = 30.0;
    let right_w = pad_x * 2.0 + seg_w * right_items.len() as f32;
    let right_left = bar.right() - right_w - 12.0;
    match segmented(
        ui,
        &g,
        right_left,
        cy,
        "right_panels",
        &right_items,
        s.glass_quality,
    ) {
        Some(0) => s.show_lists = !s.show_lists,
        Some(1) => s.show_notes = !s.show_notes,
        Some(2) => s.browser.toggle(Tab::Loops),
        Some(3) => s.browser.toggle(Tab::Files),
        _ => {}
    }
}

/// Master volume slider: its usual place and width (126 points, right of LCD and left of right panel toggles),
/// moved right of the metronome (which ends at `after`) and narrowed so it never reaches the
/// icons at the right edge. Width 0 = no room at all, leave it out.
fn volume_span(after: f32, right: f32) -> (f32, f32) {
    const WIDTH: f32 = 126.0;
    const MIN_WIDTH: f32 = 80.0;
    let icons_left = right - 136.0;
    let left = (right - 148.0 - WIDTH).max(after + 14.0);
    let width = WIDTH.min(icons_left - 8.0 - left);
    if width < MIN_WIDTH {
        (left, 0.0)
    } else {
        (left, width)
    }
}

/// The largest font, up to `size`, that fits `text` into `max_w` points: an LCD column has a
/// fixed width, and some languages need more room (German, Finnish).
fn fitting_font(g: &Painter, text: &str, size: f32, max_w: f32) -> FontId {
    let w = g
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), LCD_TEXT)
        .size()
        .x;
    FontId::proportional(if w > max_w {
        (size * max_w / w).max(8.0)
    } else {
        size
    })
}

#[allow(dead_code)]
fn _unused_system_status_strings() {
    let _ = tr("No In");
    let _ = tr("No Out");
    let _ = trf("Audio DSP load: {}%", &[&0]);
    let _ = trf("Disk streaming load: {}%", &[&0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_never_covers_the_metronome_or_the_icons() {
        // Wide bar: the usual place and width.
        assert_eq!(volume_span(1200.0, 2000.0), (2000.0 - 148.0 - 126.0, 126.0));
        // Narrow bar: after the metronome, narrowed before the icons.
        let (left, width) = volume_span(1300.0, 1500.0);
        assert!(left >= 1300.0 + 14.0);
        assert!(left + width <= 1500.0 - 136.0 - 8.0 + 1e-3);
        assert!(width >= 80.0 || width == 0.0);
        // No room: left out.
        assert_eq!(volume_span(1400.0, 1470.0).1, 0.0);
    }

    #[test]
    fn slow_drags_add_up_to_whole_steps() {
        let mut left = 0.0;
        let total: i32 = (0..25).map(|_| take_steps(&mut left, 1.0, 10.0)).sum();
        assert_eq!(total, 2);
        assert!((left - 5.0).abs() < 1e-4);

        // Dragging down gives negative steps, and a reversal cancels what was built up.
        let mut left = 0.0;
        assert_eq!(take_steps(&mut left, -25.0, 10.0), -2);
        assert_eq!(take_steps(&mut left, 25.0, 10.0), 2);
        assert!(left.abs() < 1e-4);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn bar(w: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(w, 44.0))
    }

    /// The space between the Editors button and Rewind, and between the metronome and the
    /// right-hand icons, with the LCD placed by [`lcd_center_x`].
    fn gaps(w: f32) -> (f32, f32) {
        let c = lcd_center_x(bar(w));
        let rewind_left = c - LCD_WIDTH / 2.0 - TRANSPORT_REACH;
        let metronome_right = c + LCD_WIDTH / 2.0 + RIGHT_REACH;
        (
            rewind_left - (PANES_LEFT + PANES_WIDTH),
            (w - RIGHT_ICONS) - metronome_right,
        )
    }

    #[test]
    fn a_wide_bar_keeps_the_lcd_centred_and_both_gaps_open() {
        for w in [1700.0, 1920.0, 2560.0] {
            let (left, right) = gaps(w);
            assert!(
                left >= PANES_GAP && right >= PANES_GAP,
                "{w}: {left} {right}"
            );
            assert_eq!(lcd_center_x(bar(w)), w / 2.0 + 20.0);
        }
    }

    #[test]
    fn a_bar_that_fits_both_sides_keeps_the_editors_button_clear_of_rewind() {
        for w in (1320..1700).step_by(20) {
            let (left, right) = gaps(w as f32);
            assert!(
                left >= PANES_GAP - 0.01,
                "{w}: Rewind is {left} from the Editors button"
            );
            assert!(
                right >= PANES_GAP - 0.01,
                "{w}: the metronome is {right} from the icons"
            );
        }
    }

    #[test]
    fn a_narrow_bar_is_better_than_before_and_shares_the_squeeze() {
        for w in (1000..1320).step_by(40) {
            let w = w as f32;
            let before =
                (w / 2.0 + 20.0) - LCD_WIDTH / 2.0 - TRANSPORT_REACH - (PANES_LEFT + PANES_WIDTH);
            let (left, right) = gaps(w);
            assert!(left > before, "{w}: {left} is not better than {before}");
            assert!((left - right).abs() < 0.01, "{w}: {left} vs {right}");
        }
    }
}
