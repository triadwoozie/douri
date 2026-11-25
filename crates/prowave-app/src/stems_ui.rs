//! Local AI Stem Separation UI, Inline Micro-Faders (V D B O), Drum Kit & Groove Extraction,
//! De-Bleed / Spill Reducer, and `.douri-chain` Track Template / FX Chain Presets.

use crate::model::{Note, Project, Region, Track, TrackKind};
use egui::{pos2, vec2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui};
use prowave_engine::stems::{
    extract_drum_kit_and_groove, separate_stems, stem_cache_dir, write_wav_stereo, StemKind,
    StemRecipe, StemSet,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Per-region non-destructive stem-split state.
#[derive(Clone, Debug)]
pub struct StemRegionState {
    pub active: bool,
    /// Gains for `[Vocals, Drums, Bass, Other]` in `0.0..=2.0` (1.0 = 0 dB).
    pub gains: [f32; 4],
    pub mutes: [bool; 4],
    /// De-Bleed / Spill Reducer (0.0..=100.0%).
    pub debleed_pct: f32,
    pub recipe: Option<StemRecipe>,
    pub stems: Option<StemSet>,
}

impl Default for StemRegionState {
    fn default() -> Self {
        Self {
            active: false,
            gains: [1.0; 4],
            mutes: [false; 4],
            debleed_pct: 0.0,
            recipe: None,
            stems: None,
        }
    }
}

/// Renders 4 tiny vertical micro-faders (`V D B O`) directly inside a region's header bar.
pub fn draw_inline_stem_faders(ui: &mut Ui, header_rect: Rect, state: &mut StemRegionState) {
    if !state.active || header_rect.width() < 90.0 {
        return;
    }

    let fader_w = 12.0;
    let fader_h = (header_rect.height() - 4.0).max(10.0);
    let gap = 2.0;
    let total_w = 4.0 * fader_w + 3.0 * gap;
    let start_x = header_rect.right() - total_w - 4.0;
    let top_y = header_rect.top() + 2.0;

    let colors = [
        Color32::from_rgb(0x4a, 0x9e, 0xff), // V: Blue
        Color32::from_rgb(0xff, 0x7b, 0x4a), // D: Orange
        Color32::from_rgb(0xb0, 0x6c, 0xff), // B: Purple
        Color32::from_rgb(0x3a, 0xd1, 0x8a), // O: Green
    ];

    for (i, kind) in StemKind::ALL.iter().enumerate() {
        let x = start_x + i as f32 * (fader_w + gap);
        let rect = Rect::from_min_size(pos2(x, top_y), vec2(fader_w, fader_h));
        let resp = ui.interact(
            rect,
            ui.id().with(("stem_fader", i, rect.left() as i32)),
            Sense::click_and_drag(),
        );

        if resp.clicked() {
            state.mutes[i] = !state.mutes[i];
        }
        if resp.dragged() {
            let delta = -resp.drag_delta().y / fader_h.max(1.0);
            state.gains[i] = (state.gains[i] + delta * 1.5).clamp(0.0, 2.0);
        }

        let painter = ui.painter();
        painter.rect_filled(
            rect,
            CornerRadius::same(2),
            Color32::from_rgba_unmultiplied(15, 17, 22, 200),
        );

        let level = if state.mutes[i] {
            0.0
        } else {
            (state.gains[i] / 2.0).clamp(0.0, 1.0)
        };
        let fill_h = fader_h * level;
        let fill_rect = Rect::from_min_max(
            pos2(rect.left() + 1.0, rect.bottom() - fill_h),
            pos2(rect.right() - 1.0, rect.bottom()),
        );
        painter.rect_filled(fill_rect, CornerRadius::same(1), colors[i]);

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            kind.short_label(),
            egui::FontId::proportional(8.5),
            if state.mutes[i] {
                Color32::from_rgb(130, 130, 135)
            } else {
                Color32::WHITE
            },
        );

        resp.on_hover_text(format!(
            "{}: {:.0}% (Click to mute, drag to adjust)",
            kind.name(),
            state.gains[i] * 100.0
        ));
    }
}

/// Runs local stem separation on a region's audio and caches the deterministic recipe.
pub fn split_region_stems_in_place(region: &mut Region, debleed_pct: f32) -> Result<StemSet, String> {
    let audio = region
        .audio
        .as_ref()
        .ok_or_else(|| "Selected region has no audio".to_string())?;
    let (left, right) = read_audio_stereo(&audio.file.path)?;
    let stems = separate_stems(&left, &right, audio.file.sample_rate, debleed_pct);

    let recipe = StemRecipe {
        source_hash: format!("{:x}", audio.file.frames ^ (audio.file.sample_rate as u64)),
        start_frame: audio.start_frame,
        len_frames: audio.len_frames,
        model_id: "htdemucs-spectral-v1".to_string(),
        debleed_pct,
    };

    let cache_dir = stem_cache_dir().join(recipe.cache_key());
    for (i, kind) in StemKind::ALL.iter().enumerate() {
        let wav_path = cache_dir.join(format!("{}.wav", kind.name().to_lowercase()));
        let _ = write_wav_stereo(&wav_path, &stems.stems[i].0, &stems.stems[i].1, stems.sample_rate);
    }

    Ok(stems)
}

