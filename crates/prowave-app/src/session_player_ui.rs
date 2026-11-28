//! Bottom Session Player Editor (`sessionrefs/Untitled.png`, `session1.png`, `session2.png` layout
//! using the `Untitled.png` / `session1.png` dark + golden-ochre color system).
//!
//! Uses Dori's `daw_knob::draw_custom` (`DawKnobFinish::LogicSession`), `widgets::daw_adsr_envelope`,
//! and `phosphor_egui::regular` vector icons throughout.

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

const BG_DARK: Color32 = Color32::from_rgb(0x18, 0x19, 0x1d);
const TOOLBAR_BG: Color32 = Color32::from_rgb(0x1f, 0x20, 0x25);
const CARD_BG: Color32 = Color32::from_rgb(0x1f, 0x20, 0x25);
const WELL_BG: Color32 = Color32::from_rgb(0x13, 0x14, 0x18);
const PILL_BG: Color32 = Color32::from_rgb(0x2c, 0x2e, 0x35);
const BORDER: Color32 = Color32::from_rgb(0x2c, 0x2e, 0x35);
const ACCENT_YELLOW: Color32 = Color32::from_rgb(0xf5, 0xc5, 0x18);
const REGION_HEADER_BG: Color32 = Color32::from_rgb(0xeb, 0xe3, 0xbd);
const REGION_BODY_BG: Color32 = Color32::from_rgb(0xb5, 0x99, 0x34);
const BLUE_ACTIVE: Color32 = Color32::from_rgb(0x12, 0x69, 0xcc);
const TEXT_MAIN: Color32 = Color32::from_rgb(0xea, 0xec, 0xf0);
const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9f, 0xaa);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorTab {
    Main,
    Details,
    SlidesOrEnv,
    Lfo,
    Manual,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewTool {
    Pointer,
    Velocity,
    Pencil,
    Eraser,
}

impl PreviewTool {
    fn is_pointer_group(self) -> bool {
        matches!(self, Self::Pointer | Self::Velocity)
    }
    fn is_pencil_group(self) -> bool {
        matches!(self, Self::Pencil | Self::Eraser)
    }
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
    fn step_beats(self) -> f32 {
        match self {
            Self::Beat => 1.0,
            Self::Bar => 4.0,
            Self::Sixteenth => 0.25,
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

/// Returns whether the selected track or selection currently has a Session Player / Drummer region.
pub fn has_active_region(p: &Project, s: &UiState) -> bool {
    find_session_or_drummer_region(p, s).is_some()
}

/// Renders the bottom Session Player Editor for the active Session Player or Drummer region.
pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let target = find_session_or_drummer_region(p, s);
    let Some((ti, ri, was_legacy_drummer)) = target else {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("No Session Player region on the selected track.")
                    .size(13.0)
                    .color(TEXT_DIM),
            );
            ui.add_space(6.0);
            if ui.button("Create Keyboard Session Region").clicked() {
                let ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
                if p.chords.is_empty() {
                    p.chords = default_chords();
                }
                p.add_session_region(ti, SessionPlayerParams::default(), 1.0);
            }
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

    // Override slider active fill and selection colors to Session Player Yellow (#f5c518)
    // with dark (#1c1c1e) text on active yellow pills across all tabs.
    {
        let vis = &mut ui.style_mut().visuals;
        vis.selection.bg_fill = ACCENT_YELLOW;
        vis.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0x1c, 0x1c, 0x1e));
        vis.widgets.active.bg_fill = ACCENT_YELLOW;
        vis.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(0x1c, 0x1c, 0x1e));
        vis.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_YELLOW);
    }

    let has_five_tabs = sp.kind == SessionPlayerKind::Bass
        || (sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad());
    if !has_five_tabs && matches!(tab, EditorTab::SlidesOrEnv | EditorTab::Lfo) {
        tab = EditorTab::Main;
    }

    // 1. Top Toolbar
    egui::Frame::new()
        .fill(TOOLBAR_BG)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            let (toolbar_rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
            let center_w = 74.0;
            let left_rect = Rect::from_min_max(
                toolbar_rect.min,
                pos2(
                    (toolbar_rect.center().x - center_w * 0.5 - 8.0).max(toolbar_rect.left()),
                    toolbar_rect.bottom(),
                ),
            );
            let center_rect = Rect::from_center_size(
                toolbar_rect.center(),
                vec2(center_w, toolbar_rect.height()),
            );
            let right_rect = Rect::from_min_max(
                pos2(
                    (toolbar_rect.center().x + center_w * 0.5 + 8.0).min(toolbar_rect.right()),
                    toolbar_rect.top(),
                ),
                toolbar_rect.max,
            );

            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(left_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    // Left: Chords v + Preset: <name> ^v
                    let chords_lbl = format!("Chords  {}", phosphor_egui::regular::CARET_DOWN);
                    let chords_resp = ui.add(
                        egui::Button::new(RichText::new(chords_lbl).size(11.0).color(TEXT_MAIN))
                            .fill(PILL_BG)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(CornerRadius::same(4))
                            .min_size(vec2(72.0, 21.0)),
                    );
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
                    let preset_btn_w = 172.0;
                    let (p_rect, preset_resp) =
                        ui.allocate_exact_size(vec2(preset_btn_w, 21.0), Sense::click());
                    {
                        let g = ui.painter_at(p_rect);
                        g.rect_filled(p_rect, 4.0, Color32::from_rgb(0x28, 0x2a, 0x31));
                        g.rect_stroke(p_rect, 4.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
                        g.text(
                            pos2(p_rect.left() + 8.0, p_rect.center().y),
                            Align2::LEFT_CENTER,
                            &sp.preset_name,
                            FontId::proportional(11.0),
                            TEXT_MAIN,
                        );
                        g.text(
                            pos2(p_rect.right() - 8.0, p_rect.center().y),
                            Align2::RIGHT_CENTER,
                            phosphor_egui::regular::CARET_UP_DOWN,
                            FontId::proportional(11.0),
                            TEXT_DIM,
                        );
                    }
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
                },
            );

            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(center_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    // Centered Segmented Tool Switcher Pill (Pointer / Velocity | Pencil / Eraser)
                    egui::Frame::new()
                        .fill(PILL_BG)
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(1, 1))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.horizontal(|ui| {
                                let ptr_active = tool.is_pointer_group();
                                let ptr_icon = if tool == PreviewTool::Velocity {
                                    phosphor_egui::regular::SLIDERS
                                } else {
                                    phosphor_egui::regular::CURSOR
                                };
                                let ptr_bg = if ptr_active {
                                    Color32::from_rgb(0x3f, 0x42, 0x4b)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(ptr_icon).size(10.5).color(
                                                if ptr_active { Color32::WHITE } else { TEXT_DIM },
                                            ),
                                        )
                                        .fill(ptr_bg)
                                        .corner_radius(CornerRadius::same(3))
                                        .min_size(vec2(22.0, 19.0)),
                                    )
                                    .on_hover_text("Pointer / Scrub Tool")
                                    .clicked()
                                {
                                    tool = PreviewTool::Pointer;
                                }
                                let ptr_menu_resp = ui.add(
                                    egui::Button::new(
                                        RichText::new(phosphor_egui::regular::CARET_DOWN)
                                            .size(9.0)
                                            .color(if ptr_active {
                                                Color32::WHITE
                                            } else {
                                                TEXT_DIM
                                            }),
                                    )
                                    .fill(ptr_bg)
                                    .corner_radius(CornerRadius::same(3))
                                    .min_size(vec2(13.0, 19.0)),
                                );
                                egui::Popup::menu(&ptr_menu_resp).show(|ui| {
                                    if ui
                                        .selectable_label(
                                            tool == PreviewTool::Pointer,
                                            "Pointer (Select / Move / Scrub)",
                                        )
                                        .clicked()
                                    {
                                        tool = PreviewTool::Pointer;
                                        ui.close();
                                    }
                                    if ui
                                        .selectable_label(
                                            tool == PreviewTool::Velocity,
                                            "Velocity (Drag Vertical)",
                                        )
                                        .clicked()
                                    {
                                        tool = PreviewTool::Velocity;
                                        ui.close();
                                    }
                                });

                                let (div, _) =
                                    ui.allocate_exact_size(vec2(1.0, 16.0), Sense::hover());
                                ui.painter().rect_filled(
                                    div,
                                    0.0,
                                    Color32::from_rgb(0x1b, 0x1c, 0x20),
                                );

                                let pen_active = tool.is_pencil_group();
                                let pen_icon = if tool == PreviewTool::Eraser {
                                    phosphor_egui::regular::ERASER
                                } else {
                                    phosphor_egui::regular::PENCIL_SIMPLE
                                };
                                let pen_bg = if pen_active {
                                    Color32::from_rgb(0x3f, 0x42, 0x4b)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(pen_icon).size(10.5).color(
                                                if pen_active { Color32::WHITE } else { TEXT_DIM },
                                            ),
                                        )
                                        .fill(pen_bg)
                                        .corner_radius(CornerRadius::same(3))
                                        .min_size(vec2(22.0, 19.0)),
                                    )
                                    .on_hover_text("Pencil Tool (Add Notes)")
                                    .clicked()
                                {
                                    tool = PreviewTool::Pencil;
                                }
                                let pen_menu_resp = ui.add(
                                    egui::Button::new(
                                        RichText::new(phosphor_egui::regular::CARET_DOWN)
                                            .size(9.0)
                                            .color(if pen_active {
                                                Color32::WHITE
                                            } else {
                                                TEXT_DIM
                                            }),
                                    )
                                    .fill(pen_bg)
                                    .corner_radius(CornerRadius::same(3))
                                    .min_size(vec2(13.0, 19.0)),
                                );
                                egui::Popup::menu(&pen_menu_resp).show(|ui| {
                                    if ui
                                        .selectable_label(
                                            tool == PreviewTool::Pencil,
                                            "Pencil (Add Notes)",
                                        )
                                        .clicked()
                                    {
                                        tool = PreviewTool::Pencil;
                                        ui.close();
                                    }
                                    if ui
                                        .selectable_label(
                                            tool == PreviewTool::Eraser,
                                            "Eraser (Remove Notes)",
                                        )
                                        .clicked()
                                    {
                                        tool = PreviewTool::Eraser;
                                        ui.close();
                                    }
                                });
                            });
                        });
                },
            );

            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(right_rect)
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    // Right: Snap: [Magnet] [Beat ^v] [|↔|] [↔ ===●==]
                    draw_white_thumb_zoom_slider(ui, &mut zoom);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(phosphor_egui::regular::ARROWS_LEFT_RIGHT)
                            .size(11.0)
                            .color(TEXT_DIM),
                    );
                    ui.add_space(5.0);

                    let catch_btn = egui::Button::new(
                        RichText::new(phosphor_egui::regular::ARROWS_HORIZONTAL)
                            .size(11.0)
                            .color(if catch_on { Color32::WHITE } else { TEXT_DIM }),
                    )
                    .fill(if catch_on { BLUE_ACTIVE } else { PILL_BG })
                    .corner_radius(CornerRadius::same(4))
                    .min_size(vec2(24.0, 20.0));
                    if ui.add(catch_btn).on_hover_text("Catch Playhead").clicked() {
                        catch_on = !catch_on;
                    }

                    ui.add_space(5.0);

                    let (s_rect, snap_mode_resp) =
                        ui.allocate_exact_size(vec2(78.0, 20.0), Sense::click());
                    {
                        let g = ui.painter_at(s_rect);
                        g.rect_filled(s_rect, 4.0, PILL_BG);
                        g.rect_stroke(s_rect, 4.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
                        g.text(
                            pos2(s_rect.left() + 8.0, s_rect.center().y),
                            Align2::LEFT_CENTER,
                            snap_mode.label(),
                            FontId::proportional(11.0),
                            TEXT_MAIN,
                        );
                        g.text(
                            pos2(s_rect.right() - 7.0, s_rect.center().y),
                            Align2::RIGHT_CENTER,
                            phosphor_egui::regular::CARET_UP_DOWN,
                            FontId::proportional(10.5),
                            TEXT_DIM,
                        );
                    }
                    egui::Popup::menu(&snap_mode_resp).show(|ui| {
                        for m in SnapMode::ALL {
                            if ui.selectable_label(snap_mode == m, m.label()).clicked() {
                                snap_mode = m;
                                ui.close();
                            }
                        }
                    });

                    ui.add_space(4.0);
                    let snap_btn = egui::Button::new(
                        RichText::new(phosphor_egui::regular::MAGNET)
                            .size(11.0)
                            .color(if snap_on { Color32::WHITE } else { TEXT_DIM }),
                    )
                    .fill(if snap_on { BLUE_ACTIVE } else { PILL_BG })
                    .corner_radius(CornerRadius::same(4))
                    .min_size(vec2(22.0, 20.0));
                    if ui.add(snap_btn).on_hover_text("Toggle Snap").clicked() {
                        snap_on = !snap_on;
                    }
                    ui.label(RichText::new("Snap:").size(11.0).color(TEXT_DIM));
                },
            );
        });

    // 2. Bar Ruler + Zoomed Region Strip (interactive Pointer / Velocity / Pencil / Eraser + Playhead)
    let (strip_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::hover());
    draw_bar_ruler_and_strip(
        ui, strip_rect, p, ti, ri, &sp, tool, snap_on, snap_mode, zoom,
    );

    ui.add_space(8.0);

    // 3. Scrollable Centered Main Workspace (prevents bottom panel from snapping back on resize)
    egui::ScrollArea::vertical()
        .id_salt("session_player_body_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let workspace_w = (ui.available_width() - 32.0).clamp(720.0, 1140.0);
            let left_pad = ((ui.available_width() - workspace_w) * 0.5).max(12.0);
            let third_label =
                if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
                    "Envelope"
                } else {
                    "Slides"
                };

            ui.horizontal_top(|ui| {
                ui.add_space(left_pad);

                // Left Column: Player Identity + Wireframe Disc + Regenerate + Complexity/Intensity Faders
                ui.vertical(|ui| {
                    ui.set_width(244.0);
                    ui.add_space(14.0);
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(118.0);
                            let (role_lbl, style_name) = match sp.kind {
                                SessionPlayerKind::Keyboard => {
                                    ("Keyboard Player", title_case(sp.keyboard_style.label()))
                                }
                                SessionPlayerKind::Bass => {
                                    ("Bass Player", title_case(sp.bass_style.label()))
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
                                let style_resp = ui.add(
                                    egui::Button::new(
                                        RichText::new(style_name)
                                            .size(11.5)
                                            .strong()
                                            .color(ACCENT_YELLOW),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .frame(false),
                                );
                                egui::Popup::menu(&style_resp).show(|ui| {
                                    ui.label(RichText::new("Drummer").size(10.0).color(TEXT_DIM));
                                    for st in DrumStyle::ALL {
                                        let sel = sp.kind == SessionPlayerKind::Drummer
                                            && sp.drum_style == st;
                                        if ui.selectable_label(sel, title_case(st.name())).clicked()
                                        {
                                            sp.kind = SessionPlayerKind::Drummer;
                                            sp.drum_style = st;
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    ui.label(
                                        RichText::new("Keyboard Player").size(10.0).color(TEXT_DIM),
                                    );
                                    for ks in KeyboardStyle::ALL {
                                        let sel = sp.kind == SessionPlayerKind::Keyboard
                                            && sp.keyboard_style == ks;
                                        if ui.selectable_label(sel, ks.label()).clicked() {
                                            sp.kind = SessionPlayerKind::Keyboard;
                                            sp.keyboard_style = ks;
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    ui.label(
                                        RichText::new("Bass Player").size(10.0).color(TEXT_DIM),
                                    );
                                    for bs in BassStyle::ALL {
                                        let sel = sp.kind == SessionPlayerKind::Bass
                                            && sp.bass_style == bs;
                                        if ui.selectable_label(sel, bs.label()).clicked() {
                                            sp.kind = SessionPlayerKind::Bass;
                                            sp.bass_style = bs;
                                            ui.close();
                                        }
                                    }
                                });

                                ui.add_space(4.0);
                                let (disc_rect, _) =
                                    ui.allocate_exact_size(vec2(96.0, 96.0), Sense::hover());
                                draw_player_disc(ui, disc_rect, sp.kind);
                                ui.add_space(6.0);
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("Regenerate").size(11.0).color(TEXT_MAIN),
                                        )
                                        .fill(Color32::from_rgb(0x3a, 0x3c, 0x43))
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

                ui.add_space(12.0);

                // Right Workspace: Compact Borderless Segmented Tab Bar + Rounded Card
                let card_w = (workspace_w - 256.0).max(460.0);
                ui.vertical(|ui| {
                    ui.set_width(card_w);

                    let tabs: Vec<(EditorTab, &str)> = if has_five_tabs {
                        vec![
                            (EditorTab::Main, "Main"),
                            (EditorTab::Details, "Details"),
                            (EditorTab::SlidesOrEnv, third_label),
                            (EditorTab::Lfo, "LFO"),
                            (EditorTab::Manual, "Manual"),
                        ]
                    } else {
                        vec![
                            (EditorTab::Main, "Main"),
                            (EditorTab::Details, "Details"),
                            (EditorTab::Manual, "Manual"),
                        ]
                    };

                    // Right-Aligned Segmented Tab Bar flush with card right edge
                    ui.allocate_ui_with_layout(
                        vec2(card_w, 24.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(0x13, 0x14, 0x18))
                                .corner_radius(CornerRadius::same(5))
                                .inner_margin(egui::Margin::symmetric(2, 2))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    for (t, lbl) in tabs.iter().rev() {
                                        let active = tab == *t;
                                        let btn = egui::Button::new(
                                            RichText::new(*lbl).size(11.0).strong().color(
                                                if active {
                                                    Color32::from_rgb(0x1c, 0x1c, 0x1e)
                                                } else {
                                                    TEXT_DIM
                                                },
                                            ),
                                        )
                                        .fill(if active {
                                            ACCENT_YELLOW
                                        } else {
                                            Color32::from_rgb(0x26, 0x26, 0x2a)
                                        })
                                        .corner_radius(CornerRadius::same(4))
                                        .min_size(vec2(0.0, 20.0));
                                        if ui.add(btn).clicked() {
                                            tab = *t;
                                        }
                                    }
                                });
                        },
                    );

                    ui.add_space(6.0);

                    egui::Frame::new()
                        .fill(CARD_BG)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(0x29, 0x2b, 0x32)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(16, 14))
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                let right_w = 182.0;
                                let center_w = (card_w - right_w - 46.0).max(250.0);
                                ui.vertical(|ui| {
                                    ui.set_width(center_w);
                                    match tab {
                                        EditorTab::Main => draw_main_tab(ui, &mut sp, center_w),
                                        EditorTab::Details => draw_details_tab(ui, &mut sp),
                                        EditorTab::SlidesOrEnv => {
                                            draw_slides_or_env_tab(ui, &mut sp, center_w)
                                        }
                                        EditorTab::Lfo => draw_lfo_tab(ui, &mut sp, center_w),
                                        EditorTab::Manual => draw_manual_tab(ui, &mut sp, center_w),
                                    }
                                });

                                ui.add_space(12.0);

                                ui.vertical(|ui| {
                                    ui.set_width(right_w);
                                    draw_right_column(ui, &mut sp, tab);
                                });
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
    let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 18.0), Sense::click_and_drag());
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
    let norm = ((*zoom - 0.5) / 1.5).clamp(0.0, 1.0);
    let tx = x0 + norm * (x1 - x0);
    g.line_segment(
        [pos2(x0, cy), pos2(x1, cy)],
        Stroke::new(3.0, Color32::from_rgb(0x2e, 0x30, 0x37)),
    );
    g.line_segment(
        [pos2(x0, cy), pos2(tx, cy)],
        Stroke::new(3.0, ACCENT_YELLOW),
    );
    g.circle_filled(pos2(tx, cy), 4.8, Color32::WHITE);
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

#[allow(clippy::too_many_arguments)]
fn draw_bar_ruler_and_strip(
    ui: &Ui,
    rect: Rect,
    p: &mut Project,
    ti: usize,
    ri: usize,
    sp: &SessionPlayerParams,
    tool: PreviewTool,
    snap_on: bool,
    snap_mode: SnapMode,
    zoom: f32,
) {
    let ruler_h = 18.0;
    let header_h = 16.0;
    let ruler = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + ruler_h));
    let header = Rect::from_min_max(
        pos2(rect.left(), ruler.bottom()),
        pos2(rect.right(), ruler.bottom() + header_h),
    );
    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);

    let total_beats = ((p.tracks[ti].regions[ri].len_bars * 4.0) / zoom.max(0.5)).max(4.0);
    let reg_start_beats = f64::from(p.tracks[ti].regions[ri].start_bar - 1.0) * 4.0;
    let quantize = |b: f32| -> f32 {
        if snap_on {
            let s = snap_mode.step_beats();
            ((b / s).round() * s).max(0.0)
        } else {
            b.max(0.0)
        }
    };

    // Ruler interaction: scrub playhead
    let ruler_resp = ui.interact(
        ruler,
        egui::Id::unique("sp_ruler_scrub"),
        Sense::click_and_drag(),
    );
    if ruler_resp.drag_started() {
        crate::tracks_area::begin_playhead_drag(p);
    }
    if (ruler_resp.clicked() || ruler_resp.dragged()) && ruler_resp.interact_pointer_pos().is_some()
    {
        if let Some(pt) = ruler_resp.interact_pointer_pos() {
            let rel_b = ((pt.x - ruler.left()) / ruler.width()).clamp(0.0, 1.0) * total_beats;
            let target = reg_start_beats + f64::from(quantize(rel_b));
            if ruler_resp.dragged() {
                crate::tracks_area::update_playhead_drag(p, target);
            } else {
                p.position_beats = target;
            }
        }
    }
    if ruler_resp.drag_stopped() {
        crate::tracks_area::end_playhead_drag(p);
    }

    // Header interaction: click framed play button to jump playhead to region start
    let header_resp = ui.interact(header, egui::Id::unique("sp_header_play"), Sense::click());
    if header_resp.clicked() {
        p.position_beats = reg_start_beats;
    }

    // Region body interaction: Pointer / Velocity / Pencil / Eraser
    let body_resp = ui.interact(
        body,
        egui::Id::unique("sp_body_edit"),
        Sense::click_and_drag(),
    );
    if body_resp.hovered() || body_resp.dragged() {
        ui.ctx().set_cursor_icon(match tool {
            PreviewTool::Pointer => egui::CursorIcon::Default,
            PreviewTool::Velocity => egui::CursorIcon::ResizeVertical,
            PreviewTool::Pencil => egui::CursorIcon::Crosshair,
            PreviewTool::Eraser => egui::CursorIcon::NotAllowed,
        });
    }
    if (body_resp.clicked() || body_resp.dragged()) && body_resp.interact_pointer_pos().is_some() {
        if let Some(pt) = body_resp.interact_pointer_pos() {
            let rel_b = ((pt.x - body.left()) / body.width()).clamp(0.0, 1.0) * total_beats;
            let q_b = quantize(rel_b);
            let y_frac = (1.0 - (pt.y - body.top()) / body.height()).clamp(0.0, 1.0);
            let notes = &mut p.tracks[ti].regions[ri].notes;
            match tool {
                PreviewTool::Pointer => {
                    if body_resp.dragged() {
                        if let Some((idx, _)) = notes
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                (a.start - rel_b).abs().total_cmp(&(b.start - rel_b).abs())
                            })
                            .filter(|(_, n)| (n.start - rel_b).abs() < 0.5)
                        {
                            notes[idx].start = q_b;
                        }
                    } else if body_resp.clicked() {
                        p.position_beats = reg_start_beats + f64::from(q_b);
                    }
                }
                PreviewTool::Velocity => {
                    if let Some((idx, _)) = notes
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            (a.start - rel_b).abs().total_cmp(&(b.start - rel_b).abs())
                        })
                        .filter(|(_, n)| (n.start - rel_b).abs() < 0.6)
                    {
                        notes[idx].vel = (20.0 + y_frac * 107.0).round().clamp(20.0, 127.0) as u8;
                    }
                }
                PreviewTool::Pencil => {
                    let pitch = if sp.kind == SessionPlayerKind::Drummer {
                        if y_frac > 0.55 {
                            42 // Closed Hi-Hat
                        } else if y_frac > 0.28 {
                            38 // Snare
                        } else {
                            36 // Kick
                        }
                    } else {
                        (28.0 + y_frac * 60.0).round().clamp(24.0, 96.0) as u8
                    };
                    let exists = notes
                        .iter()
                        .any(|n| (n.start - q_b).abs() < 0.05 && n.pitch == pitch);
                    if !exists {
                        notes.push(crate::model::Note {
                            start: q_b,
                            len: snap_mode.step_beats().min(0.5),
                            pitch,
                            vel: 100,
                            mute: false,
                            lyric: Default::default(),
                        });
                    }
                }
                PreviewTool::Eraser => {
                    notes.retain(|n| (n.start - rel_b).abs() > 0.25);
                }
            }
        }
    }

    let region = &p.tracks[ti].regions[ri];
    let g = ui.painter_at(rect);

    g.rect_filled(ruler, 0.0, BG_DARK);
    g.rect_filled(header, 0.0, REGION_HEADER_BG);
    g.rect_filled(body, 0.0, REGION_BODY_BG);

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
                Color32::from_rgb(0x88, 0x8d, 0x98)
            },
        );
    }

    // Fine beat/16th tick marks along bottom edge of header strip
    let total_sixteenths = (total_beats * 4.0).round() as usize;
    for s in 0..total_sixteenths {
        let x = header.left() + (s as f32 / total_sixteenths.max(1) as f32) * header.width();
        let h_tick = if s % 4 == 0 { 5.0 } else { 3.0 };
        g.line_segment(
            [pos2(x, header.bottom() - h_tick), pos2(x, header.bottom())],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(60, 52, 28, 70)),
        );
    }

    // Framed play icon + repeating region name across the cream header strip
    let icon_box = Rect::from_center_size(
        pos2(header.left() + 12.0, header.center().y),
        vec2(10.0, 10.0),
    );
    g.rect_stroke(
        icon_box,
        2.0,
        Stroke::new(1.0, Color32::from_rgb(0x52, 0x48, 0x2a)),
        StrokeKind::Inside,
    );
    let tri_c = icon_box.center();
    g.add(Shape::convex_polygon(
        vec![
            pos2(tri_c.x - 1.8, tri_c.y - 2.3),
            pos2(tri_c.x + 2.3, tri_c.y),
            pos2(tri_c.x - 1.8, tri_c.y + 2.3),
        ],
        Color32::from_rgb(0x3a, 0x33, 0x1e),
        Stroke::NONE,
    ));

    let (role_title, style_lbl) = match sp.kind {
        SessionPlayerKind::Keyboard => ("Keyboard Player", title_case(sp.keyboard_style.label())),
        SessionPlayerKind::Bass => ("Bass Player", title_case(sp.bass_style.label())),
        SessionPlayerKind::Drummer => ("Drummer", title_case(sp.drum_style.name())),
    };
    let region_title = format!("{role_title} - {style_lbl}");
    let repeat_px = 210.0_f32;
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
        // Two-lane high-contrast triangular transient wedges (Untitled.png)
        let mid_y = body.top() + body.height() * 0.45;
        for n in &region.notes {
            let x0 = body.left() + (n.start / total_beats) * body.width();
            if x0 > body.right() {
                continue;
            }
            let vel_norm = (f32::from(n.vel) / 127.0).clamp(0.3, 1.0);
            let is_top_lane = n.pitch >= 42;
            let (cy, half_h, w) = if is_top_lane {
                (
                    body.top() + (mid_y - body.top()) * 0.5,
                    (mid_y - body.top()) * 0.24 * vel_norm,
                    18.0,
                )
            } else {
                (
                    mid_y + (body.bottom() - mid_y) * 0.52,
                    (body.bottom() - mid_y) * 0.38 * vel_norm,
                    16.0,
                )
            };
            let col = Color32::from_rgba_unmultiplied(255, 253, 240, (180 + n.vel / 2).min(255));
            g.add(Shape::convex_polygon(
                vec![
                    pos2(x0, cy - half_h),
                    pos2(x0 + w, cy),
                    pos2(x0, cy + half_h),
                ],
                col,
                Stroke::NONE,
            ));
        }
    } else {
        // Horizontal note bars with crisp left transient ticks (session1.png)
        for n in &region.notes {
            let x0 = body.left() + (n.start / total_beats) * body.width();
            let x1 = (x0 + (n.len / total_beats) * body.width()).max(x0 + 5.0);
            if x0 > body.right() {
                continue;
            }
            let pitch_norm = ((f32::from(n.pitch) - 28.0) / 60.0).clamp(0.10, 0.90);
            let y = body.bottom() - pitch_norm * body.height();
            let col = Color32::from_rgba_unmultiplied(255, 252, 232, (165 + n.vel / 2).min(255));
            g.line_segment(
                [pos2(x0, y - 2.5), pos2(x0, y + 2.5)],
                Stroke::new(1.8, col),
            );
            g.line_segment(
                [pos2(x0, y), pos2(x1.min(body.right()), y)],
                Stroke::new(1.5, col),
            );
        }

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

    // Live playhead line across ruler + header + body
    let rel_play = (p.position_beats - reg_start_beats) as f32;
    if rel_play >= 0.0 && rel_play <= total_beats {
        let px = rect.left() + (rel_play / total_beats) * rect.width();
        g.line_segment(
            [pos2(px, ruler.top()), pos2(px, body.bottom())],
            Stroke::new(1.4, Color32::WHITE),
        );
        g.add(Shape::convex_polygon(
            vec![
                pos2(px - 4.0, ruler.top()),
                pos2(px + 4.0, ruler.top()),
                pos2(px, ruler.top() + 6.0),
            ],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }
}

fn draw_player_disc(ui: &Ui, rect: Rect, kind: SessionPlayerKind) {
    let g = ui.painter_at(rect);
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.48;
    g.circle_filled(c, r, Color32::from_rgb(0x11, 0x12, 0x15));
    g.circle_filled(
        c,
        r * 0.85,
        Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 14),
    );
    g.circle_filled(
        c,
        r * 0.56,
        Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 24),
    );
    g.circle_stroke(c, r, Stroke::new(1.0, BORDER));

    let st = Stroke::new(1.2, ACCENT_YELLOW);
    let dim_st = Stroke::new(0.95, Color32::from_rgba_unmultiplied(0xea, 0xc3, 0x2a, 150));
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
            let kick_rect = Rect::from_center_size(c + vec2(0.0, 12.0), vec2(22.0, 16.0));
            g.rect_stroke(kick_rect, 4.0, st, StrokeKind::Inside);
            g.rect_stroke(
                Rect::from_center_size(c + vec2(0.0, 23.0), vec2(7.0, 8.0)),
                2.0,
                dim_st,
                StrokeKind::Inside,
            );
            let snare_c = c + vec2(-15.0, 8.0);
            g.circle_stroke(snare_c, 9.5, st);
            g.circle_stroke(snare_c, 7.2, dim_st);
            g.circle_stroke(c + vec2(-9.0, -5.0), 7.0, st);
            g.circle_stroke(c + vec2(9.0, -5.0), 7.5, st);
            g.circle_stroke(c + vec2(17.0, 11.0), 9.5, st);
            g.circle_stroke(c + vec2(17.0, 11.0), 7.2, dim_st);
            let hh_c = c + vec2(-25.0, -2.0);
            g.circle_stroke(hh_c, 7.5, st);
            g.circle_filled(hh_c, 1.5, ACCENT_YELLOW);
            let crash_c = c + vec2(-20.0, -18.0);
            g.circle_stroke(crash_c, 10.5, st);
            g.circle_stroke(crash_c, 2.8, dim_st);
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
            ui.label(RichText::new(label).size(10.0).color(TEXT_DIM));
            ui.add_space(4.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 122.0), Sense::click_and_drag());
            if (resp.dragged() || resp.clicked()) && resp.interact_pointer_pos().is_some() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    *val = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
                }
            }
            let g = ui.painter_at(rect);
            let x = rect.center().x;
            let track = Rect::from_min_max(
                pos2(x - 2.5, rect.top() + 5.0),
                pos2(x + 2.5, rect.bottom() - 5.0),
            );
            g.rect_filled(track, 2.5, Color32::from_rgb(0x11, 0x12, 0x15));
            let thumb_y = track.bottom() - *val * track.height();
            let fill = Rect::from_min_max(pos2(x - 2.0, thumb_y), pos2(x + 2.0, track.bottom()));
            g.rect_filled(fill, 2.0, ACCENT_YELLOW);
            let thumb = Rect::from_center_size(pos2(x, thumb_y), vec2(22.0, 10.0));
            g.rect_filled(thumb, 4.5, Color32::from_rgb(0x5e, 0x62, 0x6c));
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
                Stroke::new(1.5, ACCENT_YELLOW),
            );
        });
    });
}

