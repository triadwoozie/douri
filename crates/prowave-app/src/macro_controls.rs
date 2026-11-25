//! Logic Pro-inspired Macro Controls bottom panel and floating window (`B` shortcut).
//! Features a 30px top menu bar (`Track`/`Master`, `Compare`, `Controls`/`EQ`, `Tuner`/`Arpeggiator`),
//! a parameter mapping drawer, a full-width draggable 8-band Channel EQ view, and a skinned
//! patch faceplate with grouped section cards and Moog-style `AudioKnobs`.
use crate::i18n::tr;
use crate::model::{Insert, Project, SendSlot, TrackKind};
use crate::moog_knob::{self, MoogKnobConfig, MoogKnobStyle};
use crate::widgets::{BLUE, TEXT, TEXT_DIM};
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui,
};
use prowave_engine::{eq_idx, FxKind};

const BAR_BG: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1e);
const BAR_BORDER: Color32 = Color32::from_rgb(0x2c, 0x2c, 0x2e);
const PILL_ACTIVE: Color32 = Color32::from_rgb(0x12, 0x69, 0xcc);
const PILL_INACTIVE: Color32 = Color32::from_rgb(0x28, 0x28, 0x2b);
fn card_border() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 45)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatchKind {
    Synth,
    Keys,
    Drums,
    AudioOrMaster,
}

#[derive(Clone, Copy)]
struct MacroSpec {
    label: &'static str,
    target_desc: &'static str,
    help_id: &'static str,
    style: MoogKnobStyle,
    min: f32,
    max: f32,
    default: f32,
    log: bool,
    unit: &'static str,
    kind: MacroParamKind,
}

#[derive(Clone, Copy)]
enum MacroParamKind {
    EqLpfFreq,
    EqLpfQ,
    EqLowGain,
    EqMidGain,
    EqHighGain,
    EqAirGain,
    CompAttack,
    CompRelease,
    CompThreshold,
    Drive,
    DelayWet,
    ReverbWet,
    Send1,
    OutputDb,
}

#[derive(Clone, PartialEq)]
struct MacroSnapshot {
    inserts: Vec<Insert>,
    sends: Vec<SendSlot>,
    volume_db: f32,
    master_vol: f32,
}

#[derive(Clone)]
pub struct MacroControlsView {
    pub master_mode: bool,
    pub target_bus: Option<u16>,
    pub eq_tab: bool,
    pub show_mapping: bool,
    pub show_utility: bool,
    pub arp_rate: usize,
    pub arp_pattern: usize,
    pub arp_octaves: u8,
    pub compare_active: bool,
    snapshot_track: Option<(bool, usize)>,
    snapshot: Option<MacroSnapshot>,
    pub midi_cc: [Option<u8>; 8],
    pub learning_knob: Option<usize>,
    drag_eq_band: Option<usize>,
}

impl Default for MacroControlsView {
    fn default() -> Self {
        Self {
            master_mode: false,
            target_bus: None,
            eq_tab: false,
            show_mapping: false,
            show_utility: false,
            arp_rate: 2,
            arp_pattern: 0,
            arp_octaves: 1,
            compare_active: false,
            snapshot_track: None,
            snapshot: None,
            midi_cc: [None; 8],
            learning_knob: None,
            drag_eq_band: None,
        }
    }
}

fn detect_patch_kind(p: &Project, ti: usize, master: bool) -> PatchKind {
    if master {
        return PatchKind::AudioOrMaster;
    }
    let Some(t) = p.tracks.get(ti) else {
        return PatchKind::AudioOrMaster;
    };
    if t.kind == TrackKind::Audio || t.kind == TrackKind::Aux || t.kind == TrackKind::Vca {
        return PatchKind::AudioOrMaster;
    }
    if t.drums.is_some() {
        return PatchKind::Drums;
    }
    let cat = t.category.unwrap_or("").to_ascii_lowercase();
    let name = t.name.to_ascii_lowercase();
    let inst = t.instrument.unwrap_or("").to_ascii_lowercase();
    let combined = format!("{cat} {name} {inst}");
    if combined.contains("drum") || combined.contains("perc") || combined.contains("kit") {
        PatchKind::Drums
    } else if combined.contains("piano")
        || combined.contains("key")
        || combined.contains("organ")
        || combined.contains("string")
        || combined.contains("mallet")
        || t.epiano.is_some()
    {
        PatchKind::Keys
    } else {
        PatchKind::Synth
    }
}

