//! Bottom Session Player Editor (`session1.png` & `session2.png` layout).
//!
//! Replicates the Logic Pro Session Player Editor: top preview header (`Chords`, `Preset`,
//! Pointer/Pencil tools, `Snap`, zoom), bar ruler + zoomed Region Strip with note stems &
//! pitch-bend glide arrows, left column (detailed circular instrument graphic, `Regenerate`,
//! vertical yellow `Complexity` & `Intensity` sliders), 5-tab central card (`Main`, `Details`,
//! `Slides`/`Envelope`, `LFO`, `Manual`), and right-hand `daw_knob` timing/fills/feel controls.

use crate::daw_knob::{self, DawKnobConfig, DawKnobStyle};
use crate::model::{Generator, Project, PITCH_BEND};
use crate::pattern::{DrumStyle, DrummerParams};
use crate::session_player::{
    default_chords, detect_chords_from_notes, eval_lfo, AddNotesMode, BassStyle, FollowRhythmMode,
    HandMovement, HandStyle, KeyboardStyle, LfoWave, PhraseLength, SessionPlayerKind,
    SessionPlayerParams, SlideStart, SwingSubdiv, VoicingMode, SESSION_PRESETS,
};
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui,
};

const BG_DARK: Color32 = Color32::from_rgb(0x1b, 0x1c, 0x20);
const CARD_BG: Color32 = Color32::from_rgb(0x21, 0x22, 0x27);
const WELL_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
const BORDER: Color32 = Color32::from_rgb(0x34, 0x36, 0x3e);
const ACCENT_YELLOW: Color32 = Color32::from_rgb(0xea, 0xc3, 0x2a);
const BLUE_ACTIVE: Color32 = Color32::from_rgb(0x12, 0x69, 0xcc);
const TEXT_MAIN: Color32 = Color32::from_rgb(0xea, 0xec, 0xf0);
const TEXT_DIM: Color32 = Color32::from_rgb(0x8e, 0x93, 0x9e);

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorTab {
    Main,
    Details,
    SlidesOrEnv,
    Lfo,
    Manual,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewTool {
    Pointer,
    Pencil,
}

fn midi_note_name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let oct = i32::from(midi / 12) - 1;
    format!("{}{}", NAMES[usize::from(midi % 12)], oct)
}

fn from_drummer_params(d: &DrummerParams) -> SessionPlayerParams {
    SessionPlayerParams {
        kind: SessionPlayerKind::Drummer,
        preset_name: "Neon Pulse".to_string(),
        drum_style: d.style,
        complexity: f32::from(d.complexity) / 100.0,
        intensity: f32::from(d.loudness) / 100.0,
        swing: f32::from(d.swing) / 100.0,
        fill_amount: f32::from(d.fills) / 100.0,
        seed: d.seed,
        ..Default::default()
    }
}

