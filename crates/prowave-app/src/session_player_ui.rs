//! Bottom Session Player Editor (`session1.png` & `session2.png` layout).
//!
//! Replicates the Logic Pro Session Player Editor: top `Session Player` header strip,
//! unified `Chords` pill, `Preset:` selector with double-chevron indicator, segmented
//! Pointer/Pencil tool switcher, `Snap:` dropdown + magnet button, catch-playhead +
//! zoom slider, beat-subdivision ruler + region strip (triangular velocity transients for
//! Drummer; note stems + pitch-bend glides for Keyboard/Bass), left column (wireframe kit /
//! synth / piano disc with radial yellow glow, `Regenerate`, thick vertical yellow
//! `Complexity` & `Intensity` faders), compact borderless segmented tab bar (3 tabs for
//! Drummer, 5 tabs for Keyboard/Bass), Drummer kit piece toggles + 4x4 pattern dot matrices,
//! and right-hand golden-yellow `daw_knob` Fill & Swing controls with lock icons.

use crate::daw_knob::{self, DawKnobConfig, DawKnobFinish, DawKnobStyle};
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
const STRIP_BG: Color32 = Color32::from_rgb(0x17, 0x18, 0x1c);
const CARD_BG: Color32 = Color32::from_rgb(0x21, 0x22, 0x27);
const WELL_BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
const PILL_BG: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x33);
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SnapMode {
    Beat,
    Bar,
    Sixteenth,
}

impl SnapMode {
    const ALL: [Self; 3] = [Self::Beat, Self::Bar, Self::Sixteenth];
    fn label(self) -> &'static str {
        match self {
            Self::Beat => "Beat",
            Self::Bar => "Bar",
            Self::Sixteenth => "1/16",
        }
    }
}

