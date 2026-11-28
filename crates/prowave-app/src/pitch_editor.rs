//! Audio Track Editor: waveform view, region controls, Audacity-ported cleanup tools,
//! and 1-click external PitchNet VST3 / Standalone launcher.

use crate::i18n::{tr, trf};
use crate::model::{Insert, Project, TrackKind};
use crate::{Bottom, UiState};
use egui::{pos2, vec2, Color32, FontId, Rect, Sense, Stroke, Ui};
use prowave_engine::{AudioFile, FlexMode};
use std::path::{Path, PathBuf};

/// Kept for `.douri` project round-trip compatibility with saved Flex Pitch edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Algorithm {
    #[default]
    Melodic,
    Percussive,
    Universal,
}

impl Algorithm {
    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Melodic => "Melodic",
            Algorithm::Percussive => "Percussive",
            Algorithm::Universal => "Universal",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "Melodic" => Some(Algorithm::Melodic),
            "Percussive" => Some(Algorithm::Percussive),
            "Universal" => Some(Algorithm::Universal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteEdit {
    pub start: f64,
    pub end: f64,
    pub pitch: f32,
    pub onset: f64,
    pub new_start: f64,
    pub new_end: f64,
    pub shift: f32,
    pub drift: f32,
    pub modulation: f32,
    pub formant: f32,
    pub gain_db: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MelodyEdit {
    pub algorithm: Algorithm,
    pub notes: Vec<NoteEdit>,
}

impl MelodyEdit {
    pub fn is_neutral(&self) -> bool {
        true
    }

    pub fn fingerprint(&self, mix: &mut impl FnMut(u64)) {
        mix(0);
    }

    pub fn warp_points(&self, _duration: f64) -> Vec<prowave_engine::WarpPoint> {
        Vec::new()
    }
}

pub fn to_doc(m: &MelodyEdit) -> prowave_project::MelodyDoc {
    prowave_project::MelodyDoc {
        algorithm: m.algorithm.name().to_string(),
        notes: m
            .notes
            .iter()
            .map(|n| prowave_project::NoteEditDoc {
                start: n.start,
                end: n.end,
                pitch: n.pitch,
                onset: n.onset,
                new_start: n.new_start,
                new_end: n.new_end,
                shift: n.shift,
                drift: n.drift,
                modulation: n.modulation,
                formant: n.formant,
                gain_db: n.gain_db,
            })
            .collect(),
    }
}

pub fn from_doc(d: &prowave_project::MelodyDoc) -> Option<MelodyEdit> {
    let algorithm = Algorithm::from_name(&d.algorithm)?;
    let notes = d
        .notes
        .iter()
        .map(|n| NoteEdit {
            start: n.start,
            end: n.end,
            pitch: n.pitch,
            onset: n.onset,
            new_start: n.new_start,
            new_end: n.new_end,
            shift: n.shift,
            drift: n.drift,
            modulation: n.modulation,
            formant: n.formant,
            gain_db: n.gain_db,
        })
        .collect();
    Some(MelodyEdit { algorithm, notes })
}

pub fn analysing() -> bool {
    false
}

pub fn render(_original: &AudioFile, input: Vec<f32>, _m: &MelodyEdit) -> Result<Vec<f32>, String> {
    Ok(input)
}

/// Persistent view state for the bottom Audio Track Editor.
#[derive(Clone, Debug)]
pub struct PitchView {
    pub region: Option<(usize, usize)>,
    pub zoom: f32,
    pub scroll: f64,
    pub catch_playhead: bool,
    was_playing: bool,
    catch_jump: bool,
    fitted: Option<(usize, usize, PathBuf)>,
    pub plot: Rect,
    pub pitchnet_status: Option<String>,
    pub panel_height: f32,
}

impl Default for PitchView {
    fn default() -> Self {
        Self {
            region: None,
            zoom: 120.0,
            scroll: 0.0,
            catch_playhead: true,
            was_playing: false,
            catch_jump: false,
            fitted: None,
            plot: Rect::NOTHING,
            pitchnet_status: None,
            panel_height: 360.0,
        }
    }
}

pub fn open(s: &mut UiState, t: usize, r: usize) {
    open_in(&mut s.pitch_ed, &mut s.bottom, t, r);
}

pub fn open_in(v: &mut PitchView, bottom: &mut Bottom, t: usize, r: usize) {
    v.region = Some((t, r));
    *bottom = Bottom::PitchEditor;
}

pub fn target(p: &Project, s: &UiState) -> Option<(usize, usize)> {
    let audio = |(t, r): (usize, usize)| {
        p.tracks.get(t).is_some_and(|tr| {
            tr.kind == TrackKind::Audio && tr.regions.get(r).is_some_and(|x| x.audio.is_some())
        })
    };
    if let Some(tr) = p.tracks.get(p.selected_track) {
        if tr.kind == TrackKind::Audio {
            if let Some(sel) = s
                .tracks
                .selected
                .filter(|&(t, _)| t == p.selected_track)
                .filter(|x| audio(*x))
            {
                return Some(sel);
            }
            if let Some(reg) = s
                .pitch_ed
                .region
                .filter(|&(t, _)| t == p.selected_track)
                .filter(|x| audio(*x))
            {
                return Some(reg);
            }
            if let Some(ri) = tr
                .regions
                .iter()
                .position(|x| {
                    x.audio.is_some()
                        && p.region_start_beats(x) <= p.position_beats
                        && p.region_end_beats(x) >= p.position_beats
                })
                .or_else(|| tr.regions.iter().position(|x| x.audio.is_some()))
            {
                return Some((p.selected_track, ri));
            }
        }
    }
    s.pitch_ed
        .region
        .filter(|x| audio(*x))
        .or(s.tracks.selected.filter(|x| audio(*x)))
}

struct Clock {
    map: prowave_engine::TempoMap,
    s0: f64,
    offset: f64,
}

impl Clock {
    fn beat(&self, t: f64) -> f64 {
        self.map.beat_at(self.s0 + t - self.offset)
    }

    fn secs(&self, beat: f64) -> f64 {
        self.map.seconds_at(beat) - self.s0 + self.offset
    }
}

/// Discovers a locally installed PitchNet VST3 bundle (`PitchNet.vst3`).
pub fn find_pitchnet_vst3() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("PITCHNET_VST3") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(h) = home {
        candidates.push(h.join(".vst3").join("PitchNet.vst3"));
        candidates.push(
            h.join(".config")
                .join("douri")
                .join("plugins")
                .join("PitchNet.vst3"),
        );
    }
    candidates.push(PathBuf::from("/usr/lib/vst3/PitchNet.vst3"));
    candidates.push(PathBuf::from("/usr/local/lib/vst3/PitchNet.vst3"));
    candidates.into_iter().find(|p| p.exists())
}

/// Discovers a locally installed PitchNet standalone binary.
pub fn find_pitchnet_standalone() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("PITCHNET_BIN") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(h) = home {
        candidates.push(h.join(".local/bin/pitchnet"));
        candidates.push(h.join(".local/bin/PitchNet"));
        candidates.push(h.join(".local/share/Session Loops/PitchNet/PitchNet"));
        candidates.push(h.join(".config/douri/plugins/PitchNet"));
    }
    candidates.push(PathBuf::from("/usr/bin/pitchnet"));
    candidates.push(PathBuf::from("/usr/bin/PitchNet"));
    candidates.push(PathBuf::from("/usr/local/bin/pitchnet"));
    candidates.push(PathBuf::from("/usr/local/bin/PitchNet"));
    candidates.into_iter().find(|p| p.is_file())
}

fn sync_pitchnet_language() {
    let douri_lang = crate::i18n::current().code();
    let pitchnet_lang = match douri_lang {
        "ja" => "ja",
        "es" => "es",
        "pt-BR" | "pt-PT" => "pt",
        "zh-Hans" => "zh",
        _ => "en",
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let cfg_dir = home.join(".config").join("PitchNet");
    let _ = std::fs::create_dir_all(&cfg_dir);
    let cfg_path = cfg_dir.join("config.json");
    let mut obj = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&s).ok())
        .unwrap_or_default();
    obj.insert(
        "language".to_string(),
        serde_json::Value::String(pitchnet_lang.to_string()),
    );
    if let Ok(json) = serde_json::to_string_pretty(&obj) {
        let _ = std::fs::write(&cfg_path, json);
    }
}

