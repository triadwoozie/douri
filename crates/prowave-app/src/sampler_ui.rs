// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Transient-Slicing Sampler UI: 3-tab dark layout (`Pads`, `Slices`, `Instrument`).
//!
//! Provides interactive editing of drum pad grids, transient loop slicing, and pitched chromatic zones.

use prowave_engine::sampler::{detect_transients, slice_into_zones, LoopMode, SamplerMode, Zone};
use std::sync::Arc;

pub const DRUM_PAD_NAMES: [&str; 16] = [
    "Pad 1 (C1: Kick)",
    "Pad 2 (C#1: Side Stick)",
    "Pad 3 (D1: Snare)",
    "Pad 4 (D#1: Clap)",
    "Pad 5 (E1: Snare 2)",
    "Pad 6 (F1: Low Tom)",
    "Pad 7 (F#1: Closed Hat)",
    "Pad 8 (G1: Low Tom 2)",
    "Pad 9 (G#1: Pedal Hat)",
    "Pad 10 (A1: Mid Tom)",
    "Pad 11 (A#1: Open Hat)",
    "Pad 12 (B1: Mid Tom 2)",
    "Pad 13 (C2: High Tom)",
    "Pad 14 (C#2: Crash 1)",
    "Pad 15 (D2: High Tom 2)",
    "Pad 16 (D#2: Ride 1)",
];

/// The view state of the Sampler editor window.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplerUiState {
    pub mode: SamplerMode,
    pub sensitivity: f32,
    pub selected_pad: usize,
    pub detected_slices: Vec<usize>,
    pub slice_preview_key: Option<u8>,
}

impl Default for SamplerUiState {
    fn default() -> Self {
        Self {
            mode: SamplerMode::Instrument,
            sensitivity: 0.65,
            selected_pad: 0,
            detected_slices: Vec::new(),
            slice_preview_key: None,
        }
    }
}

/// Renders the three-tab Sampler user interface.
pub fn show_sampler_ui(
    ui: &mut egui::Ui,
    state: &mut SamplerUiState,
    zones: &mut Vec<Zone>,
    active_audition: &mut Option<(u8, u8)>, // (pitch, vel)
) {
    ui.horizontal(|ui| {
        for (m, title, icon) in [
            (SamplerMode::Pads, "Pads", "▦"),
            (SamplerMode::Slices, "Slices", "✂"),
            (SamplerMode::Instrument, "Instrument", "🎹"),
        ] {
            let active = state.mode == m;
            let (bg, text_color) = if active {
                (
                    egui::Color32::from_rgb(0x32, 0x36, 0x42),
                    egui::Color32::WHITE,
                )
            } else {
                (
                    egui::Color32::from_rgb(0x20, 0x22, 0x26),
                    egui::Color32::from_rgb(0x8A, 0x8E, 0x99),
                )
            };

            let btn = egui::Button::new(format!("{icon}  {title}"))
                .fill(bg)
                .corner_radius(egui::CornerRadius::same(6))
                .min_size(egui::vec2(90.0, 26.0));

            if ui.add(btn).clicked() {
                state.mode = m;
            }
        }
    });

    ui.add_space(8.0);

    match state.mode {
        SamplerMode::Pads => show_pads_tab(ui, state, zones, active_audition),
        SamplerMode::Slices => show_slices_tab(ui, state, zones, active_audition),
        SamplerMode::Instrument => show_instrument_tab(ui, zones),
    }
}

/// 4x4 Drum pad grid tab.
fn show_pads_tab(
    ui: &mut egui::Ui,
    state: &mut SamplerUiState,
    zones: &[Zone],
    active_audition: &mut Option<(u8, u8)>,
) {
    ui.label(
        egui::RichText::new("4×4 Drum Trigger Grid (MIDI Notes 36–51)")
            .color(egui::Color32::from_rgb(0x8E, 0x92, 0x9C))
            .small(),
    );
    ui.add_space(6.0);

    let grid_size = 4;
    egui::Grid::new("sampler_pads_4x4_grid")
        .spacing([8.0, 8.0])
        .show(ui, |ui| {
            for row in 0..grid_size {
                for col in 0..grid_size {
                    // Standard drum pad mapping: row 3 is C1..D#1, row 0 is C2..D#2
                    let pad_idx = (3 - row) * 4 + col;
                    let midi_key = 36 + pad_idx as u8;
                    let is_selected = state.selected_pad == pad_idx;
                    let has_zone = zones.iter().any(|z| z.plays(midi_key, 100));

                    let (bg, stroke, text_col) = if is_selected {
                        (
                            egui::Color32::from_rgb(0x42, 0x58, 0x7E),
                            egui::Stroke::new(1.5, egui::Color32::from_rgb(0x60, 0x85, 0xC4)),
                            egui::Color32::WHITE,
                        )
                    } else if has_zone {
                        (
                            egui::Color32::from_rgb(0x28, 0x2C, 0x36),
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x3E, 0x43, 0x52)),
                            egui::Color32::from_rgb(0xE6, 0xE8, 0xEE),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(0x1C, 0x1E, 0x22),
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x28, 0x2B, 0x32)),
                            egui::Color32::from_rgb(0x66, 0x6A, 0x75),
                        )
                    };

                    let pad_name = DRUM_PAD_NAMES.get(pad_idx).unwrap_or(&"Pad");
                    let btn = egui::Button::new(
                        egui::RichText::new(format!("Pad {}\n{}", pad_idx + 1, pad_name))
                            .color(text_col)
                            .strong(),
                    )
                    .fill(bg)
                    .stroke(stroke)
                    .corner_radius(egui::CornerRadius::same(8))
                    .min_size(egui::vec2(95.0, 60.0));

                    let resp = ui.add(btn);
                    if resp.clicked() {
                        state.selected_pad = pad_idx;
                        *active_audition = Some((midi_key, 100));
                    }

                    if col == grid_size - 1 {
                        ui.end_row();
                    }
                }
            }
        });
}

