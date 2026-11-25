//! The window of an effect in a channel strip's Audio FX slot (Logic: the plug-in window).
//!
//! Each window has the same chrome: the effect's name, a family chip (EQ, Dynamics, Space,
//! Time, Drive, Filter), Bypass and Reset, one bespoke visualization, and a rack of knobs.
//! Every control is automatable: moving one while the project plays writes it, and it moves by
//! itself when its lane is played.
use crate::automation;
use crate::i18n::{tr, trf};
use crate::mixer::Meters;
use crate::model::{AutoTarget, Insert, Project, Track};
use crate::UiState;
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Painter, Pos2, Rect,
    RichText, Sense, Shape, Stroke, StrokeKind, Ui,
};
use prowave_engine::{
    eq_band_response_db, eq_idx, eq_response_db, response_db, FxKind, ParamInfo, AMP_CABS,
    AMP_MODELS, EQ_BANDS,
};

/// Sample rate the curve of the EQs is drawn for.
const CURVE_RATE: f32 = 48_000.0;

/// Width and height of the Amp Designer's faceplate.
const AMP_W: f32 = 470.0;
const AMP_H: f32 = 146.0;

// The window palette: a warm charcoal rack, one signal accent per effect family.
const WIN_BG: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x1e);
const HEAD_BG: Color32 = Color32::from_rgb(0x23, 0x25, 0x2a);
const INSET: Color32 = Color32::from_rgb(0x12, 0x13, 0x16);
const LINE: Color32 = Color32::from_rgb(0x30, 0x33, 0x3a);
const TXT: Color32 = Color32::from_rgb(0xe8, 0xe6, 0xe1);
const DIM: Color32 = Color32::from_rgb(0x96, 0x9d, 0xa8);
const FAINT: Color32 = Color32::from_rgb(0x5f, 0x66, 0x70);

const EQ_BLUE: Color32 = Color32::from_rgb(0x6f, 0xb2, 0xff);
const DYN_AMBER: Color32 = Color32::from_rgb(0xe8, 0xb4, 0x5a);
const SPACE_VIOLET: Color32 = Color32::from_rgb(0xa9, 0x8b, 0xf5);
const TIME_BLUE: Color32 = Color32::from_rgb(0x8f, 0x9c, 0xff);
const DRIVE_RUST: Color32 = Color32::from_rgb(0xe0, 0x7a, 0x5f);
const FILTER_TEAL: Color32 = Color32::from_rgb(0x5f, 0xd1, 0xc5);

/// A slider the user moved: parameter number, new value, and whether the gesture ended.
pub(crate) struct Edit {
    pub(crate) param: usize,
    pub(crate) value: f32,
    pub(crate) ended: bool,
}

/// The family an effect belongs to; it chooses the accent and the copy in the header.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Eq,
    Dynamics,
    Space,
    Time,
    Drive,
    Filter,
}

fn family(kind: FxKind) -> (Family, &'static str, Color32) {
    match kind {
        // A plug-in has its own window (a LADSPA one `plugin_rack`, a hosted one `plugin_window`).
        FxKind::Plugin => (Family::Filter, "Plug-in", FILTER_TEAL),
        FxKind::ChannelEq | FxKind::MultiEq | FxKind::PultEq => (Family::Eq, "EQ", EQ_BLUE),
        FxKind::Compressor
        | FxKind::MultiComp
        | FxKind::Gate
        | FxKind::Limiter
        | FxKind::LemonLimiter
        | FxKind::LemonClip
        | FxKind::LemonTransient => (Family::Dynamics, "Dynamics", DYN_AMBER),
        FxKind::Reverb | FxKind::LemonReverb => (Family::Space, "Space", SPACE_VIOLET),
        FxKind::Delay | FxKind::Chorus | FxKind::Phaser | FxKind::LemonDelay => {
            (Family::Time, "Time", TIME_BLUE)
        }
        FxKind::Overdrive | FxKind::Amp | FxKind::LemonDist => (Family::Drive, "Drive", DRIVE_RUST),
        FxKind::Filter | FxKind::FreqShifter | FxKind::AutoPan => {
            (Family::Filter, "Modulation", FILTER_TEAL)
        }
    }
}

fn window_width(kind: FxKind) -> f32 {
    match kind {
        FxKind::ChannelEq => 780.0,
        FxKind::MultiEq | FxKind::PultEq => 632.0,
        FxKind::Amp => AMP_W + 28.0,
        FxKind::MultiComp => 604.0,
        _ => 560.0,
    }
}

/// Draws the effect window for the insert the Mixer opened, if any.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut UiState, meters: &Meters) {
    if let Some(ti) = s.mixer.inst_window {
        if let Some(track) = p.tracks.get(ti) {
            if track.hosted_instrument != 0 {
                let insert = Insert::new_hosted(track.hosted_instrument);
                forget_editor(s, Some(track.hosted_instrument));
                plugin_window(ctx, p, s, ti, 999, insert);
                return;
            }
        }
        s.mixer.inst_window = None;
    }
    let Some((ti, slot)) = s.mixer.fx_window else {
        forget_editor(s, None);
        return;
    };
    // The Stereo Out strip has effects but no automation lanes: a stand-in track with none.
    let master = ti == crate::mixer::MASTER_STRIP;
    let found = if master {
        p.master_inserts.get(slot)
    } else {
        p.tracks.get(ti).and_then(|t| t.inserts.get(slot))
    };
    let Some(insert) = found.copied() else {
        s.mixer.fx_window = None;
        return;
    };
    // One plug-in window at a time, as in Logic with its Link button on: another slot's
    // window takes the place of the one that was open.
    forget_editor(s, (insert.hosted != 0).then_some(insert.hosted));
    if insert.hosted != 0 {
        plugin_window(ctx, p, s, ti, slot, insert);
        return;
    }
    let stand_in;
    let track: &Track = if master {
        stand_in = Project::empty().tracks[0].clone();
        &stand_in
    } else {
        &p.tracks[ti]
    };
    let (reduction, input_level) = if master {
        (meters.master_reduction, meters.master_fx_input)
    } else {
        (
            meters.reduction.get(ti).copied().unwrap_or(0.0),
            meters.fx_input.get(ti).copied().unwrap_or(0.0),
        )
    };
    if insert.kind == FxKind::Reverb {
        #[allow(deprecated)]
        let id = egui::Id::new(("space_designer", ti, slot));
        let (edited, edits, open) = crate::space_designer_ui::window(
            ctx,
            &insert,
            track,
            slot,
            p.position_beats,
            id,
            input_level,
        );
        finish(p, s, ti, slot, insert, edited, edits, open);
        return;
    }
    let mut open = true;
    let title = if master {
        format!("{} · {}", tr("Stereo Out"), insert.label())
    } else {
        format!("{} · {}", p.tracks[ti].name, insert.label())
    };
    let beat = p.position_beats;
    let mut edited = insert;
    let mut edits: Vec<Edit> = Vec::new();
    let (_, family_name, accent) = family(edited.kind);
    let frame = egui::Frame::NONE
        .fill(WIN_BG)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::ZERO);
    egui::Window::new(title)
        .id(egui::Id::unique("fx_window"))
        .open(&mut open)
        .resizable(false)
        .frame(frame)
        .show(ctx, |ui| {
            header(ui, &mut edited, &mut edits, family_name, accent);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(window_width(edited.kind));
                    if edited.kind == FxKind::ChannelEq {
                        channel_eq_window(
                            ui,
                            &mut edited,
                            track,
                            slot,
                            beat,
                            input_level,
                            &mut edits,
                        );
                    } else {
                        visual(ui, &edited, reduction, beat, accent);
                        ui.add_space(10.0);
                        if edited.kind == FxKind::Amp {
                            amp_faceplate(ui, &mut edited, track, slot, beat, &mut edits);
                        } else if edited.kind == FxKind::Plugin {
                            plugin_rack(ui, &edited, track, slot, beat, accent, &mut edits);
                        } else {
                            knob_rack(ui, &mut edited, track, slot, beat, accent, &mut edits);
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Drag a knob to adjust · double-click to reset")
                                .size(9.5)
                                .color(FAINT),
                        );
                    }
                });
        });
    finish(p, s, ti, slot, insert, edited, edits, open);
}

/// Logic's blue of a plug-in that is on (its power button, its slot in the Mixer).
const POWER_BLUE: Color32 = Color32::from_rgb(0x36, 0x6f, 0xd2);

/// The power button at the left of a plug-in window's header (Logic: Bypass): blue while the
/// plug-in is on, grey while it is bypassed. Returns whether it was clicked.
fn power_button(ui: &mut Ui, on: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::click());
    let painter = ui.painter();
    let fill = if on {
        POWER_BLUE
    } else {
        Color32::from_gray(0x2f)
    };
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    let ink = if on { Color32::WHITE } else { DIM };
    let c = rect.center();
    painter.circle_stroke(c + vec2(0.0, 1.0), 4.5, Stroke::new(1.4, ink));
    // The gap at the top of the ring, and the bar through it.
    painter.rect_filled(
        egui::Rect::from_center_size(c - vec2(0.0, 3.5), vec2(4.0, 4.0)),
        0.0,
        fill,
    );
    painter.line_segment(
        [c - vec2(0.0, 5.5), c - vec2(0.0, 0.5)],
        Stroke::new(1.4, ink),
    );
    response.on_hover_text(tr("Bypass")).clicked()
}

/// A hosted plug-in's own window, as this side follows it: the window belongs to the plug-in's
/// process.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EditorWindow {
    /// The plug-in's number in `crate::plugins`.
    plugin: u32,
    /// When it was asked for (egui's clock), and whether it has been seen on screen since.
    asked: f64,
    seen: bool,
}

/// How long a plug-in has to put its own window on screen before its Controls are shown
/// instead.
const EDITOR_GRACE: f64 = 2.0;

/// Closes the plug-in window that is not the one the Mixer has open any more.
fn forget_editor(s: &mut UiState, keep: Option<u32>) {
    if let Some(editor) = s.mixer.plugin_editor {
        if keep != Some(editor.plugin) {
            if let Some(instance) = crate::hosted_plugins::instance(editor.plugin) {
                instance.close_editor();
            }
            s.mixer.plugin_editor = None;
        }
    }
}

/// A hosted plug-in (CLAP, VST3, Audio Unit) the Mixer opened. As in Logic it has two views:
/// **Editor**, the plug-in's own window (shown by the plug-in's process, see
/// `prowave-plugin-host`), and **Controls**, drawn here: the header with the power button,
/// then every parameter as a row with its name, a slider, and the value in the plug-in's own
/// wording. Double-click a slider for its default. Controls is also what shows for a plug-in
/// without a window of its own, and for one that stopped working, with Reload.
fn plugin_window(
    ctx: &egui::Context,
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    slot: usize,
    insert: Insert,
) {
    let master = ti == crate::mixer::MASTER_STRIP;
    let owner = if master {
        tr("Stereo Out").to_string()
    } else {
        p.tracks[ti].name.clone()
    };
    let instance = crate::hosted_plugins::instance(insert.hosted);
    let info = crate::hosted_plugins::info(insert.hosted);
    let crashed = crate::hosted_plugins::crashed(insert.hosted);
    let mut open = true;
    let mut bypass = insert.bypass;
    let mut reload = false;
    let now = ctx.input(|i| i.time);

    // The Editor view: the plug-in's own window, until it is closed there.
    let wants_editor =
        !s.mixer.plugin_controls && !crashed && instance.as_ref().is_some_and(|i| i.has_editor());
    if let (true, Some(instance)) = (wants_editor, &instance) {
        if instance.take_power_click() {
            bypass = !bypass;
            instance.open_editor(&owner, bypass);
        }
        let editor = s.mixer.plugin_editor.get_or_insert_with(|| {
            instance.open_editor(&owner, bypass);
            EditorWindow {
                plugin: insert.hosted,
                asked: now,
                seen: false,
            }
        });
        if instance.editor_open() {
            editor.seen = true;
        } else if editor.seen {
            // Closed in its own window: the plug-in is no longer open here either.
            open = false;
        }
        let waiting = !editor.seen && now - editor.asked < EDITOR_GRACE;
        if editor.seen || waiting {
            // The window is in another process: look again soon for its close or power button.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            apply_plugin_window(p, s, ti, slot, insert, bypass, open, false);
            return;
        }
    }

    let frame = egui::Frame::NONE
        .fill(WIN_BG)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::ZERO);
    let mut edits: Vec<Edit> = Vec::new();
    let mut to_editor = false;
    egui::Window::new(format!("{owner} · {}", insert.name()))
        .id(egui::Id::unique("plugin_window"))
        .open(&mut open)
        .resizable([false, true])
        .default_height(420.0)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(520.0);
            egui::Frame::NONE
                .fill(HEAD_BG)
                .inner_margin(egui::Margin::symmetric(14, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if power_button(ui, !bypass) {
                            bypass = !bypass;
                        }
                        ui.label(RichText::new(insert.name()).size(13.5).color(TXT));
                        if let Some(info) = &info {
                            let about = format!("{} · {}", info.vendor, info.format.name());
                            ui.label(RichText::new(about).size(9.5).color(DIM));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // Logic's View menu: Controls is this window, Editor the plug-in's.
                            let has_editor =
                                !crashed && instance.as_ref().is_some_and(|i| i.has_editor());
                            if has_editor {
                                let label = RichText::new(tr("Editor")).size(10.5);
                                if ui.add(egui::Button::new(label)).clicked() {
                                    to_editor = true;
                                }
                            }
                            if let Some(inst) = &instance {
                                let ext = match info.as_ref().map(|i| i.format) {
                                    Some(prowave_plugin_host::Format::Clap) => "clappreset",
                                    _ => "vstpreset",
                                };
                                if ui
                                    .add(egui::Button::new(RichText::new(tr("Save…")).size(10.5)))
                                    .clicked()
                                {
                                    if let Some(blob) = crate::hosted_plugins::state(insert.hosted)
                                    {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("Preset", &[ext])
                                            .set_file_name(format!("{}.{ext}", insert.name()))
                                            .save_file()
                                        {
                                            let _ = std::fs::write(path, blob);
                                        }
                                    }
                                }
                                if ui
                                    .add(egui::Button::new(RichText::new(tr("Open…")).size(10.5)))
                                    .clicked()
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("Preset", &[ext, "vstpreset", "clappreset"])
                                        .pick_file()
                                    {
                                        if let Ok(blob) = std::fs::read(path) {
                                            inst.load_state(&blob);
                                        }
                                    }
                                }
                            }
                            ui.label(RichText::new(tr("Controls")).size(10.5).color(DIM));
                        });
                    });
                });
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, LINE);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    if crashed {
                        ui.label(
                            RichText::new(tr(
                                "The plug-in stopped working. The sound passes through its slot.",
                            ))
                            .color(TXT),
                        );
                        ui.add_space(6.0);
                        reload = ui.button(tr("Reload")).clicked();
                        return;
                    }
                    let Some(instance) = &instance else {
                        // Loaded by the engine's next sync, or not at all: say which.
                        let text = crate::hosted_plugins::error(insert.hosted)
                            .unwrap_or_else(|| tr("Loading…").to_string());
                        ui.label(RichText::new(text).color(DIM));
                        return;
                    };
                    ui.add(
                        egui::TextEdit::singleline(&mut s.mixer.plugin_search)
                            .hint_text(tr("Search"))
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    let track = (!master).then(|| &p.tracks[ti]);
                    let search = s.mixer.plugin_search.clone();
                    let beat = p.position_beats;
                    plugin_controls(ui, instance, &search, track, slot, beat, &mut edits);
                });
        });
    if to_editor {
        s.mixer.plugin_controls = false;
        s.mixer.plugin_editor = None;
    }
    // A slider move goes through the strip's automation, as a built-in effect's knob does: it
    // sets the plug-in, and in Touch, Latch and Write it is written to the lane.
    if !master {
        let (beat, playing) = (p.position_beats, p.playing);
        for e in &edits {
            let target = AutoTarget::Fx {
                slot: slot as u8,
                param: e.param as u8,
            };
            automation::moved(&mut p.tracks[ti], target, e.value);
            if e.ended {
                automation::released(&mut p.tracks[ti], target, beat, playing);
            }
        }
    }
    apply_plugin_window(p, s, ti, slot, insert, bypass, open, reload);
}

