//! Inspector (Logic Pro: the "i" panel): Region Inspector, Track Inspector and the two
//! channel strips (selected track and its output).
//!
//! Works today: Region Mute, Quantize, Transpose, Velocity, and for audio regions Gain, Fade
//! In / Out, Curve and Reverse; Track Transpose, Velocity,
//! Key Limit and Vel Limit (all applied non-destructively when the engine song is built);
//! both channel strips (fader, pan, M/S, meters). Fields Logic has that are not implemented
//! yet are shown dimmed and do nothing.
use crate::i18n::tr;
use crate::mixer::{channel_strip, fx_rows_for, rows, send_rows_for, Meters, NameStyle, StripKey};
use crate::model::{Project, SoundFontUse, TrackKind, MAX_REGION_GAIN, MIN_REGION_GAIN};
use crate::track_icons::{draw_icon_tinted, track_icon};
use crate::widgets::{stroke, BTN_BG, TEXT, TEXT_DIM};
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder,
};
use prowave_engine::FadeCurve;

const PANEL_BG: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1e);
const HEADER_BG: Color32 = Color32::from_rgb(0x25, 0x25, 0x27);
const ROW_H: f32 = 20.0;
const HEADER_H: f32 = 24.0;
/// The inspector's width: Logic's, which is two channel strips of the Mixer's width side by
/// side (the selected track's and its output's).
pub const WIDTH: f32 = 276.0;
const STRIPS_MIN_H: f32 = 380.0;

/// Fade lengths move in 1/16 notes.
const FADE_STEPS_PER_BEAT: i32 = 4;

pub(crate) const FADE_CURVES: [(&str, FadeCurve); 3] = [
    ("Equal Power", FadeCurve::EqualPower),
    ("Linear", FadeCurve::Linear),
    ("S-Curve", FadeCurve::SCurve),
];

/// A SoundFont setting in the inspector: the preset and the file name, or the file name and
/// "missing" when the file is gone.
fn soundfont_label(sf: &SoundFontUse) -> String {
    let path = std::path::Path::new(&sf.path);
    let file = path
        .file_name()
        .map_or_else(|| sf.path.clone(), |n| n.to_string_lossy().into_owned());
    let preset = crate::soundfonts::loaded(&sf.path).and_then(|bank| {
        bank.presets()
            .iter()
            .find(|pr| pr.bank == sf.bank && pr.program == sf.program)
            .map(|pr| pr.name.clone())
    });
    match preset {
        Some(name) => format!("{name} ({file})"),
        None if !path.exists() => format!("{file} · {}", tr("missing")),
        None => file,
    }
}

/// The `drumkit.xml` to play for what the user picked: an archive is unpacked first.
fn hydrogen_kit_file(path: &std::path::Path) -> Result<String, String> {
    let file = if crate::hydrogen_archive::is_archive(path) {
        let into = crate::hydrogen_archive::kits_dir()
            .ok_or_else(|| "there is no folder to unpack the drum kit into".to_string())?;
        crate::hydrogen_archive::unpack(path, &into)?
    } else {
        path.to_path_buf()
    };
    Ok(file.to_string_lossy().into_owned())
}

/// Gives track `ti` the SoundFont or Hydrogen drum kit at `path`, playing its first preset.
fn use_soundfont_file(p: &mut Project, ti: usize, path: String) {
    // Read the file again, even if it was loaded before.
    crate::soundfonts::forget(&path);
    let first = match crate::soundfonts::load(&path) {
        Ok(bank) => bank
            .presets()
            .first()
            .map_or((0, 0), |pr| (pr.bank, pr.program)),
        Err(e) => {
            tracing::warn!("{e}");
            (0, 0)
        }
    };
    p.tracks[ti].soundfont = Some(SoundFontUse {
        path,
        bank: first.0,
        program: first.1,
    });
    p.tracks[ti].epiano = None;
    p.tracks[ti].sampler = None;
    p.tracks[ti].drums = None;
    p.tracks[ti].bass808 = None;
    p.tracks[ti].hosted_instrument = 0;
}

/// The Sampler's dials as inspector rows: dial, name, drag steps per unit and range in steps.
pub(crate) type SamplerRow = (usize, &'static str, f32, (i32, i32));

pub(crate) const SAMPLER_ROWS: [SamplerRow; prowave_engine::sampler::PARAMS] = [
    (
        prowave_engine::sampler::dial::LEVEL,
        "Level:",
        2.0,
        (-48, 24),
    ),
    (
        prowave_engine::sampler::dial::ATTACK,
        "Attack:",
        1.0,
        (0, 2000),
    ),
    (
        prowave_engine::sampler::dial::RELEASE,
        "Release:",
        1.0,
        (0, 5000),
    ),
];

/// The text of a Sampler dial at `steps`.
fn sampler_text(dial: usize, steps: i32) -> String {
    match dial {
        prowave_engine::sampler::dial::LEVEL => format!("{:+.1} dB", steps as f32 / 2.0),
        _ => format!("{steps} ms"),
    }
}

/// The name of MIDI key `key` (60 is C3, as in Logic).
fn key_name(key: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        NAMES[usize::from(key % 12)],
        i32::from(key / 12) - 2
    )
}

/// Makes the track play the Sampler with `source`, and not a SoundFont or the Electric Piano.
fn use_sampler(p: &mut Project, ti: usize, source: crate::sampler_load::SamplerSource) {
    p.tracks[ti].sampler = Some(crate::model::SamplerUse::new(source));
    p.tracks[ti].soundfont = None;
    p.tracks[ti].epiano = None;
    p.tracks[ti].drums = None;
    p.tracks[ti].bass808 = None;
    p.tracks[ti].hosted_instrument = 0;
}

/// The Sampler's rows under the instrument row: its three dials, and for a single audio file its
/// root key and loop. Returns the changed Sampler, if the user changed it. `y` moves past the
/// rows.
fn sampler_rows(
    ui: &Ui,
    g: &Painter,
    (label_right, vx, vw): (f32, f32, f32),
    y: &mut f32,
    current: &crate::model::SamplerUse,
) -> Option<crate::model::SamplerUse> {
    use crate::sampler_load::SamplerSource;
    let mut next = current.clone();
    let mut changed = false;
    for (dial, name, per_unit, range) in SAMPLER_ROWS {
        label(g, label_right, *y, tr(name), true);
        let r = Rect::from_min_size(pos2(vx, *y + 2.0), vec2(vw, ROW_H - 4.0));
        let steps = (current.params[dial] * per_unit).round() as i32;
        let default = (prowave_engine::sampler::defaults()[dial] * per_unit).round() as i32;
        if let Some(v) = drag_value(
            ui,
            g,
            r,
            &format!("sampler{dial}"),
            steps,
            range,
            default,
            sampler_text(dial, steps),
            true,
        ) {
            next.params[dial] = v as f32 / per_unit;
            changed = true;
        }
        *y += ROW_H;
    }
    if let SamplerSource::Audio { path, root, looped } = &current.source {
        label(g, label_right, *y, tr("Root Key:"), true);
        let r = Rect::from_min_size(pos2(vx, *y + 2.0), vec2(vw, ROW_H - 4.0));
        if let Some(v) = drag_value(
            ui,
            g,
            r,
            "sampler_root",
            i32::from(*root),
            (0, 127),
            60,
            key_name(*root),
            true,
        ) {
            next.source = SamplerSource::Audio {
                path: path.clone(),
                root: v.clamp(0, 127) as u8,
                looped: *looped,
            };
            changed = true;
        }
        *y += ROW_H;
        label(g, label_right, *y, tr("Loop:"), true);
        let r = Rect::from_min_size(pos2(vx, *y + 2.0), vec2(vw, ROW_H - 4.0));
        let text = if *looped { tr("On") } else { tr("Off") }.to_string();
        if let Some(v) = drag_value(
            ui,
            g,
            r,
            "sampler_loop",
            i32::from(*looped),
            (0, 1),
            0,
            text,
            true,
        ) {
            // The source is the same file: only its loop changes (the sound is made again).
            next.source = SamplerSource::Audio {
                path: path.clone(),
                root: *root,
                looped: v != 0,
            };
            changed = true;
        }
        *y += ROW_H;
    }
    changed.then_some(next)
}

/// The Electric Piano's dials as inspector rows: name, drag steps per unit, range in steps and
/// the text for a value.
pub(crate) type PianoRow = (usize, &'static str, f32, (i32, i32));

pub(crate) const PIANO_ROWS: [PianoRow; prowave_engine::epiano::PARAMS] = [
    (
        prowave_engine::epiano::dial::TREMOLO_DEPTH,
        "Tremolo:",
        100.0,
        (0, 100),
    ),
    (
        prowave_engine::epiano::dial::TREMOLO_RATE,
        "Tremolo Rate:",
        10.0,
        (10, 100),
    ),
    (prowave_engine::epiano::dial::BARK, "Bark:", 100.0, (0, 100)),
    (prowave_engine::epiano::dial::TONE, "Tone:", 100.0, (0, 100)),
    (
        prowave_engine::epiano::dial::DECAY,
        "Decay:",
        100.0,
        (50, 200),
    ),
    (
        prowave_engine::epiano::dial::LEVEL,
        "Level:",
        2.0,
        (-24, 12),
    ),
];

/// The text of an Electric Piano dial at `steps` (see [`PIANO_ROWS`]).
fn piano_text(dial: usize, steps: i32) -> String {
    use prowave_engine::epiano::dial as d;
    match dial {
        d::TREMOLO_RATE => format!("{:.1} Hz", steps as f32 / 10.0),
        d::DECAY => format!("{:.2}x", steps as f32 / 100.0),
        d::LEVEL => format!("{:+.1} dB", steps as f32 / 2.0),
        _ => format!("{steps} %"),
    }
}

/// Draws the Electric Piano's dials under the instrument row and returns the new dials when one
/// was dragged. Each row is one `ROW_H` tall; `y` moves past them.
fn piano_rows(
    ui: &Ui,
    g: &Painter,
    (label_right, vx, vw): (f32, f32, f32),
    y: &mut f32,
    dials: [f32; prowave_engine::epiano::PARAMS],
) -> Option<[f32; prowave_engine::epiano::PARAMS]> {
    let mut out = None;
    for (dial, name, per_unit, range) in PIANO_ROWS {
        label(g, label_right, *y, tr(name), true);
        let r = Rect::from_min_size(pos2(vx, *y + 2.0), vec2(vw, ROW_H - 4.0));
        let steps = (dials[dial] * per_unit).round() as i32;
        let default = (prowave_engine::epiano::defaults()[dial] * per_unit).round() as i32;
        if let Some(v) = drag_value(
            ui,
            g,
            r,
            &format!("piano{dial}"),
            steps,
            range,
            default,
            piano_text(dial, steps),
            true,
        ) {
            let mut next = out.unwrap_or(dials);
            next[dial] = v as f32 / per_unit;
            out = Some(prowave_engine::epiano::clamp(next));
        }
        *y += ROW_H;
    }
    out
}

fn gain_text(db: f32) -> String {
    if db == 0.0 {
        "0 dB".into()
    } else {
        format!("{db:+.1} dB")
    }
}

/// A fade of `beats` at `bpm`, in milliseconds like Logic shows it.
fn fade_text(beats: f32, bpm: f32) -> String {
    if beats <= 0.0 {
        "0".into()
    } else {
        format!("{:.0} ms", beats * 60_000.0 / bpm.max(1.0))
    }
}

pub(crate) const QUANTIZE: [(&str, Option<f32>); 7] = [
    ("Off", None),
    ("1/4 Note", Some(1.0)),
    ("1/8 Note", Some(0.5)),
    ("1/16 Note", Some(0.25)),
    ("1/32 Note", Some(0.125)),
    ("1/8 Triplet", Some(1.0 / 3.0)),
    ("1/16 Triplet", Some(1.0 / 6.0)),
];

pub struct InspectorView {
    pub region_open: bool,
    pub track_open: bool,
    pub more_open: bool,
    pub region_general_open: bool,
    pub quantize_open: bool,
    pub details_open: bool,
    pub track_general_open: bool,
    pub track_more_open: bool,
    scroll_y: f32,
    /// Edits waiting for the mouse to be released before the engine song is rebuilt.
    pending_song_change: bool,
}

impl Default for InspectorView {
    fn default() -> Self {
        Self {
            region_open: true,
            track_open: false,
            more_open: false,
            region_general_open: true,
            quantize_open: false,
            details_open: false,
            track_general_open: true,
            track_more_open: false,
            scroll_y: 0.0,
            pending_song_change: false,
        }
    }
}

fn id(key: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::unique(("inspector", key))
}

/// Logic note names: 0 = C-2, 127 = G8.
fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NAMES[(n % 12) as usize], n as i32 / 12 - 2)
}

