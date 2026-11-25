//! Save Tagged User Loop workflow (`Add Region to Loops Library`), persistence in
//! `<config_dir>/user_loops/`, and instantiation back into a project.
use crate::i18n::tr;
use crate::model::{Note, Project, Region};
use crate::track_drop_slice::{slice_sample_to_drum_kit_stack, DropAudioSource};
use egui::{vec2, Align2, Color32, Margin, RichText, Stroke};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

const INSTRUMENT_DESCRIPTORS: &[&str] = &[
    "All Drums",
    "Bass",
    "Brass",
    "Guitars",
    "Keyboards",
    "Mallets",
    "Percussion",
    "Strings",
    "Synthesizer",
    "Textures",
    "Vocals",
    "Woodwind",
];

const SUB_DESCRIPTORS: &[&str] = &["Kits", "Beats", "Kick", "Snare", "Tom", "Cymbal", "Hi-Hat"];

const MOOD_PAIRS: &[(&str, &str)] = &[
    ("Single", "Ensemble"),
    ("Clean", "Distorted"),
    ("Acoustic", "Electric"),
    ("Dry", "Processed"),
    ("Grooving", "Arrhythmic"),
    ("Melodic", "Dissonant"),
];

const GENRES: &[&str] = &[
    "Electronic",
    "Hip Hop",
    "Rock / Blues",
    "Jazz",
    "R&B",
    "Pop",
    "Cinematic",
    "World",
];