/// What a frame of a hosted plug-in's window changed in the project: Bypass, the window
/// closing, and Reload, which puts a fresh copy of a plug-in that stopped working in its slot.
#[allow(clippy::too_many_arguments)]
fn apply_plugin_window(
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    slot: usize,
    insert: Insert,
    bypass: bool,
    open: bool,
    reload: bool,
) {
    if slot == 999 {
        if let Some(t) = p.tracks.get_mut(ti) {
            if reload {
                if let Some(fresh) = crate::hosted_plugins::again(insert.hosted) {
                    t.hosted_instrument = fresh;
                }
            }
        }
        if !open {
            s.mixer.inst_window = None;
            forget_editor(s, None);
        }
        return;
    }
    let inserts = if ti == crate::mixer::MASTER_STRIP {
        Some(&mut p.master_inserts)
    } else {
        p.tracks.get_mut(ti).map(|t| &mut t.inserts)
    };
    if let Some(found) = inserts.and_then(|list| list.get_mut(slot)) {
        found.bypass = bypass;
        if reload {
            if let Some(fresh) = crate::hosted_plugins::again(insert.hosted) {
                found.hosted = fresh;
            }
        }
    }
    if !open {
        s.mixer.fx_window = None;
        forget_editor(s, None);
    }
}

/// The rows of Logic's Controls view for the parameters whose name has `search` in it: the
/// name, right-aligned as Logic sets it, a slider, and the value in a dark field. A parameter
/// the strip is playing a lane for shows the lane's value and cannot be moved (as a built-in
/// effect's knob); moves of the others are added to `edits`, or go straight to the plug-in when
/// they cannot be automated (the Stereo Out strip, a parameter past the 256th).
fn plugin_controls(
    ui: &mut Ui,
    instance: &crate::hosted_plugins::Instance,
    search: &str,
    track: Option<&Track>,
    slot: usize,
    beat: f64,
    edits: &mut Vec<Edit>,
) {
    const NAME_W: f32 = 170.0;
    const VALUE_W: f32 = 104.0;
    let query = search.trim().to_lowercase();
    let rows: Vec<usize> = instance
        .params()
        .iter()
        .enumerate()
        .filter(|(_, param)| query.is_empty() || param.name.to_lowercase().contains(&query))
        .map(|(index, _)| index)
        .collect();
    if rows.is_empty() {
        ui.label(RichText::new(tr("No plug-ins match")).color(FAINT));
        return;
    }
    let height = 20.0;
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        height,
        rows.len(),
        |ui, range| {
            for &index in &rows[range] {
                let param = &instance.params()[index];
                let lane = track.filter(|_| index < 256).map(|t| {
                    let target = AutoTarget::Fx {
                        slot: slot as u8,
                        param: index as u8,
                    };
                    (t, target)
                });
                let free = lane.is_none_or(|(t, target)| automation::editable(t, target));
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(NAME_W, height),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&param.name).size(11.0).color(TXT))
                                    .truncate(),
                            );
                        },
                    );
                    let mut value = match lane {
                        Some((t, target)) => automation::shown_value(t, target, beat),
                        None => instance.param(index),
                    };
                    ui.spacing_mut().slider_width =
                        (ui.available_width() - VALUE_W - 16.0).max(60.0);
                    let slider = ui.add_enabled(
                        free,
                        egui::Slider::new(&mut value, 0.0..=1.0).show_value(false),
                    );
                    let moved = if slider.double_clicked() {
                        Some(param.default)
                    } else if slider.changed() {
                        Some(value)
                    } else {
                        None
                    };
                    let ended = slider.drag_stopped() || slider.double_clicked();
                    match (moved, lane) {
                        (Some(value), None) => instance.set_param(index, value),
                        (moved, Some(_)) if moved.is_some() || ended => edits.push(Edit {
                            param: index,
                            value: moved.unwrap_or(value),
                            ended,
                        }),
                        _ => {}
                    }
                    egui::Frame::NONE
                        .fill(INSET)
                        .corner_radius(CornerRadius::same(3))
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .show(ui, |ui| {
                            ui.set_width(VALUE_W - 12.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(instance.param_text(index))
                                        .size(10.5)
                                        .color(TXT),
                                )
                                .truncate(),
                            );
                        });
                });
            }
        },
    );
}

/// The title strip: the effect's family, Bypass and Reset.
fn header(
    ui: &mut Ui,
    edited: &mut Insert,
    _edits: &mut [Edit],
    family_name: &str,
    accent: Color32,
) {
    egui::Frame::NONE
        .fill(HEAD_BG)
        .inner_margin(egui::Margin::symmetric(14, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(family_name).size(9.5).color(accent));
                ui.label(RichText::new("·").size(10.0).color(FAINT));
                ui.label(RichText::new(edited.label()).size(13.5).color(TXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(tr("Reset")).size(10.5))
                                .min_size(vec2(48.0, 20.0)),
                        )
                        .clicked()
                    {
                        edited.params = edited.kind.defaults();
                    }
                    let mut bypass = edited.bypass;
                    let (sw_rect, _) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::hover());
                    if crate::widgets::daw_rocker_switch(
                        ui,
                        sw_rect,
                        ui.make_persistent_id(("fx_pwr", edited.label())),
                        !bypass,
                        false,
                    ) {
                        bypass = !bypass;
                        edited.bypass = bypass;
                    }
                    let plate_label = match edited.kind {
                        FxKind::Overdrive | FxKind::LemonDist | FxKind::Amp => "DRIVE",
                        FxKind::Limiter | FxKind::LemonLimiter | FxKind::LemonClip => "LIMIT",
                        FxKind::Compressor | FxKind::MultiComp => "COMP",
                        _ => "ACTIVE",
                    };
                    let (pl_rect, _) = ui.allocate_exact_size(vec2(52.0, 20.0), Sense::hover());
                    if crate::widgets::daw_backlit_plate(
                        ui,
                        pl_rect,
                        ui.make_persistent_id(("fx_plate", edited.label())),
                        plate_label,
                        !bypass,
                        matches!(
                            edited.kind,
                            FxKind::Overdrive | FxKind::LemonDist | FxKind::Amp
                        ),
                    ) {
                        bypass = !bypass;
                        edited.bypass = bypass;
                    }
                    let response = ui.add(
                        egui::Button::new(
                            RichText::new(tr("Bypass")).size(10.5).color(if bypass {
                                Color32::WHITE
                            } else {
                                DIM
                            }),
                        )
                        .fill(if bypass {
                            Color32::from_rgb(0xd9, 0x77, 0x06)
                        } else {
                            Color32::from_rgb(0x2b, 0x2b, 0x2e)
                        })
                        .stroke(Stroke::new(
                            1.0,
                            if bypass {
                                Color32::from_rgb(0xf5, 0x9e, 0x0b)
                            } else {
                                Color32::from_rgb(0x38, 0x38, 0x3c)
                            },
                        ))
                        .min_size(vec2(52.0, 20.0)),
                    );
                    if response.clicked() {
                        bypass = !bypass;
                        edited.bypass = bypass;
                    }
                });
            });
        });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

/// The bespoke visualization of each effect.
fn visual(ui: &mut Ui, insert: &Insert, reduction: f32, beat: f64, accent: Color32) {
    let params = insert.kind.clamp_params(insert.params);
    match insert.kind {
        FxKind::ChannelEq | FxKind::MultiEq | FxKind::PultEq => {
            eq_curve(ui, insert.kind, &params, accent)
        }
        FxKind::Compressor | FxKind::Gate | FxKind::Limiter => {
            transfer_curve(ui, insert.kind, &params, reduction, accent)
        }
        FxKind::MultiComp => multicomp(ui, &params, reduction, accent),
        FxKind::Delay => delay_panel(ui, &params, accent),
        FxKind::Chorus => chorus_panel(ui, &params, accent),
        FxKind::Phaser => phaser_panel(ui, &params, beat, accent),
        FxKind::Overdrive => drive_panel(ui, &params, accent),
        FxKind::Filter => filter_panel(ui, &params, accent),
        FxKind::LemonLimiter => transfer_curve(ui, FxKind::Limiter, &params, reduction, accent),
        FxKind::LemonDelay => delay_panel(ui, &params, accent),
        FxKind::LemonDist => drive_panel(ui, &params, accent),
        FxKind::AutoPan => phaser_panel(ui, &params, beat, accent),
        FxKind::LemonClip
        | FxKind::LemonTransient
        | FxKind::LemonReverb
        | FxKind::FreqShifter
        | FxKind::Amp
        | FxKind::Reverb
        | FxKind::Plugin => {}
    }
}