/// Renders the bottom Session Player Editor for the active Session Player or Drummer region.
pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let target = find_session_or_drummer_region(p, s);
    let Some((ti, ri, was_legacy_drummer)) = target else {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("No Session Player region on the selected track.")
                        .size(13.0)
                        .color(TEXT_DIM),
                );
                ui.add_space(8.0);
                if ui.button("Create Keyboard Session Region").clicked() {
                    let ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
                    if p.chords.is_empty() {
                        p.chords = default_chords();
                    }
                    p.add_session_region(ti, SessionPlayerParams::default(), 1.0);
                }
            });
        });
        return;
    };

    let tab_id = egui::Id::unique("session_player_tab");
    let tool_id = egui::Id::unique("session_player_tool");
    let snap_id = egui::Id::unique("session_player_snap_on");
    let zoom_id = egui::Id::unique("session_player_zoom");
    let mut tab: EditorTab = ui
        .ctx()
        .data(|d| d.get_temp(tab_id).unwrap_or(EditorTab::Main));
    let mut tool: PreviewTool = ui
        .ctx()
        .data(|d| d.get_temp(tool_id).unwrap_or(PreviewTool::Pointer));
    let mut snap_on: bool = ui.ctx().data(|d| d.get_temp(snap_id).unwrap_or(true));
    let mut zoom: f32 = ui.ctx().data(|d| d.get_temp(zoom_id).unwrap_or(1.0));

    let orig = match p.tracks[ti].regions[ri]
        .generated
        .as_ref()
        .map(|g| &g.source)
    {
        Some(Generator::Session(sp)) => sp.clone(),
        Some(Generator::Drummer(d)) => from_drummer_params(d),
        _ => return,
    };
    let mut sp = orig.clone();

    // 1. Top Preview Header Bar (matching session1.png & session2.png)
    egui::Frame::new()
        .fill(BG_DARK)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("Chords", |ui| {
                    if ui
                        .button("Use Default Progression (Cmaj7 - Am7 - Fmaj7 - G7)")
                        .clicked()
                    {
                        p.chords = default_chords();
                        p.regenerate(ti, ri);
                        ui.close();
                    }
                    if ui.button("Chord ID from Selected Track").clicked() {
                        let notes = p.tracks[ti].regions[ri].notes.clone();
                        let bars = p.tracks[ti].regions[ri].len_bars.ceil() as usize;
                        p.chords = detect_chords_from_notes(&notes, bars);
                        p.regenerate(ti, ri);
                        ui.close();
                    }
                    if ui.button("Clear Chords").clicked() {
                        p.chords.clear();
                        p.regenerate(ti, ri);
                        ui.close();
                    }
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Preset:").size(11.0).color(TEXT_DIM));
                egui::ComboBox::from_id_salt("sp_preset_combo")
                    .width(165.0)
                    .selected_text(&sp.preset_name)
                    .show_ui(ui, |ui| {
                        for name in SESSION_PRESETS {
                            if ui.selectable_label(sp.preset_name == name, name).clicked() {
                                sp.apply_preset(name);
                            }
                        }
                    });

                ui.add_space(ui.available_width() * 0.14);
                let ptr_lbl = format!("{} ", phosphor_egui::regular::CURSOR);
                let pen_lbl = format!("{} ", phosphor_egui::regular::PENCIL_SIMPLE);
                if ui
                    .selectable_label(tool == PreviewTool::Pointer, ptr_lbl)
                    .on_hover_text("Pointer Tool")
                    .clicked()
                {
                    tool = PreviewTool::Pointer;
                }
                if ui
                    .selectable_label(tool == PreviewTool::Pencil, pen_lbl)
                    .on_hover_text("Pencil Tool")
                    .clicked()
                {
                    tool = PreviewTool::Pencil;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.scope(|ui| {
                        ui.spacing_mut().slider_width = 48.0;
                        ui.add(egui::Slider::new(&mut zoom, 0.5..=2.0).show_value(false));
                    });
                    ui.label(
                        RichText::new(phosphor_egui::regular::ARROWS_LEFT_RIGHT)
                            .size(11.0)
                            .color(TEXT_DIM),
                    );
                    let snap_btn = egui::Button::new(
                        RichText::new(phosphor_egui::regular::POWER)
                            .size(11.0)
                            .color(Color32::WHITE),
                    )
                    .fill(if snap_on { BLUE_ACTIVE } else { CARD_BG })
                    .corner_radius(CornerRadius::same(4));
                    if ui.add(snap_btn).on_hover_text("Toggle Snap").clicked() {
                        snap_on = !snap_on;
                    }
                    ui.label(RichText::new("Snap: Beat").size(11.0).color(TEXT_DIM));
                    ui.add_space(6.0);
                    egui::ComboBox::from_id_salt("sp_kind_combo")
                        .width(120.0)
                        .selected_text(sp.kind.label())
                        .show_ui(ui, |ui| {
                            for k in SessionPlayerKind::ALL {
                                ui.selectable_value(&mut sp.kind, k, k.label());
                            }
                        });
                });
            });
        });

    // 2. Bar Ruler + Zoomed Region Strip
    let (strip_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 68.0), Sense::hover());
    draw_bar_ruler_and_strip(ui, strip_rect, &p.tracks[ti].regions[ri], &sp, zoom);

    ui.add_space(8.0);

    // 3. Main Body: Left Column + Central 5-Tab Card (with aligned Tab Bar above it)
    let left_col_w = 222.0;
    let third_label = if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
        "Envelope"
    } else {
        "Slides"
    };

    ui.horizontal_top(|ui| {
        // Left Column: Player Graphic + Regenerate + Vertical Complexity/Intensity Sliders
        ui.vertical(|ui| {
            ui.set_width(left_col_w);
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(122.0);
                    let style_name = match sp.kind {
                        SessionPlayerKind::Keyboard => sp.keyboard_style.label(),
                        SessionPlayerKind::Bass => sp.bass_style.label(),
                        SessionPlayerKind::Drummer => sp.drum_style.name(),
                    };
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(sp.kind.label()).size(10.5).color(TEXT_DIM));
                        ui.label(
                            RichText::new(style_name)
                                .size(11.5)
                                .strong()
                                .color(ACCENT_YELLOW),
                        );
                        ui.add_space(4.0);
                        let (disc_rect, _) =
                            ui.allocate_exact_size(vec2(94.0, 94.0), Sense::hover());
                        draw_player_disc(ui, disc_rect, sp.kind);
                        ui.add_space(6.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Regenerate").size(11.0).color(TEXT_MAIN),
                                )
                                .fill(Color32::from_rgb(0x3a, 0x3c, 0x42))
                                .corner_radius(CornerRadius::same(4))
                                .min_size(vec2(84.0, 22.0)),
                            )
                            .clicked()
                        {
                            sp.seed = sp.seed.wrapping_add(1);
                        }
                    });
                });

                vertical_yellow_slider(ui, "Complexity", &mut sp.complexity);
                ui.add_space(6.0);
                vertical_yellow_slider(ui, "Intensity", &mut sp.intensity);
            });
        });

        ui.add_space(8.0);

        // Right Area: Centered Tab Strip + Dark Rounded Card (Central Panel + Right Timing Column)
        ui.vertical(|ui| {
            // Tab Bar centered above the central card
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() * 0.22).max(20.0));
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x16, 0x17, 0x1b))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(3, 2))
                    .show(ui, |ui| {
                        for (t, lbl) in [
                            (EditorTab::Main, "Main"),
                            (EditorTab::Details, "Details"),
                            (EditorTab::SlidesOrEnv, third_label),
                            (EditorTab::Lfo, "LFO"),
                            (EditorTab::Manual, "Manual"),
                        ] {
                            let active = tab == t;
                            let btn = egui::Button::new(
                                RichText::new(lbl).size(11.5).strong().color(if active {
                                    Color32::BLACK
                                } else {
                                    TEXT_DIM
                                }),
                            )
                            .fill(if active {
                                ACCENT_YELLOW
                            } else {
                                Color32::TRANSPARENT
                            })
                            .corner_radius(CornerRadius::same(4));
                            if ui.add(btn).clicked() {
                                tab = t;
                            }
                        }
                    });
            });

            ui.add_space(6.0);

            egui::Frame::new()
                .fill(CARD_BG)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        // Central Tab Area
                        ui.vertical(|ui| {
                            ui.set_width((ui.available_width() - 188.0).max(280.0));
                            match tab {
                                EditorTab::Main => draw_main_tab(ui, &mut sp),
                                EditorTab::Details => draw_details_tab(ui, &mut sp),
                                EditorTab::SlidesOrEnv => draw_slides_or_env_tab(ui, &mut sp),
                                EditorTab::Lfo => draw_lfo_tab(ui, &mut sp),
                                EditorTab::Manual => draw_manual_tab(ui, &mut sp),
                            }
                        });

                        ui.separator();

                        // Right Column: daw_knob Timing, Fills & Feel
                        ui.vertical(|ui| {
                            ui.set_width(172.0);
                            draw_right_column(ui, &mut sp, tab);
                        });
                    });
                });
        });
    });

    ui.ctx().data_mut(|d| {
        d.insert_temp(tab_id, tab);
        d.insert_temp(tool_id, tool);
        d.insert_temp(snap_id, snap_on);
        d.insert_temp(zoom_id, zoom);
    });

    if sp != orig || was_legacy_drummer {
        if let Some(g) = p.tracks[ti].regions[ri].generated.as_mut() {
            g.source = Generator::Session(sp);
        }
        p.regenerate(ti, ri);
    }
}