fn draw_labeled_horizontal_slider(ui: &mut Ui, label: &str, val: &mut f32) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(label).size(10.5).color(TEXT_DIM)).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{:.2}", *val))
                        .size(10.0)
                        .color(TEXT_MAIN),
                )
                .truncate(),
            );
            ui.add(
                egui::Slider::new(val, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            );
        });
    });
}

fn draw_right_column(ui: &mut Ui, sp: &mut SessionPlayerParams, tab: EditorTab) {
    if tab == EditorTab::Details {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("Phrase Length")
                    .size(11.0)
                    .color(Color32::from_rgb(0x9e, 0x9e, 0xa6)),
            );
            ui.add_space(2.0);
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = 24.0;
                egui::ComboBox::from_id_salt("sp_phrase_len")
                    .height(24.0)
                    .selected_text(sp.phrase_length.label())
                    .show_ui(ui, |ui| {
                        for pl in PhraseLength::ALL {
                            ui.selectable_value(&mut sp.phrase_length, pl, pl.label());
                        }
                    });
            });
            ui.add_space(10.0);
            draw_labeled_horizontal_slider(ui, "Chord Resp.", &mut sp.chord_response);
            ui.add_space(10.0);
            ui.label(
                RichText::new("Tempo: Automatic")
                    .size(11.5)
                    .color(Color32::from_rgb(0xb8, 0xb8, 0xc0)),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Feel").size(11.0).color(TEXT_DIM));
            let feel_res = draw_session_knob(
                ui,
                DawKnobConfig {
                    id: egui::Id::unique("sp_feel_knob"),
                    label: "",
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
            ui.add_space(10.0);
            draw_labeled_horizontal_slider(ui, "Humanize", &mut sp.humanize);
        });
    } else {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(92.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Fill Amount").size(11.0).color(TEXT_MAIN));
                    let lock_ic = if sp.lock_fill_amount {
                        phosphor_egui::regular::LOCK_SIMPLE
                    } else {
                        phosphor_egui::regular::LOCK_SIMPLE_OPEN
                    };
                    let lock_col = if sp.lock_fill_amount {
                        Color32::from_rgb(0xf5, 0xc5, 0x18)
                    } else {
                        TEXT_DIM
                    };
                    let resp = ui.add(
                        egui::Label::new(RichText::new(lock_ic).size(11.0).color(lock_col))
                            .sense(Sense::click()),
                    );
                    if resp.on_hover_text("Lock Fill Amount").clicked() {
                        sp.lock_fill_amount = !sp.lock_fill_amount;
                    }
                });

                let fill_res = draw_session_knob(
                    ui,
                    DawKnobConfig {
                        id: egui::Id::unique("sp_fill_amount"),
                        label: "",
                        help_id: "session_fill_amount",
                        style: DawKnobStyle::Continuous,
                        min: 0.0,
                        max: 1.0,
                        default: 0.35,
                        log: false,
                        bipolar: false,
                        accent: true,
                        unit: "%",
                        diameter: 36.0,
                    },
                    sp.fill_amount,
                );
                if fill_res.changed {
                    sp.fill_amount = fill_res.value;
                }

                ui.add_space(4.0);
                ui.label(RichText::new("Fill Complexity").size(11.0).color(TEXT_MAIN));
                let fc_res = draw_session_knob(
                    ui,
                    DawKnobConfig {
                        id: egui::Id::unique("sp_fill_complexity"),
                        label: "",
                        help_id: "session_fill_complexity",
                        style: DawKnobStyle::Continuous,
                        min: 0.0,
                        max: 1.0,
                        default: 0.30,
                        log: false,
                        bipolar: false,
                        accent: true,
                        unit: "%",
                        diameter: 36.0,
                    },
                    sp.fill_complexity,
                );
                if fc_res.changed {
                    sp.fill_complexity = fc_res.value;
                }
            });

            ui.add_space(4.0);

            ui.vertical(|ui| {
                ui.set_width(86.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Swing").size(11.0).color(TEXT_MAIN));
                    let lock_ic = if sp.lock_swing {
                        phosphor_egui::regular::LOCK_SIMPLE
                    } else {
                        phosphor_egui::regular::LOCK_SIMPLE_OPEN
                    };
                    let lock_col = if sp.lock_swing {
                        Color32::from_rgb(0xf5, 0xc5, 0x18)
                    } else {
                        TEXT_DIM
                    };
                    let resp = ui.add(
                        egui::Label::new(RichText::new(lock_ic).size(11.0).color(lock_col))
                            .sense(Sense::click()),
                    );
                    if resp.on_hover_text("Lock Swing").clicked() {
                        sp.lock_swing = !sp.lock_swing;
                    }
                });

                let swing_res = draw_session_knob(
                    ui,
                    DawKnobConfig {
                        id: egui::Id::unique("sp_swing_knob"),
                        label: "",
                        help_id: "session_swing",
                        style: DawKnobStyle::Continuous,
                        min: 0.0,
                        max: 1.0,
                        default: 0.08,
                        log: false,
                        bipolar: false,
                        accent: true,
                        unit: "%",
                        diameter: 36.0,
                    },
                    sp.swing,
                );
                if swing_res.changed {
                    sp.swing = swing_res.value;
                }

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

fn draw_drummer_main_tab(ui: &mut Ui, sp: &mut SessionPlayerParams, center_w: f32) {
    ui.add_space(6.0);
    // Row 1: Hi-Hat, Tambourine, Cowbell + [1] + 4x4 step dots
    ui.horizontal(|ui| {
        if draw_kit_icon_button(ui, "hihat", sp.kit_hihat, KitPieceIcon::HiHat) {
            sp.kit_hihat = !sp.kit_hihat;
        }
        ui.add_space(2.0);
        if draw_kit_icon_button(
            ui,
            "tambourine",
            sp.kit_tambourine,
            KitPieceIcon::Tambourine,
        ) {
            sp.kit_tambourine = !sp.kit_tambourine;
        }
        ui.add_space(2.0);
        if draw_kit_icon_button(ui, "cowbell", sp.kit_cowbell, KitPieceIcon::Cowbell) {
            sp.kit_cowbell = !sp.kit_cowbell;
        }

        ui.add_space(14.0);
        let dots_w = (center_w - 170.0).max(120.0);
        draw_pattern_badge_and_dots(ui, 1, &mut sp.pattern_idx, sp.complexity, false, dots_w);
    });

    ui.horizontal(|ui| {
        ui.add_space(138.0);
        draw_inline_follow(ui, sp);
    });

    ui.add_space(6.0);
    let (div_r, _) = ui.allocate_exact_size(vec2(center_w, 1.0), Sense::hover());
    ui.painter()
        .rect_filled(div_r, 0.0, Color32::from_rgb(0x2b, 0x2d, 0x34));
    ui.add_space(6.0);

    // Row 2: Kick, Snare + [2] + two-tier 4x4 step dots
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        if draw_kit_icon_button(ui, "kick", sp.kit_kick, KitPieceIcon::Kick) {
            sp.kit_kick = !sp.kit_kick;
        }
        ui.add_space(8.0);
        if draw_kit_icon_button(ui, "snare", sp.kit_snare, KitPieceIcon::Snare) {
            sp.kit_snare = !sp.kit_snare;
        }

        ui.add_space(34.0);
        let dots_w = (center_w - 170.0).max(120.0);
        draw_pattern_badge_and_dots(ui, 2, &mut sp.pattern_idx_2, sp.intensity, true, dots_w);
    });

    ui.add_space(8.0);

    // Directly beneath Row 2's dot matrix: inline Follow: Chord Rhythm
    ui.horizontal(|ui| {
        ui.add_space(138.0);
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
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let g = ui.painter_at(rect);
    let c = rect.center();
    let col = if active {
        ACCENT_YELLOW
    } else {
        Color32::from_rgb(0x6e, 0x73, 0x7e)
    };
    let st = Stroke::new(1.4, col);
    let thin = Stroke::new(1.0, col);
    match icon {
        KitPieceIcon::HiHat => {
            g.line_segment(
                [pos2(c.x - 12.0, c.y - 3.0), pos2(c.x + 12.0, c.y - 3.0)],
                st,
            );
            g.line_segment(
                [pos2(c.x - 11.0, c.y + 1.0), pos2(c.x + 11.0, c.y + 1.0)],
                st,
            );
            g.line_segment([pos2(c.x, c.y - 7.0), pos2(c.x, c.y + 11.0)], thin);
        }
        KitPieceIcon::Tambourine => {
            g.circle_stroke(c, 9.5, st);
            for i in 0..6 {
                let a = (i as f32 / 6.0) * std::f32::consts::TAU;
                g.circle_filled(c + vec2(a.cos(), a.sin()) * 9.5, 1.6, col);
            }
        }
        KitPieceIcon::Cowbell => {
            g.add(Shape::convex_polygon(
                vec![
                    pos2(c.x - 4.5, c.y - 9.0),
                    pos2(c.x + 4.5, c.y - 9.0),
                    pos2(c.x + 8.0, c.y + 8.0),
                    pos2(c.x - 8.0, c.y + 8.0),
                ],
                Color32::TRANSPARENT,
                st,
            ));
            g.line_segment(
                [pos2(c.x - 2.0, c.y - 12.0), pos2(c.x + 2.0, c.y - 12.0)],
                st,
            );
        }
        KitPieceIcon::Kick => {
            g.circle_stroke(c, 12.0, st);
            g.circle_stroke(c, 8.5, thin);
            g.circle_filled(pos2(c.x + 3.5, c.y + 2.5), 2.0, col);
        }
        KitPieceIcon::Snare => {
            let body = Rect::from_center_size(c, vec2(24.0, 12.0));
            g.rect_stroke(body, 2.0, st, StrokeKind::Inside);
            for i in -1..=1 {
                let x = c.x + i as f32 * 7.0;
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
    dots_w: f32,
) {
    let badge = egui::Button::new(
        RichText::new(format!("{badge_num}"))
            .size(10.0)
            .strong()
            .color(Color32::BLACK),
    )
    .fill(ACCENT_YELLOW)
    .corner_radius(CornerRadius::same(10))
    .min_size(vec2(19.0, 19.0));
    if ui.add(badge).clicked() {
        *pattern_idx = (*pattern_idx + 1) % 16;
    }
    ui.add_space(6.0);

    let h = if two_tier { 24.0 } else { 18.0 };
    let (dot_rect, dot_resp) = ui.allocate_exact_size(vec2(dots_w, h), Sense::click());
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
                g.circle_filled(pos2(cx, y_top), if is_sel { 2.8 } else { 1.9 }, col);
                let bot_active = is_sel || (step % 2 == 0 && density > 0.25);
                g.circle_filled(
                    pos2(cx, y_bot),
                    if is_sel { 2.8 } else { 1.9 },
                    if bot_active {
                        ACCENT_YELLOW
                    } else {
                        Color32::from_rgb(0x4a, 0x4d, 0x56)
                    },
                );
            } else {
                g.circle_filled(
                    pos2(cx, dot_rect.center().y),
                    if is_sel { 3.2 } else { 2.1 },
                    col,
                );
            }
        }
    }
}

fn draw_main_tab(ui: &mut Ui, sp: &mut SessionPlayerParams, center_w: f32) {
    if sp.kind == SessionPlayerKind::Drummer {
        draw_drummer_main_tab(ui, sp, center_w);
        return;
    }

    match sp.kind {
        SessionPlayerKind::Bass => {
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    ui.add_space(30.0);
                    draw_pattern_badge_and_dots(
                        ui,
                        1,
                        &mut sp.pattern_idx,
                        sp.complexity,
                        false,
                        (center_w - 100.0).max(140.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add_space(56.0);
                    draw_inline_follow(ui, sp);
                });
            });
            ui.add_space(6.0);
            let (div_r, _) = ui.allocate_exact_size(vec2(center_w, 1.0), Sense::hover());
            ui.painter()
                .rect_filled(div_r, 0.0, Color32::from_rgb(0x2b, 0x2d, 0x34));
            ui.add_space(8.0);

            ui.horizontal_top(|ui| {
                // Square XY Pad using reusable daw_xy_pad widget
                let (xy_rect, _) = ui.allocate_exact_size(vec2(130.0, 116.0), Sense::hover());
                crate::widgets::daw_xy_pad(
                    ui,
                    xy_rect,
                    egui::Id::unique("session_bass_xy"),
                    &mut sp.xy_melody,
                    &mut sp.xy_octave,
                    "Melody",
                    "Octave",
                    Color32::from_rgb(0xf5, 0xc5, 0x18),
                );

                ui.add_space(14.0);
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
                        vec2(ui.available_width(), 20.0),
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
                        Color32::from_rgb(0x28, 0x2a, 0x31),
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
                ui.add_space(8.0);
                ui.label(
                    RichText::new(phosphor_egui::regular::HAND_PALM)
                        .size(20.0)
                        .color(ACCENT_YELLOW),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(phosphor_egui::regular::HAND_PALM)
                        .size(20.0)
                        .color(ACCENT_YELLOW),
                );
                ui.add_space(16.0);
                draw_pattern_badge_and_dots(
                    ui,
                    1,
                    &mut sp.pattern_idx,
                    sp.complexity,
                    false,
                    (center_w - 135.0).max(120.0),
                );
            });
            ui.horizontal(|ui| {
                ui.add_space(104.0);
                draw_inline_follow(ui, sp);
            });

            ui.add_space(6.0);
            let (div_r, _) = ui.allocate_exact_size(vec2(center_w, 1.0), Sense::hover());
            ui.painter()
                .rect_filled(div_r, 0.0, Color32::from_rgb(0x2b, 0x2d, 0x34));
            ui.add_space(6.0);

            let (kr_rect, kr_resp) =
                ui.allocate_exact_size(vec2(center_w, 20.0), Sense::click_and_drag());
            if (kr_resp.dragged() || kr_resp.clicked()) && kr_resp.interact_pointer_pos().is_some()
            {
                if let Some(pos) = kr_resp.interact_pointer_pos() {
                    let frac = ((pos.x - kr_rect.left()) / kr_rect.width()).clamp(0.0, 1.0);
                    let midi = (36.0 + frac * 60.0).round() as u8;
                    if midi.abs_diff(sp.key_range_lo) <= midi.abs_diff(sp.key_range_hi) {
                        sp.key_range_lo = midi.min(sp.key_range_hi.saturating_sub(5));
                    } else {
                        sp.key_range_hi = midi.max(sp.key_range_lo + 5);
                    }
                }
            }
            let gk = ui.painter_at(kr_rect);
            gk.rect_filled(kr_rect, 4.0, WELL_BG);
            let x_l = kr_rect.left()
                + ((f32::from(sp.key_range_lo) - 36.0) / 60.0).clamp(0.0, 1.0) * kr_rect.width();
            let x_r = kr_rect.left()
                + ((f32::from(sp.key_range_hi) - 36.0) / 60.0).clamp(0.0, 1.0) * kr_rect.width();
            gk.rect_filled(
                Rect::from_min_max(pos2(x_l, kr_rect.top()), pos2(x_r, kr_rect.bottom())),
                2.0,
                Color32::from_rgb(0x28, 0x2a, 0x31),
            );
            let left_thumb =
                Rect::from_center_size(pos2(x_l, kr_rect.center().y), vec2(20.0, 16.0));
            gk.rect_filled(left_thumb, 3.0, Color32::from_rgb(0x3a, 0x3c, 0x44));
            gk.rect_stroke(
                left_thumb,
                3.0,
                Stroke::new(1.0, Color32::from_rgb(0xf5, 0xc5, 0x18)),
                egui::StrokeKind::Inside,
            );
            gk.text(
                left_thumb.center(),
                Align2::CENTER_CENTER,
                phosphor_egui::regular::HAND_PALM,
                FontId::proportional(11.0),
                Color32::from_rgb(0xf5, 0xc5, 0x18),
            );

            let right_thumb =
                Rect::from_center_size(pos2(x_r, kr_rect.center().y), vec2(20.0, 16.0));
            gk.rect_filled(right_thumb, 3.0, Color32::from_rgb(0x3a, 0x3c, 0x44));
            gk.rect_stroke(
                right_thumb,
                3.0,
                Stroke::new(1.0, Color32::from_rgb(0xf5, 0xc5, 0x18)),
                egui::StrokeKind::Inside,
            );
            gk.text(
                right_thumb.center(),
                Align2::CENTER_CENTER,
                phosphor_egui::regular::HAND_PALM,
                FontId::proportional(11.0),
                Color32::from_rgb(0xf5, 0xc5, 0x18),
            );

            ui.add_space(8.0);
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

fn draw_inline_follow(ui: &mut Ui, sp: &mut SessionPlayerParams) {
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
    ui.add_space(8.0);
    ui.menu_button(
        RichText::new(match sp.add_notes {
            AddNotesMode::None => "Add Notes: None",
            AddNotesMode::Eighths => "Add Notes: 8ths",
            AddNotesMode::Sixteenths => "Add Notes: 16ths",
        })
        .size(10.0)
        .color(TEXT_DIM),
        |ui| {
            ui.selectable_value(&mut sp.add_notes, AddNotesMode::None, "None");
            ui.selectable_value(&mut sp.add_notes, AddNotesMode::Eighths, "8ths");
            ui.selectable_value(&mut sp.add_notes, AddNotesMode::Sixteenths, "16ths");
        },
    );
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
            ui.add_space(4.0);
            draw_labeled_horizontal_slider(ui, "Amount", &mut sp.pump_amount);
            draw_labeled_horizontal_slider(ui, "Shape", &mut sp.pump_shape);
        }
        SessionPlayerKind::Bass | SessionPlayerKind::Drummer => {
            ui.columns(2, |cols| {
                cols[0].vertical(|ui| {
                    ui.label(
                        RichText::new("Accents")
                            .size(11.0)
                            .strong()
                            .color(TEXT_MAIN),
                    );
                    ui.add_space(4.0);
                    draw_labeled_horizontal_slider(ui, "Rhythm", &mut sp.accent_vel);
                    draw_labeled_horizontal_slider(ui, "Octave", &mut sp.accent_len);
                    draw_labeled_horizontal_slider(ui, "Melody", &mut sp.accent_cc);
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("MIDI Target:").size(10.0).color(TEXT_DIM));
                        egui::ComboBox::from_id_salt("sp_cc_target_cb")
                            .selected_text(format!("CC {}", sp.cc_target))
                            .show_ui(ui, |ui| {
                                for cc in [1, 2, 11, 71, 74, 91] {
                                    ui.selectable_value(&mut sp.cc_target, cc, format!("CC {cc}"));
                                }
                            });
                    });
                });

                cols[1].vertical(|ui| {
                    ui.label(
                        RichText::new("Phrase Variation")
                            .size(11.0)
                            .strong()
                            .color(TEXT_MAIN),
                    );
                    ui.add_space(4.0);
                    draw_labeled_horizontal_slider(ui, "Rhythm", &mut sp.var_rhythm);
                    draw_labeled_horizontal_slider(ui, "Octave", &mut sp.var_octave);
                    draw_labeled_horizontal_slider(ui, "Melody", &mut sp.var_melody);
                });
            });
        }
        SessionPlayerKind::Keyboard => {
            ui.label(
                RichText::new("Keyboard Phrasing & Strum")
                    .size(11.0)
                    .strong()
                    .color(TEXT_MAIN),
            );
            ui.add_space(4.0);
            draw_labeled_horizontal_slider(ui, "Phrasing", &mut sp.phrasing);
            draw_labeled_horizontal_slider(ui, "Strum", &mut sp.strum);
            ui.add_space(4.0);
            ui.checkbox(&mut sp.tie_notes, "Tie Overlapping Chord Notes");
        }
    }
}