const KEYS: &[&str] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopKind {
    Loop,
    OneShot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleKind {
    Any,
    Major,
    Minor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedNote {
    pub start: f32,
    pub len: f32,
    pub pitch: u8,
    pub velocity: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserLoopEntry {
    pub id: String,
    pub name: String,
    pub loop_type: LoopKind,
    pub scale: ScaleKind,
    pub genre: String,
    pub key: String,
    pub tempo: f32,
    pub descriptors: Vec<String>,
    pub characters: Vec<String>,
    pub length_beats: f32,
    pub notes: Vec<SavedNote>,
    pub source_audio: Option<PathBuf>,
    pub slice_frames: Vec<usize>,
}

#[derive(Default)]
pub struct AddLoopModal {
    pub open: bool,
    pub name: String,
    pub loop_type: Option<LoopKind>,
    pub scale: Option<ScaleKind>,
    pub genre: String,
    pub key: String,
    pub tempo: f32,
    pub descriptors: Vec<String>,
    pub characters: Vec<String>,
    pub length_beats: f32,
    pub notes: Vec<SavedNote>,
    pub source_audio: Option<PathBuf>,
    pub slice_frames: Vec<usize>,
}

impl AddLoopModal {
    pub fn open_from_region(&mut self, p: &Project, track_idx: usize, region_idx: usize) {
        let Some(track) = p.tracks.get(track_idx) else {
            return;
        };
        let Some(region) = track.regions.get(region_idx) else {
            return;
        };
        self.open = true;
        self.name = if region.name.is_empty() {
            track.name.clone()
        } else {
            region.name.clone()
        };
        self.loop_type = Some(LoopKind::Loop);
        self.scale = Some(ScaleKind::Any);
        self.genre = GENRES[0].to_string();
        self.key = KEYS[0].to_string();
        let bar_len = p.bar_len(0).max(1.0) as f32;
        let start_beat = ((region.start_bar - 1.0).max(0.0) * bar_len) as f64;
        self.tempo = p.tempo_at(start_beat);
        self.descriptors.clear();
        self.characters.clear();
        self.length_beats = (region.len_bars * bar_len).max(1.0);
        self.notes = region
            .notes
            .iter()
            .map(|n| SavedNote {
                start: n.start,
                len: n.len,
                pitch: n.pitch,
                velocity: n.vel,
            })
            .collect();
        self.source_audio = region
            .audio
            .as_ref()
            .map(|a| a.file.path.clone())
            .or_else(|| {
                track
                    .sampler
                    .as_ref()
                    .map(|s| s.source.path().to_path_buf())
            });
        self.slice_frames.clear();
    }
}

#[derive(Default)]
pub struct UserLoopsCatalog {
    pub entries: Vec<UserLoopEntry>,
    loaded: bool,
}

impl UserLoopsCatalog {
    pub fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.entries = load_all_user_loops();
    }

    pub fn add_and_save(&mut self, entry: UserLoopEntry) {
        let _ = save_user_loop(&entry);
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
    }
}

pub fn user_loops_dir() -> PathBuf {
    crate::recent_files::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("user_loops")
}

pub fn save_user_loop(entry: &UserLoopEntry) -> Result<PathBuf, String> {
    let dir = user_loops_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", entry.id));
    let json = serde_json::to_string_pretty(entry).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_all_user_loops() -> Vec<UserLoopEntry> {
    let dir = user_loops_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in rd.flatten() {
        let p = item.path();
        if p.extension().is_some_and(|e| e == "json") {
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Ok(entry) = serde_json::from_str::<UserLoopEntry>(&text) {
                    out.push(entry);
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn instantiate_user_loop(p: &mut Project, entry: &UserLoopEntry) -> Result<usize, String> {
    if let Some(audio_path) = &entry.source_audio {
        if audio_path.exists() {
            let sample =
                prowave_engine::sample_file::read_sample(audio_path).map_err(|e| e.to_string())?;
            let src = DropAudioSource::File {
                path: audio_path.clone(),
                name: entry.name.clone(),
            };
            let slice_ref =
                (!entry.slice_frames.is_empty()).then_some(entry.slice_frames.as_slice());
            return slice_sample_to_drum_kit_stack(p, &src, Arc::new(sample), slice_ref);
        }
    }
    let notes: Vec<Note> = entry
        .notes
        .iter()
        .map(|n| Note {
            start: n.start,
            len: n.len,
            pitch: n.pitch,
            vel: n.velocity,
            mute: false,
            lyric: Default::default(),
        })
        .collect();
    let mut t = crate::model::Track::new(entry.name.clone(), crate::model::TrackKind::Instrument);
    let bar_len = p.bar_len(0).max(1.0) as f32;
    let len_bars = (entry.length_beats.max(1.0) / bar_len).max(1.0 / 64.0);
    let mut reg = Region::new_midi(&entry.name, 1.0, len_bars);
    reg.notes = notes;
    t.regions.push(reg);
    let idx = p.tracks.len();
    p.tracks.push(t);
    p.selected_track = idx;
    Ok(idx)
}

/// Renders the `Add Region to Loops Library` modal dialog.
pub fn show_add_modal(
    ctx: &egui::Context,
    modal: &mut AddLoopModal,
    catalog: &mut UserLoopsCatalog,
) {
    if !modal.open {
        return;
    }
    let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let mut create = false;

    egui::Window::new(tr("Add Region to Loops Library"))
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
            ui.set_width(500.0);
            ui.label(
                RichText::new(tr("Add Region to Loops Library"))
                    .strong()
                    .size(14.0)
                    .color(Color32::WHITE),
            );
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Name")).size(12.0));
                ui.add(egui::TextEdit::singleline(&mut modal.name).desired_width(240.0));
                ui.label(
                    RichText::new(format!("{} {:.1} BPM", tr("Tempo"), modal.tempo))
                        .size(11.5)
                        .color(Color32::from_rgb(0x98, 0x98, 0x9f)),
                );
            });
            ui.add_space(6.0);

            let mut loop_t = modal.loop_type.unwrap_or(LoopKind::Loop);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Type")).size(12.0));
                ui.radio_value(&mut loop_t, LoopKind::Loop, tr("Loop"));
                ui.radio_value(&mut loop_t, LoopKind::OneShot, tr("One-shot"));
            });
            modal.loop_type = Some(loop_t);
            ui.add_space(6.0);

            let mut scale = modal.scale.unwrap_or(ScaleKind::Any);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Scale")).size(12.0));
                egui::ComboBox::from_id_salt("loop_scale")
                    .selected_text(match scale {
                        ScaleKind::Any => "Any",
                        ScaleKind::Major => "Major",
                        ScaleKind::Minor => "Minor",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut scale, ScaleKind::Any, "Any");
                        ui.selectable_value(&mut scale, ScaleKind::Major, "Major");
                        ui.selectable_value(&mut scale, ScaleKind::Minor, "Minor");
                    });
                ui.label(RichText::new(tr("Genre")).size(12.0));
                egui::ComboBox::from_id_salt("loop_genre")
                    .selected_text(&modal.genre)
                    .show_ui(ui, |ui| {
                        for g in GENRES {
                            ui.selectable_value(&mut modal.genre, (*g).to_string(), *g);
                        }
                    });
                ui.label(RichText::new(tr("Key")).size(12.0));
                egui::ComboBox::from_id_salt("loop_key")
                    .selected_text(&modal.key)
                    .show_ui(ui, |ui| {
                        for k in KEYS {
                            ui.selectable_value(&mut modal.key, (*k).to_string(), *k);
                        }
                    });
            });
            modal.scale = Some(scale);
            ui.add_space(10.0);

            // Two-column Instrument Descriptor list
            ui.columns(2, |cols| {
                cols[0].label(
                    RichText::new(tr("Instrument Descriptors"))
                        .strong()
                        .size(11.5),
                );
                egui::ScrollArea::vertical()
                    .id_salt("loop_inst_desc")
                    .max_height(130.0)
                    .show(&mut cols[0], |ui| {
                        for d in INSTRUMENT_DESCRIPTORS {
                            let has = modal.descriptors.iter().any(|x| x == d);
                            if ui.selectable_label(has, *d).clicked() {
                                if has {
                                    modal.descriptors.retain(|x| x != d);
                                } else {
                                    modal.descriptors.push((*d).to_string());
                                }
                            }
                        }
                    });
                cols[1].label(RichText::new(tr("Sub-Descriptors")).strong().size(11.5));
                egui::ScrollArea::vertical()
                    .id_salt("loop_sub_desc")
                    .max_height(130.0)
                    .show(&mut cols[1], |ui| {
                        for d in SUB_DESCRIPTORS {
                            let has = modal.descriptors.iter().any(|x| x == d);
                            if ui.selectable_label(has, *d).clicked() {
                                if has {
                                    modal.descriptors.retain(|x| x != d);
                                } else {
                                    modal.descriptors.push((*d).to_string());
                                }
                            }
                        }
                    });
            });
            ui.add_space(8.0);

            // Paired Character / Mood toggle pills
            ui.label(RichText::new(tr("Mood & Character")).strong().size(11.5));
            ui.horizontal_wrapped(|ui| {
                for &(left, right) in MOOD_PAIRS {
                    for tag in [left, right] {
                        let on = modal.characters.iter().any(|x| x == tag);
                        let btn = egui::Button::new(RichText::new(tag).size(11.0).color(if on {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(0xb8, 0xb8, 0xc0)
                        }))
                        .fill(if on {
                            Color32::from_rgb(0x12, 0x69, 0xcc)
                        } else {
                            Color32::from_rgb(0x28, 0x28, 0x2b)
                        })
                        .corner_radius(5);
                        if ui.add(btn).clicked() {
                            if on {
                                modal.characters.retain(|x| x != tag);
                            } else {
                                let other = if tag == left { right } else { left };
                                modal.characters.retain(|x| x != other);
                                modal.characters.push(tag.to_string());
                            }
                        }
                    }
                }
            });
            ui.add_space(12.0);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new(tr("Create")).color(Color32::WHITE))
                            .fill(Color32::from_rgb(0x12, 0x69, 0xcc))
                            .corner_radius(5)
                            .min_size(vec2(88.0, 26.0)),
                    )
                    .clicked()
                {
                    create = true;
                }
                if ui
                    .add(
                        egui::Button::new(RichText::new(tr("Cancel")).color(Color32::WHITE))
                            .fill(Color32::from_rgb(0x3a, 0x3a, 0x3e))
                            .corner_radius(5)
                            .min_size(vec2(88.0, 26.0)),
                    )
                    .clicked()
                {
                    close = true;
                }
            });
        });

    if create {
        let slug: String = modal
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let entry = UserLoopEntry {
            id: format!("loop-{}", slug.trim_matches('-')),
            name: modal.name.trim().to_string(),
            loop_type: modal.loop_type.unwrap_or(LoopKind::Loop),
            scale: modal.scale.unwrap_or(ScaleKind::Any),
            genre: modal.genre.clone(),
            key: modal.key.clone(),
            tempo: modal.tempo,
            descriptors: modal.descriptors.clone(),
            characters: modal.characters.clone(),
            length_beats: modal.length_beats,
            notes: modal.notes.clone(),
            source_audio: modal.source_audio.clone(),
            slice_frames: modal.slice_frames.clone(),
        };
        catalog.add_and_save(entry);
        modal.open = false;
    } else if close {
        modal.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_loop_round_trips_json_and_instantiates_midi_track() {
        let entry = UserLoopEntry {
            id: "test-drum-loop".into(),
            name: "Test Drum Loop".into(),
            loop_type: LoopKind::Loop,
            scale: ScaleKind::Minor,
            genre: "Electronic".into(),
            key: "C".into(),
            tempo: 120.0,
            descriptors: vec!["All Drums".into(), "Beats".into()],
            characters: vec!["Grooving".into(), "Dry".into()],
            length_beats: 4.0,
            notes: vec![SavedNote {
                start: 0.0,
                len: 0.5,
                pitch: 36,
                velocity: 100,
            }],
            source_audio: None,
            slice_frames: Vec::new(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: UserLoopEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Test Drum Loop");
        assert_eq!(back.notes.len(), 1);

        let mut p = Project::empty();
        let idx = instantiate_user_loop(&mut p, &back).unwrap();
        assert_eq!(p.tracks[idx].name, "Test Drum Loop");
        assert_eq!(p.tracks[idx].regions.len(), 1);
        assert_eq!(p.tracks[idx].regions[0].notes.len(), 1);
    }
}