fn find_session_or_drummer_region(p: &Project, s: &UiState) -> Option<(usize, usize, bool)> {
    for (ti, ri) in s.tracks.selection() {
        if let Some(r) = p.tracks.get(ti).and_then(|t| t.regions.get(ri)) {
            match r.generated.as_ref().map(|g| &g.source) {
                Some(Generator::Session(_)) => return Some((ti, ri, false)),
                Some(Generator::Drummer(_)) => return Some((ti, ri, true)),
                _ => {}
            }
        }
    }
    let ti = p.selected_track;
    if let Some(t) = p.tracks.get(ti) {
        for (ri, r) in t.regions.iter().enumerate() {
            match r.generated.as_ref().map(|g| &g.source) {
                Some(Generator::Session(_)) => return Some((ti, ri, false)),
                Some(Generator::Drummer(_)) => return Some((ti, ri, true)),
                _ => {}
            }
        }
    }
    None
}

fn draw_bar_ruler_and_strip(
    ui: &Ui,
    rect: Rect,
    region: &crate::model::Region,
    sp: &SessionPlayerParams,
    zoom: f32,
) {
    let g = ui.painter_at(rect);
    let ruler_h = 15.0;
    let header_h = 15.0;
    let ruler = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + ruler_h));
    let header = Rect::from_min_max(
        pos2(rect.left(), ruler.bottom()),
        pos2(rect.right(), ruler.bottom() + header_h),
    );
    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);

    g.rect_filled(ruler, 0.0, Color32::from_rgb(0x16, 0x17, 0x1b));
    g.rect_filled(header, 0.0, Color32::from_rgb(0xe6, 0xdf, 0xbe));
    let body_bg = if sp.kind == SessionPlayerKind::Bass && sp.bass_style == BassStyle::Bass808 {
        Color32::from_rgb(0x3c, 0x3d, 0x41)
    } else {
        Color32::from_rgb(0xa8, 0x8f, 0x2d)
    };
    g.rect_filled(body, 0.0, body_bg);

    let total_beats = ((region.len_bars * 4.0) / zoom.max(0.5)).max(4.0);
    let num_bars = (total_beats / 4.0).ceil() as usize;
    for bar in 1..=num_bars {
        let x = rect.left() + ((bar - 1) as f32 * 4.0 / total_beats) * rect.width();
        g.line_segment(
            [pos2(x, ruler.top()), pos2(x, body.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 45)),
        );
        g.text(
            pos2(x + 4.0, ruler.center().y),
            Align2::LEFT_CENTER,
            format!("{bar}"),
            FontId::proportional(9.5),
            TEXT_DIM,
        );
    }

    let style_lbl = match sp.kind {
        SessionPlayerKind::Keyboard => sp.keyboard_style.label(),
        SessionPlayerKind::Bass => sp.bass_style.label(),
        SessionPlayerKind::Drummer => sp.drum_style.name(),
    };
    g.text(
        pos2(header.left() + 8.0, header.center().y),
        Align2::LEFT_CENTER,
        format!("▸ {} - {style_lbl}", sp.kind.label()),
        FontId::proportional(10.0),
        Color32::from_rgb(0x28, 0x25, 0x1a),
    );

    // Note stems with notehead + tail
    for n in &region.notes {
        let x0 = body.left() + (n.start / total_beats) * body.width();
        let x1 = (x0 + (n.len / total_beats) * body.width()).max(x0 + 4.0);
        if x0 > body.right() {
            continue;
        }
        let pitch_norm = ((f32::from(n.pitch) - 28.0) / 60.0).clamp(0.08, 0.92);
        let y = body.bottom() - pitch_norm * body.height();
        let col = Color32::from_rgba_unmultiplied(255, 252, 232, (150 + n.vel / 2).min(255));
        g.circle_filled(pos2(x0, y), 2.2, col);
        g.line_segment(
            [pos2(x0, y), pos2(x1.min(body.right()), y)],
            Stroke::new(1.6, col),
        );
    }

    // Smooth white pitch-bend glide curve overlay
    let mut bend_pts: Vec<Pos2> = region
        .controls
        .iter()
        .filter(|c| c.num == PITCH_BEND)
        .map(|c| {
            let x = body.left() + (c.start / total_beats) * body.width();
            let y = body.bottom() - (f32::from(c.val) / 16383.0).clamp(0.05, 0.95) * body.height();
            pos2(x, y)
        })
        .collect();
    if bend_pts.len() >= 2 {
        bend_pts.sort_by(|a, b| a.x.total_cmp(&b.x));
        g.add(Shape::line(bend_pts, Stroke::new(1.3, Color32::WHITE)));
    }
}