fn draw_slides_or_env_tab(ui: &mut Ui, sp: &mut SessionPlayerParams, center_w: f32) {
    if sp.kind == SessionPlayerKind::Keyboard && sp.keyboard_style.is_pad() {
        ui.horizontal(|ui| {
            ui.spacing_mut().slider_width = 76.0;
            ui.add(egui::Slider::new(&mut sp.env_shift_start, 0.0..=0.5).text("Shift Start"));
            ui.checkbox(&mut sp.env_align_peak, "Align: Peak");
            ui.checkbox(&mut sp.env_invert, "Invert");
        });
        let (env_rect, _) = ui.allocate_exact_size(vec2(center_w.max(220.0), 86.0), Sense::hover());
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
            let (rect, _) = ui.allocate_exact_size(vec2(104.0, 96.0), Sense::hover());
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
                ui.spacing_mut().slider_width = (center_w - 190.0).clamp(72.0, 120.0);
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("sp_slide_start")
                        .width(88.0)
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

fn draw_lfo_tab(ui: &mut Ui, sp: &mut SessionPlayerParams, center_w: f32) {
    ui.horizontal_wrapped(|ui| {
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
        let (rect, _) = ui.allocate_exact_size(vec2(104.0, 76.0), Sense::hover());
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
            ui.spacing_mut().slider_width = (center_w - 190.0).clamp(72.0, 120.0);
            ui.add(egui::Slider::new(&mut sp.lfo_rate, 0.05..=2.0).text("Rate"));
            ui.add(egui::Slider::new(&mut sp.lfo_amount, 0.0..=1.0).text("Amount"));
            ui.add(egui::Slider::new(&mut sp.lfo_phase, 0.0..=1.0).text("Phase"));
            ui.add(egui::Slider::new(&mut sp.lfo_symmetry, 0.05..=0.95).text("Symmetry"));
        });
    });
}

