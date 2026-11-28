#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(unused, dead_code, clippy::all)]
//! Pro Wave desktop app. Pure Rust (egui/eframe) for Windows, Linux and macOS.
// Debug builds abort if the audio callback allocates (see `prowave-engine` output.rs).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC_GUARD: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

mod aaf;
mod apple_loops;
pub mod arrange_nav;
mod arrange_select;
pub mod arrange_zoom;
mod audio_prefs;
mod audio_rec;
mod automation;
mod automation_ui;
mod autosave;
mod bass808_ui;
mod bench;
mod bounce;
mod clock_follow;
mod clock_in;
mod collaborate_panel;
pub mod command_palette;
mod control;
mod count_in;
mod crash_log;
pub mod cycle_area;
mod daw_knob;
pub mod diagnostics;
mod drag_modes;
mod drum_kit;
mod drum_machine_ui;
mod eclipsa_ui;
mod engine_sync;
mod event_list;
mod fade_curve;
mod fade_tool;
mod file_browser;
mod file_commands;
mod flex_edit;
mod free_plugins;
mod fx_ui;
mod garageband;
mod graphics;
mod groove;
mod group_editor;
mod groups;
mod hosted_plugins;
pub mod hw;
pub mod hw_vocal;
mod hydrogen_archive;
mod i18n;
mod icons;
mod import;
mod inspector;
mod ir_files;
pub mod key_commands;
mod library;
pub mod liquid_glass;
mod logging;
mod logicx;
mod logicx_import;
pub mod lyrics;
pub mod lyrics_ui;
mod macro_controls;
mod marbble_panel_host;
pub mod marker_edit;
pub mod marker_number;
mod marker_track;
mod marquee;
pub mod mcp_server;
mod menus;
mod midi_file;
mod midi_fx;
mod midi_fx_ui;
mod midi_in;
mod midi_out;
mod mixer;
mod model;
pub mod modulation_drawer;
mod mtc_in;
mod multimeter;
mod musical_typing;
mod native_menu;
mod new_track_dialog;
mod note_edit;
mod patches;
mod pattern;
mod piano_roll;
mod piano_tools;
pub mod pipewire_jack;
mod pitch_editor;
mod plugin_manager;
mod plugin_picker;
mod plugin_ratings;
mod plugins;
mod project_io;
mod project_settings;
mod quick_help;
mod recent_files;
mod recorder;
mod recovery;
mod region_colors;
mod region_commands;
mod region_edit;
mod region_solo;
mod rex_import;
mod sampler_load;
pub mod sampler_ui;
mod score;
mod score_export;
mod score_import;
mod scripter;
mod section_edit;
mod select_similar;
mod session_player;
mod session_player_ui;
mod shuffle_edit;
mod side_panes;
mod signature_track;
mod smart_tempo;
mod snap_grid;
pub mod sound_library;
mod soundfonts;
mod space_designer_ui;
pub mod spectrogram_view;
mod stack_dialog;
pub mod start_screen;
pub mod stems_ui;
mod step_editor;
mod sync_menu;
mod sysex;
mod take_pack;
mod takes;
pub mod template;
mod tempo_track;
mod theme;
pub mod timeline_viewport;
mod tooltips;
pub mod touchpad_guard;
mod track_commands;
mod track_dialogs;
mod track_drop_slice;
mod track_icons;
mod track_selection;
mod tracks_area;
mod transform;
mod transform_ui;
mod transport;
pub mod ui_chrome;
mod ui_scale;
#[cfg(test)]
mod ui_test;
mod undo;
mod undo_history;
mod updater;
mod user_loops;
mod vamp;
mod widgets;
mod window_chrome;

pub const APP_NAME: &str = "Dōri";

pub fn about_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
pub fn window_title() -> String {
    APP_NAME.to_string()
}

use model::Project;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bottom {
    Mixer,
    PianoRoll,
    /// The Pitch Editor (Melodyne/Logic Flex Pitch note editor for audio regions).
    PitchEditor,
    /// Macro Controls 8-knob Moog-style control surface and inline EQ.
    MacroControls,
    /// Revamped Session Player Editor (Keyboard, Bass, Drummer).
    SessionPlayer,
}

pub struct UiState {
    pub show_mixer: bool,
    pub show_library: bool,
    pub show_inspector: bool,
    /// Whether the top bar LCD displays SMPTE timecode (true) or Bars/Beats (false).
    pub lcd_smpte: bool,
    /// Control Bar LCD display mode (Beats & Project, Beats & Time, Beats, Time, Custom).
    pub lcd_mode: project_settings::LcdMode,
    /// Customizable LCD items under Custom display mode.
    pub lcd_custom: project_settings::LcdCustomOptions,
    /// Whether the LCD display mode popup menu is open.
    pub show_lcd_menu: bool,
    /// Whether the "Customize Control Bar and Display" dialog is open.
    pub show_lcd_customize: bool,
    /// Live MIDI In activity indicator (0.0..1.0, decays over time).
    pub midi_in_activity: f32,
    /// Live MIDI Out activity indicator (0.0..1.0, decays over time).
    pub midi_out_activity: f32,
    /// List Editors (the Event list on the right).
    pub show_lists: bool,
    pub lists: event_list::ListView,
    /// Collaborate Version History & Project Alternatives panel.
    pub show_collaborate: bool,
    pub command_palette: command_palette::CommandPaletteState,
    pub modulation_drawer: modulation_drawer::ModulationDrawerState,
    pub diagnostics_window: diagnostics::DiagnosticsWindowState,
    pub git_ab_audition: bool,
    pub stem_regions: std::collections::HashMap<(usize, usize), stems_ui::StemRegionState>,
    pub stem_splitter: stems_ui::StemSplitterState,
    pub collaborate_enabled: bool,
    pub collaborate_worker: Option<prowave_vcs::VcsWorker>,
    pub collaborate_note_input: String,
    pub collaborate_alt_input: String,
    pub collaborate_creating_alt: bool,
    pub collaborate_link_input: String,
    pub collaborate_showing_link_modal: bool,
    pub collaborate_revert_confirm: bool,
    pub collaborate_trigger_snapshot: bool,
    pub collaborate_branch_from_rev: Option<String>,
    /// The MIDI recording in progress.
    pub rec: recorder::Recorder,
    /// The audio recording in progress, and the open audio input.
    pub audio_rec: audio_rec::AudioRecorder,
    /// Capture Recording was asked for.
    pub capture_cmd: bool,
    pub show_quick_help: bool,
    pub show_quick_help_window: bool,
    pub quick_help_query: String,
    pub show_toolbar: bool,
    pub show_macro_window: bool,
    pub macro_controls: macro_controls::MacroControlsView,
    /// The Note Pad.
    pub show_notes: bool,
    /// The Loop Browser and All Files.
    pub browser: file_browser::BrowserView,
    /// Files to add to the project, from the browsers.
    pub open_paths: Vec<std::path::PathBuf>,
    /// Help > Check for Updates… was chosen.
    pub update_check: bool,
    /// Help > Check for Updates Automatically.
    pub update_auto: bool,
    pub bottom: Bottom,
    /// Last import error or engine warning, shown in the menu bar.
    pub status: Option<String>,
    pub status_at: Option<std::time::Instant>,
    pub status_last_msg: Option<String>,
    pub tracks: tracks_area::TracksView,
    /// The region Edit > Cut / Copy put on the clipboard.
    pub region_clip: region_edit::RegionClip,
    pub mixer: mixer::MixerView,
    /// What Mute and Solo were last frame, so mix groups follow (see `groups.rs`).
    pub group_watch: groups::GroupWatch,
    pub inspector: inspector::InspectorView,
    pub library: library::LibraryView,
    pub piano: piano_roll::PianoView,
    /// The MIDI FX window of the selected track is open.
    pub show_midi_fx: bool,
    /// The Project Settings window, with the tab it shows.
    pub settings_tab: Option<project_settings::SettingsTab>,
    /// Global application and project preferences matching Logic Pro settings.
    pub app_settings: project_settings::AppSettings,
    /// A Track menu window (rename, colour, icon, header), when one is open.
    pub track_dialog: Option<track_dialogs::TrackDialog>,
    /// Logic's Create New Track dialog (the + above the track headers).
    pub new_track: new_track_dialog::NewTrackDialog,
    /// Track > Create Track Stack…: the dialog choosing the kind of stack.
    pub stack_dialog: stack_dialog::StackDialog,
    /// The audio output the engine plays on, for Project Settings > Audio.
    pub audio_info: Option<engine_sync::AudioInfo>,
    /// The output device and buffer size the user chose (remembered in `audio.txt`).
    pub audio_prefs: audio_prefs::AudioPrefs,
    /// The keymap profile and keyboard layout preferences (remembered in `keymap.txt`).
    pub keymap_prefs: key_commands::KeymapPrefs,
    /// A new output choice from Project Settings > Audio, carried out by `App` after the window.
    pub audio_request: Option<audio_prefs::AudioPrefs>,
    /// The output devices and the buffer sizes the chosen device takes, read when the Audio
    /// tab is opened.
    pub audio_devices: Vec<prowave_engine::OutputDevice>,
    pub audio_buffer_range: Option<(u32, u32)>,
    /// The Audio tab was shown last frame, so its lists are read once per opening.
    pub audio_tab_open: bool,
    /// Currently connected USB / hardware MIDI device name, for Settings > Audio.
    pub active_midi_device: Option<String>,
    /// Project Settings > MIDI: send MIDI panic on the next frame.
    pub midi_panic: bool,
    /// Event list > SysEx…: the message being typed, while its window is open.
    pub sysex_entry: Option<String>,
    /// The MIDI Transform window.
    pub transform: transform_ui::TransformView,
    /// The editor panel shows the Score instead of the Piano Roll for ordinary MIDI regions.
    pub score_tab: bool,
    pub score: score::ScoreView,
    /// A note the Score just added is sounding: its pitch and when it stops.
    pub score_heard: Option<(u8, f64)>,
    /// Flex Time and Flex Pitch work running in the background.
    pub flex: flex_edit::FlexState,
    /// The audio Groove Track's hits being read.
    pub groove: groove::GrooveState,
    /// File > Import > MIDI File… or Export > Selection as MIDI File…, run after the menus.
    pub midi_cmd: Option<midi_file::MidiCmd>,
    /// File > Export > Score as MusicXML… picked this frame.
    pub export_xml_cmd: bool,
    /// File > Import > MusicXML / Guitar Pro / Hydrogen Song picked this frame.
    pub import_cmd: Option<score_import::ImportCmd>,
    /// Edit > Undo History… picked this frame: the history window is showing.
    pub show_undo_history: bool,
    /// File > Import > Audio File… picked this frame.
    pub import_audio_cmd: bool,
    /// File > Export > Track as Audio File… picked this frame.
    pub export_track_cmd: bool,
    /// File > Export > All Tracks as Audio Files… picked this frame (stem export).
    pub export_all_tracks_cmd: bool,
    /// File > Bounce > Regions in Place… (and Record > Bounce In Place) picked this frame.
    pub bounce_region_cmd: bool,
    /// Edit > Bounce and Join picked this frame.
    pub bounce_join_cmd: bool,
    /// File > Bounce picked this frame; run by `App` (which owns the engine).
    pub bounce_cmd: bool,
    /// A bounce is running (greys the Bounce entry).
    pub bouncing: bool,
    /// File > Import > Other Projects… picked this frame (a GarageBand or Logic Pro bundle).
    pub open_band_cmd: bool,
    /// The Logic Pro Import window, while it is open (see `logicx::show_report`).
    pub logicx_report: Option<logicx::Report>,
    /// File > Show in File Manager picked this frame.
    pub reveal_cmd: bool,
    /// The project has a file on disk (greys the File Manager entry until it is saved).
    pub has_path: bool,
    /// The Pattern and Drummer editors, which take the Piano Roll's place for those regions.
    pub steps: step_editor::StepView,
    /// The Pitch Editor's view of an audio region.
    pub pitch_ed: pitch_editor::PitchView,
    /// True while the pointer is over the Piano Roll (it owns Delete/Cmd+R then).
    pub piano_focus: bool,
    /// Live notes from the Piano Roll, forwarded to the engine every frame.
    pub audition: Vec<piano_roll::Audition>,
    /// File menu command picked this frame; run by `App` once the panels are drawn.
    pub file_cmd: Option<file_commands::FileCmd>,
    /// A New / Open waiting for the answer to the "save changes?" prompt.
    pub ask_save: Option<file_commands::FileCmd>,
    /// True while the "revert to the last saved version?" prompt is showing.
    pub ask_revert: bool,
    /// The project picked in File > Open Recent, until `App` has opened it.
    pub recent_pick: Option<std::path::PathBuf>,
    /// File > Open Recent. In memory only until `main` loads the saved list.
    pub recent: recent_files::Recent,
    /// DSP load shown by the LCD's CPU meter (1.0 = full budget); peak-held, falls slowly.
    pub cpu_load: f32,
    /// Disk streaming load shown by the LCD's HD meter (1.0 = the thread busy all the time).
    pub hd_load: f32,
    /// Light (white) menu bar and menus, toggled by the Smart Controls button.
    pub light_menus: bool,
    /// The Project Chooser, while it covers the window (File > New from Template… or at startup).
    pub chooser: Option<start_screen::Chooser>,
    /// Project Chooser details (Tempo, Key, Time Sig, Audio Devices, etc.).
    pub chooser_details: start_screen::ChooserDetails,
    /// The user's interface size (1.0 = 100 %), on top of fitting small windows (`ui_scale`).
    pub ui_scale: f32,
    /// Edit > Undo / Redo picked this frame; run by `App` (which keeps the history).
    pub undo_cmd: Option<undo::UndoCmd>,
    /// Whether there is something to undo / redo (greys the Edit entries).
    pub can_undo: bool,
    pub can_redo: bool,
    /// Mix > Edit Groups… window is open.
    pub show_groups: bool,
    /// Musical Typing: the computer keyboard as a MIDI keyboard (Window > Show Musical Typing).
    pub typing: musical_typing::MusicalTyping,
    /// Window > Plugin Manager is open.
    pub show_plugin_manager: bool,
    /// The track whose Drum Machine window is open, the selected pad, the pad being held by the
    /// mouse, and why the last sample could not be used.
    pub show_drum_machine: Option<usize>,
    pub drum_pad: usize,
    pub drum_held: Option<usize>,
    pub drum_error: Option<String>,
    /// The track whose 808 Bass window is open.
    pub show_bass808: Option<usize>,
    /// The track whose Marbble Synth / Sampler window is open.
    pub show_marbble_inst: Option<usize>,
    /// MultiMeter analyzer window is open.
    pub show_multimeter: bool,
    /// Eclipsa Audio Master Renderer window is open.
    pub show_eclipsa_renderer: bool,
    /// Eclipsa Track Spatial Panner window is open.
    pub show_eclipsa_panner: bool,
    /// The text in the Plugin Manager's search field, and the manufacturer chosen in its list
    /// (`None`: Show All).
    pub plugin_manager_search: String,
    pub plugin_manager_vendor: Option<String>,
    pub plugin_manager_category: Option<String>,
    pub plugin_manager_format: Option<String>,
    pub plugin_manager_selected_path: Option<usize>,
    /// UI Glass Quality mode (Full Refraction, Frosted Blur, Flat Low-Power).
    pub glass_quality: liquid_glass::GlassQuality,
    /// MCP Server enable toggle.
    pub mcp_server_enabled: bool,
    /// MCP Server listening port.
    pub mcp_port: u16,
    /// Help > About Dōri window is open.
    pub show_about: bool,
    /// Dōri Sound Library Manager window is open.
    pub show_sound_library_manager: bool,
    /// Window > Get Free Plug-ins installer window.
    pub free_plugins: free_plugins::FreePlugins,
    /// Modal opened when dropping audio onto the Track Header gutter.
    pub track_drop_slice: track_drop_slice::TrackDropSliceModal,
    /// Modal opened when dragging a region to the Sound Library / Loops panel.
    pub add_loop_modal: user_loops::AddLoopModal,
    /// Saved tagged User Loops catalog.
    pub user_loops: user_loops::UserLoopsCatalog,
    /// Last rendered Track Header gutter rect for drop detection.
    pub tracks_headers_rect: Option<egui::Rect>,
    /// Last rendered Tracks Area timeline rect for User Loop drop detection.
    pub tracks_timeline_rect: Option<egui::Rect>,
    /// Beat position under the pointer in the Tracks Area timeline.
    pub tracks_drop_beat: Option<f64>,
    /// Track index under the pointer when dragging a file from the browser.
    pub browser_drop_track: Option<usize>,
    /// Pending file drops from the Loop / Files browser with optional `(track, beat)`.
    pub open_drops: Vec<(std::path::PathBuf, Option<(usize, f64)>)>,
    /// Last rendered Sound Library / Loops panel rect for drop detection.
    pub library_panel_rect: Option<egui::Rect>,
    /// Active cross-panel region drag payload mirrored alongside egui DragAndDrop.
    pub dragged_region: Option<track_drop_slice::DraggedRegionPayload>,
    /// Active User Loop drag payload when dragging a saved User Loop from the Library.
    pub dragged_user_loop: Option<user_loops::UserLoopEntry>,
    /// Measured round-trip loopback latency calibration figure.
    pub measured_roundtrip: Option<prowave_engine::calibration::Measured>,
    /// URL to open in external browser.
    pub open_url: Option<String>,
    /// Cached About Dōri app icon texture.
    pub about_icon: Option<egui::TextureHandle>,
    /// Interface mode: Simplified Mode (true) vs Complete Mode (false).
    pub simplified_mode: bool,
    /// Force a top-level menu open for verification screenshots.
    pub shot_top_menu: Option<String>,
    /// Missing audio files detected during project load or recovery.
    pub missing_audio: Vec<String>,
    /// What the Tracks area's track commands remember (`track_commands`).
    pub arrange: track_commands::ArrangeView,
    /// Spectrogram view of audio regions.
    pub spectrogram: spectrogram_view::SpectrogramView,
    /// Automation view state.
    pub auto: automation::AutoView,
    /// Window > Open Piano Roll: the Piano Roll in a window of its own.
    pub piano_window: bool,
}