/// The knob rack: every parameter as a knob, in rows that fit the window.
fn knob_rack(
    ui: &mut Ui,
    insert: &mut Insert,
    track: &Track,
    slot: usize,
    beat: f64,
    accent: Color32,
    edits: &mut Vec<Edit>,
) {
    let cells = insert.kind.params().len();
    // Hardware auxiliary controls for specific built-in FX engines.
    if insert.kind == FxKind::Filter && cells >= 3 {
        ui.horizontal(|ui| {
            let (sel_rect, _) = ui.allocate_exact_size(vec2(128.0, 76.0), Sense::hover());
            let mode = insert.params[2].round().clamp(0.0, 2.0) as usize;
            if let Some(next) = crate::widgets::daw_radial_selector(
                ui,
                sel_rect,
                ui.make_persistent_id(("filter_mode", slot)),
                &["LP", "HP", "BP"],
                mode,
            ) {
                edits.push(Edit {
                    param: 2,
                    value: next as f32,
                    ended: true,
                });
            }
        });
        ui.add_space(4.0);
    } else if matches!(insert.kind, FxKind::Overdrive | FxKind::LemonDist) && cells >= 3 {
        let infos = insert.kind.params();
        let mut norm_vals = [
            infos[0].to_norm(insert.params[0]),
            infos[1].to_norm(insert.params[1]),
            infos[2].to_norm(insert.params[2]),
        ];
        ui.horizontal(|ui| {
            let (xyz_rect, _) = ui.allocate_exact_size(vec2(150.0, 82.0), Sense::hover());
            if crate::widgets::daw_xyz_sliders(
                ui,
                xyz_rect,
                ui.make_persistent_id(("drive_xyz", slot)),
                &["DRIVE", "TONE", "MIX"],
                &mut norm_vals,
                true,
            ) {
                for (k, &nv) in norm_vals.iter().enumerate() {
                    edits.push(Edit {
                        param: k,
                        value: infos[k].from_norm(nv),
                        ended: false,
                    });
                }
            }
            let (led_rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            let boosted = insert.params[0] > infos[0].default + 1.0;
            if crate::widgets::daw_led_button(
                ui,
                led_rect.center(),
                9.0,
                ui.make_persistent_id(("drive_led", slot)),
                boosted,
                true,
            ) {
                let target_v = if boosted {
                    infos[0].default
                } else {
                    (infos[0].max * 0.6).max(infos[0].default + 6.0)
                };
                edits.push(Edit {
                    param: 0,
                    value: target_v.clamp(infos[0].min, infos[0].max),
                    ended: true,
                });
            }
        });
        ui.add_space(4.0);
    } else if matches!(
        insert.kind,
        FxKind::Compressor | FxKind::MultiComp | FxKind::Limiter
    ) {
        ui.horizontal(|ui| {
            let (led_rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            let on = !insert.bypass;
            if crate::widgets::daw_led_button(
                ui,
                led_rect.center(),
                8.5,
                ui.make_persistent_id(("dyn_led", slot)),
                on,
                false,
            ) {
                insert.bypass = !insert.bypass;
            }
            ui.label(RichText::new("ENGAGE").size(9.5).color(DIM));
        });
        ui.add_space(2.0);
    }
    ui.horizontal_wrapped(|ui| {
        for k in 0..cells {
            let info = &insert.kind.params()[k];
            let target = AutoTarget::Fx {
                slot: slot as u8,
                param: k as u8,
            };
            let played = track.auto.played(target, beat);
            let value = played
                .map(|n| info.from_norm(n))
                .unwrap_or(insert.params[k]);
            let locked = played.is_some() || !automation::editable(track, target);
            let automated = !track.auto.lane(target).is_empty();
            let change = param_knob(
                ui,
                info,
                value,
                locked,
                automated,
                accent,
                insert.kind,
                None,
            );
            if let Some((value, ended)) = change {
                edits.push(Edit {
                    param: k,
                    value,
                    ended,
                });
            }
        }
    });
}

/// The names of a plug-in's controls as `&'static str`, for [`ParamInfo`]: each distinct name is
/// kept once for the life of the program (a plug-in has a few dozen at most).
fn static_name(name: &str) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut map = NAMES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(known) = map.get(name) {
        return known;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    map.insert(name.to_string(), leaked);
    leaked
}

/// The knobs of a LADSPA effect: one for each control input, in the plug-in's own name and
/// range (a toggle shows On or Off, a whole-number control steps). The settings are kept as
/// positions from 0 to 1, so automation lanes work like those of the built-in effects.
fn plugin_rack(
    ui: &mut Ui,
    insert: &Insert,
    track: &Track,
    slot: usize,
    beat: f64,
    accent: Color32,
    edits: &mut Vec<Edit>,
) {
    let Some(id) = insert.plugin else {
        return;
    };
    let Some(info) = crate::plugins::info(id) else {
        ui.label(tr(
            "This plug-in is not on this computer; the sound passes through.",
        ));
        return;
    };
    if info.controls.is_empty() {
        ui.label(tr("This plug-in has no settings."));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (k, c) in info.controls.iter().enumerate() {
            let target = AutoTarget::Fx {
                slot: slot as u8,
                param: k as u8,
            };
            let played = track.auto.played(target, beat);
            let norm = played.unwrap_or(insert.params[k]);
            let value = c.at_norm(norm);
            let locked = played.is_some() || !automation::editable(track, target);
            let automated = !track.auto.lane(target).is_empty();
            let (lo, hi) = c.range();
            let knob = ParamInfo {
                name: static_name(&c.name),
                min: lo,
                max: hi,
                default: c.clamp(c.default),
                unit: "",
                log: c.log && lo > 0.0,
            };
            let text = |v: f32| c.text(c.clamp(v));
            let change = param_knob(
                ui,
                &knob,
                value,
                locked,
                automated,
                accent,
                FxKind::Plugin,
                Some(&text),
            );
            if let Some((v, ended)) = change {
                edits.push(Edit {
                    param: k,
                    value: c.to_norm(v),
                    ended,
                });
            }
        }
    });
}

/// One knob cell: label, knob, value. Returns a change when the user moves it.
#[allow(clippy::too_many_arguments)]
fn param_knob(
    ui: &mut Ui,
    info: &ParamInfo,
    value: f32,
    locked: bool,
    automated: bool,
    accent: Color32,
    kind: FxKind,
    text: Option<&dyn Fn(f32) -> String>,
) -> Option<(f32, bool)> {
    let mut change = None;
    ui.allocate_ui_with_layout(vec2(94.0, 74.0), Layout::top_down(Align::Center), |ui| {
        ui.add_space(1.0);
        let label =
            ui.label(
                RichText::new(info.name)
                    .size(9.5)
                    .color(if locked { FAINT } else { DIM }),
            );
        if automated {
            label.on_hover_text(tr("Automated"));
        }
        let (rect, response) = ui.allocate_exact_size(vec2(44.0, 38.0), Sense::click_and_drag());
        let norm = info.to_norm(value);
        let painter = ui.painter_at(rect);
        let c = rect.center();
        // The value arc behind the knob.
        arc(
            &painter,
            c,
            17.0,
            -225f32.to_radians(),
            (-225.0 + 270.0 * norm).to_radians(),
            accent.gamma_multiply(0.75),
            2.2,
        );
        crate::widgets::draw_knob(&painter, c, 13.0, norm * 2.0 - 1.0);
        if automated {
            painter.circle_filled(pos2(rect.left() + 2.0, rect.top() + 3.0), 2.2, accent);
        }
        if !locked {
            let ctrl_cmd = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
            if response.double_clicked() || (response.clicked() && ctrl_cmd) {
                change = Some((info.default, true));
            } else if response.dragged() {
                response.request_focus();
                let fine = if ui.input(|i| i.modifiers.shift) {
                    0.1
                } else {
                    1.0
                };
                let n = (norm - response.drag_delta().y * 0.006 * fine).clamp(0.0, 1.0);
                change = Some((info.from_norm(n), false));
            } else if response.drag_stopped() {
                change = Some((value, true));
            } else if response.has_focus() {
                let (up, down) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowUp),
                        i.key_pressed(egui::Key::ArrowDown),
                    )
                });
                if up || down {
                    let dir = if up { 1.0 } else { -1.0 };
                    let step = dir * if ctrl_cmd { 0.005 } else { 0.02 };
                    let n = (norm + step).clamp(0.0, 1.0);
                    change = Some((info.from_norm(n), true));
                }
            }
        }
        let v = change.map_or(value, |(v, _)| v);
        let formatted = match text {
            Some(f) => f(v),
            None => param_text(kind, info, v),
        };
        let (r_rect, _) = ui.allocate_exact_size(vec2(88.0, 14.0), Sense::hover());
        let r_painter = ui.painter_at(r_rect);
        if !locked {
            if let Some(edited) = crate::widgets::daw_param_readout(
                ui,
                &r_painter,
                r_rect,
                response.id.with("readout"),
                &formatted,
                info.unit,
                info.min,
                info.max,
                true,
                None,
            ) {
                change = Some((edited, true));
            }
        } else {
            r_painter.text(
                r_rect.center(),
                egui::Align2::CENTER_CENTER,
                formatted,
                egui::FontId::monospace(9.5),
                FAINT,
            );
        }
    });
    change
}

/// A parameter's value with the units the effect uses (and names for mode switches).
fn param_text(kind: FxKind, info: &ParamInfo, v: f32) -> String {
    if kind == FxKind::Filter && info.name == "Mode" {
        return ["Low Pass", "High Pass", "Band Pass"][(v.round() as usize).min(2)].to_string();
    }
    if kind == FxKind::PultEq {
        if let Some(text) = pult_text(info.name, v) {
            return text;
        }
    }
    if matches!(info.name, "Amp" | "Cabinet") {
        return format!("{:.0}", v + 1.0);
    }
    if info.name == "Reverse" {
        return if v >= 0.5 { "On".into() } else { "Off".into() };
    }
    knob_text(info, v)
}

/// Pult EQ's selectors by name (the knob stops at whole positions): the mode, and the frequency
/// of each selector; its dials run 0 to 10 like the hardware's.
fn pult_text(name: &str, v: f32) -> Option<String> {
    use prowave_engine::pult_eq::{atten_freq_hz, high_freq_hz, low_freq_hz, mode_name};
    let hz = |f: f32| {
        if f >= 1000.0 {
            format!("{} kHz", f / 1000.0)
        } else {
            format!("{f} Hz")
        }
    };
    match name {
        "Mode" => Some(tr(mode_name(v)).to_string()),
        "Low Freq" => Some(hz(low_freq_hz(v))),
        "High Freq" => Some(hz(high_freq_hz(v))),
        "Atten Freq" => Some(hz(atten_freq_hz(v))),
        "Input" | "Output" => None,
        _ => Some(format!("{v:.1}")),
    }
}

/// Draws an arc between two angles (radians, screen space) around `c`.
fn arc(g: &egui::Painter, c: Pos2, r: f32, from: f32, to: f32, color: Color32, width: f32) {
    let n = 28;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let a = from + (to - from) * i as f32 / n as f32;
        pts.push(c + vec2(a.cos(), a.sin()) * r);
    }
    g.add(Shape::line(pts, Stroke::new(width, color)));
}

/// A panel with the window's inset background and a hairline border.
fn inset(ui: &mut Ui, height: f32, add: impl FnOnce(&egui::Painter, Rect)) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let g = ui.painter_at(rect);
    g.rect_filled(rect, CornerRadius::same(5), INSET);
    g.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, LINE),
        StrokeKind::Inside,
    );
    add(&g, rect.shrink(1.0));
}

fn graph_grid(g: &egui::Painter, rect: Rect, xs: &[f32], ys: &[f32], color: Color32) {
    for x in xs {
        let x = rect.left() + rect.width() * x;
        g.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, color),
        );
    }
    for y in ys {
        let y = rect.top() + rect.height() * y;
        g.line_segment(
            [pos2(rect.left(), y), pos2(rect.right(), y)],
            Stroke::new(1.0, color),
        );
    }
}

/// Puts an edited effect and the setting moves of its window into the project (and the
/// automation), and closes the window when it was closed.
#[allow(clippy::too_many_arguments)]
fn finish(
    p: &mut Project,
    s: &mut UiState,
    ti: usize,
    slot: usize,
    insert: Insert,
    mut edited: Insert,
    edits: Vec<Edit>,
    open: bool,
) {
    let beat = p.position_beats;
    if edited != insert {
        edited.params = edited.kind.clamp_params(edited.params);
        if ti == crate::mixer::MASTER_STRIP {
            if let Some(i) = p.master_inserts.get_mut(slot) {
                *i = edited;
            }
        } else {
            p.tracks[ti].inserts[slot] = edited;
        }
    }
    if ti == crate::mixer::MASTER_STRIP {
        // No automation lanes on the Stereo Out strip: a knob moves the setting itself.
        if let Some(ins) = p.master_inserts.get_mut(slot) {
            for e in &edits {
                if let Some(info) = ins.kind.params().get(e.param) {
                    if e.value.is_finite() {
                        ins.params[e.param] = e.value.clamp(info.min, info.max);
                    }
                }
            }
        }
        if !open {
            s.mixer.fx_window = None;
        }
        return;
    }
    let playing = p.playing;
    for e in edits {
        let target = AutoTarget::Fx {
            slot: slot as u8,
            param: e.param as u8,
        };
        automation::moved(&mut p.tracks[ti], target, e.value);
        if e.ended {
            automation::released(&mut p.tracks[ti], target, beat, playing);
        }
    }
    if !open {
        s.mixer.fx_window = None;
    }
}

/// What a control of the amplifier shows at `beat`: its lane's value while the strip plays
/// one, else the setting the user made.
fn shown(insert: &Insert, track: &Track, slot: usize, param: usize, beat: f64) -> f32 {
    let info = &insert.kind.params()[param];
    let target = AutoTarget::Fx {
        slot: slot as u8,
        param: param as u8,
    };
    track
        .auto
        .played(target, beat)
        .map(|n| info.from_norm(n))
        .unwrap_or(insert.params[param])
}

/// A setting under a knob, in its own units.
fn knob_text(info: &ParamInfo, v: f32) -> String {
    if info.unit == "dB" {
        format!("{v:+.1} dB")
    } else if info.unit.is_empty() && info.min >= 0.0 && info.max <= 10.0 {
        format!("{v:.1}")
    } else if info.unit.is_empty() {
        format!("{:.0} %", v * 100.0)
    } else {
        format!("{v:.1} {}", info.unit)
    }
}

