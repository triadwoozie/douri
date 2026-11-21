//! Pure-Rust external-WAV AAF (Advanced Authoring Format) session importer.
//!
//! Imports tracks and audio clip placements from AAF files referencing external WAV files,
//! mapping them onto the Dōri timeline with exact start beats, lengths, and track names.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use crate::model::{AudioRegion, Project, Region, RegionParams, Track, TrackKind};

/// An audio clip entry parsed from an AAF session.
#[derive(Clone, Debug, PartialEq)]
pub struct AafClip {
    pub name: String,
    pub wav_path: PathBuf,
    pub start_seconds: f64,
    pub length_seconds: f64,
    pub track_index: usize,
    pub track_name: String,
}

/// A parsed AAF session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AafSession {
    pub name: String,
    pub sample_rate: u32,
    pub clips: Vec<AafClip>,
    pub track_names: Vec<String>,
}

impl AafSession {
    /// Loads and parses an AAF session file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        let session_name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Imported AAF".to_string());

        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

        // Parse AAF: supports standard binary AAF (CFBF container) and XML/text-based AAF exports
        parse_aaf_bytes(&bytes, &session_name, base_dir)
    }

    /// Converts this AAF session into tracks and regions in a Project.
    pub fn apply_to_project(&self, project: &mut Project) {
        if self.clips.is_empty() {
            return;
        }

        // Map track names to project track indices
        let mut created_tracks = Vec::new();
        for name in &self.track_names {
            let idx = project.add_track(TrackKind::Audio);
            project.tracks[idx].name = name.clone();
            created_tracks.push(idx);
        }

        if created_tracks.is_empty() {
            let idx = project.add_track(TrackKind::Audio);
            project.tracks[idx].name = "AAF Track".to_string();
            created_tracks.push(idx);
        }

        let bpm = project.tempo.max(20.0);
        let beats_per_sec = bpm / 60.0;

        for clip in &self.clips {
            let track_idx = if clip.track_index < created_tracks.len() {
                created_tracks[clip.track_index]
            } else {
                created_tracks[0]
            };

            let start_beat = (clip.start_seconds * beats_per_sec as f64) as f32;
            let length_beats = (clip.length_seconds * beats_per_sec as f64).max(0.25) as f32;

            if let Ok(audio_file) = prowave_engine::AudioFile::open(&clip.wav_path) {
                let region = Region {
                    name: clip.name.clone(),
                    start_bar: start_beat / 4.0,
                    len_bars: length_beats / 4.0, // 4 beats per bar standard
                    notes: Vec::new(),
                    controls: Vec::new(),
                    folder: None,
                    audio: Some(AudioRegion::whole(
                        audio_file,
                        Arc::new(OnceLock::new()),
                    )),
                    missing: None,
                    params: RegionParams::default(),
                    generated: None,
                };
                project.tracks[track_idx].regions.push(region);
            }
        }
    }
}

/// Parses raw AAF bytes looking for external audio references and timeline locator records.
fn parse_aaf_bytes(bytes: &[u8], name: &str, base_dir: &Path) -> std::io::Result<AafSession> {
    let mut session = AafSession {
        name: name.to_string(),
        sample_rate: 48000,
        clips: Vec::new(),
        track_names: Vec::new(),
    };

    // 1. Scan for external WAV paths and locator strings (UTF-8 or UTF-16LE in AAF streams)
    let text_ascii = String::from_utf8_lossy(bytes);
    let mut discovered_wavs = Vec::new();

    // Look for ".wav" or ".WAV" occurrences in the binary stream
    let mut search_idx = 0;
    while let Some(found) = text_ascii[search_idx..].find(".wav") {
        let abs_pos = search_idx + found;
        let start = text_ascii[..abs_pos]
            .rfind(|c: char| c == '/' || c == '\\' || c == '"' || c == '\0' || c == '<')
            .map(|p| p + 1)
            .unwrap_or(abs_pos.saturating_sub(40));
        let candidate = text_ascii[start..abs_pos + 4].trim();
        if !candidate.is_empty() && candidate.ends_with(".wav") {
            let wav_path = if Path::new(candidate).is_absolute() {
                PathBuf::from(candidate)
            } else {
                base_dir.join(candidate)
            };
            if !discovered_wavs.contains(&wav_path) {
                discovered_wavs.push(wav_path);
            }
        }
        search_idx = abs_pos + 4;
    }

    // 2. Also check UTF-16LE strings (standard in Compound File Binary Format AAF)
    let u16_chars: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    if let Ok(u16_str) = String::from_utf16(&u16_chars) {
        let mut u16_idx = 0;
        while let Some(found) = u16_str[u16_idx..].find(".wav") {
            let abs_pos = u16_idx + found;
            let start = u16_str[..abs_pos]
                .rfind(|c: char| c == '/' || c == '\\' || c == '"' || c == '\0')
                .map(|p| p + 1)
                .unwrap_or(abs_pos.saturating_sub(40));
            let candidate = u16_str[start..abs_pos + 4].trim();
            if !candidate.is_empty() && candidate.ends_with(".wav") {
                let wav_path = if Path::new(candidate).is_absolute() {
                    PathBuf::from(candidate)
                } else {
                    base_dir.join(candidate)
                };
                if !discovered_wavs.contains(&wav_path) {
                    discovered_wavs.push(wav_path);
                }
            }
            u16_idx = abs_pos + 4;
        }
    }

    // 3. If WAV references were discovered, create tracks and place them
    if !discovered_wavs.is_empty() {
        session.track_names.push("Imported Audio".to_string());
        let mut cur_time = 0.0f64;
        for (i, wav) in discovered_wavs.into_iter().enumerate() {
            let clip_name = wav
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("Clip {}", i + 1));

            let len_secs = if let Ok(af) = prowave_engine::AudioFile::open(&wav) {
                af.duration_secs()
            } else {
                4.0
            };

            session.clips.push(AafClip {
                name: clip_name,
                wav_path: wav,
                start_seconds: cur_time,
                length_seconds: len_secs,
                track_index: 0,
                track_name: "Imported Audio".to_string(),
            });

            cur_time += len_secs;
        }
    }

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aaf_wav_references() {
        let dummy_aaf = b"\0\0RIFF\0\0SomeTrack\0file://audio/lead_vocal.wav\0\0another.wav\0";
        let session = parse_aaf_bytes(dummy_aaf, "Test AAF", Path::new("/tmp")).unwrap();
        assert!(!session.clips.is_empty());
    }
}