impl UiState {
    fn new() -> Self {
        Self {
            show_mixer: true,
            show_library: true,
            show_inspector: false,
            lcd_smpte: false,
            lcd_mode: project_settings::LcdMode::default(),
            lcd_custom: project_settings::LcdCustomOptions::default(),
            show_lcd_menu: false,
            show_lcd_customize: false,
            midi_in_activity: 0.0,
            midi_out_activity: 0.0,
            show_lists: false,
            lists: event_list::ListView::default(),
            show_collaborate: false,
            command_palette: command_palette::CommandPaletteState::default(),
            modulation_drawer: modulation_drawer::ModulationDrawerState::default(),
            diagnostics_window: diagnostics::DiagnosticsWindowState::default(),
            git_ab_audition: false,
            stem_regions: std::collections::HashMap::new(),
            stem_splitter: stems_ui::StemSplitterState::default(),
            collaborate_enabled: false,
            collaborate_worker: None,
            collaborate_note_input: String::new(),
            collaborate_alt_input: String::new(),
            collaborate_creating_alt: false,
            collaborate_link_input: String::new(),
            collaborate_showing_link_modal: false,
            collaborate_revert_confirm: false,
            collaborate_trigger_snapshot: false,
            collaborate_branch_from_rev: None,
            rec: recorder::Recorder::default(),
            audio_rec: audio_rec::AudioRecorder::default(),
            capture_cmd: false,
            show_quick_help: false,
            show_quick_help_window: false,
            quick_help_query: String::new(),
            show_toolbar: false,
            show_macro_window: false,
            macro_controls: macro_controls::MacroControlsView::default(),
            show_notes: false,
            browser: file_browser::BrowserView::default(),
            open_paths: Vec::new(),
            update_check: false,
            update_auto: true,
            bottom: Bottom::Mixer,
            status: None,
            status_at: None,
            status_last_msg: None,
            tracks: tracks_area::TracksView::default(),
            region_clip: Vec::new(),
            mixer: mixer::MixerView {
                inputs: audio_rec::InputChoice::load(),
                ..mixer::MixerView::default()
            },
            group_watch: groups::GroupWatch::default(),
            inspector: inspector::InspectorView::default(),
            library: library::LibraryView::default(),
            free_plugins: free_plugins::FreePlugins::default(),
            track_drop_slice: track_drop_slice::TrackDropSliceModal::default(),
            add_loop_modal: user_loops::AddLoopModal::default(),
            user_loops: user_loops::UserLoopsCatalog::default(),
            tracks_headers_rect: None,
            tracks_timeline_rect: None,
            tracks_drop_beat: None,
            browser_drop_track: None,
            open_drops: Vec::new(),
            library_panel_rect: None,
            dragged_region: None,
            dragged_user_loop: None,
            piano: piano_roll::PianoView::default(),
            steps: step_editor::StepView::default(),
            show_midi_fx: false,
            settings_tab: None,
            track_dialog: None,
            new_track: Default::default(),
            stack_dialog: Default::default(),
            audio_info: None,
            audio_prefs: audio_prefs::AudioPrefs::default(),
            keymap_prefs: key_commands::KeymapPrefs::default(),
            audio_request: None,
            audio_devices: Vec::new(),
            audio_buffer_range: None,
            audio_tab_open: false,
            active_midi_device: None,
            midi_panic: false,
            sysex_entry: None,
            transform: transform_ui::TransformView::default(),
            score_tab: false,
            score: score::ScoreView::default(),
            score_heard: None,
            flex: flex_edit::FlexState::default(),
            pitch_ed: pitch_editor::PitchView::default(),
            groove: groove::GrooveState::default(),
            midi_cmd: None,
            export_xml_cmd: false,
            import_cmd: None,
            show_undo_history: false,
            import_audio_cmd: false,
            export_track_cmd: false,
            export_all_tracks_cmd: false,
            bounce_region_cmd: false,
            bounce_join_cmd: false,
            bounce_cmd: false,
            bouncing: false,
            open_band_cmd: false,
            logicx_report: None,
            reveal_cmd: false,
            has_path: false,
            piano_focus: false,
            audition: Vec::new(),
            file_cmd: None,
            ask_save: None,
            ask_revert: false,
            recent_pick: None,
            recent: recent_files::Recent::default(),
            cpu_load: 0.0,
            hd_load: 0.0,
            light_menus: false,
            ui_scale: 1.0,
            undo_cmd: None,
            can_undo: false,
            can_redo: false,
            show_groups: false,
            typing: musical_typing::MusicalTyping::new(),
            show_plugin_manager: false,
            show_drum_machine: None,
            drum_pad: 0,
            drum_held: None,
            drum_error: None,
            show_bass808: None,
            show_marbble_inst: None,
            show_multimeter: false,
            show_eclipsa_renderer: false,
            show_eclipsa_panner: false,
            plugin_manager_search: String::new(),
            plugin_manager_vendor: None,
            plugin_manager_category: None,
            plugin_manager_format: None,
            plugin_manager_selected_path: None,
            glass_quality: liquid_glass::GlassQuality::FullRefraction,
            mcp_server_enabled: false,
            mcp_port: 38420,
            show_about: false,
            show_sound_library_manager: false,
            measured_roundtrip: None,
            open_url: None,
            about_icon: None,
            simplified_mode: false,
            chooser: None,
            chooser_details: start_screen::ChooserDetails::default(),
            app_settings: project_settings::AppSettings::load(),
            shot_top_menu: None,
            missing_audio: Vec::new(),
            arrange: track_commands::ArrangeView::default(),
            spectrogram: spectrogram_view::SpectrogramView::default(),
            auto: automation::AutoView::default(),
            piano_window: false,
        }
    }

    /// Reads the system's output devices and the buffer sizes the chosen one takes.
    pub fn refresh_audio_list(&mut self) {
        self.audio_devices = prowave_engine::output_devices();
        self.audio_buffer_range = prowave_engine::buffer_range(self.audio_prefs.device.as_deref());
    }

    /// After a project is replaced: drop editor state (selections, open regions) that indexes
    /// into the old project, keeping which panels are shown.
    fn reset_views(&mut self) {
        self.status = None;
        self.tracks = tracks_area::TracksView::default();
        self.mixer = mixer::MixerView::default();
        self.group_watch.reset();
        self.inspector = inspector::InspectorView::default();
        self.piano = piano_roll::PianoView::default();
        self.piano_focus = false;
        self.audition.clear();
    }
}

struct App {
    project: Project,
    ui: UiState,
    sync: Option<engine_sync::EngineSync>,
    /// File > Bounce in progress.
    bounce: Option<bounce::Bounce>,
    /// The regions a region bounce will replace when it ends: (track, regions).
    region_bounce: Option<(usize, Vec<usize>)>,
    /// The `.prowave` file the project was opened from or last saved to.
    path: Option<std::path::PathBuf>,
    /// Content at the last open/save, to detect unsaved changes (see `file_commands`).
    saved: prowave_project::ProjectDoc,
    /// macOS: the menus in the system menu bar (elsewhere they are drawn in the window).
    native_menu: Option<native_menu::NativeMenu>,
    /// `--bench <file>`: measuring startup, memory and DSP load, then closing.
    bench: Option<bench::Probe>,
    /// `--layout-shots <folder>`: pictures of the layout at several window sizes, then closing.
    shots: Option<bench::LayoutShots>,
    /// `--screenshot <file>`: one picture of the window, then closing.
    shot: Option<bench::Screenshot>,
    /// The exit code a failed `--screenshot` leaves for `main` (0 while it is fine).
    shot_exit: std::sync::Arc<std::sync::atomic::AtomicI32>,
    /// Edit > Undo / Redo.
    history: undo::History,
    /// Frames since the history last looked for an edit.
    frames_since_record: u32,
    /// The MIDI keyboard (none while benchmarking).
    midi: Option<midi_in::MidiIn>,
    /// Sends the notes of External MIDI tracks to their ports (none while benchmarking).
    midi_out: Option<midi_out::MidiOut>,
    /// Reads MIDI Time Code from the project's port (none while benchmarking).
    mtc_in: Option<mtc_in::MtcIn>,
    /// The input port whose MIDI Clock the song follows, and the follower that reads it.
    clock_in: clock_in::ClockIn,
    clock_follow: clock_follow::ClockFollower,
    /// Makes the transport follow that time code.
    chase: mtc_in::Chase,
    /// Looks for a newer release and installs it (Windows).
    updater: updater::Updater,
    /// MCP Server command bridge and receiver.
    mcp_bridge: Option<mcp_server::McpBridge>,
    control_server: Option<control::ControlServer>,
    touchpad_guard: touchpad_guard::TouchpadGuard,
    mcp_receiver: Option<std::sync::mpsc::Receiver<mcp_server::McpCommand>>,
    pub plugins: plugins::PluginHost,
    pub autosave: autosave::Autosave,
}

impl App {
    pub fn has_unsaved_changes(&self) -> bool {
        crate::file_commands::content(&self.project) != self.saved
    }
    /// Project Settings > Audio chose another output device or buffer size: the engine restarts
    /// on it. Playback stops and the playhead stays where it was; the MIDI sender and the audio
    /// input are started again on the new engine. Refused while recording or bouncing. If the
    /// new output cannot be opened, the current one keeps playing.
    fn restart_audio(&mut self, prefs: audio_prefs::AudioPrefs) {
        if prefs == self.ui.audio_prefs {
            return;
        }
        if self.bounce.is_some()
            || self.region_bounce.is_some()
            || self.ui.rec.active()
            || self.ui.audio_rec.active()
        {
            self.ui.status = Some(
                i18n::tr("Stop the recording or the bounce before changing the audio output.")
                    .to_string(),
            );
            return;
        }
        let engine = match prowave_engine::Engine::start_on(prefs.device.as_deref(), prefs.buffer) {
            Ok(engine) => engine,
            Err(e) => {
                tracing::warn!("audio output change refused: {e}");
                self.ui.status = Some(i18n::trf(
                    "Cannot use that audio output: {}. The current output keeps playing.",
                    &[&e],
                ));
                return;
            }
        };
        // The new engine starts stopped, so the transport stops here too; the playhead is kept.
        if self.project.playing {
            if let Some(old) = self.sync.as_mut() {
                self.project.position_beats = old.engine.state().position_beats;
            }
            self.project.playing = false;
            self.project.counting_in = false;
        }
        // The MIDI sender goes first: its Drop ends its thread before the old engine is dropped.
        self.midi_out = None;
        let mut sync = engine_sync::EngineSync::new(engine, &self.project);
        self.midi_out = start_midi_out(&mut sync);
        if let Some(midi) = self.midi.as_mut() {
            if let Some(tx) = sync.take_live_midi_producer() {
                midi.attach_live_ring(tx, sync.armed_track());
            }
        }
        // Dropping the old engine stops its stream.
        let _ = self.sync.replace(sync);
        // The monitoring feed of an open input went to the old engine: the input is opened again.
        self.ui.audio_rec.forget_input();
        self.ui.audio_info = self.sync.as_mut().map(|s| s.audio_info());
        self.ui.audio_buffer_range = prowave_engine::buffer_range(prefs.device.as_deref());
        self.ui.audio_prefs = prefs;
        self.ui.audio_prefs.save();
        self.ui.audition.clear();
    }

    /// File > Show in File Manager: opens the folder that holds the project file, with the
    /// project selected where the system supports it.
    fn reveal_project(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        if let Err(e) = file_commands::reveal(&path) {
            self.ui.status = Some(format!("{}: {e}", path.display()));
        }
    }

    /// File > Import > MIDI File… and File > Export > Selection as MIDI File…
    fn run_midi_cmd(&mut self, cmd: midi_file::MidiCmd) {
        let (p, s) = (&mut self.project, &mut self.ui);
        match cmd {
            midi_file::MidiCmd::Import => {
                let picked = rfd::FileDialog::new()
                    .add_filter(i18n::tr("MIDI File"), &["mid", "midi", "smf"])
                    .pick_file();
                if let Some(path) = picked {
                    s.status = Some(match midi_file::import_path(p, &path) {
                        Ok(n) => i18n::trf("imported {} tracks from {}", &[&n, &path.display()]),
                        Err(e) => i18n::trf("cannot import {}: {}", &[&path.display(), &e]),
                    });
                }
            }
            midi_file::MidiCmd::Export => {
                // The selected region, or all the MIDI of the project.
                let only = s.tracks.selected.filter(|(t, r)| {
                    p.tracks.get(*t).is_some_and(|tr| {
                        tr.kind != model::TrackKind::Audio && *r < tr.regions.len()
                    })
                });
                let picked = rfd::FileDialog::new()
                    .add_filter(i18n::tr("MIDI File"), &["mid"])
                    .set_file_name(format!("{}.mid", p.name))
                    .save_file();
                if let Some(path) = picked {
                    s.status = Some(match midi_file::export_path(p, only, &path) {
                        Ok(()) => i18n::trf("saved {}", &[&path.display()]),
                        Err(e) => i18n::trf("cannot write {}: {}", &[&path.display(), &e]),
                    });
                }
            }
        }
    }

    fn import(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        if logicx::is_bundle(path) {
            // A Logic Pro project opens as a new project, as File > Open does, after the question
            // about unsaved changes. Open Recent takes the path from `recent_pick`.
            self.ui.recent_pick = Some(path.to_path_buf());
            self.run_file_cmd(file_commands::FileCmd::OpenRecent, ctx);
            return;
        }
        if midi_file::is_midi_path(path) {
            self.ui.status = Some(match midi_file::import_path(&mut self.project, path) {
                Ok(n) => i18n::trf("imported {} tracks from {}", &[&n, &path.display()]),
                Err(e) => i18n::trf("cannot import {}: {}", &[&path.display(), &e]),
            });
            return;
        }
        if logicx_import::is_supported_path(path) {
            self.ui.status = Some(match logicx_import::preview(&mut self.project, path) {
                Ok(status) => {
                    self.history.record(&self.project);
                    status
                }
                Err(e) => i18n::trf("cannot import {}: {}", &[&path.display(), &e]),
            });
            return;
        }
        if rex_import::is_supported_path(path) {
            self.ui.status = Some(match rex_import::import(&mut self.project, path, ctx) {
                Ok(status) => {
                    self.history.record(&self.project);
                    status
                }
                Err(e) => i18n::trf("cannot import {}: {}", &[&path.display(), &e]),
            });
            return;
        }
        if score_import::is_supported_path(path) {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let result = match ext.as_str() {
                "mxl" | "xml" | "musicxml" => {
                    score_import::import_musicxml(&mut self.project, path)
                }
                "gp" => score_import::import_guitarpro(&mut self.project, path),
                _ => score_import::import_hydrogen(&mut self.project, path),
            };
            self.ui.status = Some(match result {
                Ok(n) => {
                    self.history.record(&self.project);
                    i18n::trf("imported {} tracks from {}", &[&n, &path.display()])
                }
                Err(e) => i18n::trf("cannot import {}: {}", &[&path.display(), &e]),
            });
            return;
        }
        if !import::is_audio_path(path) {
            self.ui.status = Some(i18n::trf("not an audio file: {}", &[&path.display()]));
            return;
        }
        if let Err(e) = import::import_audio(&mut self.project, path, ctx) {
            self.ui.status = Some(i18n::trf("cannot import {}: {}", &[&path.display(), &e]));
        }
    }

    /// File > Export > Score as MusicXML…: asks where to write and exports every track's notes.
    fn export_score(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MusicXML", &["musicxml", "xml"])
            .set_file_name(format!("{}.musicxml", self.project.name))
            .save_file()
        else {
            return;
        };
        self.ui.status = Some(match score_export::export(&self.project, &path) {
            Ok(notes) => i18n::trf("exported {} notes to {}", &[&notes, &path.display()]),
            Err(e) => e,
        });
    }

    /// File > Import entries handled by the format-specific readers.
    fn run_import_cmd(&mut self, cmd: score_import::ImportCmd, ctx: &egui::Context) {
        use score_import::ImportCmd;
        let path = match cmd {
            ImportCmd::LogicX => rfd::FileDialog::new()
                .set_title(i18n::tr("LogicX Project (Preview)…"))
                .pick_folder(),
            ImportCmd::MusicXml => rfd::FileDialog::new()
                .set_title(i18n::tr("MusicXML Score…"))
                .add_filter(i18n::tr("MusicXML"), &["musicxml", "xml", "mxl"])
                .pick_file(),
            ImportCmd::GuitarPro => rfd::FileDialog::new()
                .set_title(i18n::tr("Guitar Pro…"))
                .add_filter(i18n::tr("Guitar Pro"), &["gp"])
                .pick_file(),
            ImportCmd::Hydrogen => rfd::FileDialog::new()
                .set_title(i18n::tr("Hydrogen Song…"))
                .add_filter(i18n::tr("Hydrogen Song"), &["h2song"])
                .pick_file(),
            ImportCmd::Rex => rfd::FileDialog::new()
                .set_title(i18n::tr("REX Loop…"))
                .add_filter(i18n::tr("REX Loop"), &["rx2", "rex"])
                .pick_file(),
        };
        let Some(path) = path else {
            return;
        };
        let result = match cmd {
            ImportCmd::MusicXml => score_import::import_musicxml(&mut self.project, &path)
                .map(|n| i18n::trf("imported {} tracks from {}", &[&n, &path.display()])),
            ImportCmd::GuitarPro => score_import::import_guitarpro(&mut self.project, &path)
                .map(|n| i18n::trf("imported {} tracks from {}", &[&n, &path.display()])),
            ImportCmd::Hydrogen => score_import::import_hydrogen(&mut self.project, &path)
                .map(|n| i18n::trf("imported {} tracks from {}", &[&n, &path.display()])),
            ImportCmd::Rex => rex_import::import(&mut self.project, &path, ctx),
            ImportCmd::LogicX => logicx_import::preview(&mut self.project, &path),
        };
        self.ui.status = Some(match result {
            Ok(status) => {
                self.history.record(&self.project);
                status
            }
            Err(e) => i18n::trf("cannot import {}: {}", &[&path.display(), &e]),
        });
    }