fn patch_layout(kind: PatchKind) -> (&'static str, &'static str, [MacroSpec; 8]) {
    match kind {
        PatchKind::Synth => (
            "TONE & FILTER",
            "ENVELOPE & SPACE",
            [
                MacroSpec {
                    label: "FILTER CUTOFF",
                    target_desc: "Channel EQ LPF Freq",
                    help_id: "sc_cutoff",
                    style: MoogKnobStyle::Scalloped,
                    min: 120.0,
                    max: 20000.0,
                    default: 20000.0,
                    log: true,
                    unit: "Hz",
                    kind: MacroParamKind::EqLpfFreq,
                },
                MacroSpec {
                    label: "RESONANCE",
                    target_desc: "Channel EQ LPF Q",
                    help_id: "sc_res",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.1,
                    max: 10.0,
                    default: 0.71,
                    log: true,
                    unit: "",
                    kind: MacroParamKind::EqLpfQ,
                },
                MacroSpec {
                    label: "DRIVE",
                    target_desc: "Overdrive Gain",
                    help_id: "sc_drive",
                    style: MoogKnobStyle::ChickenHead,
                    min: 0.0,
                    max: 24.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::Drive,
                },
                MacroSpec {
                    label: "SUB / TONE",
                    target_desc: "Low Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqLowGain,
                },
                MacroSpec {
                    label: "ATTACK",
                    target_desc: "Compressor Attack",
                    help_id: "sc_attack",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.1,
                    max: 100.0,
                    default: 10.0,
                    log: true,
                    unit: "ms",
                    kind: MacroParamKind::CompAttack,
                },
                MacroSpec {
                    label: "RELEASE",
                    target_desc: "Compressor Release",
                    help_id: "sc_release",
                    style: MoogKnobStyle::Scalloped,
                    min: 10.0,
                    max: 1000.0,
                    default: 120.0,
                    log: true,
                    unit: "ms",
                    kind: MacroParamKind::CompRelease,
                },
                MacroSpec {
                    label: "REVERB / SEND",
                    target_desc: "Bus 1 Send Level",
                    help_id: "sc_send1",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.0,
                    max: 100.0,
                    default: 0.0,
                    log: false,
                    unit: "%",
                    kind: MacroParamKind::Send1,
                },
                MacroSpec {
                    label: "OUTPUT",
                    target_desc: "Channel Strip Volume",
                    help_id: "sc_output",
                    style: MoogKnobStyle::Scalloped,
                    min: -60.0,
                    max: 6.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::OutputDb,
                },
            ],
        ),
        PatchKind::Keys => (
            "TONE & BODY",
            "SPACE & OUTPUT",
            [
                MacroSpec {
                    label: "BRIGHTNESS",
                    target_desc: "High Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqHighGain,
                },
                MacroSpec {
                    label: "BODY",
                    target_desc: "Low Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqLowGain,
                },
                MacroSpec {
                    label: "DYNAMICS",
                    target_desc: "Compressor Threshold",
                    help_id: "fx_comp",
                    style: MoogKnobStyle::ChickenHead,
                    min: -48.0,
                    max: 0.0,
                    default: -18.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::CompThreshold,
                },
                MacroSpec {
                    label: "AIR",
                    target_desc: "Upper-Mid Air Band",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqAirGain,
                },
                MacroSpec {
                    label: "ATTACK",
                    target_desc: "Compressor Attack",
                    help_id: "sc_attack",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.1,
                    max: 100.0,
                    default: 10.0,
                    log: true,
                    unit: "ms",
                    kind: MacroParamKind::CompAttack,
                },
                MacroSpec {
                    label: "RELEASE",
                    target_desc: "Compressor Release",
                    help_id: "sc_release",
                    style: MoogKnobStyle::Scalloped,
                    min: 10.0,
                    max: 1000.0,
                    default: 120.0,
                    log: true,
                    unit: "ms",
                    kind: MacroParamKind::CompRelease,
                },
                MacroSpec {
                    label: "AMBIENCE",
                    target_desc: "Bus 1 Send Level",
                    help_id: "sc_send1",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.0,
                    max: 100.0,
                    default: 0.0,
                    log: false,
                    unit: "%",
                    kind: MacroParamKind::Send1,
                },
                MacroSpec {
                    label: "OUTPUT",
                    target_desc: "Channel Strip Volume",
                    help_id: "sc_output",
                    style: MoogKnobStyle::Scalloped,
                    min: -60.0,
                    max: 6.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::OutputDb,
                },
            ],
        ),
        PatchKind::Drums => (
            "PUNCH & TONE",
            "ROOM & OUTPUT",
            [
                MacroSpec {
                    label: "PUNCH",
                    target_desc: "Compressor Threshold",
                    help_id: "fx_comp",
                    style: MoogKnobStyle::ChickenHead,
                    min: -48.0,
                    max: 0.0,
                    default: -18.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::CompThreshold,
                },
                MacroSpec {
                    label: "LOW",
                    target_desc: "Low Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqLowGain,
                },
                MacroSpec {
                    label: "HIGH",
                    target_desc: "High Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqHighGain,
                },
                MacroSpec {
                    label: "DRIVE",
                    target_desc: "Overdrive Gain",
                    help_id: "sc_drive",
                    style: MoogKnobStyle::ChickenHead,
                    min: 0.0,
                    max: 24.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::Drive,
                },
                MacroSpec {
                    label: "ATTACK",
                    target_desc: "Compressor Attack",
                    help_id: "sc_attack",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.1,
                    max: 100.0,
                    default: 10.0,
                    log: true,
                    unit: "ms",
                    kind: MacroParamKind::CompAttack,
                },
                MacroSpec {
                    label: "DECAY",
                    target_desc: "Compressor Release",
                    help_id: "sc_release",
                    style: MoogKnobStyle::Scalloped,
                    min: 10.0,
                    max: 1000.0,
                    default: 120.0,
                    log: true,
                    unit: "ms",
                    kind: MacroParamKind::CompRelease,
                },
                MacroSpec {
                    label: "ROOM SEND",
                    target_desc: "Bus 1 Send Level",
                    help_id: "sc_send1",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.0,
                    max: 100.0,
                    default: 0.0,
                    log: false,
                    unit: "%",
                    kind: MacroParamKind::Send1,
                },
                MacroSpec {
                    label: "OUTPUT",
                    target_desc: "Channel Strip Volume",
                    help_id: "sc_output",
                    style: MoogKnobStyle::Scalloped,
                    min: -60.0,
                    max: 6.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::OutputDb,
                },
            ],
        ),
        PatchKind::AudioOrMaster => (
            "EQ & DYNAMICS",
            "SPACE & OUTPUT",
            [
                MacroSpec {
                    label: "LOW",
                    target_desc: "Low Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqLowGain,
                },
                MacroSpec {
                    label: "MID",
                    target_desc: "Mid Bell Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqMidGain,
                },
                MacroSpec {
                    label: "HIGH",
                    target_desc: "High Shelf Gain",
                    help_id: "sc_tone",
                    style: MoogKnobStyle::Scalloped,
                    min: -12.0,
                    max: 12.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::EqHighGain,
                },
                MacroSpec {
                    label: "COMPRESS",
                    target_desc: "Compressor Threshold",
                    help_id: "fx_comp",
                    style: MoogKnobStyle::ChickenHead,
                    min: -48.0,
                    max: 0.0,
                    default: -18.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::CompThreshold,
                },
                MacroSpec {
                    label: "DRIVE / LIMIT",
                    target_desc: "Overdrive Gain",
                    help_id: "sc_drive",
                    style: MoogKnobStyle::ChickenHead,
                    min: 0.0,
                    max: 24.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::Drive,
                },
                MacroSpec {
                    label: "DELAY",
                    target_desc: "Tape Delay Mix",
                    help_id: "cs_fx",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.0,
                    max: 100.0,
                    default: 0.0,
                    log: false,
                    unit: "%",
                    kind: MacroParamKind::DelayWet,
                },
                MacroSpec {
                    label: "REVERB",
                    target_desc: "Space D Wet",
                    help_id: "sc_send1",
                    style: MoogKnobStyle::Scalloped,
                    min: 0.0,
                    max: 100.0,
                    default: 0.0,
                    log: false,
                    unit: "%",
                    kind: MacroParamKind::ReverbWet,
                },
                MacroSpec {
                    label: "OUTPUT",
                    target_desc: "Output Volume",
                    help_id: "sc_output",
                    style: MoogKnobStyle::Scalloped,
                    min: -60.0,
                    max: 6.0,
                    default: 0.0,
                    log: false,
                    unit: "dB",
                    kind: MacroParamKind::OutputDb,
                },
            ],
        ),
    }
}