fn draw_player_disc(ui: &Ui, rect: Rect, kind: SessionPlayerKind) {
    let g = ui.painter_at(rect);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.48;
    g.circle_filled(c, r, Color32::from_rgb(0x14, 0x15, 0x18));
    g.circle_stroke(c, r, Stroke::new(1.0, BORDER));
    let st = Stroke::new(1.3, ACCENT_YELLOW);
    let dim_st = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 140));
    match kind {
        SessionPlayerKind::Keyboard => {
            // Grand piano silhouette (matching session1.png)
            let body = Rect::from_center_size(c + vec2(0.0, -3.0), vec2(52.0, 20.0));
            g.rect_stroke(body, 3.0, st, StrokeKind::Inside);
            g.line_segment(
                [
                    pos2(body.left() + 4.0, body.bottom()),
                    pos2(body.left() + 4.0, c.y + 24.0),
                ],
                st,
            );
            g.line_segment(
                [
                    pos2(body.right() - 4.0, body.bottom()),
                    pos2(body.right() - 4.0, c.y + 24.0),
                ],
                st,
            );
            g.line_segment(
                [
                    pos2(body.left() + 6.0, body.top() - 8.0),
                    pos2(body.right() - 6.0, body.top() - 2.0),
                ],
                dim_st,
            );
            for i in 0..8 {
                let x = body.left() + 6.0 + i as f32 * 5.7;
                g.line_segment(
                    [pos2(x, body.top() + 8.0), pos2(x, body.bottom() - 2.0)],
                    dim_st,
                );
            }
        }
        SessionPlayerKind::Bass => {
            // Hardware synth panel + 1-octave keyboard (matching session2.png)
            let chassis = Rect::from_center_size(c, vec2(52.0, 38.0));
            g.rect_stroke(chassis, 3.0, st, StrokeKind::Inside);
            for row in 0..2 {
                for col in 0..4 {
                    let kc = pos2(
                        chassis.left() + 10.0 + col as f32 * 10.5,
                        chassis.top() + 9.0 + row as f32 * 9.0,
                    );
                    g.circle_stroke(kc, 2.5, dim_st);
                }
            }
            let keys_y = chassis.bottom() - 12.0;
            g.line_segment(
                [
                    pos2(chassis.left() + 3.0, keys_y),
                    pos2(chassis.right() - 3.0, keys_y),
                ],
                st,
            );
            for i in 1..8 {
                let x = chassis.left() + 4.0 + i as f32 * 5.8;
                g.line_segment([pos2(x, keys_y), pos2(x, chassis.bottom() - 2.0)], dim_st);
            }
        }
        SessionPlayerKind::Drummer => {
            g.circle_stroke(c + vec2(0.0, 6.0), 16.0, st);
            g.circle_stroke(c + vec2(-18.0, -10.0), 9.0, st);
            g.circle_stroke(c + vec2(18.0, -10.0), 9.0, st);
            g.circle_stroke(c + vec2(-12.0, 14.0), 7.0, dim_st);
        }
    }
}

fn vertical_yellow_slider(ui: &mut Ui, label: &str, val: &mut f32) {
    ui.vertical(|ui| {
        ui.set_width(42.0);
        ui.label(RichText::new(label).size(9.5).color(TEXT_DIM));
        let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 118.0), Sense::click_and_drag());
        if (resp.dragged() || resp.clicked()) && resp.interact_pointer_pos().is_some() {
            if let Some(pos) = resp.interact_pointer_pos() {
                *val = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            }
        }
        let g = ui.painter_at(rect);
        let x = rect.center().x;
        g.line_segment(
            [pos2(x, rect.top() + 5.0), pos2(x, rect.bottom() - 5.0)],
            Stroke::new(3.0, WELL_BG),
        );
        let thumb_y = rect.bottom() - 5.0 - *val * (rect.height() - 10.0);
        g.line_segment(
            [pos2(x, rect.bottom() - 5.0), pos2(x, thumb_y)],
            Stroke::new(3.0, ACCENT_YELLOW),
        );
        let thumb = Rect::from_center_size(pos2(x, thumb_y), vec2(18.0, 8.0));
        g.rect_filled(thumb, 4.0, Color32::from_rgb(0x6e, 0x72, 0x7c));
        g.line_segment(
            [
                pos2(thumb.left() + 4.0, thumb.center().y),
                pos2(thumb.right() - 4.0, thumb.center().y),
            ],
            Stroke::new(1.5, ACCENT_YELLOW),
        );
    });
}

