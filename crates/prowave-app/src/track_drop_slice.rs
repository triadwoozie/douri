//! Drag-audio-to-track-header modal (`Create New Track using:`) and auto-slicing
//! (`Quick Sampler (Original)`, `Quick Sampler (Optimized)`, `Drum Kit Designer`,
//! `Sampler (Zone Per Note)`).
use crate::i18n::tr;
use crate::model::{Note, Project, Region, SamplerUse, Track, TrackKind};
use crate::sampler_load::SamplerSource;
use egui::{vec2, Align2, Color32, Margin, RichText, Stroke};
use prowave_engine::sample_file::SampleData;
use prowave_engine::sampler::detect_transients;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_DRUM_KIT_SLICES: usize = 32;

/// Shared cross-panel payload when dragging an Audio or MIDI region out of the Tracks Area.
#[derive(Clone, Debug, PartialEq)]
pub struct DraggedRegionPayload {
    pub track_index: usize,
    pub region_index: usize,
    pub region_name: String,
    pub is_midi: bool,
    pub original_start_beats: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossPanelDropTarget {
    TrackHeaders,
    SoundLibrary,
}

/// Hit-tests `pos` against the full Track Header column rect and Sound Library panel rect.
pub fn cross_panel_drop_target(
    pos: egui::Pos2,
    headers_rect: Option<egui::Rect>,
    library_rect: Option<egui::Rect>,
) -> Option<CrossPanelDropTarget> {
    if headers_rect.is_some_and(|r| r.contains(pos)) {
        Some(CrossPanelDropTarget::TrackHeaders)
    } else if library_rect.is_some_and(|r| r.contains(pos)) {
        Some(CrossPanelDropTarget::SoundLibrary)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub enum DropAudioSource {
    File {
        path: PathBuf,
        name: String,
    },
    Region {
        name: String,
        path: PathBuf,
        start_frame: u64,
        len_frames: Option<u64>,
    },
    Buffer {
        name: String,
        sample: Arc<SampleData>,
    },
}

impl DropAudioSource {
    pub fn name(&self) -> &str {
        match self {
            Self::File { name, .. } | Self::Region { name, .. } | Self::Buffer { name, .. } => name,
        }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            Self::File { path, .. } | Self::Region { path, .. } => Some(path),
            Self::Buffer { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackCreateMode {
    QuickSamplerOriginal,
    QuickSamplerOptimized,
    DrumKitDesigner,
    SamplerZonePerNote,
}

#[derive(Default)]
pub struct TrackDropSliceModal {
    pub pending: Option<DropAudioSource>,
    pub selected: Option<TrackCreateMode>,
    pub error: Option<String>,
}

impl TrackDropSliceModal {
    pub fn open(&mut self, source: DropAudioSource) {
        self.pending = Some(source);
        self.selected = Some(TrackCreateMode::DrumKitDesigner);
        self.error = None;
    }

    pub fn close(&mut self) {
        self.pending = None;
        self.error = None;
    }
}

/// Renders the `Create New Track using:` modal when an audio region or file was dropped
/// onto the Track Header gutter.
pub fn show(ctx: &egui::Context, p: &mut Project, modal: &mut TrackDropSliceModal) {
    let Some(source) = modal.pending.clone() else {
        return;
    };
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut picked: Option<TrackCreateMode> = None;

    egui::Window::new(tr("Create New Track using:"))
        .title_bar(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x1c, 0x1c, 0x1e))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x40)))
                .corner_radius(10)
                .inner_margin(Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(
                RichText::new(tr("Create New Track using:"))
                    .strong()
                    .size(14.0)
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new(source.name())
                    .size(11.5)
                    .color(Color32::from_rgb(0x98, 0x98, 0x9f)),
            );
            ui.add_space(10.0);

            let choices = [
                (
                    TrackCreateMode::QuickSamplerOriginal,
                    tr("Quick Sampler (Original)"),
                    "Maps full audio sample to C3",
                ),
                (
                    TrackCreateMode::QuickSamplerOptimized,
                    tr("Quick Sampler (Optimized)"),
                    "Trims silence, normalizes peak, and detects pitch",
                ),
                (
                    TrackCreateMode::DrumKitDesigner,
                    tr("Drum Kit Designer"),
                    "Auto-slices transients into a Drum Kit Track Stack + MIDI groove",
                ),
                (
                    TrackCreateMode::SamplerZonePerNote,
                    tr("Sampler (Zone Per Note)"),
                    "Maps detected transient slices chromatically from C1",
                ),
            ];

            for (mode, title, desc) in choices {
                let is_sel = modal.selected == Some(mode);
                let resp = ui.add_sized(
                    vec2(340.0, 44.0),
                    egui::Button::new(RichText::new(format!("{title}\n{desc}")).size(11.5).color(
                        if is_sel {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(0xd8, 0xd8, 0xde)
                        },
                    ))
                    .fill(if is_sel {
                        Color32::from_rgb(0x12, 0x69, 0xcc)
                    } else {
                        Color32::from_rgb(0x28, 0x28, 0x2b)
                    })
                    .corner_radius(6),
                );
                if resp.clicked() {
                    picked = Some(mode);
                }
                ui.add_space(4.0);
            }

            if let Some(err) = &modal.error {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(err)
                        .size(11.0)
                        .color(Color32::from_rgb(0xff, 0x5c, 0x5c)),
                );
            }

            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new(tr("Cancel")).color(Color32::WHITE))
                            .fill(Color32::from_rgb(0x3a, 0x3a, 0x3e))
                            .corner_radius(5),
                    )
                    .clicked()
                {
                    close = true;
                }
            });
        });

    if let Some(mode) = picked {
        match apply_create_mode(p, &source, mode) {
            Ok(_) => modal.close(),
            Err(e) => modal.error = Some(e),
        }
    } else if close {
        modal.close();
    }
}