/// The Amp Designer's window: the faceplate with its six knobs, the amplifier and cabinet
/// choices and the microphone position. All of it is automatable, like the sliders of the
/// other effects.
fn amp_faceplate(
    ui: &mut Ui,
    insert: &mut Insert,
    track: &Track,
    slot: usize,
    beat: f64,
    edits: &mut Vec<Edit>,
) {
    let model = shown(insert, track, slot, 0, beat).round().clamp(0.0, 4.0) as usize;
    let cab = shown(insert, track, slot, 7, beat).round().clamp(0.0, 3.0) as usize;

    // The amplifier itself: dark tolex, a cream piping and a speaker grille on the right.
    let (rect, _) = ui.allocate_exact_size(vec2(AMP_W, AMP_H), Sense::hover());
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 7.0, Color32::from_rgb(0x23, 0x21, 0x1e));
    g.rect_stroke(
        rect,
        7.0,
        Stroke::new(1.0, Color32::from_gray(0x0c)),
        StrokeKind::Inside,
    );
    g.rect_stroke(
        rect.shrink(4.0),
        5.0,
        Stroke::new(1.0, Color32::from_rgb(0x6d, 0x5f, 0x43)),
        StrokeKind::Inside,
    );
    let cream = Color32::from_rgb(0xf1, 0xe7, 0xcd);
    let dim = Color32::from_rgb(0x9c, 0x8f, 0x74);
    g.text(
        pos2(rect.left() + 18.0, rect.top() + 17.0),
        Align2::LEFT_CENTER,
        "PRO WAVE",
        FontId::proportional(10.0),
        Color32::from_rgb(0xc8, 0xb6, 0x8c),
    );
    g.text(
        pos2(rect.left() + 18.0, rect.top() + 39.0),
        Align2::LEFT_CENTER,
        AMP_MODELS[model],
        FontId::proportional(19.0),
        cream,
    );
    g.text(
        pos2(rect.left() + 18.0, rect.top() + 57.0),
        Align2::LEFT_CENTER,
        AMP_CABS[cab],
        FontId::proportional(10.5),
        dim,
    );

    let grille = Rect::from_min_max(
        pos2(rect.right() - 130.0, rect.top() + 12.0),
        pos2(rect.right() - 12.0, rect.bottom() - 12.0),
    );
    let cloth = g.with_clip_rect(grille);
    cloth.rect_filled(grille, 4.0, Color32::from_rgb(0x3a, 0x32, 0x25));
    let mut x = grille.left() - grille.height();
    while x < grille.right() {
        cloth.line_segment(
            [
                pos2(x, grille.bottom()),
                pos2(x + grille.height(), grille.top()),
            ],
            Stroke::new(1.0, Color32::from_rgb(0x47, 0x3d, 0x2d)),
        );
        x += 5.0;
    }
    let c = grille.center();
    g.circle_filled(c, 40.0, Color32::from_rgb(0x1b, 0x18, 0x13));
    g.circle_stroke(
        c,
        40.0,
        Stroke::new(2.0, Color32::from_rgb(0x5c, 0x50, 0x3a)),
    );
    g.circle_stroke(
        c,
        27.0,
        Stroke::new(1.0, Color32::from_rgb(0x35, 0x2f, 0x24)),
    );
    g.circle_filled(c, 11.0, Color32::from_rgb(0x2a, 0x25, 0x1d));

    // The six knobs: Gain, Bass, Mid, Treble, Presence, Master.
    const KNOBS: [(usize, &str); 6] = [
        (1, "Gain"),
        (2, "Bass"),
        (3, "Mid"),
        (4, "Treble"),
        (5, "Presence"),
        (6, "Master"),
    ];
    for (i, (param, label)) in KNOBS.iter().enumerate() {
        let info = &insert.kind.params()[*param];
        let target = AutoTarget::Fx {
            slot: slot as u8,
            param: *param as u8,
        };
        let played = track.auto.played(target, beat);
        let v = shown(insert, track, slot, *param, beat);
        let kc = pos2(rect.left() + 42.0 + i as f32 * 54.0, rect.top() + 104.0);
        g.text(
            pos2(kc.x, rect.top() + 76.0),
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(9.5),
            dim,
        );
        g.text(
            pos2(kc.x, rect.top() + 130.0),
            Align2::CENTER_CENTER,
            knob_text(info, v),
            FontId::proportional(9.5),
            cream,
        );
        crate::widgets::draw_knob(&g, kc, 13.0, info.to_norm(v) * 2.0 - 1.0);
        if played.is_none() && automation::editable(track, target) {
            let r = Rect::from_center_size(kc, vec2(32.0, 32.0));
            let resp = ui.interact(
                r,
                ui.scope_id().with(("amp_knob", slot, param)),
                Sense::click_and_drag(),
            );
            if resp.dragged() {
                let n = info.to_norm(v) - resp.drag_delta().y * 0.006;
                edits.push(Edit {
                    param: *param,
                    value: info.from_norm(n),
                    ended: false,
                });
            }
            if resp.drag_stopped() {
                edits.push(Edit {
                    param: *param,
                    value: v,
                    ended: true,
                });
            }
            if resp.double_clicked() {
                edits.push(Edit {
                    param: *param,
                    value: info.default,
                    ended: true,
                });
            }
        }
    }

    // The amplifier and cabinet choices, and where the microphone stands.
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let choices = |ui: &mut Ui,
                       param: usize,
                       label: &str,
                       names: &[&'static str],
                       current: usize,
                       edits: &mut Vec<Edit>| {
            ui.label(label);
            let target = AutoTarget::Fx {
                slot: slot as u8,
                param: param as u8,
            };
            ui.add_enabled_ui(automation::editable(track, target), |ui| {
                egui::ComboBox::from_id_salt(("amp_choice", slot, param))
                    .width(140.0)
                    .selected_text(names[current])
                    .show_ui(ui, |ui| {
                        for (i, name) in names.iter().enumerate() {
                            if ui.selectable_label(i == current, *name).clicked() && i != current {
                                edits.push(Edit {
                                    param,
                                    value: i as f32,
                                    ended: true,
                                });
                            }
                        }
                    });
            });
        };
        choices(ui, 0, tr("Amp"), &AMP_MODELS, model, edits);
        ui.add_space(10.0);
        choices(ui, 7, tr("Cabinet"), &AMP_CABS, cab, edits);
        ui.add_space(10.0);
        ui.label(tr("Mic"));
        let target = AutoTarget::Fx {
            slot: slot as u8,
            param: 8,
        };
        let mut mic = shown(insert, track, slot, 8, beat);
        let resp = ui.add_enabled(
            automation::editable(track, target),
            egui::Slider::new(&mut mic, 0.0..=1.0)
                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
        );
        if resp.changed() {
            edits.push(Edit {
                param: 8,
                value: mic,
                ended: !resp.dragged(),
            });
        } else if resp.drag_stopped() {
            edits.push(Edit {
                param: 8,
                value: mic,
                ended: true,
            });
        }
        resp.on_hover_text(tr("From the edge of the cone to its centre"));
    });
}

/// The EQ's gain over 20 Hz to 20 kHz, ±18 dB, with a node for each band.
fn eq_curve(
    ui: &mut Ui,
    kind: FxKind,
    params: &[f32; prowave_engine::MAX_FX_PARAMS],
    accent: Color32,
) {
    inset(ui, 150.0, |g, rect| {
        let x_of = |hz: f32| rect.left() + (hz / 20.0).log10() / 3.0 * rect.width();
        let y_of = |db: f32| rect.center().y - db / 18.0 * rect.height() / 2.0;
        graph_grid(
            g,
            rect,
            &[
                x_of(50.0),
                x_of(100.0),
                x_of(200.0),
                x_of(500.0),
                x_of(1_000.0),
                x_of(2_000.0),
                x_of(5_000.0),
                x_of(10_000.0),
            ]
            .iter()
            .map(|x| (x - rect.left()) / rect.width())
            .collect::<Vec<_>>(),
            &[0.1667, 0.3333, 0.5, 0.6667, 0.8333],
            Color32::from_gray(0x25),
        );
        // The 0 dB line is brighter.
        g.line_segment(
            [pos2(rect.left(), y_of(0.0)), pos2(rect.right(), y_of(0.0))],
            Stroke::new(1.0, Color32::from_gray(0x44)),
        );
        let n = 180;
        let freqs: Vec<f32> = (0..=n)
            .map(|i| 20.0 * 1000f32.powf(i as f32 / n as f32))
            .collect();
        let curve = prowave_engine::response_db(kind, *params, &freqs, CURVE_RATE);
        let pts: Vec<_> = freqs
            .iter()
            .zip(&curve)
            .map(|(f, db)| pos2(x_of(*f), y_of(db.clamp(-18.0, 18.0))))
            .collect();
        // A soft fill under the curve, then the line itself.
        if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
            let mut shape = pts.clone();
            shape.push(pos2(last.x, y_of(0.0)));
            shape.push(pos2(first.x, y_of(0.0)));
            g.add(Shape::convex_polygon(
                shape,
                accent.gamma_multiply(0.10),
                Stroke::NONE,
            ));
        }
        g.add(Shape::line(pts, Stroke::new(1.8, accent)));
        let (low, mid, high) = if kind == FxKind::ChannelEq {
            (
                params[eq_idx::LOW_SHELF_GAIN],
                params[eq_idx::MID_GAIN],
                params[eq_idx::HIGH_SHELF_GAIN],
            )
        } else {
            (params[0], params[6], params[12])
        };
        let mid_f = if kind == FxKind::ChannelEq {
            params[eq_idx::MID_FREQ]
        } else {
            1_000.0
        };
        for (hz, db) in [(70.0, low), (mid_f, mid), (11_000.0, high)] {
            let c = pos2(x_of(hz), y_of(db.clamp(-18.0, 18.0)));
            g.circle_filled(c, 4.5, accent);
            g.circle_stroke(c, 4.5, Stroke::new(1.2, Color32::from_black_alpha(180)));
        }
        g.text(
            pos2(rect.left() + 6.0, rect.top() + 8.0),
            Align2::LEFT_TOP,
            "20 Hz",
            FontId::monospace(9.0),
            FAINT,
        );
        g.text(
            pos2(rect.right() - 6.0, rect.top() + 8.0),
            Align2::RIGHT_TOP,
            "20 kHz",
            FontId::monospace(9.0),
            FAINT,
        );
        g.text(
            pos2(rect.right() - 6.0, rect.bottom() - 6.0),
            Align2::RIGHT_BOTTOM,
            "±18 dB",
            FontId::monospace(9.0),
            FAINT,
        );
    });
}

struct EqBandMeta {
    name: &'static str,
    short: &'static str,
    color: Color32,
    on_idx: usize,
    freq_idx: usize,
    gain_idx: usize, // slope for HPF/LPF
    q_idx: usize,
    is_cut: bool,
}

const EQ_BANDS_META: [EqBandMeta; 8] = [
    EqBandMeta {
        name: "High Pass Filter",
        short: "HP",
        color: Color32::from_rgb(0xe0, 0x53, 0x38), // Orange
        on_idx: eq_idx::HPF_ON,
        freq_idx: eq_idx::HPF_FREQ,
        gain_idx: eq_idx::HPF_SLOPE,
        q_idx: eq_idx::HPF_Q,
        is_cut: true,
    },
    EqBandMeta {
        name: "Low Shelf",
        short: "Low Shelf",
        color: Color32::from_rgb(0xd9, 0x77, 0x06), // Amber
        on_idx: eq_idx::LOW_SHELF_ON,
        freq_idx: eq_idx::LOW_SHELF_FREQ,
        gain_idx: eq_idx::LOW_SHELF_GAIN,
        q_idx: eq_idx::LOW_SHELF_Q,
        is_cut: false,
    },
    EqBandMeta {
        name: "Low-Mid Bell",
        short: "L-Mid",
        color: Color32::from_rgb(0x84, 0xcc, 0x16), // Lime
        on_idx: eq_idx::LOW_MID_ON,
        freq_idx: eq_idx::LOW_MID_FREQ,
        gain_idx: eq_idx::LOW_MID_GAIN,
        q_idx: eq_idx::LOW_MID_Q,
        is_cut: false,
    },
    EqBandMeta {
        name: "Mid Bell",
        short: "Mid",
        color: Color32::from_rgb(0x22, 0xc5, 0x5e), // Green
        on_idx: eq_idx::MID_ON,
        freq_idx: eq_idx::MID_FREQ,
        gain_idx: eq_idx::MID_GAIN,
        q_idx: eq_idx::MID_Q,
        is_cut: false,
    },
    EqBandMeta {
        name: "High-Mid Bell",
        short: "H-Mid",
        color: Color32::from_rgb(0x06, 0xb6, 0xd4), // Cyan
        on_idx: eq_idx::HIGH_MID_ON,
        freq_idx: eq_idx::HIGH_MID_FREQ,
        gain_idx: eq_idx::HIGH_MID_GAIN,
        q_idx: eq_idx::HIGH_MID_Q,
        is_cut: false,
    },
    EqBandMeta {
        name: "Upper-Mid Bell",
        short: "U-Mid",
        color: Color32::from_rgb(0x3b, 0x82, 0xf6), // Blue
        on_idx: eq_idx::UPPER_MID_ON,
        freq_idx: eq_idx::UPPER_MID_FREQ,
        gain_idx: eq_idx::UPPER_MID_GAIN,
        q_idx: eq_idx::UPPER_MID_Q,
        is_cut: false,
    },
    EqBandMeta {
        name: "High Shelf",
        short: "Hi Shelf",
        color: Color32::from_rgb(0x8b, 0x5c, 0xf6), // Violet
        on_idx: eq_idx::HIGH_SHELF_ON,
        freq_idx: eq_idx::HIGH_SHELF_FREQ,
        gain_idx: eq_idx::HIGH_SHELF_GAIN,
        q_idx: eq_idx::HIGH_SHELF_Q,
        is_cut: false,
    },
    EqBandMeta {
        name: "Low Pass Filter",
        short: "LP",
        color: Color32::from_rgb(0xec, 0x48, 0x99), // Magenta
        on_idx: eq_idx::LPF_ON,
        freq_idx: eq_idx::LPF_FREQ,
        gain_idx: eq_idx::LPF_SLOPE,
        q_idx: eq_idx::LPF_Q,
        is_cut: true,
    },
];

const EQ_SLOPES: [&str; 6] = [
    "6 dB/Oct",
    "12 dB/Oct",
    "18 dB/Oct",
    "24 dB/Oct",
    "36 dB/Oct",
    "48 dB/Oct",
];

const PROCESSING_MODES: [&str; 5] = ["Stereo", "Left Only", "Right Only", "Mid Only", "Side Only"];
const ANALYZER_MODES: [&str; 3] = ["Analyzer: Off", "Analyzer: Pre", "Analyzer: Post"];

fn format_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else {
        format!("{:.0} Hz", hz)
    }
}

fn format_gain(db: f32) -> String {
    if db.abs() < 0.05 {
        "0.0 dB".to_string()
    } else {
        format!("{:+.1} dB", db)
    }
}

fn format_q(q: f32) -> String {
    format!("{:.2}", q)
}

fn draw_filter_icon(g: &Painter, r: Rect, band_idx: usize, stroke: Stroke) {
    let c = r.center();
    let left = r.left() + 7.0;
    let right = r.right() - 7.0;
    let top = r.top() + 6.0;
    let bottom = r.bottom() - 6.0;
    let mid_y = c.y;

    match band_idx {
        0 => {
            g.add(Shape::line(
                vec![
                    pos2(left, bottom),
                    pos2(left + (right - left) * 0.35, bottom),
                    pos2(left + (right - left) * 0.65, top + 2.0),
                    pos2(right, top + 2.0),
                ],
                stroke,
            ));
        }
        1 => {
            g.add(Shape::line(
                vec![
                    pos2(left, bottom - 2.0),
                    pos2(c.x - 4.0, bottom - 2.0),
                    pos2(c.x + 4.0, mid_y),
                    pos2(right, mid_y),
                ],
                stroke,
            ));
        }
        2 | 3 | 4 | 5 => {
            g.add(Shape::line(
                vec![
                    pos2(left, mid_y + 4.0),
                    pos2(c.x - 5.0, mid_y + 4.0),
                    pos2(c.x, top + 1.0),
                    pos2(c.x + 5.0, mid_y + 4.0),
                    pos2(right, mid_y + 4.0),
                ],
                stroke,
            ));
        }
        6 => {
            g.add(Shape::line(
                vec![
                    pos2(left, mid_y),
                    pos2(c.x - 4.0, mid_y),
                    pos2(c.x + 4.0, bottom - 2.0),
                    pos2(right, bottom - 2.0),
                ],
                stroke,
            ));
        }
        _ => {
            g.add(Shape::line(
                vec![
                    pos2(left, top + 2.0),
                    pos2(left + (right - left) * 0.35, top + 2.0),
                    pos2(left + (right - left) * 0.65, bottom),
                    pos2(right, bottom),
                ],
                stroke,
            ));
        }
    }
}