    /// File > Import > Audio File…: the same import as dropping a file on the window.
    fn import_audio_file(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                i18n::tr("Audio File…"),
                &[
                    "wav", "wave", "aif", "aiff", "aifc", "flac", "mp3", "caf", "ogg", "oga",
                    "m4a", "mp4",
                ],
            )
            .pick_file()
        else {
            return;
        };
        match import::import_audio(&mut self.project, &path, ctx) {
            Ok(()) => {
                // The whole import is one undo step.
                self.history.record(&self.project);
                self.ui.status = Some(i18n::trf(
                    "imported {} tracks from {}",
                    &[&1, &path.display()],
                ));
            }
            Err(e) => {
                self.ui.status = Some(i18n::trf("cannot import {}: {}", &[&path.display(), &e]))
            }
        }
    }

    /// File > Bounce > Project or Section…: asks where to write and starts the bounce.
    fn start_bounce(&mut self) {
        let (start, end) = if self.project.cycle {
            (self.project.cycle_start, self.project.cycle_end)
        } else {
            let end = self
                .project
                .tracks
                .iter()
                .flat_map(|t| t.regions.iter())
                .map(|r| self.project.region_end_beats(r))
                .fold(0.0, f64::max);
            (0.0, end)
        };
        if end <= start || self.bounce.is_some() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV", &["wav", "WAV"])
            .add_filter("FLAC", &["flac", "FLAC"])
            .set_file_name(format!("{}.wav", self.project.name))
            .save_file()
        else {
            return;
        };
        // A name with no extension is a WAV file.
        let path = if path.extension().is_none() {
            path.with_extension("wav")
        } else {
            path
        };
        let engine = self.sync.as_mut().map(|s| &mut s.engine);
        match bounce::Bounce::start(engine, &mut self.project, start, end, path) {
            Ok(running) => self.begun_bounce(running, None),
            Err(e) => self.ui.status = Some(e),
        }
    }

    /// Keeps a bounce that has just started. A real-time bounce sends its seek and Play in this
    /// same frame, so the engine starts capturing exactly when playback starts (no silent lead-in
    /// in the file). An offline bounce has nothing to send: it works on a copy of the project.
    fn begun_bounce(&mut self, running: bounce::Bounce, placed: Option<(usize, Vec<usize>)>) {
        let realtime = running.is_realtime();
        self.bounce = Some(running);
        self.region_bounce = placed;
        if realtime {
            if let Some(sync) = self.sync.as_mut() {
                sync.sync(&mut self.project);
            }
        }
    }

    /// File > Import > Other Projects…: a Logic Pro `.logicx` project adds its tracks, with their
    /// plug-ins and regions (see `logicx`); otherwise the audio files of a GarageBand bundle
    /// become tracks (see `garageband`; the arrangement inside the project is not read yet).
    fn import_band(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(i18n::tr("Other Projects…"))
            .pick_folder()
        else {
            return;
        };
        if logicx::is_bundle(&path) {
            match logicx::import_into(&mut self.project, &path, ctx) {
                Ok(report) => {
                    self.ui.status = Some(i18n::trf(
                        "imported {} tracks from {}",
                        &[&report.tracks, &path.display()],
                    ));
                    // The whole import is one undo step.
                    self.history.record(&self.project);
                    self.ui.logicx_report = Some(report);
                }
                Err(e) => self.ui.status = Some(e),
            }
            return;
        }
        match garageband::import(&mut self.project, &path, ctx) {
            Ok(n) => {
                self.ui.status = Some(i18n::trf(
                    "imported {} tracks from {}",
                    &[&n, &path.display()],
                ));
                // The whole import is one undo step.
                self.history.record(&self.project);
            }
            Err(e) => self.ui.status = Some(e),
        }
    }

    /// File > Export > Track as Audio File…: bounces the selected track alone to a WAV, with its
    /// effects and sends (the other tracks are soloed off while it runs, then put back).
    fn export_track(&mut self) {
        let track = self.project.selected_track;
        let Some(selected) = self.project.tracks.get(track) else {
            return;
        };
        if selected.regions.is_empty() || self.bounce.is_some() {
            return;
        }
        let name = selected.name.clone();
        let end = self
            .project
            .tracks
            .iter()
            .flat_map(|t| t.regions.iter())
            .map(|r| self.project.region_end_beats(r))
            .fold(0.0, f64::max);
        if end <= 0.0 {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV", &["wav", "WAV"])
            .set_file_name(format!("{name}.wav"))
            .save_file()
        else {
            return;
        };
        let engine = self.sync.as_mut().map(|s| &mut s.engine);
        match bounce::Bounce::start_track(engine, &mut self.project, track, 0.0, end, path) {
            Ok(running) => self.begun_bounce(running, None),
            Err(e) => self.ui.status = Some(e),
        }
    }

    /// File > Export > All Tracks as Audio Files…: exports every active track as an individual stem
    /// using faster-than-realtime offline bounce.
    fn export_all_tracks(&mut self) {
        let Some(dir) = rfd::FileDialog::new()
            .set_title(i18n::tr("All Tracks as Audio Files…"))
            .pick_folder()
        else {
            return;
        };

        let end = self
            .project
            .tracks
            .iter()
            .flat_map(|t| t.regions.iter())
            .map(|r| self.project.region_end_beats(r))
            .fold(0.0, f64::max);
        if end <= 0.0 {
            self.ui.status = Some("no active regions to export".to_string());
            return;
        }

        let sample_rate = self
            .sync
            .as_ref()
            .map_or(48_000, |s| s.engine.sample_rate());
        let eng = self.sync.as_ref().map(|s| &s.engine);
        let mut count = 0;
        let proj_name = file_name(&self.project.name);

        for (ti, t) in self.project.tracks.iter().enumerate() {
            if t.regions.is_empty() {
                continue;
            }
            let track_name = file_name(&t.name);
            let path = dir.join(format!("{proj_name}_{track_name}.wav"));
            if bounce::run_offline_bounce(
                eng,
                &self.project,
                0.0,
                end,
                &path,
                Some(ti),
                prowave_engine::BitDepth::Pcm24,
                true,
                sample_rate,
            )
            .is_ok()
            {
                count += 1;
            }
        }
        self.ui.status = Some(format!("Exported {count} track stems to {}", dir.display()));
    }

    /// File > Bounce > Regions in Place… and Record > Bounce In Place: plays the selected MIDI
    /// region's track alone over the region's span, then replaces the region with the audio.
    fn bounce_region_in_place(&mut self) {
        let Some((track, region)) = self.ui.tracks.selected else {
            return;
        };
        self.start_region_bounce(track, vec![region]);
    }

    /// Edit > Bounce and Join: bounces the selected regions of one track into one audio region,
    /// covering the span from the earliest to the latest (the gaps included).
    fn bounce_and_join(&mut self) {
        let Some((track, first)) = self.ui.tracks.selected else {
            return;
        };
        let mut regions: Vec<usize> = std::iter::once(first)
            .chain(
                self.ui
                    .tracks
                    .extra
                    .iter()
                    .filter(|(t, _)| *t == track)
                    .map(|(_, r)| *r),
            )
            .collect();
        regions.sort_unstable();
        regions.dedup();
        self.start_region_bounce(track, regions);
    }

    /// Bounces `regions` of `track` over their span and remembers to put the audio there.
    fn start_region_bounce(&mut self, track: usize, regions: Vec<usize>) {
        if self.bounce.is_some() {
            return;
        }
        let Some((start, end, name)) = join_span(&self.project, track, &regions) else {
            return;
        };
        let dir = project_io::bounces_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.ui.status = Some(format!("{}: {e}", dir.display()));
            return;
        }
        let path = bounce::free_path(&dir, &format!("{} {start:.3}", file_name(&name)));
        let engine = self.sync.as_mut().map(|s| &mut s.engine);
        match bounce::Bounce::start_regions(
            engine,
            &mut self.project,
            track,
            &regions,
            start,
            end,
            path,
        ) {
            Ok(running) => self.begun_bounce(running, Some((track, regions))),
            Err(e) => self.ui.status = Some(e),
        }
    }

    /// Puts a finished region bounce where the MIDI regions were: they become one audio region.
    fn place_bounce(
        &mut self,
        track: usize,
        regions: Vec<usize>,
        path: &std::path::Path,
        ctx: &egui::Context,
    ) {
        let Ok(file) = prowave_engine::AudioFile::open(path) else {
            self.ui.status = Some(format!("{}: cannot open the bounced audio", path.display()));
            return;
        };
        let audio = crate::import::audio_region(file, ctx);
        let Some((start_beat, end_beat, name)) = join_span(&self.project, track, &regions) else {
            return;
        };
        let start_bar = self.project.beats_to_bar(start_beat) + 1.0;
        let len_bars =
            (self.project.beats_to_bar(end_beat) - self.project.beats_to_bar(start_beat)).max(1e-9);
        let Some(t) = self.project.tracks.get_mut(track) else {
            return;
        };
        let mut removed: Vec<usize> = regions
            .into_iter()
            .filter(|r| *r < t.regions.len())
            .collect();
        removed.sort_unstable();
        removed.dedup();
        for r in removed.into_iter().rev() {
            t.regions.remove(r);
        }
        t.regions.push(crate::model::Region {
            name: name.clone(),
            start_bar: start_bar as f32,
            len_bars: len_bars as f32,
            notes: Vec::new(),
            controls: Vec::new(),
            folder: None,
            generated: None,
            audio: Some(audio),
            missing: None,
            params: crate::model::RegionParams::default(),
        });
        t.regions
            .sort_by(|a, b| a.start_bar.total_cmp(&b.start_bar));
        // The engine streams the new audio: rebuild its song.
        self.project.song_rev += 1;
        self.history.record(&self.project);
        self.ui.status = Some(i18n::trf("Bounced {} in place", &[&name]));
    }

    /// Runs a bounce once a frame: gathers the engine's frames and stops at the end beat (or
    /// when the transport stops, which cancels it).
    fn run_bounce(&mut self, ctx: &egui::Context) {
        let Some(running) = self.bounce.as_mut() else {
            self.ui.bouncing = false;
            return;
        };
        self.ui.bouncing = true;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            running.cancel(&mut self.project);
        }
        if running.advance(&self.project) {
            let percent = (running.progress(&self.project) * 100.0).round();
            self.ui.status = Some(i18n::trf("Bouncing {}%", &[&format!("{percent:.0}")]));
            ctx.request_repaint();
            return;
        }
        let reached_end = self.project.playing && self.project.position_beats >= running.end_beat();
        let Some(running) = self.bounce.take() else {
            self.ui.bouncing = false;
            return;
        };
        self.ui.bouncing = false;
        let engine = self.sync.as_mut().map(|s| &mut s.engine);
        let result = running.finish(engine, &mut self.project, reached_end);
        let placed = self.region_bounce.take();
        match result {
            Ok(Some(done)) => {
                self.ui.status = Some(i18n::trf("Bounced to {}", &[&done.path.display()]));
                if let Some((track, regions)) = placed {
                    if done.placeable {
                        self.place_bounce(track, regions, &done.path, ctx);
                    } else {
                        // The file is kept, but the regions are not what it was bounced from.
                        self.ui.status = Some(i18n::trf(
                            "Bounced {} but the region changed during the bounce, so it was not replaced",
                            &[&done.path.display()],
                        ));
                    }
                }
            }
            Ok(None) => self.ui.status = Some(i18n::tr("Bounce cancelled").to_string()),
            Err(e) => self.ui.status = Some(e),
        }
    }
}

/// A region name made safe for a file name (the bounce's media).
fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "Bounce".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod file_name_tests {
    use super::*;

    #[test]
    fn bounce_file_names_are_safe() {
        assert_eq!(file_name("Drums: verse"), "Drums_ verse");
        assert_eq!(file_name("Kick & Snare"), "Kick _ Snare");
        assert_eq!(file_name("   "), "Bounce");
        assert_eq!(file_name(&"x".repeat(100)).chars().count(), 48);
    }

    #[test]
    fn joining_regions_covers_them_all() {
        use crate::model::{Note, Region, RegionParams, TrackKind};
        let mut project = Project::empty();
        project.tracks[0].kind = TrackKind::Instrument;
        for (bar, len, name) in [(1.0, 2.0, "One"), (4.0, 2.0, "Two")] {
            project.tracks[0].regions.push(Region {
                name: name.to_string(),
                start_bar: bar,
                len_bars: len,
                notes: vec![Note {
                    start: 0.0,
                    len: 1.0,
                    pitch: 60,
                    vel: 100,
                    mute: false,
                    lyric: crate::lyrics::Lyric::default(),
                }],
                controls: Vec::new(),
                folder: None,
                generated: None,
                audio: None,
                missing: None,
                params: RegionParams::default(),
            });
        }
        let (start, end, name) = join_span(&project, 0, &[0, 1]).unwrap_or_else(|| panic!("span"));
        assert_eq!((start, end, name.as_str()), (0.0, 20.0, "One"));

        // A region without notes is not bounced, so the span shrinks to the other one.
        project.tracks[0].regions[1].notes.clear();
        let (start, end, _) = join_span(&project, 0, &[0, 1]).unwrap_or_else(|| panic!("span"));
        assert_eq!((start, end), (0.0, 8.0));
        assert!(join_span(&project, 0, &[1]).is_none());
    }
}

