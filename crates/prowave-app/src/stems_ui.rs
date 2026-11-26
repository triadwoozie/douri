//! Local AI Stem Separation UI, Inline Micro-Faders (V D B O), Drum Kit & Groove Extraction,
//! De-Bleed / Spill Reducer, and `.douri-chain` Track Template / FX Chain Presets.

use crate::import;
use crate::model::{AudioRegion, Note, Project, Region, RegionParams, Track, TrackKind};
use crate::UiState;
use egui::{pos2, vec2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui};
use prowave_engine::stems::{
    discover_local_onnx_models, extract_drum_kit_and_groove, separate_stems_with_model,
    stem_cache_dir, stems_model_dir, write_wav_stereo, LocalStemModel, StemKind, StemRecipe,
    StemSet,
};
use prowave_engine::AudioFile;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

const HTDEMUCS_ONNX_URL: &str =
    "https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/main/htdemucs.onnx";

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

/// Output produced by the background stem-separation worker thread.
pub struct StemSplitOutput {
    pub track_idx: usize,
    pub region_idx: usize,
    pub region_name: String,
    pub start_bar: f32,
    pub len_bars: f32,
    pub recipe: StemRecipe,
    pub stem_paths: [Option<PathBuf>; 4],
    pub mute_source: bool,
    pub debleed_pct: f32,
}

/// State for the Stem Splitter modal dialog.
pub struct StemSplitterState {
    pub open: bool,
    pub selected_model_idx: usize,
    pub custom_onnx_path: Option<PathBuf>,
    pub discovered_models: Vec<LocalStemModel>,
    pub include_stems: [bool; 4],
    pub debleed_pct: f32,
    pub mute_source_region: bool,
    pub job_running: bool,
    pub progress: Arc<Mutex<(f32, String)>>,
    pub result_rx: Option<mpsc::Receiver<Result<StemSplitOutput, String>>>,
    pub download_running: bool,
    pub download_rx: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    pub last_error: Option<String>,
}

impl Default for StemSplitterState {
    fn default() -> Self {
        Self {
            open: false,
            selected_model_idx: 0,
            custom_onnx_path: None,
            discovered_models: Vec::new(),
            include_stems: [true, true, true, true],
            debleed_pct: 15.0,
            mute_source_region: true,
            job_running: false,
            progress: Arc::new(Mutex::new((0.0, String::new()))),
            result_rx: None,
            download_running: false,
            download_rx: None,
            last_error: None,
        }
    }
}

impl StemSplitterState {
    pub fn open_dialog(&mut self) {
        self.open = true;
        self.last_error = None;
        self.discovered_models = discover_local_onnx_models();
    }
}

/// Returns the first selected audio `(track_idx, region_idx)` or the first audio region in `project`.
pub fn target_audio_region(project: &Project, s: &UiState) -> Option<(usize, usize)> {
    if let Some((ti, ri)) = s.tracks.selected {
        if project
            .tracks
            .get(ti)
            .and_then(|t| t.regions.get(ri))
            .is_some_and(|r| r.audio.is_some())
        {
            return Some((ti, ri));
        }
    }
    for (ti, t) in project.tracks.iter().enumerate() {
        for (ri, r) in t.regions.iter().enumerate() {
            if r.audio.is_some() {
                return Some((ti, ri));
            }
        }
    }
    None
}