/// Section header: "Title: name" on the left and a disclosure chevron on the right.
fn section_header(
    ui: &Ui,
    g: &Painter,
    r: Rect,
    key: &str,
    open: bool,
    title: &str,
    name: &str,
) -> egui::Response {
    let resp = ui.interact(r, id(("section", key)), Sense::click());
    g.rect_filled(r, 0.0, HEADER_BG);
    g.line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
    );
    let t = g.text(
        pos2(r.left() + 12.0, r.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(12.5),
        TEXT,
    );
    let name_left = t.right() + 5.0;
    let font = FontId::proportional(12.5);
    let shown = crate::mixer::elide(g, name, &font, (r.right() - 28.0 - name_left).max(20.0));
    g.text(
        pos2(name_left, r.center().y),
        Align2::LEFT_CENTER,
        shown,
        font,
        TEXT_DIM,
    );
    let c = pos2(r.right() - 14.0, r.center().y);
    let chev = if open {
        vec![
            c + vec2(-4.0, -2.0),
            c + vec2(0.0, 2.0),
            c + vec2(4.0, -2.0),
        ]
    } else {
        vec![
            c + vec2(-2.0, -4.0),
            c + vec2(2.0, 0.0),
            c + vec2(-2.0, 4.0),
        ]
    };
    g.add(Shape::line(chev, Stroke::new(1.4, TEXT_DIM)));
    resp
}

fn label(g: &Painter, label_x: f32, y: f32, text: &str, enabled: bool) {
    let col = if enabled {
        Color32::from_rgb(0xc6, 0xc6, 0xcc)
    } else {
        Color32::from_rgb(0x64, 0x64, 0x6a)
    };
    let clean = text.trim_end_matches(':');
    g.text(
        pos2(label_x, y + ROW_H / 2.0),
        Align2::LEFT_CENTER,
        clean,
        FontId::proportional(11.5),
        col,
    );
}

