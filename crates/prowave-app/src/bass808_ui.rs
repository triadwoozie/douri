//! UI window for the tuned 808 Bass synthesizer and sample player.

use crate::UiState;
use egui::{Slider, Ui};
use prowave_engine::{
    Bass808Spec, BASS808_DECAY, BASS808_DRIVE, BASS808_GLIDE, BASS808_HIGH_PASS, BASS808_LEVEL,
    BASS808_PARAMS, BASS808_PUNCH, BASS808_PUNCH_TIME, BASS808_RELEASE, BASS808_SINE,
    BASS808_SQUARE, BASS808_TUNE,
};
use std::path::Path;

const ANALYSIS_RATE: u32 = 44_100;

/// The 808 Bass instrument editor window.
pub fn show(ctx: &egui::Context, p: &mut crate::model::Project, s: &mut UiState) {
    let Some(ti) = s.show_bass808 else {
        return;
    };
    let Some(mut spec) = p.tracks.get(ti).and_then(|t| t.bass808.clone()) else {
        s.show_bass808 = None;
        return;
    };
    let initial = spec.clone();
    let mut open = true;
    let track_name = p.tracks.get(ti).map_or("", |t| &t.name);
    let title = format!("808 Bass · {}", track_name);
    egui::Window::new(title)
        .id(egui::Id::unique("bass808_window"))
        .open(&mut open)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            render_controls(ui, &mut spec);
        });
    if spec != initial {
        if let Some(t) = p.tracks.get_mut(ti) {
            t.bass808 = Some(spec);
        }
    }
    if !open {
        s.show_bass808 = None;
    }
}

fn render_controls(ui: &mut Ui, spec: &mut Bass808Spec) {
    ui.vertical(|ui| {
        // --- Source Section ---
        ui.group(|ui| {
            ui.heading("Sound Source");
            if spec.sample.is_empty() {
                ui.label("Synthesized Voice (TR-808 Sine Pitch-Drop)");
            } else {
                let name = Path::new(&spec.sample)
                    .file_name()
                    .map(|f| f.to_string_lossy())
                    .unwrap_or_default();
                ui.label(format!("Sample: {}", name));
                if spec.root_hz > 0.0 {
                    ui.label(format!("Root: {:.1} Hz", spec.root_hz));
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Synthesized 808").clicked() {
                    spec.sample.clear();
                    spec.root_hz = 0.0;
                }
                if ui.button("Choose Sample…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Audio file", &["wav", "WAV", "aif", "aiff", "flac"])
                        .pick_file()
                    {
                        if let Ok(mono) = prowave_engine::bass808_prepare_sample(&path, ANALYSIS_RATE) {
                            let root = prowave_engine::bass808_measure_root_hz(&mono, ANALYSIS_RATE).unwrap_or(55.0);
                            spec.sample = path.to_string_lossy().into_owned();
                            spec.root_hz = root;
                        }
                    }
                }
            });
        });

        ui.add_space(4.0);

        // --- Pitch & Glide ---
        ui.group(|ui| {
            ui.heading("Pitch & Articulation");
            let p_tune = &BASS808_PARAMS[BASS808_TUNE];
            ui.add(
                Slider::new(&mut spec.params.tune_st, p_tune.lo..=p_tune.hi)
                    .text("Tune (st)")
                    .step_by(1.0),
            );

            let p_glide = &BASS808_PARAMS[BASS808_GLIDE];
            ui.add(
                Slider::new(&mut spec.params.glide_ms, p_glide.lo..=p_glide.hi)
                    .text("Glide (ms)")
                    .logarithmic(true),
            );

            ui.checkbox(&mut spec.params.legato, "Legato (Overlap slides without retrigger)");
        });

        ui.add_space(4.0);

        // --- Punch (Pitch Drop) ---
        ui.group(|ui| {
            ui.heading("Punch (Attack Transient)");
            let p_punch = &BASS808_PARAMS[BASS808_PUNCH];
            ui.add(
                Slider::new(&mut spec.params.punch_st, p_punch.lo..=p_punch.hi)
                    .text("Punch Amount (st)")
                    .step_by(1.0),
            );

            let p_time = &BASS808_PARAMS[BASS808_PUNCH_TIME];
            ui.add(
                Slider::new(&mut spec.params.punch_ms, p_time.lo..=p_time.hi)
                    .text("Punch Time (ms)")
                    .logarithmic(true),
            );
        });

        ui.add_space(4.0);

        // --- Envelope ---
        ui.group(|ui| {
            ui.heading("Amp Envelope");
            let p_decay = &BASS808_PARAMS[BASS808_DECAY];
            ui.add(
                Slider::new(&mut spec.params.decay_s, p_decay.lo..=p_decay.hi)
                    .text("Decay (s)")
                    .logarithmic(true),
            );

            let p_rel = &BASS808_PARAMS[BASS808_RELEASE];
            ui.add(
                Slider::new(&mut spec.params.release_s, p_rel.lo..=p_rel.hi)
                    .text("Release (s)")
                    .logarithmic(true),
            );

            let can_sustain = if spec.sample.is_empty() {
                true
            } else {
                prowave_engine::bass808_can_sustain(spec, ANALYSIS_RATE)
            };
            ui.add_enabled(can_sustain, egui::Checkbox::new(&mut spec.params.sustain, "Sustain Loop"));
        });

        ui.add_space(4.0);

        // --- Tone, Subs & Output ---
        ui.group(|ui| {
            ui.heading("Tone & Output");
            let p_drive = &BASS808_PARAMS[BASS808_DRIVE];
            ui.add(
                Slider::new(&mut spec.params.drive, p_drive.lo..=p_drive.hi)
                    .text("Drive (Saturation)"),
            );

            let p_hp = &BASS808_PARAMS[BASS808_HIGH_PASS];
            ui.add(
                Slider::new(&mut spec.params.high_pass_hz, p_hp.lo..=p_hp.hi)
                    .text("High Pass (Hz)")
                    .logarithmic(true),
            );

            let p_sine = &BASS808_PARAMS[BASS808_SINE];
            ui.add(
                Slider::new(&mut spec.params.sine, p_sine.lo..=p_sine.hi)
                    .text("Sub Sine Mix"),
            );

            let p_sq = &BASS808_PARAMS[BASS808_SQUARE];
            ui.add(
                Slider::new(&mut spec.params.square, p_sq.lo..=p_sq.hi)
                    .text("Sub Square Mix"),
            );

            let p_lvl = &BASS808_PARAMS[BASS808_LEVEL];
            ui.add(
                Slider::new(&mut spec.params.level_db, p_lvl.lo..=p_lvl.hi)
                    .text("Output Level (dB)"),
            );
        });
    });
}