/// Matrix parameter scrubber with vertical cursor and hover border highlight.
fn eq_matrix_scrubber(
    ui: &mut Ui,
    r: Rect,
    id: egui::Id,
    val_text: &str,
    is_active: bool,
) -> egui::Response {
    let resp = ui.interact(r, id, Sense::click_and_drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let (bg, stroke_col) = if resp.dragged() {
        (
            Color32::from_rgb(0x22, 0x24, 0x2a),
            Color32::from_rgb(0x12, 0x69, 0xcc),
        )
    } else if resp.hovered() {
        (
            Color32::from_rgb(0x26, 0x28, 0x2f),
            Color32::from_rgb(0x58, 0x58, 0x5e),
        )
    } else {
        (
            Color32::from_rgb(0x1a, 0x1b, 0x1e),
            Color32::from_rgb(0x30, 0x33, 0x3a),
        )
    };
    let g = ui.painter();
    g.rect_filled(r, CornerRadius::same(3), bg);
    g.rect_stroke(
        r,
        CornerRadius::same(3),
        Stroke::new(1.0, stroke_col),
        StrokeKind::Inside,
    );
    let text_col = if is_active {
        Color32::from_rgb(0xe0, 0xe2, 0xe8)
    } else {
        Color32::from_rgb(0x71, 0x71, 0x7a)
    };
    g.text(
        r.center(),
        Align2::CENTER_CENTER,
        val_text,
        FontId::monospace(10.0),
        text_col,
    );
    resp
}

fn eq_set_param(
    track: Option<&Track>,
    slot: usize,
    insert: &mut Insert,
    edits: &mut Vec<Edit>,
    param: usize,
    val: f32,
    ended: bool,
) {
    let target = AutoTarget::Fx {
        slot: slot as u8,
        param: param as u8,
    };
    if track.is_none_or(|t| automation::editable(t, target)) {
        insert.params[param] = val;
        edits.push(Edit {
            param,
            value: val,
            ended,
        });
    }
}

/// Authentic Logic Pro 8-Band Channel EQ window.
fn channel_eq_window(
    ui: &mut Ui,
    insert: &mut Insert,
    track: &Track,
    slot: usize,
    _beat: f64,
    input_level: f32,
    edits: &mut Vec<Edit>,
) {
    render_channel_eq_editor(ui, insert, Some(track), slot, input_level, edits, false);
}

/// Shared 8-band Channel EQ editor used by both the floating Channel EQ window and the
/// compact Macro Controls `EQ` tab.
pub(crate) fn render_channel_eq_editor(
    ui: &mut Ui,
    insert: &mut Insert,
    track: Option<&Track>,
    slot: usize,
    input_level: f32,
    edits: &mut Vec<Edit>,
    compact: bool,
) {
    let sel_id = ui.scope_id().with("selected_eq_band");
    let mut selected_band: usize = ui
        .data_mut(|d| *d.get_temp_mut_or_insert_with(sel_id, || 3))
        .min(7);

    let band_btn_size = if compact {
        vec2(32.0, 22.0)
    } else {
        vec2(38.0, 26.0)
    };
    let ctrl_btn_h = if compact { 22.0 } else { 24.0 };
    let gap_v = if compact { 4.0 } else { 8.0 };
    let matrix_h = if compact { 76.0 } else { 98.0 };
    let scrub_h = if compact { 16.0 } else { 18.0 };
    let row_step = if compact { 18.0 } else { 22.0 };
    let hdr_step = if compact { 15.0 } else { 18.0 };

    // -------------------------------------------------------------
    // Top Filter Icon Strip & Global Toolbar
    // -------------------------------------------------------------
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

        // 8 Band Filter Buttons
        for (i, meta) in EQ_BANDS_META.iter().enumerate() {
            let is_on = insert.params[meta.on_idx] > 0.5;
            let is_selected = i == selected_band;
            let (rect, resp) = ui.allocate_exact_size(band_btn_size, Sense::click());
            let g = ui.painter();

            let fill = if is_selected {
                Color32::from_rgb(0x2a, 0x2c, 0x32)
            } else if resp.hovered() {
                Color32::from_rgb(0x24, 0x26, 0x2b)
            } else {
                Color32::from_rgb(0x1c, 0x1d, 0x21)
            };
            g.rect_filled(rect, CornerRadius::same(4), fill);

            let stroke_border = if is_selected {
                Stroke::new(1.2, Color32::WHITE)
            } else if resp.hovered() {
                Stroke::new(1.0, Color32::from_rgb(0x50, 0x50, 0x55))
            } else {
                Stroke::new(1.0, Color32::from_rgb(0x30, 0x33, 0x3a))
            };
            g.rect_stroke(
                rect,
                CornerRadius::same(4),
                stroke_border,
                StrokeKind::Inside,
            );

            let icon_stroke = Stroke::new(
                if is_on { 1.6 } else { 1.0 },
                if is_on {
                    meta.color
                } else {
                    Color32::from_rgb(0x55, 0x55, 0x5a)
                },
            );
            draw_filter_icon(g, rect, i, icon_stroke);

            if resp.double_clicked() {
                eq_set_param(
                    track,
                    slot,
                    insert,
                    edits,
                    meta.on_idx,
                    if is_on { 0.0 } else { 1.0 },
                    true,
                );
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            } else if resp.clicked() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }
            resp.on_hover_text(format!("{} (double-click to toggle)", tr(meta.name)));
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Processing mode selector
            let proc_mode = insert.params[eq_idx::PROCESSING].clamp(0.0, 4.0).round() as usize;
            let proc_label = PROCESSING_MODES[proc_mode.min(4)];
            let proc_resp = ui.add(
                egui::Button::new(RichText::new(proc_label).size(10.0))
                    .fill(Color32::from_rgb(0x25, 0x26, 0x2b))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x40)))
                    .min_size(vec2(76.0, ctrl_btn_h)),
            );
            egui::Popup::menu(&proc_resp).show(|ui| {
                for (idx, name) in PROCESSING_MODES.iter().enumerate() {
                    if ui.selectable_label(idx == proc_mode, *name).clicked() {
                        eq_set_param(
                            track,
                            slot,
                            insert,
                            edits,
                            eq_idx::PROCESSING,
                            idx as f32,
                            true,
                        );
                        ui.close();
                    }
                }
            });

            // Analyzer mode selector
            let ana_mode = insert.params[eq_idx::ANALYZER].clamp(0.0, 2.0).round() as usize;
            let ana_label = ANALYZER_MODES[ana_mode.min(2)];
            let ana_resp = ui.add(
                egui::Button::new(RichText::new(ana_label).size(10.0))
                    .fill(Color32::from_rgb(0x25, 0x26, 0x2b))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x40)))
                    .min_size(vec2(90.0, ctrl_btn_h)),
            );
            egui::Popup::menu(&ana_resp).show(|ui| {
                for (idx, name) in ANALYZER_MODES.iter().enumerate() {
                    if ui.selectable_label(idx == ana_mode, *name).clicked() {
                        eq_set_param(
                            track,
                            slot,
                            insert,
                            edits,
                            eq_idx::ANALYZER,
                            idx as f32,
                            true,
                        );
                        ui.close();
                    }
                }
            });

            // HQ Toggle
            let hq_on = insert.params[eq_idx::HQ] > 0.5;
            if ui
                .add(
                    egui::Button::new(RichText::new("HQ").size(10.0).color(if hq_on {
                        Color32::WHITE
                    } else {
                        DIM
                    }))
                    .fill(if hq_on {
                        Color32::from_rgb(0x12, 0x69, 0xcc)
                    } else {
                        Color32::from_rgb(0x25, 0x26, 0x2b)
                    })
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x40)))
                    .min_size(vec2(34.0, ctrl_btn_h)),
                )
                .clicked()
            {
                eq_set_param(
                    track,
                    slot,
                    insert,
                    edits,
                    eq_idx::HQ,
                    if hq_on { 0.0 } else { 1.0 },
                    true,
                );
            }

            // Q-Couple Toggle
            let q_couple_on = insert.params[eq_idx::Q_COUPLE] > 0.5;
            if ui
                .add(
                    egui::Button::new(RichText::new("Q-Couple").size(10.0).color(if q_couple_on {
                        Color32::WHITE
                    } else {
                        DIM
                    }))
                    .fill(if q_couple_on {
                        Color32::from_rgb(0x12, 0x69, 0xcc)
                    } else {
                        Color32::from_rgb(0x25, 0x26, 0x2b)
                    })
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x40)))
                    .min_size(vec2(64.0, ctrl_btn_h)),
                )
                .clicked()
            {
                eq_set_param(
                    track,
                    slot,
                    insert,
                    edits,
                    eq_idx::Q_COUPLE,
                    if q_couple_on { 0.0 } else { 1.0 },
                    true,
                );
            }
        });
    });

    ui.add_space(gap_v);

    // -------------------------------------------------------------
    // Center Frequency Graph + Master Gain Fader
    // -------------------------------------------------------------
    let full_w = ui.available_width();
    let fader_w = 42.0;
    let graph_w = (full_w - fader_w - 8.0).max(100.0);
    let graph_h = if compact {
        (ui.available_height() - matrix_h - gap_v * 2.0 - 4.0).clamp(72.0, 210.0)
    } else {
        210.0
    };

    ui.horizontal(|ui| {
        // Log Frequency Plot Rect
        let (plot_rect, _) = ui.allocate_exact_size(vec2(graph_w, graph_h), Sense::hover());
        let g = ui.painter().with_clip_rect(plot_rect);

        g.rect_filled(plot_rect, CornerRadius::same(6), INSET);
        g.rect_stroke(
            plot_rect,
            CornerRadius::same(6),
            Stroke::new(1.0, LINE),
            StrokeKind::Inside,
        );

        let x_of = |hz: f32| {
            plot_rect.left() + (hz.clamp(20.0, 20000.0) / 20.0).log10() / 3.0 * plot_rect.width()
        };
        let hz_of = |x: f32| {
            20.0 * 10.0f32.powf(((x - plot_rect.left()) / plot_rect.width()).clamp(0.0, 1.0) * 3.0)
        };
        let y_of = |db: f32| {
            plot_rect.center().y - (db.clamp(-24.0, 24.0) / 24.0) * (plot_rect.height() * 0.5)
        };
        let db_of = |y: f32| -((y - plot_rect.center().y) / (plot_rect.height() * 0.5)) * 24.0;

        // Vertical Frequency Grid Lines
        let grid_freqs = [
            50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ];
        for f in grid_freqs {
            let x = x_of(f);
            g.line_segment(
                [pos2(x, plot_rect.top()), pos2(x, plot_rect.bottom())],
                Stroke::new(1.0, Color32::from_gray(0x22)),
            );
            let lbl = if f >= 1000.0 {
                format!("{:.0}k", f / 1000.0)
            } else {
                format!("{:.0}", f)
            };
            g.text(
                pos2(x, plot_rect.bottom() - 6.0),
                Align2::CENTER_BOTTOM,
                lbl,
                FontId::monospace(8.5),
                FAINT,
            );
        }

        // Horizontal dB Grid Lines
        for db in [-18.0, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
            let y = y_of(db);
            let stroke_grid = if db == 0.0 {
                Stroke::new(1.0, Color32::from_gray(0x44))
            } else {
                Stroke::new(1.0, Color32::from_gray(0x22))
            };
            g.line_segment(
                [pos2(plot_rect.left(), y), pos2(plot_rect.right(), y)],
                stroke_grid,
            );
            if db != 0.0 {
                g.text(
                    pos2(plot_rect.left() + 4.0, y),
                    Align2::LEFT_CENTER,
                    format!("{:+.0}", db),
                    FontId::monospace(8.0),
                    Color32::from_gray(0x55),
                );
            }
        }
        g.text(
            pos2(plot_rect.left() + 4.0, y_of(0.0)),
            Align2::LEFT_CENTER,
            "0 dB",
            FontId::monospace(8.0),
            Color32::from_gray(0x77),
        );

        // Q-Bandwidth Highlight for Selected Band
        if selected_band < EQ_BANDS_META.len() {
            let meta = &EQ_BANDS_META[selected_band];
            if insert.params[meta.on_idx] > 0.5 {
                let f0 = insert.params[meta.freq_idx];
                let q = insert.params[meta.q_idx].max(0.1);
                let gain = if meta.is_cut {
                    0.0
                } else {
                    insert.params[meta.gain_idx]
                };
                if gain.abs() > 0.1 || meta.is_cut {
                    let bw_ratio = 1.0 + 0.35 / q;
                    let x_lo = x_of(f0 / bw_ratio);
                    let x_hi = x_of(f0 * bw_ratio);
                    let bw_rect = Rect::from_min_max(
                        pos2(x_lo, plot_rect.top()),
                        pos2(x_hi, plot_rect.bottom()),
                    );
                    g.rect_filled(bw_rect, 0.0, meta.color.gamma_multiply(0.07));
                }
            }
        }

        // Live Analyzer Spectrum Display
        if insert.params[eq_idx::ANALYZER] > 0.5 && input_level > 0.001 {
            let bars = 48;
            for b in 0..bars {
                let frac = b as f32 / bars as f32;
                let hz = 20.0 * 1000.0f32.powf(frac);
                let x = x_of(hz);
                // Spectral tilt simulation based on input level
                let tilt = 1.0 - frac * 0.5;
                let mag =
                    (input_level * tilt * (1.0 + 0.3 * (b as f32 * 1.7).sin())).clamp(0.0, 1.0);
                let h = mag * (plot_rect.height() * 0.7);
                let bar_rect = Rect::from_min_max(
                    pos2(x - 2.0, plot_rect.bottom() - h),
                    pos2(x + 2.0, plot_rect.bottom()),
                );
                g.rect_filled(
                    bar_rect,
                    1.0,
                    Color32::from_rgba_unmultiplied(111, 178, 255, 30),
                );
            }
        }

        // Composite 8-Band Frequency Response Curve
        let n_pts = 200;
        let freqs: Vec<f32> = (0..=n_pts)
            .map(|i| 20.0 * 1000f32.powf(i as f32 / n_pts as f32))
            .collect();
        let curve_dbs = response_db(FxKind::ChannelEq, insert.params, &freqs, CURVE_RATE);
        let pts: Vec<Pos2> = freqs
            .iter()
            .zip(&curve_dbs)
            .map(|(f, db)| pos2(x_of(*f), y_of(*db)))
            .collect();

        // Curve polygon fill
        if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
            let mut shape = pts.clone();
            shape.push(pos2(last.x, y_of(0.0)));
            shape.push(pos2(first.x, y_of(0.0)));
            let fill_col = if insert.bypass {
                Color32::from_rgba_unmultiplied(90, 90, 95, 15)
            } else {
                Color32::from_rgba_unmultiplied(111, 178, 255, 35)
            };
            g.add(Shape::convex_polygon(shape, fill_col, Stroke::NONE));
        }

        // Curve stroke
        let curve_stroke_col = if insert.bypass {
            Color32::from_rgb(0x55, 0x55, 0x58)
        } else {
            Color32::from_rgb(0x6f, 0xb2, 0xff)
        };
        g.add(Shape::line(pts, Stroke::new(2.0, curve_stroke_col)));

        // Interactive Draggable Node Handles (8 Bands)
        for (i, meta) in EQ_BANDS_META.iter().enumerate() {
            let is_on = insert.params[meta.on_idx] > 0.5;
            let f = insert.params[meta.freq_idx];
            let gain = if meta.is_cut {
                0.0
            } else {
                insert.params[meta.gain_idx]
            };
            let q = insert.params[meta.q_idx];
            let node_center = pos2(x_of(f), y_of(gain));
            let handle_r = Rect::from_center_size(node_center, vec2(18.0, 18.0));

            let handle_resp = ui.interact(
                handle_r,
                ui.scope_id().with(("eq_node", i)),
                Sense::click_and_drag(),
            );

            if handle_resp.hovered() || handle_resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            if handle_resp.clicked() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }
            if handle_resp.double_clicked() {
                if !meta.is_cut {
                    eq_set_param(track, slot, insert, edits, meta.gain_idx, 0.0, true);
                } else {
                    eq_set_param(track, slot, insert, edits, meta.gain_idx, 1.0, true);
                    // 12 dB/Oct default
                }
            }
            if handle_resp.dragged() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
                let delta = handle_resp.drag_delta();
                let new_hz = hz_of(node_center.x + delta.x).clamp(20.0, 20000.0);
                eq_set_param(track, slot, insert, edits, meta.freq_idx, new_hz, false);

                if !meta.is_cut {
                    let new_gain =
                        (gain - delta.y / (plot_rect.height() * 0.5) * 24.0).clamp(-24.0, 24.0);
                    eq_set_param(track, slot, insert, edits, meta.gain_idx, new_gain, false);
                } else {
                    let slope_step = (delta.y / 24.0).round() as i32;
                    if slope_step != 0 {
                        let cur_slope = insert.params[meta.gain_idx];
                        let new_slope = (cur_slope - slope_step as f32).clamp(0.0, 5.0);
                        eq_set_param(track, slot, insert, edits, meta.gain_idx, new_slope, false);
                    }
                }
            }
            if handle_resp.drag_stopped() {
                let cur_gain = insert.params[meta.gain_idx];
                eq_set_param(track, slot, insert, edits, meta.freq_idx, f, true);
                eq_set_param(track, slot, insert, edits, meta.gain_idx, cur_gain, true);
            }

            // Scroll wheel adjusts Q
            if handle_resp.hovered() {
                let scroll = ui.input(|inp| inp.smooth_scroll_delta.y);
                if scroll.abs() > 0.1 {
                    let new_q = (q * (1.0 + scroll * 0.005)).clamp(0.1, 10.0);
                    eq_set_param(track, slot, insert, edits, meta.q_idx, new_q, true);
                }
            }

            // Draw Node Handle
            let is_sel = i == selected_band;
            let radius = if handle_resp.hovered() || handle_resp.dragged() || is_sel {
                6.5
            } else {
                5.0
            };
            let fill_color = if is_on {
                meta.color
            } else {
                Color32::from_rgb(0x44, 0x44, 0x48)
            };
            g.circle_filled(node_center, radius, fill_color);
            let ring_stroke = if is_sel || handle_resp.dragged() {
                Stroke::new(1.8, Color32::WHITE)
            } else if handle_resp.hovered() {
                Stroke::new(1.2, Color32::from_rgb(0xe0, 0xe0, 0xe5))
            } else {
                Stroke::new(1.0, Color32::from_black_alpha(180))
            };
            g.circle_stroke(node_center, radius, ring_stroke);

            // Floating Tooltip Badge
            if handle_resp.hovered() || handle_resp.dragged() {
                let gain_str = if meta.is_cut {
                    let s_idx = insert.params[meta.gain_idx].clamp(0.0, 5.0).round() as usize;
                    EQ_SLOPES[s_idx.min(5)].to_string()
                } else {
                    format_gain(gain)
                };
                handle_resp.on_hover_ui(|ui| {
                    ui.label(
                        RichText::new(format!("{}: {}", tr(meta.name), format_hz(f)))
                            .size(11.0)
                            .strong(),
                    );
                    ui.label(RichText::new(format!("Gain: {} · Q: {:.2}", gain_str, q)).size(10.5));
                });
            }
        }

        // Master Output Gain Fader (Right Side)
        let (fader_rect, _) = ui.allocate_exact_size(vec2(fader_w, graph_h), Sense::hover());
        let gf = ui.painter().with_clip_rect(fader_rect);
        gf.rect_filled(fader_rect, CornerRadius::same(6), INSET);
        gf.rect_stroke(
            fader_rect,
            CornerRadius::same(6),
            Stroke::new(1.0, LINE),
            StrokeKind::Inside,
        );

        let master_gain = insert.params[eq_idx::MASTER_GAIN];
        let f_top = fader_rect.top() + 18.0;
        let f_bottom = fader_rect.bottom() - 22.0;
        let f_h = (f_bottom - f_top).max(20.0);
        let f_center_x = fader_rect.center().x;

        // Fader track
        gf.rect_filled(
            Rect::from_center_size(pos2(f_center_x, (f_top + f_bottom) * 0.5), vec2(4.0, f_h)),
            2.0,
            Color32::from_rgb(0x28, 0x29, 0x2e),
        );

        // 0 dB tick
        let mid_y = (f_top + f_bottom) * 0.5;
        gf.line_segment(
            [pos2(f_center_x - 7.0, mid_y), pos2(f_center_x + 7.0, mid_y)],
            Stroke::new(1.0, Color32::from_gray(0x60)),
        );

        let fader_y = mid_y - (master_gain / 24.0) * (f_h * 0.5);
        let cap_rect = Rect::from_center_size(pos2(f_center_x, fader_y), vec2(24.0, 14.0));
        let cap_resp = ui.interact(
            cap_rect.expand(4.0),
            ui.scope_id().with("master_gain_cap"),
            Sense::click_and_drag(),
        );
        if cap_resp.hovered() || cap_resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if cap_resp.double_clicked() {
            eq_set_param(track, slot, insert, edits, eq_idx::MASTER_GAIN, 0.0, true);
        }
        if cap_resp.dragged() {
            let delta = cap_resp.drag_delta().y;
            let new_gain = (master_gain - delta / (f_h * 0.5) * 24.0).clamp(-24.0, 24.0);
            eq_set_param(
                track,
                slot,
                insert,
                edits,
                eq_idx::MASTER_GAIN,
                new_gain,
                false,
            );
        }
        if cap_resp.drag_stopped() {
            eq_set_param(
                track,
                slot,
                insert,
                edits,
                eq_idx::MASTER_GAIN,
                master_gain,
                true,
            );
        }

        // Draw fader cap
        gf.rect_filled(
            cap_rect,
            CornerRadius::same(3),
            if cap_resp.dragged() {
                Color32::from_rgb(0x12, 0x69, 0xcc)
            } else if cap_resp.hovered() {
                Color32::from_rgb(0x48, 0x4a, 0x52)
            } else {
                Color32::from_rgb(0x38, 0x3a, 0x40)
            },
        );
        gf.rect_stroke(
            cap_rect,
            CornerRadius::same(3),
            Stroke::new(1.0, Color32::from_rgb(0x5a, 0x5c, 0x64)),
            StrokeKind::Inside,
        );
        gf.line_segment(
            [
                pos2(cap_rect.left() + 4.0, cap_rect.center().y),
                pos2(cap_rect.right() - 4.0, cap_rect.center().y),
            ],
            Stroke::new(1.2, Color32::WHITE),
        );

        gf.text(
            pos2(f_center_x, fader_rect.top() + 8.0),
            Align2::CENTER_CENTER,
            "Gain",
            FontId::proportional(8.5),
            DIM,
        );
        gf.text(
            pos2(f_center_x, fader_rect.bottom() - 8.0),
            Align2::CENTER_CENTER,
            format_gain(master_gain),
            FontId::monospace(8.0),
            TXT,
        );
    });

    ui.add_space(gap_v);

    // -------------------------------------------------------------
    // Bottom 8-Column Parameter Matrix
    // -------------------------------------------------------------
    let matrix_w = ui.available_width();
    let col_w = (matrix_w / 8.0).floor();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

        for (i, meta) in EQ_BANDS_META.iter().enumerate() {
            let (col_rect, col_resp) =
                ui.allocate_exact_size(vec2(col_w, matrix_h), Sense::click());

            let is_on = insert.params[meta.on_idx] > 0.5;
            let is_sel = i == selected_band;

            let col_bg = if is_sel {
                Color32::from_rgb(0x23, 0x25, 0x2a)
            } else if col_resp.hovered() {
                Color32::from_rgb(0x1f, 0x20, 0x24)
            } else {
                Color32::from_rgb(0x18, 0x19, 0x1d)
            };
            ui.painter()
                .rect_filled(col_rect, CornerRadius::same(4), col_bg);
            if is_sel {
                ui.painter().rect_filled(
                    Rect::from_min_size(col_rect.min, vec2(col_rect.width(), 2.0)),
                    0.0,
                    meta.color,
                );
            }
            ui.painter().rect_stroke(
                col_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_rgb(0x2c, 0x2d, 0x33)),
                StrokeKind::Inside,
            );

            if col_resp.clicked() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }

            let pad = 4.0;
            let row_w = col_rect.width() - pad * 2.0;
            let x0 = col_rect.left() + pad;
            let mut y = col_rect.top() + if compact { 4.0 } else { 6.0 };

            // Column Header: Power Toggle + Short Name
            let p_center = pos2(x0 + 6.0, y + 6.0);
            let p_resp = ui.interact(
                Rect::from_center_size(p_center, vec2(12.0, 12.0)),
                ui.scope_id().with(("pwr", i)),
                Sense::click(),
            );
            if p_resp.clicked() {
                eq_set_param(
                    track,
                    slot,
                    insert,
                    edits,
                    meta.on_idx,
                    if is_on { 0.0 } else { 1.0 },
                    true,
                );
            }
            ui.painter().circle_filled(
                p_center,
                4.0,
                if is_on {
                    meta.color
                } else {
                    Color32::from_rgb(0x40, 0x40, 0x45)
                },
            );

            ui.painter().text(
                pos2(x0 + 15.0, y + 6.0),
                Align2::LEFT_CENTER,
                meta.short,
                FontId::proportional(10.0),
                if is_on { TXT } else { FAINT },
            );
            y += hdr_step;

            // Row 1: Frequency Scrubber
            let freq_r = Rect::from_min_size(pos2(x0, y), vec2(row_w, scrub_h));
            let f = insert.params[meta.freq_idx];
            let f_resp = eq_matrix_scrubber(
                ui,
                freq_r,
                ui.scope_id().with(("scrub_f", i)),
                &format_hz(f),
                is_on,
            );
            if f_resp.clicked() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }
            if f_resp.double_clicked() {
                let def = insert.kind.defaults()[meta.freq_idx];
                eq_set_param(track, slot, insert, edits, meta.freq_idx, def, true);
            }
            if f_resp.dragged() {
                let delta = f_resp.drag_delta().y;
                let new_f = (f * (1.0 - delta * 0.015)).clamp(20.0, 20000.0);
                eq_set_param(track, slot, insert, edits, meta.freq_idx, new_f, false);
            }
            if f_resp.drag_stopped() {
                eq_set_param(track, slot, insert, edits, meta.freq_idx, f, true);
            }
            y += row_step;

            // Row 2: Gain / Slope Scrubber
            let gain_r = Rect::from_min_size(pos2(x0, y), vec2(row_w, scrub_h));
            let gain_val = insert.params[meta.gain_idx];
            let gain_text = if meta.is_cut {
                let s_idx = gain_val.clamp(0.0, 5.0).round() as usize;
                EQ_SLOPES[s_idx.min(5)].to_string()
            } else {
                format_gain(gain_val)
            };
            let g_resp = eq_matrix_scrubber(
                ui,
                gain_r,
                ui.scope_id().with(("scrub_g", i)),
                &gain_text,
                is_on,
            );
            if g_resp.clicked() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }
            if g_resp.double_clicked() {
                let def = insert.kind.defaults()[meta.gain_idx];
                eq_set_param(track, slot, insert, edits, meta.gain_idx, def, true);
            }
            if g_resp.dragged() {
                let delta = g_resp.drag_delta().y;
                if !meta.is_cut {
                    let new_g = (gain_val - delta * 0.3).clamp(-24.0, 24.0);
                    eq_set_param(track, slot, insert, edits, meta.gain_idx, new_g, false);
                } else {
                    let s_delta = (delta / 12.0).round() as i32;
                    if s_delta != 0 {
                        let new_s = (gain_val - s_delta as f32).clamp(0.0, 5.0);
                        eq_set_param(track, slot, insert, edits, meta.gain_idx, new_s, false);
                    }
                }
            }
            if g_resp.drag_stopped() {
                eq_set_param(track, slot, insert, edits, meta.gain_idx, gain_val, true);
            }
            y += row_step;

            // Row 3: Q Scrubber
            let q_r = Rect::from_min_size(pos2(x0, y), vec2(row_w, scrub_h));
            let q_val = insert.params[meta.q_idx];
            let q_resp = eq_matrix_scrubber(
                ui,
                q_r,
                ui.scope_id().with(("scrub_q", i)),
                &format_q(q_val),
                is_on,
            );
            if q_resp.clicked() {
                selected_band = i;
                ui.data_mut(|d| d.insert_temp(sel_id, i));
            }
            if q_resp.double_clicked() {
                let def = insert.kind.defaults()[meta.q_idx];
                eq_set_param(track, slot, insert, edits, meta.q_idx, def, true);
            }
            if q_resp.dragged() {
                let delta = q_resp.drag_delta().y;
                let new_q = (q_val * (1.0 - delta * 0.015)).clamp(0.1, 10.0);
                eq_set_param(track, slot, insert, edits, meta.q_idx, new_q, false);
            }
            if q_resp.drag_stopped() {
                eq_set_param(track, slot, insert, edits, meta.q_idx, q_val, true);
            }
        }
    });
}