/// Executes the chosen `TrackCreateMode` on `p` for `source`.
pub fn apply_create_mode(
    p: &mut Project,
    source: &DropAudioSource,
    mode: TrackCreateMode,
) -> Result<usize, String> {
    let sample = load_sample_from_source(source)?;
    match mode {
        TrackCreateMode::QuickSamplerOriginal => create_quick_sampler_original(p, source, sample),
        TrackCreateMode::QuickSamplerOptimized => create_quick_sampler_optimized(p, source, sample),
        TrackCreateMode::DrumKitDesigner => slice_sample_to_drum_kit_stack(p, source, sample, None),
        TrackCreateMode::SamplerZonePerNote => create_sampler_zone_per_note(p, source, sample),
    }
}

fn load_sample_from_source(source: &DropAudioSource) -> Result<Arc<SampleData>, String> {
    if let DropAudioSource::Buffer { sample, .. } = source {
        return Ok(Arc::clone(sample));
    }
    let Some(path) = source.path() else {
        return Err("Missing audio path".into());
    };
    let raw = prowave_engine::sample_file::read_sample(path)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    match source {
        DropAudioSource::File { .. } => Ok(Arc::new(raw)),
        DropAudioSource::Buffer { sample, .. } => Ok(Arc::clone(sample)),
        DropAudioSource::Region {
            start_frame,
            len_frames,
            ..
        } => {
            let start = (*start_frame as usize).min(raw.frames.len());
            let end = len_frames
                .map(|l| (start + l as usize).min(raw.frames.len()))
                .unwrap_or(raw.frames.len());
            let slice = if start < end {
                raw.frames[start..end].to_vec()
            } else {
                raw.frames.clone()
            };
            Ok(Arc::new(SampleData {
                frames: slice,
                sample_rate: raw.sample_rate,
            }))
        }
    }
}