fn draw_right_column(ui: &mut Ui, sp: &mut SessionPlayerParams, tab: EditorTab) {
    if tab == EditorTab::Details {
        ui.label(RichText::new("Phrase Length").size(10.5).color(TEXT_DIM));
        egui::ComboBox::from_id_salt("sp_phrase_len")
            .selected_text(sp.phrase_length.label())
            .show_ui(ui, |ui| {
                for pl in PhraseLength::ALL {
                    ui.selectable_value(&mut sp.phrase_length, pl, pl.label());
                }
            });
        ui.add(egui::Slider::new(&mut sp.chord_response, 0.0..=1.0).text("Chord Resp."));
        ui.label(RichText::new("Tempo: Automatic").size(10.5).color(TEXT_DIM));
        let feel_res = draw_session_knob(
            ui,
            DawKnobConfig {
                id: egui::Id::unique("sp_feel_knob"),
                label: "Feel",
                help_id: "session_feel",
                style: DawKnobStyle::Continuous,
                min: -1.0,
                max: 1.0,
                default: 0.0,
                log: false,
                bipolar: true,
                accent: true,
                unit: "",
                diameter: 34.0,
            },
            sp.feel,
        );
        if feel_res.changed {
            sp.feel = feel_res.value;
        }
        ui.add(egui::Slider::new(&mut sp.humanize, 0.0..=1.0).text("Humanize"));
    } else {
        ui.horizontal(|ui| {
            let fill_res = draw_session_knob(
                ui,
                DawKnobConfig {
                    id: egui::Id::unique("sp_fill_amount"),
                    label: "Fill Amount",
                    help_id: "session_fill_amount",
                    style: DawKnobStyle::Continuous,
                    min: 0.0,
                    max: 1.0,
                    default: 0.35,
                    log: false,
                    bipolar: false,
                    accent: true,
                    unit: "",
                    diameter: 38.0,
                },
                sp.fill_amount,
            );
            if fill_res.changed {
                sp.fill_amount = fill_res.value;
            }
            ui.add_space(6.0);
            let swing_res = draw_session_knob(
                ui,
                DawKnobConfig {
                    id: egui::Id::unique("sp_swing_knob"),
                    label: "Swing",
                    help_id: "session_swing",
                    style: DawKnobStyle::Continuous,
                    min: 0.0,
                    max: 1.0,
                    default: 0.08,
                    log: false,
                    bipolar: false,
                    accent: true,
                    unit: "",
                    diameter: 38.0,
                },
                sp.swing,
            );
            if swing_res.changed {
                sp.swing = swing_res.value;
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let fc_res = draw_session_knob(
                ui,
                DawKnobConfig {
                    id: egui::Id::unique("sp_fill_complexity"),
                    label: "Fill Comp.",
                    help_id: "session_fill_complexity",
                    style: DawKnobStyle::Continuous,
                    min: 0.0,
                    max: 1.0,
                    default: 0.30,
                    log: false,
                    bipolar: false,
                    accent: true,
                    unit: "",
                    diameter: 38.0,
                },
                sp.fill_complexity,
            );
            if fc_res.changed {
                sp.fill_complexity = fc_res.value;
            }
            ui.add_space(12.0);
            let sub_lbl = match sp.swing_subdiv {
                SwingSubdiv::Eighth => "8th",
                SwingSubdiv::Sixteenth => "16th",
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(sub_lbl).size(10.5).color(TEXT_MAIN))
                        .fill(Color32::from_rgb(0x30, 0x32, 0x3a))
                        .corner_radius(CornerRadius::same(4)),
                )
                .clicked()
            {
                sp.swing_subdiv = match sp.swing_subdiv {
                    SwingSubdiv::Eighth => SwingSubdiv::Sixteenth,
                    SwingSubdiv::Sixteenth => SwingSubdiv::Eighth,
                };
            }
        });
    }
}

