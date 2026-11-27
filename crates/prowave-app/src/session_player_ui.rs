//! Bottom Session Player Editor (`session1.png` & `session2.png` layout).
//!
//! Features a top preview header (`Chords`, `Preset`, tools, `Snap`), a live zoomed yellow
//! Region Strip showing notes & pitch-slide curves, a left column (Player graphic, `Regenerate`,
//! vertical `Complexity` & `Intensity` sliders), a 5-tab central panel (`Main`, `Details`,
//! `Slides`/`Envelope`, `LFO`, `Manual`), and a right-hand timing/fills/feel column.

use crate::model::{Generator, Project, PITCH_BEND};
use crate::pattern::DrumStyle;
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
const CARD_BG: Color32 = Color32::from_rgb(0x23, 0x24, 0x29);
const WELL_BG: Color32 = Color32::from_rgb(0x16, 0x17, 0x1b);
const BORDER: Color32 = Color32::from_rgb(0x36, 0x38, 0x40);
const ACCENT_YELLOW: Color32 = Color32::from_rgb(0xea, 0xc3, 0x2a);
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

/// Renders the bottom Session Player Editor for the active Session Player region.
pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let target = find_session_region(p, s);
    let Some((ti, ri)) = target else {
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
    let mut tab: EditorTab = ui
        .ctx()
        .data(|d| d.get_temp(tab_id).unwrap_or(EditorTab::Main));
    let mut tool: PreviewTool = ui
        .ctx()
        .data(|d| d.get_temp(tool_id).unwrap_or(PreviewTool::Pointer));

    let Some(Generator::Session(orig)) = p.tracks[ti].regions[ri]
        .generated
        .as_ref()
        .map(|g| g.source.clone())
    else {
        return;
    };
    let mut sp = orig.clone();

    // 1. Top Preview Header Bar
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

                ui.label(RichText::new("Preset:").size(11.0).color(TEXT_DIM));
                egui::ComboBox::from_id_salt("sp_preset_combo")
                    .width(150.0)
                    .selected_text(&sp.preset_name)
                    .show_ui(ui, |ui| {
                        for name in SESSION_PRESETS {
                            if ui.selectable_label(sp.preset_name == name, name).clicked() {
                                sp.apply_preset(name);
                            }
                        }
                    });

                ui.add_space(12.0);
                if ui
                    .selectable_label(tool == PreviewTool::Pointer, "Pointer")
                    .clicked()
                {
                    tool = PreviewTool::Pointer;
                }
                if ui
                    .selectable_label(tool == PreviewTool::Pencil, "Pencil")
                    .clicked()
                {
                    tool = PreviewTool::Pencil;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Snap: Beat").size(11.0).color(TEXT_DIM));
                    ui.add_space(8.0);
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

    // 2. Zoomed Yellow Region Strip
    let (strip_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    draw_region_strip(ui, strip_rect, &p.tracks[ti].regions[ri], &sp);

    ui.add_space(6.0);

    // 3. Tab Bar (Main | Details | Slides/Envelope | LFO | Manual)
    ui.horizontal(|ui| {
        ui.add_space((ui.available_width() * 0.42).max(160.0));
        let third_label = if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
            "Envelope"
        } else {
            "Slides"
        };
        for (t, lbl) in [
            (EditorTab::Main, "Main"),
            (EditorTab::Details, "Details"),
            (EditorTab::SlidesOrEnv, third_label),
            (EditorTab::Lfo, "LFO"),
            (EditorTab::Manual, "Manual"),
        ] {
            let active = tab == t;
            let btn = egui::Button::new(RichText::new(lbl).size(11.5).strong().color(if active {
                Color32::BLACK
            } else {
                TEXT_DIM
            }))
            .fill(if active { ACCENT_YELLOW } else { CARD_BG })
            .corner_radius(CornerRadius::same(4));
            if ui.add(btn).clicked() {
                tab = t;
            }
        }
    });

    ui.add_space(6.0);

    // 4. Body: Left Column + Central Tab Panel + Right Column
    ui.horizontal_top(|ui| {
        // Left Column: Player Graphic + Regenerate + Vertical Complexity/Intensity Sliders
        ui.vertical(|ui| {
            ui.set_width(210.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(116.0);
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
                        let (disc_rect, _) =
                            ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
                        draw_player_disc(ui, disc_rect, sp.kind);
                        ui.add_space(4.0);
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Regenerate").size(11.0))
                                    .fill(Color32::from_rgb(0x3a, 0x3c, 0x42))
                                    .corner_radius(CornerRadius::same(4)),
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

        // Central Panel + Right Column inside dark rounded card
        egui::Frame::new()
            .fill(CARD_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // Central tab area
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 185.0).max(260.0));
                        match tab {
                            EditorTab::Main => draw_main_tab(ui, &mut sp),
                            EditorTab::Details => draw_details_tab(ui, &mut sp),
                            EditorTab::SlidesOrEnv => draw_slides_or_env_tab(ui, &mut sp),
                            EditorTab::Lfo => draw_lfo_tab(ui, &mut sp),
                            EditorTab::Manual => draw_manual_tab(ui, &mut sp),
                        }
                    });

                    ui.separator();

                    // Right Column: Timing, Fills & Feel
                    ui.vertical(|ui| {
                        ui.set_width(168.0);
                        if tab == EditorTab::Details {
                            ui.label(RichText::new("Phrase Length").size(10.5).color(TEXT_DIM));
                            egui::ComboBox::from_id_salt("sp_phrase_len")
                                .selected_text(sp.phrase_length.label())
                                .show_ui(ui, |ui| {
                                    for pl in PhraseLength::ALL {
                                        ui.selectable_value(&mut sp.phrase_length, pl, pl.label());
                                    }
                                });
                            ui.add(
                                egui::Slider::new(&mut sp.chord_response, 0.0..=1.0)
                                    .text("Chord Resp."),
                            );
                            ui.label(RichText::new("Tempo: Automatic").size(10.5).color(TEXT_DIM));
                            ui.add(egui::Slider::new(&mut sp.feel, -1.0..=1.0).text("Feel"));
                            ui.add(egui::Slider::new(&mut sp.humanize, 0.0..=1.0).text("Humanize"));
                        } else {
                            ui.horizontal(|ui| {
                                yellow_arc_knob(ui, "Fill Amount", &mut sp.fill_amount);
                                ui.add_space(6.0);
                                yellow_arc_knob(ui, "Swing", &mut sp.swing);
                            });
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                yellow_arc_knob(ui, "Fill Comp.", &mut sp.fill_complexity);
                                ui.add_space(10.0);
                                let sub_lbl = match sp.swing_subdiv {
                                    SwingSubdiv::Eighth => "8th",
                                    SwingSubdiv::Sixteenth => "16th",
                                };
                                if ui.button(sub_lbl).clicked() {
                                    sp.swing_subdiv = match sp.swing_subdiv {
                                        SwingSubdiv::Eighth => SwingSubdiv::Sixteenth,
                                        SwingSubdiv::Sixteenth => SwingSubdiv::Eighth,
                                    };
                                }
                            });
                        }
                    });
                });
            });
    });

    ui.ctx().data_mut(|d| {
        d.insert_temp(tab_id, tab);
        d.insert_temp(tool_id, tool);
    });

    if sp != orig {
        if let Some(g) = p.tracks[ti].regions[ri].generated.as_mut() {
            g.source = Generator::Session(sp);
        }
        p.regenerate(ti, ri);
    }
}