/// Opens PitchNet for track `ti`. Prefers the PitchNet standalone app with `audio_path` so the
/// region's audio loads directly into PitchNet's editor without muting or stealing DAW playback,
/// and falls back to the VST3 plugin only if standalone is not installed.
pub fn open_in_pitchnet(p: &mut Project, s: &mut UiState, ti: usize, audio_path: &Path) {
    sync_pitchnet_language();

    // 1. Prefer PitchNet standalone with the region's audio file so the audio appears in its UI
    //    and DAW track playback is never muted/stolen by a non-ARA VST3 insert.
    if let Some(bin) = find_pitchnet_standalone() {
        // Remove any leftover PitchNet insert that would silence the track in non-ARA VST3 mode.
        if let Some(tr) = p.tracks.get_mut(ti) {
            let before = tr.inserts.len();
            tr.inserts.retain(|ins| {
                ins.hosted == 0
                    || !crate::hosted_plugins::name(ins.hosted)
                        .to_ascii_lowercase()
                        .contains("pitchnet")
            });
            if tr.inserts.len() != before {
                p.song_rev += 1;
            }
        }
        match std::process::Command::new(&bin)
            .arg(audio_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(_) => {
                s.pitch_ed.pitchnet_status =
                    Some("Launched PitchNet with region audio".to_string());
            }
            Err(e) => {
                s.pitch_ed.pitchnet_status = Some(format!("Failed to launch PitchNet: {e}"));
            }
        }
        return;
    }

    // 2. Check already-loaded PitchNet hosted plugin on the track.
    if let Some(slot) = p.tracks.get(ti).and_then(|t| {
        t.inserts.iter().position(|ins| {
            ins.hosted != 0
                && crate::hosted_plugins::name(ins.hosted)
                    .to_ascii_lowercase()
                    .contains("pitchnet")
        })
    }) {
        s.mixer.plugin_controls = false;
        s.mixer.fx_window = Some((ti, slot));
        s.pitch_ed.pitchnet_status = Some("Opened PitchNet VST3 editor".to_string());
        return;
    }

    // 2. Check hosted_plugins catalog (VST3/CLAP/AU/LV2) for PitchNet.
    let catalog_entry = crate::hosted_plugins::catalog()
        .into_iter()
        .find(|info| info.name.to_ascii_lowercase().contains("pitchnet"));
    if let Some(info) = catalog_entry {
        if let Some(tr) = p.tracks.get_mut(ti) {
            let slot = tr.inserts.len();
            tr.inserts
                .push(Insert::new_hosted(crate::hosted_plugins::add(&info, None)));
            p.song_rev += 1;
            s.mixer.plugin_controls = false;
            s.mixer.fx_window = Some((ti, slot));
            s.pitch_ed.pitchnet_status = Some("Inserted PitchNet plugin on track".to_string());
            return;
        }
    }

    // 3. Check PitchNet VST3 path directly on disk.
    if let Some(vst3_path) = find_pitchnet_vst3() {
        if let Ok(mut list) =
            prowave_plugin_host::scan_file(prowave_plugin_host::Format::Vst3, &vst3_path)
        {
            if let Some(info) = list.drain(..).next() {
                if let Some(tr) = p.tracks.get_mut(ti) {
                    let slot = tr.inserts.len();
                    tr.inserts
                        .push(Insert::new_hosted(crate::hosted_plugins::add(&info, None)));
                    p.song_rev += 1;
                    s.mixer.plugin_controls = false;
                    s.mixer.fx_window = Some((ti, slot));
                    s.pitch_ed.pitchnet_status = Some("Loaded PitchNet.vst3".to_string());
                    return;
                }
            }
        }
    }

    s.pitch_ed.pitchnet_status =
        Some("Install PitchNet into ~/.local/bin/pitchnet or ~/.vst3/PitchNet.vst3".to_string());
}