fn target_inserts(p: &Project, ti: usize, master: bool) -> &[Insert] {
    if master {
        &p.master_inserts
    } else {
        p.tracks
            .get(ti)
            .map(|t| t.inserts.as_slice())
            .unwrap_or(&[])
    }
}

fn ensure_target_insert(p: &mut Project, ti: usize, master: bool, kind: FxKind) -> Option<usize> {
    let inserts = if master {
        &mut p.master_inserts
    } else {
        &mut p.tracks.get_mut(ti)?.inserts
    };
    if let Some(pos) = inserts.iter().position(|x| x.kind == kind) {
        return Some(pos);
    }
    if inserts.len() < prowave_engine::MAX_INSERTS {
        inserts.push(Insert::new(kind));
        return Some(inserts.len() - 1);
    }
    None
}

fn find_insert(inserts: &[Insert], kind: FxKind) -> Option<usize> {
    inserts.iter().position(|x| x.kind == kind)
}

fn read_param(p: &Project, ti: usize, master: bool, kind: MacroParamKind) -> f32 {
    let ins = target_inserts(p, ti, master);
    match kind {
        MacroParamKind::EqLpfFreq => find_insert(ins, FxKind::ChannelEq)
            .filter(|&i| ins[i].params[eq_idx::LPF_ON] >= 0.5)
            .map(|i| ins[i].params[eq_idx::LPF_FREQ])
            .unwrap_or(20000.0),
        MacroParamKind::EqLpfQ => find_insert(ins, FxKind::ChannelEq)
            .map(|i| ins[i].params[eq_idx::LPF_Q])
            .unwrap_or(0.71),
        MacroParamKind::EqLowGain => find_insert(ins, FxKind::ChannelEq)
            .map(|i| ins[i].params[eq_idx::LOW_SHELF_GAIN])
            .unwrap_or(0.0),
        MacroParamKind::EqMidGain => find_insert(ins, FxKind::ChannelEq)
            .map(|i| ins[i].params[eq_idx::MID_GAIN])
            .unwrap_or(0.0),
        MacroParamKind::EqHighGain => find_insert(ins, FxKind::ChannelEq)
            .map(|i| ins[i].params[eq_idx::HIGH_SHELF_GAIN])
            .unwrap_or(0.0),
        MacroParamKind::EqAirGain => find_insert(ins, FxKind::ChannelEq)
            .map(|i| ins[i].params[eq_idx::UPPER_MID_GAIN])
            .unwrap_or(0.0),
        MacroParamKind::CompAttack => find_insert(ins, FxKind::Compressor)
            .map(|i| ins[i].params[2])
            .unwrap_or(10.0),
        MacroParamKind::CompRelease => find_insert(ins, FxKind::Compressor)
            .map(|i| ins[i].params[3])
            .unwrap_or(120.0),
        MacroParamKind::CompThreshold => find_insert(ins, FxKind::Compressor)
            .map(|i| ins[i].params[0])
            .unwrap_or(-18.0),
        MacroParamKind::Drive => find_insert(ins, FxKind::Overdrive)
            .filter(|&i| !ins[i].bypass)
            .map(|i| ins[i].params[0])
            .unwrap_or(0.0),
        MacroParamKind::DelayWet => find_insert(ins, FxKind::Delay)
            .filter(|&i| !ins[i].bypass)
            .map(|i| ins[i].params[3] * 100.0)
            .unwrap_or(0.0),
        MacroParamKind::ReverbWet => {
            if !master {
                if let Some(s) = p.tracks.get(ti).and_then(|t| t.sends.first()) {
                    return s.level * 100.0;
                }
            }
            find_insert(ins, FxKind::Reverb)
                .filter(|&i| !ins[i].bypass)
                .map(|i| ((ins[i].params[7] + 96.0) / 102.0 * 100.0).clamp(0.0, 100.0))
                .unwrap_or(0.0)
        }
        MacroParamKind::Send1 => p
            .tracks
            .get(ti)
            .and_then(|t| t.sends.first())
            .map(|s| s.level * 100.0)
            .unwrap_or(0.0),
        MacroParamKind::OutputDb => {
            if master {
                (p.master_vol * 66.0 - 60.0).clamp(-60.0, 6.0)
            } else {
                p.tracks.get(ti).map(|t| t.volume_db).unwrap_or(0.0)
            }
        }
    }
}

