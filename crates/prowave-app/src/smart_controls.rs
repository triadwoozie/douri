//! Smart Controls bottom pane (Logic Pro `B` shortcut): an 8-knob macro control surface
//! plus a live mini Channel EQ frequency curve mapped directly to the selected track's
//! Channel EQ, Compressor, Overdrive, Sends, and Output volume.
use crate::i18n::tr;
use crate::model::{Insert, Project, SendSlot, Track};
use crate::widgets::{BLUE, TEXT, TEXT_DIM};
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui,
};
use prowave_engine::{eq_idx, FxKind};
use std::f32::consts::PI;

const PANEL_BG: Color32 = Color32::from_rgb(0x1b, 0x1c, 0x1f);
const SURFACE_BG: Color32 = Color32::from_rgb(0x23, 0x24, 0x28);
const BORDER: Color32 = Color32::from_rgb(0x36, 0x37, 0x3d);
const KNOB_BODY: Color32 = Color32::from_rgb(0x2d, 0x2f, 0x35);
const KNOB_TRACK: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
const KNOB_ACCENT: Color32 = Color32::from_rgb(0x3a, 0x9b, 0xff);

#[derive(Clone, Copy)]
struct MacroKnob {
    id: &'static str,
    label: &'static str,
    help_id: &'static str,
    min: f32,
    max: f32,
    default: f32,
    log: bool,
    unit: &'static str,
}

const KNOBS: [MacroKnob; 8] = [
    MacroKnob {
        id: "sc_tone",
        label: "Tone",
        help_id: "sc_tone",
        min: -12.0,
        max: 12.0,
        default: 0.0,
        log: false,
        unit: "dB",
    },
    MacroKnob {
        id: "sc_cutoff",
        label: "Filter Cutoff",
        help_id: "sc_cutoff",
        min: 120.0,
        max: 20000.0,
        default: 20000.0,
        log: true,
        unit: "Hz",
    },
    MacroKnob {
        id: "sc_res",
        label: "Resonance",
        help_id: "sc_res",
        min: 0.1,
        max: 10.0,
        default: 0.71,
        log: true,
        unit: "",
    },
    MacroKnob {
        id: "sc_attack",
        label: "Attack",
        help_id: "sc_attack",
        min: 0.1,
        max: 100.0,
        default: 10.0,
        log: true,
        unit: "ms",
    },
    MacroKnob {
        id: "sc_release",
        label: "Release",
        help_id: "sc_release",
        min: 10.0,
        max: 1000.0,
        default: 120.0,
        log: true,
        unit: "ms",
    },
    MacroKnob {
        id: "sc_drive",
        label: "Drive",
        help_id: "sc_drive",
        min: 0.0,
        max: 24.0,
        default: 0.0,
        log: false,
        unit: "dB",
    },
    MacroKnob {
        id: "sc_send1",
        label: "Send 1",
        help_id: "sc_send1",
        min: 0.0,
        max: 100.0,
        default: 0.0,
        log: false,
        unit: "%",
    },
    MacroKnob {
        id: "sc_output",
        label: "Output",
        help_id: "sc_output",
        min: -60.0,
        max: 6.0,
        default: 0.0,
        log: false,
        unit: "dB",
    },
];

fn to_norm(k: MacroKnob, val: f32) -> f32 {
    let v = val.clamp(k.min, k.max);
    if k.log && k.min > 0.0 {
        ((v / k.min).ln() / (k.max / k.min).ln()).clamp(0.0, 1.0)
    } else {
        ((v - k.min) / (k.max - k.min)).clamp(0.0, 1.0)
    }
}

fn from_norm(k: MacroKnob, n: f32) -> f32 {
    let n = n.clamp(0.0, 1.0);
    if k.log && k.min > 0.0 {
        k.min * (k.max / k.min).powf(n)
    } else {
        k.min + (k.max - k.min) * n
    }
}

fn format_val(k: MacroKnob, val: f32) -> String {
    match k.unit {
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
        _ => format!("{:.2}", val),
    }
}

fn find_insert(t: &Track, kind: FxKind) -> Option<usize> {
    t.inserts.iter().position(|ins| ins.kind == kind)
}

fn ensure_insert(t: &mut Track, kind: FxKind) -> Option<usize> {
    if let Some(idx) = find_insert(t, kind) {
        return Some(idx);
    }
    if t.inserts.len() < prowave_engine::MAX_INSERTS {
        t.inserts.push(Insert::new(kind));
        return Some(t.inserts.len() - 1);
    }
    None
}