#[derive(Clone, Copy)]
enum CleanupKind {
    DcOffset,
    ClickRemoval,
    NoiseReduction,
}

fn apply_cleanup_to_region(
    p: &mut Project,
    ti: usize,
    ri: usize,
    kind: CleanupKind,
    ctx: &egui::Context,
) -> Result<String, String> {
    let audio = p
        .tracks
        .get(ti)
        .and_then(|t| t.regions.get(ri))
        .and_then(|r| r.audio.as_ref())
        .ok_or_else(|| "No audio region".to_string())?;
    let src_path = audio.file.path.clone();
    let sr = audio.file.sample_rate.max(1);
    let interleaved = audio.file.read_stereo().map_err(|e| e.to_string())?;
    let frames = interleaved.len() / 2;
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for ch in interleaved.chunks_exact(2) {
        left.push(ch[0]);
        right.push(ch[1]);
    }
    let label = match kind {
        CleanupKind::DcOffset => {
            prowave_engine::audio_cleanup::remove_dc_offset(&mut left, &mut right);
            "DC offset removed"
        }
        CleanupKind::ClickRemoval => {
            prowave_engine::audio_cleanup::click_removal(&mut left, 4.0, 12);
            prowave_engine::audio_cleanup::click_removal(&mut right, 4.0, 12);
            "De-click applied"
        }
        CleanupKind::NoiseReduction => {
            prowave_engine::audio_cleanup::noise_reduction_spectral(&mut left, sr, 18.0, 1.5);
            prowave_engine::audio_cleanup::noise_reduction_spectral(&mut right, sr, 18.0, 1.5);
            "Noise reduction applied"
        }
    };
    let stem = src_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    let out_dir = crate::flex_edit::cache_dir().join("cleaned");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let out_path = out_dir.join(format!("{stem}-{nonce}.wav"));
    prowave_engine::stems::write_wav_stereo(&out_path, &left, &right, sr)?;
    let new_file = AudioFile::open(&out_path).map_err(|e| e.to_string())?;
    let new_region_audio = crate::import::audio_region(new_file, ctx);
    if let Some(reg) = p.tracks.get_mut(ti).and_then(|t| t.regions.get_mut(ri)) {
        reg.audio = Some(new_region_audio);
        p.song_rev += 1;
    }
    Ok(label.to_string())
}