/// The span a region bounce covers: the earliest and latest of `regions` on `track`, in beats,
/// and the name the audio region takes (the earliest region's). Only MIDI regions with notes
/// count; `None` when there is nothing to bounce.
fn join_span(p: &Project, track: usize, regions: &[usize]) -> Option<(f64, f64, String)> {
    let t = p.tracks.get(track)?;
    let mut spans: Vec<(f64, f64, String)> = regions
        .iter()
        .filter_map(|r| t.regions.get(*r))
        .filter(|r| r.audio.is_none() && !r.notes.is_empty())
        .map(|r| {
            (
                p.region_start_beats(r),
                p.region_end_beats(r),
                r.name.clone(),
            )
        })
        .collect();
    if spans.is_empty() {
        return None;
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let start = spans[0].0;
    let end = spans.iter().map(|s| s.1).fold(start, f64::max);
    (end > start).then(|| (start, end, spans[0].2.clone()))
}

impl App {
    /// Records finished edits for Undo, and runs Undo / Redo. An edit is looked for when a
    /// gesture ends (button let go, key, menu) and every half second (menus in the macOS menu
    /// bar send no input events), never in the middle of a drag.
    fn track_history(&mut self, ctx: &egui::Context) {
        self.frames_since_record += 1;
        let (gesture_ended, dragging) = ctx.input(|i| {
            let ended = i.pointer.any_released()
                || i.events.iter().any(|e| {
                    matches!(
                        e,
                        egui::Event::Key { pressed: true, .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                    )
                });
            (ended, i.pointer.any_down())
        });
        let undo_cmd = self.ui.undo_cmd.take();
        // A recording is one edit: it is recorded when it ends, not on every frame.
        let recording = self.ui.rec.active() || self.ui.audio_rec.active();
        if !dragging
            && !recording
            && (gesture_ended || undo_cmd.is_some() || self.frames_since_record >= 30)
        {
            self.frames_since_record = 0;
            self.history.record(&self.project);
        }
        let changed = match undo_cmd {
            Some(undo::UndoCmd::Undo) => self.history.undo(&mut self.project),
            Some(undo::UndoCmd::Redo) => self.history.redo(&mut self.project),
            Some(undo::UndoCmd::Clear) => {
                self.history.clear(&self.project);
                false
            }
            None => false,
        };
        if changed {
            // Track and region numbers may point elsewhere now.
            self.ui.tracks.clear_selection();
            self.ui.piano.selected.clear();
            ctx.request_repaint();
        }
        self.ui.can_undo = self.history.can_undo();
        self.ui.can_redo = self.history.can_redo();
    }

    /// MIDI Time Code in: the transport follows the port the project chases, if any.
    /// Follow MIDI Clock: the transport starts, continues, stops and jumps as the clock input
    /// says, and the tempo is the clock's. Arrivals are read every frame even when no port is
    /// followed, so the queue does not fill up.
    fn follow_clock(&mut self, ctx: &egui::Context) {
        self.clock_in.sync(self.project.midi_clock_in.as_deref());
        let mut arrivals = Vec::new();
        self.clock_in.poll(&mut arrivals);
        if self.project.midi_clock_in.is_none() {
            return;
        }
        for a in arrivals {
            let Some(event) = self.clock_follow.message(a.at_us, a.data()) else {
                continue;
            };
            clock_follow::apply(&mut self.project, event);
            ctx.request_repaint();
        }
    }

    fn follow_time_code(&mut self, ctx: &egui::Context) {
        let Some(input) = self.mtc_in.as_mut() else {
            return;
        };
        input.sync(self.project.mtc_in.as_deref());
        // The keyboard input does not take the port the time code comes from.
        if let Some(midi) = self.midi.as_mut() {
            midi.avoid(self.project.mtc_in.as_deref());
            midi.prefer(self.project.midi_in_port.as_deref());
        }
        let mut events = Vec::new();
        input.poll(&mut events);
        self.chase
            .update(&mut self.project, &events, std::time::Instant::now());
        if self.chase.is_following() {
            // Look again soon enough to notice when the time code stops.
            ctx.request_repaint_after(mtc_in::STALL / 3);
        }
    }

    /// MIDI keyboard notes: play them on the selected instrument and, while recording, put
    /// them into the new region.
    fn play_and_record_midi(&mut self, ctx: &egui::Context) {
        let mut hw_input = Vec::new();
        if let Some(midi) = self.midi.as_mut() {
            midi.poll(&mut hw_input);
        }
        // The computer keyboard, when Musical Typing is on, makes the same live notes.
        self.ui.typing.read_keys(ctx);
        let typing_input = self.ui.typing.take();
        let mut input = hw_input.clone();
        input.extend(typing_input.clone());
        if !input.is_empty() {
            self.ui.midi_in_activity = 1.0;
        }
        let p = &mut self.project;
        let s = &mut self.ui;
        let track = s.rec.target().map_or(p.selected_track, |(t, _)| t);
        if let Some(sync) = self.sync.as_mut() {
            sync.set_armed_track(track);
        }
        s.active_midi_device = self
            .midi
            .as_ref()
            .and_then(|m| m.active_port().map(str::to_string));
        let plays = p
            .tracks
            .get(track)
            .is_some_and(|t| t.kind != model::TrackKind::Audio);

        // Hardware MIDI has already been pushed lock-free to the audio engine callback.
        // Musical Typing (keyboard keys) and fallback auditioning route here.
        let aud_input = if self.midi.as_ref().and_then(|m| m.active_port()).is_some() {
            typing_input
        } else {
            input.clone()
        };
        for note in &aud_input {
            if !plays {
                break;
            }
            let tp = p
                .tracks
                .get(p.selected_track)
                .map(|t| t.params)
                .unwrap_or_default();
            s.audition.push(match *note {
                recorder::LiveNote::On { pitch, vel } => {
                    let eff_pitch = if tp.no_transpose {
                        pitch as i32
                    } else {
                        pitch as i32 + tp.transpose as i32
                    };
                    let eff_vel = (vel as i32 + tp.velocity as i32).clamp(1, 127);
                    if !(tp.key_limit.0 as i32..=tp.key_limit.1 as i32).contains(&eff_pitch)
                        || !(tp.vel_limit.0 as i32..=tp.vel_limit.1 as i32).contains(&eff_vel)
                        || !(0..=127).contains(&eff_pitch)
                    {
                        continue;
                    }
                    piano_roll::Audition::On {
                        pitch: eff_pitch as u8,
                        vel: eff_vel as u8,
                        track: track.min(usize::from(u16::MAX)) as u16,
                    }
                }
                recorder::LiveNote::Off { pitch } => {
                    let eff_pitch = if tp.no_transpose {
                        pitch as i32
                    } else {
                        pitch as i32 + tp.transpose as i32
                    };
                    piano_roll::Audition::Off {
                        pitch: (eff_pitch.clamp(0, 127)) as u8,
                    }
                }
                // Bend and modulation reach a SoundFont or an external instrument as they are
                // played (and are recorded).
                recorder::LiveNote::Bend { val } => piano_roll::Audition::Control {
                    track: track.min(usize::from(u16::MAX)) as u16,
                    number: prowave_engine::PITCH_BEND_CONTROL,
                    value: val,
                },
                recorder::LiveNote::Modulation { val } => piano_roll::Audition::Control {
                    track: track.min(usize::from(u16::MAX)) as u16,
                    number: recorder::MODULATION_WHEEL,
                    value: u16::from(val),
                },
                // The engine has no sustain pedal or pressure for the keys yet: they are only
                // recorded.
                recorder::LiveNote::Sustain { .. } | recorder::LiveNote::Pressure { .. } => {
                    continue
                }
            });
        }
        // Record on an audio track is the audio recorder's (see `record_audio`).
        let midi = if s.rec.active() || !audio_rec::takes_over(p) {
            recorder::update(&mut s.rec, p, &input)
        } else {
            recorder::Outcome::Nothing
        };
        match midi {
            recorder::Outcome::Started(t, r) => {
                s.tracks.select_only(t, r);
                s.piano.selected.clear();
            }
            recorder::Outcome::Finished(kept) => {
                s.tracks.set_primary(kept);
                s.piano.selected.clear();
            }
            recorder::Outcome::NoTrack => {
                s.status = Some(i18n::tr("Record: there is no instrument track").into());
            }
            recorder::Outcome::Nothing => {}
        }
        if std::mem::take(&mut s.capture_cmd) {
            match recorder::capture(&mut s.rec, p) {
                Some(region) => s.tracks.select_only(region.0, region.1),
                None => s.status = Some(i18n::tr("Capture Recording: nothing was played").into()),
            }
        }
        if s.rec.active() || !input.is_empty() {
            ctx.request_repaint();
        }
    }

    /// Audio inputs: opens the input while a track wants it and records the armed tracks.
    fn record_audio(&mut self, ctx: &egui::Context) {
        let latency_us = self
            .sync
            .as_mut()
            .map_or(0, |sync| sync.engine.state().output_latency_us);
        let s = &mut self.ui;
        match s
            .audio_rec
            .update(&mut s.mixer.inputs, &mut self.project, latency_us, ctx)
        {
            audio_rec::Outcome::Finished(kept, warning) => {
                if kept.is_some() {
                    s.tracks.set_primary(kept);
                }
                if warning.is_some() {
                    s.status = warning;
                }
            }
            audio_rec::Outcome::Failed(message) => s.status = Some(message),
            audio_rec::Outcome::Nothing => {}
        }
        if let (Some(feed), Some(sync)) = (s.audio_rec.take_feed(), self.sync.as_mut()) {
            sync.engine
                .send(prowave_engine::Command::SetInput(Some(feed)));
        }
        if s.audio_rec.active() {
            ctx.request_repaint();
        } else if s.audio_rec.listening() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }

    /// The window while the Project Chooser is up: the menu bar and the chooser, with the
    /// File key commands and prompts still working.
    fn chooser_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if let Some(menu) = self.native_menu.as_mut() {
            menu.update(ctx, &mut self.project, &mut self.ui);
        }
        if let Some(cmd) = file_commands::pressed(ctx) {
            self.ui.file_cmd = Some(cmd);
        }
        if let Some(sync) = self.sync.as_mut() {
            sync.sync(&mut self.project);
        }
        // Audio files dropped on the chooser go into the open project.
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.ui.chooser = None;
            for path in dropped {
                self.import(&path, ctx);
            }
        }

        // Keep track of active monitor dimensions for centering and startup sizing
        if let Some(mon) = ctx.input(|i| i.viewport().monitor_size) {
            if let Some(cfg) = crate::recent_files::config_dir() {
                let path = cfg.join("display.txt");
                let cur = std::fs::read_to_string(&path).unwrap_or_default();
                let target = format!("{} {}", mon.x, mon.y);
                if cur.trim() != target {
                    let _ = std::fs::write(path, target);
                }
            }
        }

        let mut visuals = ctx.global_style().visuals.clone();
        visuals.window_shadow = egui::epaint::Shadow::NONE;
        visuals.popup_shadow = egui::epaint::Shadow::NONE;
        ctx.set_visuals(visuals);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ui, |ui| start_screen::show(ui, &mut self.ui));
        // If chooser was dismissed / template chosen, open and maximize main DAW
        if let Some(cmd) = self.ui.file_cmd.take() {
            self.run_file_cmd(cmd, ctx);
        }
        if self.ui.chooser.is_none() {
            self.transition_to_daw(ctx);
        }
        self.show_save_prompt(ctx);
        window_chrome::resize_edges(ctx);
    }

    pub fn transition_to_daw(&self, ctx: &egui::Context) {
        let mon_size = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1600.0, 960.0));
        ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
        #[cfg(target_os = "macos")]
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        #[cfg(not(target_os = "macos"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
            800.0, 520.0,
        )));
        #[cfg(not(target_os = "windows"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(mon_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Dōri".into()));
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.ui.chooser.is_some() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            visuals.panel_fill.to_normalized_gamma_f32()
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let _frame_time = bench::frame_timer();
        self.ui.has_path = self.path.is_some();
        let dt = ctx.input(|i| i.stable_dt) as f64;
        self.touchpad_guard.update(
            self.ui.typing.open && ctx.input(|i| i.focused) && self.ui.typing.keep_touchpad_active,
        );

        if let Some(probe) = self.bench.as_mut() {
            if probe.frame(&ctx) {
                self.bench = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Some(shots) = self.shots.as_mut() {
            if shots.frame(&ctx) {
                self.shots = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Some(shot) = self.shot.as_mut() {
            if let Some(outcome) = shot.frame(&ctx) {
                self.shot = None;
                match outcome {
                    bench::ShotOutcome::Saved => {}
                    bench::ShotOutcome::Failed(message) => {
                        eprintln!("cannot save the picture: {message}");
                        self.shot_exit
                            .store(1, std::sync::atomic::Ordering::Release);
                    }
                    bench::ShotOutcome::TimedOut => {
                        eprintln!("the window did not hand out a picture in time");
                        self.shot_exit
                            .store(1, std::sync::atomic::Ordering::Release);
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        if self.ui.chooser.is_some() {
            // Keep native 100% zoom (self.ui.ui_scale) without shrinking by DESIGN_WIDTH (1480px)
            if (self.ui.ui_scale - ctx.zoom_factor()).abs() > 0.001 {
                ctx.set_zoom_factor(self.ui.ui_scale);
            }
            self.chooser_ui(ui, &ctx);
            return;
        }

        ui_scale::apply(&ctx, self.ui.ui_scale);
        key_commands::begin_frame(&ctx);
        key_commands::set_active_prefs(self.ui.keymap_prefs.clone());
        tracks_area::poll_normalize(&ctx, &mut self.project, &mut self.ui);
        smart_tempo::poll(&ctx, &mut self.project, &mut self.ui);
        flex_edit::poll(&ctx, &mut self.project, &mut self.ui);
        groove::poll(&ctx, &mut self.project, &mut self.ui);
        if !ctx.egui_wants_keyboard_input() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                start_or_toggle_playback(&mut self.project, &self.ui);
            } else if self.ui.keymap_prefs.profile == key_commands::KeymapProfile::Reaper {
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                    start_or_toggle_playback(&mut self.project, &self.ui);
                }
            }
        }
        if let Some(menu) = self.native_menu.as_mut() {
            menu.update(&ctx, &mut self.project, &mut self.ui);
        }
        if let Some(cmd) = file_commands::pressed(&ctx) {
            self.ui.file_cmd = Some(cmd);
        }
        menus::handle_keys(&ctx, &mut self.project, &mut self.ui);
        key_commands::handle(&ctx, &mut self.project, &mut self.ui);
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        let drop_pos = ctx.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos()));
        let over_headers = drop_pos
            .is_some_and(|pos| self.ui.tracks_headers_rect.is_some_and(|r| r.contains(pos)));
        for path in dropped {
            let is_proj = is_project_path(path.as_os_str());
            if is_proj {
                self.open(&path, &ctx);
                continue;
            }
            let is_audio = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| {
                    matches!(
                        ext.to_ascii_lowercase().as_str(),
                        "wav" | "aif" | "aiff" | "flac" | "mp3" | "ogg" | "m4a" | "caf" | "opus"
                    )
                });
            if over_headers && is_audio {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Audio")
                    .to_string();
                self.ui
                    .track_drop_slice
                    .open(track_drop_slice::DropAudioSource::File { path, name });
                continue;
            }
            self.import(&path, &ctx);
        }
        let panic = std::mem::take(&mut self.ui.midi_panic);
        if let Some(out) = self.midi_out.as_mut() {
            out.sync(&self.project);
            if panic {
                out.panic();
            }
        }
        self.follow_time_code(&ctx);
        self.follow_clock(&ctx);
        // Pattern and Drummer regions follow their length and the bars under them.
        self.project.refresh_generated();
        if let Some(sync) = self.sync.as_mut() {
            sync.sync(&mut self.project);
            automation::tick(&mut self.project);
            groups::propagate(&mut self.project, &mut self.ui.group_watch);
            if sync.needs_retry() {
                ctx.request_repaint();
            }
            let load = sync.take_dsp_load();
            self.ui.cpu_load = load.max(self.ui.cpu_load - dt as f32 * 0.8);
            // The HD meter holds its peak for about a second, like the CPU meter.
            self.ui.hd_load = sync.disk_load().max(self.ui.hd_load - dt as f32 * 0.8);
            self.ui.midi_in_activity = (self.ui.midi_in_activity - dt as f32 * 3.0).max(0.0);
            self.ui.midi_out_activity = (self.ui.midi_out_activity - dt as f32 * 3.0).max(0.0);
            if self.project.playing && !self.project.tracks.is_empty() {
                self.ui.midi_out_activity = (self.ui.midi_out_activity + 0.2).min(1.0);
            }
            if sync.underruns() > 0 {
                // Kept in i18n registry without polluting UI status bar
                let _ = i18n::trf("disk too slow: {} underruns", &[&sync.underruns()]);
            }
        } else if self.project.playing {
            // No audio device: fall back to a UI-driven clock.
            let map = self.project.tempo_map();
            self.project.position_beats = map.advance(self.project.position_beats, dt);
        }
        if self.project.playing {
            ctx.request_repaint();
        }
        let master_peaks = self.sync.as_mut().map(|s| s.master_peaks());
        let mut meters = match self.sync.as_mut() {
            Some(sync) => {
                let (lufs_m, lufs_s, lufs_i, tp) = sync.master_loudness();
                mixer::Meters {
                    tracks: sync.track_meters(self.project.tracks.len()),
                    reduction: sync.gain_reduction(self.project.tracks.len()),
                    fx_input: sync.fx_inputs(self.project.tracks.len()),
                    master_reduction: sync.master_fx_meters().0,
                    master_fx_input: sync.master_fx_meters().1,
                    master: sync.master_peaks(),
                    master_lufs_momentary: lufs_m,
                    master_lufs_short: lufs_s,
                    master_lufs_integrated: lufs_i,
                    master_true_peak_db: tp,
                }
            }
            None => mixer::Meters::default(),
        };
        self.ui
            .audio_rec
            .show_levels(&self.project, &mut meters.tracks);

        // Auto-dismiss top-bar status notifications after 3.5 seconds
        if let Some(ref msg) = self.ui.status {
            let now = std::time::Instant::now();
            if self.ui.status_last_msg.as_deref() != Some(msg.as_str()) {
                self.ui.status_last_msg = Some(msg.clone());
                self.ui.status_at = Some(now);
            }
            if let Some(at) = self.ui.status_at {
                let ttl = std::time::Duration::from_millis(3500);
                let elapsed = now.saturating_duration_since(at);
                if elapsed >= ttl {
                    self.ui.status = None;
                    self.ui.status_at = None;
                    self.ui.status_last_msg = None;
                } else {
                    ctx.request_repaint_after(ttl - elapsed);
                }
            }
        } else {
            self.ui.status_at = None;
            self.ui.status_last_msg = None;
        }

        if self.ui.show_sound_library_manager && crate::sound_library::active_download().is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        logging::report_audio_thread();
        if let Some(ctrl) = &self.control_server {
            ctrl.poll_with_screenshots(&ctx, &mut self.project, &mut self.ui);
        }
        if let Some(rx) = &self.mcp_receiver {
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    mcp_server::McpCommand::Execute { request, responder } => {
                        let resp = mcp_server::McpEngine::handle_request(
                            request,
                            &mut self.project,
                            &mut self.ui,
                            Some(&meters),
                        );
                        let _ = responder.send(resp);
                    }
                }
            }
        }
        collaborate_panel::show(&ctx, &mut self.project, &mut self.ui, self.path.as_deref());
        quick_help::begin_frame(&ctx);
        let p = &mut self.project;
        let s = &mut self.ui;

        top_bar(ui, p, s);
        egui::Panel::top("control")
            .exact_size(58.0)
            .frame(egui::Frame::new().fill(crate::theme::CHROME_BG))
            .show(ui, |ui| transport::control_bar(ui, p, s, master_peaks));
        let opaque_panel_frame =
            egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_rgb(28, 28, 30));
        if s.show_library {
            let library_frame = egui::Frame::new()
                .fill(egui::Color32::from_rgb(0x18, 0x18, 0x1a))
                .inner_margin(egui::Margin::ZERO);
            egui::Panel::left("library")
                .resizable(true)
                .min_size(240.0)
                .max_size(480.0)
                .default_size(300.0)
                .frame(library_frame)
                .show(ui, |ui| library::show(ui, p, s));
        }
        if s.show_inspector {
            let inspector_frame = egui::Frame::new()
                .fill(egui::Color32::from_rgb(28, 28, 30))
                .inner_margin(egui::Margin::ZERO);
            egui::Panel::left("inspector")
                .resizable(true)
                .default_size(276.0)
                .min_size(240.0)
                .max_size(440.0)
                .frame(inspector_frame)
                .show(ui, |ui| inspector::show(ui, p, s, &meters));
        }
        if s.show_toolbar {
            egui::Panel::top("toolbar")
                .exact_size(side_panes::TOOLBAR_HEIGHT)
                .frame(opaque_panel_frame)
                .show(ui, |ui| side_panes::toolbar(ui, p, s));
        }
        if s.browser.open.is_some() {
            egui::Panel::right("browser")
                .exact_size(file_browser::WIDTH)
                .frame(opaque_panel_frame)
                .show(ui, |ui| file_browser::show(ui, s));
        }
        if s.show_notes {
            egui::Panel::right("notes")
                .exact_size(side_panes::NOTES_WIDTH)
                .frame(opaque_panel_frame)
                .show(ui, |ui| side_panes::note_pad(ui, p));
        }
        if s.show_lists {
            egui::Panel::right("lists")
                .resizable(true)
                .min_size(340.0)
                .default_size(360.0)
                .max_size(440.0)
                .frame(
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(28, 28, 30))
                        .inner_margin(egui::Margin::ZERO)
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(0x2d, 0x2d, 0x30),
                        )),
                )
                .show(ui, |ui| event_list::show(ui, p, s));
        }

        if matches!(
            s.bottom,
            Bottom::PianoRoll | Bottom::PitchEditor | Bottom::SessionPlayer
        ) {
            if let Some(track) = p.tracks.get(p.selected_track) {
                match track.kind {
                    model::TrackKind::Audio => {
                        if s.bottom != Bottom::PitchEditor {
                            s.bottom = Bottom::PitchEditor;
                        }
                        let sel_on_track = s
                            .tracks
                            .selected
                            .filter(|&(t, r)| {
                                t == p.selected_track
                                    && track.regions.get(r).is_some_and(|reg| reg.audio.is_some())
                            })
                            .map(|(_, r)| r);
                        let reg_idx = sel_on_track.or_else(|| {
                            track
                                .regions
                                .iter()
                                .position(|reg| {
                                    reg.audio.is_some()
                                        && p.region_start_beats(reg) <= p.position_beats
                                        && p.region_end_beats(reg) >= p.position_beats
                                })
                                .or_else(|| {
                                    track.regions.iter().position(|reg| reg.audio.is_some())
                                })
                        });
                        if let Some(ri) = reg_idx {
                            if s.tracks.selected != Some((p.selected_track, ri)) {
                                s.tracks.select_only(p.selected_track, ri);
                            }
                            if s.pitch_ed.region.is_none_or(|(t, _)| t != p.selected_track) {
                                s.pitch_ed.region = Some((p.selected_track, ri));
                            }
                        }
                    }
                    model::TrackKind::Drummer => {
                        if s.bottom == Bottom::PitchEditor {
                            s.bottom = Bottom::SessionPlayer;
                        }
                    }
                    model::TrackKind::Instrument => {
                        if s.bottom == Bottom::PitchEditor {
                            s.bottom = Bottom::PianoRoll;
                        }
                    }
                    _ => {}
                }
            }
        }

        if s.show_mixer
            || matches!(
                s.bottom,
                Bottom::PianoRoll
                    | Bottom::PitchEditor
                    | Bottom::MacroControls
                    | Bottom::SessionPlayer
            )
        {
            // About half of the space under the control bar (500 points at most), so the Tracks
            // area keeps room on short screens, but never less than a whole channel strip tall:
            // a shorter panel cut every strip's fader off at the bottom (issue #36). The Piano
            // Roll can be pulled taller; the mixer has a fixed height, as in Logic. (It follows
            // the window, so it is never outside its range when the window shrinks.)
            let room = ui.available_height();
            let usual = (room * 0.48).clamp(160.0, 500.0);
            let min_h = if s.bottom == Bottom::Mixer {
                mixer::min_panel_height(p, &s.mixer).clamp(180.0, room * 0.9)
            } else if s.bottom == Bottom::MacroControls {
                200.0
            } else if s.bottom == Bottom::SessionPlayer {
                420.0
            } else if s.bottom == Bottom::PitchEditor {
                260.0
            } else {
                160.0
            };
            let panel_id = match s.bottom {
                Bottom::Mixer => "bottom_mixer",
                Bottom::PianoRoll => "bottom_piano_roll",
                Bottom::PitchEditor => "bottom_pitch_editor",
                Bottom::MacroControls => "bottom_macro_controls",
                Bottom::SessionPlayer => "bottom_session_player",
            };
            let panel = egui::Panel::bottom(panel_id)
                .resizable(true)
                .min_size(min_h)
                .max_size((room * 0.85).max(540.0))
                .default_size(match s.bottom {
                    Bottom::MacroControls => 260.0,
                    Bottom::SessionPlayer => 460.0,
                    Bottom::PitchEditor => 340.0,
                    _ => 240.0,
                })
                .frame(opaque_panel_frame);
            panel.show(ui, |ui| match s.bottom {
                Bottom::Mixer => mixer::show(ui, p, s, &meters),
                Bottom::PianoRoll => piano_roll::show(ui, p, s),
                Bottom::PitchEditor => pitch_editor::show(ui, p, s),
                Bottom::MacroControls => macro_controls::show(ui, p, s, false),
                Bottom::SessionPlayer => session_player_ui::show(ui, p, s),
            });
            if let Some(i) = s.mixer.show_drum_machine.take() {
                s.show_drum_machine = Some(i);
            }
            if let Some(i) = s.mixer.show_marbble_inst.take() {
                s.show_marbble_inst = Some(i);
            }
            if let Some(i) = s.mixer.show_bass808.take() {
                s.show_bass808 = Some(i);
            }
        }
        let sel_track = p.selected_track;
        let mut mod_drawer = std::mem::take(&mut s.modulation_drawer);
        let track_name = p
            .tracks
            .get(sel_track)
            .map(|t| t.name.as_str())
            .unwrap_or("Track");
        modulation_drawer::show_drawer(ui, &mut mod_drawer, track_name);
        s.modulation_drawer = mod_drawer;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS_BG).corner_radius(0.0))
            .show(ui, |ui| {
                ui.set_min_width(420.0);
                tracks_area::show(ui, p, s, &meters);
            });
        // Floating top-layer drag badge for cross-panel region / User Loop drags.
        let active_drag = s.dragged_region.clone().or_else(|| {
            egui::DragAndDrop::payload::<track_drop_slice::DraggedRegionPayload>(&ctx)
                .map(|a| (*a).clone())
        });
        let ptr = ctx.input(|i| i.pointer.latest_pos().or(i.pointer.interact_pos()));
        if let Some(payload) = active_drag.as_ref() {
            if let Some(pos) = ptr {
                let target = track_drop_slice::cross_panel_drop_target(
                    pos,
                    s.tracks_headers_rect,
                    s.library_panel_rect,
                );
                let hint = match target {
                    Some(track_drop_slice::CrossPanelDropTarget::TrackHeaders) => {
                        Some(i18n::tr("Create Track / Drum Kit"))
                    }
                    Some(track_drop_slice::CrossPanelDropTarget::SoundLibrary) => {
                        Some(i18n::tr("Add to Loops Library"))
                    }
                    None => None,
                };
                egui::Area::new(egui::Id::unique("region_drag_badge"))
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .fixed_pos(pos + egui::vec2(12.0, 12.0))
                    .show(&ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(0x1c, 0x1c, 0x1e))
                            .stroke(egui::Stroke::new(
                                1.0,
                                match target {
                                    Some(track_drop_slice::CrossPanelDropTarget::SoundLibrary) => {
                                        egui::Color32::from_rgb(0x51, 0xee, 0xfc)
                                    }
                                    _ => egui::Color32::from_rgb(0x12, 0x69, 0xcc),
                                },
                            ))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&payload.region_name)
                                                .size(11.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .extend(),
                                    );
                                    if let Some(h) = hint {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format!("| {h}"))
                                                    .size(11.0)
                                                    .color(egui::Color32::from_rgb(
                                                        0x51, 0xee, 0xfc,
                                                    )),
                                            )
                                            .extend(),
                                        );
                                    }
                                });
                            });
                    });
            }
        } else if let Some(ul) = s.dragged_user_loop.as_ref() {
            if let Some(pos) = ptr {
                egui::Area::new(egui::Id::unique("user_loop_drag_badge"))
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .fixed_pos(pos + egui::vec2(12.0, 12.0))
                    .show(&ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(0x1c, 0x1c, 0x1e))
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(0x51, 0xee, 0xfc),
                            ))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&ul.name)
                                                .size(11.5)
                                                .strong()
                                                .color(egui::Color32::WHITE),
                                        )
                                        .extend(),
                                    );
                                });
                            });
                    });
            }
        }
        // Single end-of-frame drop evaluator after all panels have recorded their outer rects.
        if ctx.input(|i| i.pointer.any_released()) {
            resolve_pending_drops(&ctx, p, s, ptr);
        }
        fx_ui::show(&ctx, p, s, &meters);
        plugin_manager::show(&ctx, p, s);
        free_plugins::show(&ctx, s);
        track_drop_slice::show(&ctx, p, &mut s.track_drop_slice);
        user_loops::show_add_modal(&ctx, &mut s.add_loop_modal, &mut s.user_loops);
        hosted_plugins::idle();
        automation::show(&ctx, p, s);
        undo_history::show(&ctx, p, s, &mut self.history);
        macro_controls::show_window(&ctx, p, s);
        stems_ui::show(&ctx, p, s);
        let wants_kb = ctx.egui_wants_keyboard_input();
        if ctx.input(|i| {
            (i.modifiers.command && i.key_pressed(egui::Key::K))
                || (!wants_kb && i.key_pressed(egui::Key::Colon))
        }) {
            s.command_palette.toggle();
        }
        let mut palette_state = std::mem::take(&mut s.command_palette);
        if let Some(cmd_label) = command_palette::show(&ctx, &mut palette_state, s, p) {
            if cmd_label == "Diagnostics…" {
                s.diagnostics_window.open = true;
            } else {
                menus::apply(&cmd_label, p, s);
            }
        }
        s.command_palette = palette_state;
        let sel_track = p.selected_track;
        let mut diag_win = std::mem::take(&mut s.diagnostics_window);
        diagnostics::show_diagnostics_window(&ctx, &mut diag_win, p, sel_track);
        s.diagnostics_window = diag_win;

        quick_help::show_overlay(&ctx, &mut self.ui);
        quick_help::show_reference_window(&ctx, &mut self.ui);
        for path in std::mem::take(&mut self.ui.open_paths) {
            self.import(&path, &ctx);
        }
        for (path, target) in std::mem::take(&mut self.ui.open_drops) {
            if let Some((ti, beat)) = target {
                if import::is_audio_path(&path) {
                    if let Err(e) = import::import_audio_at(
                        &mut self.project,
                        &path,
                        Some(ti),
                        Some(beat),
                        &ctx,
                    ) {
                        self.ui.status =
                            Some(i18n::trf("cannot import {}: {}", &[&path.display(), &e]));
                    } else {
                        self.history.record(&self.project);
                    }
                    continue;
                }
            }
            self.import(&path, &ctx);
        }
        self.record_audio(&ctx);
        self.run_bounce(&ctx);
        self.play_and_record_midi(&ctx);
        if std::mem::take(&mut self.ui.update_check) {
            self.updater.start_check(true);
        }
        let unsaved = || file_commands::content(&self.project) != self.saved;
        if self.updater.show(&ctx, &unsaved) == updater::Action::Quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Forward live notes (piano keys, note auditioning) to the engine.
        // Notes on tracks past the engine's limit are silent, like the rest of those tracks.
        // A rejected event stays queued (with everything after it, to keep On/Off order)
        // and is retried next frame, so a Note Off is never lost.
        if let Some(sync) = self.sync.as_mut() {
            let mut sent = 0;
            for ev in &self.ui.audition {
                let cmd = match ev {
                    piano_roll::Audition::On { track, .. }
                        if usize::from(*track) >= prowave_engine::MAX_TRACKS =>
                    {
                        None
                    }
                    piano_roll::Audition::On { pitch, vel, track } => {
                        Some(prowave_engine::Command::NoteOn {
                            pitch: *pitch,
                            vel: *vel,
                            track: *track,
                        })
                    }
                    piano_roll::Audition::Off { pitch } => {
                        Some(prowave_engine::Command::NoteOff { pitch: *pitch })
                    }
                    piano_roll::Audition::Control { track, .. }
                        if usize::from(*track) >= prowave_engine::MAX_TRACKS =>
                    {
                        None
                    }
                    piano_roll::Audition::Control {
                        track,
                        number,
                        value,
                    } => Some(prowave_engine::Command::LiveControl {
                        track: *track,
                        number: *number,
                        value: *value,
                    }),
                    piano_roll::Audition::StartSample(player) => {
                        Some(prowave_engine::Command::StartAudition(player.clone()))
                    }
                    piano_roll::Audition::StopSample => Some(prowave_engine::Command::StopAudition),
                    piano_roll::Audition::SampleParams {
                        gain,
                        speed,
                        is_stereo,
                        is_looping,
                        pos_norm,
                    } => Some(prowave_engine::Command::SetAuditionParams {
                        gain: *gain,
                        speed: *speed,
                        is_stereo: *is_stereo,
                        is_looping: *is_looping,
                        pos_norm: *pos_norm,
                    }),
                };
                if cmd.is_some_and(|cmd| !sync.engine.send(cmd)) {
                    break;
                }
                sent += 1;
            }
            self.ui.audition.drain(..sent);
            if !self.ui.audition.is_empty() {
                ctx.request_repaint();
            }
        } else {
            self.ui.audition.clear();
        }
        if let Some(cmd) = self.ui.file_cmd.take() {
            self.run_file_cmd(cmd, &ctx);
        }
        if let Some(cmd) = self.ui.midi_cmd.take() {
            self.run_midi_cmd(cmd);
        }
        if std::mem::take(&mut self.ui.export_xml_cmd) {
            self.export_score();
        }
        if let Some(cmd) = self.ui.import_cmd.take() {
            self.run_import_cmd(cmd, &ctx);
        }
        if std::mem::take(&mut self.ui.import_audio_cmd) {
            self.import_audio_file(&ctx);
        }
        if std::mem::take(&mut self.ui.bounce_cmd) || std::mem::take(&mut self.ui.mixer.bounce_cmd)
        {
            self.start_bounce();
        }
        if std::mem::take(&mut self.ui.export_track_cmd) {
            self.export_track();
        }
        if std::mem::take(&mut self.ui.export_all_tracks_cmd) {
            self.export_all_tracks();
        }
        if std::mem::take(&mut self.ui.bounce_region_cmd) {
            self.bounce_region_in_place();
        }
        if std::mem::take(&mut self.ui.bounce_join_cmd) {
            self.bounce_and_join();
        }
        if std::mem::take(&mut self.ui.open_band_cmd) {
            self.import_band(&ctx);
        }
        if std::mem::take(&mut self.ui.reveal_cmd) {
            self.reveal_project();
        }
        // The Audio tab shows the running output, its buffer size included, so it is read each frame.
        if self.ui.settings_tab == Some(project_settings::SettingsTab::Audio) {
            self.ui.audio_info = self.sync.as_mut().map(|s| s.audio_info());
        }
        project_settings::show(&ctx, &mut self.project, &mut self.ui);
        if let Some(prefs) = self.ui.audio_request.take() {
            self.restart_audio(prefs);
        }
        track_dialogs::show(&ctx, &mut self.project, &mut self.ui);
        logicx::show_report(&ctx, &mut self.ui.logicx_report);
        new_track_dialog::show(&ctx, &mut self.project, &mut self.ui);
        stack_dialog::show(&ctx, &mut self.project, &mut self.ui);
        midi_fx_ui::show(&ctx, &mut self.project, &mut self.ui);
        transform_ui::show(&ctx, &mut self.project, &mut self.ui);
        smart_tempo::show(&ctx, &mut self.project, &mut self.ui);
        group_editor::show(&ctx, &mut self.project, &mut self.ui);
        let (typed, typed_key_limit) = self
            .project
            .tracks
            .get(self.project.selected_track)
            .map_or((None, (0, 127)), |t| {
                (Some(t.name.clone()), t.params.key_limit)
            });
        self.ui.typing.key_limit = typed_key_limit;
        musical_typing::show(&ctx, &mut self.ui.typing, typed.as_deref());
        drum_machine_ui::show(&ctx, &mut self.project, &mut self.ui);
        bass808_ui::show(&ctx, &mut self.project, &mut self.ui);
        marbble_panel_host::show_track_instrument_window(&ctx, &mut self.project, &mut self.ui);
        multimeter::show(&ctx, &mut self.ui.show_multimeter, &meters, &mut self.sync);
        eclipsa_ui::show(&ctx, &mut self.project, &mut self.ui);
        self.show_save_prompt(&ctx);
        self.show_revert_prompt(&ctx);
        if let Some(url) = self.ui.open_url.take() {
            ctx.open_url(egui::OpenUrl::new_tab(&url));
        }
        show_about_dialog(&ctx, &mut self.ui);
        crate::sound_library::show_manager_window(&ctx, &mut self.ui);
        show_lcd_customize_dialog(&ctx, &mut self.ui);
        self.track_history(&ctx);
        window_chrome::resize_edges(&ctx);
    }
}

