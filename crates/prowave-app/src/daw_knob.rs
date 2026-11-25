//! Clean modern flat-bevel vector DAW rotary knobs and inline parameter readout
//! designed after Dicky Jiang's DAW UI Design (CC BY 4.0) with interaction ergonomics
//! ported from g200kg/webaudio-controls (Apache-2.0).
use crate::i18n::tr;
use egui::{
    pos2, vec2, Align2, Color32, CursorIcon, FontId, Id, Key, Painter, Pos2, Rect, Response, Sense,
    Shape, Stroke, Ui,
};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DawKnobStyle {
    Continuous,
    Stepped { steps: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct DawKnobConfig<'a> {
    pub id: Id,
    pub label: &'a str,
    pub help_id: &'static str,
    pub style: DawKnobStyle,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub log: bool,
    pub bipolar: bool,
    pub accent: bool,
    pub unit: &'a str,
    pub diameter: f32,
}

pub struct DawKnobResult {
    pub changed: bool,
    pub value: f32,
    pub response: Response,
    pub midi_learn_clicked: bool,
}

pub fn to_norm(min: f32, max: f32, log: bool, val: f32) -> f32 {
    let v = val.clamp(min, max);
    if log && min > 0.0 {
        ((v / min).ln() / (max / min).ln()).clamp(0.0, 1.0)
    } else if (max - min).abs() > 1e-9 {
        ((v - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn from_norm(min: f32, max: f32, log: bool, n: f32) -> f32 {
    let n = n.clamp(0.0, 1.0);
    if log && min > 0.0 {
        min * (max / min).powf(n)
    } else {
        min + (max - min) * n
    }
}

pub fn format_value(unit: &str, val: f32) -> String {
    match unit {
        "Hz" => {
            if val >= 1000.0 {
                format!("{:.1} kHz", val / 1000.0)
            } else {
                format!("{:.0} Hz", val)
            }
        }
        "dB" => format!("{:+.1} dB", val),
        "ms" => {
            if val >= 100.0 {
                format!("{:.0} ms", val)
            } else {
                format!("{:.1} ms", val)
            }
        }
        "%" => format!("{:.0}%", val),
        "" => format!("{:.2}", val),
        other => format!("{:.1} {other}", val),
    }
}

/// Parses an inline parameter readout entry (supporting `Hz`, `kHz`, `ms`, `s`, `dB`, `%` suffixes).
pub fn parse_param_text(raw: &str, unit: &str, min: f32, max: f32) -> Option<f32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let (num_str, scale) = if let Some(s) = lower.strip_suffix("khz") {
        (s.trim(), if unit == "Hz" { 1000.0 } else { 1.0 })
    } else if let Some(s) = lower.strip_suffix("hz") {
        (s.trim(), 1.0)
    } else if let Some(s) = lower.strip_suffix("ms") {
        (s.trim(), 1.0)
    } else if let Some(s) = lower.strip_suffix('s') {
        (s.trim(), if unit == "ms" { 1000.0 } else { 1.0 })
    } else if let Some(s) = lower.strip_suffix("db") {
        (s.trim(), 1.0)
    } else if let Some(s) = lower.strip_suffix('%') {
        (s.trim(), 1.0)
    } else {
        (lower.as_str(), 1.0)
    };
    let parsed: f32 = num_str.parse().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    Some((parsed * scale).clamp(min, max))
}

fn snap_stepped(norm: f32, style: DawKnobStyle) -> f32 {
    match style {
        DawKnobStyle::Continuous => norm,
        DawKnobStyle::Stepped { steps } => {
            let n = steps.max(2) - 1;
            (norm * n as f32).round() / n as f32
        }
    }
}

/// Draws and interacts with a modern flat-bevel vector DAW rotary knob inside `cell`.
pub fn draw(
    ui: &mut Ui,
    g: &Painter,
    cell: Rect,
    cfg: DawKnobConfig<'_>,
    current_val: f32,
    midi_cc: Option<u8>,
) -> DawKnobResult {
    let radius = (cfg.diameter * 0.5).clamp(18.0, 28.0);
    let center = pos2(cell.center().x, cell.center().y + 2.0);
    let hit_rect = Rect::from_center_size(center, vec2(radius * 2.4, radius * 2.4));

    let resp = ui.interact(hit_rect, cfg.id, Sense::click_and_drag());
    crate::quick_help::hover(ui.ctx(), &resp, cfg.help_id);

    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(if resp.dragged() {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        });
    }

    let mut val = current_val.clamp(cfg.min, cfg.max);
    let mut norm = snap_stepped(to_norm(cfg.min, cfg.max, cfg.log, val), cfg.style);
    let mut changed = false;

    let ctrl_cmd = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
    if resp.double_clicked() || (resp.clicked() && ctrl_cmd) {
        val = cfg.default;
        norm = snap_stepped(to_norm(cfg.min, cfg.max, cfg.log, val), cfg.style);
        changed = true;
    } else if resp.dragged() {
        resp.request_focus();
        let fine = if ui.input(|i| i.modifiers.shift) {
            0.1
        } else {
            1.0
        };
        let delta_vec = resp.drag_delta();
        let delta = (-delta_vec.y + delta_vec.x * 0.35) * 0.005 * fine;
        norm = snap_stepped((norm + delta).clamp(0.0, 1.0), cfg.style);
        val = from_norm(cfg.min, cfg.max, cfg.log, norm);
        changed = true;
    } else if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.1 {
            let step = match cfg.style {
                DawKnobStyle::Stepped { steps } => {
                    let s = 1.0 / (steps.max(2) - 1) as f32;
                    if scroll > 0.0 {
                        s
                    } else {
                        -s
                    }
                }
                DawKnobStyle::Continuous => {
                    let mult = if ctrl_cmd { 0.0005 } else { 0.0018 };
                    scroll * mult
                }
            };
            norm = snap_stepped((norm + step).clamp(0.0, 1.0), cfg.style);
            val = from_norm(cfg.min, cfg.max, cfg.log, norm);
            changed = true;
        }
    }

    if resp.has_focus() {
        let (up, down) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
        if up || down {
            let dir = if up { 1.0 } else { -1.0 };
            let step = match cfg.style {
                DawKnobStyle::Stepped { steps } => dir / (steps.max(2) - 1) as f32,
                DawKnobStyle::Continuous => dir * if ctrl_cmd { 0.005 } else { 0.02 },
            };
            norm = snap_stepped((norm + step).clamp(0.0, 1.0), cfg.style);
            val = from_norm(cfg.min, cfg.max, cfg.log, norm);
            changed = true;
        }
    }

    let mut midi_learn_clicked = false;
    resp.context_menu(|ui| {
        if ui.button(tr("Reset")).clicked() {
            val = cfg.default;
            changed = true;
            ui.close();
        }
        let learn_label = if let Some(cc) = midi_cc {
            format!("MIDI Learn (CC {cc})")
        } else {
            "MIDI Learn".to_string()
        };
        if ui.button(learn_label).clicked() {
            midi_learn_clicked = true;
            ui.close();
        }
    });

    // Uppercase parameter title above the knob
    g.text(
        pos2(cell.center().x, center.y - radius - 14.0),
        Align2::CENTER_CENTER,
        cfg.label.to_ascii_uppercase(),
        FontId::proportional(10.5),
        if resp.hovered() || resp.dragged() {
            Color32::from_rgb(0xf5, 0xf5, 0xf7)
        } else {
            Color32::from_rgb(0xe5, 0xe5, 0xea)
        },
    );

    paint_daw_knob(
        g,
        center,
        radius,
        norm,
        cfg.style,
        cfg.bipolar,
        cfg.accent,
        resp.hovered() || resp.dragged(),
    );

    // Interactive value readout below the knob (click/double-click for inline text edit)
    let readout_rect = Rect::from_center_size(
        pos2(cell.center().x, center.y + radius + 12.0),
        vec2(cell.width().max(56.0), 16.0),
    );
    let readout_id = cfg.id.with("readout_edit");
    if let Some(edited) = crate::widgets::daw_param_readout(
        ui,
        g,
        readout_rect,
        readout_id,
        &format_value(cfg.unit, val),
        cfg.unit,
        cfg.min,
        cfg.max,
        resp.hovered() || resp.dragged(),
        midi_cc,
    ) {
        val = edited;
        changed = true;
    }

    DawKnobResult {
        changed,
        value: val,
        response: resp,
        midi_learn_clicked,
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_daw_knob(
    g: &Painter,
    c: Pos2,
    r: f32,
    norm: f32,
    style: DawKnobStyle,
    bipolar: bool,
    accent: bool,
    active: bool,
) {
    let arc_col = if accent {
        Color32::from_rgb(0x51, 0xee, 0xfc)
    } else {
        Color32::from_rgb(0x12, 0x69, 0xcc)
    };
    let track_r = r + 3.0;
    let start_deg = -135.0_f32;
    let sweep_deg = 270.0_f32;

    match style {
        DawKnobStyle::Continuous => {
            // Subtle graduation ticks outside the recessed track
            let tick_in = track_r + 3.0;
            let tick_out = track_r + 5.5;
            for i in 0..=10 {
                let frac = i as f32 / 10.0;
                let a = (start_deg + frac * sweep_deg - 90.0).to_radians();
                let d = vec2(a.cos(), a.sin());
                let w = if i == 0 || i == 5 || i == 10 {
                    1.3
                } else {
                    0.9
                };
                g.line_segment(
                    [c + d * tick_in, c + d * tick_out],
                    Stroke::new(w, Color32::from_rgb(0x4a, 0x4d, 0x54)),
                );
            }

            // Recessed circular track groove (3.0px, #303030)
            let steps = 32;
            let groove_pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let frac = i as f32 / steps as f32;
                    let a = (start_deg + frac * sweep_deg - 90.0).to_radians();
                    c + vec2(a.cos(), a.sin()) * track_r
                })
                .collect();
            g.add(Shape::line(
                groove_pts,
                Stroke::new(3.0, Color32::from_rgb(0x30, 0x30, 0x30)),
            ));

            // Illuminated active value arc
            let (arc_a0, arc_a1) = if bipolar { (0.5, norm) } else { (0.0, norm) };
            if (arc_a1 - arc_a0).abs() > 0.005 {
                let segs = 24;
                let active_pts: Vec<Pos2> = (0..=segs)
                    .map(|i| {
                        let frac = arc_a0 + (arc_a1 - arc_a0) * (i as f32 / segs as f32);
                        let a = (start_deg + frac * sweep_deg - 90.0).to_radians();
                        c + vec2(a.cos(), a.sin()) * track_r
                    })
                    .collect();
                g.add(Shape::line(active_pts, Stroke::new(3.0, arc_col)));
            }
        }
        DawKnobStyle::Stepped { steps } => {
            let count = steps.max(2);
            let active_idx = (norm * (count - 1) as f32).round() as usize;
            let dot_r = track_r + 2.5;
            for i in 0..count {
                let frac = i as f32 / (count - 1) as f32;
                let a = (start_deg + frac * sweep_deg - 90.0).to_radians();
                let pos = c + vec2(a.cos(), a.sin()) * dot_r;
                let is_on = i == active_idx;
                g.circle_filled(
                    pos,
                    if is_on { 2.6 } else { 1.8 },
                    if is_on {
                        arc_col
                    } else {
                        Color32::from_rgb(0x4a, 0x4d, 0x54)
                    },
                );
            }
        }
    }

    // Drop shadow beneath dial cap
    let cap_r = (r - 1.0).max(10.0);
    g.circle_filled(c + vec2(0.0, 2.0), cap_r, Color32::from_black_alpha(110));

    // Matte dark dial cap (#26262a fill, #3a3a40 rim highlight)
    g.circle_filled(c, cap_r, Color32::from_rgb(0x26, 0x26, 0x2a));
    g.circle_stroke(
        c,
        cap_r,
        Stroke::new(
            1.0,
            if active {
                Color32::from_rgb(0x52, 0x54, 0x5c)
            } else {
                Color32::from_rgb(0x3a, 0x3a, 0x40)
            },
        ),
    );

    // Crisp white radial pointer bar (2.0px wide)
    let angle = -PI * 0.5 + (norm - 0.5) * (270.0_f32.to_radians());
    let dir = vec2(angle.cos(), angle.sin());
    g.line_segment(
        [c + dir * (cap_r * 0.28), c + dir * (cap_r - 2.0)],
        Stroke::new(2.0, Color32::WHITE),
    );
}