fn midi_note_name(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let oct = i32::from(midi / 12) - 1;
    format!("{}{}", NAMES[usize::from(midi % 12)], oct)
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cap = true;
    for ch in s.chars() {
        if ch.is_whitespace() || ch == '-' || ch == '_' {
            out.push(' ');
            cap = true;
        } else if cap {
            for up in ch.to_uppercase() {
                out.push(up);
            }
            cap = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn from_drummer_params(d: &DrummerParams) -> SessionPlayerParams {
    SessionPlayerParams {
        kind: SessionPlayerKind::Drummer,
        preset_name: "Back Roads".to_string(),
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
    let snap_mode_id = egui::Id::unique("session_player_snap_mode");
    let catch_id = egui::Id::unique("session_player_catch_playhead");
    let zoom_id = egui::Id::unique("session_player_zoom");

    let mut tab: EditorTab = ui
        .ctx()
        .data(|d| d.get_temp(tab_id).unwrap_or(EditorTab::Main));
    let mut tool: PreviewTool = ui
        .ctx()
        .data(|d| d.get_temp(tool_id).unwrap_or(PreviewTool::Pointer));
    let mut snap_on: bool = ui.ctx().data(|d| d.get_temp(snap_id).unwrap_or(true));
    let mut snap_mode: SnapMode = ui
        .ctx()
        .data(|d| d.get_temp(snap_mode_id).unwrap_or(SnapMode::Beat));
    let mut catch_on: bool = ui.ctx().data(|d| d.get_temp(catch_id).unwrap_or(true));
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

    if sp.kind == SessionPlayerKind::Drummer
        && matches!(tab, EditorTab::SlidesOrEnv | EditorTab::Lfo)
    {
        tab = EditorTab::Main;
    }

    // 0. Top Window Strip ("Session Player")
    egui::Frame::new()
        .fill(STRIP_BG)
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x28, 0x2a, 0x31)))
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Session Player")
                        .size(11.0)
                        .strong()
                        .color(TEXT_MAIN),
                );
            });
        });

    // 1. Top Preview Toolbar (Chords pill, Preset w/ ^v, Segmented Tool Switcher, Snap/Catch/Zoom)
    egui::Frame::new()
        .fill(BG_DARK)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Unified "Chords v" dropdown pill
                let chords_lbl = format!("Chords  {}", phosphor_egui::regular::CARET_DOWN);
                let chords_btn =
                    egui::Button::new(RichText::new(chords_lbl).size(11.0).color(TEXT_MAIN))
                        .fill(PILL_BG)
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(CornerRadius::same(4))
                        .min_size(vec2(72.0, 20.0));
                let chords_resp = ui.add(chords_btn);
                egui::Popup::menu(&chords_resp).show(|ui| {
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

                ui.add_space(10.0);
                ui.label(RichText::new("Preset:").size(11.0).color(TEXT_DIM));
                let preset_lbl = format!(
                    "{}   {}",
                    sp.preset_name,
                    phosphor_egui::regular::CARET_UP_DOWN
                );
                let preset_btn =
                    egui::Button::new(RichText::new(preset_lbl).size(11.0).color(TEXT_MAIN))
                        .fill(Color32::from_rgb(0x25, 0x27, 0x2d))
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(CornerRadius::same(4))
                        .min_size(vec2(158.0, 20.0));
                let preset_resp = ui.add(preset_btn);
                egui::Popup::menu(&preset_resp).show(|ui| {
                    for name in SESSION_PRESETS {
                        if ui.selectable_label(sp.preset_name == name, name).clicked() {
                            let saved_fill = sp.fill_amount;
                            let saved_swing = sp.swing;
                            sp.apply_preset(name);
                            if sp.lock_fill_amount {
                                sp.fill_amount = saved_fill;
                            }
                            if sp.lock_swing {
                                sp.swing = saved_swing;
                            }
                            ui.close();
                        }
                    }
                });

                ui.add_space((ui.available_width() * 0.12).max(12.0));

                // Segmented Tool Switcher Pill (Pointer v | Pencil v)
                egui::Frame::new()
                    .fill(WELL_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(1, 1))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.horizontal(|ui| {
                            let ptr_txt = format!(
                                "{} {}",
                                phosphor_egui::regular::CURSOR,
                                phosphor_egui::regular::CARET_DOWN
                            );
                            let ptr_active = tool == PreviewTool::Pointer;
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(ptr_txt).size(10.5).color(if ptr_active {
                                            Color32::WHITE
                                        } else {
                                            TEXT_DIM
                                        }),
                                    )
                                    .fill(if ptr_active {
                                        BLUE_ACTIVE
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .corner_radius(CornerRadius::same(3))
                                    .min_size(vec2(34.0, 18.0)),
                                )
                                .on_hover_text("Pointer Tool")
                                .clicked()
                            {
                                tool = PreviewTool::Pointer;
                            }

                            let (div, _) = ui.allocate_exact_size(vec2(1.0, 16.0), Sense::hover());
                            ui.painter().rect_filled(div, 0.0, BORDER);

                            let pen_txt = format!(
                                "{} {}",
                                phosphor_egui::regular::PENCIL_SIMPLE,
                                phosphor_egui::regular::CARET_DOWN
                            );
                            let pen_active = tool == PreviewTool::Pencil;
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(pen_txt).size(10.5).color(if pen_active {
                                            Color32::WHITE
                                        } else {
                                            TEXT_DIM
                                        }),
                                    )
                                    .fill(if pen_active {
                                        BLUE_ACTIVE
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .corner_radius(CornerRadius::same(3))
                                    .min_size(vec2(34.0, 18.0)),
                                )
                                .on_hover_text("Pencil / Line Tool")
                                .clicked()
                            {
                                tool = PreviewTool::Pencil;
                            }
                        });
                    });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // White-thumb Zoom Slider
                    draw_white_thumb_zoom_slider(ui, &mut zoom);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(phosphor_egui::regular::ARROWS_LEFT_RIGHT)
                            .size(11.0)
                            .color(TEXT_DIM),
                    );
                    ui.add_space(4.0);

                    // Blue Catch-Playhead Button
                    let catch_btn = egui::Button::new(
                        RichText::new(phosphor_egui::regular::ARROWS_HORIZONTAL)
                            .size(11.0)
                            .color(if catch_on { Color32::WHITE } else { TEXT_DIM }),
                    )
                    .fill(if catch_on { BLUE_ACTIVE } else { PILL_BG })
                    .corner_radius(CornerRadius::same(4))
                    .min_size(vec2(22.0, 19.0));
                    if ui.add(catch_btn).on_hover_text("Catch Playhead").clicked() {
                        catch_on = !catch_on;
                    }

                    ui.add_space(6.0);

                    // Blue Magnet Snap Toggle Button
                    let snap_btn = egui::Button::new(
                        RichText::new(phosphor_egui::regular::MAGNET)
                            .size(11.0)
                            .color(if snap_on { Color32::WHITE } else { TEXT_DIM }),
                    )
                    .fill(if snap_on { BLUE_ACTIVE } else { PILL_BG })
                    .corner_radius(CornerRadius::same(4))
                    .min_size(vec2(22.0, 19.0));
                    if ui.add(snap_btn).on_hover_text("Toggle Snap").clicked() {
                        snap_on = !snap_on;
                    }

                    // Interactive Snap Mode Dropdown ("Beat v")
                    let snap_lbl = format!(
                        "{} {}",
                        snap_mode.label(),
                        phosphor_egui::regular::CARET_DOWN
                    );
                    let snap_mode_resp = ui.add(
                        egui::Button::new(RichText::new(snap_lbl).size(11.0).color(TEXT_MAIN))
                            .fill(PILL_BG)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(CornerRadius::same(4))
                            .min_size(vec2(54.0, 19.0)),
                    );
                    egui::Popup::menu(&snap_mode_resp).show(|ui| {
                        for m in SnapMode::ALL {
                            if ui.selectable_label(snap_mode == m, m.label()).clicked() {
                                snap_mode = m;
                                ui.close();
                            }
                        }
                    });
                    ui.label(RichText::new("Snap:").size(11.0).color(TEXT_DIM));
                });
            });
        });

    // 2. Bar Ruler (beat subdivisions 1, 1.3, 2, 2.3...) + Zoomed Region Strip
    let (strip_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::hover());
    draw_bar_ruler_and_strip(ui, strip_rect, &p.tracks[ti].regions[ri], &sp, zoom);

    ui.add_space(8.0);

    // 3. Main Body: Left Column + Central Segmented Tab Bar & Dark Card
    let left_col_w = 246.0;
    let third_label = if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
        "Envelope"
    } else {
        "Slides"
    };

    ui.horizontal_top(|ui| {
        // Left Column: Player Identity + Kit/Instrument Disc + Regenerate + Vertical Yellow Faders
        ui.vertical(|ui| {
            ui.set_width(left_col_w);
            ui.add_space(10.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(120.0);
                    let (role_lbl, style_name) = match sp.kind {
                        SessionPlayerKind::Keyboard => {
                            ("Session Keyboardist", title_case(sp.keyboard_style.label()))
                        }
                        SessionPlayerKind::Bass => {
                            ("Session Bassist", title_case(sp.bass_style.label()))
                        }
                        SessionPlayerKind::Drummer => (
                            if sp.drum_style == DrumStyle::Electronic {
                                "Electronic Drummer"
                            } else {
                                "Acoustic Drummer"
                            },
                            title_case(sp.drum_style.name()),
                        ),
                    };
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(role_lbl).size(10.5).color(TEXT_DIM));
                        ui.label(
                            RichText::new(style_name)
                                .size(11.5)
                                .strong()
                                .color(ACCENT_YELLOW),
                        );
                        ui.add_space(5.0);
                        let (disc_rect, _) =
                            ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
                        draw_player_disc(ui, disc_rect, sp.kind);
                        ui.add_space(6.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Regenerate").size(11.0).color(TEXT_MAIN),
                                )
                                .fill(Color32::from_rgb(0x3a, 0x3c, 0x42))
                                .corner_radius(CornerRadius::same(4))
                                .min_size(vec2(86.0, 22.0)),
                            )
                            .clicked()
                        {
                            sp.seed = sp.seed.wrapping_add(1);
                        }
                    });
                });

                ui.add_space(4.0);
                vertical_yellow_fader(ui, "Complexity", &mut sp.complexity);
                ui.add_space(4.0);
                vertical_yellow_fader(ui, "Intensity", &mut sp.intensity);
            });
        });

        ui.add_space(8.0);

        // Right Area: Compact Borderless Segmented Tab Bar + Dark Rounded Card
        ui.vertical(|ui| {
            let tabs: Vec<(EditorTab, &str)> = if sp.kind == SessionPlayerKind::Drummer {
                vec![
                    (EditorTab::Main, "Main"),
                    (EditorTab::Details, "Details"),
                    (EditorTab::Manual, "Manual"),
                ]
            } else {
                vec![
                    (EditorTab::Main, "Main"),
                    (EditorTab::Details, "Details"),
                    (EditorTab::SlidesOrEnv, third_label),
                    (EditorTab::Lfo, "LFO"),
                    (EditorTab::Manual, "Manual"),
                ]
            };

            // Segmented Tab Control aligned above right-center of card
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() * 0.30).max(24.0));
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x16, 0x17, 0x1b))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.horizontal(|ui| {
                            for (idx, (t, lbl)) in tabs.iter().enumerate() {
                                if idx > 0 {
                                    let (div, _) =
                                        ui.allocate_exact_size(vec2(1.0, 14.0), Sense::hover());
                                    ui.painter().rect_filled(
                                        div,
                                        0.0,
                                        Color32::from_rgb(0x2a, 0x2c, 0x34),
                                    );
                                }
                                let active = tab == *t;
                                let btn = egui::Button::new(
                                    RichText::new(*lbl).size(11.0).strong().color(if active {
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
                                .corner_radius(CornerRadius::same(4))
                                .min_size(vec2(54.0, 20.0));
                                if ui.add(btn).clicked() {
                                    tab = *t;
                                }
                            }
                        });
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
                            ui.set_width((ui.available_width() - 210.0).max(260.0));
                            match tab {
                                EditorTab::Main => draw_main_tab(ui, &mut sp),
                                EditorTab::Details => draw_details_tab(ui, &mut sp),
                                EditorTab::SlidesOrEnv => draw_slides_or_env_tab(ui, &mut sp),
                                EditorTab::Lfo => draw_lfo_tab(ui, &mut sp),
                                EditorTab::Manual => draw_manual_tab(ui, &mut sp),
                            }
                        });

                        ui.add_space(14.0);

                        // Right Column: Fill & Swing Knobs (no hard full-height vertical rule)
                        ui.vertical(|ui| {
                            ui.set_width(190.0);
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
        d.insert_temp(snap_mode_id, snap_mode);
        d.insert_temp(catch_id, catch_on);
        d.insert_temp(zoom_id, zoom);
    });

    if sp != orig || was_legacy_drummer {
        let kind_or_style_changed = was_legacy_drummer
            || sp.kind != orig.kind
            || sp.bass_style != orig.bass_style
            || sp.keyboard_style != orig.keyboard_style;
        if kind_or_style_changed {
            p.sync_session_track(ti, &sp);
        }
        if let Some(g) = p.tracks[ti].regions[ri].generated.as_mut() {
            g.source = Generator::Session(sp);
        }
        p.regenerate(ti, ri);
    }
}