/// One-click "Extract Kit & Groove": isolates drums, slices transients into `.sfz` + `.wav`
/// in the Music Library, and creates a new MIDI Piano Roll track with the extracted pattern.
pub fn extract_kit_and_groove_to_project(
    project: &mut Project,
    region: &Region,
) -> Result<String, String> {
    let audio = region
        .audio
        .as_ref()
        .ok_or_else(|| "Selected region has no audio".to_string())?;
    let (left, right) = read_audio_stereo(&audio.file.path)?;
    let stems = separate_stems(&left, &right, audio.file.sample_rate, 0.0);
    let (drums_l, drums_r) = &stems.stems[StemKind::Drums as usize];

    let kit = extract_drum_kit_and_groove(drums_l, drums_r, audio.file.sample_rate, project.tempo);
    if kit.hits.is_empty() {
        return Err("No drum transients detected in region".to_string());
    }

    let kit_dir = stem_cache_dir().join("kits").join(format!("{}_kit", region.name));
    for hit in &kit.hits {
        let _ = write_wav_stereo(&kit_dir.join(&hit.name), &hit.left, &hit.right, audio.file.sample_rate);
    }
    let sfz_path = kit_dir.join("kit.sfz");
    std::fs::write(&sfz_path, &kit.sfz_content).map_err(|e| e.to_string())?;

    let mut midi_track = Track::new(format!("{} (Kit & Groove)", region.name), TrackKind::Instrument);
    let mut midi_region = Region::new_midi(
        format!("{} Groove", region.name),
        region.start_bar,
        region.len_bars,
    );
    for ev in &kit.events {
        midi_region.notes.push(Note {
            pitch: ev.midi_note,
            start: ev.beat,
            len: ev.dur_beats,
            vel: ev.velocity,
            mute: false,
            lyric: Default::default(),
        });
    }
    midi_track.regions.push(midi_region);
    project.tracks.push(midi_track);

    Ok(format!(
        "Extracted {} drum hits to {} and created MIDI groove track",
        kit.hits.len(),
        sfz_path.display()
    ))
}

fn read_audio_stereo(path: &PathBuf) -> Result<(Vec<f32>, Vec<f32>), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 44 || &data[0..4] != b"RIFF" {
        return Err("Only uncompressed WAV files are supported for direct stem slicing".to_string());
    }
    let channels = u16::from_le_bytes([data[22], data[23]]) as usize;
    let bits = u16::from_le_bytes([data[34], data[35]]);
    let audio_bytes = &data[44..];
    let mut left = Vec::new();
    let mut right = Vec::new();

    if bits == 32 {
        let mut i = 0;
        while i + 4 * channels <= audio_bytes.len() {
            let l = f32::from_le_bytes([
                audio_bytes[i],
                audio_bytes[i + 1],
                audio_bytes[i + 2],
                audio_bytes[i + 3],
            ]);
            let r = if channels >= 2 {
                f32::from_le_bytes([
                    audio_bytes[i + 4],
                    audio_bytes[i + 5],
                    audio_bytes[i + 6],
                    audio_bytes[i + 7],
                ])
            } else {
                l
            };
            left.push(l);
            right.push(r);
            i += 4 * channels;
        }
    } else if bits == 16 {
        let mut i = 0;
        while i + 2 * channels <= audio_bytes.len() {
            let l = i16::from_le_bytes([audio_bytes[i], audio_bytes[i + 1]]) as f32 / 32768.0;
            let r = if channels >= 2 {
                i16::from_le_bytes([audio_bytes[i + 2], audio_bytes[i + 3]]) as f32 / 32768.0
            } else {
                l
            };
            left.push(l);
            right.push(r);
            i += 2 * channels;
        }
    } else {
        return Err(format!("Unsupported WAV bit depth: {bits}"));
    }

    Ok((left, right))
}

/// Track Template & FX Chain Preset saved as a `.douri-chain` JSON asset in the Music Library.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DouriChainAsset {
    pub name: String,
    pub track_kind: String,
    pub color_rgb: [u8; 3],
    pub volume_db: f32,
    pub pan: f32,
    pub inserts: Vec<String>,
    pub modulation_routes: Vec<String>,
}

impl DouriChainAsset {
    pub fn from_track(track: &Track) -> Self {
        Self {
            name: track.name.clone(),
            track_kind: format!("{:?}", track.kind),
            color_rgb: [track.color.r(), track.color.g(), track.color.b()],
            volume_db: track.volume_db,
            pan: track.pan,
            inserts: track
                .inserts
                .iter()
                .map(|i| format!("{:?}", i.kind))
                .collect(),
            modulation_routes: Vec::new(),
        }
    }

    pub fn save_to_library(&self) -> Result<PathBuf, String> {
        let dir = stem_cache_dir()
            .parent()
            .unwrap_or(&stem_cache_dir())
            .join("chains");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.douri-chain", self.name.replace(' ', "_")));
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(path)
    }
}