/// Transient-detection loop slicer tab.
fn show_slices_tab(
    ui: &mut egui::Ui,
    state: &mut SamplerUiState,
    zones: &mut Vec<Zone>,
    active_audition: &mut Option<(u8, u8)>,
) {
    ui.horizontal(|ui| {
        ui.label("Transient Sensitivity:");
        if ui
            .add(egui::Slider::new(&mut state.sensitivity, 0.05..=1.0).text("Threshold"))
            .changed()
        {
            if let Some(first) = zones.first() {
                let mono_frames: Vec<f32> = first
                    .sample
                    .frames
                    .iter()
                    .map(|[l, r]| (l + r) * 0.5)
                    .collect();
                state.detected_slices =
                    detect_transients(&mono_frames, first.sample.sample_rate, state.sensitivity);
            }
        }

        let slice_btn = egui::Button::new("Slice Loop to Pads")
            .fill(egui::Color32::from_rgb(0x2E, 0x44, 0x66))
            .corner_radius(egui::CornerRadius::same(6));
        if ui.add(slice_btn).clicked() {
            if let Some(first) = zones.first() {
                let sample_arc = first.sample.clone();
                let mono_frames: Vec<f32> = sample_arc
                    .frames
                    .iter()
                    .map(|[l, r]| (l + r) * 0.5)
                    .collect();
                let slices =
                    detect_transients(&mono_frames, sample_arc.sample_rate, state.sensitivity);
                *zones = slice_into_zones(sample_arc, &slices, 36);
                state.detected_slices = slices;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(format!(
        "Detected Slices: {}",
        state.detected_slices.len().max(zones.len())
    ));

    // Horizontal slice trigger strip
    ui.horizontal_wrapped(|ui| {
        let count = state.detected_slices.len().max(zones.len()).min(16);
        for i in 0..count {
            let key = 36 + i as u8;
            let btn = egui::Button::new(format!("Slice {}", i + 1))
                .fill(egui::Color32::from_rgb(0x25, 0x28, 0x30))
                .corner_radius(egui::CornerRadius::same(4));
            if ui.add(btn).clicked() {
                *active_audition = Some((key, 100));
            }
        }
    });
}

/// Pitched chromatic instrument tab.
fn show_instrument_tab(ui: &mut egui::Ui, zones: &mut [Zone]) {
    if zones.is_empty() {
        ui.label("No sample zones loaded in this instrument.");
        return;
    }

    ui.label(
        egui::RichText::new("Chromatic Multi-Zone Instrument Map")
            .color(egui::Color32::from_rgb(0x8E, 0x92, 0x9C))
            .small(),
    );
    ui.add_space(6.0);

    for (i, zone) in zones.iter_mut().enumerate().take(8) {
        ui.horizontal(|ui| {
            ui.label(format!("Zone {}: Root Key {}", i + 1, zone.root_key));
            ui.add(egui::Slider::new(&mut zone.tune_cents, -1200.0..=1200.0).text("Tune"));
            ui.add(egui::Slider::new(&mut zone.attack, 0.001..=2.0).text("Attack"));
            ui.add(egui::Slider::new(&mut zone.release, 0.01..=5.0).text("Release"));

            let loop_text = match zone.loop_mode {
                LoopMode::NoLoop => "No Loop",
                LoopMode::OneShot => "One-Shot",
                LoopMode::Continuous => "Continuous",
                LoopMode::Sustain => "Sustain Loop",
            };
            egui::ComboBox::from_id_salt(format!("zone_loop_{i}"))
                .selected_text(loop_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut zone.loop_mode, LoopMode::NoLoop, "No Loop");
                    ui.selectable_value(&mut zone.loop_mode, LoopMode::OneShot, "One-Shot");
                    ui.selectable_value(&mut zone.loop_mode, LoopMode::Continuous, "Continuous");
                    ui.selectable_value(&mut zone.loop_mode, LoopMode::Sustain, "Sustain Loop");
                });
        });
    }
}