fn draw_white_thumb_zoom_slider(ui: &mut Ui, zoom: &mut f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 18.0), Sense::click_and_drag());
    if (resp.dragged() || resp.clicked()) && resp.interact_pointer_pos().is_some() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let norm = ((pos.x - (rect.left() + 6.0)) / (rect.width() - 12.0)).clamp(0.0, 1.0);
            *zoom = 0.5 + norm * 1.5;
        }
    }
    let g = ui.painter_at(rect);
    let cy = rect.center().y;
    let x0 = rect.left() + 6.0;
    let x1 = rect.right() - 6.0;
    g.line_segment(
        [pos2(x0, cy), pos2(x1, cy)],
        Stroke::new(3.0, Color32::from_rgb(0x3a, 0x3d, 0x45)),
    );
    let norm = ((*zoom - 0.5) / 1.5).clamp(0.0, 1.0);
    let tx = x0 + norm * (x1 - x0);
    g.circle_filled(pos2(tx, cy), 5.0, Color32::WHITE);
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
    let ruler_h = 16.0;
    let header_h = 16.0;
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
    let num_half_bars = (total_beats / 2.0).ceil() as usize;
    for hb in 0..num_half_bars {
        let beat = hb as f32 * 2.0;
        let x = rect.left() + (beat / total_beats) * rect.width();
        let is_bar = hb % 2 == 0;
        let bar_num = (hb / 2) + 1;
        g.line_segment(
            [
                pos2(
                    x,
                    if is_bar {
                        ruler.top()
                    } else {
                        ruler.center().y
                    },
                ),
                pos2(x, body.bottom()),
            ],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, if is_bar { 45 } else { 18 }),
            ),
        );
        let lbl = if is_bar {
            format!("{bar_num}")
        } else {
            format!("{bar_num}.3")
        };
        g.text(
            pos2(x + 3.0, ruler.center().y),
            Align2::LEFT_CENTER,
            lbl,
            FontId::proportional(9.5),
            if is_bar {
                TEXT_MAIN
            } else {
                Color32::from_rgb(0x6e, 0x73, 0x7e)
            },
        );
    }

    // Framed play icon + repeating region name across the header strip
    let icon_box = Rect::from_center_size(
        pos2(header.left() + 12.0, header.center().y),
        vec2(11.0, 10.0),
    );
    g.rect_stroke(
        icon_box,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(0x3a, 0x34, 0x20)),
        StrokeKind::Inside,
    );
    let tri_c = icon_box.center();
    g.add(Shape::convex_polygon(
        vec![
            pos2(tri_c.x - 2.0, tri_c.y - 2.5),
            pos2(tri_c.x + 2.5, tri_c.y),
            pos2(tri_c.x - 2.0, tri_c.y + 2.5),
        ],
        Color32::from_rgb(0x28, 0x25, 0x1a),
        Stroke::NONE,
    ));

    let style_lbl = match sp.kind {
        SessionPlayerKind::Keyboard => title_case(sp.keyboard_style.label()),
        SessionPlayerKind::Bass => title_case(sp.bass_style.label()),
        SessionPlayerKind::Drummer => title_case(sp.drum_style.name()),
    };
    let region_title = format!("{} - {style_lbl}", sp.preset_name);
    let repeat_px = 190.0_f32;
    let count = ((header.width() - 26.0) / repeat_px).ceil().max(1.0) as usize;
    for i in 0..count {
        let tx = header.left() + 24.0 + i as f32 * repeat_px;
        if tx < header.right() - 24.0 {
            g.text(
                pos2(tx, header.center().y),
                Align2::LEFT_CENTER,
                &region_title,
                FontId::proportional(9.5),
                Color32::from_rgb(0x28, 0x25, 0x1a),
            );
        }
    }

    if sp.kind == SessionPlayerKind::Drummer {
        // Two-lane high-contrast triangular transient/velocity shapes (top: cymbals/hats, bottom: kick/snare)
        let mid_y = body.top() + body.height() * 0.44;
        for n in &region.notes {
            let x0 = body.left() + (n.start / total_beats) * body.width();
            if x0 > body.right() {
                continue;
            }
            let vel_norm = (f32::from(n.vel) / 127.0).clamp(0.25, 1.0);
            let is_top_lane = n.pitch >= 42;
            let (base_y, max_h, w) = if is_top_lane {
                (mid_y - 2.0, (mid_y - body.top() - 3.0) * vel_norm, 6.0)
            } else {
                (
                    body.bottom() - 2.0,
                    (body.bottom() - mid_y - 3.0) * vel_norm,
                    8.5,
                )
            };
            let col = Color32::from_rgba_unmultiplied(255, 253, 240, (175 + n.vel / 2).min(255));
            g.add(Shape::convex_polygon(
                vec![
                    pos2(x0, base_y - max_h),
                    pos2(x0 + w, base_y),
                    pos2(x0, base_y),
                ],
                col,
                Stroke::NONE,
            ));
        }
    } else {
        // Note stems with notehead + tail for Keyboard / Bass
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
                let y =
                    body.bottom() - (f32::from(c.val) / 16383.0).clamp(0.05, 0.95) * body.height();
                pos2(x, y)
            })
            .collect();
        if bend_pts.len() >= 2 {
            bend_pts.sort_by(|a, b| a.x.total_cmp(&b.x));
            g.add(Shape::line(bend_pts, Stroke::new(1.3, Color32::WHITE)));
        }
    }
}