fn find_session_region(p: &Project, s: &UiState) -> Option<(usize, usize)> {
    for (ti, ri) in s.tracks.selection() {
        if let Some(r) = p.tracks.get(ti).and_then(|t| t.regions.get(ri)) {
            if matches!(
                r.generated.as_ref().map(|g| &g.source),
                Some(Generator::Session(_))
            ) {
                return Some((ti, ri));
            }
        }
    }
    let ti = p.selected_track;
    if let Some(t) = p.tracks.get(ti) {
        if let Some(ri) = t.regions.iter().position(|r| {
            matches!(
                r.generated.as_ref().map(|g| &g.source),
                Some(Generator::Session(_))
            )
        }) {
            return Some((ti, ri));
        }
    }
    None
}

fn draw_region_strip(ui: &Ui, rect: Rect, region: &crate::model::Region, sp: &SessionPlayerParams) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 3.0, Color32::from_rgb(0x4b, 0x45, 0x2c));
    let header_h = 16.0;
    let header = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + header_h));
    g.rect_filled(header, 0.0, Color32::from_rgb(0xe8, 0xe2, 0xc4));

    let style_lbl = match sp.kind {
        SessionPlayerKind::Keyboard => sp.keyboard_style.label(),
        SessionPlayerKind::Bass => sp.bass_style.label(),
        SessionPlayerKind::Drummer => sp.drum_style.name(),
    };
    g.text(
        pos2(header.left() + 8.0, header.center().y),
        Align2::LEFT_CENTER,
        format!("{} - {style_lbl}", sp.kind.label()),
        FontId::proportional(10.0),
        Color32::from_rgb(0x28, 0x25, 0x1a),
    );

    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
    let total_beats = (region.len_bars * 4.0).max(4.0);
    for bar in 1..=(region.len_bars.ceil() as usize) {
        let x = body.left() + ((bar - 1) as f32 * 4.0 / total_beats) * body.width();
        g.line_segment(
            [pos2(x, body.top()), pos2(x, body.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 35)),
        );
    }

    for n in &region.notes {
        let x0 = body.left() + (n.start / total_beats) * body.width();
        let x1 = (x0 + (n.len / total_beats) * body.width()).max(x0 + 3.0);
        let pitch_norm = ((f32::from(n.pitch) - 28.0) / 60.0).clamp(0.05, 0.95);
        let y = body.bottom() - pitch_norm * body.height();
        let col = Color32::from_rgba_unmultiplied(255, 250, 225, (120 + n.vel / 2).min(255));
        g.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(2.0, col));
    }

    // Draw Pitch-Bend slide curve overlay if present
    let mut bend_pts: Vec<Pos2> = region
        .controls
        .iter()
        .filter(|c| c.num == PITCH_BEND)
        .map(|c| {
            let x = body.left() + (c.start / total_beats) * body.width();
            let y = body.bottom() - (f32::from(c.val) / 16383.0).clamp(0.0, 1.0) * body.height();
            pos2(x, y)
        })
        .collect();
    if bend_pts.len() >= 2 {
        bend_pts.sort_by(|a, b| a.x.total_cmp(&b.x));
        g.add(Shape::line(bend_pts, Stroke::new(1.2, Color32::WHITE)));
    }
}