fn write_param(p: &mut Project, ti: usize, master: bool, kind: MacroParamKind, val: f32) {
    match kind {
        MacroParamKind::EqLpfFreq => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[eq_idx::LPF_FREQ] = val.clamp(20.0, 20000.0);
                ins.params[eq_idx::LPF_ON] = if val < 19500.0 { 1.0 } else { 0.0 };
                ins.bypass = false;
            }
        }
        MacroParamKind::EqLpfQ => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[eq_idx::LPF_Q] = val.clamp(0.1, 10.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::EqLowGain => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[eq_idx::LOW_SHELF_ON] = 1.0;
                ins.params[eq_idx::LOW_SHELF_GAIN] = val.clamp(-24.0, 24.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::EqMidGain => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[eq_idx::MID_ON] = 1.0;
                ins.params[eq_idx::MID_GAIN] = val.clamp(-24.0, 24.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::EqHighGain => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[eq_idx::HIGH_SHELF_ON] = 1.0;
                ins.params[eq_idx::HIGH_SHELF_GAIN] = val.clamp(-24.0, 24.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::EqAirGain => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[eq_idx::UPPER_MID_ON] = 1.0;
                ins.params[eq_idx::UPPER_MID_GAIN] = val.clamp(-24.0, 24.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::CompAttack => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::Compressor) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[2] = val.clamp(0.1, 100.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::CompRelease => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::Compressor) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[3] = val.clamp(10.0, 1000.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::CompThreshold => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::Compressor) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[0] = val.clamp(-60.0, 0.0);
                ins.bypass = false;
            }
        }
        MacroParamKind::Drive => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::Overdrive) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[0] = val.clamp(0.0, 40.0);
                ins.bypass = val <= 0.01;
            }
        }
        MacroParamKind::DelayWet => {
            if let Some(i) = ensure_target_insert(p, ti, master, FxKind::Delay) {
                let ins = if master {
                    &mut p.master_inserts[i]
                } else {
                    &mut p.tracks[ti].inserts[i]
                };
                ins.params[3] = (val / 100.0).clamp(0.0, 1.0);
                ins.bypass = val <= 0.1;
            }
        }
        MacroParamKind::ReverbWet => {
            if !master {
                write_param(p, ti, false, MacroParamKind::Send1, val);
                return;
            }
            if let Some(i) = ensure_target_insert(p, ti, true, FxKind::Reverb) {
                let ins = &mut p.master_inserts[i];
                ins.params[7] = ((val / 100.0) * 102.0 - 96.0).clamp(-96.0, 6.0);
                ins.bypass = val <= 0.1;
            }
        }
        MacroParamKind::Send1 => {
            if let Some(t) = p.tracks.get_mut(ti) {
                let level = (val / 100.0).clamp(0.0, 1.0);
                if t.sends.is_empty() {
                    if level > 0.001 {
                        t.sends.push(SendSlot {
                            bus: Some(1),
                            level,
                            pre: false,
                        });
                    }
                } else {
                    if t.sends[0].bus.is_none() && level > 0.001 {
                        t.sends[0].bus = Some(1);
                    }
                    t.sends[0].level = level;
                }
            }
        }
        MacroParamKind::OutputDb => {
            if master {
                p.master_vol = ((val + 60.0) / 66.0).clamp(0.0, 1.0);
            } else if let Some(t) = p.tracks.get_mut(ti) {
                t.volume_db = val.clamp(-60.0, 6.0);
            }
        }
    }
}

fn capture_snapshot(p: &Project, ti: usize, master: bool) -> MacroSnapshot {
    MacroSnapshot {
        inserts: target_inserts(p, ti, master).to_vec(),
        sends: if master {
            Vec::new()
        } else {
            p.tracks
                .get(ti)
                .map(|t| t.sends.clone())
                .unwrap_or_default()
        },
        volume_db: p.tracks.get(ti).map(|t| t.volume_db).unwrap_or(0.0),
        master_vol: p.master_vol,
    }
}

fn swap_snapshot(p: &mut Project, ti: usize, master: bool, snap: &mut MacroSnapshot) {
    let cur = capture_snapshot(p, ti, master);
    if master {
        p.master_inserts = snap.inserts.clone();
        p.master_vol = snap.master_vol;
    } else if let Some(t) = p.tracks.get_mut(ti) {
        t.inserts = snap.inserts.clone();
        t.sends = snap.sends.clone();
        t.volume_db = snap.volume_db;
    }
    *snap = cur;
}