const BG: Color32 = crate::theme::CANVAS_BG;
const SURFACE: Color32 = crate::theme::CHROME_BG;
const CARD: Color32 = crate::theme::CARD_BG;
const BORDER: Color32 = crate::theme::BORDER;
const ACCENT: Color32 = Color32::from_rgb(0x12, 0x69, 0xcc);
const TEXT: Color32 = crate::theme::TEXT;
const DIM: Color32 = crate::theme::TEXT_MUTED;
const INSPECTOR_W: f32 = 236.0;
const RULER_H: f32 = 22.0;
const REGION_STRIP_H: f32 = 16.0;
const TIME_H: f32 = RULER_H + REGION_STRIP_H;
const STATUS_H: f32 = 20.0;

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState) {
    // Top-edge resize handle owned directly by `s.pitch_ed.panel_height` so resizing never snaps back.
    let top_rect = Rect::from_min_size(
        ui.cursor().min,
        egui::vec2(ui.available_width().max(1.0), 5.0),
    );
    let resize_resp = ui.allocate_rect(top_rect, Sense::drag());
    if resize_resp.hovered() || resize_resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if resize_resp.dragged() {
        let max_h = (ui.ctx().content_rect().height() * 0.85).max(540.0);
        s.pitch_ed.panel_height =
            (s.pitch_ed.panel_height - resize_resp.drag_delta().y).clamp(160.0, max_h);
    }
    ui.painter().rect_filled(
        top_rect,
        0.0,
        if resize_resp.hovered() || resize_resp.dragged() {
            ACCENT
        } else {
            BORDER
        },
    );

    let area = ui.available_rect_before_wrap();
    ui.painter().rect_filled(area, 0.0, BG);
    let Some((t, r)) = target(p, s) else {
        ui.allocate_rect(area, Sense::hover());
        ui.painter().text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            tr("Double-click an audio region to edit its notes"),
            FontId::proportional(13.5),
            DIM,
        );
        return;
    };
    let Some(audio) = p.tracks[t].regions[r].audio.as_ref() else {
        return;
    };
    let file = crate::flex_edit::recording_of(audio);
    let audio_offset = audio.start_frame as f64 / f64::from(file.sample_rate.max(1));
    let audio_peaks = audio.peaks.clone();
    let duration = file.duration_secs();
    let region_key = (t, r, file.path.clone());
    let region_name = p.tracks[t].regions[r].name.clone();
    let region_start_beats = p.region_start_beats(&p.tracks[t].regions[r]);
    let region_end_beats = p.region_end_beats(&p.tracks[t].regions[r]);
    let file_sr = file.sample_rate;
    let file_ch = file.channels;
    let file_path = file.path.clone();

    // Top toolbar (Dōri #252526 chrome)
    let mut open_pitchnet = false;
    let mut cleanup_cmd: Option<CleanupKind> = None;

    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            let v = &mut s.pitch_ed;
            ui.horizontal(|ui| {
                let flex_on = p.tracks[t].params.flex_enabled;
                let flex_btn = ui.add(
                    egui::Button::new(
                        egui::RichText::new(tr("Flex"))
                            .size(11.5)
                            .color(if flex_on { Color32::WHITE } else { DIM }),
                    )
                    .fill(if flex_on { ACCENT } else { CARD }),
                );
                if flex_btn.clicked() {
                    p.tracks[t].params.flex_enabled = !flex_on;
                    if p.tracks[t].params.flex_enabled && p.tracks[t].params.flex_mode.is_none() {
                        p.tracks[t].params.flex_mode = Some(FlexMode::Monophonic);
                    }
                    p.song_rev += 1;
                }

                ui.separator();
                let pitchnet_btn = ui.add(
                    egui::Button::new(
                        egui::RichText::new(tr("Open PitchNet Editor"))
                            .size(11.5)
                            .color(Color32::WHITE),
                    )
                    .fill(ACCENT),
                );
                if pitchnet_btn.clicked() {
                    open_pitchnet = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_sized(
                        [76.0, 18.0],
                        egui::Slider::new(&mut v.zoom, 10.0..=2000.0)
                            .logarithmic(true)
                            .show_value(false),
                    )
                    .on_hover_text(tr("Horizontal Zoom"));
                    let fit_btn = ui.add(
                        egui::Button::new(egui::RichText::new(tr("Fit")).size(11.0).color(TEXT))
                            .fill(CARD),
                    );
                    if fit_btn.clicked() {
                        v.fitted = None;
                    }
                    let catch_btn = ui.add(
                        egui::Button::new(egui::RichText::new(tr("Catch")).size(11.0).color(
                            if v.catch_playhead {
                                Color32::WHITE
                            } else {
                                DIM
                            },
                        ))
                        .fill(if v.catch_playhead {
                            ACCENT
                        } else {
                            CARD
                        }),
                    );
                    if catch_btn.clicked() {
                        v.catch_playhead = !v.catch_playhead;
                        if v.catch_playhead {
                            v.catch_jump = true;
                        }
                    }
                });
            });
        });

    let body_rect = ui.available_rect_before_wrap();
    let insp_rect = Rect::from_min_max(
        body_rect.min,
        pos2(
            (body_rect.left() + INSPECTOR_W).min(body_rect.right()),
            body_rect.bottom(),
        ),
    );
    let canvas = Rect::from_min_max(pos2(insp_rect.right(), body_rect.top()), body_rect.max);

    // Left Audio Track Inspector (#252526)
    let mut insp_ui = ui.new_child(egui::UiBuilder::new().max_rect(insp_rect));
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(8))
        .show(&mut insp_ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(INSPECTOR_W - 20.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(
                        egui::RichText::new(&p.tracks[t].name)
                            .strong()
                            .size(12.5)
                            .color(TEXT),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.1} kHz · {} ch · {:.2} s",
                            file_sr as f32 / 1000.0,
                            file_ch,
                            duration
                        ))
                        .size(10.5)
                        .color(DIM),
                    );
                    ui.add_space(4.0);

                    // Region controls
                    let reg = &mut p.tracks[t].regions[r];
                    ui.label(egui::RichText::new(tr("Region")).size(11.0).color(DIM));
                    let mut gain_db = reg.params.gain_db;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tr("Gain")).size(11.0).color(DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{gain_db:+.1} dB"))
                                    .size(11.0)
                                    .color(TEXT),
                            );
                        });
                    });
                    if ui
                        .add(egui::Slider::new(&mut gain_db, -48.0..=24.0).show_value(false))
                        .changed()
                    {
                        reg.params.gain_db = gain_db;
                        p.song_rev += 1;
                    }

                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut reg.params.mute, tr("Mute")).changed() {
                            p.song_rev += 1;
                        }
                        let mut looped = reg.params.loop_beats > 0.0;
                        if ui.checkbox(&mut looped, tr("Loop")).changed() {
                            reg.params.loop_beats = if looped {
                                (reg.len_bars * 4.0).max(1.0)
                            } else {
                                0.0
                            };
                            p.song_rev += 1;
                        }
                        if ui
                            .checkbox(&mut reg.params.reverse, tr("Reverse"))
                            .changed()
                        {
                            p.song_rev += 1;
                        }
                    });

                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(tr("Audio Cleanup"))
                            .size(11.0)
                            .color(DIM),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .add(egui::Button::new(tr("DC Offset")).fill(CARD))
                            .clicked()
                        {
                            cleanup_cmd = Some(CleanupKind::DcOffset);
                        }
                        if ui
                            .add(egui::Button::new(tr("De-Click")).fill(CARD))
                            .clicked()
                        {
                            cleanup_cmd = Some(CleanupKind::ClickRemoval);
                        }
                        if ui
                            .add(egui::Button::new(tr("Denoise")).fill(CARD))
                            .clicked()
                        {
                            cleanup_cmd = Some(CleanupKind::NoiseReduction);
                        }
                    });

                    ui.add_space(6.0);
                    egui::Frame::new()
                        .fill(CARD)
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(4)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(tr("PitchNet Editor"))
                                    .strong()
                                    .size(11.5)
                                    .color(TEXT),
                            );
                            let detected = find_pitchnet_vst3().is_some()
                                || find_pitchnet_standalone().is_some();
                            ui.label(
                                egui::RichText::new(if detected {
                                    tr("External PitchNet ready")
                                } else {
                                    tr("External VST3 / Standalone plugin")
                                })
                                .size(10.5)
                                .color(DIM),
                            );
                            ui.add_space(4.0);
                            if ui
                                .add_sized(
                                    [ui.available_width(), 24.0],
                                    egui::Button::new(
                                        egui::RichText::new(tr("Open PitchNet Editor"))
                                            .size(11.5)
                                            .color(Color32::WHITE),
                                    )
                                    .fill(ACCENT),
                                )
                                .clicked()
                            {
                                open_pitchnet = true;
                            }
                            if let Some(ref msg) = s.pitch_ed.pitchnet_status {
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(msg).size(10.0).color(DIM));
                            }
                        });
                });
        });

    if open_pitchnet {
        open_in_pitchnet(p, s, t, &file_path);
    }
    if let Some(kind) = cleanup_cmd {
        match apply_cleanup_to_region(p, t, r, kind, ui.ctx()) {
            Ok(msg) => s.status = Some(msg),
            Err(e) => s.status = Some(format!("Cleanup failed: {e}")),
        }
    }

    // Main Waveform Timeline Canvas
    let time_ruler_rect =
        Rect::from_min_max(canvas.min, pos2(canvas.right(), canvas.top() + RULER_H));
    let region_strip_rect = Rect::from_min_max(
        pos2(canvas.left(), canvas.top() + RULER_H),
        pos2(canvas.right(), canvas.top() + TIME_H),
    );
    let plot = Rect::from_min_max(
        pos2(canvas.left(), canvas.top() + TIME_H),
        pos2(canvas.right(), canvas.bottom() - STATUS_H),
    );
    let time_resp = ui.allocate_rect(time_ruler_rect, Sense::click_and_drag());
    let _plot_resp = ui.allocate_rect(plot, Sense::click_and_drag());
    let g = ui.painter_at(canvas);
    g.rect_filled(canvas, 0.0, BG);

    let region = &p.tracks[t].regions[r];
    let clock = Clock {
        map: p.tempo_map(),
        s0: p.tempo_map().seconds_at(p.region_start_beats(region)),
        offset: audio_offset,
    };
    let region_secs = clock.map.seconds_at(p.region_end_beats(region)) - clock.s0;
    let v = &mut s.pitch_ed;
    v.plot = plot;

    if v.fitted.as_ref() != Some(&region_key) && plot.width() > 10.0 {
        v.fitted = Some(region_key.clone());
        let full_len = region_secs.clamp(0.5, duration.max(0.5));
        let target_w = (plot.width() * 0.90).max(40.0);
        v.zoom = (target_w / full_len as f32).clamp(10.0, 2000.0);
        v.scroll = (clock.offset - full_len * 0.04).max(0.0);
    }

    let is_running = p.playing || p.recording;
    if !v.was_playing && is_running {
        v.catch_playhead = true;
    }
    v.was_playing = is_running;

    let (middle_down, middle_delta) = ui.input(|i| (i.pointer.middle_down(), i.pointer.delta()));
    if middle_down && ui.rect_contains_pointer(canvas) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if middle_delta.x != 0.0 && is_running {
            v.catch_playhead = false;
        }
        v.scroll = (v.scroll - f64::from(middle_delta.x / v.zoom)).max(0.0);
    }
    if ui.rect_contains_pointer(canvas) {
        let (scroll_delta, zoom_delta, modifiers, pointer_pos) = ui.input(|i| {
            (
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.modifiers,
                i.pointer.hover_pos(),
            )
        });
        let ctrl = modifiers.command || modifiers.ctrl;
        if ctrl || (zoom_delta - 1.0).abs() > 1e-4 {
            let factor = crate::widgets::wheel_zoom_factor(scroll_delta.y, zoom_delta);
            if (factor - 1.0).abs() > 1e-4 {
                let px = pointer_pos.map_or(plot.center().x, |pt| pt.x);
                let (new_scroll_px, new_zoom) = crate::widgets::cursor_anchored_zoom(
                    plot.left(),
                    px,
                    v.scroll as f32 * v.zoom,
                    v.zoom,
                    factor,
                    10.0,
                    2000.0,
                );
                v.zoom = new_zoom;
                v.scroll = f64::from(new_scroll_px / new_zoom).max(0.0);
            }
        } else if modifiers.shift || scroll_delta.x != 0.0 {
            let dx = if modifiers.shift && scroll_delta.x == 0.0 {
                scroll_delta.y
            } else {
                scroll_delta.x
            };
            if dx != 0.0 && is_running {
                v.catch_playhead = false;
            }
            v.scroll = (v.scroll - f64::from(dx / v.zoom)).max(0.0);
        }
    }
    if v.catch_jump || (v.catch_playhead && is_running) {
        v.catch_jump = false;
        let ph_sec = clock.secs(p.position_beats).max(0.0);
        let view_secs = f64::from(plot.width().max(40.0) / v.zoom.max(1.0));
        if ph_sec < v.scroll || ph_sec > v.scroll + view_secs * 0.92 {
            v.scroll = (ph_sec - view_secs * 0.1).max(0.0);
        }
    }

    let (scroll, zoom) = (v.scroll, v.zoom);
    let x_of = |sec: f64| plot.left() + ((sec - scroll) as f32) * zoom;
    let sec_of = |x: f32| scroll + f64::from((x - plot.left()) / zoom);

    // Time ruler + region strip
    g.rect_filled(time_ruler_rect, 0.0, SURFACE);
    g.rect_filled(region_strip_rect, 0.0, Color32::from_rgb(0x23, 0x2b, 0x38));
    let reg_x0 = x_of(clock.offset).max(region_strip_rect.left());
    let reg_x1 = x_of(clock.offset + region_secs).min(region_strip_rect.right());
    if reg_x1 > reg_x0 + 2.0 {
        let reg_bar = Rect::from_min_max(
            pos2(reg_x0, region_strip_rect.top() + 1.0),
            pos2(reg_x1, region_strip_rect.bottom() - 1.0),
        );
        g.rect_filled(reg_bar, 2.0, Color32::from_rgb(0x1f, 0x4d, 0x7a));
        let bar_start = p.beats_to_bar(region_start_beats).floor() as i64 + 1;
        let bar_end = p.beats_to_bar(region_end_beats).ceil() as i64 + 1;
        g.text(
            pos2(reg_bar.left() + 6.0, reg_bar.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{region_name}  ({bar_start} - {bar_end})"),
            FontId::proportional(10.0),
            TEXT,
        );
    }

    let (b0, b1) = (
        clock.beat(sec_of(plot.left())).floor(),
        clock.beat(sec_of(plot.right())).ceil(),
    );
    if b1 - b0 < 2000.0 {
        let mut b = b0;
        while b <= b1 {
            let x = x_of(clock.secs(b));
            if x >= plot.left() && x <= plot.right() {
                let bar_pos = p.beats_to_bar(b.max(0.0));
                let bar = bar_pos.round() as u32 + 1;
                let is_bar = bar_pos.fract() < 0.01 || bar_pos.fract() > 0.99;
                g.line_segment(
                    [pos2(x, plot.top()), pos2(x, plot.bottom())],
                    Stroke::new(
                        1.0,
                        if is_bar {
                            Color32::from_rgb(0x34, 0x36, 0x3c)
                        } else {
                            Color32::from_rgb(0x26, 0x27, 0x2c)
                        },
                    ),
                );
                if is_bar {
                    g.text(
                        pos2(x + 3.0, time_ruler_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{bar}"),
                        FontId::proportional(10.0),
                        DIM,
                    );
                }
            }
            b += 1.0;
        }
    }

    // Center line & crisp waveform overview
    let gp = ui.painter_at(plot);
    let mid_y = plot.center().y;
    gp.line_segment(
        [pos2(plot.left(), mid_y), pos2(plot.right(), mid_y)],
        Stroke::new(1.0, BORDER),
    );
    if let Some(peaks) = audio_peaks.get() {
        if !peaks.is_empty() && duration > 0.0 {
            let gain_lin = 10f32.powf(region.params.gain_db / 20.0).clamp(0.05, 4.0);
            let amp_h = (plot.height() * 0.44).max(24.0);
            let mut x = plot.left();
            while x < plot.right() {
                let sec = sec_of(x);
                if sec >= 0.0 && sec <= duration {
                    let idx = ((sec / duration) * peaks.len() as f64) as usize;
                    if let Some(&(mn, mx)) = peaks.get(idx.min(peaks.len() - 1)) {
                        let y_top = mid_y - (mx.abs() * gain_lin).min(1.0) * amp_h;
                        let y_bot = mid_y + (mn.abs() * gain_lin).min(1.0) * amp_h;
                        gp.line_segment(
                            [pos2(x, y_top), pos2(x, y_bot)],
                            Stroke::new(1.5, Color32::from_rgb(0x51, 0xa8, 0xf0)),
                        );
                    }
                }
                x += 1.5;
            }
        }
    }

    // Region boundaries
    for sec in [clock.offset, clock.offset + region_secs] {
        let x = x_of(sec);
        g.line_segment(
            [pos2(x, plot.top()), pos2(x, plot.bottom())],
            Stroke::new(1.5, ACCENT),
        );
    }

    // Interactive ruler playhead
    if time_resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if time_resp.drag_started() {
        crate::tracks_area::begin_playhead_drag(p);
    }
    if time_resp.clicked() || time_resp.dragged() {
        if let Some(pt) = time_resp.interact_pointer_pos() {
            let beat = clock.beat(sec_of(pt.x)).max(0.0);
            crate::tracks_area::update_playhead_drag(p, beat);
        }
    }
    if time_resp.drag_stopped() {
        crate::tracks_area::end_playhead_drag(p);
    }
    let ph = clock.secs(p.position_beats);
    if ph >= scroll {
        let x = x_of(ph);
        if x >= plot.left() && x <= plot.right() {
            let (ph_line, ph_fill, ph_edge) = crate::tracks_area::playhead_colors(p.recording);
            g.line_segment(
                [pos2(x, time_ruler_rect.top()), pos2(x, plot.bottom())],
                Stroke::new(1.2, ph_line),
            );
            let cap_y = time_ruler_rect.top();
            let cap = vec![
                pos2(x - 6.0, cap_y + 1.0),
                pos2(x + 6.0, cap_y + 1.0),
                pos2(x + 6.0, cap_y + 8.0),
                pos2(x, cap_y + 15.0),
                pos2(x - 6.0, cap_y + 8.0),
            ];
            g.add(egui::Shape::convex_polygon(
                cap,
                ph_fill,
                Stroke::new(1.0, ph_edge),
            ));
        }
    }

    // Status line
    let status = Rect::from_min_max(pos2(canvas.left(), plot.bottom()), canvas.max);
    g.rect_filled(status, 0.0, SURFACE);
    let busy = if s.flex.busy() {
        format!("  ·  {}", tr("rendering…"))
    } else {
        String::new()
    };
    g.text(
        pos2(status.left() + 8.0, status.center().y),
        egui::Align2::LEFT_CENTER,
        format!("Audio Track Editor{busy}"),
        FontId::proportional(11.0),
        DIM,
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::{Arc, OnceLock};

    #[test]
    fn audio_editor_renders_waveform_and_pitchnet_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("vocals.wav");
        let mono: Vec<f32> = (0..24_000)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        prowave_engine::stems::write_wav_stereo(&wav, &mono, &mono, 48_000).unwrap();
        let file = AudioFile::open(&wav).unwrap();
        let peaks = Arc::new(OnceLock::new());
        let _ = peaks.set(file.peaks(128).unwrap());

        let mut p = Project::empty();
        let mut tr = crate::model::Track::new("Vocals", TrackKind::Audio);
        tr.regions.push(crate::model::Region {
            name: "Vocals_01".to_string(),
            start_bar: 1.0,
            len_bars: 2.0,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            generated: None,
            audio: Some(crate::model::AudioRegion::whole(file, peaks)),
            missing: None,
            params: crate::model::RegionParams::default(),
        });
        let ti = p.tracks.len();
        p.tracks.push(tr);
        p.selected_track = ti;

        let mut s = UiState::new();
        s.bottom = Bottom::PitchEditor;
        s.pitch_ed.region = Some((ti, 0));
        s.tracks.select_only(ti, 0);

        let out = std::env::var_os("PROWAVE_UI_PNG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("shot_audio_editor.png");
        crate::ui_test::render_png(1120.0, 360.0, 1.0, BG, |ui| show(ui, &mut p, &mut s), &out)
            .unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 5_000);
    }

    #[test]
    fn audio_editor_panel_resizes_without_snap_back() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("vocals.wav");
        let mono: Vec<f32> = (0..24_000)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        prowave_engine::stems::write_wav_stereo(&wav, &mono, &mono, 48_000).unwrap();
        let file = AudioFile::open(&wav).unwrap();
        let peaks = Arc::new(OnceLock::new());
        let _ = peaks.set(file.peaks(128).unwrap());

        let mut p = Project::empty();
        let mut tr = crate::model::Track::new("Vocals", TrackKind::Audio);
        tr.regions.push(crate::model::Region {
            name: "Vocals_01".to_string(),
            start_bar: 1.0,
            len_bars: 2.0,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            generated: None,
            audio: Some(crate::model::AudioRegion::whole(file, peaks)),
            missing: None,
            params: crate::model::RegionParams::default(),
        });
        p.tracks.push(tr);
        p.selected_track = 0;
        let mut s = UiState::new();
        s.bottom = Bottom::PitchEditor;
        s.pitch_ed.region = Some((0, 0));
        s.tracks.select_only(0, 0);

        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));
        let mut heights = Vec::new();
        for frame in 0..10 {
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            if frame == 1 {
                input
                    .events
                    .push(egui::Event::PointerMoved(pos2(400.0, 442.0)));
                input.events.push(egui::Event::PointerButton {
                    pos: pos2(400.0, 442.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            } else if frame == 2 {
                input
                    .events
                    .push(egui::Event::PointerMoved(pos2(400.0, 302.0)));
            } else if frame == 3 {
                input
                    .events
                    .push(egui::Event::PointerMoved(pos2(400.0, 302.0)));
                input.events.push(egui::Event::PointerButton {
                    pos: pos2(400.0, 302.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            } else if frame == 5 {
                input
                    .events
                    .push(egui::Event::PointerMoved(pos2(400.0, 302.0)));
                input.events.push(egui::Event::PointerButton {
                    pos: pos2(400.0, 302.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            } else if frame == 6 {
                input
                    .events
                    .push(egui::Event::PointerMoved(pos2(400.0, 542.0)));
            } else if frame == 7 {
                input
                    .events
                    .push(egui::Event::PointerMoved(pos2(400.0, 542.0)));
                input.events.push(egui::Event::PointerButton {
                    pos: pos2(400.0, 542.0),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut out = ctx.run_ui(input, |ui| {
                let room = ui.available_height();
                let max_h = (room * 0.85).max(540.0);
                let resp = egui::Panel::bottom("bottom_pitch_editor")
                    .resizable(false)
                    .exact_size(s.pitch_ed.panel_height.clamp(160.0, max_h))
                    .show(ui, |ui| show(ui, &mut p, &mut s));
                heights.push(resp.response.rect.height());
                egui::CentralPanel::default().show(ui, |_ui| {});
            });
            out.textures_delta.clear();
        }
        eprintln!("panel heights: {heights:?}");
        assert!(
            heights[4] > 450.0 && (heights[9] - 260.0).abs() < 5.0,
            "expected [4]>450 and [9]~260, got {heights:?}"
        );
    }

    #[test]
    fn pitchnet_vst3_is_discoverable_and_scannable() {
        if let Some(vst3) = find_pitchnet_vst3() {
            let scanned = prowave_plugin_host::scan_file(prowave_plugin_host::Format::Vst3, &vst3);
            eprintln!("pitchnet vst3: {vst3:?} -> {scanned:?}");
            assert!(scanned.is_ok(), "scan_file failed: {scanned:?}");
            assert!(
                !scanned.unwrap().is_empty(),
                "no plugins found in PitchNet.vst3"
            );
        }
    }
}