fn draw_player_disc(ui: &Ui, rect: Rect, kind: SessionPlayerKind) {
    let g = ui.painter_at(rect);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.48;
    g.circle_filled(c, r, Color32::from_rgb(0x16, 0x17, 0x1b));
    g.circle_stroke(c, r, Stroke::new(1.0, BORDER));
    let stroke = Stroke::new(1.4, ACCENT_YELLOW);
    match kind {
        SessionPlayerKind::Keyboard => {
            g.rect_stroke(
                Rect::from_center_size(c + vec2(0.0, 2.0), vec2(44.0, 22.0)),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            for i in -3..=3 {
                let x = c.x + i as f32 * 5.5;
                g.line_segment([pos2(x, c.y - 9.0), pos2(x, c.y + 4.0)], stroke);
            }
        }
        SessionPlayerKind::Bass => {
            g.rect_stroke(
                Rect::from_center_size(c, vec2(44.0, 26.0)),
                2.0,
                stroke,
                StrokeKind::Inside,
            );
            g.circle_stroke(c + vec2(-12.0, -5.0), 3.0, stroke);
            g.circle_stroke(c + vec2(0.0, -5.0), 3.0, stroke);
            g.circle_stroke(c + vec2(12.0, -5.0), 3.0, stroke);
            g.line_segment([c + vec2(-18.0, 6.0), c + vec2(18.0, 6.0)], stroke);
        }
        SessionPlayerKind::Drummer => {
            g.circle_stroke(c + vec2(0.0, 6.0), 14.0, stroke);
            g.circle_stroke(c + vec2(-15.0, -8.0), 8.0, stroke);
            g.circle_stroke(c + vec2(15.0, -8.0), 8.0, stroke);
        }
    }
}

fn vertical_yellow_slider(ui: &mut Ui, label: &str, val: &mut f32) {
    ui.vertical(|ui| {
        ui.set_width(40.0);
        ui.label(RichText::new(label).size(9.5).color(TEXT_DIM));
        let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 108.0), Sense::click_and_drag());
        if (resp.dragged() || resp.clicked()) && resp.interact_pointer_pos().is_some() {
            if let Some(pos) = resp.interact_pointer_pos() {
                *val = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            }
        }
        let g = ui.painter_at(rect);
        let x = rect.center().x;
        g.line_segment(
            [pos2(x, rect.top() + 4.0), pos2(x, rect.bottom() - 4.0)],
            Stroke::new(3.0, WELL_BG),
        );
        let thumb_y = rect.bottom() - 4.0 - *val * (rect.height() - 8.0);
        g.line_segment(
            [pos2(x, rect.bottom() - 4.0), pos2(x, thumb_y)],
            Stroke::new(3.0, ACCENT_YELLOW),
        );
        g.rect_filled(
            Rect::from_center_size(pos2(x, thumb_y), vec2(16.0, 7.0)),
            3.0,
            Color32::from_rgb(0x6e, 0x72, 0x7c),
        );
    });
}