/// Synthesizes an in-memory stereo buffer from a Drummer or MIDI region so it can be
/// sliced cleanly by `Drum Kit Designer`, `Quick Sampler`, or `Sampler (Zone Per Note)`.
pub fn render_midi_region_to_sample(
    p: &Project,
    track_idx: usize,
    region_idx: usize,
) -> SampleData {
    let sr = 44_100u32;
    let bar_len = p.bar_len(0).max(1.0);
    let region = p
        .tracks
        .get(track_idx)
        .and_then(|t| t.regions.get(region_idx));
    let len_beats = region.map_or(4.0, |r| (f64::from(r.len_bars) * bar_len).max(1.0));
    let bpm = f64::from(p.tempo_at(0.0).max(20.0));
    let sec_per_beat = 60.0 / bpm;
    let total_frames = ((len_beats * sec_per_beat * f64::from(sr)) as usize)
        .clamp(sr as usize / 2, sr as usize * 30);
    let mut frames = vec![[0.0_f32, 0.0_f32]; total_frames];

    let mut events: Vec<(f64, u8, u8)> = region
        .map(|r| {
            r.notes
                .iter()
                .map(|n| (f64::from(n.start), n.pitch, n.vel.max(40)))
                .collect()
        })
        .unwrap_or_default();
    if events.is_empty() {
        // Fallback 4/4 groove hits for generated Drummer regions without cached notes.
        let beats = len_beats.ceil() as usize;
        for b in 0..beats {
            let pitch = if b % 2 == 0 { 36 } else { 38 };
            events.push((b as f64, pitch, 110));
            events.push((b as f64 + 0.5, 42, 85));
        }
    }

    for (beat, pitch, vel) in events {
        let start_f = (beat * sec_per_beat * f64::from(sr)) as usize;
        if start_f >= total_frames {
            continue;
        }
        let amp = f32::from(vel) / 127.0 * 0.85;
        let hit_len = (sr as usize / 6).min(total_frames - start_f);
        let base_hz = 440.0 * 2.0_f32.powf((f32::from(pitch) - 69.0) / 12.0);
        for j in 0..hit_len {
            let t = j as f32 / sr as f32;
            let env = (-t * 18.0).exp() * amp;
            let sig = if pitch <= 37 {
                // Kick-like pitch drop
                let f = 140.0 * (-t * 24.0).exp() + 48.0;
                (t * std::f32::consts::TAU * f).sin()
            } else if pitch <= 41 {
                // Snare-like body + transient
                let body = (t * std::f32::consts::TAU * 190.0).sin() * 0.6;
                let snap = ((j as f32 * 0.618_033_98).fract() * 2.0 - 1.0) * 0.4;
                body + snap
            } else {
                (t * std::f32::consts::TAU * base_hz.clamp(80.0, 2400.0)).sin()
            };
            let s = sig * env;
            frames[start_f + j][0] = (frames[start_f + j][0] + s).clamp(-1.0, 1.0);
            frames[start_f + j][1] = (frames[start_f + j][1] + s).clamp(-1.0, 1.0);
        }
    }
    SampleData {
        frames,
        sample_rate: sr,
    }
}

/// Builds a `DropAudioSource` from any Audio, Drummer, or MIDI region in `p`.
pub fn source_from_region(
    p: &Project,
    track_idx: usize,
    region_idx: usize,
) -> Option<DropAudioSource> {
    let track = p.tracks.get(track_idx)?;
    let reg = track.regions.get(region_idx)?;
    let name = if reg.name.is_empty() {
        track.name.clone()
    } else {
        reg.name.clone()
    };
    if let Some(audio) = reg.audio.as_ref() {
        Some(DropAudioSource::Region {
            name,
            path: audio.file.path.clone(),
            start_frame: audio.start_frame,
            len_frames: audio.len_frames,
        })
    } else {
        let sample = Arc::new(render_midi_region_to_sample(p, track_idx, region_idx));
        Some(DropAudioSource::Buffer { name, sample })
    }
}