fn draw_player_disc(ui: &Ui, rect: Rect, kind: SessionPlayerKind) {
    let g = ui.painter_at(rect);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.48;
    // Subtle yellow radial glow behind disc
    g.circle_filled(c, r, Color32::from_rgb(0x14, 0x15, 0x18));
    g.circle_filled(
        c,
        r * 0.84,
        Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 14),
    );
    g.circle_filled(
        c,
        r * 0.56,
        Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 24),
    );
    g.circle_stroke(c, r, Stroke::new(1.0, BORDER));

    let st = Stroke::new(1.25, ACCENT_YELLOW);
    let dim_st = Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 150));
    match kind {
        SessionPlayerKind::Keyboard => {
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
            // Top-down wireframe acoustic drum kit: kick, snare, rack toms, floor tom, hi-hat, crash & ride
            // Kick drum (bottom center) + pedal
            let kick_rect = Rect::from_center_size(c + vec2(0.0, 12.0), vec2(22.0, 16.0));
            g.rect_stroke(kick_rect, 4.0, st, StrokeKind::Inside);
            g.rect_stroke(
                Rect::from_center_size(c + vec2(0.0, 23.0), vec2(7.0, 8.0)),
                2.0,
                dim_st,
                StrokeKind::Inside,
            );
            // Snare drum (left-center)
            let snare_c = c + vec2(-15.0, 8.0);
            g.circle_stroke(snare_c, 9.5, st);
            g.circle_stroke(snare_c, 7.2, dim_st);
            // Rack toms (top-left, top-right)
            g.circle_stroke(c + vec2(-9.0, -5.0), 7.0, st);
            g.circle_stroke(c + vec2(9.0, -5.0), 7.5, st);
            // Floor tom (bottom-right)
            g.circle_stroke(c + vec2(17.0, 11.0), 9.5, st);
            g.circle_stroke(c + vec2(17.0, 11.0), 7.2, dim_st);
            // Hi-hat (far-left)
            let hh_c = c + vec2(-25.0, -2.0);
            g.circle_stroke(hh_c, 7.5, st);
            g.circle_filled(hh_c, 1.5, ACCENT_YELLOW);
            // Crash cymbal (top-left)
            let crash_c = c + vec2(-20.0, -18.0);
            g.circle_stroke(crash_c, 10.5, st);
            g.circle_stroke(crash_c, 2.8, dim_st);
            // Ride cymbal (top-right)
            let ride_c = c + vec2(21.0, -14.0);
            g.circle_stroke(ride_c, 12.0, st);
            g.circle_stroke(ride_c, 3.0, dim_st);
        }
    }
}