/// Compressor, gate and limiter: the static transfer curve plus live gain reduction.
fn transfer_curve(
    ui: &mut Ui,
    kind: FxKind,
    params: &[f32; prowave_engine::MAX_FX_PARAMS],
    reduction: f32,
    accent: Color32,
) {
    inset(ui, 150.0, |g, rect| {
        let plot = Rect::from_min_max(
            pos2(rect.left() + 10.0, rect.top() + 12.0),
            pos2(rect.right() - 58.0, rect.bottom() - 12.0),
        );
        graph_grid(
            g,
            plot,
            &[0.0, 0.25, 0.5, 0.75, 1.0],
            &[0.0, 0.25, 0.5, 0.75, 1.0],
            Color32::from_gray(0x25),
        );
        // The unity diagonal, faint.
        g.line_segment(
            [plot.left_bottom(), plot.right_top()],
            Stroke::new(1.0, Color32::from_gray(0x38)),
        );
        // A gate's threshold can sit at -80 dB, so it gets the wider scale.
        let span = if kind == FxKind::Gate { 80.0 } else { 60.0 };
        let db_x = |x: f32| plot.left() + ((x.clamp(-span, 0.0) + span) / span) * plot.width();
        let db_y = |y: f32| plot.bottom() - ((y.clamp(-span, 0.0) + span) / span) * plot.height();
        let curve: Vec<Pos2> = (0..=120)
            .map(|i| {
                let x = -span + span * i as f32 / 120.0;
                let y = match kind {
                    FxKind::Compressor => {
                        let threshold = params[0];
                        let ratio = params[1].max(1.0);
                        let makeup = params[4];
                        let y = if x <= threshold {
                            x
                        } else {
                            threshold + (x - threshold) / ratio
                        };
                        y + makeup
                    }
                    FxKind::Gate => {
                        let threshold = params[0];
                        let range = params[1];
                        if x >= threshold {
                            x
                        } else {
                            x + range
                        }
                    }
                    _ => (x + params[0]).min(params[1]),
                };
                pos2(db_x(x), db_y(y))
            })
            .collect();
        g.add(Shape::line(curve, Stroke::new(2.0, accent)));
        // The threshold (dynamics) or ceiling (limiter) that shapes the curve.
        match kind {
            FxKind::Compressor | FxKind::Gate => {
                let threshold = params[0].clamp(-span, 0.0);
                let x = db_x(threshold);
                g.line_segment(
                    [pos2(x, plot.top()), pos2(x, plot.bottom())],
                    Stroke::new(1.0, accent.gamma_multiply(0.35)),
                );
                g.text(
                    pos2(x + 3.0, plot.top() + 2.0),
                    Align2::LEFT_TOP,
                    format!("{threshold:.0} dB"),
                    FontId::monospace(8.0),
                    accent,
                );
            }
            _ => {
                let ceiling = params[1].clamp(-span, 0.0);
                let y = db_y(ceiling);
                g.line_segment(
                    [pos2(plot.left(), y), pos2(plot.right(), y)],
                    Stroke::new(1.0, accent.gamma_multiply(0.35)),
                );
                g.text(
                    pos2(plot.right() - 2.0, y - 2.0),
                    Align2::RIGHT_BOTTOM,
                    format!("{ceiling:.1} dB"),
                    FontId::monospace(8.0),
                    accent,
                );
            }
        }
        // The live gain-reduction meter on the right.
        let meter = Rect::from_min_max(
            pos2(rect.right() - 34.0, plot.top()),
            pos2(rect.right() - 18.0, plot.bottom()),
        );
        g.rect_filled(meter, CornerRadius::same(2), Color32::from_gray(0x0c));
        let amount = reduction.clamp(0.0, 24.0) / 24.0;
        let fill = Rect::from_min_max(
            pos2(meter.left(), meter.top()),
            pos2(meter.right(), meter.top() + meter.height() * amount),
        );
        g.rect_filled(fill, CornerRadius::same(2), accent);
        g.text(
            pos2(rect.left() + 12.0, rect.bottom() - 6.0),
            Align2::LEFT_BOTTOM,
            trf(
                "Gain Reduction: {} dB",
                &[&format!("{:.1}", reduction.clamp(0.0, 24.0))],
            ),
            FontId::monospace(8.5),
            accent,
        );
        g.text(
            pos2(plot.left(), plot.top() - 10.0),
            Align2::LEFT_BOTTOM,
            "IN",
            FontId::monospace(8.5),
            FAINT,
        );
        g.text(
            pos2(plot.right(), plot.top() - 10.0),
            Align2::RIGHT_BOTTOM,
            "OUT",
            FontId::monospace(8.5),
            FAINT,
        );
    });
}