fn slice_cache_dir(name: &str) -> PathBuf {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let clean = slug.trim_matches('-');
    let folder = if clean.is_empty() { "slice" } else { clean };
    std::env::temp_dir()
        .join("douri-slices")
        .join(format!("{folder}-{ts}"))
}

fn write_sample_wav(path: &Path, sample: &SampleData) -> Result<(), String> {
    let mut stereo = Vec::with_capacity(sample.frames.len() * 2);
    for &[l, r] in &sample.frames {
        stereo.push(l);
        stereo.push(r);
    }
    crate::sound_library::write_wav_file(path, &stereo, 2, sample.sample_rate)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn create_quick_sampler_original(
    p: &mut Project,
    source: &DropAudioSource,
    sample: Arc<SampleData>,
) -> Result<usize, String> {
    let audio_path = match source {
        DropAudioSource::File { path, .. } => path.clone(),
        DropAudioSource::Region { .. } | DropAudioSource::Buffer { .. } => {
            let p_wav = slice_cache_dir(source.name()).join("original.wav");
            write_sample_wav(&p_wav, &sample)?;
            p_wav
        }
    };
    let mut t = Track::new(source.name().to_string(), TrackKind::Instrument);
    t.sampler = Some(SamplerUse::new(SamplerSource::Audio {
        path: audio_path,
        root: 60,
        looped: false,
    }));
    let idx = p.tracks.len();
    p.tracks.push(t);
    p.selected_track = idx;
    Ok(idx)
}

fn create_quick_sampler_optimized(
    p: &mut Project,
    source: &DropAudioSource,
    sample: Arc<SampleData>,
) -> Result<usize, String> {
    let (trimmed, root) = optimize_sample(&sample);
    let p_wav = slice_cache_dir(source.name()).join("optimized.wav");
    write_sample_wav(&p_wav, &trimmed)?;
    let mut t = Track::new(format!("{} (Opt)", source.name()), TrackKind::Instrument);
    t.sampler = Some(SamplerUse::new(SamplerSource::Audio {
        path: p_wav,
        root,
        looped: false,
    }));
    let idx = p.tracks.len();
    p.tracks.push(t);
    p.selected_track = idx;
    Ok(idx)
}

fn optimize_sample(sample: &SampleData) -> (SampleData, u8) {
    let thresh = 0.004_f32;
    let first = sample
        .frames
        .iter()
        .position(|&[l, r]| l.abs().max(r.abs()) > thresh)
        .unwrap_or(0);
    let last = sample
        .frames
        .iter()
        .rposition(|&[l, r]| l.abs().max(r.abs()) > thresh)
        .map_or(sample.frames.len(), |i| (i + 1).min(sample.frames.len()));
    let mut frames = if first < last {
        sample.frames[first..last].to_vec()
    } else {
        sample.frames.clone()
    };
    let peak = frames
        .iter()
        .fold(0.0_f32, |m, &[l, r]| m.max(l.abs()).max(r.abs()));
    if peak > 1e-5 {
        let gain = 0.98 / peak;
        for [l, r] in &mut frames {
            *l *= gain;
            *r *= gain;
        }
    }
    let mono: Vec<f32> = frames.iter().map(|&[l, r]| 0.5 * (l + r)).collect();
    let pitch = prowave_engine::track_pitch(&mono, sample.sample_rate);
    let root = pitch
        .hz
        .iter()
        .zip(pitch.voicing.iter())
        .find_map(|(&hz, &v)| {
            if hz > 20.0 && v > 0.3 {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                Some(midi.round().clamp(12.0, 108.0) as u8)
            } else {
                None
            }
        })
        .unwrap_or(60);
    (
        SampleData {
            frames,
            sample_rate: sample.sample_rate,
        },
        root,
    )
}

fn create_sampler_zone_per_note(
    p: &mut Project,
    source: &DropAudioSource,
    sample: Arc<SampleData>,
) -> Result<usize, String> {
    let slices = detect_slices_adaptive(&sample, MAX_DRUM_KIT_SLICES);
    let dir = slice_cache_dir(source.name());
    let total_frames = sample.frames.len();
    let mut sfz = String::new();
    for (i, &start_frame) in slices.iter().enumerate() {
        let end_frame = slices
            .get(i + 1)
            .copied()
            .unwrap_or(total_frames)
            .min(total_frames);
        if end_frame <= start_frame {
            continue;
        }
        let slice_sample = SampleData {
            frames: sample.frames[start_frame..end_frame].to_vec(),
            sample_rate: sample.sample_rate,
        };
        let fname = format!("slice_{i:02}.wav");
        write_sample_wav(&dir.join(&fname), &slice_sample)?;
        let key = (36 + i).min(127);
        sfz.push_str(&format!(
            "<region> sample={fname} key={key} pitch_keycenter={key} loop_mode=one_shot\n"
        ));
    }
    let sfz_path = dir.join("kit.sfz");
    std::fs::write(&sfz_path, sfz).map_err(|e| e.to_string())?;
    let mut t = Track::new(format!("{} (Slices)", source.name()), TrackKind::Instrument);
    t.sampler = Some(SamplerUse::new(SamplerSource::Sfz(sfz_path)));
    let idx = p.tracks.len();
    p.tracks.push(t);
    p.selected_track = idx;
    Ok(idx)
}

/// Detects transient slice starts with adaptive sensitivity fallback, capped at `max_slices`.
pub fn detect_slices_adaptive(sample: &SampleData, max_slices: usize) -> Vec<usize> {
    let mono: Vec<f32> = sample.frames.iter().map(|&[l, r]| 0.5 * (l + r)).collect();
    let mut slices = detect_transients(&mono, sample.sample_rate, 0.72);
    if slices.len() < 2 {
        slices = detect_transients(&mono, sample.sample_rate, 0.92);
    }
    if slices.len() > max_slices {
        slices.truncate(max_slices);
    }
    if slices.is_empty() {
        slices.push(0);
    }
    slices
}

/// Auto-slices `sample` into a collapsible Summing Track Stack (`#1 <Name>`, `#2 <Name>`, ...)
/// and creates a green MIDI groove region on the parent stack track.
pub fn slice_sample_to_drum_kit_stack(
    p: &mut Project,
    source: &DropAudioSource,
    sample: Arc<SampleData>,
    explicit_slices: Option<&[usize]>,
) -> Result<usize, String> {
    let slices: Vec<usize> = match explicit_slices {
        Some(s) if !s.is_empty() => s.iter().copied().take(MAX_DRUM_KIT_SLICES).collect(),
        _ => detect_slices_adaptive(&sample, MAX_DRUM_KIT_SLICES),
    };
    let total_frames = sample.frames.len();
    let sr = sample.sample_rate.max(1) as f64;
    let bpm = p.tempo_at(0.0).max(20.0) as f64;
    let beats_per_sec = bpm / 60.0;
    let dir = slice_cache_dir(source.name());

    let mut child_indices = Vec::with_capacity(slices.len());
    let mut notes = Vec::with_capacity(slices.len());

    for (i, &start_frame) in slices.iter().enumerate() {
        let end_frame = slices
            .get(i + 1)
            .copied()
            .unwrap_or(total_frames)
            .min(total_frames);
        if end_frame <= start_frame {
            continue;
        }
        let slice_sample = SampleData {
            frames: sample.frames[start_frame..end_frame].to_vec(),
            sample_rate: sample.sample_rate,
        };
        let slice_path = dir.join(format!("pad_{i:02}.wav"));
        write_sample_wav(&slice_path, &slice_sample)?;

        let pitch = (36 + i).min(127) as u8;
        let mut child = Track::new(
            format!("#{} {}", i + 1, source.name()),
            TrackKind::Instrument,
        );
        child.sampler = Some(SamplerUse::new(SamplerSource::Audio {
            path: slice_path,
            root: pitch,
            looped: false,
        }));
        let ci = p.tracks.len();
        p.tracks.push(child);
        child_indices.push(ci);

        let start_beats = ((start_frame as f64 / sr) * beats_per_sec) as f32;
        let dur_beats =
            (((end_frame - start_frame) as f64 / sr) * beats_per_sec).max(0.0625) as f32;
        notes.push(Note {
            start: start_beats,
            len: dur_beats,
            pitch,
            vel: 100,
            mute: false,
            lyric: Default::default(),
        });
    }

    if child_indices.is_empty() {
        return Err("No audible slices detected".into());
    }

    let parent_idx = if child_indices.len() >= 2 {
        let idx = p
            .create_track_stack(&child_indices, true)
            .ok_or_else(|| "Could not create Drum Kit Track Stack".to_string())?;
        p.tracks[idx].name = source.name().to_string();
        p.tracks[idx].color = Color32::from_rgb(0x4e, 0xb0, 0x4a);
        idx
    } else {
        child_indices[0]
    };

    let len_beats = ((total_frames as f64 / sr) * beats_per_sec).max(1.0);
    let bar_len = p.bar_len(0).max(1.0);
    let len_bars = (len_beats / bar_len).max(1.0 / 64.0) as f32;
    let mut region = Region::new_midi(source.name(), 1.0, len_bars);
    region.notes = notes;
    p.tracks[parent_idx].regions.push(region);
    p.selected_track = parent_idx;
    Ok(parent_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drum_kit_designer_creates_track_stack_and_groove_midi() {
        let mut p = Project::empty();
        let sr = 44_100u32;
        let mut frames = vec![[0.0f32, 0.0]; sr as usize * 2];
        // Three distinct drum hits at 0.0s, 0.5s, 1.0s
        for hit_sec in [0.0_f32, 0.5, 1.0] {
            let start = (hit_sec * sr as f32) as usize;
            for j in 0..800 {
                let env = (1.0 - j as f32 / 800.0) * 0.9;
                frames[start + j] = [env, env];
            }
        }
        let sample = Arc::new(SampleData {
            frames,
            sample_rate: sr,
        });
        let source = DropAudioSource::File {
            path: PathBuf::from("breakbeat.wav"),
            name: "Breakbeat".into(),
        };
        let parent_idx = slice_sample_to_drum_kit_stack(&mut p, &source, sample, None).unwrap();
        assert!(p.tracks[parent_idx].summing_stack);
        assert_eq!(p.tracks[parent_idx].name, "Breakbeat");
        assert!(p.tracks.len() >= 3);
        assert_eq!(p.tracks[parent_idx].regions.len(), 1);
        let reg = &p.tracks[parent_idx].regions[0];
        assert!(reg.notes.len() >= 2);
        assert_eq!(reg.notes[0].pitch, 36);
        assert_eq!(reg.notes[1].pitch, 37);
    }

    #[test]
    fn cross_panel_drop_target_hit_tests_headers_and_library_rects() {
        let headers = egui::Rect::from_min_max(egui::pos2(0.0, 80.0), egui::pos2(240.0, 800.0));
        let library = egui::Rect::from_min_max(egui::pos2(1100.0, 80.0), egui::pos2(1440.0, 800.0));

        assert_eq!(
            cross_panel_drop_target(egui::pos2(120.0, 420.0), Some(headers), Some(library)),
            Some(CrossPanelDropTarget::TrackHeaders)
        );
        assert_eq!(
            cross_panel_drop_target(egui::pos2(1250.0, 300.0), Some(headers), Some(library)),
            Some(CrossPanelDropTarget::SoundLibrary)
        );
        assert_eq!(
            cross_panel_drop_target(egui::pos2(600.0, 300.0), Some(headers), Some(library)),
            None
        );
    }
}