fn vertical_yellow_fader(ui: &mut Ui, label: &str, val: &mut f32) {
    ui.vertical(|ui| {
        ui.set_width(56.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(label).size(9.5).color(TEXT_DIM));
            let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 124.0), Sense::click_and_drag());
            if (resp.dragged() || resp.clicked()) && resp.interact_pointer_pos().is_some() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    *val = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                }
            }
            let g = ui.painter_at(rect);
            let x = rect.center().x;
            let track = Rect::from_min_max(
                pos2(x - 4.0, rect.top() + 6.0),
                pos2(x + 4.0, rect.bottom() - 6.0),
            );
            g.rect_filled(track, 4.0, WELL_BG);
            let thumb_y = track.bottom() - *val * track.height();
            let fill = Rect::from_min_max(pos2(x - 2.0, thumb_y), pos2(x + 2.0, track.bottom()));
            g.rect_filled(fill, 2.0, ACCENT_YELLOW);
            let thumb = Rect::from_center_size(pos2(x, thumb_y), vec2(24.0, 11.0));
            g.rect_filled(thumb, 4.5, Color32::from_rgb(0x5c, 0x60, 0x6a));
            g.rect_stroke(
                thumb,
                4.5,
                Stroke::new(1.0, Color32::from_rgb(0x28, 0x2a, 0x30)),
                StrokeKind::Inside,
            );
            g.line_segment(
                [
                    pos2(thumb.left() + 4.0, thumb.center().y),
                    pos2(thumb.right() - 4.0, thumb.center().y),
                ],
                Stroke::new(2.0, ACCENT_YELLOW),
            );
        });
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
                diameter: 36.0,
            },
            sp.feel,
        );
        if feel_res.changed {
            sp.feel = feel_res.value;
        }
        ui.add(egui::Slider::new(&mut sp.humanize, 0.0..=1.0).text("Humanize"));
    } else {
        // Left sub-column: Fill Amount (top-left) + Fill Complexity (bottom-left)
        // Right sub-column: Swing + solid grey 16th pill centered vertically
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(92.0);
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
                            diameter: 36.0,
                        },
                        sp.fill_amount,
                    );
                    if fill_res.changed {
                        sp.fill_amount = fill_res.value;
                    }
                    let lock_ic = if sp.lock_fill_amount {
                        phosphor_egui::regular::LOCK_SIMPLE
                    } else {
                        phosphor_egui::regular::LOCK_SIMPLE_OPEN
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(lock_ic).size(10.5).color(
                                if sp.lock_fill_amount {
                                    ACCENT_YELLOW
                                } else {
                                    TEXT_DIM
                                },
                            ))
                            .fill(Color32::TRANSPARENT),
                        )
                        .on_hover_text("Lock Fill Amount")
                        .clicked()
                    {
                        sp.lock_fill_amount = !sp.lock_fill_amount;
                    }
                });

                ui.add_space(4.0);
                let fc_res = draw_session_knob(
                    ui,
                    DawKnobConfig {
                        id: egui::Id::unique("sp_fill_complexity"),
                        label: "Fill Complexity",
                        help_id: "session_fill_complexity",
                        style: DawKnobStyle::Continuous,
                        min: 0.0,
                        max: 1.0,
                        default: 0.30,
                        log: false,
                        bipolar: false,
                        accent: true,
                        unit: "",
                        diameter: 36.0,
                    },
                    sp.fill_complexity,
                );
                if fc_res.changed {
                    sp.fill_complexity = fc_res.value;
                }
            });

            ui.add_space(6.0);

            ui.vertical(|ui| {
                ui.set_width(88.0);
                ui.add_space(26.0);
                ui.horizontal(|ui| {
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
                            diameter: 36.0,
                        },
                        sp.swing,
                    );
                    if swing_res.changed {
                        sp.swing = swing_res.value;
                    }
                    let lock_ic = if sp.lock_swing {
                        phosphor_egui::regular::LOCK_SIMPLE
                    } else {
                        phosphor_egui::regular::LOCK_SIMPLE_OPEN
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(lock_ic).size(10.5).color(
                                if sp.lock_swing {
                                    ACCENT_YELLOW
                                } else {
                                    TEXT_DIM
                                },
                            ))
                            .fill(Color32::TRANSPARENT),
                        )
                        .on_hover_text("Lock Swing")
                        .clicked()
                    {
                        sp.lock_swing = !sp.lock_swing;
                    }
                });

                ui.add_space(4.0);
                let sub_lbl = match sp.swing_subdiv {
                    SwingSubdiv::Eighth => "8th",
                    SwingSubdiv::Sixteenth => "16th",
                };
                ui.vertical_centered(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(sub_lbl).size(10.5).color(TEXT_MAIN))
                                .fill(Color32::from_rgb(0x3a, 0x3d, 0x45))
                                .corner_radius(CornerRadius::same(4))
                                .min_size(vec2(44.0, 20.0)),
                        )
                        .clicked()
                    {
                        sp.swing_subdiv = match sp.swing_subdiv {
                            SwingSubdiv::Eighth => SwingSubdiv::Sixteenth,
                            SwingSubdiv::Sixteenth => SwingSubdiv::Eighth,
                        };
                    }
                });
            });
        });
    }
}