/// The three-band compressor: where the bands split and how each one is set.
fn multicomp(
    ui: &mut Ui,
    params: &[f32; prowave_engine::MAX_FX_PARAMS],
    reduction: f32,
    accent: Color32,
) {
    inset(ui, 150.0, |g, rect| {
        let plot = rect.shrink2(vec2(12.0, 18.0));
        let low_mid = params[3].clamp(20.0, 20_000.0);
        let mid_high = params[7].clamp(low_mid, 20_000.0);
        let x = |hz: f32| plot.left() + (hz / 20.0).log10() / 3.0 * plot.width();
        let bands = [
            (plot.left(), x(low_mid), "LOW", params[0], params[1]),
            (x(low_mid), x(mid_high), "MID", params[4], params[5]),
            (x(mid_high), plot.right(), "HIGH", params[8], params[9]),
        ];
        for (a, b, name, threshold, ratio) in bands {
            let band = Rect::from_x_y_ranges(a..=b, plot.top()..=plot.bottom());
            g.rect_filled(
                band.shrink(2.0),
                CornerRadius::same(4),
                accent.gamma_multiply(0.08),
            );
            g.rect_stroke(
                band.shrink(2.0),
                CornerRadius::same(4),
                Stroke::new(1.0, accent.gamma_multiply(0.35)),
                StrokeKind::Inside,
            );
            g.text(
                band.center() - vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                name,
                FontId::monospace(10.0),
                accent,
            );
            g.text(
                band.center() + vec2(0.0, 6.0),
                Align2::CENTER_CENTER,
                format!("{:.0} dB  {:.1}:1", threshold, ratio),
                FontId::monospace(8.5),
                DIM,
            );
        }
        for crossover in [low_mid, mid_high] {
            let cx = x(crossover);
            g.line_segment(
                [pos2(cx, plot.top()), pos2(cx, plot.bottom())],
                Stroke::new(1.0, Color32::from_gray(0x4a)),
            );
            g.text(
                pos2(cx, plot.top() - 4.0),
                Align2::CENTER_BOTTOM,
                format!("{crossover:.0} Hz"),
                FontId::monospace(8.0),
                FAINT,
            );
        }
        let meter = Rect::from_min_max(
            pos2(rect.right() - 16.0, plot.top()),
            pos2(rect.right() - 6.0, plot.bottom()),
        );
        g.rect_filled(meter, CornerRadius::same(2), Color32::from_gray(0x0c));
        let amount = reduction.clamp(0.0, 24.0) / 24.0;
        g.rect_filled(
            Rect::from_min_max(
                pos2(meter.left(), meter.top()),
                pos2(meter.right(), meter.top() + meter.height() * amount),
            ),
            CornerRadius::same(2),
            accent,
        );
    });
}

/// The delay line: taps at multiples of the delay, with feedback fading them out.
fn delay_panel(ui: &mut Ui, params: &[f32; prowave_engine::MAX_FX_PARAMS], accent: Color32) {
    inset(ui, 150.0, |g, rect| {
        let time = params[0].max(1.0);
        let feedback = params[1].clamp(0.0, 0.95);
        let ping = params[3] >= 0.5;
        let mix = params[4].clamp(0.0, 1.0);
        let mid = rect.center().y;
        g.line_segment(
            [
                pos2(rect.left() + 16.0, mid),
                pos2(rect.right() - 16.0, mid),
            ],
            Stroke::new(1.0, Color32::from_gray(0x3a)),
        );
        g.text(
            pos2(rect.left() + 16.0, mid - 10.0),
            Align2::LEFT_BOTTOM,
            "dry",
            FontId::monospace(8.5),
            DIM,
        );
        let span = rect.width() - 52.0;
        let last = 4.0f32;
        for n in 1..=4 {
            let frac = (time * n as f32 / 1000.0 / (time * last / 1000.0)).min(1.0);
            let x = rect.left() + 26.0 + span * frac;
            let level = feedback.powi(n - 1) * mix.max(0.1);
            let y = if ping && n % 2 == 0 {
                mid - 26.0
            } else {
                mid + 12.0
            };
            let alpha = (40.0 + 180.0 * level).min(220.0) as u8;
            g.circle_filled(pos2(x, y), 5.5, accent.gamma_multiply(alpha as f32 / 255.0));
            g.line_segment(
                [pos2(x, mid), pos2(x, y)],
                Stroke::new(1.0, accent.gamma_multiply(0.25)),
            );
            g.text(
                pos2(x, y + 9.0),
                Align2::CENTER_TOP,
                format!("{:.0}", time * n as f32),
                FontId::monospace(8.0),
                DIM,
            );
        }
        g.text(
            pos2(rect.right() - 16.0, mid - 10.0),
            Align2::RIGHT_BOTTOM,
            format!("{:.0} ms · fb {:.0}%", time, feedback * 100.0),
            FontId::monospace(8.5),
            DIM,
        );
    });
}

/// Chorus: two slow LFOs, one per side.
fn chorus_panel(ui: &mut Ui, params: &[f32; prowave_engine::MAX_FX_PARAMS], accent: Color32) {
    inset(ui, 150.0, |g, rect| {
        let rate = params[0].max(0.01);
        let depth = params[1].clamp(0.0, 8.0) / 8.0;
        let mid = rect.center().y;
        let amp = rect.height() * 0.3 * depth;
        for (offset, alpha) in [(0.0f32, 0.9f32), (std::f32::consts::PI, 0.55f32)] {
            let pts: Vec<Pos2> = (0..=140)
                .map(|i| {
                    let t = i as f32 / 140.0;
                    let a = t * rate * std::f32::consts::TAU + offset;
                    pos2(
                        rect.left() + 10.0 + t * (rect.width() - 20.0),
                        mid + a.sin() * amp,
                    )
                })
                .collect();
            g.add(Shape::line(
                pts,
                Stroke::new(1.6, accent.gamma_multiply(alpha)),
            ));
        }
        g.line_segment(
            [
                pos2(rect.left() + 10.0, mid),
                pos2(rect.right() - 10.0, mid),
            ],
            Stroke::new(1.0, Color32::from_gray(0x30)),
        );
        g.text(
            pos2(rect.left() + 12.0, rect.bottom() - 8.0),
            Align2::LEFT_BOTTOM,
            format!("{:.2} Hz · depth {:.1} ms", rate, params[1]),
            FontId::monospace(8.5),
            DIM,
        );
    });
}

/// Phaser: the notches sweep with the LFO; `beat` drives the animation.
fn phaser_panel(
    ui: &mut Ui,
    params: &[f32; prowave_engine::MAX_FX_PARAMS],
    beat: f64,
    accent: Color32,
) {
    inset(ui, 150.0, |g, rect| {
        let rate = params[0].max(0.01);
        let depth = params[1].clamp(0.0, 1.0);
        let phase = ((beat * f64::from(rate)) as f32).fract() * std::f32::consts::TAU;
        let mid = rect.center().y;
        let pts: Vec<Pos2> = (0..=160)
            .map(|i| {
                let t = i as f32 / 160.0;
                let hz = 20.0 * 1000f32.powf(t);
                let mut gain = 0.0f32;
                for notch in 0..4 {
                    let center = 180.0 * 2.4f32.powi(notch) * (1.0 + 0.35 * depth * phase.sin());
                    let d = (hz / center.max(1.0)).ln().abs();
                    gain -= 14.0 * depth * (-(d * d) / 0.08).exp();
                }
                pos2(
                    rect.left() + 10.0 + t * (rect.width() - 20.0),
                    mid - gain / 28.0 * rect.height(),
                )
            })
            .collect();
        g.add(Shape::line(pts, Stroke::new(1.8, accent)));
        g.text(
            pos2(rect.left() + 12.0, rect.bottom() - 8.0),
            Align2::LEFT_BOTTOM,
            format!("{:.2} Hz · feedback {:.0}%", rate, params[2] * 100.0),
            FontId::monospace(8.5),
            DIM,
        );
    });
}

