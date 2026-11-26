// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Crash-safe system diagnostics and privacy-scrubbed issue reporting.
//!
//! Collects facts about the computer (OS distro, kernel, CPU, available cores, audio stack,
//! and detected audio devices) while redacting user names and home directory paths.

use std::path::Path;

/// Redaction of personal privacy information (usernames and home paths).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Redaction {
    pub home: Option<String>,
    pub user: Option<String>,
}

impl Redaction {
    pub fn from_env() -> Self {
        let var = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| std::env::var(n).ok())
                .filter(|v| !v.trim().is_empty())
        };
        Self {
            home: var(&["USERPROFILE", "HOME"]),
            user: var(&["USERNAME", "USER", "LOGNAME"]),
        }
    }

    /// Replaces the home directory with `~` and user name with `<user>`.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_owned();
        if let Some(home) = self
            .home
            .as_deref()
            .map(|h| h.trim_end_matches(['/', '\\']))
            .filter(|h| h.len() >= 2)
        {
            out = replace_ci(&out, home, "~");
            out = replace_ci(&out, &home.replace('\\', "/"), "~");
            out = replace_ci(&out, &home.replace('/', "\\"), "~");
        }
        if let Some(user) = self.user.as_deref().map(str::trim).filter(|u| u.len() >= 3) {
            out = replace_ci(&out, user, "<user>");
        }
        out
    }
}

/// Case-insensitive ASCII replacement.
fn replace_ci(text: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let (lower, needle_lower) = (text.to_ascii_lowercase(), needle.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(i) = lower[at..].find(&needle_lower) {
        out.push_str(&text[at..at + i]);
        out.push_str(with);
        at += i + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

/// Facts about this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    pub os: String,
    pub kernel: String,
    pub cpu: String,
    pub cores: usize,
    pub stack: Vec<(String, String)>,
}

impl Machine {
    pub fn probe() -> Self {
        let cores = std::thread::available_parallelism().map_or(0, usize::from);
        let mut m = Machine {
            cores,
            ..Machine::default()
        };

        #[cfg(target_os = "linux")]
        {
            m.os = linux_os_name();
            m.kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .unwrap_or_default()
                .trim()
                .to_string();
            m.cpu = linux_cpu();
            m.stack = linux_audio_stack();
        }

        #[cfg(target_os = "windows")]
        {
            m.os = "Windows".to_string();
            m.kernel = std::env::var("OS").unwrap_or_default();
            m.cpu = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default();
            m.stack = vec![("Backend".into(), "WASAPI / ASIO".into())];
        }

        #[cfg(target_os = "macos")]
        {
            m.os = "macOS".to_string();
            m.kernel = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
                .unwrap_or_else(|_| "Darwin".into());
            m.cpu = "Apple Silicon / Intel".to_string();
            m.stack = vec![("Backend".into(), "CoreAudio".into())];
        }

        m
    }
}

#[cfg(target_os = "linux")]
fn linux_os_name() -> String {
    if let Ok(text) = std::fs::read_to_string("/etc/os-release") {
        for line in text.lines() {
            if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                return val.trim_matches('"').to_string();
            }
        }
    }
    "Linux".to_string()
}

#[cfg(target_os = "linux")]
fn linux_cpu() -> String {
    if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in text.lines() {
            if let Some(name) = line.strip_prefix("model name") {
                if let Some((_, model)) = name.split_once(':') {
                    return model.trim().to_string();
                }
            }
        }
    }
    "Generic CPU".to_string()
}

#[cfg(target_os = "linux")]
fn linux_audio_stack() -> Vec<(String, String)> {
    let mut stack = Vec::new();

    // Check ALSA
    if let Ok(alsa_ver) = std::fs::read_to_string("/proc/asound/version") {
        stack.push(("ALSA Driver".into(), alsa_ver.trim().to_string()));
    }

    // Check PipeWire
    if let Ok(output) = std::process::Command::new("pipewire")
        .arg("--version")
        .output()
    {
        if output.status.success() {
            let ver = String::from_utf8_lossy(&output.stdout);
            let first_line = ver.lines().next().unwrap_or("").trim().to_string();
            stack.push(("PipeWire".into(), first_line));
        }
    }

    // Check JACK
    let jack_present = Path::new("/usr/lib/pipewire-0.3/jack/libjack.so.0").exists()
        || Path::new("/usr/lib/x86_64-linux-gnu/pipewire-0.3/jack/libjack.so.0").exists()
        || Path::new("/usr/lib64/pipewire-0.3/jack/libjack.so.0").exists();
    stack.push((
        "PipeWire JACK Native".into(),
        if jack_present {
            "Available".into()
        } else {
            "Not found in standard paths".into()
        },
    ));

    stack
}

