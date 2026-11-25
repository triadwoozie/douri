//! Moog-inspired GPU-accelerated rotary knobs derived from AudioKnobs
//! (https://github.com/Megaemce/AudioKnobs by Megaemce).
use crate::i18n::tr;
use egui::{
    pos2, vec2, Align2, Color32, Context, CursorIcon, FontId, Id, Painter, Pos2, Rect, Response,
    Sense, Shape, Stroke, Ui,
};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoogKnobStyle {
    Scalloped,
    ChickenHead,
}

#[derive(Clone, Copy, Debug)]
pub struct MoogKnobConfig<'a> {
    pub id: Id,
    pub label: &'a str,
    pub help_id: &'static str,
    pub style: MoogKnobStyle,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub log: bool,
    pub unit: &'a str,
    pub diameter: f32,
}

pub struct MoogKnobResult {
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

/// Draws and interacts with a Moog-style rotary knob inside `cell`.
pub fn draw(
    ui: &mut Ui,
    g: &Painter,
    cell: Rect,
    cfg: MoogKnobConfig<'_>,
    current_val: f32,
    midi_cc: Option<u8>,
) -> MoogKnobResult {
    let radius = (cfg.diameter * 0.5).clamp(20.0, 28.0);
    let center = pos2(cell.center().x, cell.center().y + 2.0);
    let hit_rect = Rect::from_center_size(center, vec2(radius * 2.4, radius * 2.4));

    let resp = ui.interact(hit_rect, cfg.id, Sense::click_and_drag());
    crate::quick_help::hover(ui.ctx(), &resp, cfg.help_id);

    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
    }

    let mut val = current_val.clamp(cfg.min, cfg.max);
    let mut norm = to_norm(cfg.min, cfg.max, cfg.log, val);
    let mut changed = false;

    if resp.double_clicked() {
        val = cfg.default;
        norm = to_norm(cfg.min, cfg.max, cfg.log, val);
        changed = true;
    } else if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) {
            0.2
        } else {
            1.0
        };
        let delta_vec = resp.drag_delta();
        // Vertical drag primary; subtle horizontal circular drag when pointer moves sideways
        let delta = (-delta_vec.y + delta_vec.x * 0.35) * 0.005 * fine;
        norm = (norm + delta).clamp(0.0, 1.0);
        val = from_norm(cfg.min, cfg.max, cfg.log, norm);
        changed = true;
    } else if resp.hovered() {
        let (scroll, ctrl_cmd) = ui.input(|i| {
            (
                i.smooth_scroll_delta.y,
                i.modifiers.ctrl || i.modifiers.command,
            )
        });
        if scroll.abs() > 0.1 {
            let step = if ctrl_cmd { 0.0008 } else { 0.0018 };
            norm = (norm + scroll * step).clamp(0.0, 1.0);
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

    // Paint the Moog knob geometry
    // Angle: -150 deg (bottom-left) to +150 deg (bottom-right), 0 deg is straight up (-PI/2)
    let angle = -PI * 0.5 + (norm - 0.5) * (300.0_f32.to_radians());
    paint_ticks(g, center, radius);

    match cfg.style {
        MoogKnobStyle::Scalloped => {
            paint_scalloped(g, center, radius, angle, resp.hovered() || resp.dragged())
        }
        MoogKnobStyle::ChickenHead => {
            paint_chicken_head(g, center, radius, angle, resp.hovered() || resp.dragged())
        }
    }

    // Live formatted value below on hover/drag (or MIDI CC badge if assigned)
    if resp.hovered() || resp.dragged() {
        g.text(
            pos2(cell.center().x, center.y + radius + 12.0),
            Align2::CENTER_CENTER,
            format_value(cfg.unit, val),
            FontId::monospace(10.5),
            Color32::from_rgb(0xd8, 0xe6, 0xf8),
        );
    } else if let Some(cc) = midi_cc {
        g.text(
            pos2(cell.center().x, center.y + radius + 12.0),
            Align2::CENTER_CENTER,
            format!("CC {cc}"),
            FontId::monospace(9.5),
            Color32::from_rgb(0x8e, 0xc2, 0xff),
        );
    }

    MoogKnobResult {
        changed,
        value: val,
        response: resp,
        midi_learn_clicked,
    }
}

/// 11 radial perimeter tick marks (-150° to +150°).
fn paint_ticks(g: &Painter, c: Pos2, r: f32) {
    let tick_col = Color32::from_rgb(0x5a, 0x5d, 0x64);
    let inner = r + 3.0;
    let outer = r + 7.0;
    for i in 0..11 {
        let frac = i as f32 / 10.0;
        let a = -PI * 0.5 + (frac - 0.5) * (300.0_f32.to_radians());
        let p0 = pos2(c.x + a.cos() * inner, c.y + a.sin() * inner);
        let p1 = pos2(c.x + a.cos() * outer, c.y + a.sin() * outer);
        let stroke_w = if i == 0 || i == 5 || i == 10 {
            1.4
        } else {
            1.0
        };
        g.line_segment([p0, p1], Stroke::new(stroke_w, tick_col));
    }
}

fn paint_scalloped(g: &Painter, c: Pos2, r: f32, angle: f32, active: bool) {
    // Soft ambient drop shadow
    g.circle_filled(c + vec2(0.0, 2.5), r + 1.5, Color32::from_black_alpha(115));

    // Dark glossy outer skirt with warm specular top-left highlight
    g.circle_filled(c, r, Color32::from_rgb(0x18, 0x18, 0x1b));
    g.circle_stroke(
        c,
        r,
        Stroke::new(
            1.2,
            if active {
                Color32::from_rgb(0x5a, 0x8d, 0xc8)
            } else {
                Color32::from_rgb(0x2a, 0x26, 0x27)
            },
        ),
    );
    // Warm top-left rim arc
    let rim_pts: Vec<Pos2> = (0..=16)
        .map(|i| {
            let a = PI * 0.85 + (i as f32 / 16.0) * (PI * 0.75);
            pos2(c.x + a.cos() * (r - 1.0), c.y + a.sin() * (r - 1.0))
        })
        .collect();
    g.add(Shape::line(
        rim_pts,
        Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 240, 230, 48)),
    ));

    // Rotating 6-lobe scalloped grip collar
    let collar_r = r * 0.79;
    let pts: Vec<Pos2> = (0..72)
        .map(|i| {
            let theta = (i as f32 / 72.0) * TAU;
            let lobe = 1.0 + 0.10 * (6.0 * (theta - angle)).cos();
            let rad = collar_r * lobe;
            pos2(c.x + theta.cos() * rad, c.y + theta.sin() * rad)
        })
        .collect();
    g.add(Shape::convex_polygon(
        pts,
        Color32::from_rgb(0x24, 0x25, 0x29),
        Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3b, 0x42)),
    ));

    // Brushed aluminum center cap
    let cap_r = r * 0.56;
    g.circle_filled(c, cap_r, Color32::from_rgb(0x8e, 0x91, 0x96));
    // Metallic sheen inner highlight
    g.circle_filled(
        c - vec2(cap_r * 0.14, cap_r * 0.14),
        cap_r * 0.76,
        Color32::from_rgb(0xc6, 0xc9, 0xce),
    );
    g.circle_stroke(
        c,
        cap_r,
        Stroke::new(1.0, Color32::from_rgb(0x55, 0x57, 0x5d)),
    );

    // Crisp white indicator dots on collar and skirt
    let dot_skirt = pos2(
        c.x + angle.cos() * (r * 0.86),
        c.y + angle.sin() * (r * 0.86),
    );
    let dot_cap = pos2(
        c.x + angle.cos() * (cap_r * 0.68),
        c.y + angle.sin() * (cap_r * 0.68),
    );
    g.circle_filled(dot_skirt, 2.0, Color32::from_rgb(0xf4, 0xf5, 0xf7));
    g.circle_filled(dot_cap, 1.6, Color32::from_rgb(0x20, 0x22, 0x26));
}