fn draw_drummer_main_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    ui.add_space(4.0);
    // Row 1: Kit pieces (Hi-Hat, Tambourine, Cowbell) + Pattern 1 (4x4 step dot groups)
    ui.horizontal(|ui| {
        if draw_kit_icon_button(ui, "hihat", sp.kit_hihat, KitPieceIcon::HiHat) {
            sp.kit_hihat = !sp.kit_hihat;
        }
        ui.add_space(4.0);
        if draw_kit_icon_button(
            ui,
            "tambourine",
            sp.kit_tambourine,
            KitPieceIcon::Tambourine,
        ) {
            sp.kit_tambourine = !sp.kit_tambourine;
        }
        ui.add_space(4.0);
        if draw_kit_icon_button(ui, "cowbell", sp.kit_cowbell, KitPieceIcon::Cowbell) {
            sp.kit_cowbell = !sp.kit_cowbell;
        }

        ui.add_space(16.0);
        draw_pattern_badge_and_dots(ui, 1, &mut sp.pattern_idx, sp.complexity, false);
    });

    ui.add_space(14.0);

    // Row 2: Kit pieces (Kick, Snare) + Pattern 2 (two-tier 4x4 step dot matrix)
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        if draw_kit_icon_button(ui, "kick", sp.kit_kick, KitPieceIcon::Kick) {
            sp.kit_kick = !sp.kit_kick;
        }
        ui.add_space(8.0);
        if draw_kit_icon_button(ui, "snare", sp.kit_snare, KitPieceIcon::Snare) {
            sp.kit_snare = !sp.kit_snare;
        }

        ui.add_space(34.0);
        draw_pattern_badge_and_dots(ui, 2, &mut sp.pattern_idx_2, sp.intensity, true);
    });

    ui.add_space(12.0);

    // Directly beneath Row 2's dot matrix: inline Follow: Chord Rhythm (no button border)
    ui.horizontal(|ui| {
        ui.add_space(148.0);
        ui.label(RichText::new("Follow:").size(10.5).color(TEXT_DIM));
        let f_lbl = match sp.follow_rhythm {
            FollowRhythmMode::ChordRhythm => "Chord Rhythm",
            FollowRhythmMode::Track => "Track",
        };
        if ui
            .add(
                egui::Button::new(RichText::new(f_lbl).size(10.5).color(ACCENT_YELLOW))
                    .fill(Color32::TRANSPARENT)
                    .frame(false),
            )
            .clicked()
        {
            sp.follow_rhythm = match sp.follow_rhythm {
                FollowRhythmMode::ChordRhythm => FollowRhythmMode::Track,
                FollowRhythmMode::Track => FollowRhythmMode::ChordRhythm,
            };
        }

        ui.add_space(18.0);
        ui.label(RichText::new("Style:").size(10.0).color(TEXT_DIM));
        egui::ComboBox::from_id_salt("sp_main_drum_style")
            .width(105.0)
            .selected_text(title_case(sp.drum_style.name()))
            .show_ui(ui, |ui| {
                for st in DrumStyle::ALL {
                    ui.selectable_value(&mut sp.drum_style, st, title_case(st.name()));
                }
            });
    });
}