fn draw_main_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    // Pattern dot matrix strip (1..8) + Follow Rhythm (matching session1.png & session2.png)
    ui.horizontal(|ui| {
        let badge = egui::Button::new(
            RichText::new(format!("{}", sp.pattern_idx + 1))
                .size(10.0)
                .strong()
                .color(Color32::BLACK),
        )
        .fill(ACCENT_YELLOW)
        .corner_radius(CornerRadius::same(9))
        .min_size(vec2(18.0, 18.0));
        if ui.add(badge).clicked() {
            sp.pattern_idx = (sp.pattern_idx + 1) % 8;
        }
        ui.add_space(6.0);
        let (dot_rect, dot_resp) = ui.allocate_exact_size(vec2(180.0, 18.0), Sense::click());
        if dot_resp.clicked() {
            if let Some(pos) = dot_resp.interact_pointer_pos() {
                let idx = (((pos.x - dot_rect.left()) / dot_rect.width()) * 8.0).floor() as usize;
                sp.pattern_idx = idx.min(7);
            }
        }
        let g = ui.painter_at(dot_rect);
        for i in 0..8 {
            let cx = dot_rect.left() + (i as f32 + 0.5) * (dot_rect.width() / 8.0);
            let active = i == sp.pattern_idx || (i % 2 == 0 && sp.complexity > 0.35);
            g.circle_filled(
                pos2(cx, dot_rect.center().y),
                if i == sp.pattern_idx { 3.8 } else { 2.4 },
                if active {
                    ACCENT_YELLOW
                } else {
                    Color32::from_rgb(0x4a, 0x4d, 0x56)
                },
            );
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Follow:").size(10.0).color(TEXT_DIM));
        let f_lbl = match sp.follow_rhythm {
            FollowRhythmMode::ChordRhythm => "Chord Rhythm",
            FollowRhythmMode::Track => "Track",
        };
        if ui
            .button(RichText::new(f_lbl).size(10.0).color(ACCENT_YELLOW))
            .clicked()
        {
            sp.follow_rhythm = match sp.follow_rhythm {
                FollowRhythmMode::ChordRhythm => FollowRhythmMode::Track,
                FollowRhythmMode::Track => FollowRhythmMode::ChordRhythm,
            };
        }
        ui.menu_button("Add Notes", |ui| {
            ui.selectable_value(&mut sp.add_notes, AddNotesMode::None, "None");
            ui.selectable_value(&mut sp.add_notes, AddNotesMode::Eighths, "8ths");
            ui.selectable_value(&mut sp.add_notes, AddNotesMode::Sixteenths, "16ths");
        });
    });

    ui.add_space(4.0);

    match sp.kind {
        SessionPlayerKind::Bass => {
            ui.horizontal_top(|ui| {
                // Square XY Pad (Y: Octave, X: Melody) matching session2.png
                let (xy_rect, xy_resp) =
                    ui.allocate_exact_size(vec2(138.0, 118.0), Sense::click_and_drag());
                if (xy_resp.dragged() || xy_resp.clicked())
                    && xy_resp.interact_pointer_pos().is_some()
                {
                    if let Some(pos) = xy_resp.interact_pointer_pos() {
                        sp.xy_melody = ((pos.x - xy_rect.left()) / xy_rect.width()).clamp(0.0, 1.0);
                        sp.xy_octave =
                            (1.0 - (pos.y - xy_rect.top()) / xy_rect.height()).clamp(0.0, 1.0);
                    }
                }
                let g = ui.painter_at(xy_rect);
                g.rect_filled(xy_rect, 6.0, WELL_BG);
                g.text(
                    pos2(xy_rect.center().x, xy_rect.bottom() - 8.0),
                    Align2::CENTER_CENTER,
                    "Melody",
                    FontId::proportional(9.5),
                    TEXT_DIM,
                );
                g.text(
                    pos2(xy_rect.left() + 18.0, xy_rect.center().y),
                    Align2::CENTER_CENTER,
                    "Octave",
                    FontId::proportional(9.5),
                    TEXT_DIM,
                );
                let px = xy_rect.left() + 14.0 + sp.xy_melody * (xy_rect.width() - 28.0);
                let py = xy_rect.bottom() - 14.0 - sp.xy_octave * (xy_rect.height() - 28.0);
                g.circle_filled(pos2(px, py), 11.0, ACCENT_YELLOW);

                ui.add_space(16.0);
                ui.vertical(|ui| {
                    // Dual-handle Note Range bar matching session2.png
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Note Range").size(10.5).color(TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{} - {}",
                                    midi_note_name(sp.note_range_lo),
                                    midi_note_name(sp.note_range_hi)
                                ))
                                .size(10.5)
                                .strong()
                                .color(ACCENT_YELLOW),
                            );
                        });
                    });
                    let (rng_rect, rng_resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 22.0),
                        Sense::click_and_drag(),
                    );
                    if rng_resp.dragged() || rng_resp.clicked() {
                        if let Some(pos) = rng_resp.interact_pointer_pos() {
                            let frac =
                                ((pos.x - rng_rect.left()) / rng_rect.width()).clamp(0.0, 1.0);
                            let midi = (24.0 + frac * 60.0).round() as u8;
                            if midi.abs_diff(sp.note_range_lo) <= midi.abs_diff(sp.note_range_hi) {
                                sp.note_range_lo = midi.min(sp.note_range_hi.saturating_sub(4));
                            } else {
                                sp.note_range_hi = midi.max(sp.note_range_lo + 4);
                            }
                        }
                    }
                    let gr = ui.painter_at(rng_rect);
                    gr.rect_filled(rng_rect, 4.0, WELL_BG);
                    let x_lo = rng_rect.left()
                        + ((f32::from(sp.note_range_lo) - 24.0) / 60.0) * rng_rect.width();
                    let x_hi = rng_rect.left()
                        + ((f32::from(sp.note_range_hi) - 24.0) / 60.0) * rng_rect.width();
                    gr.rect_filled(
                        Rect::from_min_max(
                            pos2(x_lo, rng_rect.top()),
                            pos2(x_hi, rng_rect.bottom()),
                        ),
                        2.0,
                        Color32::from_rgb(0x2b, 0x2d, 0x34),
                    );
                    gr.line_segment(
                        [
                            pos2(x_lo, rng_rect.top() + 2.0),
                            pos2(x_lo, rng_rect.bottom() - 2.0),
                        ],
                        Stroke::new(3.0, ACCENT_YELLOW),
                    );
                    gr.line_segment(
                        [
                            pos2(x_hi, rng_rect.top() + 2.0),
                            pos2(x_hi, rng_rect.bottom() - 2.0),
                        ],
                        Stroke::new(3.0, ACCENT_YELLOW),
                    );

                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Note Length").size(10.5).color(TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{}", sp.note_length))
                                    .size(10.5)
                                    .strong()
                                    .color(ACCENT_YELLOW),
                            );
                        });
                    });
                    let mut nl = f32::from(sp.note_length) / 100.0;
                    let (nl_rect, nl_resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 18.0),
                        Sense::click_and_drag(),
                    );
                    if nl_resp.dragged() || nl_resp.clicked() {
                        if let Some(pos) = nl_resp.interact_pointer_pos() {
                            nl = ((pos.x - nl_rect.left()) / nl_rect.width()).clamp(0.01, 1.0);
                            sp.note_length = (nl * 100.0).round() as u8;
                        }
                    }
                    let gnl = ui.painter_at(nl_rect);
                    let cy = nl_rect.center().y;
                    let tx = nl_rect.left() + nl * nl_rect.width();
                    gnl.line_segment(
                        [pos2(nl_rect.left(), cy), pos2(nl_rect.right(), cy)],
                        Stroke::new(3.0, WELL_BG),
                    );
                    gnl.line_segment(
                        [pos2(nl_rect.left(), cy), pos2(tx, cy)],
                        Stroke::new(3.0, ACCENT_YELLOW),
                    );
                    gnl.rect_filled(
                        Rect::from_center_size(pos2(tx, cy), vec2(8.0, 14.0)),
                        3.0,
                        Color32::from_rgb(0x6e, 0x72, 0x7c),
                    );
                });
            });
        }
        SessionPlayerKind::Keyboard => {
            // Left/Right Hand icons + Key-Range bar matching session1.png
            ui.horizontal(|ui| {
                ui.add_space(ui.available_width() * 0.30);
                ui.label(
                    RichText::new(phosphor_egui::regular::HAND_PALM)
                        .size(22.0)
                        .color(ACCENT_YELLOW),
                );
                ui.add_space(18.0);
                ui.label(
                    RichText::new(phosphor_egui::regular::HAND_PALM)
                        .size(22.0)
                        .color(ACCENT_YELLOW),
                );
            });
            let (kr_rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
            let gk = ui.painter_at(kr_rect);
            gk.rect_filled(kr_rect, 3.0, WELL_BG);
            let x_l = kr_rect.left() + kr_rect.width() * 0.33;
            let x_r = kr_rect.left() + kr_rect.width() * 0.62;
            gk.rect_filled(
                Rect::from_min_max(pos2(x_l, kr_rect.top()), pos2(x_r, kr_rect.bottom())),
                2.0,
                Color32::from_rgb(0x28, 0x2a, 0x31),
            );
            gk.rect_stroke(
                Rect::from_center_size(pos2(x_l, kr_rect.center().y), vec2(12.0, 14.0)),
                2.0,
                Stroke::new(1.0, ACCENT_YELLOW),
                StrokeKind::Inside,
            );
            gk.rect_stroke(
                Rect::from_center_size(pos2(x_r, kr_rect.center().y), vec2(12.0, 14.0)),
                2.0,
                Stroke::new(1.0, ACCENT_YELLOW),
                StrokeKind::Inside,
            );

            ui.add_space(6.0);
            ui.columns(2, |cols| {
                cols[0].label(
                    RichText::new("Left Hand")
                        .size(10.5)
                        .strong()
                        .color(TEXT_MAIN),
                );
                cols[0].horizontal(|ui| {
                    ui.label(RichText::new("Voicing").size(10.0).color(TEXT_DIM));
                    egui::ComboBox::from_id_salt("sp_lh_voicing")
                        .selected_text(RichText::new(sp.lh_voicing.label()).color(ACCENT_YELLOW))
                        .show_ui(ui, |ui| {
                            for v in VoicingMode::ALL {
                                ui.selectable_value(&mut sp.lh_voicing, v, v.label());
                            }
                        });
                });
                cols[0].horizontal(|ui| {
                    ui.label(RichText::new("Style").size(10.0).color(TEXT_DIM));
                    egui::ComboBox::from_id_salt("sp_lh_style")
                        .selected_text(RichText::new(sp.lh_style.label()).color(ACCENT_YELLOW))
                        .show_ui(ui, |ui| {
                            for st in HandStyle::ALL {
                                ui.selectable_value(&mut sp.lh_style, st, st.label());
                            }
                        });
                });

                cols[1].label(
                    RichText::new("Right Hand")
                        .size(10.5)
                        .strong()
                        .color(TEXT_MAIN),
                );
                cols[1].horizontal(|ui| {
                    ui.label(RichText::new("Voicing").size(10.0).color(TEXT_DIM));
                    egui::ComboBox::from_id_salt("sp_rh_voicing")
                        .selected_text(RichText::new(sp.rh_voicing.label()).color(ACCENT_YELLOW))
                        .show_ui(ui, |ui| {
                            for v in VoicingMode::ALL {
                                ui.selectable_value(&mut sp.rh_voicing, v, v.label());
                            }
                        });
                });
                cols[1].horizontal(|ui| {
                    ui.label(RichText::new("Movement").size(10.0).color(TEXT_DIM));
                    egui::ComboBox::from_id_salt("sp_rh_movement")
                        .selected_text(RichText::new(sp.rh_movement.label()).color(ACCENT_YELLOW))
                        .show_ui(ui, |ui| {
                            for mv in HandMovement::ALL {
                                ui.selectable_value(&mut sp.rh_movement, mv, mv.label());
                            }
                        });
                });
            });
        }
        SessionPlayerKind::Drummer => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Drum Style:").size(10.5).color(TEXT_DIM));
                egui::ComboBox::from_id_salt("sp_main_drum_style")
                    .selected_text(sp.drum_style.name())
                    .show_ui(ui, |ui| {
                        for st in DrumStyle::ALL {
                            ui.selectable_value(&mut sp.drum_style, st, st.name());
                        }
                    });
            });
        }
    }
}

