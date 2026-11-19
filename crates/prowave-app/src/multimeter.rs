// Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! MultiMeter: ITU-R BS.1770 / EBU R128 Loudness and True Peak Analyzer.
//!
//! Provides real-time momentary (400 ms), short-term (3 s), and gated integrated LUFS
//! measurements alongside 4× polyphase oversampled True Peak (dBTP) detection.

use egui::{pos2, vec2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui};

use crate::engine_sync::EngineSync;
use crate::i18n::tr;
use crate::mixer::Meters;

const WIN_BG: Color32 = Color32::from_rgb(0x1e, 0x20, 0x24);
const CARD_BG: Color32 = Color32::from_rgb(0x27, 0x2a, 0x30);
const BORDER_COLOR: Color32 = Color32::from_rgb(0x3a, 0x3e, 0x48);
const TEXT_MAIN: Color32 = Color32::from_rgb(0xee, 0xf0, 0xf4);
const TEXT_DIM: Color32 = Color32::from_rgb(0x90, 0x98, 0xa6);
const CLIP_RED: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a);
const TARGET_BLUE: Color32 = Color32::from_rgb(0x0a, 0x84, 0xff);
const LUFS_GREEN: Color32 = Color32::from_rgb(0x30, 0xd1, 0x58);
const LUFS_YELLOW: Color32 = Color32::from_rgb(0xff, 0xd6, 0x0a);

/// Renders the MultiMeter analyzer window.
pub fn show(ctx: &egui::Context, open: &mut bool, meters: &Meters, sync: &mut Option<EngineSync>) {
    if !*open {
        return;
    }

    let frame = egui::Frame::NONE
        .fill(WIN_BG)
        .stroke(Stroke::new(1.0, BORDER_COLOR))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(16, 14));

    crate::liquid_glass::glass_window(tr("MultiMeter · Loudness & True Peak"))
        .id(egui::Id::unique("multimeter_window"))
        .open(open)
        .resizable(false)
        .frame(frame)
        .show(ctx, |ui| {
            render_content(ui, meters, sync);
        });
}

fn render_content(ui: &mut Ui, meters: &Meters, sync: &mut Option<EngineSync>) {
    ui.set_width(420.0);

    // Top readout cards: True Peak & Integrated LUFS
    ui.horizontal(|ui| {
        // True Peak Card
        let tp = meters.master_true_peak_db;
        let is_clipped = tp > 0.0;
        card(ui, 202.0, 72.0, |ui| {
            ui.label(RichText::new("TRUE PEAK").size(10.0).color(TEXT_DIM));
            let tp_text = if tp <= -100.0 {
                "-∞ dBTP".to_string()
            } else {
                format!("{tp:+.1} dBTP")
            };
            let tp_col = if is_clipped { CLIP_RED } else { TEXT_MAIN };
            ui.label(RichText::new(tp_text).size(20.0).strong().color(tp_col));
            if is_clipped {
                ui.label(RichText::new("CLIP DETECTED").size(9.0).color(CLIP_RED));
            } else {
                ui.label(
                    RichText::new("Inter-sample 4× Peak")
                        .size(9.0)
                        .color(TEXT_DIM),
                );
            }
        });

        ui.add_space(8.0);

        // Integrated Loudness Card
        let lufs_i = meters.master_lufs_integrated;
        card(ui, 202.0, 72.0, |ui| {
            ui.label(RichText::new("INTEGRATED LUFS").size(10.0).color(TEXT_DIM));
            let lufs_text = if lufs_i <= -100.0 {
                "-∞ LUFS".to_string()
            } else {
                format!("{lufs_i:.1} LUFS")
            };
            ui.label(
                RichText::new(lufs_text)
                    .size(20.0)
                    .strong()
                    .color(TARGET_BLUE),
            );
            ui.label(
                RichText::new("Target: -14 (Stream) · -23 (EBU)")
                    .size(9.0)
                    .color(TEXT_DIM),
            );
        });
    });

    ui.add_space(12.0);

    // Momentary and Short-term visual meter bars
    ui.label(
        RichText::new("LOUDNESS METERS (ITU-R BS.1770-4)")
            .size(11.0)
            .strong()
            .color(TEXT_DIM),
    );
    ui.add_space(4.0);

    meter_bar(
        ui,
        "Momentary (400 ms)",
        meters.master_lufs_momentary,
        -14.0,
    );
    ui.add_space(6.0);
    meter_bar(ui, "Short-Term (3 s)", meters.master_lufs_short, -14.0);

    ui.add_space(14.0);

    // Bottom controls
    ui.horizontal(|ui| {
        if ui.button(RichText::new("Reset Meter").size(11.0)).clicked() {
            if let Some(s) = sync.as_mut() {
                s.reset_loudness_meter();
            }
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("Gated relative: -10 LU · Absolute: -70 LUFS")
                .size(10.0)
                .color(TEXT_DIM),
        );
    });
}

fn card(ui: &mut Ui, w: f32, h: f32, add_contents: impl FnOnce(&mut Ui)) {
    let frame = egui::Frame::NONE
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, BORDER_COLOR))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8));

    frame.show(ui, |ui| {
        ui.set_min_size(vec2(w, h));
        ui.set_max_size(vec2(w, h));
        add_contents(ui);
    });
}

fn meter_bar(ui: &mut Ui, label: &str, lufs: f32, target: f32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(TEXT_MAIN));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let val = if lufs <= -100.0 {
                "-∞".to_string()
            } else {
                format!("{lufs:.1} LUFS")
            };
            ui.label(RichText::new(val).size(10.5).monospace().color(TEXT_DIM));
        });
    });

    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    let g = ui.painter();

    // Background track
    g.rect_filled(
        rect,
        CornerRadius::same(3),
        Color32::from_rgb(0x16, 0x18, 0x1b),
    );
    g.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.0, BORDER_COLOR),
        egui::StrokeKind::Inside,
    );

    // Scale mapping: -60 LUFS .. 0 LUFS -> 0.0 .. 1.0
    let norm = ((lufs + 60.0) / 60.0).clamp(0.0, 1.0);
    if norm > 0.005 {
        let bar_w = rect.width() * norm;
        let fill_rect = Rect::from_min_size(rect.min, vec2(bar_w, rect.height()));
        let fill_col = if lufs > -9.0 {
            LUFS_YELLOW
        } else if lufs > 0.0 {
            CLIP_RED
        } else {
            LUFS_GREEN
        };
        g.rect_filled(fill_rect, CornerRadius::same(3), fill_col);
    }

    // Target marker line (e.g. -14 LUFS)
    let target_norm = ((target + 60.0) / 60.0).clamp(0.0, 1.0);
    let target_x = rect.left() + rect.width() * target_norm;
    g.line_segment(
        [pos2(target_x, rect.top()), pos2(target_x, rect.bottom())],
        Stroke::new(1.5, TARGET_BLUE),
    );
}