fn sub_disclosure(ui: &Ui, g: &Painter, r: Rect, key: &str, open: bool, title: &str) -> bool {
    let resp = ui.interact(r, id(("sub", key)), Sense::click());
    g.line_segment(
        [
            pos2(r.left() + 10.0, r.bottom()),
            pos2(r.right() - 10.0, r.bottom()),
        ],
        Stroke::new(1.0, Color32::from_rgb(0x2c, 0x2c, 0x31)),
    );
    g.text(
        pos2(r.left() + 12.0, r.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(11.5),
        TEXT_DIM,
    );
    let c = pos2(r.right() - 14.0, r.center().y);
    let chev = if open {
        vec![
            c + vec2(-3.5, -1.5),
            c + vec2(0.0, 2.0),
            c + vec2(3.5, -1.5),
        ]
    } else {
        vec![
            c + vec2(-1.5, -3.5),
            c + vec2(2.0, 0.0),
            c + vec2(-1.5, 3.5),
        ]
    };
    g.add(Shape::line(chev, Stroke::new(1.2, TEXT_DIM)));
    resp.clicked()
}

fn color_pill(ui: &Ui, g: &Painter, r: Rect, key: &str, current: Color32) -> Option<Color32> {
    let resp = ui.interact(r, id(("color", key)), Sense::click());
    g.rect_filled(r, r.height() / 2.0, current);
    if resp.hovered() {
        g.rect_stroke(
            r,
            r.height() / 2.0,
            Stroke::new(1.0, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
    let mut picked = None;
    egui::Popup::menu(&resp).show(|ui| {
        const PALETTE: [Color32; 12] = [
            Color32::from_rgb(0xd6, 0x18, 0x9e),
            Color32::from_rgb(0xe0, 0x4b, 0x4b),
            Color32::from_rgb(0xe6, 0x8a, 0x2e),
            Color32::from_rgb(0xd9, 0xb8, 0x38),
            Color32::from_rgb(0x4e, 0xb0, 0x4a),
            Color32::from_rgb(0x2b, 0xa8, 0x8f),
            Color32::from_rgb(0x3a, 0x7b, 0xd5),
            Color32::from_rgb(0x63, 0x5b, 0xd4),
            Color32::from_rgb(0x9b, 0x4d, 0xca),
            Color32::from_rgb(0x7c, 0x7c, 0x85),
            Color32::from_rgb(0x4f, 0x86, 0xb8),
            Color32::from_rgb(0xc8, 0x5c, 0x7e),
        ];
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(148.0);
            for c in PALETTE {
                let (rect, sw) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                ui.painter().rect_filled(rect, 4.0, c);
                if c == current {
                    ui.painter().rect_stroke(
                        rect,
                        4.0,
                        Stroke::new(1.5, Color32::WHITE),
                        egui::StrokeKind::Inside,
                    );
                }
                if sw.clicked() {
                    picked = Some(c);
                    ui.close();
                }
            }
        });
    });
    picked
}

/// Small pill toggle switch: 32px x 18px for booleans. Returns true when toggled.
fn checkbox(ui: &Ui, g: &Painter, at: Pos2, key: &str, value: bool, enabled: bool) -> bool {
    let r = Rect::from_min_size(at + vec2(0.0, (ROW_H - 18.0) / 2.0), vec2(32.0, 18.0));
    let clicked = enabled
        && ui
            .interact(r.expand(2.0), id(("check", key)), Sense::click())
            .clicked();
    let bg = if !enabled {
        Color32::from_rgb(0x28, 0x28, 0x2b)
    } else if value {
        Color32::from_rgb(0x12, 0x69, 0xcc)
    } else {
        Color32::from_rgb(0x32, 0x32, 0x36)
    };
    g.rect_filled(r, 9.0, bg);
    let thumb_x = if value {
        r.right() - 9.0
    } else {
        r.left() + 9.0
    };
    let thumb_c = pos2(thumb_x, r.center().y);
    g.circle_filled(thumb_c, 6.5, Color32::WHITE);
    clicked
}

/// Compact popup ("Off ⇕"). Disabled ones are drawn dim and never open.
fn popup(
    ui: &Ui,
    g: &Painter,
    r: Rect,
    key: &str,
    text: &str,
    enabled: bool,
) -> Option<egui::Response> {
    let col = if enabled {
        TEXT
    } else {
        Color32::from_rgb(0x8e, 0x8e, 0x93)
    };
    let resp = enabled.then(|| ui.interact(r, id(("popup", key)), Sense::click()));
    let hovered = resp.as_ref().is_some_and(|r| r.hovered());
    let (bg, border) = if hovered {
        (
            Color32::from_rgb(0x38, 0x38, 0x3c),
            Color32::from_rgb(0x50, 0x50, 0x55),
        )
    } else if enabled {
        (
            Color32::from_rgb(0x28, 0x28, 0x2b),
            Color32::from_rgb(0x28, 0x28, 0x2b),
        )
    } else {
        (
            Color32::from_rgb(0x1c, 0x1c, 0x1e),
            Color32::from_rgb(0x1c, 0x1c, 0x1e),
        )
    };
    g.rect_filled(r, 4.0, bg);
    if hovered {
        g.rect_stroke(r, 4.0, Stroke::new(1.0, border), StrokeKind::Inside);
    }
    g.text(
        pos2(r.left() + 6.0, r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        col,
    );
    let c = pos2(r.right() - 8.0, r.center().y);
    g.add(Shape::line(
        vec![
            c + vec2(-2.5, -1.5),
            c + vec2(0.0, -4.0),
            c + vec2(2.5, -1.5),
        ],
        stroke(col),
    ));
    g.add(Shape::line(
        vec![c + vec2(-2.5, 1.5), c + vec2(0.0, 4.0), c + vec2(2.5, 1.5)],
        stroke(col),
    ));
    resp
}

/// Logic-style value field: drag up/down to change, double-click to reset.
#[allow(clippy::too_many_arguments)]
fn drag_value(
    ui: &Ui,
    g: &Painter,
    r: Rect,
    key: &str,
    value: i32,
    range: (i32, i32),
    reset: i32,
    text: String,
    enabled: bool,
) -> Option<i32> {
    let col = if !enabled || (value == 0 && reset == 0) {
        Color32::from_rgb(0x8e, 0x8e, 0x93)
    } else {
        Color32::from_rgb(0xe5, 0xe5, 0xea)
    };
    let resp = if enabled {
        let resp = ui.interact(r, id(("drag", key)), Sense::click_and_drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        Some(resp)
    } else {
        None
    };

    let (bg, border) = if let Some(ref resp) = resp {
        if resp.dragged() {
            (
                Color32::from_rgb(0x32, 0x32, 0x36),
                Color32::from_rgb(0x12, 0x69, 0xcc),
            )
        } else if resp.hovered() {
            (
                Color32::from_rgb(0x32, 0x32, 0x36),
                Color32::from_rgb(0x58, 0x58, 0x5e),
            )
        } else {
            (
                Color32::from_rgb(0x2b, 0x2b, 0x2e),
                Color32::from_rgb(0x38, 0x38, 0x3c),
            )
        }
    } else {
        (
            Color32::from_rgb(0x20, 0x20, 0x22),
            Color32::from_rgb(0x28, 0x28, 0x2b),
        )
    };

    g.rect_filled(r, 4.0, bg);
    g.rect_stroke(r, 4.0, Stroke::new(1.0, border), StrokeKind::Inside);
    g.text(
        pos2(r.left() + 6.0, r.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.5),
        col,
    );

    let Some(resp) = resp else {
        return None;
    };
    if resp.double_clicked() {
        return Some(reset);
    }
    if resp.dragged() {
        // Accumulate fractional drag in egui memory so slow drags still move the value.
        let acc_id = id(("drag_acc", key));
        let acc: f32 =
            ui.ctx().data(|d| d.get_temp(acc_id).unwrap_or(0.0)) - resp.drag_delta().y / 4.0;
        let steps = acc.trunc() as i32;
        ui.ctx()
            .data_mut(|d| d.insert_temp(acc_id, acc - steps as f32));
        if steps != 0 {
            return Some((value + steps).clamp(range.0, range.1));
        }
    }
    None
}

fn transpose_text(v: i32) -> String {
    match v {
        0 => "0 st".to_string(),
        v if v > 0 => format!("+{v} st"),
        v => format!("{v} st"),
    }
}

fn velocity_text(v: i32) -> String {
    match v {
        0 => "0".to_string(),
        v if v > 0 => format!("+{v}"),
        v => v.to_string(),
    }
}

fn delay_text(v: i32) -> String {
    match v {
        0 => "0 ms".to_string(),
        v if v > 0 => format!("+{v} ms"),
        v => format!("{v} ms"),
    }
}

fn signed(v: i32) -> String {
    match v {
        0 => "0".to_string(),
        v if v > 0 => format!("+{v}"),
        v => v.to_string(),
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState, meters: &Meters) {
    let panel = ui.available_rect_before_wrap();
    ui.allocate_rect(panel, Sense::hover());
    let g = ui.painter_at(panel);
    g.rect_filled(panel, 0.0, PANEL_BG);
    let left = panel.left();
    let w = panel.width();

    let label_col_w = (w * 0.44).clamp(112.0, 150.0);
    let label_right = left + label_col_w - 6.0;
    let vx = left + label_col_w;
    let vw = (w - label_col_w - 12.0).max(40.0);

    let ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
    let sel = s
        .tracks
        .selected
        .filter(|(t, r)| p.tracks.get(*t).is_some_and(|tr| *r < tr.regions.len()));
    let is_audio = sel.is_some_and(|(t, r)| {
        p.tracks[t].regions[r].audio.is_some() || p.tracks[t].kind == TrackKind::Audio
    });

    let dest_key = if !p.tracks.is_empty() {
        if p.tracks[ti].output > 0 {
            let bus = p.tracks[ti].output;
            (0..p.tracks.len())
                .find(|&i| p.aux_bus_of(i) == bus)
                .map(StripKey::Track)
                .unwrap_or(StripKey::StereoOut)
        } else if let Some(send_bus) = p.tracks[ti].sends.first().and_then(|s| s.bus) {
            (0..p.tracks.len())
                .find(|&i| p.aux_bus_of(i) == send_bus)
                .map(StripKey::Track)
                .unwrap_or(StripKey::StereoOut)
        } else {
            StripKey::StereoOut
        }
    } else {
        StripKey::StereoOut
    };
    let keys = [StripKey::Track(ti), dest_key];
    let fx_rows = fx_rows_for(p, &keys);
    let send_rows = send_rows_for(p, &keys);
    let r_rel = rows(0.0, fx_rows, send_rows);

    let top_h_id = id("inspector_top_content_h");
    let min_top_h = HEADER_H * 2.0 + 4.0;
    let max_top_h = (panel.height() * 0.55)
        .max(min_top_h)
        .min((panel.height() - STRIPS_MIN_H - 4.0).max(min_top_h));
    let last_top_h = ui
        .ctx()
        .data(|d| d.get_temp::<f32>(top_h_id))
        .unwrap_or(min_top_h);
    let top_h = last_top_h.clamp(min_top_h, max_top_h);
    let divider_y = panel.top() + top_h;
    let top_rect = Rect::from_min_max(panel.min, pos2(panel.max.x, divider_y));
    let strips_rect = Rect::from_min_max(pos2(panel.left(), divider_y + 4.0), panel.max);

    g.line_segment(
        [
            pos2(panel.left(), divider_y),
            pos2(panel.right(), divider_y),
        ],
        Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)),
    );

    let mut changed_song = false;

    ui.scope_builder(UiBuilder::new().max_rect(top_rect), |ui| {
        ui.set_clip_rect(top_rect);
        egui::ScrollArea::vertical()
            .id_salt("inspector_accordions_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let left = ui.cursor().left();
                let w = ui.available_width();
                let label_col_w = (w * 0.44).clamp(112.0, 150.0);
                let label_right = left + 12.0;
                let vx = left + label_col_w;
                let vw = (w - label_col_w - 12.0).max(40.0);

                let g_upper = ui.painter().clone();
                let mut y = ui.cursor().top();
                let start_y = y;

                // ---------------- Region
                let region_name = sel.map_or_else(
                    || tr("MIDI Defaults").to_string(),
                    |(t, r)| p.tracks[t].regions[r].name.clone(),
                );
                let hdr = Rect::from_min_size(pos2(left, y), vec2(w, HEADER_H));
                let head = section_header(
                    ui,
                    &g_upper,
                    hdr,
                    "region",
                    s.inspector.region_open,
                    tr("Region:"),
                    &region_name,
                );
                if head.clicked() {
                    s.inspector.region_open = !s.inspector.region_open;
                }
                if sel.is_some() && head.double_clicked() {
                    crate::menus::apply("Rename Regions", p, s);
                }
                y += HEADER_H + 4.0;
                if s.inspector.region_open {
                    if sel.is_none() {
                        // Context: MIDI Defaults (no region selected)
                        let gen_r = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                        if sub_disclosure(
                            ui,
                            &g_upper,
                            gen_r,
                            "defaults_general",
                            s.inspector.region_general_open,
                            tr("General"),
                        ) {
                            s.inspector.region_general_open = !s.inspector.region_general_open;
                        }
                        y += ROW_H + 2.0;
                        if s.inspector.region_general_open {
                            label(&g_upper, label_right, y, tr("Color:"), true);
                            let default_col = p
                                .tracks
                                .get(ti)
                                .map_or(Color32::from_rgb(0xd6, 0x18, 0x9e), |t| t.color);
                            let cur_col = p.midi_defaults.color.unwrap_or(default_col);
                            if let Some(c) = color_pill(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 3.0), vec2(vw, ROW_H - 6.0)),
                                "defaults_color",
                                cur_col,
                            ) {
                                p.midi_defaults.color = Some(c);
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Velocity Offset:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_velocity",
                                p.midi_defaults.velocity as i32,
                                (-99, 99),
                                0,
                                velocity_text(p.midi_defaults.velocity as i32),
                                true,
                            ) {
                                p.midi_defaults.velocity = v as i8;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Delay:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_delay_tk",
                                p.midi_defaults.delay_ticks as i32,
                                (-480, 480),
                                0,
                                format!("{} tk", p.midi_defaults.delay_ticks),
                                true,
                            ) {
                                p.midi_defaults.delay_ticks = v as i16;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Transpose:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_transpose",
                                p.midi_defaults.transpose as i32,
                                (-96, 96),
                                0,
                                transpose_text(p.midi_defaults.transpose as i32),
                                true,
                            ) {
                                p.midi_defaults.transpose = v as i8;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Pitch Source:"), true);
                            if let Some(resp) = popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "defaults_pitch_source",
                                tr(p.midi_defaults.pitch_source.label()),
                                true,
                            ) {
                                egui::Popup::menu(&resp).show(|ui| {
                                    for src in [
                                        crate::model::PitchSource::Off,
                                        crate::model::PitchSource::Scale,
                                        crate::model::PitchSource::Root,
                                    ] {
                                        if ui
                                            .selectable_label(
                                                p.midi_defaults.pitch_source == src,
                                                tr(src.label()),
                                            )
                                            .clicked()
                                        {
                                            p.midi_defaults.pitch_source = src;
                                            changed_song = true;
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            y += ROW_H;
                        }

                        // Quantize sub-accordion
                        let q_sub = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                        if sub_disclosure(
                            ui,
                            &g_upper,
                            q_sub,
                            "defaults_quantize_sub",
                            s.inspector.quantize_open,
                            tr("Quantize"),
                        ) {
                            s.inspector.quantize_open = !s.inspector.quantize_open;
                        }
                        y += ROW_H;
                        if s.inspector.quantize_open {
                            label(&g_upper, label_right, y, tr("Quantize:"), true);
                            let q_text = QUANTIZE
                                .iter()
                                .find(|(_, q)| *q == p.midi_defaults.quantize)
                                .map_or(tr("Off"), |(t, _)| tr(t));
                            if let Some(resp) = popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "defaults_quantize",
                                q_text,
                                true,
                            ) {
                                egui::Popup::menu(&resp).show(|ui| {
                                    for (t, q) in QUANTIZE {
                                        if ui
                                            .selectable_label(p.midi_defaults.quantize == q, tr(t))
                                            .clicked()
                                        {
                                            p.midi_defaults.quantize = q;
                                            changed_song = true;
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            y += ROW_H;
                            let swing_active = p.midi_defaults.quantize.is_some();
                            label(&g_upper, label_right, y, tr("Q-Swing:"), swing_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_q_swing",
                                p.midi_defaults.q_swing.round() as i32,
                                (0, 100),
                                0,
                                if p.midi_defaults.q_swing == 0.0 {
                                    tr("Off").to_string()
                                } else {
                                    format!("{}%", p.midi_defaults.q_swing.round() as i32)
                                },
                                swing_active,
                            ) {
                                p.midi_defaults.q_swing = v as f32;
                                changed_song = true;
                            }
                            y += ROW_H;
                        }

                        // Details sub-accordion
                        let det_sub = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                        if sub_disclosure(
                            ui,
                            &g_upper,
                            det_sub,
                            "defaults_details_sub",
                            s.inspector.details_open,
                            tr("Details"),
                        ) {
                            s.inspector.details_open = !s.inspector.details_open;
                        }
                        y += ROW_H;
                        if s.inspector.details_open {
                            label(&g_upper, label_right, y, tr("Loop:"), true);
                            let looped = p.midi_defaults.loop_beats > 0.0;
                            if checkbox(ui, &g_upper, pos2(vx, y), "defaults_loop", looped, true) {
                                p.midi_defaults.loop_beats = if !looped { 16.0 } else { 0.0 };
                                changed_song = true;
                            }
                            y += ROW_H;
                        }

                        // More disclosure
                        let more = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                        if ui.interact(more, id("more"), Sense::click()).clicked() {
                            s.inspector.more_open = !s.inspector.more_open;
                        }
                        let c = pos2(more.right() - 14.0, more.center().y);
                        let chev = if s.inspector.more_open {
                            vec![
                                c + vec2(-3.5, -1.5),
                                c + vec2(0.0, 2.0),
                                c + vec2(3.5, -1.5),
                            ]
                        } else {
                            vec![
                                c + vec2(-1.5, -3.5),
                                c + vec2(2.0, 0.0),
                                c + vec2(-1.5, 3.5),
                            ]
                        };
                        g_upper.add(Shape::line(chev, Stroke::new(1.2, TEXT_DIM)));
                        g_upper.text(
                            pos2(left + 12.0, more.center().y),
                            Align2::LEFT_CENTER,
                            tr("More"),
                            FontId::proportional(11.5),
                            TEXT_DIM,
                        );
                        y += ROW_H;
                        if s.inspector.more_open {
                            label(&g_upper, label_right, y, tr("Dynamics:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_dynamics",
                                p.midi_defaults.dynamics as i32,
                                (0, 200),
                                100,
                                format!("{}%", p.midi_defaults.dynamics),
                                true,
                            ) {
                                p.midi_defaults.dynamics = v as u16;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Gate Time:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_gate_time",
                                p.midi_defaults.gate_time as i32,
                                (0, 200),
                                100,
                                format!("{}%", p.midi_defaults.gate_time),
                                true,
                            ) {
                                p.midi_defaults.gate_time = v as u16;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Clip Length:"), true);
                            if checkbox(
                                ui,
                                &g_upper,
                                pos2(vx, y),
                                "defaults_clip_length",
                                p.midi_defaults.clip_length,
                                true,
                            ) {
                                p.midi_defaults.clip_length = !p.midi_defaults.clip_length;
                                changed_song = true;
                            }
                            y += ROW_H;

                            let q_active = p.midi_defaults.quantize.is_some();

                            label(&g_upper, label_right, y, tr("Q-Velocity:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_q_velocity",
                                p.midi_defaults.q_velocity.round() as i32,
                                (0, 100),
                                0,
                                format!("{}%", p.midi_defaults.q_velocity.round() as i32),
                                q_active,
                            ) {
                                p.midi_defaults.q_velocity = v as f32;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Length:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_q_length",
                                p.midi_defaults.q_length.round() as i32,
                                (0, 100),
                                0,
                                format!("{}%", p.midi_defaults.q_length.round() as i32),
                                q_active,
                            ) {
                                p.midi_defaults.q_length = v as f32;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Flam:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            let flam_v = p.midi_defaults.q_flam as i32;
                            let flam_text = if flam_v == 0 {
                                "0 ms".to_string()
                            } else if flam_v > 0 {
                                format!("+{flam_v} ms")
                            } else {
                                format!("{flam_v} ms")
                            };
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_q_flam",
                                flam_v,
                                (-50, 50),
                                0,
                                flam_text,
                                q_active,
                            ) {
                                p.midi_defaults.q_flam = v as i16;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Range:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            let range_v = p.midi_defaults.q_range as i32;
                            let range_text = if range_v == 0 {
                                "0%".to_string()
                            } else {
                                format!("{range_v}%")
                            };
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_q_range",
                                range_v,
                                (0, 100),
                                0,
                                range_text,
                                q_active,
                            ) {
                                p.midi_defaults.q_range = v as u8;
                                changed_song = true;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Strength:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "defaults_q_strength",
                                p.midi_defaults.q_strength.round() as i32,
                                (0, 100),
                                100,
                                format!("{}%", p.midi_defaults.q_strength.round() as i32),
                                q_active,
                            ) {
                                p.midi_defaults.q_strength = v as f32;
                                changed_song = true;
                            }
                            y += ROW_H;
                        }
                        y += 4.0;
                    } else if !is_audio {
                        // Context: Selected MIDI Region
                        let primary = sel.unwrap();
                        let before = p.tracks[primary.0].regions[primary.1].params;
                        let mut params = before;

                        label(&g_upper, label_right, y, tr("Mute:"), true);
                        if checkbox(ui, &g_upper, pos2(vx, y), "region_mute", params.mute, true) {
                            params.mute = !params.mute;
                        }
                        y += ROW_H;
                        label(&g_upper, label_right, y, tr("Loop:"), true);
                        let looped = params.loop_beats > 0.0;
                        let loop_click =
                            checkbox(ui, &g_upper, pos2(vx, y), "region_loop", looped, true);
                        y += ROW_H;
                        label(&g_upper, label_right, y, tr("Quantize:"), true);
                        let q_text = QUANTIZE
                            .iter()
                            .find(|(_, q)| *q == params.quantize)
                            .map_or(tr("Off"), |(t, _)| tr(t));
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "quantize",
                            q_text,
                            true,
                        ) {
                            egui::Popup::menu(&resp).show(|ui| {
                                for (t, q) in QUANTIZE {
                                    if ui.selectable_label(params.quantize == q, tr(t)).clicked() {
                                        params.quantize = q;
                                        ui.close();
                                    }
                                }
                            });
                        }
                        y += ROW_H;
                        let swing_active = params.quantize.is_some();
                        label(&g_upper, label_right, y, tr("Q-Swing:"), swing_active);
                        let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                        if let Some(v) = drag_value(
                            ui,
                            &g_upper,
                            r,
                            "region_q_swing",
                            params.q_swing.round() as i32,
                            (0, 100),
                            0,
                            if params.q_swing == 0.0 {
                                tr("Off").to_string()
                            } else {
                                format!("{}%", params.q_swing.round() as i32)
                            },
                            swing_active,
                        ) {
                            params.q_swing = v as f32;
                        }
                        y += ROW_H;
                        label(&g_upper, label_right, y, tr("Transpose:"), true);
                        let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                        if let Some(v) = drag_value(
                            ui,
                            &g_upper,
                            r,
                            "region_transpose",
                            params.transpose as i32,
                            (-96, 96),
                            0,
                            transpose_text(params.transpose as i32),
                            true,
                        ) {
                            params.transpose = v as i8;
                        }
                        y += ROW_H;
                        label(&g_upper, label_right, y, tr("Velocity:"), true);
                        let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                        if let Some(v) = drag_value(
                            ui,
                            &g_upper,
                            r,
                            "region_velocity",
                            params.velocity as i32,
                            (-99, 99),
                            0,
                            velocity_text(params.velocity as i32),
                            true,
                        ) {
                            params.velocity = v as i8;
                        }
                        y += ROW_H;

                        // "More" disclosure
                        let more = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                        if ui.interact(more, id("more"), Sense::click()).clicked() {
                            s.inspector.more_open = !s.inspector.more_open;
                        }
                        let c = pos2(more.right() - 14.0, more.center().y);
                        let chev = if s.inspector.more_open {
                            vec![
                                c + vec2(-3.5, -1.5),
                                c + vec2(0.0, 2.0),
                                c + vec2(3.5, -1.5),
                            ]
                        } else {
                            vec![
                                c + vec2(-1.5, -3.5),
                                c + vec2(2.0, 0.0),
                                c + vec2(-1.5, 3.5),
                            ]
                        };
                        g_upper.add(Shape::line(chev, Stroke::new(1.2, TEXT_DIM)));
                        g_upper.text(
                            pos2(left + 12.0, more.center().y),
                            Align2::LEFT_CENTER,
                            tr("More"),
                            FontId::proportional(11.5),
                            TEXT_DIM,
                        );
                        y += ROW_H;
                        if s.inspector.more_open {
                            label(&g_upper, label_right, y, tr("Dynamics:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_dynamics",
                                params.dynamics as i32,
                                (0, 200),
                                100,
                                format!("{}%", params.dynamics),
                                true,
                            ) {
                                params.dynamics = v as u16;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Gate Time:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_gate_time",
                                params.gate_time as i32,
                                (0, 200),
                                100,
                                format!("{}%", params.gate_time),
                                true,
                            ) {
                                params.gate_time = v as u16;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Clip Length:"), true);
                            if checkbox(
                                ui,
                                &g_upper,
                                pos2(vx, y),
                                "region_clip_length",
                                params.clip_length,
                                true,
                            ) {
                                params.clip_length = !params.clip_length;
                            }
                            y += ROW_H;

                            let q_active = params.quantize.is_some();

                            label(&g_upper, label_right, y, tr("Q-Velocity:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_q_velocity",
                                params.q_velocity.round() as i32,
                                (0, 100),
                                0,
                                format!("{}%", params.q_velocity.round() as i32),
                                q_active,
                            ) {
                                params.q_velocity = v as f32;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Length:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_q_length",
                                params.q_length.round() as i32,
                                (0, 100),
                                0,
                                format!("{}%", params.q_length.round() as i32),
                                q_active,
                            ) {
                                params.q_length = v as f32;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Flam:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            let flam_v = params.q_flam as i32;
                            let flam_text = if flam_v == 0 {
                                "0 ms".to_string()
                            } else if flam_v > 0 {
                                format!("+{flam_v} ms")
                            } else {
                                format!("{flam_v} ms")
                            };
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_q_flam",
                                flam_v,
                                (-50, 50),
                                0,
                                flam_text,
                                q_active,
                            ) {
                                params.q_flam = v as i16;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Range:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            let range_v = params.q_range as i32;
                            let range_text = if range_v == 0 {
                                "0%".to_string()
                            } else {
                                format!("{range_v}%")
                            };
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_q_range",
                                range_v,
                                (0, 100),
                                0,
                                range_text,
                                q_active,
                            ) {
                                params.q_range = v as u8;
                            }
                            y += ROW_H;

                            label(&g_upper, label_right, y, tr("Q-Strength:"), q_active);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "region_q_strength",
                                params.q_strength.round() as i32,
                                (0, 100),
                                100,
                                format!("{}%", params.q_strength.round() as i32),
                                q_active,
                            ) {
                                params.q_strength = v as f32;
                            }
                            y += ROW_H;
                        }
                        if let Some(primary) = sel {
                            changed_song |= crate::region_commands::spread_params(
                                p, &s.tracks, primary, before, params,
                            );
                            if loop_click {
                                changed_song |=
                                    crate::region_commands::set_loop(p, &s.tracks, !looped);
                            }
                        }
                        y += 4.0;
                    } else {
                        // Context: Selected Audio Region
                        let primary = sel.unwrap();
                        let before = p.tracks[primary.0].regions[primary.1].params;
                        let mut params = before;

                        label(&g_upper, label_right, y, tr("Mute:"), true);
                        if checkbox(ui, &g_upper, pos2(vx, y), "region_mute", params.mute, true) {
                            params.mute = !params.mute;
                        }
                        y += ROW_H;
                        label(&g_upper, label_right, y, tr("Loop:"), true);
                        let looped = params.loop_beats > 0.0;
                        let loop_click =
                            checkbox(ui, &g_upper, pos2(vx, y), "region_loop", looped, true);
                        y += ROW_H;

                        let (start_beat, len_beats) = {
                            let reg = &p.tracks[primary.0].regions[primary.1];
                            (p.region_start_beats(reg), p.region_len_beats(reg))
                        };
                        let bpm = p.tempo_at(start_beat);

                        label(&g_upper, label_right, y, tr("Gain:"), true);
                        let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                        if let Some(v) = drag_value(
                            ui,
                            &g_upper,
                            r,
                            "region_gain",
                            (params.gain_db * 2.0).round() as i32,
                            (
                                (MIN_REGION_GAIN * 2.0) as i32,
                                (MAX_REGION_GAIN * 2.0) as i32,
                            ),
                            0,
                            gain_text(params.gain_db),
                            true,
                        ) {
                            params.gain_db = v as f32 / 2.0;
                        }
                        y += ROW_H;

                        let max_fade = (len_beats * FADE_STEPS_PER_BEAT as f64) as i32;
                        for (name, key, which) in [
                            (tr("Fade In:"), "region_fade_in", 0),
                            (tr("Fade Out:"), "region_fade_out", 1),
                        ] {
                            let beats = if which == 0 {
                                params.fade_in
                            } else {
                                params.fade_out
                            };
                            label(&g_upper, label_right, y, name, true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                key,
                                (beats * FADE_STEPS_PER_BEAT as f32).round() as i32,
                                (0, max_fade.max(0)),
                                0,
                                fade_text(beats, bpm),
                                true,
                            ) {
                                let beats = v as f32 / FADE_STEPS_PER_BEAT as f32;
                                if which == 0 {
                                    params.fade_in = beats;
                                } else {
                                    params.fade_out = beats;
                                }
                            }
                            y += ROW_H;
                        }

                        label(&g_upper, label_right, y, tr("Curve:"), true);
                        let curve_name = FADE_CURVES
                            .iter()
                            .find(|(_, c)| *c == params.fade_curve)
                            .map_or(tr("Equal Power"), |(t, _)| tr(t));
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "fade_curve",
                            curve_name,
                            true,
                        ) {
                            egui::Popup::menu(&resp).show(|ui| {
                                for (t, c) in FADE_CURVES {
                                    if ui.selectable_label(params.fade_curve == c, tr(t)).clicked()
                                    {
                                        params.fade_curve = c;
                                        ui.close();
                                    }
                                }
                            });
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("Reverse:"), true);
                        if checkbox(
                            ui,
                            &g_upper,
                            pos2(vx, y),
                            "region_reverse",
                            params.reverse,
                            true,
                        ) {
                            params.reverse = !params.reverse;
                        }
                        y += ROW_H;

                        label(
                            &g_upper,
                            label_right,
                            y,
                            &format!("{}:", tr("X-Fade")),
                            true,
                        );
                        if checkbox(
                            ui,
                            &g_upper,
                            pos2(vx, y),
                            "region_xfade",
                            params.crossfade,
                            true,
                        ) {
                            params.crossfade = !params.crossfade;
                        }
                        y += ROW_H;

                        // "More" disclosure
                        let more = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                        if ui.interact(more, id("more"), Sense::click()).clicked() {
                            s.inspector.more_open = !s.inspector.more_open;
                        }
                        let c = pos2(more.right() - 14.0, more.center().y);
                        let chev = if s.inspector.more_open {
                            vec![
                                c + vec2(-3.5, -1.5),
                                c + vec2(0.0, 2.0),
                                c + vec2(3.5, -1.5),
                            ]
                        } else {
                            vec![
                                c + vec2(-1.5, -3.5),
                                c + vec2(2.0, 0.0),
                                c + vec2(-1.5, 3.5),
                            ]
                        };
                        g_upper.add(Shape::line(chev, Stroke::new(1.2, TEXT_DIM)));
                        g_upper.text(
                            pos2(left + 12.0, more.center().y),
                            Align2::LEFT_CENTER,
                            tr("More"),
                            FontId::proportional(11.5),
                            TEXT_DIM,
                        );
                        y += ROW_H;
                        if s.inspector.more_open {
                            label(&g_upper, label_right, y, "Flex & Follow:", false);
                            popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "flex_follow",
                                tr("Off"),
                                false,
                            );
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Transpose:"), false);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            drag_value(
                                ui,
                                &g_upper,
                                r,
                                "audio_transpose",
                                0,
                                (-96, 96),
                                0,
                                "0 st".into(),
                                false,
                            );
                            y += ROW_H;
                            label(&g_upper, label_right, y, "Speed:", false);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            drag_value(
                                ui,
                                &g_upper,
                                r,
                                "audio_speed",
                                100,
                                (25, 400),
                                100,
                                "100 %".into(),
                                false,
                            );
                            y += ROW_H;
                        }
                        if let Some(primary) = sel {
                            changed_song |= crate::region_commands::spread_params(
                                p, &s.tracks, primary, before, params,
                            );
                            if loop_click {
                                changed_song |=
                                    crate::region_commands::set_loop(p, &s.tracks, !looped);
                            }
                        }
                        y += 4.0;
                    }
                }

                // ---------------- Track
                let ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
                let track_name = p.tracks.get(ti).map_or(String::new(), |t| t.name.clone());
                let hdr = Rect::from_min_size(pos2(left, y), vec2(w, HEADER_H));
                if section_header(
                    ui,
                    &g_upper,
                    hdr,
                    "track",
                    s.inspector.track_open,
                    tr("Track:"),
                    &track_name,
                )
                .clicked()
                {
                    s.inspector.track_open = !s.inspector.track_open;
                }
                y += HEADER_H + 4.0;
                if s.inspector.track_open && ti < p.tracks.len() {
                    let audio = p.tracks[ti].kind == TrackKind::Audio;
                    let mut tp = p.tracks[ti].params;

                    let tgen_r = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                    if sub_disclosure(
                        ui,
                        &g_upper,
                        tgen_r,
                        "track_general",
                        s.inspector.track_general_open,
                        tr("General"),
                    ) {
                        s.inspector.track_general_open = !s.inspector.track_general_open;
                    }
                    y += ROW_H + 2.0;

                    if s.inspector.track_general_open {
                        label(&g_upper, label_right, y, tr("Color:"), true);
                        if let Some(c) = color_pill(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 3.0), vec2(vw, ROW_H - 6.0)),
                            "track_color",
                            p.tracks[ti].color,
                        ) {
                            p.tracks[ti].color = c;
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("Icon:"), true);
                        let icon_r = Rect::from_min_size(pos2(vx, y + 1.0), vec2(26.0, 26.0));
                        g_upper.rect_filled(icon_r, 5.0, Color32::from_rgb(0x18, 0x18, 0x1b));
                        g_upper.rect_stroke(
                            icon_r,
                            5.0,
                            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
                            egui::StrokeKind::Inside,
                        );
                        draw_icon_tinted(
                            &g_upper,
                            icon_r.shrink(4.0),
                            track_icon(&p.tracks[ti]),
                            p.tracks[ti].color,
                        );
                        if ui
                            .interact(icon_r, id("track_icon_click"), Sense::click())
                            .clicked()
                        {
                            s.track_dialog =
                                Some(crate::track_dialogs::TrackDialog::Icon { track: ti });
                        }
                        y += 30.0;

                        label(&g_upper, label_right, y, tr("Default Region Type:"), true);
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "default_region_type",
                            tr(tp.default_region_type.label()),
                            true,
                        ) {
                            egui::Popup::menu(&resp).show(|ui| {
                                for kind in [
                                    crate::model::DefaultRegionType::Select,
                                    crate::model::DefaultRegionType::Midi,
                                    crate::model::DefaultRegionType::Pattern,
                                    crate::model::DefaultRegionType::Audio,
                                ] {
                                    if ui
                                        .selectable_label(
                                            tp.default_region_type == kind,
                                            tr(kind.label()),
                                        )
                                        .clicked()
                                    {
                                        tp.default_region_type = kind;
                                        ui.close();
                                    }
                                }
                            });
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("MIDI Input:"), true);
                        let in_port = p.tracks[ti]
                            .midi_in_port
                            .clone()
                            .unwrap_or_else(|| tr("All").to_string());
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "track_midi_input",
                            &in_port,
                            true,
                        ) {
                            egui::Popup::menu(&resp).show(|ui| {
                                if ui
                                    .selectable_label(
                                        p.tracks[ti].midi_in_port.is_none(),
                                        tr("All"),
                                    )
                                    .clicked()
                                {
                                    p.tracks[ti].midi_in_port = None;
                                    ui.close();
                                }
                                for name in crate::mtc_in::ports() {
                                    let on = p.tracks[ti].midi_in_port.as_deref() == Some(&name);
                                    if ui.selectable_label(on, &name).clicked() {
                                        p.tracks[ti].midi_in_port = Some(name);
                                        ui.close();
                                    }
                                }
                            });
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("Internal MIDI In:"), true);
                        let int_label = tp
                            .internal_midi_in
                            .and_then(|src| p.tracks.get(src))
                            .map_or_else(|| tr("Off").to_string(), |t| t.name.clone());
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "track_internal_midi_in",
                            &int_label,
                            true,
                        ) {
                            egui::Popup::menu(&resp).show(|ui| {
                                if ui
                                    .selectable_label(tp.internal_midi_in.is_none(), tr("Off"))
                                    .clicked()
                                {
                                    tp.internal_midi_in = None;
                                    ui.close();
                                }
                                for (idx, trk) in p.tracks.iter().enumerate() {
                                    if idx == ti {
                                        continue;
                                    }
                                    if ui
                                        .selectable_label(
                                            tp.internal_midi_in == Some(idx),
                                            &trk.name,
                                        )
                                        .clicked()
                                    {
                                        tp.internal_midi_in = Some(idx);
                                        ui.close();
                                    }
                                }
                            });
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("MIDI In Channel:"), true);
                        let in_ch_text = p.tracks[ti]
                            .midi_in_channel
                            .map_or_else(|| tr("All").to_string(), |c| c.to_string());
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "track_midi_in_ch_gen",
                            &in_ch_text,
                            true,
                        ) {
                            egui::Popup::menu(&resp).show(|ui| {
                                if ui
                                    .selectable_label(
                                        p.tracks[ti].midi_in_channel.is_none(),
                                        tr("All"),
                                    )
                                    .clicked()
                                {
                                    p.tracks[ti].midi_in_channel = None;
                                    ui.close();
                                }
                                for ch in 1..=16u8 {
                                    let on = p.tracks[ti].midi_in_channel == Some(ch);
                                    if ui.selectable_label(on, ch.to_string()).clicked() {
                                        p.tracks[ti].midi_in_channel = Some(ch);
                                        ui.close();
                                    }
                                }
                            });
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("MIDI Out Channel:"), true);
                        let out_ch = p.tracks[ti].midi_channel.clamp(1, 16);
                        if let Some(v) = drag_value(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "track_midi_out_ch_gen",
                            out_ch as i32,
                            (1, 16),
                            1,
                            out_ch.to_string(),
                            true,
                        ) {
                            p.tracks[ti].midi_channel = v as u8;
                        }
                        y += ROW_H;

                        label(&g_upper, label_right, y, tr("Delay:"), true);
                        if let Some(v) = drag_value(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "track_delay_gen",
                            tp.delay_ms as i32,
                            (-250, 250),
                            0,
                            delay_text(tp.delay_ms as i32),
                            true,
                        ) {
                            tp.delay_ms = v as i16;
                        }
                        y += ROW_H;
                    }

                    let tmore_r = Rect::from_min_size(pos2(left, y), vec2(w, ROW_H));
                    if sub_disclosure(
                        ui,
                        &g_upper,
                        tmore_r,
                        "track_more",
                        s.inspector.track_more_open,
                        tr("More"),
                    ) {
                        s.inspector.track_more_open = !s.inspector.track_more_open;
                    }
                    y += ROW_H;

                    if s.inspector.track_more_open {
                        let channel = p.channel_name(ti);
                        label(&g_upper, label_right, y, tr("Channel:"), true);
                        if let Some(resp) = popup(
                            ui,
                            &g_upper,
                            Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                            "channel",
                            &channel,
                            true,
                        ) {
                            resp.on_hover_text("Channel strip identifier");
                        }
                        y += ROW_H;
                        if audio {
                            label(&g_upper, label_right, y, tr("Freeze Mode:"), true);
                            let freeze_label = match tp.freeze_mode {
                                crate::model::FreezeMode::PreFader => tr("Pre Fader"),
                                crate::model::FreezeMode::SourceOnly => tr("Source Only"),
                            };
                            if let Some(resp) = popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "freeze",
                                freeze_label,
                                true,
                            ) {
                                egui::Popup::menu(&resp).show(|ui| {
                                    for (fm, lbl) in [
                                        (crate::model::FreezeMode::PreFader, tr("Pre Fader")),
                                        (crate::model::FreezeMode::SourceOnly, tr("Source Only")),
                                    ] {
                                        if ui.selectable_label(tp.freeze_mode == fm, lbl).clicked()
                                        {
                                            p.tracks[ti].params.freeze_mode = fm;
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Q-Reference:"), false);
                            checkbox(ui, &g_upper, pos2(vx, y), "qref", true, false);
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Flex Mode:"), true);
                            let flex_label = match tp.flex_mode {
                                None => tr("Off"),
                                Some(prowave_engine::FlexMode::FlexPitch) => tr("Flex Pitch"),
                                Some(prowave_engine::FlexMode::Monophonic) => {
                                    tr("Flex Time - Monophonic")
                                }
                                Some(prowave_engine::FlexMode::Rhythmic) => {
                                    tr("Flex Time - Rhythmic")
                                }
                                Some(prowave_engine::FlexMode::Polyphonic) => {
                                    tr("Flex Time - Polyphonic")
                                }
                                Some(m) => m.name(),
                            };
                            if let Some(resp) = popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "flex",
                                flex_label,
                                true,
                            ) {
                                egui::Popup::menu(&resp).show(|ui| {
                                    if ui
                                        .selectable_label(tp.flex_mode.is_none(), tr("Off"))
                                        .clicked()
                                    {
                                        p.tracks[ti].params.flex_enabled = false;
                                        p.tracks[ti].params.flex_mode = None;
                                        ui.close();
                                    }
                                    for (mode, lbl) in [
                                        (prowave_engine::FlexMode::FlexPitch, tr("Flex Pitch")),
                                        (
                                            prowave_engine::FlexMode::Monophonic,
                                            tr("Flex Time - Monophonic"),
                                        ),
                                        (
                                            prowave_engine::FlexMode::Rhythmic,
                                            tr("Flex Time - Rhythmic"),
                                        ),
                                        (
                                            prowave_engine::FlexMode::Polyphonic,
                                            tr("Flex Time - Polyphonic"),
                                        ),
                                    ] {
                                        if ui
                                            .selectable_label(tp.flex_mode == Some(mode), lbl)
                                            .clicked()
                                        {
                                            p.tracks[ti].params.flex_enabled = true;
                                            p.tracks[ti].params.flex_mode = Some(mode);
                                            p.tracks[ti].params.freeze_mode =
                                                crate::model::FreezeMode::SourceOnly;
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            y += ROW_H;
                            if tp.flex_mode == Some(prowave_engine::FlexMode::FlexPitch) {
                                label(&g_upper, label_right, y, tr("Formant Track:"), true);
                                if let Some(v) = drag_value(
                                    ui,
                                    &g_upper,
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                    "formant_track",
                                    (tp.formant_track * 100.0).round() as i32,
                                    (0, 100),
                                    0,
                                    format!("{:.2}", tp.formant_track),
                                    true,
                                ) {
                                    p.tracks[ti].params.formant_track = v as f32 / 100.0;
                                }
                                y += ROW_H;
                                label(&g_upper, label_right, y, tr("Formant Shift:"), true);
                                let fs_txt = if tp.formant_shift.abs() < 0.05 {
                                    tr("off").to_string()
                                } else {
                                    format!("{:+.1} st", tp.formant_shift)
                                };
                                if let Some(v) = drag_value(
                                    ui,
                                    &g_upper,
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                    "formant_shift",
                                    (tp.formant_shift * 10.0).round() as i32,
                                    (-120, 120),
                                    0,
                                    fs_txt,
                                    true,
                                ) {
                                    p.tracks[ti].params.formant_shift = v as f32 / 10.0;
                                }
                                y += ROW_H;
                                label(&g_upper, label_right, y, tr("Formants:"), true);
                                if let Some(resp) = popup(
                                    ui,
                                    &g_upper,
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                    "formants_mode",
                                    tr(tp.formants_mode.name()),
                                    true,
                                ) {
                                    egui::Popup::menu(&resp).show(|ui| {
                                        for fm in prowave_engine::FormantMode::ALL {
                                            if ui
                                                .selectable_label(
                                                    tp.formants_mode == fm,
                                                    tr(fm.name()),
                                                )
                                                .clicked()
                                            {
                                                p.tracks[ti].params.formants_mode = fm;
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                                y += ROW_H;
                            }
                        } else {
                            let external = p.tracks[ti].kind == TrackKind::ExternalMidi;
                            if external {
                                // Where this track's notes go: a MIDI output port and a channel.
                                label(&g_upper, label_right, y, tr("MIDI Out Port:"), true);
                                let port = p.tracks[ti]
                                    .midi_port
                                    .clone()
                                    .unwrap_or_else(|| tr("No Port").to_string());
                                if let Some(resp) = popup(
                                    ui,
                                    &g_upper,
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                    "midi_out_port",
                                    &port,
                                    true,
                                ) {
                                    egui::Popup::menu(&resp).show(|ui| {
                                        let current = p.tracks[ti].midi_port.clone();
                                        if ui
                                            .selectable_label(current.is_none(), tr("No Port"))
                                            .clicked()
                                        {
                                            p.tracks[ti].midi_port = None;
                                            ui.close();
                                        }
                                        for name in crate::midi_out::ports() {
                                            if ui
                                                .selectable_label(
                                                    current.as_deref() == Some(&name),
                                                    &name,
                                                )
                                                .clicked()
                                            {
                                                p.tracks[ti].midi_port = Some(name);
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                                y += ROW_H;
                                label(&g_upper, label_right, y, tr("MIDI Out Channel:"), true);
                                let r =
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                                let channel = p.tracks[ti].midi_channel.clamp(1, 16);
                                if let Some(v) = drag_value(
                                    ui,
                                    &g_upper,
                                    r,
                                    "midi_out_channel",
                                    channel as i32,
                                    (1, 16),
                                    1,
                                    channel.to_string(),
                                    true,
                                ) {
                                    p.tracks[ti].midi_channel = v as u8;
                                }
                                y += ROW_H;
                                // The sound sent when the track is routed: a program, and a bank (both optional;
                                // the top of each range is Off).
                                label(&g_upper, label_right, y, tr("MIDI Program:"), true);
                                let program = p.tracks[ti].midi_program.map_or(128, i32::from);
                                if let Some(v) = drag_value(
                                    ui,
                                    &g_upper,
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                    "midi_out_program",
                                    program,
                                    (0, 128),
                                    128,
                                    if program == 128 {
                                        tr("Off").to_string()
                                    } else {
                                        program.to_string()
                                    },
                                    true,
                                ) {
                                    p.tracks[ti].midi_program = (v < 128).then_some(v as u8);
                                }
                                y += ROW_H;
                                label(&g_upper, label_right, y, tr("MIDI Bank:"), true);
                                let bank = p.tracks[ti].midi_bank.map_or(16384, i32::from);
                                if let Some(v) = drag_value(
                                    ui,
                                    &g_upper,
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                    "midi_out_bank",
                                    bank,
                                    (0, 16384),
                                    16384,
                                    if bank == 16384 {
                                        tr("Off").to_string()
                                    } else {
                                        bank.to_string()
                                    },
                                    true,
                                ) {
                                    p.tracks[ti].midi_bank = (v < 16384).then_some(v as u16);
                                }
                                y += ROW_H;
                            }
                            if !external {
                                // The instrument this track plays instead of its synth patch.
                                label(&g_upper, label_right, y, tr("SoundFont:"), true);
                                let current = p.tracks[ti].soundfont.clone();
                                let piano = p.tracks[ti].epiano;
                                let sampler = p.tracks[ti].sampler.clone();
                                let has_drums = p.tracks[ti].drums.is_some();
                                let has_bass808 = p.tracks[ti].bass808.is_some();
                                let marbble_inst_name =
                                    p.tracks[ti].marbble_inst.as_ref().and_then(|m| {
                                        prowave_engine::marbble_dsp::instrument::find(&m.id)
                                            .map(|inf| inf.name.to_string())
                                    });
                                let hosted_inst = p.tracks[ti].hosted_instrument;
                                let shown = match (&current, piano) {
                                    _ if hosted_inst != 0 => {
                                        crate::hosted_plugins::name(hosted_inst).to_string()
                                    }
                                    _ if marbble_inst_name.is_some() => {
                                        marbble_inst_name.clone().unwrap_or_default()
                                    }
                                    _ if has_drums => tr("Drum Machine").to_string(),
                                    _ if has_bass808 => "808 Bass".to_string(),
                                    _ if sampler.is_some() => {
                                        let name = sampler
                                            .as_ref()
                                            .map(|s| s.source.name())
                                            .unwrap_or_default();
                                        format!("{} · {}", tr("Sampler"), name)
                                    }
                                    (_, Some(_)) => tr("Electric Piano").to_string(),
                                    (None, None) => tr("None").to_string(),
                                    (Some(sf), None) => soundfont_label(sf),
                                };
                                let rect =
                                    Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                                if let Some(resp) =
                                    popup(ui, &g_upper, rect, "soundfont", &shown, true)
                                {
                                    if resp.double_clicked() {
                                        if hosted_inst != 0 {
                                            s.mixer.inst_window = Some(ti);
                                            s.mixer.fx_window = None;
                                            s.mixer.plugin_controls = false;
                                        } else if has_drums {
                                            s.show_drum_machine = Some(ti);
                                        } else if marbble_inst_name.is_some() {
                                            s.show_marbble_inst = Some(ti);
                                        } else if has_bass808 {
                                            s.show_bass808 = Some(ti);
                                        }
                                    }
                                    egui::Popup::menu(&resp).show(|ui| {
                                        if ui.button(tr("Choose SoundFont…")).clicked() {
                                            if let Some(path) = rfd::FileDialog::new()
                                                .add_filter("SoundFont", &["sf2", "SF2"])
                                                .pick_file()
                                            {
                                                use_soundfont_file(
                                                    p,
                                                    ti,
                                                    path.to_string_lossy().into_owned(),
                                                );
                                            }
                                            ui.close();
                                        }
                                        if ui.button(tr("Choose Hydrogen Drum Kit…")).clicked() {
                                            // A kit is a downloaded `.h2drumkit` archive, or the `drumkit.xml`
                                            // of one that is already unpacked.
                                            if let Some(path) = rfd::FileDialog::new()
                                                .add_filter(
                                                    "Hydrogen drum kit",
                                                    &["h2drumkit", "xml"],
                                                )
                                                .pick_file()
                                            {
                                                match hydrogen_kit_file(&path) {
                                                    Ok(file) => use_soundfont_file(p, ti, file),
                                                    Err(e) => {
                                                        tracing::warn!("{e}");
                                                        s.status = Some(e);
                                                    }
                                                }
                                            }
                                            ui.close();
                                        }
                                        if ui.button(tr("Electric Piano")).clicked() {
                                            p.tracks[ti].epiano =
                                                Some(crate::model::EPianoUse::default());
                                            p.tracks[ti].soundfont = None;
                                            p.tracks[ti].sampler = None;
                                            p.tracks[ti].drums = None;
                                            p.tracks[ti].bass808 = None;
                                            p.tracks[ti].hosted_instrument = 0;
                                            ui.close();
                                        }
                                        ui.separator();
                                        if ui.button("808 Bass").clicked() {
                                            p.tracks[ti].bass808 =
                                                Some(prowave_engine::Bass808Spec::default());
                                            p.tracks[ti].soundfont = None;
                                            p.tracks[ti].sampler = None;
                                            p.tracks[ti].epiano = None;
                                            p.tracks[ti].drums = None;
                                            p.tracks[ti].hosted_instrument = 0;
                                            s.show_bass808 = Some(ti);
                                            ui.close();
                                        }
                                        if has_bass808 {
                                            if ui.button("Edit 808 Bass…").clicked() {
                                                s.show_bass808 = Some(ti);
                                                ui.close();
                                            }
                                            if ui.button("Remove 808 Bass").clicked() {
                                                p.tracks[ti].bass808 = None;
                                                ui.close();
                                            }
                                        }
                                        ui.separator();
                                        ui.menu_button(tr("Marbble Synths & Sampler"), |ui| {
                                            for info in
                                                prowave_engine::marbble_dsp::instrument::registry()
                                            {
                                                let active = p.tracks[ti]
                                                    .marbble_inst
                                                    .as_ref()
                                                    .is_some_and(|m| m.id == info.id);
                                                if ui.selectable_label(active, info.name).clicked()
                                                {
                                                    p.tracks[ti].marbble_inst = Some(
                                                        crate::model::MarbbleInstUse::new(info.id),
                                                    );
                                                    p.tracks[ti].soundfont = None;
                                                    p.tracks[ti].sampler = None;
                                                    p.tracks[ti].epiano = None;
                                                    p.tracks[ti].drums = None;
                                                    p.tracks[ti].bass808 = None;
                                                    p.tracks[ti].hosted_instrument = 0;
                                                    s.show_marbble_inst = Some(ti);
                                                    ui.close();
                                                }
                                            }
                                            if p.tracks[ti].marbble_inst.is_some() {
                                                ui.separator();
                                                if ui
                                                    .button(tr("Open Marbble Instrument…"))
                                                    .clicked()
                                                {
                                                    s.show_marbble_inst = Some(ti);
                                                    ui.close();
                                                }
                                                if ui
                                                    .button(tr("Remove Marbble Instrument"))
                                                    .clicked()
                                                {
                                                    p.tracks[ti].marbble_inst = None;
                                                    ui.close();
                                                }
                                            }
                                        });
                                        ui.separator();
                                        if ui.button(tr("Drum Machine")).clicked() {
                                            p.tracks[ti].drums =
                                                Some(crate::drum_kit::DrumsUse::new());
                                            p.tracks[ti].sampler = None;
                                            p.tracks[ti].epiano = None;
                                            p.tracks[ti].soundfont = None;
                                            p.tracks[ti].bass808 = None;
                                            p.tracks[ti].hosted_instrument = 0;
                                            s.show_drum_machine = Some(ti);
                                            ui.close();
                                        }
                                        if has_drums {
                                            if ui.button(tr("Edit Drum Machine…")).clicked() {
                                                s.show_drum_machine = Some(ti);
                                                ui.close();
                                            }
                                            if ui.button(tr("Remove Drum Machine")).clicked() {
                                                p.tracks[ti].drums = None;
                                                ui.close();
                                            }
                                        }
                                        let clap_insts: Vec<_> =
                                            crate::hosted_plugins::catalog_instruments()
                                                .into_iter()
                                                .filter(|i| {
                                                    i.format == crate::hosted_plugins::Format::Clap
                                                })
                                                .collect();
                                        if !clap_insts.is_empty() {
                                            ui.separator();
                                            ui.menu_button("CLAP Instruments", |ui| {
                                                for inst in &clap_insts {
                                                    let picked = hosted_inst != 0
                                                        && crate::hosted_plugins::info(hosted_inst)
                                                            .as_ref()
                                                            .map(|i| &i.id)
                                                            == Some(&inst.id);
                                                    if ui
                                                        .selectable_label(picked, &inst.name)
                                                        .clicked()
                                                    {
                                                        let slot =
                                                            crate::hosted_plugins::add(inst, None);
                                                        p.tracks[ti].hosted_instrument = slot;
                                                        p.tracks[ti].soundfont = None;
                                                        p.tracks[ti].sampler = None;
                                                        p.tracks[ti].epiano = None;
                                                        p.tracks[ti].drums = None;
                                                        p.tracks[ti].bass808 = None;
                                                        ui.close();
                                                    }
                                                }
                                            });
                                        }
                                        let vst3_insts: Vec<_> =
                                            crate::hosted_plugins::catalog_instruments()
                                                .into_iter()
                                                .filter(|i| {
                                                    i.format == crate::hosted_plugins::Format::Vst3
                                                })
                                                .collect();
                                        if !vst3_insts.is_empty() {
                                            ui.menu_button("VST3 Instruments", |ui| {
                                                for inst in &vst3_insts {
                                                    let picked = hosted_inst != 0
                                                        && crate::hosted_plugins::info(hosted_inst)
                                                            .as_ref()
                                                            .map(|i| &i.id)
                                                            == Some(&inst.id);
                                                    if ui
                                                        .selectable_label(picked, &inst.name)
                                                        .clicked()
                                                    {
                                                        let slot =
                                                            crate::hosted_plugins::add(inst, None);
                                                        p.tracks[ti].hosted_instrument = slot;
                                                        p.tracks[ti].soundfont = None;
                                                        p.tracks[ti].sampler = None;
                                                        p.tracks[ti].epiano = None;
                                                        p.tracks[ti].drums = None;
                                                        p.tracks[ti].bass808 = None;
                                                        ui.close();
                                                    }
                                                }
                                            });
                                        }
                                        if hosted_inst != 0 {
                                            ui.separator();
                                            if ui
                                                .button(format!(
                                                    "Remove {}",
                                                    crate::hosted_plugins::name(hosted_inst),
                                                ))
                                                .clicked()
                                            {
                                                p.tracks[ti].hosted_instrument = 0;
                                                ui.close();
                                            }
                                        }
                                        ui.separator();
                                        ui.menu_button(tr("Sampler"), |ui| {
                                            use crate::sampler_load::SamplerSource;
                                            if ui.button(tr("Load SFZ Instrument…")).clicked() {
                                                if let Some(path) = rfd::FileDialog::new()
                                                    .add_filter("SFZ", &["sfz", "SFZ"])
                                                    .pick_file()
                                                {
                                                    use_sampler(p, ti, SamplerSource::Sfz(path));
                                                }
                                                ui.close();
                                            }
                                            if ui.button(tr("Load EXS Instrument…")).clicked() {
                                                if let Some(path) = rfd::FileDialog::new()
                                                    .add_filter("EXS", &["exs", "EXS"])
                                                    .pick_file()
                                                {
                                                    use_sampler(p, ti, SamplerSource::Exs(path));
                                                }
                                                ui.close();
                                            }
                                            if ui
                                                .button(tr("Load Audio File (Quick Sampler)…"))
                                                .clicked()
                                            {
                                                if let Some(path) = rfd::FileDialog::new()
                                                    .add_filter(
                                                        "Audio",
                                                        &[
                                                            "wav", "aif", "aiff", "flac", "ogg",
                                                            "mp3", "caf", "m4a",
                                                        ],
                                                    )
                                                    .pick_file()
                                                {
                                                    use_sampler(
                                                        p,
                                                        ti,
                                                        SamplerSource::Audio {
                                                            path,
                                                            root: 60,
                                                            looped: false,
                                                        },
                                                    );
                                                }
                                                ui.close();
                                            }
                                            if let Some(s) = &sampler {
                                                if let Some(e) =
                                                    crate::sampler_load::error(s.source.path())
                                                {
                                                    ui.colored_label(
                                                        egui::Color32::from_rgb(0xe0, 0x70, 0x60),
                                                        crate::i18n::trf("Cannot load: {}", &[&e]),
                                                    );
                                                }
                                                ui.separator();
                                                if ui.button(tr("Remove Sampler")).clicked() {
                                                    p.tracks[ti].sampler = None;
                                                    ui.close();
                                                }
                                            }
                                        });
                                        if piano.is_some() {
                                            ui.separator();
                                            if ui.button(tr("Remove Electric Piano")).clicked() {
                                                p.tracks[ti].epiano = None;
                                                ui.close();
                                            }
                                        }
                                        if let (Some(sf), None, None) =
                                            (current.as_ref(), piano, sampler.as_ref())
                                        {
                                            if let Some(bank) = crate::soundfonts::loaded(&sf.path)
                                            {
                                                ui.separator();
                                                egui::ScrollArea::vertical()
                                                    .max_height(320.0)
                                                    .show(ui, |ui| {
                                                        for preset in bank.presets() {
                                                            let picked = preset.bank == sf.bank
                                                                && preset.program == sf.program;
                                                            if ui
                                                                .selectable_label(
                                                                    picked,
                                                                    &preset.name,
                                                                )
                                                                .clicked()
                                                            {
                                                                p.tracks[ti].soundfont =
                                                                    Some(SoundFontUse {
                                                                        path: sf.path.clone(),
                                                                        bank: preset.bank,
                                                                        program: preset.program,
                                                                    });
                                                                ui.close();
                                                            }
                                                        }
                                                    });
                                            }
                                            ui.separator();
                                            if ui.button(tr("Remove SoundFont")).clicked() {
                                                p.tracks[ti].soundfont = None;
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                                y += ROW_H;
                                if let Some(ep) = p.tracks[ti].epiano {
                                    if let Some(dials) = piano_rows(
                                        ui,
                                        &g_upper,
                                        (label_right, vx, vw),
                                        &mut y,
                                        ep.params,
                                    ) {
                                        p.tracks[ti].epiano =
                                            Some(crate::model::EPianoUse { params: dials });
                                    }
                                }
                                if let Some(current) = p.tracks[ti].sampler.clone() {
                                    if let Some(next) = sampler_rows(
                                        ui,
                                        &g_upper,
                                        (label_right, vx, vw),
                                        &mut y,
                                        &current,
                                    ) {
                                        p.tracks[ti].sampler = Some(next);
                                    }
                                }
                            }
                            // MIDI In Port
                            label(&g_upper, label_right, y, tr("MIDI In Port:"), true);
                            let in_port = p.tracks[ti]
                                .midi_in_port
                                .clone()
                                .unwrap_or_else(|| tr("All").to_string());
                            if let Some(resp) = popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "midi_in_port",
                                &in_port,
                                true,
                            ) {
                                egui::Popup::menu(&resp).show(|ui| {
                                    if ui
                                        .selectable_label(
                                            p.tracks[ti].midi_in_port.is_none(),
                                            tr("All"),
                                        )
                                        .clicked()
                                    {
                                        p.tracks[ti].midi_in_port = None;
                                        ui.close();
                                    }
                                    for name in crate::mtc_in::ports() {
                                        let on =
                                            p.tracks[ti].midi_in_port.as_deref() == Some(&name);
                                        if ui.selectable_label(on, &name).clicked() {
                                            p.tracks[ti].midi_in_port = Some(name);
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            y += ROW_H;

                            // MIDI In Channel
                            label(&g_upper, label_right, y, tr("MIDI In Channel:"), true);
                            let in_ch_text = p.tracks[ti]
                                .midi_in_channel
                                .map_or_else(|| tr("All").to_string(), |c| c.to_string());
                            if let Some(resp) = popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "midi_in_channel",
                                &in_ch_text,
                                true,
                            ) {
                                egui::Popup::menu(&resp).show(|ui| {
                                    if ui
                                        .selectable_label(
                                            p.tracks[ti].midi_in_channel.is_none(),
                                            tr("All"),
                                        )
                                        .clicked()
                                    {
                                        p.tracks[ti].midi_in_channel = None;
                                        ui.close();
                                    }
                                    for ch in 1..=16u8 {
                                        let on = p.tracks[ti].midi_in_channel == Some(ch);
                                        if ui.selectable_label(on, ch.to_string()).clicked() {
                                            p.tracks[ti].midi_in_channel = Some(ch);
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            y += ROW_H;

                            // Freeze Mode
                            label(&g_upper, label_right, y, tr("Freeze Mode:"), true);
                            popup(
                                ui,
                                &g_upper,
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0)),
                                "freeze_mode",
                                tr("Pre Fader"),
                                true,
                            );
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Transpose:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "track_transpose",
                                tp.transpose as i32,
                                (-96, 96),
                                0,
                                transpose_text(tp.transpose as i32),
                                true,
                            ) {
                                tp.transpose = v as i8;
                            }
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Velocity:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "track_velocity",
                                tp.velocity as i32,
                                (-99, 99),
                                0,
                                velocity_text(tp.velocity as i32),
                                true,
                            ) {
                                tp.velocity = v as i8;
                            }
                            y += ROW_H;
                            let half = (vw - 4.0) / 2.0;
                            label(&g_upper, label_right, y, tr("Key Limit:"), true);
                            let (lo, hi) = tp.key_limit;
                            let r1 =
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(half, ROW_H - 4.0));
                            let r2 = Rect::from_min_size(
                                pos2(vx + half + 4.0, y + 2.0),
                                vec2(half, ROW_H - 4.0),
                            );
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r1,
                                "key_lo",
                                lo as i32,
                                (0, hi as i32),
                                0,
                                note_name(lo),
                                true,
                            ) {
                                tp.key_limit.0 = v as u8;
                            }
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r2,
                                "key_hi",
                                hi as i32,
                                (lo as i32, 127),
                                127,
                                note_name(hi),
                                true,
                            ) {
                                tp.key_limit.1 = v as u8;
                            }
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Vel Limit:"), true);
                            let (lo, hi) = tp.vel_limit;
                            let r1 =
                                Rect::from_min_size(pos2(vx, y + 2.0), vec2(half, ROW_H - 4.0));
                            let r2 = Rect::from_min_size(
                                pos2(vx + half + 4.0, y + 2.0),
                                vec2(half, ROW_H - 4.0),
                            );
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r1,
                                "vel_lo",
                                lo as i32,
                                (1, hi as i32),
                                1,
                                lo.to_string(),
                                true,
                            ) {
                                tp.vel_limit.0 = v as u8;
                            }
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r2,
                                "vel_hi",
                                hi as i32,
                                (lo as i32, 127),
                                127,
                                hi.to_string(),
                                true,
                            ) {
                                tp.vel_limit.1 = v as u8;
                            }
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("Delay:"), true);
                            let r = Rect::from_min_size(pos2(vx, y + 2.0), vec2(vw, ROW_H - 4.0));
                            if let Some(v) = drag_value(
                                ui,
                                &g_upper,
                                r,
                                "track_delay",
                                tp.delay_ms as i32,
                                (-250, 250),
                                0,
                                delay_text(tp.delay_ms as i32),
                                true,
                            ) {
                                tp.delay_ms = v as i16;
                            }
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("No Transpose:"), true);
                            if checkbox(
                                ui,
                                &g_upper,
                                pos2(vx, y),
                                "no_transpose",
                                tp.no_transpose,
                                true,
                            ) {
                                tp.no_transpose = !tp.no_transpose;
                            }
                            y += ROW_H;
                            label(&g_upper, label_right, y, tr("No Reset:"), true);
                            if checkbox(ui, &g_upper, pos2(vx, y), "no_reset", tp.no_reset, true) {
                                tp.no_reset = !tp.no_reset;
                            }
                            y += ROW_H;
                        }
                    }
                    if p.tracks[ti].params != tp {
                        p.tracks[ti].params = tp;
                        changed_song = true;
                    }
                    y += 4.0;
                }

                let total_h = (y - start_y).max(min_top_h);
                if (total_h - last_top_h).abs() > 1.0 {
                    ui.ctx().data_mut(|d| d.insert_temp(top_h_id, total_h));
                    ui.ctx().request_repaint();
                }
                ui.allocate_rect(
                    Rect::from_min_size(pos2(left, start_y), vec2(w, total_h)),
                    Sense::hover(),
                );
            });
    });

    // ---------------- Channel strips: selected track + Destination (Aux or Stereo Out)
    if !p.tracks.is_empty() {
        let strips_y = strips_rect.top() + 1.0;
        let actual_strips_h = (strips_rect.height() - 2.0).max(STRIPS_MIN_H);
        let r = rows(strips_y, fx_rows, send_rows);
        let sw = (w / 2.0).floor();
        for (n, key) in keys.iter().enumerate() {
            let strip = Rect::from_min_size(
                pos2(left + n as f32 * sw, strips_y),
                vec2(sw, actual_strips_h),
            );
            channel_strip(
                ui,
                &g,
                strip,
                *key,
                p,
                &mut s.mixer,
                meters,
                &r,
                "inspector",
                NameStyle::Plain,
            );
        }
        // Single 1px vertical divider between the two strips
        let divider_x = left + sw;
        g.line_segment(
            [
                pos2(divider_x, strips_y),
                pos2(divider_x, strips_y + actual_strips_h),
            ],
            Stroke::new(1.0, Color32::from_rgb(0x2c, 0x2c, 0x2e)),
        );
    }

    // 1px right edge divider highlighting on hover
    let right_edge = Rect::from_min_max(
        pos2(panel.right() - 3.0, panel.top()),
        pos2(panel.right() + 3.0, panel.bottom()),
    );
    let edge_hovered = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|pt| right_edge.contains(pt));
    let divider_color = if edge_hovered {
        Color32::from_rgb(0x12, 0x69, 0xcc)
    } else {
        Color32::from_rgb(0x38, 0x38, 0x3c)
    };
    g.line_segment(
        [
            pos2(panel.right() - 0.5, panel.top()),
            pos2(panel.right() - 0.5, panel.bottom()),
        ],
        Stroke::new(1.0, divider_color),
    );

    // Rebuild the engine song once per gesture, not on every frame of a drag.
    s.inspector.pending_song_change |= changed_song;
    if s.inspector.pending_song_change && !ui.input(|i| i.pointer.any_down()) {
        s.inspector.pending_song_change = false;
        p.song_rev += 1;
    }
}

#[cfg(test)]
mod piano_tests {
    use super::*;

    #[test]
    fn every_dial_has_a_row_and_its_text_names_the_unit() {
        let mut seen = [false; prowave_engine::epiano::PARAMS];
        for (dial, _, per_unit, (lo, hi)) in PIANO_ROWS {
            seen[dial] = true;
            // The range of the drag steps is the dial's own range.
            let info = prowave_engine::epiano::DIALS[dial];
            assert!((lo as f32 / per_unit - info.min).abs() < 1e-4, "{dial}");
            assert!((hi as f32 / per_unit - info.max).abs() < 1e-4, "{dial}");
        }
        assert!(seen.iter().all(|s| *s));
        assert_eq!(
            piano_text(prowave_engine::epiano::dial::TREMOLO_RATE, 55),
            "5.5 Hz"
        );
        assert_eq!(
            piano_text(prowave_engine::epiano::dial::LEVEL, -4),
            "-2.0 dB"
        );
        assert_eq!(piano_text(prowave_engine::epiano::dial::BARK, 50), "50 %");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_gain_and_fades_read_like_logic() {
        assert_eq!(gain_text(0.0), "0 dB");
        assert_eq!(gain_text(-3.5), "-3.5 dB");
        assert_eq!(gain_text(2.0), "+2.0 dB");
        assert_eq!(fade_text(0.0, 120.0), "0");
        assert_eq!(fade_text(1.0, 120.0), "500 ms", "a beat at 120 bpm");
    }

    #[test]
    fn a_soundfont_setting_names_its_file_and_says_when_it_is_missing() {
        let sf = SoundFontUse {
            path: "/nonexistent/GeneralMIDI.sf2".into(),
            bank: 0,
            program: 0,
        };
        assert!(soundfont_label(&sf).starts_with("GeneralMIDI.sf2"));
    }

    #[test]
    fn a_soundfont_track_draws_and_keeps_its_setting() {
        let mut p = Project::empty();
        p.tracks.clear();
        let mut t = crate::model::Track::new("Organ", TrackKind::Instrument);
        t.soundfont = Some(SoundFontUse {
            path: "/nonexistent/GeneralMIDI.sf2".into(),
            bank: 0,
            program: 19,
        });
        p.tracks.push(t);
        p.selected_track = 0;
        let mut s = UiState::new();
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(360.0, 900.0);
        for _ in 0..3 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        assert_eq!(
            p.tracks[0].soundfont.as_ref().map(|s| s.program),
            Some(19),
            "drawing the row changes nothing"
        );
    }

    #[test]
    fn an_electric_piano_track_draws_its_dials_and_keeps_them() {
        let mut p = Project::empty();
        p.tracks.clear();
        let mut t = crate::model::Track::new("Wurli", TrackKind::Instrument);
        let mut dials = prowave_engine::epiano::defaults();
        dials[prowave_engine::epiano::dial::BARK] = 0.8;
        t.epiano = Some(crate::model::EPianoUse { params: dials });
        p.tracks.push(t);
        p.selected_track = 0;
        let mut s = UiState::new();
        let meters = Meters::default();
        let mut h = crate::ui_test::Harness::new(360.0, 900.0);
        for _ in 0..3 {
            h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
        }
        assert_eq!(
            p.tracks[0].epiano,
            Some(crate::model::EPianoUse { params: dials }),
            "drawing the dials changes nothing"
        );
    }

    #[test]
    fn a_sampler_track_draws_its_rows_and_keeps_its_setting_and_the_helpers_name_things() {
        use crate::model::SamplerUse;
        use crate::sampler_load::SamplerSource;
        assert_eq!(key_name(60), "C3");
        assert_eq!(key_name(69), "A3");
        assert_eq!(key_name(0), "C-2");
        assert_eq!(key_name(127), "G8");
        assert_eq!(
            sampler_text(prowave_engine::sampler::dial::LEVEL, -5),
            "-2.5 dB"
        );
        assert_eq!(
            sampler_text(prowave_engine::sampler::dial::RELEASE, 80),
            "80 ms"
        );
        for (dial, _, per_unit, (lo, hi)) in SAMPLER_ROWS {
            let info = prowave_engine::sampler::DIALS[dial];
            assert!((lo as f32 / per_unit - info.min).abs() < 1e-4, "{dial}");
            assert!((hi as f32 / per_unit - info.max).abs() < 1e-4, "{dial}");
        }
        for source in [
            SamplerSource::Audio {
                path: "/s/pad.wav".into(),
                root: 60,
                looped: false,
            },
            SamplerSource::Sfz("/s/kit.sfz".into()),
        ] {
            let mut p = Project::empty();
            p.tracks.clear();
            let mut t = crate::model::Track::new("Samples", TrackKind::Instrument);
            t.sampler = Some(SamplerUse::new(source));
            let before = t.sampler.clone();
            p.tracks.push(t);
            p.selected_track = 0;
            let mut s = UiState::new();
            let meters = Meters::default();
            let mut h = crate::ui_test::Harness::new(360.0, 900.0);
            for _ in 0..3 {
                h.frame(vec![], |ui| show(ui, &mut p, &mut s, &meters));
            }
            assert_eq!(
                p.tracks[0].sampler, before,
                "drawing the rows changes nothing"
            );
        }
    }

    #[test]
    fn numeric_box_labels_show_units_and_never_blank() {
        assert_eq!(transpose_text(0), "0 st");
        assert_eq!(transpose_text(12), "+12 st");
        assert_eq!(transpose_text(-7), "-7 st");

        assert_eq!(velocity_text(0), "0");
        assert_eq!(velocity_text(15), "+15");
        assert_eq!(velocity_text(-20), "-20");

        assert_eq!(delay_text(0), "0 ms");
        assert_eq!(delay_text(25), "+25 ms");
        assert_eq!(delay_text(-10), "-10 ms");

        assert_eq!(signed(0), "0");
        assert_eq!(signed(5), "+5");
        assert_eq!(signed(-8), "-8");
    }

    #[test]
    fn midi_defaults_are_inherited_by_new_regions() {
        let mut p = Project::empty();
        p.tracks
            .push(crate::model::Track::new("Piano", TrackKind::Instrument));
        p.midi_defaults.transpose = 7;
        p.midi_defaults.velocity = 15;
        p.midi_defaults.dynamics = 120;
        p.midi_defaults.gate_time = 80;
        p.midi_defaults.clip_length = false;
        p.midi_defaults.loop_beats = 16.0;
        p.midi_defaults.q_velocity = 45.0;
        p.midi_defaults.q_length = 60.0;
        p.midi_defaults.q_flam = 15;
        p.midi_defaults.q_range = 25;
        p.midi_defaults.q_strength = 80.0;

        let r_idx = p.add_empty_region(0, 1.0).unwrap();
        let reg = &p.tracks[0].regions[r_idx];
        assert_eq!(reg.params.transpose, 7);
        assert_eq!(reg.params.velocity, 15);
        assert_eq!(reg.params.dynamics, 120);
        assert_eq!(reg.params.gate_time, 80);
        assert!(!reg.params.clip_length);
        assert_eq!(reg.params.loop_beats, 16.0);
        assert_eq!(reg.params.q_velocity, 45.0);
        assert_eq!(reg.params.q_length, 60.0);
        assert_eq!(reg.params.q_flam, 15);
        assert_eq!(reg.params.q_range, 25);
        assert_eq!(reg.params.q_strength, 80.0);
    }

    #[test]
    fn channel_strip_destination_selection() {
        let mut p = Project::empty();
        p.tracks.clear();
        p.tracks
            .push(crate::model::Track::new("Audio 1", TrackKind::Audio));
        p.tracks
            .push(crate::model::Track::new("Aux 1", TrackKind::Aux));
        p.tracks[1].aux_bus = 1;

        // Output routes to Bus 1 -> destination is Aux 1
        p.tracks[0].output = 1;
        let dest = if p.tracks[0].output > 0 {
            (0..p.tracks.len())
                .find(|&i| p.aux_bus_of(i) == p.tracks[0].output)
                .map(StripKey::Track)
                .unwrap_or(StripKey::StereoOut)
        } else {
            StripKey::StereoOut
        };
        assert_eq!(dest, StripKey::Track(1));

        // Output routes to StereoOut -> destination is StereoOut
        p.tracks[0].output = 0;
        let dest = if p.tracks[0].output > 0 {
            (0..p.tracks.len())
                .find(|&i| p.aux_bus_of(i) == p.tracks[0].output)
                .map(StripKey::Track)
                .unwrap_or(StripKey::StereoOut)
        } else {
            StripKey::StereoOut
        };
        assert_eq!(dest, StripKey::StereoOut);
    }
}