/// Renders the Macro Controls UI inside `ui` (used both by the bottom pane and the floating window).
pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState, floating: bool) {
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());
    let g = ui.painter_at(area);
    g.rect_filled(area, 0.0, BAR_BG);

    let ti = if let Some(bus) = s.macro_controls.target_bus {
        p.tracks
            .iter()
            .position(|t| t.kind == TrackKind::Aux && t.aux_bus == bus)
            .unwrap_or_else(|| p.selected_track.min(p.tracks.len().saturating_sub(1)))
    } else {
        p.selected_track.min(p.tracks.len().saturating_sub(1))
    };
    let master = s.macro_controls.master_mode;

    // Ensure Compare snapshot tracks the current target
    let target_key = (master, ti);
    if s.macro_controls.snapshot_track != Some(target_key) {
        s.macro_controls.snapshot_track = Some(target_key);
        s.macro_controls.snapshot = Some(capture_snapshot(p, ti, master));
        s.macro_controls.compare_active = false;
    }

    // 1. Dedicated 30px Top Menu Bar
    let bar_h = 30.0;
    let bar = Rect::from_min_size(area.min, vec2(area.width(), bar_h));
    g.rect_filled(bar, 0.0, BAR_BG);
    g.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, BAR_BORDER),
    );

    let mut bar_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar.shrink2(vec2(8.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar_ui.spacing_mut().item_spacing.x = 6.0;

    // Left Group: Mapping Drawer + Track|Master + Target Selector + Compare
    let map_btn = bar_ui.add(
        egui::Button::new(RichText::new("Mapping").size(11.0).color(TEXT))
            .fill(if s.macro_controls.show_mapping {
                PILL_ACTIVE
            } else {
                PILL_INACTIVE
            })
            .corner_radius(CornerRadius::same(4)),
    );
    if map_btn.clicked() {
        s.macro_controls.show_mapping = !s.macro_controls.show_mapping;
    }

    // Track | Master segmented pill
    let track_btn = bar_ui.add(
        egui::Button::new(RichText::new(tr("Track")).size(11.0).color(TEXT))
            .fill(if !master { PILL_ACTIVE } else { PILL_INACTIVE })
            .corner_radius(CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0,
            }),
    );
    let master_btn = bar_ui.add(
        egui::Button::new(RichText::new(tr("Master")).size(11.0).color(TEXT))
            .fill(if master { PILL_ACTIVE } else { PILL_INACTIVE })
            .corner_radius(CornerRadius {
                nw: 0,
                sw: 0,
                ne: 4,
                se: 4,
            }),
    );
    if track_btn.clicked() {
        s.macro_controls.master_mode = false;
        s.macro_controls.target_bus = None;
    }
    if master_btn.clicked() {
        s.macro_controls.master_mode = true;
        s.macro_controls.target_bus = None;
    }

    // Target selector dropdown
    let target_name = if master {
        tr("Stereo Out").to_string()
    } else {
        p.tracks
            .get(ti)
            .map(|t| t.name.clone())
            .unwrap_or_else(|| tr("Track").to_string())
    };
    egui::ComboBox::from_id_salt("macro_target_sel")
        .selected_text(RichText::new(target_name).size(11.0))
        .width(120.0)
        .show_ui(&mut bar_ui, |ui| {
            let sel_name = p
                .tracks
                .get(p.selected_track)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| tr("Track").to_string());
            if ui
                .selectable_label(
                    !s.macro_controls.master_mode && s.macro_controls.target_bus.is_none(),
                    sel_name,
                )
                .clicked()
            {
                s.macro_controls.master_mode = false;
                s.macro_controls.target_bus = None;
            }
            if ui
                .selectable_label(s.macro_controls.master_mode, tr("Stereo Out"))
                .clicked()
            {
                s.macro_controls.master_mode = true;
                s.macro_controls.target_bus = None;
            }
            for t in &p.tracks {
                if t.kind == TrackKind::Aux && t.aux_bus > 0 {
                    if ui
                        .selectable_label(
                            !s.macro_controls.master_mode
                                && s.macro_controls.target_bus == Some(t.aux_bus),
                            format!("{} (Bus {})", t.name, t.aux_bus),
                        )
                        .clicked()
                    {
                        s.macro_controls.master_mode = false;
                        s.macro_controls.target_bus = Some(t.aux_bus);
                    }
                }
            }
        });

    // Compare toggle
    let cmp_btn = bar_ui.add(
        egui::Button::new(RichText::new(tr("Compare")).size(11.0).color(TEXT))
            .fill(if s.macro_controls.compare_active {
                PILL_ACTIVE
            } else {
                PILL_INACTIVE
            })
            .corner_radius(CornerRadius::same(4)),
    );
    if cmp_btn.clicked() {
        if let Some(snap) = s.macro_controls.snapshot.as_mut() {
            swap_snapshot(p, ti, master, snap);
            s.macro_controls.compare_active = !s.macro_controls.compare_active;
        }
    }

    // Center Group: Controls | EQ
    bar_ui.add_space((bar_ui.available_width() * 0.15).max(12.0));
    let ctrl_btn = bar_ui.add(
        egui::Button::new(RichText::new(tr("Controls")).size(11.0).color(TEXT))
            .fill(if !s.macro_controls.eq_tab {
                PILL_ACTIVE
            } else {
                PILL_INACTIVE
            })
            .corner_radius(CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0,
            }),
    );
    let eq_btn = bar_ui.add(
        egui::Button::new(RichText::new("EQ").size(11.0).color(TEXT))
            .fill(if s.macro_controls.eq_tab {
                PILL_ACTIVE
            } else {
                PILL_INACTIVE
            })
            .corner_radius(CornerRadius {
                nw: 0,
                sw: 0,
                ne: 4,
                se: 4,
            }),
    );
    if ctrl_btn.clicked() {
        s.macro_controls.eq_tab = false;
    }
    if eq_btn.clicked() {
        s.macro_controls.eq_tab = true;
    }

    // Right Group: Context utility (Tuner on Audio, Arpeggiator on Instrument/MIDI) + Pop-out
    let is_audio = !master && p.tracks.get(ti).is_some_and(|t| t.kind == TrackKind::Audio);
    bar_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if !floating {
            let pop_btn = ui.add(
                egui::Button::new(RichText::new("Open Window").size(10.5).color(TEXT))
                    .fill(PILL_INACTIVE)
                    .corner_radius(CornerRadius::same(4)),
            );
            if pop_btn.clicked() {
                s.show_macro_window = !s.show_macro_window;
            }
        }
        let util_label = if is_audio {
            tr("Tuner")
        } else {
            tr("Arpeggiator")
        };
        let util_btn = ui.add(
            egui::Button::new(RichText::new(util_label).size(11.0).color(TEXT))
                .fill(if s.macro_controls.show_utility {
                    PILL_ACTIVE
                } else {
                    PILL_INACTIVE
                })
                .corner_radius(CornerRadius::same(4)),
        );
        if util_btn.clicked() {
            s.macro_controls.show_utility = !s.macro_controls.show_utility;
        }
    });

    // Main body below the 30px top bar
    let mut body = Rect::from_min_max(pos2(area.left(), bar.bottom()), area.max);
    if body.width() < 160.0 || body.height() < 60.0 {
        return;
    }

    let patch_kind = detect_patch_kind(p, ti, master);
    let (card1_title, card2_title, specs) = patch_layout(patch_kind);

    // Optional Left Parameter Mapping Drawer
    if s.macro_controls.show_mapping {
        let drawer_w = 210.0_f32.min(body.width() * 0.32);
        let drawer = Rect::from_min_max(body.min, pos2(body.left() + drawer_w, body.max.y));
        body.min.x = drawer.right();
        g.rect_filled(drawer, 0.0, Color32::from_rgb(0x18, 0x19, 0x1c));
        g.line_segment(
            [drawer.right_top(), drawer.right_bottom()],
            Stroke::new(1.0, BAR_BORDER),
        );
        let mut d_ui = ui.new_child(egui::UiBuilder::new().max_rect(drawer.shrink(8.0)));
        d_ui.label(
            RichText::new(tr("Parameter Mapping"))
                .strong()
                .size(11.5)
                .color(TEXT),
        );
        d_ui.separator();
        egui::ScrollArea::vertical().show(&mut d_ui, |ui| {
            for (idx, sp) in specs.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{}. {}", idx + 1, sp.label))
                            .strong()
                            .size(10.5)
                            .color(TEXT),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "{} ({} .. {})",
                        sp.target_desc,
                        moog_knob::format_value(sp.unit, sp.min),
                        moog_knob::format_value(sp.unit, sp.max)
                    ))
                    .size(10.0)
                    .color(TEXT_DIM),
                );
                ui.add_space(3.0);
            }
        });
    }

    // Optional Right Utility Strip (Arpeggiator or Tuner)
    if s.macro_controls.show_utility {
        let util_h = 30.0;
        let util_rect = Rect::from_min_size(body.min, vec2(body.width(), util_h));
        body.min.y = util_rect.bottom();
        g.rect_filled(util_rect, 0.0, Color32::from_rgb(0x20, 0x22, 0x27));
        g.line_segment(
            [util_rect.left_bottom(), util_rect.right_bottom()],
            Stroke::new(1.0, BAR_BORDER),
        );
        let mut u_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(util_rect.shrink2(vec2(12.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if is_audio {
            u_ui.label(
                RichText::new(format!("{}  -  A4 = {:.1} Hz", tr("Tuner"), p.tuning_hz))
                    .size(11.5)
                    .color(TEXT),
            );
        } else {
            u_ui.label(
                RichText::new(tr("Arpeggiator"))
                    .strong()
                    .size(11.5)
                    .color(TEXT),
            );
            u_ui.separator();
            let rates = ["1/4", "1/8", "1/16", "1/32", "1/8T"];
            u_ui.label(RichText::new(tr("Rate:")).size(11.0).color(TEXT_DIM));
            for (ri, r) in rates.iter().enumerate() {
                if u_ui
                    .selectable_label(s.macro_controls.arp_rate == ri, *r)
                    .clicked()
                {
                    s.macro_controls.arp_rate = ri;
                }
            }
            u_ui.separator();
            let patterns = ["Up", "Down", "Up/Down", "Random"];
            u_ui.label(RichText::new(tr("Pattern:")).size(11.0).color(TEXT_DIM));
            for (pi, pat) in patterns.iter().enumerate() {
                if u_ui
                    .selectable_label(s.macro_controls.arp_pattern == pi, *pat)
                    .clicked()
                {
                    s.macro_controls.arp_pattern = pi;
                }
            }
            u_ui.separator();
            u_ui.label(RichText::new(tr("Octaves:")).size(11.0).color(TEXT_DIM));
            for oct in 1..=4_u8 {
                if u_ui
                    .selectable_label(s.macro_controls.arp_octaves == oct, format!("{oct}"))
                    .clicked()
                {
                    s.macro_controls.arp_octaves = oct;
                }
            }
        }
    }

    // EQ View vs Skinned Patch Faceplate
    if s.macro_controls.eq_tab {
        draw_inline_eq(
            ui,
            &g,
            body.shrink(10.0),
            p,
            ti,
            master,
            &mut s.macro_controls.drag_eq_band,
        );
        return;
    }

    // Skinned Patch Faceplate vertical gradient
    let (top_col, bot_col) = if master {
        (
            Color32::from_rgb(0x23, 0x29, 0x3a),
            Color32::from_rgb(0x18, 0x1c, 0x26),
        )
    } else if patch_kind == PatchKind::Drums || patch_kind == PatchKind::AudioOrMaster {
        (
            Color32::from_rgb(0x26, 0x29, 0x2e),
            Color32::from_rgb(0x1b, 0x1d, 0x21),
        )
    } else {
        (
            Color32::from_rgb(0x25, 0x3c, 0x54),
            Color32::from_rgb(0x1a, 0x2b, 0x3c),
        )
    };
    paint_vertical_gradient(&g, body, top_col, bot_col);

    // 2 grouped section cards side-by-side
    let inner = body.shrink(12.0);
    let gap = 12.0;
    let card_w = ((inner.width() - gap) * 0.5).max(80.0);
    let card1 = Rect::from_min_size(inner.min, vec2(card_w, inner.height()));
    let card2 = Rect::from_min_size(
        pos2(card1.right() + gap, inner.top()),
        vec2(card_w, inner.height()),
    );

    for (card_idx, (card_rect, title, slice)) in [
        (card1, card1_title, &specs[0..4]),
        (card2, card2_title, &specs[4..8]),
    ]
    .into_iter()
    .enumerate()
    {
        g.rect_filled(
            card_rect,
            CornerRadius::same(8),
            Color32::from_rgba_unmultiplied(12, 14, 18, 85),
        );
        g.rect_stroke(
            card_rect,
            CornerRadius::same(8),
            Stroke::new(1.0, card_border()),
            StrokeKind::Inside,
        );
        g.text(
            pos2(card_rect.left() + 12.0, card_rect.top() + 12.0),
            Align2::LEFT_CENTER,
            title,
            FontId::proportional(10.5),
            Color32::from_rgb(0xb8, 0xc4, 0xd4),
        );

        let grid = Rect::from_min_max(
            pos2(card_rect.left() + 8.0, card_rect.top() + 24.0),
            pos2(card_rect.right() - 8.0, card_rect.bottom() - 6.0),
        );
        let two_rows = grid.height() >= 150.0 && grid.width() < 460.0;
        let cols = if two_rows { 2 } else { 4 };
        let rows = if two_rows { 2 } else { 1 };
        let cw = grid.width() / cols as f32;
        let ch = grid.height() / rows as f32;
        let diameter = (ch * 0.52).clamp(42.0, 56.0);

        for (local_i, spec) in slice.iter().enumerate() {
            let global_i = card_idx * 4 + local_i;
            let col = local_i % cols;
            let row = local_i / cols;
            let cell = Rect::from_min_size(
                pos2(grid.left() + col as f32 * cw, grid.top() + row as f32 * ch),
                vec2(cw, ch),
            );
            let cur_val = read_param(p, ti, master, spec.kind);
            let res = moog_knob::draw(
                ui,
                &g,
                cell,
                MoogKnobConfig {
                    id: ui.scope_id().with(("macro_knob", global_i)),
                    label: spec.label,
                    help_id: spec.help_id,
                    style: spec.style,
                    min: spec.min,
                    max: spec.max,
                    default: spec.default,
                    log: spec.log,
                    unit: spec.unit,
                    diameter,
                },
                cur_val,
                s.macro_controls.midi_cc[global_i],
            );
            if res.changed {
                write_param(p, ti, master, spec.kind, res.value);
            }
            if res.midi_learn_clicked {
                if s.macro_controls.midi_cc[global_i].is_some() {
                    s.macro_controls.midi_cc[global_i] = None;
                } else {
                    s.macro_controls.midi_cc[global_i] = Some((14 + global_i) as u8);
                }
            }
        }
    }
}

fn paint_vertical_gradient(g: &egui::Painter, rect: Rect, top: Color32, bot: Color32) {
    let steps = 16;
    let h = rect.height() / steps as f32;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let col = top.lerp_to_gamma(bot, t);
        let r = Rect::from_min_size(
            pos2(rect.left(), rect.top() + i as f32 * h),
            vec2(rect.width(), h + 0.5),
        );
        g.rect_filled(r, 0.0, col);
    }
}

fn draw_inline_eq(
    ui: &mut Ui,
    g: &egui::Painter,
    rect: Rect,
    p: &mut Project,
    ti: usize,
    master: bool,
    drag_band: &mut Option<usize>,
) {
    g.rect_filled(
        rect,
        CornerRadius::same(8),
        Color32::from_rgb(0x16, 0x17, 0x1b),
    );
    g.rect_stroke(
        rect,
        CornerRadius::same(8),
        Stroke::new(1.0, BAR_BORDER),
        StrokeKind::Inside,
    );

    let plot = rect.shrink2(vec2(26.0, 22.0));
    if plot.width() < 40.0 || plot.height() < 30.0 {
        return;
    }

    // Frequency gridlines (50, 100, 200, 500, 1k, 2k, 5k, 10k)
    for &f in &[
        50.0_f32, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0,
    ] {
        let frac = (f / 20.0).ln() / (1000.0_f32).ln();
        let x = plot.left() + frac * plot.width();
        g.line_segment(
            [pos2(x, plot.top()), pos2(x, plot.bottom())],
            Stroke::new(1.0, Color32::from_rgb(0x23, 0x24, 0x2a)),
        );
        let lbl = if f >= 1000.0 {
            format!("{}k", (f / 1000.0) as i32)
        } else {
            format!("{}", f as i32)
        };
        g.text(
            pos2(x, plot.bottom() + 9.0),
            Align2::CENTER_CENTER,
            lbl,
            FontId::proportional(9.5),
            TEXT_DIM,
        );
    }

    // 0 dB, +12 dB, -12 dB lines
    let zero_y = plot.center().y;
    for &(db, col) in &[
        (0.0_f32, Color32::from_rgb(0x36, 0x38, 0x40)),
        (12.0, Color32::from_rgb(0x23, 0x24, 0x2a)),
        (-12.0, Color32::from_rgb(0x23, 0x24, 0x2a)),
    ] {
        let y = zero_y - (db / 18.0) * (plot.height() * 0.45);
        g.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(1.0, col),
        );
    }

    // 6 draggable EQ bands (Low Shelf, Low-Mid, Mid, High-Mid, Upper-Mid, High Shelf)
    const BANDS: [(usize, usize, usize, usize, Color32); 6] = [
        (
            eq_idx::LOW_SHELF_ON,
            eq_idx::LOW_SHELF_FREQ,
            eq_idx::LOW_SHELF_GAIN,
            eq_idx::LOW_SHELF_Q,
            Color32::from_rgb(0xe8, 0x5c, 0x4a),
        ),
        (
            eq_idx::LOW_MID_ON,
            eq_idx::LOW_MID_FREQ,
            eq_idx::LOW_MID_GAIN,
            eq_idx::LOW_MID_Q,
            Color32::from_rgb(0xeb, 0xa2, 0x3b),
        ),
        (
            eq_idx::MID_ON,
            eq_idx::MID_FREQ,
            eq_idx::MID_GAIN,
            eq_idx::MID_Q,
            Color32::from_rgb(0x5b, 0xba, 0x6f),
        ),
        (
            eq_idx::HIGH_MID_ON,
            eq_idx::HIGH_MID_FREQ,
            eq_idx::HIGH_MID_GAIN,
            eq_idx::HIGH_MID_Q,
            Color32::from_rgb(0x3e, 0x9b, 0xff),
        ),
        (
            eq_idx::UPPER_MID_ON,
            eq_idx::UPPER_MID_FREQ,
            eq_idx::UPPER_MID_GAIN,
            eq_idx::UPPER_MID_Q,
            Color32::from_rgb(0x7c, 0x76, 0xf2),
        ),
        (
            eq_idx::HIGH_SHELF_ON,
            eq_idx::HIGH_SHELF_FREQ,
            eq_idx::HIGH_SHELF_GAIN,
            eq_idx::HIGH_SHELF_Q,
            Color32::from_rgb(0xc6, 0x6b, 0xe8),
        ),
    ];

    let resp = ui.interact(
        plot,
        ui.scope_id().with("macro_inline_eq"),
        Sense::click_and_drag(),
    );
    let freq_to_x = |f: f32| -> f32 {
        let frac = ((f.clamp(20.0, 20000.0)) / 20.0).ln() / (1000.0_f32).ln();
        plot.left() + frac * plot.width()
    };
    let gain_to_y =
        |db: f32| -> f32 { zero_y - (db.clamp(-18.0, 18.0) / 18.0) * (plot.height() * 0.45) };

    let cur_params = find_insert(target_inserts(p, ti, master), FxKind::ChannelEq)
        .map(|i| target_inserts(p, ti, master)[i].params)
        .unwrap_or_else(|| FxKind::ChannelEq.defaults());

    if resp.drag_started() {
        if let Some(pt) = resp.interact_pointer_pos() {
            *drag_band = BANDS
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let pa = pos2(freq_to_x(cur_params[a.1]), gain_to_y(cur_params[a.2]));
                    let pb = pos2(freq_to_x(cur_params[b.1]), gain_to_y(cur_params[b.2]));
                    pa.distance_sq(pt).total_cmp(&pb.distance_sq(pt))
                })
                .map(|(i, _)| i);
        }
    }
    if resp.dragged() {
        if let (Some(bi), Some(pt)) = (*drag_band, resp.interact_pointer_pos()) {
            if let Some(slot) = ensure_target_insert(p, ti, master, FxKind::ChannelEq) {
                let ins = if master {
                    &mut p.master_inserts[slot]
                } else {
                    &mut p.tracks[ti].inserts[slot]
                };
                ins.bypass = false;
                let (on_i, f_i, g_i, _, _) = BANDS[bi];
                ins.params[on_i] = 1.0;
                let frac = ((pt.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
                ins.params[f_i] = 20.0 * (1000.0_f32).powf(frac);
                let db = ((zero_y - pt.y) / (plot.height() * 0.45)) * 18.0;
                ins.params[g_i] = db.clamp(-18.0, 18.0);
            }
        }
    }
    if resp.drag_stopped() {
        *drag_band = None;
    }

    let prm = find_insert(target_inserts(p, ti, master), FxKind::ChannelEq)
        .map(|i| target_inserts(p, ti, master)[i].params)
        .unwrap_or_else(|| FxKind::ChannelEq.defaults());

    // Curve
    let samples = 96;
    let mut pts = Vec::with_capacity(samples + 1);
    for i in 0..=samples {
        let frac = i as f32 / samples as f32;
        let freq = 20.0 * (1000.0_f32).powf(frac);
        let mut db = 0.0_f32;
        if prm[eq_idx::LOW_SHELF_ON] >= 0.5 {
            let r = freq / prm[eq_idx::LOW_SHELF_FREQ].max(1.0);
            db += prm[eq_idx::LOW_SHELF_GAIN] / (1.0 + r * r);
        }
        if prm[eq_idx::HIGH_SHELF_ON] >= 0.5 {
            let r = prm[eq_idx::HIGH_SHELF_FREQ].max(1.0) / freq;
            db += prm[eq_idx::HIGH_SHELF_GAIN] / (1.0 + r * r);
        }
        for &(on_i, f_i, g_i, q_i, _) in &BANDS[1..5] {
            if prm[on_i] >= 0.5 {
                let d = (freq / prm[f_i].max(1.0)).ln() * prm[q_i].max(0.1) * 2.0;
                db += prm[g_i] * (-d * d).exp();
            }
        }
        pts.push(pos2(
            plot.left() + frac * plot.width(),
            gain_to_y(db).clamp(plot.top(), plot.bottom()),
        ));
    }
    g.add(Shape::line(pts, Stroke::new(2.0, BLUE)));

    // Band handles
    for (bi, &(_, f_i, g_i, _, col)) in BANDS.iter().enumerate() {
        let p_handle = pos2(freq_to_x(prm[f_i]), gain_to_y(prm[g_i]));
        let active = *drag_band == Some(bi);
        g.circle_filled(p_handle, if active { 7.0 } else { 5.5 }, col);
        g.circle_stroke(
            p_handle,
            if active { 7.0 } else { 5.5 },
            Stroke::new(1.2, Color32::WHITE),
        );
    }
}

/// Floating pop-out Macro Controls window (`Window > Open Macro Controls`).
pub fn show_window(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    if !s.show_macro_window {
        return;
    }
    let mut open = s.show_macro_window;
    crate::liquid_glass::glass_window(tr("Macro Controls"))
        .open(&mut open)
        .resizable(true)
        .default_size(vec2(760.0, 290.0))
        .show(ctx, |ui| {
            show(ui, p, s, true);
        });
    s.show_macro_window = open;
}