fn show_about_dialog(ctx: &egui::Context, ui_state: &mut UiState) {
    if !ui_state.show_about {
        return;
    }
    if ui_state.about_icon.is_none() {
        if let Ok(icon) = eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            );
            ui_state.about_icon = Some(ctx.load_texture(
                "about_douri_icon",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    let mut close = false;
    crate::liquid_glass::glass_window("About Dōri")
        .open(&mut ui_state.show_about)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                if let Some(texture) = &ui_state.about_icon {
                    ui.add(egui::Image::new(texture).max_width(96.0).max_height(96.0));
                    ui.add_space(8.0);
                }
                ui.heading("Dōri");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(6.0);
                ui.label(
                    "A pure-Rust digital audio workstation built Linux-first with cross-platform desktop support.",
                );
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label("Based on fabox-os/Pro-Wave, with contributions and donor code from:");
                ui.label("• yuribeats/prowave");
                ui.label("• storytold/soundcraft");
                ui.label("• Ardour");
                ui.add_space(8.0);
                ui.label("Icons & Artwork:");
                ui.label("• Phosphor Icons by Helena Zhang & Tobias Fried (MIT License)");
                ui.label("• DAW UI Design by Dicky Jiang (CC BY 4.0) - Rotary knob and DAW control styling");
                ui.label("• webaudio-controls by g200kg et al. (Apache-2.0) - Hardware control ergonomics");
                ui.label("• ouestlabs/audio-ui by Ouest Labs (MIT License) - Meter ballistics, goniometer & ADSR UI");
                ui.label("• google/eclipsa-studio by Google LLC (Apache-2.0) - Eclipsa IAMF 7.1.2/7.1.4 spatial audio");
                ui.label("• joshband/GenerativeMIDI (MIT) & billyblu2000/accomontage2 (MIT) - Session Player engines & voicings");
                ui.label("• Dōri 3D app icon & studio artwork by triadwoozie");
                ui.add_space(8.0);
                ui.label(
                    "Released under GNU General Public License v3.0 or later (GPL-3.0-or-later).",
                );
                ui.add_space(14.0);
                if ui
                    .add_sized(
                        egui::vec2(120.0, 30.0),
                        egui::Button::new(
                            egui::RichText::new("Close")
                                .color(egui::Color32::from_rgb(0x11, 0x12, 0x15))
                                .size(12.5)
                                .strong(),
                        )
                        .fill(egui::Color32::from_rgb(0xf2, 0xf3, 0xf5))
                        .corner_radius(egui::CornerRadius::same(6)),
                    )
                    .clicked()
                {
                    close = true;
                }
                ui.add_space(4.0);
            });
        });
    if close {
        ui_state.show_about = false;
    }
}

