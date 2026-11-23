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