#[derive(Clone, Copy)]
enum KitPieceIcon {
    HiHat,
    Tambourine,
    Cowbell,
    Kick,
    Snare,
}

fn draw_kit_icon_button(ui: &mut Ui, tip: &str, active: bool, icon: KitPieceIcon) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
    let g = ui.painter_at(rect);
    let c = rect.center();
    let col = if active {
        ACCENT_YELLOW
    } else {
        Color32::from_rgb(0x6e, 0x73, 0x7e)
    };
    let st = Stroke::new(1.5, col);
    let thin = Stroke::new(1.1, col);
    match icon {
        KitPieceIcon::HiHat => {
            g.line_segment(
                [pos2(c.x - 13.0, c.y - 3.0), pos2(c.x + 13.0, c.y - 3.0)],
                st,
            );
            g.line_segment(
                [pos2(c.x - 12.0, c.y + 1.0), pos2(c.x + 12.0, c.y + 1.0)],
                st,
            );
            g.line_segment([pos2(c.x, c.y - 8.0), pos2(c.x, c.y + 13.0)], thin);
        }
        KitPieceIcon::Tambourine => {
            g.circle_stroke(c, 11.0, st);
            for i in 0..6 {
                let a = (i as f32 / 6.0) * std::f32::consts::TAU;
                g.circle_filled(c + vec2(a.cos(), a.sin()) * 11.0, 1.8, col);
            }
        }
        KitPieceIcon::Cowbell => {
            g.add(Shape::convex_polygon(
                vec![
                    pos2(c.x - 5.0, c.y - 10.0),
                    pos2(c.x + 5.0, c.y - 10.0),
                    pos2(c.x + 9.0, c.y + 9.0),
                    pos2(c.x - 9.0, c.y + 9.0),
                ],
                Color32::TRANSPARENT,
                st,
            ));
            g.line_segment(
                [pos2(c.x - 2.0, c.y - 13.0), pos2(c.x + 2.0, c.y - 13.0)],
                st,
            );
        }
        KitPieceIcon::Kick => {
            g.circle_stroke(c, 13.0, st);
            g.circle_stroke(c, 9.5, thin);
            g.circle_filled(pos2(c.x + 4.0, c.y + 3.0), 2.2, col);
        }
        KitPieceIcon::Snare => {
            let body = Rect::from_center_size(c, vec2(26.0, 14.0));
            g.rect_stroke(body, 2.0, st, StrokeKind::Inside);
            for i in -1..=1 {
                let x = c.x + i as f32 * 8.0;
                g.line_segment([pos2(x, body.top()), pos2(x, body.bottom())], thin);
            }
        }
    }
    resp.on_hover_text(title_case(tip)).clicked()
}