fn show_lcd_customize_dialog(ctx: &egui::Context, ui_state: &mut UiState) {
    if !ui_state.show_lcd_customize {
        return;
    }
    let mut open = true;
    crate::liquid_glass::glass_window("Customize Control Bar and Display")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("LCD Display Options")
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.checkbox(
                &mut ui_state.lcd_custom.show_division_ticks,
                "Division & Ticks",
            );
            ui.checkbox(&mut ui_state.lcd_custom.show_tempo, "Tempo");
            ui.checkbox(
                &mut ui_state.lcd_custom.show_time_signature,
                "Time Signature",
            );
            ui.checkbox(&mut ui_state.lcd_custom.show_key_signature, "Key Signature");
            ui.checkbox(
                &mut ui_state.lcd_custom.show_performance_meters,
                "Performance Meters (CPU & HD)",
            );
            ui.checkbox(
                &mut ui_state.lcd_custom.show_midi_activity,
                "MIDI Activity Indicators (In / Out)",
            );
            ui.checkbox(&mut ui_state.lcd_custom.show_varispeed, "Varispeed");
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Apply Defaults").clicked() {
                    ui_state.lcd_custom = project_settings::LcdCustomOptions::default();
                    ui_state.lcd_mode = project_settings::LcdMode::BeatsAndProject;
                    ui_state.app_settings.lcd_custom = ui_state.lcd_custom;
                    ui_state.app_settings.lcd_mode = ui_state.lcd_mode;
                    ui_state.app_settings.save();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        ui_state.show_lcd_customize = false;
                        ui_state.app_settings.lcd_custom = ui_state.lcd_custom;
                        ui_state.app_settings.lcd_mode = ui_state.lcd_mode;
                        ui_state.app_settings.save();
                    }
                });
            });
            ui.add_space(4.0);
        });
    if !open {
        ui_state.show_lcd_customize = false;
        ui_state.app_settings.lcd_custom = ui_state.lcd_custom;
        ui_state.app_settings.lcd_mode = ui_state.lcd_mode;
        ui_state.app_settings.save();
    }
}

/// The in-window menu bar; on macOS, where the menus are in the system menu bar, only a line
/// for errors and warnings while there is one.
fn top_bar(ui: &mut egui::Ui, p: &mut Project, s: &mut UiState) {
    let frame = egui::Frame::side_top_panel(ui.style()).fill(menus::bar_fill(s.light_menus));
    if menus::IN_WINDOW {
        let bar = egui::Panel::top("menu")
            .exact_size(28.0)
            .frame(frame)
            .show(ui, |ui| menus::menu_bar(ui, p, s));
        if window_chrome::CUSTOM {
            let fg = menus::bar_text(s.light_menus);
            window_chrome::bar_controls(ui, bar.response.rect, bar.inner + 8.0, fg);
        }
    } else if menus::has_status(p, s) {
        egui::Panel::top("status")
            .frame(frame)
            .show(ui, |ui| ui.horizontal(|ui| menus::status_line(ui, p, s)));
    }
}

/// Opens the output the saved choice asks for. A saved device that is gone, or will not start,
/// gives the system's default output instead; the flag tells the caller that happened.
fn open_output(
    prefs: &audio_prefs::AudioPrefs,
) -> Result<(prowave_engine::Engine, bool), prowave_engine::EngineError> {
    match prowave_engine::Engine::start_on(prefs.device.as_deref(), prefs.buffer) {
        Ok(engine) => Ok((engine, false)),
        Err(e) if prefs.device.is_some() => {
            tracing::warn!("saved audio output failed ({e}); using the system default");
            prowave_engine::Engine::start_on(None, prefs.buffer).map(|engine| (engine, true))
        }
        Err(e) => Err(e),
    }
}

/// The MIDI sender for one engine: it plays the engine's note, time code and clock queues out
/// on the project's ports.
fn start_midi_out(sync: &mut engine_sync::EngineSync) -> Option<midi_out::MidiOut> {
    sync.take_midi_out()
        .zip(sync.take_mtc_out())
        .zip(sync.take_clock_out())
        .map(|((notes, mtc), clock)| {
            midi_out::MidiOut::start(midi_out::Queues { notes, mtc, clock })
        })
}

/// Log to stderr; `RUST_LOG` overrides the default (warnings, plus info from ProWave; the
/// Vulkan loader's harmless layer-manifest warnings from `wgpu_hal` are hidden).
fn init_logging() {
    logging::attach_default_dir();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,wgpu_hal=error,prowave=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(logging::TeeMakeWriter)
        .init();
}

/// The Dōri logo, 256×256 with transparency (generated from dist/icons/douri-2048.png).
pub const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icon-1024.png");

/// The Flex and Smart Tempo layout pictures (`PROWAVE_SHOT_SCENE=flex…`): a drum loop on an
/// audio track, made in the temporary folder, with Flex markers, a Flex Pitch MIDI track and
/// a Groove Track; `flexeditor` opens the Smart Tempo Editor on it, `flexmenu` the loop's
/// right-click menu and `flexgroove` a track's.
fn shot_flex_scene(app: &mut App, ctx: &egui::Context, kind: &str) {
    let dir = std::env::temp_dir().join("prowave-shots");
    let _ = std::fs::create_dir_all(&dir);
    let wav = dir.join("Drum Loop.wav");
    // Two bars of a 96 BPM beat: kick, snare and hats, a little late here and there.
    let rate = 44_100usize;
    let beat = 60.0 / 96.0;
    let frames = (beat * 8.0 * 2.0 * rate as f64) as usize;
    let mut v = vec![0f32; frames];
    let mut seed = 7u32;
    for k in 0..32 {
        let t = k as f64 * beat / 2.0 + if k % 4 == 3 { 0.025 } else { 0.0 };
        let at = (t * rate as f64) as usize;
        let (len, level, low) = match k % 4 {
            0 => (9000, 0.9, true),
            2 => (6000, 0.8, false),
            _ => (2500, 0.07, false),
        };
        for i in 0..len {
            if at + i >= frames {
                break;
            }
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let decay = (-(i as f32) / (len as f32 / 5.0)).exp();
            let body = if low {
                (i as f32 * 60.0 * std::f32::consts::TAU / rate as f32).sin()
            } else {
                noise
            };
            v[at + i] += body * level * decay;
        }
    }
    let stereo: Vec<f32> = v.iter().flat_map(|s| [*s, *s]).collect();
    if prowave_engine::write_wav(&wav, &stereo, rate as u32).is_err() {
        return;
    }
    let pr = &mut app.project;
    pr.position_beats = 0.0;
    if import::import_audio(pr, &wav, ctx).is_err() {
        return;
    }
    let t = pr.tracks.len() - 1;
    pr.tracks[t].regions[0].start_bar = 3.0;
    // Move the audio track near the top, where the pictures see it.
    let track = pr.tracks.remove(t);
    pr.tracks.insert(5, track);
    let t = 5;
    flex_edit::set_mode(pr, t, 0, prowave_engine::FlexMode::Rhythmic);
    if let Some(f) = pr.tracks[t].regions[0]
        .audio
        .as_mut()
        .and_then(|a| a.flex.as_mut())
    {
        f.markers = flex_edit::markers_from(
            (1..32).map(|k| (k as f64 * beat / 2.0, k as f64 * beat / 2.0 * 0.98)),
        );
    }
    groove::set_groove_track(pr, Some(t));
    pr.tracks[0].params.follow_groove = true;
    pr.selected_track = t;
    app.ui.tracks.select_only(t, 0);
    match kind {
        "editor" => smart_tempo::open_editor(&app.project, &mut app.ui, t, 0),
        "menu" => app.ui.tracks.shot_menu = Some((t, Some(0))),
        "groove" => app.ui.tracks.shot_menu = Some((0, None)),
        _ => {}
    }
}

/// The Mixer layout pictures (`PROWAVE_SHOT_SCENE=mix…`): the demo project playing, with two
/// Aux strips (a reverb and a delay), sends to them, effects on the strips, a compressor
/// working, and automation in its different modes. `mixfx` also opens the compressor's window,
/// `mixeq` the Channel EQ's, `mixspace` the Space Designer's and `mixauto` the Automation editor;
/// `mixplugin` puts a hosted plug-in on a strip and opens its window, `mixplugins` opens the
/// Plugin Manager.
fn shot_mixer_scene(app: &mut App, kind: &str) {
    use model::{AutoMode, AutoTarget, Insert, SendSlot, TrackKind};
    use prowave_engine::FxKind;
    let p = &mut app.project;
    let aux = |p: &mut model::Project, name: &str, kind: FxKind, mix: usize| {
        let i = p.add_track(TrackKind::Aux);
        p.tracks[i].name = name.into();
        let mut fx = Insert::new(kind);
        fx.params[mix] = 1.0;
        p.tracks[i].inserts = vec![fx];
        p.tracks[i].volume_db = -4.0;
    };
    aux(p, "Hall Reverb", FxKind::Reverb, 3);
    aux(p, "Echo", FxKind::Delay, 4);
    let at =
        |p: &model::Project, name: &str| p.tracks.iter().position(|t| t.name == name).unwrap_or(0);
    let piano = at(p, "Classic Electric Piano");
    let grand = at(p, "Steinway Grand Piano");
    let pad = at(p, "Analog Pad");
    let drums = at(p, "Drummer");
    let bass = at(p, "Electric Bass");
    let vocal = at(p, "Lead Vocal");
    for (t, a, b) in [
        (piano, 0.45, 0.30),
        (grand, 0.35, 0.0),
        (pad, 0.60, 0.25),
        (bass, 0.20, 0.0),
        (vocal, 0.58, 0.42),
    ] {
        let mut v = vec![SendSlot {
            bus: Some(1),
            level: a,
            pre: t == pad,
        }];
        if b > 0.0 {
            v.push(SendSlot {
                bus: Some(2),
                level: b,
                pre: false,
            });
        }
        p.tracks[t].sends = v;
    }
    // A broad boost low and high, a notch in the middle.
    let mut eq = Insert::new(FxKind::ChannelEq);
    eq.params[..5].copy_from_slice(&[4.0, -3.5, 600.0, 1.4, 5.0]);
    p.tracks[piano].inserts = vec![Insert::new(FxKind::Delay), eq];
    let mut eq2 = Insert::new(FxKind::ChannelEq);
    eq2.params[..5].copy_from_slice(&[-2.0, 5.0, 2500.0, 0.9, 1.0]);
    let mut comp = Insert::new(FxKind::Compressor);
    comp.params[0] = -34.0;
    comp.params[1] = 6.0;
    p.tracks[grand].inserts = vec![eq2, comp];
    let mut multi = Insert::new(FxKind::MultiEq);
    multi.params[..16].copy_from_slice(&[
        70.0, 3.0, 200.0, -4.0, 1.2, 700.0, 2.5, 1.5, 2200.0, -3.0, 2.0, 5500.0, 5.0, 1.0, 11000.0,
        -2.0,
    ]);
    p.tracks[pad].inserts = vec![Insert::new(FxKind::Chorus), multi];
    // A noise gate and a limiter on the vocal, and a group of the two pianos.
    let mut gate = Insert::new(FxKind::Gate);
    gate.params[0] = -38.0;
    let mut limiter = Insert::new(FxKind::Limiter);
    limiter.params[0] = 9.0;
    limiter.params[1] = -3.0;
    p.tracks[vocal].inserts = vec![gate, limiter];
    if let Some(g) = p.add_group("Keys") {
        p.assign_group(piano, Some(g));
        p.assign_group(grand, Some(g));
    }
    for (t, mode) in [
        (piano, AutoMode::Read),
        (grand, AutoMode::Read),
        (pad, AutoMode::Touch),
        (drums, AutoMode::Latch),
        (bass, AutoMode::Off),
    ] {
        p.tracks[t].auto.mode = mode;
    }
    for (beat, db) in [
        (0.0, -9.0),
        (16.0, -3.2),
        (32.0, -9.0),
        (48.0, -1.0),
        (64.0, -12.0),
    ] {
        p.tracks[piano].auto.set_point(AutoTarget::Volume, beat, db);
    }
    for (beat, pan) in [(0.0, -0.6), (24.0, 0.6), (48.0, -0.2)] {
        p.tracks[piano].auto.set_point(AutoTarget::Pan, beat, pan);
    }
    p.tracks[grand]
        .auto
        .set_point(AutoTarget::Volume, 0.0, -4.0);
    // The Tape Delay's feedback follows a lane with bent stretches.
    let feedback = AutoTarget::Fx { slot: 0, param: 1 };
    for (beat, v) in [(0.0, 0.2), (16.0, 0.8), (40.0, 0.35), (60.0, 0.9)] {
        p.tracks[piano].auto.set_point(feedback, beat, v);
    }
    p.tracks[piano].auto.fx[0].points[0].curve = 0.7;
    p.tracks[piano].auto.fx[0].points[1].curve = -0.6;
    p.tracks[piano].auto.shown = feedback;
    let (piano_t, grand_t) = (piano, grand);
    p.selected_track = grand_t;
    p.position_beats = 16.0;
    p.playing = true;
    app.ui.show_mixer = true;
    app.ui.bottom = Bottom::Mixer;
    match kind {
        "fx" => app.ui.mixer.fx_window = Some((grand_t, 1)),
        "eq" => app.ui.mixer.fx_window = Some((piano_t, 1)),
        "multi" => app.ui.mixer.fx_window = Some((pad, 1)),
        "limiter" => app.ui.mixer.fx_window = Some((vocal, 1)),
        "master" => {
            // The Mastering Glue preset on the Stereo Out strip.
            let mut glue = Insert::new(FxKind::Reverb);
            glue.params = space_designer_ui::mastering_preset(0);
            app.project.master_inserts = vec![glue];
            app.ui.mixer.fx_window = Some((mixer::MASTER_STRIP, 0));
        }
        "space" => {
            app.project.tracks[bass].inserts = vec![Insert::new(FxKind::Reverb)];
            app.ui.mixer.fx_window = Some((bass, 0));
        }
        // A hosted plug-in's window: the first Audio Unit the system lists (macOS has its
        // own), since no plug-in file is certain to be installed.
        "plugin" => {
            if let Some(info) = prowave_plugin_host::scan_audio_units().first() {
                let hosted = Insert::new_hosted(hosted_plugins::add(info, None));
                app.project.tracks[bass].inserts = vec![hosted];
                app.ui.mixer.fx_window = Some((bass, 0));
            }
        }
        "plugins" => app.ui.show_plugin_manager = true,
        "auto" => {
            app.ui.mixer.auto_editor.track = Some(piano_t);
            app.ui.mixer.auto_editor.span = 72.0;
        }
        _ => {}
    }
}