/// Renders a full privacy-scrubbed diagnostic report.
pub fn generate_report(output_devices: &[prowave_engine::OutputDevice]) -> String {
    let redaction = Redaction::from_env();
    let machine = Machine::probe();

    let mut report = String::new();
    report.push_str("=== Dōri Diagnostic Report ===\n");
    report.push_str(&format!(
        "Application Version : {}\n",
        env!("CARGO_PKG_VERSION")
    ));
    report.push_str(&format!("Operating System    : {}\n", machine.os));
    report.push_str(&format!("Kernel Version      : {}\n", machine.kernel));
    report.push_str(&format!("Processor           : {}\n", machine.cpu));
    report.push_str(&format!("Logical CPU Cores   : {}\n", machine.cores));
    report.push_str("\n--- Audio Stack ---\n");
    for (k, v) in &machine.stack {
        report.push_str(&format!("{:<20}: {}\n", k, v));
    }

    report.push_str("\n--- Detected Audio Output Devices ---\n");
    if output_devices.is_empty() {
        report.push_str("  (None detected or using default output)\n");
    } else {
        for (i, dev) in output_devices.iter().enumerate() {
            report.push_str(&format!("  [{}] {} (id: {})\n", i + 1, dev.name, dev.id));
        }
    }

    report.push_str("\n==============================\n");
    redaction.apply(&report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_replaces_home_and_username() {
        let red = Redaction {
            home: Some("/home/triadwoozie".into()),
            user: Some("triadwoozie".into()),
        };

        let raw = "Error loading /home/triadwoozie/Documents/Douri for user triadwoozie.";
        let clean = red.apply(raw);
        assert_eq!(clean, "Error loading ~/Documents/Douri for user <user>.");
    }
}

/// UI state for Help -> Diagnostics (Engine Stress Benchmark & PDC Inspector).
#[derive(Clone, Default)]
pub struct DiagnosticsWindowState {
    pub open: bool,
    pub last_report: Option<prowave_engine::stress_bench::StressBenchReport>,
}

/// Renders the Diagnostics, 32-Track Stress Benchmark, and Sample-Accurate PDC Inspector window.
pub fn show_diagnostics_window(
    ctx: &egui::Context,
    state: &mut DiagnosticsWindowState,
    project: &crate::model::Project,
    selected_track: usize,
) {
    if !state.open {
        return;
    }
    let mut open = state.open;
    egui::Window::new("Diagnostics — Stress Bench & PDC Inspector")
        .open(&mut open)
        .default_size([560.0, 420.0])
        .show(ctx, |ui| {
            ui.heading("Engine Stress & Torture Benchmark (32 Tracks)");
            ui.label("Spawns 32 tracks with 4-slot DSP chains + automation curves and measures callback jitter (µs), XRuns, and PDC drift.");
            ui.horizontal(|ui| {
                if ui.button("▶ Run 32-Track Stress Benchmark").clicked() {
                    state.last_report = Some(prowave_engine::stress_bench::run_stress_benchmark(32, 128));
                }
                if let Some(ref rep) = state.last_report {
                    if ui.button("Copy Verifiable Badge").clicked() {
                        ctx.copy_text(rep.format_badge());
                    }
                }
            });

            if let Some(ref rep) = state.last_report {
                let badge_color = if rep.passed {
                    egui::Color32::from_rgb(0x2e, 0xa0, 0x43)
                } else {
                    egui::Color32::from_rgb(0xd2, 0x99, 0x22)
                };
                ui.colored_label(badge_color, rep.format_badge());
                egui::Grid::new("stress_grid").striped(true).show(ui, |ui| {
                    ui.label("Audio Budget:");
                    ui.label(format!("{:.1} µs ({} smp @ {} Hz)", rep.budget_us, rep.block_size, rep.sample_rate));
                    ui.end_row();
                    ui.label("Avg Callback:");
                    ui.label(format!("{:.1} µs", rep.avg_callback_us));
                    ui.end_row();
                    ui.label("Worst Callback / Jitter:");
                    ui.label(format!("{:.1} µs / {:.1} µs", rep.worst_callback_us, rep.jitter_us));
                    ui.end_row();
                    ui.label("XRun Count / PDC Drift:");
                    ui.label(format!("{} XRuns / {} samples", rep.xruns, rep.pdc_drift_samples));
                    ui.end_row();
                });
            }

            ui.separator();
            ui.heading("Sample-Accurate PDC Inspector");
            if let Some(track) = project.tracks.get(selected_track) {
                ui.label(format!("Channel: {} (48000 Hz)", track.name));
                let mut total_samples = 0usize;
                egui::Grid::new("pdc_grid").striped(true).show(ui, |ui| {
                    ui.strong("Slot");
                    ui.strong("Plugin / DSP");
                    ui.strong("Latency (samples)");
                    ui.strong("Latency (ms)");
                    ui.end_row();

                    for (idx, ins) in track.inserts.iter().enumerate() {
                        let smp: usize = match ins.kind {
                            prowave_engine::FxKind::Limiter => 64,
                            _ => 0,
                        };
                        total_samples += smp;
                        ui.label(format!("#{}", idx + 1));
                        ui.label(format!("{:?}", ins.kind));
                        ui.label(format!("{smp} smp"));
                        ui.label(format!("{:.2} ms", smp as f32 / 48.0));
                        ui.end_row();
                    }
                });
                ui.label(
                    egui::RichText::new(format!(
                        "Total Compensated Path Delay: {} samples ({:.2} ms)",
                        total_samples,
                        total_samples as f32 / 48.0
                    ))
                    .strong(),
                );
            } else {
                ui.label("No track selected.");
            }
        });
    state.open = open;
}