fn draw_details_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    match sp.kind {
        SessionPlayerKind::Bass if sp.bass_style == BassStyle::PumpBass => {
            ui.label(
                RichText::new("Pumping (Sidechain Feel)")
                    .size(11.0)
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.add(egui::Slider::new(&mut sp.pump_amount, 0.0..=1.0).text("Amount"));
            ui.add(egui::Slider::new(&mut sp.pump_shape, 0.0..=1.0).text("Shape"));
        }
        SessionPlayerKind::Bass | SessionPlayerKind::Drummer => {
            ui.columns(2, |cols| {
                cols[0].label(
                    RichText::new("Accents")
                        .size(10.5)
                        .strong()
                        .color(TEXT_MAIN),
                );
                cols[0].add(egui::Slider::new(&mut sp.accent_vel, 0.0..=1.0).text("Velocity"));
                cols[0].add(egui::Slider::new(&mut sp.accent_len, 0.0..=1.0).text("Length"));
                cols[0].add(egui::Slider::new(&mut sp.accent_cc, 0.0..=1.0).text("MIDI CC"));
                cols[0].add(
                    egui::DragValue::new(&mut sp.cc_target)
                        .range(1..=127)
                        .prefix("CC "),
                );

                cols[1].label(
                    RichText::new("Phrase Variation")
                        .size(10.5)
                        .strong()
                        .color(TEXT_MAIN),
                );
                cols[1].add(egui::Slider::new(&mut sp.var_rhythm, 0.0..=1.0).text("Rhythm"));
                cols[1].add(egui::Slider::new(&mut sp.var_octave, 0.0..=1.0).text("Octave"));
                cols[1].add(egui::Slider::new(&mut sp.var_melody, 0.0..=1.0).text("Melody"));
            });
        }
        SessionPlayerKind::Keyboard => {
            ui.add(egui::Slider::new(&mut sp.phrasing, 0.0..=1.0).text("Phrasing"));
            ui.add(egui::Slider::new(&mut sp.strum, 0.0..=1.0).text("Strum"));
            ui.add(egui::Slider::new(&mut sp.accent_vel, 0.0..=1.0).text("Accent Velocity"));
        }
    }
}

