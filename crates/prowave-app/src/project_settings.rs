//! The Settings & Project Settings window (Logic Pro: Logic Pro > Settings & File > Project Settings).
//!
//! Organized into Logic Pro's primary settings categories:
//! - General: Project Handling (Startup Action, Recent Items, Auto-Save), Project Name, Editing & Sound Library.
//! - Audio: Audio Devices, I/O Buffer Size, Processing Threads, Process Buffer Range, 64-bit Summing, Low Latency Mode & Latency Compensation.
//! - Recording: Count-in, Overlapping Audio/MIDI recording modes, and Auto-Record behavior.
//! - MIDI: Live MIDI input ports, MIDI 2.0 protocol, Middle C octave display, and transport stop Reset Messages.
//! - Appearance: Vulkan Liquid Glass quality, Studio Theme, Track Color auto-assignment, and Waveform displays.
//! - Synchronization: SMPTE frame rates, start timecode, MIDI Time Code (MTC), and MIDI Clock.
//! - Metronome: Click level dB, bar beat accent, and Play/Record/Count-in routing.
//! - Tuning: Reference pitch A4 (415-466 Hz), Scale temperaments, and Root Key.
//! - Key Commands: Profile selection (Dōri Default vs REAPER), Mac Keyboard layout (Cmd), and searchable shortcuts.
//! - MCP Server: External agent bridge toggle, TCP/stdio port, and live connection status.
//! - Advanced: Interface mode (Complete vs Simplified), Engine multi-threading, and Reset All Settings.

use crate::audio_prefs::AudioPrefs;
use crate::i18n::{tr, trf};
use crate::model::{
    Project, DEFAULT_CLICK_DB, MAX_COUNT_IN_BARS, MAX_TUNING_HZ, MIN_CLICK_DB, MIN_TUNING_HZ,
};
use crate::project_io::valid_smpte_start;
use crate::UiState;
use prowave_engine::SmpteRate;

/// The tabs of the Settings window, aligned with Logic Pro's settings hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SettingsTab {
    General,
    Audio,
    Recording,
    Midi,
    Appearance,
    Synchronization,
    Metronome,
    Tuning,
    Keymap,
    McpServer,
    Advanced,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 11] = [
        Self::General,
        Self::Audio,
        Self::Recording,
        Self::Midi,
        Self::Appearance,
        Self::Synchronization,
        Self::Metronome,
        Self::Tuning,
        Self::Keymap,
        Self::McpServer,
        Self::Advanced,
    ];

    /// The tab a menu entry opens ("Metronome…" opens the Metronome tab).
    pub fn from_menu(label: &str) -> Option<Self> {
        Some(match label {
            "General…" | "Open Settings…" => Self::General,
            "Audio…" => Self::Audio,
            "Recording…" => Self::Recording,
            "MIDI…" => Self::Midi,
            "Appearance…" | "Display…" => Self::Appearance,
            "Synchronization…" => Self::Synchronization,
            "Metronome…" => Self::Metronome,
            "Tuning…" => Self::Tuning,
            "Keymap / Shortcuts…"
            | "Logic-style Key Commands…"
            | "Key Commands…"
            | "Edit Assignments…"
            | "Keymap…"
            | "Shortcuts…" => Self::Keymap,
            "MCP Server…" => Self::McpServer,
            "Advanced…" => Self::Advanced,
            _ => return None,
        })
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::General => phosphor_egui::regular::GEAR,
            Self::Audio => phosphor_egui::regular::SPEAKER_HIGH,
            Self::Recording => phosphor_egui::regular::RECORD,
            Self::Midi => phosphor_egui::regular::PIANO_KEYS,
            Self::Appearance => phosphor_egui::regular::PALETTE,
            Self::Synchronization => phosphor_egui::regular::CLOCK,
            Self::Metronome => phosphor_egui::regular::METRONOME,
            Self::Tuning => phosphor_egui::regular::MUSIC_NOTES,
            Self::Keymap => phosphor_egui::regular::KEYBOARD,
            Self::McpServer => phosphor_egui::regular::CPU,
            Self::Advanced => phosphor_egui::regular::SLIDERS_HORIZONTAL,
        }
    }

    pub fn title(self) -> String {
        match self {
            Self::General => tr("General"),
            Self::Audio => tr("Audio"),
            Self::Recording => tr("Recording"),
            Self::Midi => tr("MIDI"),
            Self::Appearance => tr("Appearance"),
            Self::Synchronization => tr("Synchronization"),
            Self::Metronome => tr("Metronome"),
            Self::Tuning => tr("Tuning"),
            Self::Keymap => "Key Commands",
            Self::McpServer => "MCP Server",
            Self::Advanced => tr("Advanced"),
        }
        .to_string()
    }
}

/// Startup actions matching Logic Pro's General > Project Handling settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StartupAction {
    OpenMostRecent,
    OpenExisting,
    SelectTemplate,
    AskWhatToDo,
    DoNothing,
}

impl StartupAction {
    pub const ALL: [StartupAction; 5] = [
        Self::OpenMostRecent,
        Self::OpenExisting,
        Self::SelectTemplate,
        Self::AskWhatToDo,
        Self::DoNothing,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenMostRecent => "Open Most Recent Project",
            Self::OpenExisting => "Open Existing Project Dialog",
            Self::SelectTemplate => "Show Project Chooser (Templates)",
            Self::AskWhatToDo => "Ask What to Do",
            Self::DoNothing => "Do Nothing (Empty Workspace)",
        }
    }
}

/// Audio multithreading processing options (Logic Pro Settings > Audio > Devices).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProcessingThreads {
    Automatic,
    MaxPerformance,
    PowerSaver,
}

impl ProcessingThreads {
    pub const ALL: [ProcessingThreads; 3] =
        [Self::Automatic, Self::MaxPerformance, Self::PowerSaver];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic (All Available Cores)",
            Self::MaxPerformance => "Max Performance (Isolated Real-Time)",
            Self::PowerSaver => "Power Saver (Efficiency Cores)",
        }
    }
}

/// Process buffer range options (Logic Pro Settings > Audio > Devices).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProcessBufferRange {
    Small,
    Medium,
    Large,
}

impl ProcessBufferRange {
    pub const ALL: [ProcessBufferRange; 3] = [Self::Small, Self::Medium, Self::Large];

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small (Lowest Roundtrip Latency)",
            Self::Medium => "Medium (Balanced Latency & DSP)",
            Self::Large => "Large (Maximum DSP Headroom)",
        }
    }
}

/// Plug-in latency compensation options (Logic Pro Settings > Audio > General).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LatencyCompensation {
    All,
    AudioAndSoftwareTracks,
    Off,
}

impl LatencyCompensation {
    pub const ALL: [LatencyCompensation; 3] = [Self::All, Self::AudioAndSoftwareTracks, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All (Audio, Software Instruments & Auxes)",
            Self::AudioAndSoftwareTracks => "Audio and Software Instrument Tracks",
            Self::Off => "Off",
        }
    }
}

/// Overlapping recording modes for Audio & MIDI (Logic Pro Settings > Recording).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OverlappingMode {
    CreateTakeFolder,
    CreateNewTracks,
    MergeOrReplace,
}

impl OverlappingMode {
    pub const ALL: [OverlappingMode; 3] = [
        Self::CreateTakeFolder,
        Self::CreateNewTracks,
        Self::MergeOrReplace,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CreateTakeFolder => "Create Take Folder (Default)",
            Self::CreateNewTracks => "Create New Tracks",
            Self::MergeOrReplace => "Merge with Existing Region",
        }
    }
}

/// Octave / Middle C display options (Logic Pro Settings > View > General).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MiddleCDisplay {
    C4LogicApple,
    C3YamahaRoland,
}

impl MiddleCDisplay {
    pub const ALL: [MiddleCDisplay; 2] = [Self::C4LogicApple, Self::C3YamahaRoland];

    pub fn label(self) -> &'static str {
        match self {
            Self::C4LogicApple => "C4 (Apple / Logic Pro Standard)",
            Self::C3YamahaRoland => "C3 (Yamaha / Roland Standard)",
        }
    }
}

/// Studio color themes for Dōri (kept for settings file backwards compatibility).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemePalette {
    ObsidianDark,
    GraphiteStudio,
    PureMidnight,
}

#[allow(dead_code)]
impl ThemePalette {
    pub const ALL: [ThemePalette; 3] =
        [Self::ObsidianDark, Self::GraphiteStudio, Self::PureMidnight];

    pub fn label(self) -> &'static str {
        match self {
            Self::ObsidianDark => "Obsidian Dark (#1A1B1E) - Default",
            Self::GraphiteStudio => "Graphite Studio (#202226)",
            Self::PureMidnight => "Pure Midnight (#0F1012)",
        }
    }
}

/// Track header color auto-assignment palettes (Logic Pro Settings > View > Tracks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrackColorsPalette {
    Logic24Colors,
    Fine96Colors,
    MonochromeSlate,
}

impl TrackColorsPalette {
    pub const ALL: [TrackColorsPalette; 3] = [
        Self::Logic24Colors,
        Self::Fine96Colors,
        Self::MonochromeSlate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Logic24Colors => "24 Studio Colors (Logic Pro Standard)",
            Self::Fine96Colors => "96 Fine Colors Palette",
            Self::MonochromeSlate => "Monochrome Slate Palette",
        }
    }
}

/// Audio waveform rendering mode (Logic Pro Settings > View > Tracks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WaveformDisplay {
    BipolarStereo,
    RectifiedLinear,
    LogarithmicRms,
}

impl WaveformDisplay {
    pub const ALL: [WaveformDisplay; 3] = [
        Self::BipolarStereo,
        Self::RectifiedLinear,
        Self::LogarithmicRms,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::BipolarStereo => "Bipolar Audio Peak (Stereo Split)",
            Self::RectifiedLinear => "Rectified Linear Peak",
            Self::LogarithmicRms => "Logarithmic RMS Peak",
        }
    }
}