fn draw_manual_tab(ui: &mut Ui, sp: &mut SessionPlayerParams, center_w: f32) {
    ui.vertical_centered(|ui| {
        ui.checkbox(
            &mut sp.manual_override,
            "Override Pattern (1 Bar / 16 Steps)",
        );
        ui.add_space(8.0);
        let gap = 3.0;
        let step_w = ((center_w - 15.0 * gap) / 16.0).clamp(11.0, 17.0);
        let row_w = step_w * 16.0 + gap * 15.0;
        let pad = ((center_w - row_w) * 0.5).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.spacing_mut().item_spacing.x = gap;
            for (i, step) in sp.manual_steps.iter_mut().enumerate() {
                let col = if *step { ACCENT_YELLOW } else { WELL_BG };
                let btn = egui::Button::new(
                    RichText::new(format!("{}", i + 1))
                        .size(8.5)
                        .color(if *step { Color32::BLACK } else { TEXT_DIM }),
                )
                .fill(col)
                .min_size(vec2(step_w, 38.0))
                .corner_radius(CornerRadius::same(3));
                if ui.add(btn).clicked() {
                    *step = !*step;
                    sp.manual_override = true;
                }
            }
        });
    });
}

fn draw_session_knob(
    ui: &mut Ui,
    cfg: DawKnobConfig<'_>,
    current_val: f32,
) -> daw_knob::DawKnobResult {
    let side = cfg.diameter.max(36.0);
    let (cell, _) = ui.allocate_exact_size(vec2(side + 12.0, side + 12.0), Sense::hover());
    let g = ui.painter().clone();
    daw_knob::draw_custom(
        ui,
        &g,
        cell,
        cfg,
        current_val,
        None,
        DawKnobFinish::LogicSession,
        Some(Color32::from_rgb(0xf5, 0xc5, 0x18)),
        true,
    )
}