fn draw_slides_or_env_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut sp.env_align_peak, "Align: Peak");
            ui.checkbox(&mut sp.env_invert, "Invert");
        });
        let (env_rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width().max(220.0), 86.0), Sense::hover());
        let mut sustain = sp.env_hold;
        let mut rel = sp.env_decay;
        crate::widgets::daw_adsr_envelope(
            ui,
            egui::Id::unique("session_pad_env"),
            env_rect,
            &mut sp.env_attack,
            &mut sp.env_decay,
            &mut sustain,
            &mut rel,
            1.0,
            1.0,
            1.0,
        );
        sp.env_hold = sustain;
        sp.env_decay = rel;
    } else {
        ui.horizontal_top(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(140.0, 96.0), Sense::hover());
            let g = ui.painter_at(rect);
            g.rect_filled(rect, 6.0, WELL_BG);
            let pts: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    let shaped = if sp.slide_s_curve {
                        t * t * (3.0 - 2.0 * t)
                    } else {
                        t.powf(1.0 + sp.slide_curve.abs() * 2.0)
                    };
                    pos2(
                        rect.left() + 8.0 + t * (rect.width() - 16.0),
                        rect.bottom()
                            - 10.0
                            - shaped * (rect.height() - 20.0) * sp.slide_amount.max(0.1),
                    )
                })
                .collect();
            g.add(Shape::line(pts, Stroke::new(2.0, ACCENT_YELLOW)));

            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("sp_slide_start")
                        .selected_text(match sp.slide_start {
                            SlideStart::Auto => "Auto",
                            SlideStart::ToNextNote => "To Next Note",
                            SlideStart::OnNoteStart => "On Note Start",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut sp.slide_start, SlideStart::Auto, "Auto");
                            ui.selectable_value(
                                &mut sp.slide_start,
                                SlideStart::ToNextNote,
                                "To Next Note",
                            );
                            ui.selectable_value(
                                &mut sp.slide_start,
                                SlideStart::OnNoteStart,
                                "On Note Start",
                            );
                        });
                    ui.checkbox(&mut sp.slide_s_curve, "S-Curve");
                    ui.checkbox(&mut sp.slide_auto, "Auto Time");
                });
                ui.add(egui::Slider::new(&mut sp.slide_amount, 0.0..=1.0).text("Amount"));
                ui.add(egui::Slider::new(&mut sp.slide_time, 0.05..=1.0).text("Slide Time"));
                ui.add(egui::Slider::new(&mut sp.slide_curve, -1.0..=1.0).text("Curve"));
            });
        });
    }
}

fn draw_lfo_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    ui.horizontal(|ui| {
        for (w, lbl) in [
            (LfoWave::TriSaw, "Tri/Saw"),
            (LfoWave::Sine, "Sine"),
            (LfoWave::Square, "Square"),
            (LfoWave::Random, "Random"),
        ] {
            ui.selectable_value(&mut sp.lfo_wave, w, lbl);
        }
        ui.checkbox(&mut sp.lfo_sync_chord, "Sync to Chord");
    });

    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(140.0, 76.0), Sense::hover());
        let g = ui.painter_at(rect);
        g.rect_filled(rect, 6.0, WELL_BG);
        let pts: Vec<Pos2> = (0..=32)
            .map(|i| {
                let t = i as f32 / 32.0;
                let v = eval_lfo(sp.lfo_wave, t + sp.lfo_phase, sp.lfo_symmetry, sp.seed);
                pos2(
                    rect.left() + 6.0 + t * (rect.width() - 12.0),
                    rect.bottom() - 8.0 - v * (rect.height() - 16.0),
                )
            })
            .collect();
        g.add(Shape::line(pts, Stroke::new(1.8, ACCENT_YELLOW)));

        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.add(egui::Slider::new(&mut sp.lfo_rate, 0.05..=2.0).text("Rate"));
            ui.add(egui::Slider::new(&mut sp.lfo_amount, 0.0..=1.0).text("Amount"));
            ui.add(egui::Slider::new(&mut sp.lfo_phase, 0.0..=1.0).text("Phase"));
            ui.add(egui::Slider::new(&mut sp.lfo_symmetry, 0.05..=0.95).text("Symmetry"));
        });
    });
}

fn draw_manual_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut sp.manual_override,
            "Override AI Pattern (1 Bar / 16 Steps)",
        );
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (i, step) in sp.manual_steps.iter_mut().enumerate() {
            let col = if *step { ACCENT_YELLOW } else { WELL_BG };
            let btn = egui::Button::new(
                RichText::new(format!("{}", i + 1))
                    .size(9.5)
                    .color(if *step { Color32::BLACK } else { TEXT_DIM }),
            )
            .fill(col)
            .min_size(vec2(24.0, 42.0))
            .corner_radius(CornerRadius::same(3));
            if ui.add(btn).clicked() {
                *step = !*step;
                sp.manual_override = true;
            }
        }
    });
}

fn draw_session_knob(
    ui: &mut Ui,
    cfg: DawKnobConfig<'_>,
    current_val: f32,
) -> daw_knob::DawKnobResult {
    let side = cfg.diameter.max(36.0);
    let (cell, _) = ui.allocate_exact_size(vec2(side + 26.0, side + 34.0), Sense::hover());
    let g = ui.painter().clone();
    daw_knob::draw(ui, &g, cell, cfg, current_val, None)
}