/// Renders the Stem Splitter modal dialog and polls background AI separation / model download jobs.
pub fn show(ctx: &egui::Context, project: &mut Project, s: &mut UiState) {
    // Poll completed stem-separation job even if dialog was closed.
    if let Some(rx) = s.stem_splitter.result_rx.take() {
        match rx.try_recv() {
            Ok(Ok(out)) => {
                s.stem_splitter.job_running = false;
                apply_stem_split_output(ctx, project, s, out);
            }
            Ok(Err(err)) => {
                s.stem_splitter.job_running = false;
                s.stem_splitter.last_error = Some(err.clone());
                s.status = Some(format!("Stem Splitter failed: {err}"));
            }
            Err(mpsc::TryRecvError::Empty) => {
                s.stem_splitter.result_rx = Some(rx);
                ctx.request_repaint();
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                s.stem_splitter.job_running = false;
            }
        }
    }

    // Poll optional local ONNX model download.
    if let Some(rx) = s.stem_splitter.download_rx.take() {
        match rx.try_recv() {
            Ok(Ok(path)) => {
                s.stem_splitter.download_running = false;
                s.stem_splitter.discovered_models = discover_local_onnx_models();
                if let Some(idx) = s
                    .stem_splitter
                    .discovered_models
                    .iter()
                    .position(|m| m.path == path)
                {
                    s.stem_splitter.selected_model_idx = idx + 1;
                    s.stem_splitter.custom_onnx_path = None;
                }
                s.status = Some(format!("Downloaded local AI model to {}", path.display()));
            }
            Ok(Err(err)) => {
                s.stem_splitter.download_running = false;
                s.stem_splitter.last_error = Some(err);
            }
            Err(mpsc::TryRecvError::Empty) => {
                s.stem_splitter.download_rx = Some(rx);
                ctx.request_repaint();
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                s.stem_splitter.download_running = false;
            }
        }
    }

    if !s.stem_splitter.open {
        return;
    }

    let mut open = s.stem_splitter.open;
    let target = target_audio_region(project, s);

    egui::Window::new("Local AI Stem Splitter")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("100% Local AI Audio Source Separation")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            if let Some((ti, ri)) = target {
                let track_name = &project.tracks[ti].name;
                let reg_name = &project.tracks[ti].regions[ri].name;
                ui.label(format!("Target Region: {track_name} › {reg_name}"));
            } else {
                ui.colored_label(
                    Color32::from_rgb(0xff, 0x9b, 0x4a),
                    "No audio region found. Import an audio file first (File > Import > Audio File…).",
                );
            }

            ui.separator();
            ui.label(RichText::new("Local AI Model").strong());

            let model_label = if let Some(ref p) = s.stem_splitter.custom_onnx_path {
                format!(
                    "Custom ONNX: {}",
                    p.file_name().and_then(|n| n.to_str()).unwrap_or("model.onnx")
                )
            } else if s.stem_splitter.selected_model_idx == 0 {
                "Built-in Local AI Hybrid (Instant, 100% Offline)".to_string()
            } else if let Some(m) = s
                .stem_splitter
                .discovered_models
                .get(s.stem_splitter.selected_model_idx - 1)
            {
                format!(
                    "ONNX: {} ({:.1} MB)",
                    m.name,
                    m.size_bytes as f64 / 1_048_576.0
                )
            } else {
                "Built-in Local AI Hybrid (Instant, 100% Offline)".to_string()
            };

            egui::ComboBox::from_id_salt("stem_model_combo")
                .selected_text(model_label)
                .width(390.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            s.stem_splitter.custom_onnx_path.is_none()
                                && s.stem_splitter.selected_model_idx == 0,
                            "Built-in Local AI Hybrid (Instant, 100% Offline)",
                        )
                        .clicked()
                    {
                        s.stem_splitter.selected_model_idx = 0;
                        s.stem_splitter.custom_onnx_path = None;
                    }
                    for (idx, m) in s.stem_splitter.discovered_models.iter().enumerate() {
                        let label = format!(
                            "ONNX: {} ({:.1} MB)",
                            m.name,
                            m.size_bytes as f64 / 1_048_576.0
                        );
                        if ui
                            .selectable_label(
                                s.stem_splitter.custom_onnx_path.is_none()
                                    && s.stem_splitter.selected_model_idx == idx + 1,
                                label,
                            )
                            .clicked()
                        {
                            s.stem_splitter.selected_model_idx = idx + 1;
                            s.stem_splitter.custom_onnx_path = None;
                        }
                    }
                });

            ui.horizontal(|ui| {
                if ui.button("Load Local .onnx…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("ONNX Neural Model", &["onnx"])
                        .pick_file()
                    {
                        s.stem_splitter.custom_onnx_path = Some(path);
                    }
                }
                let has_htdemucs = s
                    .stem_splitter
                    .discovered_models
                    .iter()
                    .any(|m| m.name.to_ascii_lowercase().contains("htdemucs"));
                if !has_htdemucs
                    && ui
                        .add_enabled(
                            !s.stem_splitter.download_running,
                            egui::Button::new(if s.stem_splitter.download_running {
                                "Downloading HTDemucs ONNX…"
                            } else {
                                "Download HTDemucs v4 ONNX"
                            }),
                        )
                        .on_hover_text(format!(
                            "Downloads open-source HTDemucs v4 ONNX to {}",
                            stems_model_dir().display()
                        ))
                        .clicked()
                {
                    start_htdemucs_download(ctx, s);
                }
            });

            ui.separator();
            ui.label(RichText::new("Stems to Extract").strong());
            ui.horizontal(|ui| {
                for (i, kind) in StemKind::ALL.iter().enumerate() {
                    ui.checkbox(&mut s.stem_splitter.include_stems[i], kind.name());
                }
            });

            ui.add_space(4.0);
            ui.add(
                egui::Slider::new(&mut s.stem_splitter.debleed_pct, 0.0..=100.0)
                    .text("De-Bleed / Spill Reducer (%)"),
            );
            ui.checkbox(
                &mut s.stem_splitter.mute_source_region,
                "Mute original region after splitting",
            );

            if let Some(ref err) = s.stem_splitter.last_error {
                ui.colored_label(Color32::from_rgb(0xff, 0x5c, 0x5c), err);
            }

            if s.stem_splitter.job_running {
                let (prog, msg) = s
                    .stem_splitter
                    .progress
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or((0.0, "Processing…".to_string()));
                ui.add(egui::ProgressBar::new(prog).text(msg).animate(true));
            }

            ui.separator();
            ui.horizontal(|ui| {
                let can_run = target.is_some()
                    && !s.stem_splitter.job_running
                    && s.stem_splitter.include_stems.iter().any(|&b| b);
                if ui
                    .add_enabled(can_run, egui::Button::new("Split Stems into Tracks"))
                    .clicked()
                {
                    if let Some((ti, ri)) = target {
                        start_stem_split_job(ctx, project, s, ti, ri);
                    }
                }
                if ui
                    .add_enabled(
                        target.is_some() && !s.stem_splitter.job_running,
                        egui::Button::new("Extract Drum Kit & Groove"),
                    )
                    .clicked()
                {
                    if let Some((ti, ri)) = target {
                        let region = project.tracks[ti].regions[ri].clone();
                        match extract_kit_and_groove_to_project(project, &region) {
                            Ok(msg) => s.status = Some(msg),
                            Err(e) => s.stem_splitter.last_error = Some(e),
                        }
                    }
                }
            });
        });

    s.stem_splitter.open = open;
}