fn read_macro(t: &Track, idx: usize) -> f32 {
    match idx {
        // Tone -> Channel EQ High Shelf Gain
        0 => find_insert(t, FxKind::ChannelEq)
            .map(|i| t.inserts[i].params[eq_idx::HIGH_SHELF_GAIN])
            .unwrap_or(0.0),
        // Filter Cutoff -> Filter Cutoff if Filter insert exists, else Channel EQ LPF Freq
        1 => {
            if let Some(i) = find_insert(t, FxKind::Filter) {
                t.inserts[i].params[0]
            } else if let Some(i) = find_insert(t, FxKind::ChannelEq) {
                if t.inserts[i].params[eq_idx::LPF_ON] >= 0.5 {
                    t.inserts[i].params[eq_idx::LPF_FREQ]
                } else {
                    20000.0
                }
            } else {
                20000.0
            }
        }
        // Resonance -> Filter Resonance if Filter insert exists, else Channel EQ LPF Q
        2 => {
            if let Some(i) = find_insert(t, FxKind::Filter) {
                t.inserts[i].params[1]
            } else if let Some(i) = find_insert(t, FxKind::ChannelEq) {
                t.inserts[i].params[eq_idx::LPF_Q]
            } else {
                0.71
            }
        }
        // Attack -> Compressor Attack (params[2])
        3 => find_insert(t, FxKind::Compressor)
            .map(|i| t.inserts[i].params[2])
            .unwrap_or(10.0),
        // Release -> Compressor Release (params[3])
        4 => find_insert(t, FxKind::Compressor)
            .map(|i| t.inserts[i].params[3])
            .unwrap_or(120.0),
        // Drive -> Overdrive Drive (params[0])
        5 => find_insert(t, FxKind::Overdrive)
            .filter(|&i| !t.inserts[i].bypass)
            .map(|i| t.inserts[i].params[0])
            .unwrap_or(0.0),
        // Send 1 -> sends[0].level * 100
        6 => t.sends.first().map(|s| s.level * 100.0).unwrap_or(0.0),
        // Output -> volume_db
        7 => t.volume_db,
        _ => 0.0,
    }
}