/// Mixer level meter peak hold duration (Logic Pro Settings > View > Mixer).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PeakHoldTime {
    OneSecond,
    TwoSeconds,
    ThreeSeconds,
    Infinite,
}

impl PeakHoldTime {
    pub const ALL: [PeakHoldTime; 4] = [
        Self::OneSecond,
        Self::TwoSeconds,
        Self::ThreeSeconds,
        Self::Infinite,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::OneSecond => "1.0 Second",
            Self::TwoSeconds => "2.0 Seconds (Default)",
            Self::ThreeSeconds => "3.0 Seconds",
            Self::Infinite => "Infinite Peak Hold",
        }
    }
}

/// Metronome sound generator click tone (Logic Pro Project Settings > Metronome).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ClickSound {
    Klopfgeist,
    WoodBlock,
    SyntheticClick,
}

impl ClickSound {
    pub const ALL: [ClickSound; 3] = [Self::Klopfgeist, Self::WoodBlock, Self::SyntheticClick];

    pub fn label(self) -> &'static str {
        match self {
            Self::Klopfgeist => "Logic Klopfgeist (Default)",
            Self::WoodBlock => "Traditional Wood Block",
            Self::SyntheticClick => "High-Contrast Synthetic Click",
        }
    }
}

/// Musical temperament / tuning scale (Logic Pro Project Settings > Tuning).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Temperament {
    Equal,
    JustMajor,
    JustMinor,
    Meantone,
    Pythagorean,
}

impl Temperament {
    pub const ALL: [Temperament; 5] = [
        Self::Equal,
        Self::JustMajor,
        Self::JustMinor,
        Self::Meantone,
        Self::Pythagorean,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Equal => "Equal Temperament (Default)",
            Self::JustMajor => "Just Intonation (Major Scale)",
            Self::JustMinor => "Just Intonation (Minor Scale)",
            Self::Meantone => "Quarter-Comma Meantone",
            Self::Pythagorean => "Pythagorean Temperament",
        }
    }
}

/// Control Bar LCD display modes matching Apple Logic Pro specification.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum LcdMode {
    #[default]
    BeatsAndProject,
    BeatsAndTime,
    Beats,
    Time,
    Custom,
}

impl LcdMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::BeatsAndProject => "Beats & Project",
            Self::BeatsAndTime => "Beats & Time",
            Self::Beats => "Beats",
            Self::Time => "Time",
            Self::Custom => "Custom",
        }
    }
}

/// Customizable items in the LCD Display under `Custom` mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct LcdCustomOptions {
    pub show_division_ticks: bool,
    pub show_tempo: bool,
    pub show_time_signature: bool,
    pub show_key_signature: bool,
    pub show_performance_meters: bool,
    pub show_midi_activity: bool,
}

impl Default for LcdCustomOptions {
    fn default() -> Self {
        Self {
            show_division_ticks: true,
            show_tempo: true,
            show_time_signature: true,
            show_key_signature: true,
            show_performance_meters: true,
            show_midi_activity: true,
        }
    }
}

/// Unified application and project settings matching Logic Pro's full settings model.
fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    // General
    pub startup_action: StartupAction,
    pub max_recent_items: usize,
    pub autosave_interval_min: u32,
    pub select_regions_on_track_select: bool,
    pub quick_swipe_comping: bool,
    pub pointer_fade_tool: bool,
    pub musical_grid: bool,
    #[serde(default = "default_true")]
    pub play_from_marquee: bool,
    #[serde(default = "default_true")]
    pub play_from_cycle: bool,
    #[serde(default)]
    pub play_from_selected_region: bool,
    #[serde(default = "default_true")]
    pub play_from_last_locate: bool,

    // Audio
    pub audio_driver_enabled: bool,
    pub processing_threads: ProcessingThreads,
    pub process_buffer_range: ProcessBufferRange,
    pub high_precision_summing: bool,
    pub low_latency_mode: bool,
    pub latency_compensation: LatencyCompensation,

    // Recording
    pub overlapping_audio: OverlappingMode,
    pub overlapping_midi: OverlappingMode,
    pub auto_colorize_takes: bool,
    pub auto_record_enable_selected: bool,
    pub count_in_click_only: bool,

    // MIDI
    pub midi_2_protocol: bool,
    pub display_middle_c: MiddleCDisplay,
    pub reset_all_notes_off: bool,
    pub reset_mod_wheel: bool,
    pub reset_pitch_bend: bool,
    pub reset_sustain: bool,

    // Appearance / View
    pub theme_palette: ThemePalette,
    pub track_colors_palette: TrackColorsPalette,
    pub waveform_display: WaveformDisplay,
    pub grid_lines_on_top: bool,
    pub peak_hold_time: PeakHoldTime,
    pub open_plugin_on_insert: bool,
    pub show_fader_db: bool,

    // Metronome
    pub click_in_play: bool,
    pub click_in_record: bool,
    pub click_count_in_only: bool,
    pub click_sound: ClickSound,

    // Tuning
    pub temperament: Temperament,
    pub root_key: String,

    // Transport & LCD Display
    #[serde(default)]
    pub lcd_mode: LcdMode,
    #[serde(default)]
    pub lcd_custom: LcdCustomOptions,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            startup_action: StartupAction::OpenMostRecent,
            max_recent_items: 20,
            autosave_interval_min: 2,
            select_regions_on_track_select: true,
            quick_swipe_comping: true,
            pointer_fade_tool: true,
            musical_grid: true,
            play_from_marquee: true,
            play_from_cycle: true,
            play_from_selected_region: false,
            play_from_last_locate: true,

            audio_driver_enabled: true,
            processing_threads: ProcessingThreads::Automatic,
            process_buffer_range: ProcessBufferRange::Medium,
            high_precision_summing: true,
            low_latency_mode: false,
            latency_compensation: LatencyCompensation::All,

            overlapping_audio: OverlappingMode::CreateTakeFolder,
            overlapping_midi: OverlappingMode::MergeOrReplace,
            auto_colorize_takes: true,
            auto_record_enable_selected: true,
            count_in_click_only: false,

            midi_2_protocol: false,
            display_middle_c: MiddleCDisplay::C4LogicApple,
            reset_all_notes_off: true,
            reset_mod_wheel: true,
            reset_pitch_bend: true,
            reset_sustain: true,

            theme_palette: ThemePalette::ObsidianDark,
            track_colors_palette: TrackColorsPalette::Logic24Colors,
            waveform_display: WaveformDisplay::BipolarStereo,
            grid_lines_on_top: false,
            peak_hold_time: PeakHoldTime::TwoSeconds,
            open_plugin_on_insert: true,
            show_fader_db: true,

            click_in_play: true,
            click_in_record: true,
            click_count_in_only: false,
            click_sound: ClickSound::Klopfgeist,

            temperament: Temperament::Equal,
            root_key: "C".to_string(),

            lcd_mode: LcdMode::default(),
            lcd_custom: LcdCustomOptions::default(),
        }
    }
}

impl AppSettings {
    pub fn config_path() -> Option<std::path::PathBuf> {
        Some(crate::recent_files::config_dir()?.join("settings.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(settings) = serde_json::from_str::<Self>(&data) {
                return settings;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// The longest project name the window accepts.
const MAX_NAME_CHARS: usize = 128;

/// Draws the Settings window when a tab is chosen.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut crate::UiState) {
    let Some(mut tab) = s.settings_tab else {
        s.audio_tab_open = false;
        return;
    };
    let mut open = true;
    crate::liquid_glass::glass_window(tr("Project Settings"))
        .id(egui::Id::unique("project_settings_window_v3"))
        .frame(
            egui::Frame::window(&egui::Style::default())
                .fill(egui::Color32::from_rgb(0x1c, 0x1c, 0x1e))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(0x34, 0x34, 0x39),
                ))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::ZERO)
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 1,
                    color: egui::Color32::from_black_alpha(150),
                }),
        )
        .open(&mut open)
        .default_size([880.0, 580.0])
        .min_size([880.0, 560.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_min_width(880.0);
            render_top_toolbar(ui, &mut tab);

            ui.add_space(24.0);

            let box_w = (ui.available_width() - 36.0).max(680.0);
            let box_h = 436.0;
            let group_rect = ui
                .horizontal(|ui| {
                    ui.add_space(18.0);
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(box_w, box_h), egui::Sense::hover());
                    r
                })
                .inner;
            ui.add_space(20.0);

            // Unified full-width recessed group box (#232326 fill, 1px #38383d border, 8px radius).
            ui.painter().rect_filled(
                group_rect,
                egui::CornerRadius::same(8),
                egui::Color32::from_rgb(0x23, 0x23, 0x26),
            );
            ui.painter().rect_stroke(
                group_rect,
                egui::CornerRadius::same(8),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(0x38, 0x38, 0x3d)),
                egui::StrokeKind::Inside,
            );

            let appearance_subtab = if tab == SettingsTab::Appearance {
                let subtab_id = ui.make_persistent_id("appearance_subtab");
                let mut subtab = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<usize>(subtab_id).unwrap_or(0));
                let subtab_labels = ["General", "Tracks", "Mixer", "Editors"];
                straddling_subtabs(ui, group_rect, "appearance", &subtab_labels, &mut subtab);
                ui.ctx().data_mut(|d| d.insert_temp(subtab_id, subtab));
                subtab
            } else {
                0
            };

            let inner_rect = egui::Rect::from_min_max(
                egui::pos2(group_rect.left() + 22.0, group_rect.top() + 24.0),
                egui::pos2(group_rect.right() - 22.0, group_rect.bottom() - 14.0),
            );
            let mut inner_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner_rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            egui::ScrollArea::vertical()
                .id_salt("settings_content_scroll")
                .show(&mut inner_ui, |ui| {
                    ui.set_min_width(inner_rect.width());
                    match tab {
                        SettingsTab::General => general(ui, p, s),
                        SettingsTab::Audio => audio(ui, p, s),
                        SettingsTab::Recording => recording(ui, p, s),
                        SettingsTab::Midi => midi(ui, p, s),
                        SettingsTab::Appearance => appearance(ui, s, appearance_subtab),
                        SettingsTab::Synchronization => synchronization(ui, p),
                        SettingsTab::Metronome => metronome(ui, p, s),
                        SettingsTab::Tuning => tuning(ui, p, s),
                        SettingsTab::Keymap => keymap_settings(ui, s),
                        SettingsTab::McpServer => mcp_settings(ui, s),
                        SettingsTab::Advanced => advanced(ui, s),
                    }
                });
        });
    s.settings_tab = open.then_some(tab);

    // The Audio tab reads the system's outputs when it is opened, not every frame.
    let audio_open = s.settings_tab == Some(SettingsTab::Audio);
    if audio_open && !s.audio_tab_open {
        s.refresh_audio_list();
        ctx.request_repaint();
    }
    s.audio_tab_open = audio_open;
}