fn start_htdemucs_download(ctx: &egui::Context, s: &mut UiState) {
    s.stem_splitter.download_running = true;
    s.stem_splitter.last_error = None;
    let (tx, rx) = mpsc::channel();
    s.stem_splitter.download_rx = Some(rx);
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        let res = (|| -> Result<PathBuf, String> {
            let dir = stems_model_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let dest = dir.join("htdemucs.onnx");
            let mut resp = ureq::get(HTDEMUCS_ONNX_URL)
                .call()
                .map_err(|e| format!("Download failed: {e}"))?;
            let mut reader = resp.body_mut().as_reader();
            let mut file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
            Ok(dest)
        })();
        let _ = tx.send(res);
        ctx.request_repaint();
    });
}

fn start_stem_split_job(
    ctx: &egui::Context,
    project: &Project,
    s: &mut UiState,
    ti: usize,
    ri: usize,
) {
    let region = &project.tracks[ti].regions[ri];
    let Some(audio) = region.audio.as_ref() else {
        return;
    };

    let audio_path = audio.file.path.clone();
    let sample_rate = audio.file.sample_rate;
    let frames = audio.file.frames;
    let start_frame = audio.start_frame;
    let len_frames = audio.len_frames;
    let region_name = region.name.clone();
    let start_bar = region.start_bar;
    let len_bars = region.len_bars;
    let include = s.stem_splitter.include_stems;
    let debleed_pct = s.stem_splitter.debleed_pct;
    let mute_source = s.stem_splitter.mute_source_region;

    let model_path: Option<PathBuf> = if let Some(ref p) = s.stem_splitter.custom_onnx_path {
        Some(p.clone())
    } else if s.stem_splitter.selected_model_idx > 0 {
        s.stem_splitter
            .discovered_models
            .get(s.stem_splitter.selected_model_idx - 1)
            .map(|m| m.path.clone())
    } else {
        None
    };
    let model_id = model_path
        .as_ref()
        .and_then(|p| p.file_stem().and_then(|n| n.to_str()))
        .unwrap_or("douri-local-ai-hybrid")
        .to_string();

    s.stem_splitter.job_running = true;
    s.stem_splitter.last_error = None;
    if let Ok(mut g) = s.stem_splitter.progress.lock() {
        *g = (0.02, "Decoding audio file…".to_string());
    }

    let progress_arc = s.stem_splitter.progress.clone();
    let (tx, rx) = mpsc::channel();
    s.stem_splitter.result_rx = Some(rx);
    let ctx = ctx.clone();

    std::thread::spawn(move || {
        let res = (|| -> Result<StemSplitOutput, String> {
            let (left, right) = read_audio_stereo(&audio_path)?;
            let prog_cb = |pct: f32, msg: &str| {
                if let Ok(mut g) = progress_arc.lock() {
                    *g = (pct, msg.to_string());
                }
                ctx.request_repaint();
            };

            let stems = separate_stems_with_model(
                &left,
                &right,
                sample_rate,
                debleed_pct,
                model_path.as_deref(),
                Some(&prog_cb),
            )?;

            let recipe = StemRecipe {
                source_hash: format!("{:x}", frames ^ (sample_rate as u64)),
                start_frame,
                len_frames,
                model_id,
                debleed_pct,
            };

            let cache_dir = stem_cache_dir().join(recipe.cache_key());
            let mut stem_paths: [Option<PathBuf>; 4] = [None, None, None, None];
            for (i, kind) in StemKind::ALL.iter().enumerate() {
                if !include[i] {
                    continue;
                }
                let wav_path = cache_dir.join(format!("{}.wav", kind.name().to_lowercase()));
                write_wav_stereo(
                    &wav_path,
                    &stems.stems[i].0,
                    &stems.stems[i].1,
                    stems.sample_rate,
                )?;
                stem_paths[i] = Some(wav_path);
            }

            Ok(StemSplitOutput {
                track_idx: ti,
                region_idx: ri,
                region_name,
                start_bar,
                len_bars,
                recipe,
                stem_paths,
                mute_source,
                debleed_pct,
            })
        })();
        let _ = tx.send(res);
        ctx.request_repaint();
    });
}