/// Overdrive: the waveshaper's transfer curve, from the drive and tone.
fn drive_panel(ui: &mut Ui, params: &[f32; prowave_engine::MAX_FX_PARAMS], accent: Color32) {
    inset(ui, 150.0, |g, rect| {
        let plot = rect.shrink2(vec2(24.0, 14.0));
        graph_grid(
            g,
            plot,
            &[0.0, 0.25, 0.5, 0.75, 1.0],
            &[0.0, 0.25, 0.5, 0.75, 1.0],
            Color32::from_gray(0x25),
        );
        g.line_segment(
            [plot.left_bottom(), plot.right_top()],
            Stroke::new(1.0, Color32::from_gray(0x38)),
        );
        let drive = 10f32.powf(params[0].clamp(0.0, 40.0) / 20.0);
        let norm = drive.tanh().max(1e-3);
        let pts: Vec<Pos2> = (0..=140)
            .map(|i| {
                let x = -1.0 + 2.0 * i as f32 / 140.0;
                let y = (x * drive).tanh() / norm;
                pos2(
                    plot.center().x + x * plot.width() / 2.0,
                    plot.center().y - y * plot.height() / 2.0,
                )
            })
            .collect();
        g.add(Shape::line(pts, Stroke::new(2.0, accent)));
        let scope_rect = Rect::from_min_max(
            pos2(rect.right() - 124.0, rect.top() + 10.0),
            pos2(rect.right() - 10.0, rect.top() + 58.0),
        );
        let scope_samples: Vec<f32> = (0..48)
            .map(|i| {
                let ph = i as f32 / 47.0 * std::f32::consts::TAU * 2.0;
                ((ph.sin() * drive).tanh() / norm).clamp(-1.0, 1.0)
            })
            .collect();
        crate::widgets::daw_phosphor_scope(g, scope_rect, &scope_samples);
        g.text(
            pos2(rect.left() + 12.0, rect.bottom() - 8.0),
            Align2::LEFT_BOTTOM,
            format!("drive {:.0} dB · tone {:.0} Hz", params[0], params[1]),
            FontId::monospace(8.5),
            DIM,
        );
    });
}

/// Filter: the magnitude response of the selected mode, with a marker at the cutoff.
fn filter_panel(ui: &mut Ui, params: &[f32; prowave_engine::MAX_FX_PARAMS], accent: Color32) {
    inset(ui, 150.0, |g, rect| {
        let plot = rect.shrink2(vec2(10.0, 12.0));
        let cutoff = params[0].clamp(20.0, 20_000.0);
        let resonance = params[1].clamp(0.5, 10.0);
        let mode = params[2].round().clamp(0.0, 2.0) as usize;
        graph_grid(
            g,
            plot,
            &[0.0, 0.25, 0.5, 0.75, 1.0],
            &[0.0, 0.25, 0.5, 0.75, 1.0],
            Color32::from_gray(0x25),
        );
        let x_of = |hz: f32| plot.left() + (hz / 20.0).log10() / 3.0 * plot.width();
        let pts: Vec<Pos2> = (0..=160)
            .map(|i| {
                let hz = 20.0 * 1000f32.powf(i as f32 / 160.0);
                let x = hz / cutoff;
                let (mut db, peak) = match mode {
                    0 => (-10.0 * (1.0 + x.powi(4)).log10(), 12.0),
                    1 => (-10.0 * (1.0 + (1.0 / x).powi(4)).log10(), 12.0),
                    _ => (-10.0 * (1.0 + ((x - 1.0 / x) * 2.0).powi(2)).log10(), 9.0),
                };
                db += peak * (resonance - 0.5) / 9.5 * (-((hz / cutoff).ln() * 4.0).powi(2)).exp();
                let y = plot.center().y - db.clamp(-36.0, 18.0) / 36.0 * plot.height() / 2.0;
                pos2(x_of(hz), y)
            })
            .collect();
        g.add(Shape::line(pts, Stroke::new(2.0, accent)));
        let cx = x_of(cutoff);
        g.line_segment(
            [pos2(cx, plot.top()), pos2(cx, plot.bottom())],
            Stroke::new(1.0, accent.gamma_multiply(0.35)),
        );
        let mode_name = ["Low Pass", "High Pass", "Band Pass"][mode];
        g.text(
            pos2(rect.left() + 12.0, rect.bottom() - 8.0),
            Align2::LEFT_BOTTOM,
            format!("{mode_name} · {cutoff:.0} Hz · Q {resonance:.1}"),
            FontId::monospace(8.5),
            DIM,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Insert;

    #[test]
    fn every_effect_window_draws_without_changing_its_insert() {
        use crate::model::Insert;
        let kinds = [
            FxKind::ChannelEq,
            FxKind::MultiEq,
            FxKind::Compressor,
            FxKind::MultiComp,
            FxKind::Delay,
            FxKind::Chorus,
            FxKind::Phaser,
            FxKind::Overdrive,
            FxKind::Filter,
            FxKind::Gate,
            FxKind::Limiter,
            FxKind::Amp,
            FxKind::Reverb,
        ];
        let mut p = Project::empty();
        p.tracks[0].inserts = kinds.iter().copied().map(Insert::new).collect();
        let mut s = UiState::new();
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(1000.0, 860.0);
        for (slot, kind) in kinds.iter().enumerate() {
            s.mixer.fx_window = Some((0, slot));
            for _ in 0..3 {
                h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
            }
            assert_eq!(
                p.tracks[0].inserts[slot],
                Insert::new(*kind),
                "{} changed while it was only looked at",
                kind.name()
            );
            assert_eq!(s.mixer.fx_window, Some((0, slot)));
        }
    }

    #[test]
    fn the_window_edits_the_insert_and_closes_with_it() {
        let mut p = Project::empty();
        p.tracks[0].inserts = vec![
            Insert::new(FxKind::Compressor),
            Insert::new(FxKind::ChannelEq),
            Insert::new(FxKind::MultiEq),
            Insert::new(FxKind::Limiter),
            Insert::new(FxKind::Gate),
        ];
        let mut s = UiState::new();
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(800.0, 700.0);
        for slot in 0..5 {
            s.mixer.fx_window = Some((0, slot));
            for _ in 0..3 {
                h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
            }
            assert_eq!(s.mixer.fx_window, Some((0, slot)));
        }
        // Nothing was changed by looking.
        assert_eq!(p.tracks[0].inserts[0], Insert::new(FxKind::Compressor));
        assert_eq!(p.tracks[0].inserts[2], Insert::new(FxKind::MultiEq));
        // The insert goes away: the window closes.
        p.tracks[0].inserts.clear();
        h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        assert_eq!(s.mixer.fx_window, None);
    }

    #[test]
    fn the_space_designer_window_draws_and_changes_nothing_by_looking() {
        let mut p = Project::empty();
        p.tracks[0].inserts = vec![Insert::new(FxKind::Reverb)];
        let mut s = UiState::new();
        s.mixer.fx_window = Some((0, 0));
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(1000.0, 800.0);
        for _ in 0..4 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        assert_eq!(s.mixer.fx_window, Some((0, 0)));
        assert_eq!(p.tracks[0].inserts[0], Insert::new(FxKind::Reverb));
    }

    #[test]
    fn a_knob_of_a_stereo_out_effect_moves_its_setting() {
        let mut p = Project::empty();
        p.master_inserts = vec![Insert::new(FxKind::Compressor)];
        let mut s = UiState::new();
        let insert = p.master_inserts[0];
        let edit = |param, value| Edit {
            param,
            value,
            ended: true,
        };
        finish(
            &mut p,
            &mut s,
            crate::mixer::MASTER_STRIP,
            0,
            insert,
            insert,
            // Out of range, not finite, or a parameter the effect does not have.
            vec![
                edit(0, -30.0),
                edit(1, 1000.0),
                edit(2, f32::NAN),
                edit(30, 1.0),
            ],
            true,
        );
        let infos = FxKind::Compressor.params();
        let after = p.master_inserts[0];
        assert_eq!(after.params[0], -30.0);
        assert_eq!(after.params[1], infos[1].max);
        assert_eq!(after.params[2], insert.params[2]);
    }

    #[test]
    fn a_ladspa_effect_window_draws_a_missing_plugin_and_a_real_one() {
        let mut p = Project::empty();
        let gone = crate::plugins::intern(&crate::plugins::PluginRef {
            library: "/nowhere/ui.so".into(),
            label: "gone".into(),
            unique_id: 1,
            name: "Gone Plugin".into(),
        });
        p.tracks[0].inserts = vec![Insert::new_plugin(gone)];
        let mut s = UiState::new();
        s.mixer.fx_window = Some((0, 0));
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(1000.0, 800.0);
        for _ in 0..3 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        assert_eq!(s.mixer.fx_window, Some((0, 0)), "it stays open");
        assert_eq!(p.tracks[0].inserts[0].label(), "Gone Plugin");
        // A plug-in the catalogue knows draws its controls.
        let known = crate::plugins::PluginInfo {
            library: "/plugins/ui_known.so".into(),
            index: 0,
            unique_id: 2,
            label: "known".into(),
            name: "Known Plugin".into(),
            maker: "m".into(),
            audio_in: 1,
            audio_out: 1,
            controls: vec![crate::plugins::ControlInfo {
                name: "Drive".into(),
                port: 0,
                min: 0.0,
                max: 10.0,
                default: 5.0,
                toggled: false,
                integer: false,
                log: false,
            }],
            latency_port: None,
        };
        let id = crate::plugins::intern(&known.plugin_ref());
        crate::plugins::set_catalog(crate::plugins::Catalog {
            plugins: vec![known],
            scanned: true,
            ..Default::default()
        });
        p.tracks[0].inserts = vec![Insert::new_plugin(id)];
        assert_eq!(
            p.tracks[0].inserts[0].params[0], 0.5,
            "the default is half way"
        );
        for _ in 0..3 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        assert_eq!(p.tracks[0].inserts[0].label(), "Known Plugin");
    }

    #[test]
    fn the_pult_eq_window_draws_names_its_selectors_and_survives_a_save() {
        let mut p = Project::empty();
        let mut ins = Insert::new(FxKind::PultEq);
        ins.params[prowave_engine::pult_eq::idx::LOW_BOOST] = 6.0;
        ins.params[prowave_engine::pult_eq::idx::HIGH_FREQ] = 6.0;
        p.tracks[0].inserts = vec![ins];
        let mut s = UiState::new();
        s.mixer.fx_window = Some((0, 0));
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(1000.0, 800.0);
        for _ in 0..3 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        assert_eq!(s.mixer.fx_window, Some((0, 0)));
        assert_eq!(pult_text("Mode", 1.0).as_deref(), Some("Stereo"));
        assert_eq!(pult_text("Low Freq", 3.0).as_deref(), Some("100 Hz"));
        assert_eq!(pult_text("High Freq", 6.0).as_deref(), Some("16 kHz"));
        assert_eq!(pult_text("Low Boost", 6.0).as_deref(), Some("6.0"));
        assert_eq!(pult_text("Output", 1.0), None);
        // It is saved by name and opens again with the same settings.
        let (doc, _) = crate::project_io::to_doc(&p);
        assert_eq!(doc.tracks[0].fx[0].kind, "Pult EQ");
        let (back, _) =
            crate::project_io::from_doc(&doc, std::path::Path::new("."), &egui::Context::default());
        assert_eq!(back.tracks[0].inserts, p.tracks[0].inserts);
    }

    #[test]
    fn the_stereo_out_effect_window_opens_edits_and_closes() {
        let mut p = Project::empty();
        p.master_inserts = vec![Insert::new(FxKind::Reverb), Insert::new(FxKind::Compressor)];
        let mut s = UiState::new();
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(1000.0, 800.0);
        for slot in 0..2 {
            s.mixer.fx_window = Some((crate::mixer::MASTER_STRIP, slot));
            for _ in 0..3 {
                h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
            }
            assert_eq!(s.mixer.fx_window, Some((crate::mixer::MASTER_STRIP, slot)));
        }
        assert_eq!(
            p.master_inserts[0],
            Insert::new(FxKind::Reverb),
            "looking changes nothing"
        );
        // The effect goes away: the window closes.
        p.master_inserts.clear();
        h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        assert_eq!(s.mixer.fx_window, None);
    }

    #[test]
    fn the_amp_designer_window_draws_and_locks_its_automated_knobs() {
        use crate::model::AutoMode;
        let mut p = Project::empty();
        p.tracks[0].inserts = vec![Insert::new(FxKind::Amp)];
        let mut s = UiState::new();
        s.mixer.fx_window = Some((0, 0));
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(900.0, 760.0);
        for _ in 0..4 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        // Looking at the faceplate changes nothing.
        assert_eq!(s.mixer.fx_window, Some((0, 0)));
        assert_eq!(p.tracks[0].inserts[0], Insert::new(FxKind::Amp));
        // A played Gain lane locks the knob in Read mode, as the sliders are locked.
        let gain = AutoTarget::Fx { slot: 0, param: 1 };
        p.tracks[0].auto.mode = AutoMode::Read;
        p.tracks[0].auto.set_point(gain, 0.0, 0.25);
        assert!(!automation::editable(&p.tracks[0], gain));
        for _ in 0..4 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        assert_eq!(p.tracks[0].inserts[0].params[1], 12.0);
        assert_eq!(p.tracks[0].auto.lane(gain).len(), 1);
    }

    #[test]
    fn a_played_lane_moves_the_slider_and_locks_it_in_read_mode() {
        use crate::model::AutoMode;
        let mut p = Project::empty();
        p.tracks[0].inserts = vec![Insert::new(FxKind::Compressor)];
        let thr = AutoTarget::Fx { slot: 0, param: 0 };
        p.tracks[0].auto.mode = AutoMode::Read;
        p.tracks[0].auto.set_point(thr, 0.0, 0.25);
        let mut s = UiState::new();
        s.mixer.fx_window = Some((0, 0));
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(800.0, 600.0);
        for _ in 0..4 {
            h.frame(vec![], |ui| show(ui.ctx(), &mut p, &mut s, &meters));
        }
        // Looking at a played lane changes neither the setting nor the lane.
        assert_eq!(p.tracks[0].inserts[0].params[0], -18.0);
        assert_eq!(p.tracks[0].auto.lane(thr).len(), 1);
        assert_eq!(p.tracks[0].auto.held, None);
        assert!(!automation::editable(&p.tracks[0], thr));
    }
}