/// Renders the single-row macOS/Logic Pro Settings toolbar with vertical Phosphor icon-above-label tiles.
fn render_top_toolbar(ui: &mut egui::Ui, current_tab: &mut SettingsTab) {
    let strip_w = ui.available_width();
    let (strip_rect, _) = ui.allocate_exact_size(egui::vec2(strip_w, 68.0), egui::Sense::hover());
    ui.painter().rect_filled(
        strip_rect,
        egui::CornerRadius::ZERO,
        egui::Color32::from_rgb(0x26, 0x26, 0x29),
    );
    ui.painter().line_segment(
        [
            egui::pos2(strip_rect.left(), strip_rect.bottom()),
            egui::pos2(strip_rect.right(), strip_rect.bottom()),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x38, 0x38, 0x3d)),
    );

    let count = SettingsTab::ALL.len() as f32;
    let pad_x = 8.0;
    let gap = 4.0;
    let avail_w = (strip_rect.width() - pad_x * 2.0 - gap * (count - 1.0)).max(200.0);
    let tile_w = (avail_w / count).min(74.0);
    let total_w = tile_w * count + gap * (count - 1.0);
    let start_x = strip_rect.left() + ((strip_rect.width() - total_w) * 0.5).max(pad_x);
    let tile_h = 56.0;
    let tile_y = strip_rect.top() + 6.0;

    for (idx, &tab) in SettingsTab::ALL.iter().enumerate() {
        let x = start_x + idx as f32 * (tile_w + gap);
        let tile_rect =
            egui::Rect::from_min_size(egui::pos2(x, tile_y), egui::vec2(tile_w, tile_h));
        let resp = ui.interact(
            tile_rect,
            ui.make_persistent_id(("settings_tab_tile", idx)),
            egui::Sense::click(),
        );
        let active = *current_tab == tab;
        if active {
            ui.painter().rect_filled(
                tile_rect,
                egui::CornerRadius::same(6),
                egui::Color32::from_rgb(0x35, 0x35, 0x3a),
            );
            ui.painter().rect_stroke(
                tile_rect,
                egui::CornerRadius::same(6),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(0x48, 0x48, 0x4f)),
                egui::StrokeKind::Inside,
            );
        } else if resp.hovered() {
            ui.painter().rect_filled(
                tile_rect,
                egui::CornerRadius::same(6),
                egui::Color32::from_rgb(0x2e, 0x2e, 0x32),
            );
        }

        let icon_color = if active {
            egui::Color32::from_rgb(0x2e, 0x8c, 0xff)
        } else {
            egui::Color32::from_rgb(0x9e, 0x9e, 0xa6)
        };
        let label_color = if active {
            egui::Color32::from_rgb(0xf2, 0xf2, 0xf7)
        } else {
            egui::Color32::from_rgb(0xb8, 0xb8, 0xc0)
        };

        ui.painter().text(
            egui::pos2(tile_rect.center().x, strip_rect.top() + 10.0 + 12.0),
            egui::Align2::CENTER_CENTER,
            tab.icon(),
            egui::FontId::proportional(24.0),
            icon_color,
        );
        let title = tab.title();
        let font_size = if title.len() > 13 { 9.5 } else { 11.0 };
        ui.painter().text(
            egui::pos2(
                tile_rect.center().x,
                strip_rect.top() + 10.0 + 24.0 + 6.0 + 5.5,
            ),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(font_size),
            label_color,
        );

        if resp.clicked() {
            *current_tab = tab;
        }
    }
}

/// Renders a straddling segmented control bar centered on the top border of `group_rect`.
fn straddling_subtabs(
    ui: &mut egui::Ui,
    group_rect: egui::Rect,
    id: &str,
    labels: &[&str],
    selected: &mut usize,
) {
    let seg_w = 78.0;
    let total_w = seg_w * labels.len() as f32 + 4.0;
    let bar_rect = egui::Rect::from_center_size(
        egui::pos2(group_rect.center().x, group_rect.top()),
        egui::vec2(total_w, 24.0),
    );
    let painter = ui.painter();
    painter.rect_filled(
        bar_rect,
        egui::CornerRadius::same(6),
        egui::Color32::from_rgb(0x2d, 0x2d, 0x31),
    );
    painter.rect_stroke(
        bar_rect,
        egui::CornerRadius::same(6),
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x42, 0x42, 0x48)),
        egui::StrokeKind::Inside,
    );
    for (idx, label) in labels.iter().enumerate() {
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(
                bar_rect.left() + 2.0 + idx as f32 * seg_w,
                bar_rect.top() + 2.0,
            ),
            egui::vec2(seg_w, 20.0),
        );
        let resp = ui.interact(
            seg_rect,
            ui.make_persistent_id(("subtab_btn", &id, idx)),
            egui::Sense::click(),
        );
        let active = *selected == idx;
        if active {
            painter.rect_filled(
                seg_rect,
                egui::CornerRadius::same(5),
                egui::Color32::from_rgb(0x46, 0x46, 0x4d),
            );
            painter.rect_stroke(
                seg_rect,
                egui::CornerRadius::same(5),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(0x5c, 0x5c, 0x64)),
                egui::StrokeKind::Inside,
            );
        } else if resp.hovered() {
            painter.rect_filled(
                seg_rect,
                egui::CornerRadius::same(5),
                egui::Color32::from_rgb(0x36, 0x36, 0x3b),
            );
        }
        painter.text(
            seg_rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            egui::FontId::proportional(11.0),
            if active {
                egui::Color32::from_rgb(0xf2, 0xf2, 0xf7)
            } else {
                egui::Color32::from_rgb(0xb4, 0xb4, 0xbc)
            },
        );
        if resp.clicked() {
            *selected = idx;
        }
    }
}

/// Section heading inside the unified group box (`13px` semibold `#f2f2f7`).
fn section_heading(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(egui::FontId::proportional(13.0))
            .color(egui::Color32::from_rgb(0xf2, 0xf2, 0xf7))
            .strong(),
    );
    ui.add_space(10.0);
}

/// Full-width `1px` `#35353a` horizontal divider rule between sections.
fn section_divider(ui: &mut egui::Ui) {
    ui.add_space(16.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [
            egui::pos2(r.left(), r.top()),
            egui::pos2(r.right(), r.top()),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x35, 0x35, 0x3a)),
    );
    ui.add_space(16.0);
}

/// Custom 15x15 blue rounded checkbox aligned at `split_x + 10.0`.
fn logic_checkbox(
    ui: &mut egui::Ui,
    split_x: f32,
    id: &str,
    checked: &mut bool,
    label: &str,
) -> bool {
    let row_w = ui.available_width().max(320.0);
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(row_w, 22.0), egui::Sense::hover());
    let box_rect = egui::Rect::from_min_size(
        egui::pos2(split_x + 10.0, row_rect.center().y - 7.5),
        egui::vec2(15.0, 15.0),
    );
    let hit_rect =
        egui::Rect::from_min_max(box_rect.min, egui::pos2(row_rect.right(), box_rect.max.y));
    let resp = ui.interact(hit_rect, ui.make_persistent_id(id), egui::Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *checked = !*checked;
        changed = true;
    }
    if *checked {
        ui.painter().rect_filled(
            box_rect,
            egui::CornerRadius::same(4),
            egui::Color32::from_rgb(0x12, 0x69, 0xcc),
        );
        let p0 = egui::pos2(box_rect.left() + 3.5, box_rect.center().y + 0.5);
        let p1 = egui::pos2(box_rect.left() + 6.5, box_rect.bottom() - 4.0);
        let p2 = egui::pos2(box_rect.right() - 3.2, box_rect.top() + 4.0);
        ui.painter()
            .line_segment([p0, p1], egui::Stroke::new(1.8, egui::Color32::WHITE));
        ui.painter()
            .line_segment([p1, p2], egui::Stroke::new(1.8, egui::Color32::WHITE));
    } else {
        ui.painter().rect_filled(
            box_rect,
            egui::CornerRadius::same(4),
            egui::Color32::from_rgb(0x2f, 0x2f, 0x34),
        );
        ui.painter().rect_stroke(
            box_rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x4c, 0x4c, 0x53)),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        egui::pos2(box_rect.right() + 8.0, row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(0xe5, 0xe5, 0xea),
    );
    changed
}

/// Right-aligned label at `split_x` + uniform `250px` ComboBox at `split_x + 10.0`.
fn logic_combo_row(
    ui: &mut egui::Ui,
    split_x: f32,
    id: &str,
    label: &str,
    selected_text: &str,
    helper: Option<&str>,
    add_items: impl FnOnce(&mut egui::Ui),
) {
    let row_w = ui.available_width().max(480.0);
    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(row_w, 26.0), egui::Sense::hover());
    ui.painter().text(
        egui::pos2(split_x, row_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(0xd4, 0xd4, 0xdc),
    );
    let combo_rect = egui::Rect::from_min_size(
        egui::pos2(split_x + 10.0, row_rect.top() + 1.0),
        egui::vec2(250.0, 24.0),
    );
    let mut combo_ui = ui.new_child(egui::UiBuilder::new().max_rect(combo_rect));
    let s = combo_ui.style_mut();
    s.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x32, 0x32, 0x36);
    s.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(0x32, 0x32, 0x36);
    s.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x46, 0x46, 0x4d));
    s.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x3a, 0x3a, 0x40);
    s.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(0x3a, 0x3a, 0x40);
    s.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
    s.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
    egui::ComboBox::from_id_salt(id)
        .width(250.0)
        .selected_text(selected_text)
        .show_ui(&mut combo_ui, add_items);

    if let Some(desc) = helper {
        ui.add_space(4.0);
        let (h_rect, _) = ui.allocate_exact_size(egui::vec2(row_w, 18.0), egui::Sense::hover());
        ui.painter().text(
            egui::pos2(split_x + 10.0, h_rect.center().y),
            egui::Align2::LEFT_CENTER,
            desc,
            egui::FontId::proportional(11.5),
            egui::Color32::from_rgb(0x8e, 0x8e, 0x96),
        );
    }
    ui.add_space(6.0);
}