fn draw_pattern_badge_and_dots(
    ui: &mut Ui,
    badge_num: usize,
    pattern_idx: &mut usize,
    density: f32,
    two_tier: bool,
) {
    let badge = egui::Button::new(
        RichText::new(format!("{badge_num}"))
            .size(10.0)
            .strong()
            .color(Color32::BLACK),
    )
    .fill(ACCENT_YELLOW)
    .corner_radius(CornerRadius::same(10))
    .min_size(vec2(20.0, 20.0));
    if ui.add(badge).clicked() {
        *pattern_idx = (*pattern_idx + 1) % 16;
    }
    ui.add_space(6.0);

    let h = if two_tier { 24.0 } else { 18.0 };
    let (dot_rect, dot_resp) = ui.allocate_exact_size(vec2(216.0, h), Sense::click());
    if dot_resp.clicked() {
        if let Some(pos) = dot_resp.interact_pointer_pos() {
            let idx = (((pos.x - dot_rect.left()) / dot_rect.width()) * 16.0).floor() as usize;
            *pattern_idx = idx.min(15);
        }
    }
    let g = ui.painter_at(dot_rect);
    let group_w = dot_rect.width() / 4.0;
    for grp in 0..4 {
        if grp > 0 {
            let gx = dot_rect.left() + grp as f32 * group_w;
            g.line_segment(
                [
                    pos2(gx, dot_rect.top() + 3.0),
                    pos2(gx, dot_rect.bottom() - 3.0),
                ],
                Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3d, 0x45)),
            );
        }
        for step in 0..4 {
            let idx = grp * 4 + step;
            let cx = dot_rect.left() + grp as f32 * group_w + (step as f32 + 0.5) * (group_w / 4.0);
            let is_sel = idx == *pattern_idx;
            let active = is_sel || (step == 0) || (step == 2 && density > 0.35);
            let col = if active {
                ACCENT_YELLOW
            } else {
                Color32::from_rgb(0x4a, 0x4d, 0x56)
            };
            if two_tier {
                let y_top = dot_rect.center().y - 4.5;
                let y_bot = dot_rect.center().y + 4.5;
                g.circle_filled(pos2(cx, y_top), if is_sel { 3.0 } else { 2.0 }, col);
                let bot_active = is_sel || (step % 2 == 0 && density > 0.25);
                g.circle_filled(
                    pos2(cx, y_bot),
                    if is_sel { 3.0 } else { 2.0 },
                    if bot_active {
                        ACCENT_YELLOW
                    } else {
                        Color32::from_rgb(0x4a, 0x4d, 0x56)
                    },
                );
            } else {
                g.circle_filled(
                    pos2(cx, dot_rect.center().y),
                    if is_sel { 3.4 } else { 2.2 },
                    col,
                );
            }
        }
    }
}

fn draw_main_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    if sp.kind == SessionPlayerKind::Drummer {
        draw_drummer_main_tab(ui, sp);
        return;
    }

    // Pattern dot matrix strip (4x4) + Follow Rhythm (for Keyboard / Bass)
    ui.horizontal(|ui| {
        draw_pattern_badge_and_dots(ui, 1, &mut sp.pattern_idx, sp.complexity, false);
        ui.add_space(8.0);
        ui.label(RichText::new("Follow:").size(10.0).color(TEXT_DIM));
        let f_lbl = match sp.follow_rhythm {
            FollowRhythmMode::ChordRhythm => "Chord Rhythm",
            FollowRhythmMode::Track => "Track",
        };
        if ui
            .add(
                egui::Button::new(RichText::new(f_lbl).size(10.0).color(ACCENT_YELLOW))
                    .fill(Color32::TRANSPARENT)
                    .frame(false),
            )
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

    ui.add_space(6.0);

    match sp.kind {
        SessionPlayerKind::Bass => {
            ui.horizontal_top(|ui| {
                // Square XY Pad (Y: Octave, X: Melody)
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
        SessionPlayerKind::Drummer => {}
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
            ui.label(
                RichText::new("Keyboard Phrasing & Strum")
                    .size(11.0)
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.add(egui::Slider::new(&mut sp.phrasing, 0.0..=1.0).text("Phrasing"));
            ui.add(egui::Slider::new(&mut sp.strum, 0.0..=1.0).text("Strum"));
            ui.checkbox(&mut sp.tie_notes, "Tie Overlapping Chord Notes");
        }
    }
}

fn draw_slides_or_env_tab(ui: &mut Ui, sp: &mut SessionPlayerParams) {
    if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut sp.env_shift_start, 0.0..=0.5).text("Shift Start"));
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
                    let y_norm = if sp.slide_s_curve {
                        let s = t * t * (3.0 - 2.0 * t);
                        if sp.slide_curve >= 0.0 {
                            s.powf(1.0 + sp.slide_curve)
                        } else {
                            1.0 - (1.0 - s).powf(1.0 - sp.slide_curve)
                        }
                    } else {
                        t
                    };
                    pos2(
                        rect.left() + 8.0 + t * (rect.width() - 16.0),
                        rect.bottom() - 10.0 - y_norm * (rect.height() - 20.0),
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
    let (cell, _) = ui.allocate_exact_size(vec2(side + 28.0, side + 32.0), Sense::hover());
    let g = ui.painter().clone();
    daw_knob::draw_custom(
        ui,
        &g,
        cell,
        cfg,
        current_val,
        None,
        DawKnobFinish::StudioMatte,
        Some(ACCENT_YELLOW),
        true,
    )
}