fn apply_stem_split_output(
    ctx: &egui::Context,
    project: &mut Project,
    s: &mut UiState,
    out: StemSplitOutput,
) {
    let colors = [
        Color32::from_rgb(0x4a, 0x9e, 0xff), // Vocals: Blue
        Color32::from_rgb(0xff, 0x7b, 0x4a), // Drums: Orange
        Color32::from_rgb(0xb0, 0x6c, 0xff), // Bass: Purple
        Color32::from_rgb(0x3a, 0xd1, 0x8a), // Other: Green
    ];

    if out.mute_source {
        if let Some(reg) = project
            .tracks
            .get_mut(out.track_idx)
            .and_then(|t| t.regions.get_mut(out.region_idx))
        {
            reg.params.mute = true;
        }
    }

    let state = s
        .stem_regions
        .entry((out.track_idx, out.region_idx))
        .or_default();
    state.active = true;
    state.debleed_pct = out.debleed_pct;
    state.recipe = Some(out.recipe);

    let mut created = 0usize;
    for (i, kind) in StemKind::ALL.iter().enumerate() {
        let Some(ref wav_path) = out.stem_paths[i] else {
            continue;
        };
        let Ok(file) = AudioFile::open(wav_path) else {
            continue;
        };
        let stem_title = format!("{} - {}", out.region_name, kind.name());
        let region_audio: AudioRegion = import::audio_region(file, ctx);
        let mut stem_track = Track::new(stem_title.clone(), TrackKind::Audio);
        stem_track.color = colors[i];
        stem_track.regions.push(Region {
            name: stem_title,
            start_bar: out.start_bar,
            len_bars: out.len_bars,
            notes: vec![],
            controls: Vec::new(),
            folder: None,
            generated: None,
            audio: Some(region_audio),
            missing: None,
            params: RegionParams::default(),
        });
        project.tracks.push(stem_track);
        created += 1;
    }

    s.status = Some(format!(
        "Split '{}' into {created} local AI stem tracks",
        out.region_name
    ));
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
            ui.scope_id().with(("stem_fader", i, rect.left() as i32)),
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
pub fn split_region_stems_in_place(
    region: &mut Region,
    debleed_pct: f32,
) -> Result<StemSet, String> {
    let audio = region
        .audio
        .as_ref()
        .ok_or_else(|| "Selected region has no audio".to_string())?;
    let (left, right) = read_audio_stereo(&audio.file.path)?;
    let stems = separate_stems_with_model(
        &left,
        &right,
        audio.file.sample_rate,
        debleed_pct,
        None,
        None,
    )?;

    let recipe = StemRecipe {
        source_hash: format!("{:x}", audio.file.frames ^ (audio.file.sample_rate as u64)),
        start_frame: audio.start_frame,
        len_frames: audio.len_frames,
        model_id: "douri-local-ai-hybrid".to_string(),
        debleed_pct,
    };

    let cache_dir = stem_cache_dir().join(recipe.cache_key());
    for (i, kind) in StemKind::ALL.iter().enumerate() {
        let wav_path = cache_dir.join(format!("{}.wav", kind.name().to_lowercase()));
        let _ = write_wav_stereo(
            &wav_path,
            &stems.stems[i].0,
            &stems.stems[i].1,
            stems.sample_rate,
        );
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
    let stems = separate_stems_with_model(&left, &right, audio.file.sample_rate, 0.0, None, None)?;
    let (drums_l, drums_r) = &stems.stems[StemKind::Drums as usize];

    let kit = extract_drum_kit_and_groove(drums_l, drums_r, audio.file.sample_rate, project.tempo);
    if kit.hits.is_empty() {
        return Err("No drum transients detected in region".to_string());
    }

    let kit_dir = stem_cache_dir()
        .join("kits")
        .join(format!("{}_kit", region.name));
    for hit in &kit.hits {
        let _ = write_wav_stereo(
            &kit_dir.join(&hit.name),
            &hit.left,
            &hit.right,
            audio.file.sample_rate,
        );
    }
    let sfz_path = kit_dir.join("kit.sfz");
    std::fs::write(&sfz_path, &kit.sfz_content).map_err(|e| e.to_string())?;

    let mut midi_track = Track::new(
        format!("{} (Kit & Groove)", region.name),
        TrackKind::Instrument,
    );
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

fn read_audio_stereo(path: &Path) -> Result<(Vec<f32>, Vec<f32>), String> {
    let file = AudioFile::open(path).map_err(|e| e.to_string())?;
    let interleaved = file.read_stereo().map_err(|e| e.to_string())?;
    let frames = interleaved.len() / 2;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for chunk in interleaved.chunks_exact(2) {
        left.push(chunk[0]);
        right.push(chunk[1]);
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