fn write_macro(t: &mut Track, idx: usize, val: f32) {
    match idx {
        0 => {
            if let Some(i) = ensure_insert(t, FxKind::ChannelEq) {
                t.inserts[i].params[eq_idx::HIGH_SHELF_ON] = 1.0;
                t.inserts[i].params[eq_idx::HIGH_SHELF_GAIN] = val.clamp(-24.0, 24.0);
                t.inserts[i].bypass = false;
            }
        }
        1 => {
            if let Some(i) = find_insert(t, FxKind::Filter) {
                t.inserts[i].params[0] = val.clamp(20.0, 20000.0);
                t.inserts[i].bypass = false;
            } else if let Some(i) = ensure_insert(t, FxKind::ChannelEq) {
                t.inserts[i].params[eq_idx::LPF_FREQ] = val.clamp(20.0, 20000.0);
                t.inserts[i].params[eq_idx::LPF_ON] = if val < 19500.0 { 1.0 } else { 0.0 };
                t.inserts[i].bypass = false;
            }
        }
        2 => {
            if let Some(i) = find_insert(t, FxKind::Filter) {
                t.inserts[i].params[1] = val.clamp(0.5, 10.0);
                t.inserts[i].bypass = false;
            } else if let Some(i) = ensure_insert(t, FxKind::ChannelEq) {
                t.inserts[i].params[eq_idx::LPF_Q] = val.clamp(0.1, 10.0);
                t.inserts[i].bypass = false;
            }
        }
        3 => {
            if let Some(i) = ensure_insert(t, FxKind::Compressor) {
                t.inserts[i].params[2] = val.clamp(0.1, 100.0);
                t.inserts[i].bypass = false;
            }
        }
        4 => {
            if let Some(i) = ensure_insert(t, FxKind::Compressor) {
                t.inserts[i].params[3] = val.clamp(10.0, 1000.0);
                t.inserts[i].bypass = false;
            }
        }
        5 => {
            if let Some(i) = ensure_insert(t, FxKind::Overdrive) {
                t.inserts[i].params[0] = val.clamp(0.0, 40.0);
                t.inserts[i].bypass = val <= 0.01;
            }
        }
        6 => {
            let level = (val / 100.0).clamp(0.0, 1.0);
            if t.sends.is_empty() {
                if level > 0.001 {
                    t.sends.push(SendSlot {
                        bus: Some(1),
                        level,
                        pre: false,
                    });
                }
            } else {
                if t.sends[0].bus.is_none() && level > 0.001 {
                    t.sends[0].bus = Some(1);
                }
                t.sends[0].level = level;
            }
        }
        7 => {
            t.volume_db = val.clamp(-60.0, 6.0);
        }
        _ => {}
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    let g = ui.painter_at(area);
    g.rect_filled(area, 0.0, PANEL_BG);
    g.line_segment(
        [area.left_top(), area.right_top()],
        Stroke::new(1.0, Color32::from_rgb(0x11, 0x11, 0x13)),
    );

    let ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
    if p.tracks.is_empty() {
        g.text(
            area.center(),
            Align2::CENTER_CENTER,
            tr("Smart Controls"),
            FontId::proportional(13.0),
            TEXT_DIM,
        );
        return;
    }

    // Header strip
    let header_h = 30.0;
    let header = Rect::from_min_size(area.min, vec2(area.width(), header_h));
    g.rect_filled(header, 0.0, Color32::from_rgb(0x22, 0x23, 0x27));
    g.line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, BORDER),
    );

    let track_name = p.tracks[ti].name.clone();
    let track_color = p.tracks[ti].color;
    g.circle_filled(
        pos2(header.left() + 14.0, header.center().y),
        4.5,
        track_color,
    );
    g.text(
        pos2(header.left() + 26.0, header.center().y),
        Align2::LEFT_CENTER,
        format!("{}  -  {}", tr("Smart Controls"), track_name),
        FontId::proportional(12.5),
        TEXT,
    );

    // "Open Channel EQ" pill button on right of header
    let eq_btn = Rect::from_center_size(
        pos2(header.right() - 70.0, header.center().y),
        vec2(116.0, 20.0),
    );
    let eq_resp = ui.interact(eq_btn, ui.scope_id().with("sc_open_eq"), Sense::click());
    crate::quick_help::hover(ui.ctx(), &eq_resp, "eq_thumb");
    g.rect_filled(
        eq_btn,
        4.0,
        if eq_resp.hovered() {
            Color32::from_rgb(0x3a, 0x3b, 0x42)
        } else {
            Color32::from_rgb(0x2c, 0x2d, 0x32)
        },
    );
    g.rect_stroke(eq_btn, 4.0, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    g.text(
        eq_btn.center(),
        Align2::CENTER_CENTER,
        tr("Channel EQ"),
        FontId::proportional(11.0),
        TEXT,
    );
    if eq_resp.clicked() {
        let eq_slot = ensure_insert(&mut p.tracks[ti], FxKind::ChannelEq);
        if let Some(slot) = eq_slot {
            s.mixer.fx_window = Some((ti, slot));
            s.mixer.inst_window = None;
            s.mixer.plugin_controls = false;
        }
    }

    // Main body: left = 8 macro knobs (2x4 or 1x8 depending on width), right = Mini EQ Curve
    let body = Rect::from_min_max(pos2(area.left(), header.bottom()), area.max).shrink(12.0);
    if body.width() < 200.0 || body.height() < 60.0 {
        return;
    }

    let eq_w = (body.width() * 0.28).clamp(180.0, 300.0);
    let knobs_rect = Rect::from_min_max(body.min, pos2(body.right() - eq_w - 12.0, body.max.y));
    let eq_rect = Rect::from_min_max(pos2(knobs_rect.right() + 12.0, body.top()), body.max);

    // Surface background for knobs
    g.rect_filled(knobs_rect, CornerRadius::same(8), SURFACE_BG);
    g.rect_stroke(
        knobs_rect,
        CornerRadius::same(8),
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );

    // Arrange 8 knobs in 2 rows of 4 if height allows (>= 150px), else 1 row of 8
    let two_rows = knobs_rect.height() >= 150.0 && knobs_rect.width() < 860.0;
    let cols = if two_rows { 4 } else { 8 };
    let rows = if two_rows { 2 } else { 1 };
    let cell_w = knobs_rect.width() / cols as f32;
    let cell_h = knobs_rect.height() / rows as f32;

    for (idx, knob) in KNOBS.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        let cell = Rect::from_min_size(
            pos2(
                knobs_rect.left() + col as f32 * cell_w,
                knobs_rect.top() + row as f32 * cell_h,
            ),
            vec2(cell_w, cell_h),
        );
        let radius = (cell_h * 0.25).clamp(16.0, 26.0);
        let center = pos2(cell.center().x, cell.center().y - 4.0);
        let hit_rect = Rect::from_center_size(center, vec2(radius * 2.4, radius * 2.4));

        let resp = ui.interact(
            hit_rect,
            ui.scope_id().with(knob.id),
            Sense::click_and_drag(),
        );
        crate::quick_help::hover(ui.ctx(), &resp, knob.help_id);

        let mut val = read_macro(&p.tracks[ti], idx);
        let mut norm = to_norm(*knob, val);
        let mut changed = false;

        if resp.double_clicked() {
            val = knob.default;
            norm = to_norm(*knob, val);
            changed = true;
        } else if resp.dragged() {
            let fine = if ui.input(|i| i.modifiers.shift) {
                0.25
            } else {
                1.0
            };
            let delta = -resp.drag_delta().y * 0.005 * fine;
            norm = (norm + delta).clamp(0.0, 1.0);
            val = from_norm(*knob, norm);
            changed = true;
        } else if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                norm = (norm + scroll * 0.0015).clamp(0.0, 1.0);
                val = from_norm(*knob, norm);
                changed = true;
            }
        }

        if changed {
            write_macro(&mut p.tracks[ti], idx, val);
        }

        // Draw knob label above
        g.text(
            pos2(cell.center().x, center.y - radius - 10.0),
            Align2::CENTER_CENTER,
            tr(knob.label),
            FontId::proportional(11.0),
            if resp.hovered() || resp.dragged() {
                TEXT
            } else {
                TEXT_DIM
            },
        );

        // Draw rotary arc (from 135 deg to 405 deg = 270 deg sweep)
        let start_ang = 0.75 * PI;
        let sweep = 1.5 * PI;
        let steps = 28;
        let arc_r = radius + 4.0;

        let track_pts: Vec<_> = (0..=steps)
            .map(|s_i| {
                let a = start_ang + sweep * (s_i as f32 / steps as f32);
                pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r)
            })
            .collect();
        g.add(egui::Shape::line(track_pts, Stroke::new(3.0, KNOB_TRACK)));

        let active_steps = ((norm * steps as f32).round() as usize).max(1);
        if norm > 0.01 {
            let active_pts: Vec<_> = (0..=active_steps)
                .map(|s_i| {
                    let t = (s_i as f32 / steps as f32).min(norm);
                    let a = start_ang + sweep * t;
                    pos2(center.x + a.cos() * arc_r, center.y + a.sin() * arc_r)
                })
                .collect();
            g.add(egui::Shape::line(active_pts, Stroke::new(3.0, KNOB_ACCENT)));
        }

        // Knob cap
        g.circle_filled(center, radius, KNOB_BODY);
        g.circle_stroke(
            center,
            radius,
            Stroke::new(
                1.0,
                if resp.hovered() || resp.dragged() {
                    BLUE
                } else {
                    BORDER
                },
            ),
        );

        // Pointer tick
        let ang = start_ang + sweep * norm;
        let p0 = pos2(
            center.x + ang.cos() * (radius * 0.35),
            center.y + ang.sin() * (radius * 0.35),
        );
        let p1 = pos2(
            center.x + ang.cos() * (radius * 0.85),
            center.y + ang.sin() * (radius * 0.85),
        );
        g.line_segment([p0, p1], Stroke::new(2.0, Color32::WHITE));

        // Draw value readout below
        g.text(
            pos2(cell.center().x, center.y + radius + 12.0),
            Align2::CENTER_CENTER,
            format_val(*knob, val),
            FontId::monospace(10.5),
            TEXT_DIM,
        );
    }

    // Mini EQ Curve panel on the right
    g.rect_filled(eq_rect, CornerRadius::same(8), SURFACE_BG);
    g.rect_stroke(
        eq_rect,
        CornerRadius::same(8),
        Stroke::new(1.0, BORDER),
        StrokeKind::Inside,
    );
    let eq_resp = ui.interact(eq_rect, ui.scope_id().with("sc_mini_eq"), Sense::click());
    crate::quick_help::hover(ui.ctx(), &eq_resp, "eq_thumb");
    if eq_resp.clicked() {
        if let Some(slot) = ensure_insert(&mut p.tracks[ti], FxKind::ChannelEq) {
            s.mixer.fx_window = Some((ti, slot));
            s.mixer.inst_window = None;
            s.mixer.plugin_controls = false;
        }
    }

    let plot = eq_rect.shrink2(vec2(10.0, 22.0));
    g.text(
        pos2(eq_rect.left() + 10.0, eq_rect.top() + 11.0),
        Align2::LEFT_CENTER,
        tr("EQ"),
        FontId::proportional(11.0),
        TEXT_DIM,
    );

    // 0 dB centerline
    let zero_y = plot.center().y;
    g.line_segment(
        [pos2(plot.left(), zero_y), pos2(plot.right(), zero_y)],
        Stroke::new(1.0, Color32::from_rgb(0x32, 0x34, 0x3a)),
    );

    // Compute approximate EQ curve if ChannelEq exists, else flat 0 dB
    let eq_params = find_insert(&p.tracks[ti], FxKind::ChannelEq).map(|i| p.tracks[ti].inserts[i]);
    let samples = 64;
    let mut curve_pts = Vec::with_capacity(samples + 1);
    for i in 0..=samples {
        let frac = i as f32 / samples as f32;
        let freq = 20.0 * (1000.0_f32).powf(frac);
        let mut db = 0.0_f32;
        if let Some(ins) = eq_params {
            if !ins.bypass {
                let prm = &ins.params;
                // HPF
                if prm[eq_idx::HPF_ON] >= 0.5 && freq < prm[eq_idx::HPF_FREQ] {
                    let oct = (prm[eq_idx::HPF_FREQ] / freq.max(1.0)).log2().max(0.0);
                    db -= (oct * 12.0).min(24.0);
                }
                // LPF
                if prm[eq_idx::LPF_ON] >= 0.5 && freq > prm[eq_idx::LPF_FREQ] {
                    let oct = (freq / prm[eq_idx::LPF_FREQ].max(1.0)).log2().max(0.0);
                    db -= (oct * 12.0).min(24.0);
                }
                // Low Shelf
                if prm[eq_idx::LOW_SHELF_ON] >= 0.5 {
                    let ratio = freq / prm[eq_idx::LOW_SHELF_FREQ].max(1.0);
                    let w = 1.0 / (1.0 + ratio * ratio);
                    db += prm[eq_idx::LOW_SHELF_GAIN] * w;
                }
                // High Shelf
                if prm[eq_idx::HIGH_SHELF_ON] >= 0.5 {
                    let ratio = prm[eq_idx::HIGH_SHELF_FREQ].max(1.0) / freq;
                    let w = 1.0 / (1.0 + ratio * ratio);
                    db += prm[eq_idx::HIGH_SHELF_GAIN] * w;
                }
                // 4 Bell bands
                for &(on_i, f_i, g_i, q_i) in &[
                    (
                        eq_idx::LOW_MID_ON,
                        eq_idx::LOW_MID_FREQ,
                        eq_idx::LOW_MID_GAIN,
                        eq_idx::LOW_MID_Q,
                    ),
                    (
                        eq_idx::MID_ON,
                        eq_idx::MID_FREQ,
                        eq_idx::MID_GAIN,
                        eq_idx::MID_Q,
                    ),
                    (
                        eq_idx::HIGH_MID_ON,
                        eq_idx::HIGH_MID_FREQ,
                        eq_idx::HIGH_MID_GAIN,
                        eq_idx::HIGH_MID_Q,
                    ),
                    (
                        eq_idx::UPPER_MID_ON,
                        eq_idx::UPPER_MID_FREQ,
                        eq_idx::UPPER_MID_GAIN,
                        eq_idx::UPPER_MID_Q,
                    ),
                ] {
                    if prm[on_i] >= 0.5 {
                        let d = (freq / prm[f_i].max(1.0)).ln() * prm[q_i].max(0.1) * 2.0;
                        db += prm[g_i] * (-d * d).exp();
                    }
                }
                db += prm[eq_idx::MASTER_GAIN];
            }
        }
        let y = (zero_y - (db.clamp(-18.0, 18.0) / 18.0) * (plot.height() * 0.46))
            .clamp(plot.top(), plot.bottom());
        curve_pts.push(pos2(plot.left() + frac * plot.width(), y));
    }
    g.add(egui::Shape::line(
        curve_pts,
        Stroke::new(2.0, Color32::from_rgb(0x4b, 0xa3, 0xff)),
    ));
}