fn paint_chicken_head(g: &Painter, c: Pos2, r: f32, angle: f32, active: bool) {
    // Soft ambient drop shadow
    g.circle_filled(c + vec2(0.0, 2.5), r + 1.0, Color32::from_black_alpha(115));

    // Dark glossy circular base skirt
    let base_r = r * 0.84;
    g.circle_filled(c, base_r, Color32::from_rgb(0x18, 0x18, 0x1b));
    g.circle_stroke(
        c,
        base_r,
        Stroke::new(
            1.2,
            if active {
                Color32::from_rgb(0x5a, 0x8d, 0xc8)
            } else {
                Color32::from_rgb(0x2d, 0x2a, 0x2c)
            },
        ),
    );

    // Rotating dark molded chicken-head pointer body
    let cos = angle.cos();
    let sin = angle.sin();
    let perp_x = -sin;
    let perp_y = cos;

    let tip_len = r * 1.02;
    let tail_len = r * 0.54;
    let tip_w = r * 0.14;
    let tail_w = r * 0.34;

    let body = vec![
        pos2(
            c.x + cos * tip_len + perp_x * tip_w,
            c.y + sin * tip_len + perp_y * tip_w,
        ),
        pos2(
            c.x + cos * tip_len - perp_x * tip_w,
            c.y + sin * tip_len - perp_y * tip_w,
        ),
        pos2(
            c.x - cos * tail_len - perp_x * tail_w,
            c.y - sin * tail_len - perp_y * tail_w,
        ),
        pos2(
            c.x - cos * tail_len + perp_x * tail_w,
            c.y - sin * tail_len + perp_y * tail_w,
        ),
    ];
    g.add(Shape::convex_polygon(
        body,
        Color32::from_rgb(0x23, 0x24, 0x28),
        Stroke::new(1.0, Color32::from_rgb(0x42, 0x43, 0x4a)),
    ));

    // Inlaid cream/ivory indicator bar extending toward outer dial tick
    let bar_start = pos2(c.x + cos * (r * 0.32), c.y + sin * (r * 0.32));
    let bar_end = pos2(c.x + cos * (r * 0.96), c.y + sin * (r * 0.96));
    g.line_segment(
        [bar_start, bar_end],
        Stroke::new(2.4, Color32::from_rgb(0xdc, 0xd8, 0xc8)),
    );

    // Brushed aluminum center cap
    let cap_r = r * 0.34;
    g.circle_filled(c, cap_r, Color32::from_rgb(0xb2, 0xb5, 0xbb));
    g.circle_stroke(
        c,
        cap_r,
        Stroke::new(1.0, Color32::from_rgb(0x52, 0x54, 0x5a)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_round_trip_linear_and_log() {
        for log in [false, true] {
            let (min, max) = (20.0, 20000.0);
            for n in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
                let v = from_norm(min, max, log, n);
                let n2 = to_norm(min, max, log, v);
                assert!((n - n2).abs() < 1e-4, "log={log} n={n} n2={n2}");
            }
        }
    }
}