fn yellow_arc_knob(ui: &mut Ui, label: &str, val: &mut f32) {
    ui.vertical(|ui| {
        ui.set_width(70.0);
        ui.label(RichText::new(label).size(10.0).color(TEXT_DIM));
        let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::drag());
        if resp.dragged() {
            *val = (*val - resp.drag_delta().y * 0.008).clamp(0.0, 1.0);
        }
        let g = ui.painter_at(rect);
        let c = rect.center();
        let r = 15.0;
        g.circle_filled(c, r, Color32::from_rgb(0x2f, 0x31, 0x38));
        let a0 = std::f32::consts::PI * 0.75;
        let sweep = *val * std::f32::consts::PI * 1.5;
        let segs = 16;
        let pts: Vec<Pos2> = (0..=segs)
            .map(|i| {
                let a = a0 + sweep * (i as f32 / segs as f32);
                pos2(c.x + (r + 3.0) * a.cos(), c.y + (r + 3.0) * a.sin())
            })
            .collect();
        g.add(Shape::line(pts, Stroke::new(2.2, ACCENT_YELLOW)));
        let end_a = a0 + sweep;
        g.line_segment(
            [
                c,
                pos2(c.x + r * 0.8 * end_a.cos(), c.y + r * 0.8 * end_a.sin()),
            ],
            Stroke::new(1.8, ACCENT_YELLOW),
        );
    });
}

fn draw_main_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    // Pattern strip (1..8) + Follow Rhythm
    ui.horizontal(|ui| {
        for i in 0..8 {
            let active = sp.pattern_idx == i;
            if ui
                .selectable_label(
                    active,
                    RichText::new(format!("{}", i + 1))
                        .size(10.5)
                        .color(if active { ACCENT_YELLOW } else { TEXT_DIM }),
                )
                .clicked()
            {
                sp.pattern_idx = i;
            }
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

    ui.separator();

    match sp.kind {
        SessionPlayerKind::Bass => {
            ui.horizontal_top(|ui| {
                // Square XY Pad (Y: Octave, X: Melody)
                let (xy_rect, xy_resp) =
                    ui.allocate_exact_size(vec2(130.0, 108.0), Sense::click_and_drag());
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
                let px = xy_rect.left() + sp.xy_melody * xy_rect.width();
                let py = xy_rect.bottom() - sp.xy_octave * xy_rect.height();
                g.circle_filled(pos2(px, py), 10.0, ACCENT_YELLOW);

                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Note Range: MIDI {} - {}",
                            sp.note_range_lo, sp.note_range_hi
                        ))
                        .size(10.5)
                        .color(ACCENT_YELLOW),
                    );
                    ui.add(egui::Slider::new(&mut sp.note_range_lo, 24..=60).text("Low"));
                    ui.add(egui::Slider::new(&mut sp.note_range_hi, 36..=84).text("High"));
                    ui.add(egui::Slider::new(&mut sp.note_length, 1..=100).text("Note Length"));
                });
            });
        }
        SessionPlayerKind::Keyboard => {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Style:").size(10.5).color(TEXT_DIM));
                egui::ComboBox::from_id_salt("sp_main_kb_style")
                    .selected_text(sp.keyboard_style.label())
                    .show_ui(ui, |ui| {
                        for st in KeyboardStyle::ALL {
                            ui.selectable_value(&mut sp.keyboard_style, st, st.label());
                        }
                    });
                ui.checkbox(&mut sp.tie_notes, "Tie Notes");
            });
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                cols[0].label(
                    RichText::new("Left Hand")
                        .size(10.5)
                        .strong()
                        .color(TEXT_MAIN),
                );
                egui::ComboBox::from_id_salt("sp_lh_voicing")
                    .selected_text(sp.lh_voicing.label())
                    .show_ui(&mut cols[0], |ui| {
                        for v in VoicingMode::ALL {
                            ui.selectable_value(&mut sp.lh_voicing, v, v.label());
                        }
                    });
                egui::ComboBox::from_id_salt("sp_lh_style")
                    .selected_text(sp.lh_style.label())
                    .show_ui(&mut cols[0], |ui| {
                        for st in HandStyle::ALL {
                            ui.selectable_value(&mut sp.lh_style, st, st.label());
                        }
                    });

                cols[1].label(
                    RichText::new("Right Hand")
                        .size(10.5)
                        .strong()
                        .color(TEXT_MAIN),
                );
                egui::ComboBox::from_id_salt("sp_rh_voicing")
                    .selected_text(sp.rh_voicing.label())
                    .show_ui(&mut cols[1], |ui| {
                        for v in VoicingMode::ALL {
                            ui.selectable_value(&mut sp.rh_voicing, v, v.label());
                        }
                    });
                egui::ComboBox::from_id_salt("sp_rh_movement")
                    .selected_text(sp.rh_movement.label())
                    .show_ui(&mut cols[1], |ui| {
                        for mv in HandMovement::ALL {
                            ui.selectable_value(&mut sp.rh_movement, mv, mv.label());
                        }
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
        ui.add(egui::Slider::new(&mut sp.env_attack, 0.01..=1.0).text("Attack"));
        ui.add(egui::Slider::new(&mut sp.env_hold, 0.0..=1.0).text("Hold"));
        ui.add(egui::Slider::new(&mut sp.env_decay, 0.01..=1.0).text("Decay"));
        ui.add(egui::Slider::new(&mut sp.env_shift_start, -0.5..=0.5).text("Shift Start"));
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
