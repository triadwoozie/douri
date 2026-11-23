// SPDX-License-Identifier: GPL-3.0-or-later
// Portions adapted from transit-audio/transit (MIT OR Apache-2.0, Copyright (c) 2024 transit contributors).

//! Linux PipeWire-native JACK detection and quantum diagnostics.
//!
//! Eliminates the requirement to wrap Dōri with `pw-jack` on modern Linux systems
//! by locating PipeWire's own `libjack.so.0` and reading quantum / buffer metrics.

use std::path::{Path, PathBuf};

pub const PIPEWIRE_JACK_SUBDIR: &str = "pipewire-0.3/jack";
pub const LIBJACK_FILE: &str = "libjack.so.0";

/// Standard library directories to search.
pub const CANDIDATE_DIRS: [&str; 6] = [
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib64",
    "/usr/lib",
    "/lib/x86_64-linux-gnu",
    "/lib64",
    "/lib",
];

/// PipeWire quantum and rate metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeWireQuantum {
    pub quantum: u32,
    pub rate: u32,
    pub period_ms: String,
    pub native_jack_path: Option<PathBuf>,
}

impl Default for PipeWireQuantum {
    fn default() -> Self {
        Self {
            quantum: 1024,
            rate: 48000,
            period_ms: "21.3 ms".into(),
            native_jack_path: None,
        }
    }
}

/// Discovers the location of PipeWire's native `libjack.so.0`.
pub fn find_pipewire_libjack() -> Option<PathBuf> {
    // 1. Check directories under candidate library directories
    for dir in &CANDIDATE_DIRS {
        let p = Path::new(dir).join(PIPEWIRE_JACK_SUBDIR).join(LIBJACK_FILE);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. Check if pw-jack script exists on PATH and inspect its contents
    if let Ok(path_var) = std::env::var("PATH") {
        for entry in std::env::split_paths(&path_var) {
            let pw_jack = entry.join("pw-jack");
            if pw_jack.is_file() {
                if let Ok(content) = std::fs::read_to_string(&pw_jack) {
                    for token in content.split_whitespace() {
                        if token.contains(PIPEWIRE_JACK_SUBDIR) {
                            let clean = token.trim_matches(['"', '\'', ':', '=']);
                            let candidate = Path::new(clean).join(LIBJACK_FILE);
                            if candidate.exists() {
                                return Some(candidate);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Probes live PipeWire quantum settings from the system.
pub fn probe_pipewire_quantum() -> PipeWireQuantum {
    let native_jack_path = find_pipewire_libjack();
    let mut quantum = 1024u32;
    let mut rate = 48000u32;

    #[cfg(target_os = "linux")]
    {
        // Try reading pw-cli or pw-dump
        if let Ok(output) = std::process::Command::new("pw-cli")
            .args(["info", "0"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let line = line.trim();
                    if let Some(val) = line.strip_prefix("default.clock.quantum = ") {
                        if let Ok(q) = val.trim_matches('"').parse::<u32>() {
                            quantum = q;
                        }
                    }
                    if let Some(val) = line.strip_prefix("default.clock.rate = ") {
                        if let Ok(r) = val.trim_matches('"').parse::<u32>() {
                            rate = r;
                        }
                    }
                }
            }
        }
    }

    let ms = (quantum as f64 * 1000.0 / rate.max(1) as f64);
    PipeWireQuantum {
        quantum,
        rate,
        period_ms: format!("{ms:.1} ms"),
        native_jack_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipewire_quantum_probe_does_not_panic() {
        let q = probe_pipewire_quantum();
        assert!(q.quantum > 0);
        assert!(q.rate > 0);
        assert!(!q.period_ms.is_empty());
    }
}