/// The window's class (Wayland: its app id). Wayland desktops find the icon and the menu entry
/// by the `.desktop` file's name, which is `prowave-studio` for the packages and the Flatpak's
/// own id inside a Flatpak, so the class is the same; X11 desktops use `StartupWMClass`.
fn window_app_id() -> String {
    std::env::var("FLATPAK_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| "io.github.triadwoozie.douri".to_string())
}

/// What `--help` prints.
fn print_help() {
    println!(
        "Dōri {} — a music production app in Rust (egui + cpal)",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("Usage: douri [options] [project.douri] [audio files...]");
    println!();
    println!("  --play                    start playing on launch");
    println!("  --piano                   open the Piano Roll on launch");
    println!("  -h, --help                print this help and exit");
    println!("  -V, --version             print the version and exit");
    println!("  --report [file]           generate a privacy-scrubbed diagnostic report and exit");
    println!("  --bench <file>            measure startup, memory and DSP load, then exit");
    println!(
        "  --layout-shots <folder>   save pictures of the layout at four window sizes, then exit"
    );
    println!("  --screenshot <file.png>   save one picture of the window, then exit");
    println!(
        "  --mcp-stdio               run as a Model Context Protocol (MCP) JSON-RPC stdio server"
    );
    println!("  --scan-ladspa <file>      list the plug-ins in a LADSPA library, then exit (the");
    println!("                            Plugin Manager reads each library in a child process)");
    println!("  --scan-vamp <file>        list the analysers in a Vamp library, then exit");
    println!();
    println!(
        "A .douri or .prowave path opens that project; other paths are imported as audio files."
    );
}

fn is_project_path(a: &std::ffi::OsStr) -> bool {
    let p = std::path::Path::new(a);
    crate::logicx::is_bundle(p)
        || p.extension().is_some_and(|e| {
            e.eq_ignore_ascii_case(prowave_project::EXTENSION)
                || e.eq_ignore_ascii_case(prowave_project::LEGACY_EXTENSION)
        })
}

fn detect_initial_monitor_size() -> egui::Vec2 {
    // 1. Try reading saved monitor dimensions from config
    if let Some(cfg) = crate::recent_files::config_dir() {
        let path = cfg.join("display.txt");
        if let Ok(s) = std::fs::read_to_string(path) {
            let mut parts = s.split_whitespace();
            if let (Some(w_str), Some(h_str)) = (parts.next(), parts.next()) {
                if let (Ok(w), Ok(h)) = (w_str.parse::<f32>(), h_str.parse::<f32>()) {
                    if w >= 640.0 && h >= 480.0 {
                        return egui::vec2(w, h);
                    }
                }
            }
        }
    }

    // 2. On Linux, check /sys/class/drm/*/modes
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
            for entry in entries.flatten() {
                let modes_file = entry.path().join("modes");
                if let Ok(s) = std::fs::read_to_string(modes_file) {
                    if let Some(first_line) = s.lines().next() {
                        let mut parts = first_line.split('x');
                        if let (Some(w_str), Some(h_str)) = (parts.next(), parts.next()) {
                            if let (Ok(w), Ok(h)) = (w_str.parse::<f32>(), h_str.parse::<f32>()) {
                                if w >= 640.0 && h >= 480.0 {
                                    return egui::vec2(w, h);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Fallback standard desktop resolution
    egui::vec2(1920.0, 1080.0)
}

fn main() -> eframe::Result {
    // The Plugin Manager reads each plug-in library in a copy of this program, so a plug-in
    // that crashes or hangs does not take the app with it: print what is in it and go.
    {
        let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
        if let [_, flag, path] = args.as_slice() {
            if flag == "--scan-ladspa" {
                std::process::exit(plugins::scan_main(std::path::Path::new(path)));
            }
            if flag == "--scan-vamp" {
                std::process::exit(vamp::scan_main(std::path::Path::new(path)));
            }
        }
    }
    bench::mark_start();
    // Started by an update that is waiting for the program to close: put the update in place.
    if updater::run_helper_if_requested() {
        return Ok(());
    }
    let all_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // A plug-in's own process is this program started with `--plugin-host` (see `plugins`): it
    // serves the plug-in and never comes back here.
    prowave_plugin_host::serve_if_requested(&all_args);
    // The child process that lists one plug-in file's plug-ins (see `plugins`).
    if hosted_plugins::run_scan_if_requested(&all_args) {
        return Ok(());
    }
    // `--version`, `--help`, and `--mcp-stdio` answer without opening a window.
    let flag = |name: &str| {
        let name = std::ffi::OsStr::new(name);
        all_args.iter().any(|a| a.as_os_str() == name)
    };
    if flag("--mcp-stdio") {
        if let Err(e) = mcp_server::run_stdio_mode() {
            eprintln!("Dōri MCP server stdio error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if flag("--version") || flag("-V") {
        println!("Dōri {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if flag("--help") || flag("-h") {
        print_help();
        return Ok(());
    }
    if flag("--report") {
        let mut report_path = None;
        for i in 0..all_args.len() {
            if all_args[i] == "--report" && i + 1 < all_args.len() {
                let candidate = &all_args[i + 1];
                if !candidate.to_string_lossy().starts_with('-') {
                    report_path = Some(std::path::PathBuf::from(candidate));
                }
            }
        }
        let devices = prowave_engine::output_devices();
        let report = diagnostics::generate_report(&devices);
        if let Some(path) = report_path {
            if let Err(e) = std::fs::write(&path, &report) {
                eprintln!(
                    "Failed to write diagnostic report to {}: {e}",
                    path.display()
                );
                std::process::exit(1);
            }
            println!("Diagnostic report saved to {}", path.display());
        } else {
            print!("{report}");
        }
        return Ok(());
    }
    let bench_out = bench::requested(&all_args);
    let shots_dir = bench::shots_requested(&all_args);
    let shot_out = bench::screenshot_requested(&all_args);
    // A `--screenshot` without a file is an error before the window opens: the session it
    // would otherwise start is not a measuring run and may change the user's settings.
    if shot_out.is_none() && all_args.iter().any(|a| a == bench::SCREENSHOT_ARG) {
        eprintln!("--screenshot needs a file name, for example: douri --screenshot shot.png");
        std::process::exit(2);
    }
    let shot_exit = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));
    if bench_out.is_none() && shots_dir.is_none() && shot_out.is_none() {
        updater::clean_up_after_update();
    }
    // Measuring runs leave the user's settings (recent projects, size) alone.
    let measuring = bench_out.is_some() || shots_dir.is_some() || shot_out.is_some();
    let scene = std::env::var("PROWAVE_SHOT_SCENE").unwrap_or_default();
    // Release builds: on a crash, write a report to the temp folder instead of a raw panic.
    human_panic::setup_panic!(human_panic::Metadata::new(
        "Dōri",
        env!("CARGO_PKG_VERSION")
    ));
    init_logging();
    i18n::init();
    project_io::remove_stale_media(std::time::Duration::from_secs(7 * 24 * 3600));
    let mon = detect_initial_monitor_size();
    let has_project_arg = all_args.iter().any(|a| is_project_path(a.as_os_str()));
    let no_chooser_flag = flag("--no-chooser")
        || (measuring && scene != "chooser" && scene != "welcome" && scene != "chooser-details")
        || std::env::var("CI").is_ok()
        || std::env::var("HEADLESS").is_ok();
    let start_in_dialog = !has_project_arg && !no_chooser_flag;
    let (init_size, init_min_size, init_maximized, init_resizable, init_title) = if start_in_dialog
    {
        let s = start_screen::chooser_viewport_size(Some(mon));
        (s, egui::vec2(760.0, 520.0), false, true, "Dōri")
    } else {
        (
            egui::vec2(1600.0, 980.0),
            egui::vec2(800.0, 520.0),
            true,
            true,
            "Dōri",
        )
    };

    let center_pos = egui::pos2(
        ((mon.x - init_size.x) / 2.0).max(0.0),
        ((mon.y - init_size.y) / 2.0).max(0.0),
    );

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(init_size)
        .with_min_inner_size(init_min_size)
        .with_maximized(init_maximized)
        .with_resizable(init_resizable)
        .with_title(init_title)
        .with_app_id(window_app_id())
        .with_decorations(cfg!(target_os = "macos"))
        .with_transparent(true)
        .with_has_shadow(!start_in_dialog)
        .with_position(center_pos);
    // Title bar / taskbar icon. A broken icon must not stop the app from starting.
    // macOS takes the Dock icon from the app bundle (prowave.icns, on the macOS icon tile);
    // setting one here would swap it for the bare logo once the app is running. An empty
    // icon tells eframe to leave the Dock icon alone instead of using its default egui logo.
    if cfg!(target_os = "macos") {
        viewport = viewport.with_icon(egui::IconData::default());
    } else {
        match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
            Ok(icon) => viewport = viewport.with_icon(icon),
            Err(e) => tracing::warn!("app icon could not be decoded: {e}"),
        }
    }
    // `--screenshot`: a picture without stealing the focus, so whoever asked for it keeps
    // working in their own window while the app draws.
    if shot_out.is_some() {
        viewport = viewport.with_active(false);
    }
    let wgpu_options = graphics::config();
    let started_backends = graphics::started_backends(&wgpu_options);
    let renderer = if std::env::var("CI").is_ok() || std::env::var("HEADLESS").is_ok() {
        eframe::Renderer::Glow
    } else {
        eframe::Renderer::Wgpu
    };
    let opts = eframe::NativeOptions {
        viewport,
        renderer,
        wgpu_options,
        // macOS activates a newly launched app and puts it in front; the picture must not
        // pull the user away from what they are doing.
        #[cfg(target_os = "macos")]
        event_loop_builder: shot_out.as_ref().map(|_| {
            Box::new(
                |builder: &mut winit::event_loop::EventLoopBuilder<eframe::UserEvent>| {
                    use winit::platform::macos::EventLoopBuilderExtMacOS as _;
                    builder.with_activate_ignoring_other_apps(false);
                },
            ) as eframe::EventLoopBuilderHook
        }),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Dōri",
        opts,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            icons::install_fonts(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);
            theme::apply(&cc.egui_ctx);
            // Cmd/Ctrl + = / - / 0 resize the interface through `ui_scale` instead, which
            // also keeps the fit for small windows.
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            // The plug-ins of the last run are listed at once, and the scan for new, changed and
            // removed ones runs in the background, as Logic does at every start.
            hosted_plugins::set_repaint(&cc.egui_ctx);
            hosted_plugins::scan(false);
            // Every start begins clean; the picture runs need something to show.
            let project = if shots_dir.is_some() || shot_out.is_some() {
                Project::demo()
            } else {
                Project::empty()
            };
            let mut audio_error = None;
            let mut midi_out = None;
            let prefs = audio_prefs::AudioPrefs::load();
            let keymap_prefs = key_commands::KeymapPrefs::load();
            key_commands::set_active_prefs(keymap_prefs.clone());
            let sync = match open_output(&prefs) {
                Ok((e, fell_back)) => {
                    if fell_back {
                        audio_error = Some(
                            i18n::tr(
                                "The saved output device is not available; using the system default",
                            )
                            .to_string(),
                        );
                    }
                    let mut sync = engine_sync::EngineSync::new(e, &project);
                    if !measuring {
                        midi_out = start_midi_out(&mut sync);
                    }
                    Some(sync)
                }
                Err(err) => {
                    tracing::warn!("audio disabled: {err}");
                    audio_error =
                        Some(i18n::trf("No sound: {}. Check your audio device.", &[&err]));
                    None
                }
            };
            let (mcp_tx, mcp_rx) = std::sync::mpsc::channel();
            let mcp_bridge = mcp_server::McpBridge::new(mcp_tx);
            if !measuring && flag("--mcp-server") {
                let _ = mcp_server::start_tcp_listener(
                    mcp_server::DEFAULT_MCP_PORT,
                    mcp_bridge.clone(),
                );
            }
            let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
            let control_server =
                control::requested_port(&args).and_then(
                    |port| match control::ControlServer::start(port, cc.egui_ctx.clone()) {
                        Ok(srv) => Some(srv),
                        Err(e) => {
                            tracing::warn!("control server failed: {e}");
                            None
                        }
                    },
                );
            let mut app = App {
                saved: file_commands::content(&project),
                history: undo::History::new(&project),
                project,
                ui: UiState {
                    audio_prefs: prefs,
                    keymap_prefs,
                    ..UiState::new()
                },
                sync,
                bounce: None,
                region_bounce: None,
                path: None,
                native_menu: None,
                bench: bench_out.clone().map(bench::Probe::new),
                shots: shots_dir.clone().map(bench::LayoutShots::new),
                shot: shot_out.clone().map(bench::Screenshot::new),
                shot_exit: shot_exit.clone(),
                frames_since_record: 0,
                midi: (!measuring).then(midi_in::MidiIn::new),
                midi_out,
                mtc_in: (!measuring).then(|| mtc_in::MtcIn::new(cc.egui_ctx.clone())),
                clock_in: clock_in::ClockIn::default(),
                clock_follow: clock_follow::ClockFollower::default(),
                chase: mtc_in::Chase::default(),
                updater: updater::Updater::new(),
                mcp_bridge: Some(mcp_bridge),
                mcp_receiver: Some(mcp_rx),
                control_server,
                touchpad_guard: touchpad_guard::TouchpadGuard::new(),
                plugins: plugins::PluginHost::default(),
                autosave: autosave::Autosave::default(),
            };
            app.ui.lcd_mode = app.ui.app_settings.lcd_mode;
            app.ui.lcd_custom = app.ui.app_settings.lcd_custom;
            if let Some(sync) = app.sync.as_mut() {
                if let Some(midi) = app.midi.as_mut() {
                    if let Some(tx) = sync.take_live_midi_producer() {
                        midi.attach_live_ring(tx, sync.armed_track());
                    }
                }
            }
            if !measuring {
                app.ui.update_auto = updater::auto_enabled();
                app.updater.check_at_startup();
                app.ui.simplified_mode = project_settings::load_simplified_mode();
            }
            #[cfg(target_os = "macos")]
            {
                app.native_menu = Some(native_menu::NativeMenu::install(&cc.egui_ctx));
            }
            app.ui.status = audio_error;
            if let Some(n) = measuring
                .then(|| {
                    std::env::var("PROWAVE_BENCH_TRACKS")
                        .ok()?
                        .parse::<usize>()
                        .ok()
                })
                .flatten()
            {
                app.project = bench::many_tracks(n.clamp(1, 1000));
                app.saved = file_commands::content(&app.project);
                app.history.reset(&app.project);
            }
            if app.shots.is_some() {
                // The pictures also show the Tempo track, with a few tempo changes.
                app.ui.tracks.show_tempo = true;
                app.ui.tracks.show_markers = true;
                app.ui.tracks.show_signatures = true;
                app.project.set_signature_from(5, (3, 4));
                app.project.set_signature_from(9, (6, 8));
                // A VCA and an External MIDI track near the front, to see their strips.
                {
                    use model::{Track, TrackKind};
                    let pr = &mut app.project;
                    pr.tracks.insert(1, Track::new("VCA 1", TrackKind::Vca));
                    pr.vca_inserted(1);
                    let mut ext = Track::new("External MIDI 1", TrackKind::ExternalMidi);
                    ext.midi_port = Some("USB MIDI Interface".into());
                    ext.midi_channel = 3;
                    pr.tracks.insert(2, ext);
                    pr.vca_inserted(2);
                    pr.assign_vca(0, Some(1));
                    pr.assign_vca(3, Some(1));
                    pr.tracks[1].volume_db = -4.0;
                }
                if let Some(r) = app
                    .project
                    .tracks
                    .get_mut(4)
                    .and_then(|t| t.regions.get_mut(0))
                {
                    r.params.fade_in = 4.0;
                    r.params.fade_out = 8.0;
                }
                for (bar, name) in [(0.0, "Intro"), (4.0, "Verse"), (12.0, "Chorus")] {
                    let i = app.project.add_marker(app.project.bar_to_beats(bar));
                    app.project.markers[i].name = name.into();
                }
                app.project.add_tempo_change(32.0, 132.0);
                app.project.add_tempo_change(48.0, 104.0);
                // A tempo curve from the first change to the second.
                app.project.set_tempo_ramp(1, true);
                // `PROWAVE_SHOT_SCENE=pattern` or `drummer` opens that editor on a new region
                // of the Drummer track.
                let scene = std::env::var("PROWAVE_SHOT_SCENE").unwrap_or_default();
                // `midi-out`: the External MIDI track selected, with a program and a bank set.
                if scene == "midi-out" {
                    if let Some(ti) = app
                        .project
                        .tracks
                        .iter()
                        .position(|t| t.kind == crate::model::TrackKind::ExternalMidi)
                    {
                        app.project.selected_track = ti;
                        app.project.tracks[ti].midi_program = Some(40);
                        app.project.tracks[ti].midi_bank = Some(2);
                        app.ui.inspector.track_open = true;
                    }
                }
                // `select-all`: every region selected (Edit > Select All).
                if scene == "select-all" {
                    app.ui.tracks.select_all(&app.project);
                }
                // `tracks-extras`: the Master strip, two hidden header parts and a chosen icon.
                if scene == "tracks-extras" {
                    app.project.show_master_track = true;
                    app.project.header_hidden = vec![
                        crate::model::HeaderPart::Pan,
                        crate::model::HeaderPart::Volume,
                    ];
                    app.project.tracks[0].icon = Some(crate::track_icons::IconKind::Guitar);
                }
                // `settings-metronome` and the like open Project Settings on that tab.
                app.ui.settings_tab = match scene.as_str() {
                    "settings-general" => Some(project_settings::SettingsTab::General),
                    "settings-sync" => Some(project_settings::SettingsTab::Synchronization),
                    "settings-metronome" => Some(project_settings::SettingsTab::Metronome),
                    "settings-recording" => Some(project_settings::SettingsTab::Recording),
                    "settings-tuning" => Some(project_settings::SettingsTab::Tuning),
                    "settings-audio" => Some(project_settings::SettingsTab::Audio),
                    "settings-midi" => Some(project_settings::SettingsTab::Midi),
                    "settings-appearance" => Some(project_settings::SettingsTab::Appearance),
                    _ => None,
                };
                if scene == "score" || scene == "midifx" {
                    app.ui.tracks.select_only(0, 0);
                    app.project.selected_track = 0;
                    app.ui.bottom = Bottom::PianoRoll;
                    app.ui.score_tab = true;
                    app.ui.show_midi_fx = scene == "midifx";
                    app.project.tracks[0].midi_fx = vec![
                        midi_fx::FxSlot {
                            fx: midi_fx::MidiFx::Arpeggiator {
                                rate: pattern::StepRate::Eighth,
                                pattern: midi_fx::ArpPattern::UpDown,
                                octaves: 2,
                                gate: 80,
                            },
                            on: true,
                        },
                        midi_fx::FxSlot {
                            fx: midi_fx::MidiFx::Scale {
                                root: 0,
                                minor: true,
                            },
                            on: true,
                        },
                    ];
                }
                if let Some(kind) = scene.strip_prefix("flex") {
                    shot_flex_scene(&mut app, &cc.egui_ctx, kind);
                }
                if let Some(kind) = scene.strip_prefix("mix") {
                    shot_mixer_scene(&mut app, kind);
                }
                if scene.starts_with("session-") {
                    let pr = &mut app.project;
                    let mut sp = crate::session_player::SessionPlayerParams::default();
                    let (kind, tab) = match scene.as_str() {
                        "session-drummer" => (
                            crate::session_player::SessionPlayerKind::Drummer,
                            crate::session_player_ui::EditorTab::Main,
                        ),
                        "session-drummer-details" => (
                            crate::session_player::SessionPlayerKind::Drummer,
                            crate::session_player_ui::EditorTab::Details,
                        ),
                        "session-drummer-manual" => (
                            crate::session_player::SessionPlayerKind::Drummer,
                            crate::session_player_ui::EditorTab::Manual,
                        ),
                        "session-keyboard" => (
                            crate::session_player::SessionPlayerKind::Keyboard,
                            crate::session_player_ui::EditorTab::Main,
                        ),
                        "session-bass" => (
                            crate::session_player::SessionPlayerKind::Bass,
                            crate::session_player_ui::EditorTab::Main,
                        ),
                        _ => (
                            crate::session_player::SessionPlayerKind::Drummer,
                            crate::session_player_ui::EditorTab::Main,
                        ),
                    };
                    sp.kind = kind;
                    if let Some(t) = pr
                        .tracks
                        .iter()
                        .position(|t| t.kind == model::TrackKind::Drummer)
                    {
                        pr.tracks[t].regions.clear();
                        if let Some(ri) = pr.add_session_region(t, sp, 1.0) {
                            pr.selected_track = t;
                            app.ui.tracks.select_only(t, ri);
                            app.ui.bottom = Bottom::SessionPlayer;
                            cc.egui_ctx.data_mut(|d| {
                                d.insert_temp(egui::Id::unique("session_player_tab"), tab);
                            });
                        }
                    }
                }
                if scene == "pattern" || scene == "step-sequencer" || scene == "drummer" {
                    let pr = &mut app.project;
                    if let Some(t) = pr
                        .tracks
                        .iter()
                        .position(|t| t.kind == model::TrackKind::Drummer)
                    {
                        pr.tracks[t].regions.clear();
                        if let Some(ri) = pr.add_generated_region(t, scene == "drummer", 40.0) {
                            pr.selected_track = t;
                            app.ui.tracks.select_only(t, ri);
                            app.ui.bottom = Bottom::PianoRoll;
                        }
                    }
                }
                app.saved = file_commands::content(&app.project);
                app.history.reset(&app.project);
            }
            if !measuring {
                app.ui.recent = recent_files::Recent::load();
                app.ui.ui_scale = ui_scale::load();
            }
            let scene = std::env::var("PROWAVE_SHOT_SCENE").unwrap_or_default();
            if scene == "chooser" {
                app.ui.chooser = Some(start_screen::Chooser::default());
            }
            if scene == "chooser-details" {
                let mut c = start_screen::Chooser::default();
                c.details.show_details = true;
                app.ui.chooser = Some(c);
            }
            if scene == "updater" {
                app.updater.show_up_to_date();
            }
            if scene == "glass-menu" {
                app.ui.show_multimeter = true;
                app.ui.shot_top_menu = Some("Track".to_string());
            }
            if let Some(tab) = match scene.as_str() {
                "settings-general" => Some(project_settings::SettingsTab::General),
                "settings-sync" => Some(project_settings::SettingsTab::Synchronization),
                "settings-metronome" => Some(project_settings::SettingsTab::Metronome),
                "settings-recording" => Some(project_settings::SettingsTab::Recording),
                "settings-tuning" => Some(project_settings::SettingsTab::Tuning),
                "settings-audio" => Some(project_settings::SettingsTab::Audio),
                "settings-midi" => Some(project_settings::SettingsTab::Midi),
                "settings-appearance" => Some(project_settings::SettingsTab::Appearance),
                _ => None,
            } {
                app.ui.settings_tab = Some(tab);
            }
            if scene.starts_with("session-") {
                let pr = &mut app.project;
                let mut sp = crate::session_player::SessionPlayerParams::default();
                let (kind, tab) = match scene.as_str() {
                    "session-drummer" => (
                        crate::session_player::SessionPlayerKind::Drummer,
                        crate::session_player_ui::EditorTab::Main,
                    ),
                    "session-drummer-details" => (
                        crate::session_player::SessionPlayerKind::Drummer,
                        crate::session_player_ui::EditorTab::Details,
                    ),
                    "session-drummer-manual" => (
                        crate::session_player::SessionPlayerKind::Drummer,
                        crate::session_player_ui::EditorTab::Manual,
                    ),
                    "session-keyboard" => (
                        crate::session_player::SessionPlayerKind::Keyboard,
                        crate::session_player_ui::EditorTab::Main,
                    ),
                    "session-bass" => (
                        crate::session_player::SessionPlayerKind::Bass,
                        crate::session_player_ui::EditorTab::Main,
                    ),
                    _ => (
                        crate::session_player::SessionPlayerKind::Drummer,
                        crate::session_player_ui::EditorTab::Main,
                    ),
                };
                sp.kind = kind;
                if let Some(t) = pr
                    .tracks
                    .iter()
                    .position(|t| t.kind == model::TrackKind::Drummer)
                {
                    pr.tracks[t].regions.clear();
                    if let Some(ri) = pr.add_session_region(t, sp, 1.0) {
                        pr.selected_track = t;
                        app.ui.tracks.select_only(t, ri);
                        app.ui.bottom = Bottom::SessionPlayer;
                        cc.egui_ctx.data_mut(|d| {
                            d.insert_temp(egui::Id::unique("session_player_tab"), tab);
                        });
                    }
                }
            }
            if scene == "kestrel-eq" {
                app.ui.bottom = Bottom::Mixer;
                let mut ins = crate::model::Insert::new(prowave_engine::FxKind::KestrelEq);
                ins.params[prowave_engine::kestrel_eq::idx::MID_GAIN] = 6.0;
                ins.params[prowave_engine::kestrel_eq::idx::LOW_GAIN] = 3.5;
                ins.params[prowave_engine::kestrel_eq::idx::HPF] = 2.0;
                app.project.tracks[0].inserts = vec![ins];
                app.ui.mixer.fx_window = Some((0, 0));
            }
            if scene == "mixer-meters" {
                app.ui.bottom = Bottom::Mixer;
            }
            if scene == "pattern" || scene == "step-sequencer" {
                let pr = &mut app.project;
                if let Some(t) = pr
                    .tracks
                    .iter()
                    .position(|t| t.kind == model::TrackKind::Drummer)
                {
                    pr.tracks[t].regions.clear();
                    if let Some(ri) = pr.add_generated_region(t, false, 40.0) {
                        pr.selected_track = t;
                        app.ui.tracks.select_only(t, ri);
                        app.ui.bottom = Bottom::PianoRoll;
                    }
                }
            }
            let is_project = |a: &std::ffi::OsString| is_project_path(a.as_os_str());
            let args: Vec<std::ffi::OsString> = if measuring {
                std::env::args_os().skip(1).filter(is_project).collect()
            } else {
                std::env::args_os().skip(1).collect()
            };
            if !measuring {
                recovery::session_dir();
                let offers = recovery::sweep(&recovery::root(), std::time::SystemTime::now());
                for c in &offers {
                    tracing::info!(
                        "recovery copy of \"{}\" in {} ({})",
                        c.name,
                        c.dir.display(),
                        if c.unclean {
                            "that run did not end cleanly"
                        } else {
                            "that run was quit without saving"
                        }
                    );
                }
            }
            let has_project_arg = args.iter().any(|a| is_project(a));
            if let Some(file) = args.iter().find(|a| is_project(a)) {
                app.open(std::path::Path::new(file), &cc.egui_ctx);
            }
            if scene == "workspace" || scene == "tracks-and-mixer" {
                app.ui.show_library = true;
                app.ui.show_inspector = false;
                app.ui.show_lists = true;
                app.ui.bottom = Bottom::PianoRoll;
                app.ui.score_tab = false;
                app.ui.tracks.bar_w = 30.0;
                app.ui.tracks.scroll_x = 0.0;
                app.project.position_beats = 40.0;

                if !has_project_arg {
                    let green = egui::Color32::from_rgb(0x4e, 0xb0, 0x4a);
                    let blue = egui::Color32::from_rgb(0x2f, 0x70, 0xd0);

                    if let Some(t) = app.project.tracks.get_mut(0) {
                        t.color = green;
                    }
                    if let Some(t) = app.project.tracks.get_mut(1) {
                        t.color = green;
                    }
                    if let Some(t) = app.project.tracks.get_mut(2) {
                        t.color = green;
                    }
                    if let Some(t) = app.project.tracks.get_mut(3) {
                        t.color = blue;
                        t.name = "Drum Pattern".to_string();
                    }
                    if let Some(t) = app.project.tracks.get_mut(4) {
                        t.color = blue;
                        t.name = "Synth Bass".to_string();
                    }

                    for (ti, t) in app.project.tracks.iter_mut().enumerate() {
                        if let Some(r) = t.regions.first_mut() {
                            r.len_bars = 22.0;
                            if !r.notes.is_empty() {
                                r.notes = model::demo_notes((ti + 1) as u32, 22.0, 48, 76);
                            }
                        }
                    }
                }
            }
            if scene == "playback" {
                app.ui.show_library = false;
                app.ui.show_mixer = true;
                app.ui.bottom = Bottom::Mixer;
                app.project.playing = true;
                app.project.position_beats = 24.0;
            }
            if scene == "piano-roll" {
                app.ui.show_library = false;
                app.ui.bottom = Bottom::PianoRoll;
                app.ui.score_tab = false;
                app.project.position_beats = 16.0;
            }
            if scene == "inspector" {
                app.ui.show_inspector = true;
                app.ui.show_library = false;
                app.ui.inspector.region_open = true;
                app.ui.inspector.track_open = true;
            }
            if scene == "eq" {
                app.ui.show_inspector = true;
                app.ui.show_library = false;
                app.project.tracks[0].inserts =
                    vec![model::Insert::new(prowave_engine::FxKind::ChannelEq)];
                app.ui.mixer.fx_window = Some((0, 0));
            }
            if scene == "compressor" {
                app.ui.show_inspector = true;
                app.ui.show_library = false;
                app.project.tracks[0].inserts =
                    vec![model::Insert::new(prowave_engine::FxKind::Compressor)];
                app.ui.mixer.fx_window = Some((0, 0));
            }
            if scene == "sound-library" {
                app.ui.show_sound_library_manager = true;
            }
            if !no_chooser_flag && !has_project_arg {
                app.ui.chooser = Some(start_screen::Chooser::default());
            }
            for arg in args.iter().filter(|a| !is_project(a)) {
                if arg == "--play" {
                    app.project.playing = true;
                } else if arg == "--piano" {
                    app.ui.bottom = Bottom::PianoRoll;
                } else if !arg.to_string_lossy().starts_with("--") {
                    app.import(std::path::Path::new(arg), &cc.egui_ctx);
                }
            }
            Ok(Box::new(app))
        }),
    );
    // The app (and with it the engine's open media files) is gone by now.
    project_io::remove_session_media();
    recovery::end_session();
    // OpenGL can't draw to this window: the app never started, so try the system's own API.
    let step = std::env::var(graphics::STEP_VAR)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    if let Some(how) = result.as_ref().err().and_then(|e| {
        graphics::next_relaunch(
            step,
            started_backends,
            e,
            wayland,
            cfg!(target_os = "linux"),
        )
    }) {
        tracing::warn!("the window could not be drawn; relaunching ({how:?})");
        match graphics::relaunch(&how) {
            Ok(code) => std::process::exit(code),
            Err(e) => tracing::error!("could not relaunch: {e}"),
        }
    }
    if let (Some(out), Err(e)) = (&bench_out, &result) {
        bench::report_without_window(out, &e.to_string());
        return Ok(());
    }
    // A `--screenshot` that failed to save or timed out is an error for the script that ran it.
    let shot_code = shot_exit.load(std::sync::atomic::Ordering::Acquire);
    if shot_code != 0 {
        std::process::exit(shot_code);
    }
    result
}

fn resolve_pending_drops(
    ctx: &egui::Context,
    p: &mut model::Project,
    s: &mut UiState,
    release_pos: Option<egui::Pos2>,
) {
    if let Some(payload) = s.dragged_region.take() {
        egui::DragAndDrop::clear_payload(ctx);
        // Revert temporary timeline position back to pre-drag start_beat.
        let orig_bar = region_edit::landing_bar(p, payload.original_start_beats, 0.0);
        if let Some(reg) = p
            .tracks
            .get_mut(payload.track_index)
            .and_then(|t| t.regions.get_mut(payload.region_index))
        {
            reg.start_bar = orig_bar;
        }
        s.tracks.group_drag.clear();
        s.tracks.drag_by = (0.0, 0);

        if let Some(pos) = release_pos {
            match track_drop_slice::cross_panel_drop_target(
                pos,
                s.tracks_headers_rect,
                s.library_panel_rect,
            ) {
                Some(track_drop_slice::CrossPanelDropTarget::TrackHeaders) => {
                    if let Some(src) = track_drop_slice::source_from_region(
                        p,
                        payload.track_index,
                        payload.region_index,
                    ) {
                        s.track_drop_slice.open_at(src, Some(pos));
                    }
                }
                Some(track_drop_slice::CrossPanelDropTarget::SoundLibrary) => {
                    s.add_loop_modal
                        .open_from_region(p, payload.track_index, payload.region_index);
                }
                None => {}
            }
        }
    }
    if let Some(ul) = s.dragged_user_loop.take() {
        egui::DragAndDrop::clear_payload(ctx);
        if let Some(pos) = release_pos {
            if s.tracks_timeline_rect.is_some_and(|r| r.contains(pos)) {
                let beat = s.tracks_drop_beat.unwrap_or(0.0);
                let _ = user_loops::instantiate_user_loop_at_beat(p, &ul, beat);
            }
        }
    }
    s.mixer.fx_drag = None;
}

pub(crate) fn start_or_toggle_playback(p: &mut model::Project, s: &UiState) {
    let marquee_start = s
        .tracks
        .range
        .range
        .filter(|r| !r.is_empty())
        .map(|r| r.start);
    let region_start = if s.app_settings.play_from_selected_region {
        s.tracks
            .selected
            .iter()
            .filter_map(|&(ti, ri)| p.tracks.get(ti).and_then(|t| t.regions.get(ri)))
            .map(|r| p.bar_to_beats(f64::from(r.start_bar - 1.0)))
            .reduce(f64::min)
    } else {
        None
    };
    toggle_playback_with_options(
        p,
        if s.app_settings.play_from_marquee {
            marquee_start
        } else {
            None
        },
        s.app_settings.play_from_cycle,
        region_start,
        s.app_settings.play_from_last_locate,
    );
}

fn toggle_playback_with_options(
    p: &mut model::Project,
    marquee_start: Option<f64>,
    play_from_cycle: bool,
    selected_region_start: Option<f64>,
    play_from_last_locate: bool,
) {
    if !p.playing {
        if !play_from_last_locate {
            if let Some(start) = marquee_start {
                p.position_beats = start.max(0.0);
            } else if play_from_cycle && p.cycle && p.cycle_end > p.cycle_start {
                p.position_beats = p.cycle_start.max(0.0);
            } else if let Some(r_start) = selected_region_start {
                p.position_beats = r_start.max(0.0);
            }
        }
        p.playing = true;
    } else {
        p.playing = false;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_icon_decodes_as_a_square_rgba_image() {
        let icon = eframe::icon_data::from_png_bytes(super::APP_ICON_PNG).unwrap();
        assert_eq!(icon.width, icon.height);
        assert!(icon.width >= 256);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

    #[test]
    fn space_restarts_playback_from_selected_range_start_when_cycle_enabled() {
        let mut p = super::model::Project::empty();
        p.cycle = true;
        p.cycle_start = 8.0;
        p.cycle_end = 16.0;
        p.position_beats = 12.0;

        // With an explicit marquee range start, playback jumps to marquee start.
        super::toggle_playback_with_options(&mut p, Some(4.0), true, None, false);
        assert!(p.playing);
        assert!((p.position_beats - 4.0).abs() < 1e-9);

        // Stop and restart with no marquee range -> jumps to cycle_start.
        super::toggle_playback_with_options(&mut p, None, true, None, false);
        assert!(!p.playing);
        p.position_beats = 14.0;
        super::toggle_playback_with_options(&mut p, None, true, None, false);
        assert!(p.playing);
        assert!((p.position_beats - 8.0).abs() < 1e-9);

        // With play_from_last_locate enabled (the default), position_beats stays at current state.
        assert!(super::project_settings::AppSettings::default().play_from_last_locate);
        p.playing = false;
        p.position_beats = 14.0;
        super::toggle_playback_with_options(&mut p, Some(4.0), true, None, true);
        assert!(p.playing);
        assert!((p.position_beats - 14.0).abs() < 1e-9);

        // With cycle disabled and no marquee, position_beats stays unchanged.
        p.playing = false;
        p.cycle = false;
        p.position_beats = 3.0;
        super::toggle_playback_with_options(&mut p, None, false, None, false);
        assert!(p.playing);
        assert!((p.position_beats - 3.0).abs() < 1e-9);
    }

    #[test]
    fn releasing_pending_region_drag_in_headers_opens_create_track_modal_and_reverts_source() {
        let mut p = super::model::Project::empty();
        p.tracks[0]
            .regions
            .push(super::model::Region::new_midi("Drummer Groove", 1.0, 2.0));
        let mut s = super::UiState::new();
        s.tracks_headers_rect = Some(egui::Rect::from_min_max(
            egui::pos2(0.0, 80.0),
            egui::pos2(240.0, 800.0),
        ));
        s.library_panel_rect = Some(egui::Rect::from_min_max(
            egui::pos2(1100.0, 80.0),
            egui::pos2(1440.0, 800.0),
        ));
        // Simulate temporary timeline move during drag, then release over Track Headers.
        p.tracks[0].regions[0].start_bar = 5.0;
        s.dragged_region = Some(super::track_drop_slice::DraggedRegionPayload {
            track_index: 0,
            region_index: 0,
            region_name: "Drummer Groove".into(),
            is_midi: true,
            original_start_beats: 0.0,
        });
        let ctx = egui::Context::default();
        super::resolve_pending_drops(&ctx, &mut p, &mut s, Some(egui::pos2(120.0, 300.0)));
        assert!(s.track_drop_slice.pending.is_some());
        assert!((p.tracks[0].regions[0].start_bar - 1.0).abs() < 1e-4);
    }

    #[test]
    fn releasing_pending_region_drag_in_library_opens_add_loop_modal() {
        let mut p = super::model::Project::empty();
        p.tracks[0]
            .regions
            .push(super::model::Region::new_midi("Beat Loop", 1.0, 2.0));
        let mut s = super::UiState::new();
        s.library_panel_rect = Some(egui::Rect::from_min_max(
            egui::pos2(1100.0, 80.0),
            egui::pos2(1440.0, 800.0),
        ));
        s.dragged_region = Some(super::track_drop_slice::DraggedRegionPayload {
            track_index: 0,
            region_index: 0,
            region_name: "Beat Loop".into(),
            is_midi: true,
            original_start_beats: 0.0,
        });
        let ctx = egui::Context::default();
        super::resolve_pending_drops(&ctx, &mut p, &mut s, Some(egui::pos2(1250.0, 300.0)));
        assert!(s.add_loop_modal.open);
        assert_eq!(s.add_loop_modal.name, "Beat Loop");
    }

    #[test]
    fn dropping_saved_user_loop_onto_timeline_spawns_track_and_midi_region() {
        let mut p = super::model::Project::empty();
        let mut s = super::UiState::new();
        s.tracks_timeline_rect = Some(egui::Rect::from_min_max(
            egui::pos2(240.0, 80.0),
            egui::pos2(1100.0, 800.0),
        ));
        s.tracks_drop_beat = Some(8.0);
        s.dragged_user_loop = Some(super::user_loops::UserLoopEntry {
            id: "loop-user-kit".into(),
            name: "User Kit".into(),
            loop_type: super::user_loops::LoopKind::Loop,
            scale: super::user_loops::ScaleKind::Any,
            genre: "Electronic".into(),
            key: "C".into(),
            tempo: 120.0,
            descriptors: vec!["All Drums".into()],
            characters: vec!["Dry".into()],
            length_beats: 4.0,
            notes: vec![super::user_loops::SavedNote {
                start: 0.0,
                len: 0.5,
                pitch: 36,
                velocity: 100,
            }],
            source_audio: None,
            slice_frames: Vec::new(),
        });
        let ctx = egui::Context::default();
        super::resolve_pending_drops(&ctx, &mut p, &mut s, Some(egui::pos2(500.0, 300.0)));
        let added = p.tracks.last().unwrap();
        assert_eq!(added.name, "User Kit");
        assert_eq!(added.regions.len(), 1);
        assert!((added.regions[0].start_bar - 3.0).abs() < 1e-4);
    }
}