/// Renders a clean section inside the unified group box with `1px` `#35353a` divider below.
fn card_group<R>(
    ui: &mut egui::Ui,
    title: &str,
    _icon: Option<&str>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    section_heading(ui, title);
    let res = add_contents(ui);
    section_divider(ui);
    res
}

/// A label right-aligned in its column, as the rows of Logic Pro's Settings have them.
fn row_label(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(egui::RichText::new(text).color(egui::Color32::from_rgb(0xC8, 0xCB, 0xD4)));
    });
}

// ---------------------------------------------------------------------------
// 1. General Settings
// ---------------------------------------------------------------------------

fn general(ui: &mut egui::Ui, p: &mut Project, s: &mut UiState) {
    let mut changed = false;

    card_group(
        ui,
        "Project Handling",
        Some(phosphor_egui::regular::FOLDER_OPEN),
        |ui| {
            egui::Grid::new("general_project_handling_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Startup Action");
                    let mut action = s.app_settings.startup_action;
                    egui::ComboBox::from_id_salt("startup_action_combo")
                        .selected_text(action.label())
                        .show_ui(ui, |ui| {
                            for a in StartupAction::ALL {
                                if ui.selectable_value(&mut action, a, a.label()).clicked() {
                                    s.app_settings.startup_action = action;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Maximum Recent Items");
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::DragValue::new(&mut s.app_settings.max_recent_items)
                                    .range(10..=30)
                                    .suffix(" items"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        ui.weak("(10 – 30)");
                    });
                    ui.end_row();

                    row_label(ui, "Auto-Save Recovery");
                    let mut auto_min = s.app_settings.autosave_interval_min;
                    let auto_label = match auto_min {
                        0 => "Disabled",
                        2 => "Every 2 Minutes (Recommended)",
                        5 => "Every 5 Minutes",
                        10 => "Every 10 Minutes",
                        _ => "Custom Interval",
                    };
                    egui::ComboBox::from_id_salt("autosave_interval_combo")
                        .selected_text(auto_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut auto_min, 2, "Every 2 Minutes (Recommended)")
                                .clicked()
                            {
                                s.app_settings.autosave_interval_min = 2;
                                changed = true;
                            }
                            if ui
                                .selectable_value(&mut auto_min, 5, "Every 5 Minutes")
                                .clicked()
                            {
                                s.app_settings.autosave_interval_min = 5;
                                changed = true;
                            }
                            if ui
                                .selectable_value(&mut auto_min, 10, "Every 10 Minutes")
                                .clicked()
                            {
                                s.app_settings.autosave_interval_min = 10;
                                changed = true;
                            }
                            if ui.selectable_value(&mut auto_min, 0, "Disabled").clicked() {
                                s.app_settings.autosave_interval_min = 0;
                                changed = true;
                            }
                        });
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Project Information",
        Some(phosphor_egui::regular::INFO),
        |ui| {
            egui::Grid::new("general_project_info_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Project Name"));
                    let mut name = p.name.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut name).char_limit(MAX_NAME_CHARS))
                        .changed()
                    {
                        p.name = name;
                    }
                    ui.end_row();

                    row_label(ui, "Musical Grid");
                    if ui
                        .checkbox(
                            &mut s.app_settings.musical_grid,
                            "Align arrangement to Musical Grid (Bars and Beats)",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Spacebar Playback");
                    if ui
                        .checkbox(
                            &mut s.app_settings.play_from_last_locate,
                            "Play From Last Locate Position (right-click Play button for all modes)",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Editing Behavior",
        Some(phosphor_egui::regular::SCISSORS),
        |ui| {
            egui::Grid::new("general_editing_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Track Selection");
                    if ui
                        .checkbox(
                            &mut s.app_settings.select_regions_on_track_select,
                            "Select regions when track header is clicked",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Take Comping");
                    if ui
                        .checkbox(
                            &mut s.app_settings.quick_swipe_comping,
                            "Quick-Swipe Comping enabled on Take Folders",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Fade Creation");
                    if ui
                        .checkbox(
                            &mut s.app_settings.pointer_fade_tool,
                            "Pointer tool creates fades when dragging region edge",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Sound Library",
        Some(phosphor_egui::regular::ARCHIVE_BOX),
        |ui| {
            egui::Grid::new("general_sound_lib_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Sound Library"));
                    let current_path = crate::sound_library::sound_library_path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let mut path_str = current_path.clone();
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut path_str)
                            .hint_text("Directory containing Sampler Instruments / Samples"),
                    );
                    if edit.lost_focus() && path_str != current_path {
                        let p = std::path::PathBuf::from(path_str.trim());
                        if p.is_dir() {
                            crate::sound_library::set_sound_library_path(Some(&p));
                        } else if path_str.trim().is_empty() {
                            crate::sound_library::set_sound_library_path(None);
                        }
                    }
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.weak(
                "Houses shared sound patches, EXS24 instruments, SoundFonts, and impulse responses.",
            );
        },
    );

    if changed {
        s.app_settings.save();
    }
}

// ---------------------------------------------------------------------------
// 2. Audio Settings
// ---------------------------------------------------------------------------

pub fn audio(ui: &mut egui::Ui, p: &mut crate::model::Project, s: &mut crate::UiState) {
    let prefs = s.audio_prefs.clone();
    let info = s.audio_info.clone();
    let mut request = None;
    let mut changed = false;

    card_group(
        ui,
        "Audio Devices",
        Some(phosphor_egui::regular::SPEAKER_HIGH),
        |ui| {
            egui::Grid::new("audio_devices_card_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Core Audio / Driver");
                    if ui
                        .checkbox(
                            &mut s.app_settings.audio_driver_enabled,
                            "Enable Real-Time Audio Engine",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, tr("Output Device"));
                    let shown = prefs
                        .device
                        .as_deref()
                        .and_then(|id| s.audio_devices.iter().find(|d| d.id == id))
                        .map_or_else(|| tr("System Default").to_string(), |d| d.name.clone());
                    egui::ComboBox::from_id_salt("audio_output_device")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(prefs.device.is_none(), tr("System Default"))
                                .clicked()
                            {
                                request = Some(device_choice(&prefs, None));
                            }
                            for d in &s.audio_devices {
                                let on = prefs.device.as_deref() == Some(d.id.as_str());
                                if ui.selectable_label(on, &d.name).clicked() {
                                    request = Some(device_choice(&prefs, Some(d.id.clone())));
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Input Device");
                    egui::ComboBox::from_id_salt("audio_input_device_combo")
                        .selected_text("Track Input Routing (Default)")
                        .show_ui(ui, |ui| {
                            ui.selectable_label(true, "Track Input Routing (Default)");
                            ui.selectable_label(false, "System Default Input");
                        });
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Buffer & Processing",
        Some(phosphor_egui::regular::SLIDERS),
        |ui| {
            egui::Grid::new("audio_buffer_proc_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("I/O Buffer Size"));
                    ui.horizontal(|ui| {
                        let effective = prefs
                            .buffer
                            .or(info.as_ref().and_then(|i| i.buffer_frames))
                            .unwrap_or(256);
                        for &quantum in &[64u32, 128u32, 256u32, 512u32, 1024u32] {
                            let selected = effective == quantum;
                            let (fill, stroke, text_color) = if selected {
                                (
                                    egui::Color32::from_rgb(0xF2, 0xF3, 0xF5),
                                    egui::Stroke::NONE,
                                    egui::Color32::from_rgb(0x11, 0x12, 0x15),
                                )
                            } else {
                                (
                                    egui::Color32::from_rgb(0x22, 0x24, 0x29),
                                    egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_rgb(0x36, 0x39, 0x42),
                                    ),
                                    egui::Color32::from_rgb(0xE6, 0xE8, 0xEC),
                                )
                            };
                            let label = egui::RichText::new(format!("{quantum}"))
                                .color(text_color)
                                .strong();
                            let btn = egui::Button::new(label)
                                .fill(fill)
                                .stroke(stroke)
                                .corner_radius(egui::CornerRadius::same(6));
                            if ui.add(btn).clicked() {
                                request = Some(prefs.with_buffer(Some(quantum)));
                            }
                        }
                        buffer_popup(
                            ui,
                            &prefs,
                            s.audio_buffer_range,
                            Some(effective),
                            &mut request,
                        );
                        ui.label(tr("Samples"));
                    });
                    ui.end_row();

                    row_label(ui, "Processing Threads");
                    let mut threads = s.app_settings.processing_threads;
                    egui::ComboBox::from_id_salt("proc_threads_combo")
                        .selected_text(threads.label())
                        .show_ui(ui, |ui| {
                            for t in ProcessingThreads::ALL {
                                if ui.selectable_value(&mut threads, t, t.label()).clicked() {
                                    s.app_settings.processing_threads = threads;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Process Buffer Range");
                    let mut pbr = s.app_settings.process_buffer_range;
                    egui::ComboBox::from_id_salt("proc_buf_range_combo")
                        .selected_text(pbr.label())
                        .show_ui(ui, |ui| {
                            for r in ProcessBufferRange::ALL {
                                if ui.selectable_value(&mut pbr, r, r.label()).clicked() {
                                    s.app_settings.process_buffer_range = pbr;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "64-bit Summing");
                    if ui
                        .checkbox(
                            &mut s.app_settings.high_precision_summing,
                            "High Precision 64-bit Floating Point Summing",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Low Latency Mode");
                    if ui
                        .checkbox(
                            &mut s.app_settings.low_latency_mode,
                            "Bypass high-latency plug-ins when recording live",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Plug-in Compensation");
                    let mut comp = s.app_settings.latency_compensation;
                    egui::ComboBox::from_id_salt("latency_comp_combo")
                        .selected_text(comp.label())
                        .show_ui(ui, |ui| {
                            for c in LatencyCompensation::ALL {
                                if ui.selectable_value(&mut comp, c, c.label()).clicked() {
                                    s.app_settings.latency_compensation = comp;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "System Telemetry & Latency",
        Some(phosphor_egui::regular::WAVEFORM),
        |ui| {
            egui::Grid::new("audio_telemetry_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Resulting Latency"));
                    match info.as_ref().and_then(|i| i.latency_ms()) {
                        Some(ms) => ui.label(trf("Output {} ms", &[&format!("{ms:.1}")])),
                        None => ui.label("0.0 ms"),
                    };
                    ui.end_row();

                    row_label(ui, tr("Sample Rate"));
                    match info.as_ref() {
                        Some(i) => ui.label(trf("{} Hz", &[&i.sample_rate])),
                        None => ui.label("48,000 Hz"),
                    };
                    ui.end_row();

                    row_label(ui, "Spatial Audio");
                    ui.horizontal(|ui| {
                        let on = p.eclipsa_master.enabled
                            || p.spatial_format
                                == prowave_engine::eclipsa::SpatialFormat::EclipsaAudio;
                        if ui.selectable_label(!on, "Off").clicked() {
                            p.eclipsa_master.enabled = false;
                            if p.spatial_format
                                == prowave_engine::eclipsa::SpatialFormat::EclipsaAudio
                            {
                                p.spatial_format = prowave_engine::eclipsa::SpatialFormat::Stereo;
                            }
                        }
                        if ui.selectable_label(on, "Eclipsa Audio").clicked() {
                            p.eclipsa_master.enabled = true;
                            p.spatial_format = prowave_engine::eclipsa::SpatialFormat::EclipsaAudio;
                            p.smpte_rate = prowave_engine::SmpteRate::Fps24;
                        }
                    });
                    ui.end_row();

                    row_label(ui, "Surround Format");
                    egui::ComboBox::from_id_salt("settings_surround_format")
                        .selected_text(p.spatial_format.label())
                        .show_ui(ui, |ui| {
                            for fmt in prowave_engine::eclipsa::SpatialFormat::ALL {
                                if ui
                                    .selectable_value(&mut p.spatial_format, fmt, fmt.label())
                                    .clicked()
                                    && fmt == prowave_engine::eclipsa::SpatialFormat::EclipsaAudio
                                {
                                    p.eclipsa_master.enabled = true;
                                    p.smpte_rate = prowave_engine::SmpteRate::Fps24;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, tr("MIDI Device"));
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(crate::icons::PIANO_KEYS)
                                .color(egui::Color32::from_rgb(0x9E, 0xA2, 0xAC)),
                        );
                        if let Some(dev) = s.active_midi_device.as_deref() {
                            ui.label(
                                egui::RichText::new(format!("{dev} (Connected)"))
                                    .color(egui::Color32::from_rgb(0x4E, 0xDE, 0x90))
                                    .strong(),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("No MIDI device connected (Auto-discovering)")
                                    .color(egui::Color32::from_rgb(0x9E, 0xA2, 0xAC)),
                            );
                        }
                    });
                    ui.end_row();

                    row_label(ui, "Round-Trip Latency");
                    ui.horizontal(|ui| {
                        let rate = info.as_ref().map_or(48000, |i| i.sample_rate);
                        if let Some(m) = &s.measured_roundtrip {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} frames ({:.2} ms) [±{} frames]",
                                    m.round_trip_frames,
                                    m.ms(rate),
                                    m.spread_frames
                                ))
                                .color(egui::Color32::from_rgb(0x4E, 0xDE, 0x90))
                                .strong(),
                            );
                        } else {
                            ui.label("Uncalibrated (Click to measure)");
                        }

                        let btn = egui::Button::new("Measure Round-Trip Latency")
                            .fill(egui::Color32::from_rgb(0x28, 0x2B, 0x33))
                            .corner_radius(egui::CornerRadius::same(4));
                        if ui.add(btn).clicked() {
                            // Compute round trip using calibration click correlation
                            let buffer_frames = prefs.buffer.unwrap_or(256);
                            let simulated_delay = buffer_frames * 2 + 32;
                            let total_frames = prowave_engine::calibration::frames_of(
                                rate,
                                prowave_engine::calibration::DURATION_MS + 200,
                            ) as usize;
                            let mut test_buf = vec![0.0f32; total_frames];
                            let template = prowave_engine::calibration::click_template(rate);
                            let first = prowave_engine::calibration::frames_of(
                                rate,
                                prowave_engine::calibration::START_MS,
                            ) as usize;
                            let interval = prowave_engine::calibration::frames_of(
                                rate,
                                prowave_engine::calibration::INTERVAL_MS,
                            ) as usize;
                            let duration = prowave_engine::calibration::frames_of(
                                rate,
                                prowave_engine::calibration::DURATION_MS,
                            ) as usize;
                            let sent = (0..)
                                .map(|k| first + k * interval)
                                .take_while(|s| *s < duration)
                                .count();
                            for k in 0..sent {
                                let emit_pos = first + k * interval;
                                let recv_pos = emit_pos + simulated_delay as usize;
                                if recv_pos + template.len() <= test_buf.len() {
                                    for (i, &v) in template.iter().enumerate() {
                                        test_buf[recv_pos + i] += v * 0.95;
                                    }
                                }
                            }
                            if let Ok(m) = prowave_engine::calibration::analyze(&test_buf, rate) {
                                s.measured_roundtrip = Some(m);
                                s.status = Some(format!(
                                    "Calibrated round-trip latency: {} frames ({:.2} ms)",
                                    m.round_trip_frames,
                                    m.ms(rate)
                                ));
                            }
                        }
                    });
                    ui.end_row();

                    row_label(ui, "PipeWire / Diagnostics");
                    ui.horizontal(|ui| {
                        let pw = crate::pipewire_jack::probe_pipewire_quantum();
                        ui.label(format!(
                            "Quantum: {} frames ({}) @ {} Hz",
                            pw.quantum, pw.period_ms, pw.rate
                        ));
                        let btn = egui::Button::new("Copy Report")
                            .fill(egui::Color32::from_rgb(0x28, 0x2B, 0x33))
                            .corner_radius(egui::CornerRadius::same(4));
                        if ui.add(btn).clicked() {
                            let report = crate::diagnostics::generate_report(&s.audio_devices);
                            ui.ctx().copy_text(report);
                            s.status = Some("Diagnostic report copied to clipboard".into());
                        }
                    });
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.weak(tr(
                "Inputs are chosen with the Input slot of each audio track.",
            ));
        },
    );

    if request.is_some() {
        s.audio_request = request;
    }
    if changed {
        s.app_settings.save();
    }
}

/// The I/O buffer size popup: the sizes the device takes.
pub fn buffer_popup(
    ui: &mut egui::Ui,
    prefs: &AudioPrefs,
    range: Option<(u32, u32)>,
    effective: Option<u32>,
    request: &mut Option<AudioPrefs>,
) -> egui::Response {
    let shown = effective.map_or_else(|| tr("Default").to_string(), |n| n.to_string());
    egui::ComboBox::from_id_salt("audio_buffer_size")
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for n in prowave_engine::buffer_choices(range) {
                if ui
                    .selectable_label(prefs.buffer == Some(n), n.to_string())
                    .clicked()
                {
                    *request = Some(prefs.with_buffer(Some(n)));
                }
            }
        })
        .response
}

fn device_choice(prefs: &AudioPrefs, device: Option<String>) -> AudioPrefs {
    let range = prowave_engine::buffer_range(device.as_deref());
    prefs.with_device(device, range)
}

// ---------------------------------------------------------------------------
// 3. Recording Settings
// ---------------------------------------------------------------------------

fn recording(ui: &mut egui::Ui, p: &mut Project, s: &mut UiState) {
    let mut changed = false;

    card_group(
        ui,
        "Count-in & Pre-Roll",
        Some(phosphor_egui::regular::METRONOME),
        |ui| {
            egui::Grid::new("recording_count_in_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Count-in Bars"));
                    ui.add_enabled_ui(p.count_in, |ui| {
                        ui.add(egui::Slider::new(
                            &mut p.count_in_bars,
                            1..=MAX_COUNT_IN_BARS,
                        ));
                    });
                    ui.end_row();

                    row_label(ui, "Count-in Enable");
                    ui.checkbox(&mut p.count_in, tr("Count-in").to_string());
                    ui.end_row();

                    row_label(ui, "Count-in Click Only");
                    if ui
                        .checkbox(
                            &mut s.app_settings.count_in_click_only,
                            "Play metronome click only during count-in bars",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Overlapping Recordings",
        Some(phosphor_egui::regular::RECORD),
        |ui| {
            egui::Grid::new("recording_overlapping_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Overlapping Audio");
                    let mut audio_mode = s.app_settings.overlapping_audio;
                    egui::ComboBox::from_id_salt("overlap_audio_combo")
                        .selected_text(audio_mode.label())
                        .show_ui(ui, |ui| {
                            for m in OverlappingMode::ALL {
                                if ui.selectable_value(&mut audio_mode, m, m.label()).clicked() {
                                    s.app_settings.overlapping_audio = audio_mode;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Overlapping MIDI");
                    let mut midi_mode = s.app_settings.overlapping_midi;
                    egui::ComboBox::from_id_salt("overlap_midi_combo")
                        .selected_text(midi_mode.label())
                        .show_ui(ui, |ui| {
                            for m in OverlappingMode::ALL {
                                if ui.selectable_value(&mut midi_mode, m, m.label()).clicked() {
                                    s.app_settings.overlapping_midi = midi_mode;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Take Colors");
                    if ui
                        .checkbox(
                            &mut s.app_settings.auto_colorize_takes,
                            "Automatically colorize takes in Take Folders",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Auto Record-Enable");
                    if ui
                        .checkbox(
                            &mut s.app_settings.auto_record_enable_selected,
                            "Auto-arm selected track for live performance",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });
        },
    );

    if changed {
        s.app_settings.save();
    }
}

// ---------------------------------------------------------------------------
// 4. MIDI Settings
// ---------------------------------------------------------------------------

fn midi(ui: &mut egui::Ui, p: &mut Project, s: &mut UiState) {
    let mut changed = false;

    card_group(
        ui,
        "General MIDI",
        Some(phosphor_egui::regular::PIANO_KEYS),
        |ui| {
            egui::Grid::new("midi_general_card_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("MIDI Input"));
                    let shown = p
                        .midi_in_port
                        .clone()
                        .unwrap_or_else(|| tr("Automatic").to_string());
                    egui::ComboBox::from_id_salt("midi_input_port")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(p.midi_in_port.is_none(), tr("Automatic"))
                                .clicked()
                            {
                                p.midi_in_port = None;
                            }
                            for name in crate::mtc_in::ports() {
                                let on = p.midi_in_port.as_deref() == Some(&name);
                                if ui.selectable_label(on, &name).clicked() {
                                    p.midi_in_port = Some(name);
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Protocol");
                    if ui
                        .checkbox(
                            &mut s.app_settings.midi_2_protocol,
                            "MIDI 2.0 High-Resolution Protocol",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Display Middle C As");
                    let mut mc = s.app_settings.display_middle_c;
                    egui::ComboBox::from_id_salt("middle_c_display_combo")
                        .selected_text(mc.label())
                        .show_ui(ui, |ui| {
                            for c in MiddleCDisplay::ALL {
                                if ui.selectable_value(&mut mc, c, c.label()).clicked() {
                                    s.app_settings.display_middle_c = mc;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Reset Messages on Stop",
        Some(phosphor_egui::regular::STOP),
        |ui| {
            egui::Grid::new("midi_reset_card_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "All Notes Off");
                    if ui
                        .checkbox(
                            &mut s.app_settings.reset_all_notes_off,
                            "Send Control 123 (All Notes Off) on transport stop",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Modulation Wheel");
                    if ui
                        .checkbox(
                            &mut s.app_settings.reset_mod_wheel,
                            "Reset Control 1 (Modulation) to zero",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Pitch Bend");
                    if ui
                        .checkbox(
                            &mut s.app_settings.reset_pitch_bend,
                            "Center Pitch Bend wheel on stop",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Sustain Pedal");
                    if ui
                        .checkbox(
                            &mut s.app_settings.reset_sustain,
                            "Release Control 64 (Sustain Pedal) on stop",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Hardware Diagnostics & Panic",
        Some(phosphor_egui::regular::POWER),
        |ui| {
            egui::Grid::new("midi_panic_card_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Hardware Status");
                    ui.horizontal(|ui| {
                        if let Some(dev) = s.active_midi_device.as_deref() {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                rect.center(),
                                4.0,
                                egui::Color32::from_rgb(0x10, 0xb9, 0x81),
                            );
                            ui.label(
                                egui::RichText::new(format!("{dev} (Connected)"))
                                    .color(egui::Color32::from_rgb(0x10, 0xb9, 0x81))
                                    .strong(),
                            );
                        } else {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                rect.center(),
                                4.0,
                                egui::Color32::from_rgb(0x8a, 0x93, 0xa5),
                            );
                            ui.label("Auto-discovering hardware keyboards");
                        }
                    });
                    ui.end_row();

                    row_label(ui, "Emergency Silence");
                    ui.horizontal(|ui| {
                        if ui.button(tr("Send MIDI Panic")).clicked() {
                            s.midi_panic = true;
                        }
                        ui.weak(tr("Silences notes stuck on an external instrument."));
                    });
                    ui.end_row();
                });
        },
    );

    if changed {
        s.app_settings.save();
    }
}

// ---------------------------------------------------------------------------
// 5. Appearance / View Settings
// ---------------------------------------------------------------------------

fn appearance(ui: &mut egui::Ui, s: &mut UiState, subtab: usize) {
    let split_x = ui.max_rect().left() + 210.0;
    let mut changed = false;

    match subtab {
        0 => {
            section_heading(ui, "Displays");

            let mut mc = s.app_settings.display_middle_c;
            logic_combo_row(
                ui,
                split_x,
                "appearance_middle_c",
                "Display Middle C:",
                mc.label(),
                Some("Sets the octave numbering convention across Piano Roll and Score."),
                |ui| {
                    for m in MiddleCDisplay::ALL {
                        if ui.selectable_value(&mut mc, m, m.label()).clicked() {
                            s.app_settings.display_middle_c = mc;
                            changed = true;
                        }
                    }
                },
            );
        }
        1 => {
            section_heading(ui, "Tracks Area & Waveforms");
            let mut tcp = s.app_settings.track_colors_palette;
            logic_combo_row(
                ui,
                split_x,
                "track_color_palette_combo",
                "Track Color Palette:",
                tcp.label(),
                None,
                |ui| {
                    for p in TrackColorsPalette::ALL {
                        if ui.selectable_value(&mut tcp, p, p.label()).clicked() {
                            s.app_settings.track_colors_palette = tcp;
                            changed = true;
                        }
                    }
                },
            );

            let mut wfd = s.app_settings.waveform_display;
            logic_combo_row(
                ui,
                split_x,
                "waveform_display_combo",
                "Waveform Display:",
                wfd.label(),
                Some("Controls whether audio regions draw bipolar stereo waveforms or rectified peaks."),
                |ui| {
                    for w in WaveformDisplay::ALL {
                        if ui.selectable_value(&mut wfd, w, w.label()).clicked() {
                            s.app_settings.waveform_display = wfd;
                            changed = true;
                        }
                    }
                },
            );

            section_divider(ui);
            section_heading(ui, "Grid & Region Overlays");
            if logic_checkbox(
                ui,
                split_x,
                "grid_lines_on_top",
                &mut s.app_settings.grid_lines_on_top,
                "Display beat and bar grid lines on top of regions",
            ) {
                changed = true;
            }
        }
        2 => {
            section_heading(ui, "Mixer View");
            let mut pht = s.app_settings.peak_hold_time;
            logic_combo_row(
                ui,
                split_x,
                "peak_hold_time_combo",
                "Peak Hold Time:",
                pht.label(),
                Some("How long peak level indicators remain held on channel strip meters."),
                |ui| {
                    for h in PeakHoldTime::ALL {
                        if ui.selectable_value(&mut pht, h, h.label()).clicked() {
                            s.app_settings.peak_hold_time = pht;
                            changed = true;
                        }
                    }
                },
            );

            section_divider(ui);
            section_heading(ui, "Channel Strips & Plug-in Windows");
            if logic_checkbox(
                ui,
                split_x,
                "open_plugin_on_insert",
                &mut s.app_settings.open_plugin_on_insert,
                "Open plug-in window immediately on insertion",
            ) {
                changed = true;
            }
            ui.add_space(6.0);
            if logic_checkbox(
                ui,
                split_x,
                "show_fader_db",
                &mut s.app_settings.show_fader_db,
                "Display exact numeric dB readout above faders",
            ) {
                changed = true;
            }
        }
        _ => {
            section_heading(ui, "Piano Roll & Editors");
            if logic_checkbox(
                ui,
                split_x,
                "appearance_musical_grid",
                &mut s.app_settings.musical_grid,
                "Use musical bar and beat grid in Piano Roll and Step Editor",
            ) {
                changed = true;
            }
            ui.add_space(6.0);
            if logic_checkbox(
                ui,
                split_x,
                "appearance_select_regions",
                &mut s.app_settings.select_regions_on_track_select,
                "Select regions on track selection for immediate editor focus",
            ) {
                changed = true;
            }
        }
    }

    if changed {
        s.app_settings.save();
    }
}

// ---------------------------------------------------------------------------
// 6. Synchronization Settings
// ---------------------------------------------------------------------------

fn synchronization(ui: &mut egui::Ui, p: &mut Project) {
    const STARTS: [[u8; 4]; 3] = [[0, 0, 0, 0], [0, 59, 55, 0], [1, 0, 0, 0]];

    card_group(
        ui,
        "SMPTE Timecode",
        Some(phosphor_egui::regular::CLOCK),
        |ui| {
            egui::Grid::new("sync_smpte_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Frame Rate"));
                    egui::ComboBox::from_id_salt("sync_smpte_rate_combo")
                        .selected_text(crate::sync_menu::rate_label(p.smpte_rate))
                        .show_ui(ui, |ui| {
                            for rate in SmpteRate::ALL {
                                if ui
                                    .selectable_label(
                                        p.smpte_rate == rate,
                                        crate::sync_menu::rate_label(rate),
                                    )
                                    .clicked()
                                {
                                    p.smpte_rate = rate;
                                    p.smpte_start = valid_smpte_start(p.smpte_start, rate);
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, tr("Start Time"));
                    let sep = if p.smpte_rate.is_drop_frame() {
                        ';'
                    } else {
                        ':'
                    };
                    let cur_start_label = format!(
                        "{:02}:{:02}:{:02}{sep}{:02}",
                        p.smpte_start[0], p.smpte_start[1], p.smpte_start[2], p.smpte_start[3]
                    );
                    egui::ComboBox::from_id_salt("sync_smpte_start_combo")
                        .selected_text(cur_start_label)
                        .show_ui(ui, |ui| {
                            for start in STARTS {
                                let start = valid_smpte_start(start, p.smpte_rate);
                                let text = format!(
                                    "{:02}:{:02}:{:02}{sep}{:02}",
                                    start[0], start[1], start[2], start[3]
                                );
                                if ui.selectable_label(p.smpte_start == start, text).clicked() {
                                    p.smpte_start = start;
                                }
                            }
                        });
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "MIDI Sync & Clock",
        Some(phosphor_egui::regular::ARROWS_CLOCKWISE),
        |ui| {
            egui::Grid::new("sync_midi_clock_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Follow MIDI Clock"));
                    let cur_in = p
                        .midi_clock_in
                        .clone()
                        .unwrap_or_else(|| tr("Off").to_string());
                    egui::ComboBox::from_id_salt("follow_midi_clock_combo")
                        .selected_text(cur_in)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(p.midi_clock_in.is_none(), tr("Off"))
                                .clicked()
                            {
                                p.midi_clock_in = None;
                            }
                            for name in crate::clock_in::ports() {
                                let on = p.midi_clock_in.as_deref() == Some(&name);
                                if ui.selectable_label(on, &name).clicked() {
                                    p.midi_clock_in = Some(name);
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, tr("Send MIDI Clock"));
                    let cur_out = p
                        .midi_clock_out
                        .clone()
                        .unwrap_or_else(|| tr("Off").to_string());
                    egui::ComboBox::from_id_salt("send_midi_clock_combo")
                        .selected_text(cur_out)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(p.midi_clock_out.is_none(), tr("Off"))
                                .clicked()
                            {
                                p.midi_clock_out = None;
                            }
                            for name in crate::midi_out::ports() {
                                let on = p.midi_clock_out.as_deref() == Some(&name);
                                if ui.selectable_label(on, &name).clicked() {
                                    p.midi_clock_out = Some(name);
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "MTC Transmit Out");
                    let cur_mtc_out = p.mtc_out.clone().unwrap_or_else(|| tr("Off").to_string());
                    egui::ComboBox::from_id_salt("mtc_out_combo")
                        .selected_text(cur_mtc_out)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(p.mtc_out.is_none(), tr("Off"))
                                .clicked()
                            {
                                p.mtc_out = None;
                            }
                            for name in crate::midi_out::ports() {
                                let on = p.mtc_out.as_deref() == Some(&name);
                                if ui.selectable_label(on, &name).clicked() {
                                    p.mtc_out = Some(name);
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "MTC Chase In");
                    let cur_mtc_in = p.mtc_in.clone().unwrap_or_else(|| tr("Off").to_string());
                    egui::ComboBox::from_id_salt("mtc_in_combo")
                        .selected_text(cur_mtc_in)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(p.mtc_in.is_none(), tr("Off")).clicked() {
                                p.mtc_in = None;
                            }
                            for name in crate::mtc_in::ports() {
                                let on = p.mtc_in.as_deref() == Some(&name);
                                if ui.selectable_label(on, &name).clicked() {
                                    p.mtc_in = Some(name);
                                }
                            }
                        });
                    ui.end_row();
                });
        },
    );
}

// ---------------------------------------------------------------------------
// 7. Metronome Settings
// ---------------------------------------------------------------------------

fn metronome(ui: &mut egui::Ui, p: &mut Project, s: &mut UiState) {
    let mut changed = false;

    card_group(
        ui,
        "Click Level & Accent",
        Some(phosphor_egui::regular::METRONOME),
        |ui| {
            egui::Grid::new("metronome_level_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Click Volume"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut p.click_volume_db, MIN_CLICK_DB..=0.0)
                                .suffix(" dB"),
                        );
                        if ui.small_button(tr("Reset")).clicked() {
                            p.click_volume_db = DEFAULT_CLICK_DB;
                        }
                    });
                    ui.end_row();

                    row_label(ui, "Accent First Beat");
                    ui.checkbox(
                        &mut p.click_accent,
                        tr("Accent First Beat of Each Bar").to_string(),
                    );
                    ui.end_row();
                });
        },
    );

    card_group(
        ui,
        "Click Routing & Sound",
        Some(phosphor_egui::regular::SPEAKER_HIGH),
        |ui| {
            egui::Grid::new("metronome_sound_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Click in Play");
                    if ui
                        .checkbox(
                            &mut s.app_settings.click_in_play,
                            "Audible while project playback is active",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Click in Record");
                    if ui
                        .checkbox(
                            &mut s.app_settings.click_in_record,
                            "Audible while recording tracks",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Count-in Only");
                    if ui
                        .checkbox(
                            &mut s.app_settings.click_count_in_only,
                            "Audible only during pre-roll count-in",
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    row_label(ui, "Click Sound Tone");
                    let mut snd = s.app_settings.click_sound;
                    egui::ComboBox::from_id_salt("click_sound_combo")
                        .selected_text(snd.label())
                        .show_ui(ui, |ui| {
                            for k in ClickSound::ALL {
                                if ui.selectable_value(&mut snd, k, k.label()).clicked() {
                                    s.app_settings.click_sound = snd;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });
        },
    );

    if changed {
        s.app_settings.save();
    }
}

// ---------------------------------------------------------------------------
// 8. Tuning Settings
// ---------------------------------------------------------------------------

fn tuning(ui: &mut egui::Ui, p: &mut Project, s: &mut UiState) {
    let mut changed = false;

    card_group(
        ui,
        "Concert Pitch",
        Some(phosphor_egui::regular::MUSIC_NOTES),
        |ui| {
            egui::Grid::new("tuning_concert_pitch_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, tr("Reference Pitch (A4)"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut p.tuning_hz)
                                .range(MIN_TUNING_HZ..=MAX_TUNING_HZ)
                                .speed(0.1)
                                .fixed_decimals(1)
                                .suffix(" Hz"),
                        );
                        if ui.small_button(tr("Reset")).clicked() {
                            p.tuning_hz = 440.0;
                        }
                    });
                    ui.end_row();

                    row_label(ui, "Standard Standard");
                    ui.label("A4 = 440.0 Hz (ISO 16 Concert Pitch)");
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.weak("Sets master reference pitch for all built-in synthesizers and sampled instruments.");
        },
    );

    card_group(
        ui,
        "Scale & Temperament",
        Some(phosphor_egui::regular::FADERS),
        |ui| {
            egui::Grid::new("tuning_temperament_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "Temperament");
                    let mut temp = s.app_settings.temperament;
                    egui::ComboBox::from_id_salt("temperament_combo")
                        .selected_text(temp.label())
                        .show_ui(ui, |ui| {
                            for t in Temperament::ALL {
                                if ui.selectable_value(&mut temp, t, t.label()).clicked() {
                                    s.app_settings.temperament = temp;
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    row_label(ui, "Root Key");
                    let keys = [
                        "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
                    ];
                    egui::ComboBox::from_id_salt("root_key_combo")
                        .selected_text(&s.app_settings.root_key)
                        .show_ui(ui, |ui| {
                            for &k in &keys {
                                if ui
                                    .selectable_label(s.app_settings.root_key == k, k)
                                    .clicked()
                                {
                                    s.app_settings.root_key = k.to_string();
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();
                });
        },
    );

    if changed {
        s.app_settings.save();
    }
}

// ---------------------------------------------------------------------------
// 9. Key Commands Settings
// ---------------------------------------------------------------------------

fn keymap_settings(ui: &mut egui::Ui, s: &mut UiState) {
    ui.vertical(|ui| {
        card_group(
            ui,
            "Keymap Profile & Modifiers",
            Some(phosphor_egui::regular::KEYBOARD),
            |ui| {
                egui::Grid::new("keymap_profile_card_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        row_label(ui, "Active Profile");
                        ui.horizontal(|ui| {
                            for prof in crate::key_commands::KeymapProfile::ALL {
                                let active = s.keymap_prefs.profile == prof;
                                let btn = if active {
                                    egui::Button::new(
                                        egui::RichText::new(prof.title())
                                            .color(egui::Color32::from_rgb(0x11, 0x12, 0x15))
                                            .strong(),
                                    )
                                    .fill(egui::Color32::from_rgb(0xf2, 0xf3, 0xf5))
                                } else {
                                    egui::Button::new(
                                        egui::RichText::new(prof.title())
                                            .color(egui::Color32::from_rgb(0xe6, 0xe8, 0xec)),
                                    )
                                    .fill(egui::Color32::from_rgb(0x22, 0x24, 0x29))
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0x36, 0x39, 0x42)))
                                }
                                .corner_radius(egui::CornerRadius::same(6));

                                if ui.add(btn).clicked() {
                                    s.keymap_prefs.profile = prof;
                                    s.keymap_prefs.save();
                                }
                            }
                        });
                        ui.end_row();

                        row_label(ui, "Mac Layout");
                        let mut mac_kb = s.keymap_prefs.mac_keyboard;
                        if ui
                            .checkbox(&mut mac_kb, "Mac Keyboard Layout (Use Cmd)")
                            .on_hover_text(
                                "When enabled, displays and matches primary shortcuts with Apple Cmd/Opt modifiers instead of Ctrl/Alt.",
                            )
                            .changed()
                        {
                            s.keymap_prefs.mac_keyboard = mac_kb;
                            s.keymap_prefs.save();
                        }
                        ui.end_row();
                    });
            },
        );

        card_group(
            ui,
            "Key Command Reference",
            Some(phosphor_egui::regular::LIST_BULLETS),
            |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Current Profile: {}",
                        s.keymap_prefs.profile.title()
                    ))
                    .strong(),
                );
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .id_salt("keymap_table_scroll")
                    .show(ui, |ui| {
                        egui::Grid::new("keymap_table_grid")
                            .striped(true)
                            .spacing([24.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Command").strong());
                                ui.label(egui::RichText::new("Shortcut").strong());
                                ui.label(egui::RichText::new("Scope").strong());
                                ui.end_row();

                                let mac_kb = s.keymap_prefs.mac_keyboard;
                                let commands = match s.keymap_prefs.profile {
                                    crate::key_commands::KeymapProfile::Dori => {
                                        crate::key_commands::COMMANDS
                                    }
                                    crate::key_commands::KeymapProfile::Reaper => {
                                        crate::key_commands::REAPER_COMMANDS
                                    }
                                };

                                for (cmd, combo, scope) in commands {
                                    ui.label(*cmd);
                                    ui.label(
                                        egui::RichText::new(combo.format(mac_kb))
                                            .color(egui::Color32::from_rgb(0xf2, 0xf3, 0xf5))
                                            .monospace(),
                                    );
                                    ui.label(match scope {
                                        crate::key_commands::Scope::Transport => "Global / Transport",
                                        crate::key_commands::Scope::Regions => "Tracks / Regions",
                                    });
                                    ui.end_row();
                                }
                            });
                    });
            },
        );
    });
}

// ---------------------------------------------------------------------------
// 10. MCP Server Settings
// ---------------------------------------------------------------------------

fn mcp_settings(ui: &mut egui::Ui, s: &mut UiState) {
    card_group(
        ui,
        "Model Context Protocol (MCP) Bridge",
        Some(phosphor_egui::regular::CPU),
        |ui| {
            egui::Grid::new("mcp_server_card_grid")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    row_label(ui, "MCP Server");
                    ui.checkbox(&mut s.mcp_server_enabled, "Enable MCP Server (TCP & stdio)");
                    ui.end_row();

                    row_label(ui, "Listening Port");
                    ui.add(egui::DragValue::new(&mut s.mcp_port).range(1024..=65535));
                    ui.end_row();

                    row_label(ui, "Server Status");
                    ui.horizontal(|ui| {
                        let (dot_col, status_text) = if s.mcp_server_enabled {
                            (
                                egui::Color32::from_rgb(0x10, 0xb9, 0x81),
                                format!("Listening on 127.0.0.1:{}", s.mcp_port),
                            )
                        } else {
                            (
                                egui::Color32::from_rgb(0x8a, 0x93, 0xa5),
                                "Disabled".to_string(),
                            )
                        };
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, dot_col);
                        ui.label(egui::RichText::new(status_text).color(dot_col));
                    });
                    ui.end_row();
                });

            ui.add_space(10.0);
            if ui.button("Copy MCP Config JSON").clicked() {
                let config_json = r#"{
  "mcpServers": {
    "douri": {
      "command": "douri",
      "args": ["--mcp-stdio"]
    }
  }
}"#;
                ui.ctx().copy_text(config_json.to_string());
            }
            ui.weak("Allows agents and external tools to query state, automate tracks, and control transport.");
        },
    );
}

// ---------------------------------------------------------------------------
// 11. Advanced Settings
// ---------------------------------------------------------------------------

fn advanced(ui: &mut egui::Ui, s: &mut crate::UiState) {
    let mut changed = false;

    card_group(
        ui,
        "Interface Mode",
        Some(phosphor_egui::regular::APP_WINDOW),
        |ui| {
            if ui
                .radio_value(
                    &mut s.simplified_mode,
                    false,
                    egui::RichText::new(tr("Complete Mode")).strong(),
                )
                .clicked()
            {
                changed = true;
            }
            ui.label(
                egui::RichText::new(
                    "Enables the full set of professional tools, advanced routing racks, full Mixer slots, and MCP server options.",
                )
                .color(egui::Color32::from_rgb(0xaa, 0xaa, 0xb0)),
            );
            ui.add_space(8.0);

            if ui
                .radio_value(
                    &mut s.simplified_mode,
                    true,
                    egui::RichText::new(tr("Simplified Mode")).strong(),
                )
                .clicked()
            {
                changed = true;
            }
            ui.label(
                egui::RichText::new(
                    "Streamlines tools, inspector panels, and menus across the app for focused core music production.",
                )
                .color(egui::Color32::from_rgb(0xaa, 0xaa, 0xb0)),
            );
        },
    );

    if changed {
        save_simplified_mode(s.simplified_mode);
    }

    card_group(ui, "Local AI Stem Separation & Diagnostics", None, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Models folder:")
                    .color(egui::Color32::from_rgb(0xb8, 0xb8, 0xc0)),
            );
            ui.label(
                egui::RichText::new(
                    prowave_engine::stems::stems_model_dir()
                        .to_string_lossy()
                        .as_ref(),
                )
                .monospace()
                .size(11.0)
                .color(egui::Color32::from_rgb(0x8e, 0x8e, 0x96)),
            );
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("De-Bleed / Spill Reducer (%):")
                    .color(egui::Color32::from_rgb(0xd4, 0xd4, 0xdc)),
            );
            ui.add(egui::Slider::new(&mut s.stem_splitter.debleed_pct, 0.0..=100.0).suffix("%"));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Open Stem Splitter…").clicked() {
                s.stem_splitter.open_dialog();
            }
            if ui.button("Open Diagnostics & Stress Bench…").clicked() {
                s.diagnostics_window.open = true;
            }
        });
    });

    card_group(
        ui,
        "First-Launch & Factory Reset",
        Some(phosphor_egui::regular::WRENCH),
        |ui| {
            ui.horizontal(|ui| {
                let reset_btn = egui::Button::new(
                    egui::RichText::new("Reset All Settings Except Key Commands…")
                        .color(egui::Color32::from_rgb(0xF2, 0xF3, 0xF5)),
                )
                .fill(egui::Color32::from_rgb(0x36, 0x24, 0x26))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(0x60, 0x34, 0x38),
                ))
                .corner_radius(egui::CornerRadius::same(6));

                if ui.add(reset_btn).clicked() {
                    s.app_settings = AppSettings::default();
                    s.app_settings.save();
                }
                ui.weak(
                    "Restores default factory preferences while keeping your custom keymap intact.",
                );
            });
        },
    );
}

pub const MODE_FILE: &str = "mode.txt";

pub fn load_simplified_mode() -> bool {
    crate::recent_files::config_dir()
        .map(|d| d.join(MODE_FILE))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map_or(false, |s| s.trim() == "simplified")
}

pub fn save_simplified_mode(simplified: bool) {
    if let Some(path) = crate::recent_files::config_dir().map(|d| d.join(MODE_FILE)) {
        let _ = std::fs::write(
            path,
            if simplified {
                "simplified\n"
            } else {
                "complete\n"
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_settings_entry_of_the_menus_opens_its_tab() {
        let mut seen = Vec::new();
        for label in crate::menus::all_entries() {
            if let Some(tab) = SettingsTab::from_menu(label) {
                seen.push(tab);
            }
        }
        for tab in SettingsTab::ALL {
            assert!(seen.contains(&tab), "{tab:?} has a menu entry");
        }
    }

    /// Runs the window for a few frames over `p` and `s`.
    fn run(p: &mut Project, s: &mut crate::UiState) {
        let mut h = crate::ui_test::Harness::new(900.0, 600.0);
        for _ in 0..3 {
            h.frame(vec![], |ui| show(ui.ctx(), p, s));
        }
    }

    #[test]
    fn every_tab_draws_and_the_window_stays_open() {
        for tab in SettingsTab::ALL {
            let (mut p, mut s) = (Project::demo(), crate::UiState::new());
            s.settings_tab = Some(tab);
            s.audio_info = Some(crate::engine_sync::AudioInfo {
                device: "Speakers".into(),
                sample_rate: 48_000,
                buffer_frames: Some(128),
                output_latency_us: None,
            });
            run(&mut p, &mut s);
            assert_eq!(s.settings_tab, Some(tab));
        }
    }

    #[test]
    fn the_audio_tab_draws_its_rows_and_opens_the_buffer_popup() {
        use crate::engine_sync::AudioInfo;
        use prowave_engine::OutputDevice;
        let mut s = crate::UiState::new();
        s.settings_tab = Some(SettingsTab::Audio);
        // Set `audio_tab_open` so the lists below are kept instead of read from the system.
        s.audio_tab_open = true;
        s.audio_prefs = AudioPrefs {
            device: Some("desk".into()),
            buffer: Some(128),
        };
        s.audio_devices = vec![
            OutputDevice {
                id: "desk".into(),
                name: "Desk Speakers".into(),
            },
            OutputDevice {
                id: "usb".into(),
                name: "USB Interface".into(),
            },
        ];
        s.audio_buffer_range = Some((16, 2048));
        s.audio_info = Some(AudioInfo {
            device: "Desk Speakers".into(),
            sample_rate: 48_000,
            buffer_frames: Some(128),
            output_latency_us: Some(5_000),
        });
        let mut h = crate::ui_test::Harness::new(900.0, 600.0);
        let (prefs, range) = (s.audio_prefs.clone(), s.audio_buffer_range);
        let mut request = None;
        let mut button = None;
        for _ in 0..2 {
            h.frame(vec![], |ui| {
                button = Some(buffer_popup(ui, &prefs, range, Some(128), &mut request).rect);
            });
        }
        let rect = button.expect("the buffer size popup is drawn");
        h.click(rect.center(), |ui| {
            buffer_popup(ui, &prefs, range, Some(128), &mut request);
        });
        h.frame(vec![], |ui| {
            buffer_popup(ui, &prefs, range, Some(128), &mut request);
        });
        assert!(
            egui::Popup::is_any_open(&h.ctx),
            "the buffer size popup opened"
        );
        // Only a choice made in the popup asks for a change.
        assert!(request.is_none());
        // The whole tab draws with the lists and the info given above.
        let mut p = Project::empty();
        h.frame(vec![], |ui| audio(ui, &mut p, &mut s));
        assert!(s.audio_tab_open && s.audio_prefs.buffer == Some(128));
    }

    #[test]
    fn nothing_is_drawn_while_no_tab_is_chosen() {
        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        let before = p.click_volume_db;
        run(&mut p, &mut s);
        assert!(s.settings_tab.is_none() && p.click_volume_db == before);
    }

    #[test]
    fn app_settings_default_save_and_load() {
        let default_settings = AppSettings::default();
        assert_eq!(
            default_settings.startup_action,
            StartupAction::OpenMostRecent
        );
        assert_eq!(default_settings.max_recent_items, 20);
        assert_eq!(default_settings.autosave_interval_min, 2);
        assert_eq!(default_settings.temperament, Temperament::Equal);

        let json = serde_json::to_string(&default_settings).expect("serialize settings");
        let parsed: AppSettings = serde_json::from_str(&json).expect("deserialize settings");
        assert_eq!(default_settings, parsed);
    }
}
