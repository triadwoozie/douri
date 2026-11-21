//! Top menu bar with the same menus and entries as Logic Pro. Each entry is drawn like a macOS
//! menu item: check-mark column, title, key command on the right; entries that do not apply
//! now, or are not implemented yet, are greyed out (see [`enabled`]). The Smart Controls button in the control bar switches the bar and its
//! menus between the dark look and the light (white) macOS look (`UiState::light_menus`).
use crate::file_commands::FileCmd;
use crate::i18n::{self, tr, trf, Lang};
use crate::model::{Project, TrackKind};
use crate::project_settings::SettingsTab;
use crate::track_dialogs;
use crate::undo::UndoCmd;
use crate::Bottom;
use egui::{pos2, vec2, Align2, Color32, FontId, Key, KeyboardShortcut, Modifiers, Sense, Stroke};

const FILE: &[&str] = &[
    "New…",
    "New from Template…",
    "Open…",
    "Open Recent",
    "-",
    "Close",
    "Close Project",
    "Save",
    "Save As…",
    "Save A Copy As…",
    "Save as Template…",
    "Revert to",
    "-",
    "Alternatives",
    "Project Management",
    "Project Settings",
    "-",
    "Page Setup…",
    "Print",
    "-",
    "Movie",
    "-",
    "Import",
    "Export",
    "Bounce",
    "Share",
    "-",
    "Show in File Manager",
    "Open in Sound Library Manager",
    "-",
    "Project Alternatives…",
];

/// Entries that open a submenu (▸), as in Logic, and what is in each. "Open Recent" is filled
/// with the remembered projects instead.
const SUBMENUS: &[(&str, &[&str])] = &[
    ("Revert to", &["Revert to Saved"]),
    (
        "Alternatives",
        &[
            "New Alternative…",
            "Rename Alternative…",
            "Delete Alternative",
        ],
    ),
    (
        "Project Management",
        &["Clean Up…", "Consolidate…", "Rename Project…"],
    ),
    (
        "Project Settings",
        &[
            "General…",
            "Synchronization…",
            "Metronome…",
            "Recording…",
            "Tuning…",
            "Audio…",
            "MIDI…",
            "Appearance…",
            "MCP Server…",
            "Advanced…",
            "Plug-In Manager…",
        ],
    ),
    ("Movie", &["Open Movie…", "Remove Movie"]),
    ("Import", &["Audio File…", "MIDI File…", "Other Projects…"]),
    (
        "Export",
        &[
            "Selection as MIDI File…",
            "Track as Audio File…",
            "All Tracks as Audio Files…",
            "Score as MusicXML…",
        ],
    ),
    (
        "Bounce",
        &[
            "Project or Section…",
            "Regions in Place…",
            "Tracks in Place…",
        ],
    ),
    ("Share", &["Song to File…", "Song to Mobile Device…"]),
    ("Language", LANGUAGE),
    (
        "Cut/Insert Time",
        &[
            "Cut Section Between Locators",
            "Insert Silence Between Locators",
            "Repeat Section Between Locators",
        ],
    ),
    ("Convert", &["Convert Regions to New Audio Files"]),
    (
        "Nudge Value",
        &[
            "Set Nudge Value to Tick",
            "Set Nudge Value to Division",
            "Set Nudge Value to Beat",
            "Set Nudge Value to Bar",
        ],
    ),
    (
        "Select",
        &[
            "Select All",
            "Select All Following",
            "Select All Following of Same Track",
            "Select Inside Locators",
            "Select Overlapped Regions",
            "Select Muted Regions",
            "Select Similar Regions",
            "Select Equal Regions",
            "Select Same Colored Regions",
            "Invert Selection",
            "Deselect All",
            "-",
            "Select Next Region",
            "Select Previous Region",
            "Add Next Region to Selection",
            "Add Previous Region to Selection",
            "-",
            "Select Next Track",
            "Select Previous Track",
            "Add Next Track to Selection",
            "Add Previous Track to Selection",
        ],
    ),
    ("I/O Buffer Size", BUFFER_SIZES),
    ("Process Buffer", &["Small", "Medium", "Large"]),
    (
        "Space Designer",
        &[
            "Show",
            "Mastering Glue - Small Room",
            "Mastering Plate - Vocal Sheen",
            "Mastering Wide Hall",
        ],
    ),
    (
        "Snap",
        &[
            "Snap to Grid",
            "-",
            "Snap Regions to Relative Value",
            "Snap Regions to Absolute Value",
        ],
    ),
];

const BUFFER_SIZES: &[&str] = &[
    "32 Samples",
    "64 Samples",
    "128 Samples",
    "256 Samples",
    "512 Samples",
    "1024 Samples",
];

fn buffer_frames(label: &str) -> Option<u32> {
    if !BUFFER_SIZES.contains(&label) {
        return None;
    }
    label.split(' ').next()?.parse().ok()
}

/// What is in the submenu opened by `label`, if it opens one.
pub(crate) fn submenu(label: &str) -> Option<&'static [&'static str]> {
    SUBMENUS
        .iter()
        .find(|(title, _)| *title == label)
        .map(|(_, items)| *items)
}

/// Every entry that runs something, submenus included (not separators or submenu titles).
#[cfg(test)]
pub(crate) fn all_entries() -> Vec<&'static str> {
    let mut out = Vec::new();
    for (_, items) in MENUS {
        for &it in items {
            match submenu(it) {
                Some(children) => out.extend_from_slice(children),
                None => out.push(it),
            }
        }
    }
    out.retain(|it| *it != "-" && *it != "Open Recent");
    out
}
const EDIT: &[&str] = &[
    "Undo",
    "Redo",
    "Undo History…",
    "Delete Undo History",
    "-",
    "Cut",
    "Copy",
    "Paste",
    "Paste Replace",
    "Delete",
    "-",
    "Select",
    "Snap",
    "-",
    "Split at Playhead",
    "Join",
    "Bounce and Join",
    "Pack Take Folder",
    "Convert",
    "-",
    "Set Region Start to Playhead",
    "Set Region End to Playhead",
    "Repeat…",
    "Cut/Insert Time",
    "-",
    "Mute On/Off",
    "Loop On/Off",
    "Reverse Region",
    "Rename Regions",
    "Quantize Selected Events",
    "Move to Playhead",
    "Nudge Left",
    "Nudge Right",
    "Nudge Value",
    "Shuffle Left within Selection",
    "Shuffle Right within Selection",
    "-",
    "Color Regions by Track Color",
];
const TRACK: &[&str] = &[
    "New Tracks…",
    "New",
    "New Track with Duplicate Settings",
    "New Software Instrument Track",
    "New Audio Track",
    "New Aux Track",
    "New VCA Track",
    "New External MIDI Track",
    "New Vocal Top Line Track",
    "Vocal Top Line…",
    "New Pattern Region",
    "New Drummer Region",
    "MIDI FX…",
    "Rename Track",
    "Delete Track",
    "Delete Unused Tracks",
    "-",
    "Hide Track",
    "Show Hidden Tracks",
    "Unhide All Tracks",
    "-",
    "Toggle Track Mute",
    "Toggle Track Solo",
    "-",
    "Create Track Stack…",
    "Release Track Stack",
    "Show Master Track",
    "Show Editor",
    "Show Tempo Track",
    "Show Chord Track",
    "Show Marker Track",
    "Show Signature Track",
    "Show Arrangement Track",
    "-",
    "Assign Track Color…",
    "Assign Track Icon…",
    "Configure Track Header…",
];
const NAVIGATE: &[&str] = &[
    "Play",
    "Stop",
    "Play from Selection",
    "Play from Left Locator",
    "-",
    "Rewind",
    "Forward",
    "Fast Rewind",
    "Fast Forward",
    "-",
    "Go to Beginning",
    "Go to End",
    "Go to Position…",
    "-",
    "Cycle Mode",
    "Set Locators by Regions",
    "Go to Left Locator",
    "Go to Right Locator",
    "Move Locators Forward by Cycle Length",
    "Move Locators Backward by Cycle Length",
    "-",
    "Go to Next Marker",
    "Go to Previous Marker",
    "Go to Marker Number…",
    "Create Marker",
    "Create Marker Without Rounding",
    "Create Markers by Regions",
    "Delete Marker",
    "Set Locators by Marker and Enable Cycle",
    "-",
    "Go to Next Region",
    "Go to Previous Region",
    "-",
    "Zoom to Fit",
    "Toggle Zoom to Fit Selection or All Contents",
    "Zoom In",
    "Zoom Out",
    "Zoom Horizontal In",
    "Zoom Horizontal Out",
    "Zoom Vertical In",
    "Zoom Vertical Out",
];
const RECORD: &[&str] = &[
    "Record",
    "Record / Record Repeat",
    "Record Toggle",
    "Record Enable",
    "-",
    "Capture Recording",
    "-",
    "Software Monitoring",
    "Low Latency Mode",
    "I/O Buffer Size",
    "Process Buffer",
    "Record / Mix Mode",
    "Switch to Record Mode When Arming",
    "Send All Notes Off",
    "-",
    "New Take",
    "Delete Take",
    "Bounce In Place",
    "-",
    "Record Project Settings",
    "Project Settings",
];
const MIX: &[&str] = &[
    "Show Automation",
    "Create Automation",
    "Convert Automation",
    "Delete Automation",
    "-",
    "Group",
    "Edit Groups…",
    "-",
    "Pre-Fader Metering",
    "Mastering",
    "Create Mastering Track",
    "Stem Splitter…",
    "-",
    "Space Designer",
    "-",
    "Bounce",
    "Export",
    "Import",
];
/// Logic Pro 11's View menu.
const VIEW: &[&str] = &[
    "Hide All Automation",
    "Show Automation",
    "Create Track Automation",
    "Convert Automation",
    "-",
    "Show Region Automation",
    "Show Track Automation",
    "-",
    "Show Advanced Tools",
    "Show Smart Controls",
    "-",
    "Show Editor",
    "Show Piano Roll",
    "Show Score Editor",
    "Show Step Sequencer",
    "Show Session Player",
    "Show Flex Editor",
    "-",
    "Show Mixer",
    "Show Library",
    "Show List Editors",
    "Show Quick Help",
    "Show Inspector",
    "Show Performers",
    "Show Project Notes",
    "Show Movie",
    "Show Media Browser",
    "-",
    "Show Toolbar",
    "Customize Toolbar…",
    "-",
    "Show Track Header",
    "Show Ruler",
    "Show Global Tracks",
    "Show Colors",
    "Show Markers",
    "Show Grid",
    "Show Smart Tempo",
    "-",
    "Bigger Interface",
    "Smaller Interface",
    "Actual Interface Size",
    "-",
    "Enter Full Screen",
];
const SETTINGS: &[&str] = &[
    "Appearance…",
    "MCP Server…",
    "Advanced…",
    "Plug-In Manager…",
];
const WINDOW: &[&str] = &[
    "Minimize",
    "Zoom",
    "-",
    "Score Editor",
    "Step Sequencer",
    "Smart Tempo",
    "Piano Roll",
    "Pitch Editor",
    "Mixer",
    "Show Musical Typing",
    "Plugin Manager",
    "MultiMeter",
    "Video",
    "-",
    "Bring All to Front",
];
const HELP: &[&str] = &[
    "Quick Help",
    "Logic-style Key Commands…",
    "Show Welcome Screen",
    "-",
    "Language",
    "-",
    "Check for Updates…",
    "Check for Updates Automatically",
    "-",
    "About Dōri",
];

/// Help > Language: every language, each named in its own language.
const LANGUAGE: &[&str] = &[
    "English (Default)",
    "简体中文",
    "日本語",
    "Français",
    "Deutsch",
    "Español",
    "Português (Portugal)",
    "Português (Brasil)",
    "Filipino",
    "Bahasa Indonesia",
    "हिन्दी",
    "Українська",
    "Norsk",
    "Svenska",
    "Suomi",
    "Polski",
    "Lietuvių",
];

/// The menus in bar order, used by the in-window bar and the macOS menu bar.
pub(crate) const MENUS: [(&str, &[&str]); 10] = [
    ("File", FILE),
    ("Edit", EDIT),
    ("Track", TRACK),
    ("Navigate", NAVIGATE),
    ("Record", RECORD),
    ("Mix", MIX),
    ("View", VIEW),
    ("Settings", SETTINGS),
    ("Window", WINDOW),
    ("Help", HELP),
];

/// Entries that can show a check mark (see [`checked`]).
pub(crate) const CHECKABLE: &[&str] = &[
    "Show Editor",
    "Show Tempo Track",
    "Show Marker Track",
    "Show Signature Track",
    "Show Master Track",
    "Show Piano Roll",
    "Show Mixer",
    "Show Library",
    "Show Inspector",
    "Show List Editors",
    "Show Quick Help",
    "Show Project Notes",
    "Show Media Browser",
    "Check for Updates Automatically",
    "Show Toolbar",
    "Show Track Header",
    "Show Ruler",
    "Show Colors",
    "Show Grid",
];

/// Whether `label` shows a check mark when it is on: [`CHECKABLE`] and the Language entries.
pub(crate) fn is_checkable(label: &str) -> bool {
    CHECKABLE.contains(&label)
        || Lang::from_native_name(label).is_some()
        || crate::arrange_nav::checkable(label)
}

/// Whether the menu bar is drawn inside the window. On macOS the menus go in the system menu
/// bar at the top of the screen instead (`native_menu`).
pub const IN_WINDOW: bool = !cfg!(target_os = "macos");

/// Colours of one menu look.
struct Look {
    bar: Color32,
    bar_text: Color32,
    menu: Color32,
    text: Color32,
    dim: Color32,
    hover: Color32,
    hover_text: Color32,
    separator: Color32,
}

const DARK: Look = Look {
    bar: Color32::from_rgb(0x22, 0x22, 0x25),
    bar_text: Color32::from_rgb(0xdc, 0xdc, 0xdc),
    menu: Color32::from_rgb(0x4a, 0x4a, 0x4c),
    text: Color32::from_rgb(0xe6, 0xe6, 0xe6),
    dim: Color32::from_rgb(0x8a, 0x8a, 0x8c),
    hover: Color32::from_rgb(0x2f, 0x6f, 0xd8),
    hover_text: Color32::WHITE,
    separator: Color32::from_rgb(0x5e, 0x5e, 0x60),
};

/// macOS light menu bar and menus, as in Logic with the system in light mode.
const LIGHT: Look = Look {
    bar: Color32::from_rgb(0xf5, 0xf5, 0xf5),
    bar_text: Color32::from_rgb(0x1d, 0x1d, 0x1f),
    menu: Color32::from_rgb(0xec, 0xec, 0xec),
    text: Color32::from_rgb(0x1d, 0x1d, 0x1f),
    dim: Color32::from_rgb(0xa8, 0xa8, 0xaa),
    hover: Color32::from_rgb(0x2f, 0x7b, 0xf6),
    hover_text: Color32::WHITE,
    separator: Color32::from_rgb(0xd2, 0xd2, 0xd4),
};

fn look(light: bool) -> &'static Look {
    if light {
        &LIGHT
    } else {
        &DARK
    }
}

/// Fill for the panel behind the menu bar.
pub fn bar_fill(light: bool) -> Color32 {
    look(light).bar
}

/// Logic's default key commands for the entries this app implements.
pub(crate) fn shortcut(label: &str) -> Option<KeyboardShortcut> {
    if let Some(cmd) = FileCmd::from_menu(label) {
        return cmd.shortcut();
    }
    match label {
        "Undo" => return Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)),
        "Redo" => {
            return Some(KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::Z,
            ))
        }
        "Delete" => return Some(KeyboardShortcut::new(Modifiers::NONE, Key::Delete)),
        "Snap to Grid" => return Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::G)),
        "Split at Playhead" => return Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::T)),
        _ => {}
    }
    let interface_key = match label {
        "Bigger Interface" => Some(Key::Equals),
        "Smaller Interface" => Some(Key::Minus),
        "Actual Interface Size" => Some(Key::Num0),
        _ => None,
    };
    if let Some(key) = interface_key {
        return Some(KeyboardShortcut::new(Modifiers::COMMAND, key));
    }
    if label == "Open Piano Roll" {
        return Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Num4));
    }
    if label == "Record / Mix Mode" {
        return Some(KeyboardShortcut::new(
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::L,
        ));
    }
    let key = match label {
        "Show Library" => Key::Y,
        "Show Inspector" => Key::I,
        "Show List Editors" => Key::D,
        "Show Mixer" => Key::X,
        "Show Piano Roll" => Key::P,
        // Logic's key command for Musical Typing.
        "Show Musical Typing" => {
            return Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::K));
        }
        _ => return crate::key_commands::combo_of(label).map(|c| c.shortcut()),
    };
    Some(KeyboardShortcut::new(Modifiers::NONE, key))
}

/// Whether track `t` has a Space Designer or has room for one.
fn space_designer_room(t: &crate::model::Track) -> bool {
    t.kind != TrackKind::Vca
        && (t.inserts.len() < prowave_engine::MAX_INSERTS
            || t.inserts
                .iter()
                .any(|i| i.kind == prowave_engine::FxKind::Reverb))
}

/// Whether the master bus has a Space Designer or room for one.
fn master_has_room(p: &Project) -> bool {
    p.master_inserts.len() < prowave_engine::MAX_INSERTS
        || p.master_inserts
            .iter()
            .any(|i| i.kind == prowave_engine::FxKind::Reverb)
}

/// Opens the Space Designer in `inserts` (adding one when there is none) as the effect window of
/// strip `strip`, set to mastering preset `preset` if given.
fn show_space_designer(
    inserts: &mut Vec<crate::model::Insert>,
    s: &mut crate::UiState,
    strip: usize,
    preset: Option<usize>,
) {
    use prowave_engine::FxKind;
    let slot = inserts
        .iter()
        .position(|i| i.kind == FxKind::Reverb)
        .or_else(|| {
            (inserts.len() < prowave_engine::MAX_INSERTS).then(|| {
                inserts.push(crate::model::Insert::new(FxKind::Reverb));
                inserts.len() - 1
            })
        });
    if let Some(slot) = slot {
        if let Some(i) = preset {
            inserts[slot].params = crate::space_designer_ui::mastering_preset(i);
        }
        s.mixer.fx_window = Some((strip, slot));
    }
}

/// Whether `label` can be chosen now. Entries that are not implemented yet are greyed out,
/// so a click never silently does nothing; others are greyed while there is nothing to act on.
pub(crate) fn enabled(label: &str, p: &Project, s: &crate::UiState) -> bool {
    if let Some(on) = crate::arrange_nav::enabled(label, p, s) {
        return on;
    }
    if let Some(on) = crate::select_similar::enabled(label, p, s) {
        return on;
    }
    if let Some(on) = crate::marker_number::enabled(label, p) {
        return on;
    }
    if FileCmd::from_menu(label).is_some() || is_checkable(label) && !CHECKABLE.contains(&label) {
        return true;
    }
    if SettingsTab::from_menu(label).is_some() {
        return true;
    }
    match label {
        "Undo" => s.can_undo,
        "Redo" => s.can_redo,
        "Delete Undo History" => s.can_undo || s.can_redo,
        "Delete" => {
            !s.tracks.selection().is_empty()
                || s.tracks.range.active().is_some()
                || p.tracks.len() > 1
                    && p.tracks
                        .get(p.selected_track)
                        .is_some_and(|t| t.kind.has_regions() && t.regions.is_empty())
        }
        "Copy" | "Cut" => !s.tracks.selection().is_empty() || s.tracks.range.active().is_some(),
        "Deselect All"
        | "Play from Selection"
        | "Set Locators by Regions"
        | "Select All Following"
        | "Select All Following of Same Track"
        | "Mute On/Off"
        | "Loop On/Off"
        | "Move to Playhead"
        | "Nudge Left"
        | "Nudge Right" => !s.tracks.selection().is_empty(),
        "Shuffle Left within Selection" | "Shuffle Right within Selection" => {
            crate::shuffle_edit::can_shuffle(p, &s.tracks)
        }
        "Rename Track" | "Assign Track Color…" | "Assign Track Icon…" => {
            p.tracks.get(p.selected_track).is_some()
        }
        "Configure Track Header…" | "Show Master Track" | "Show Automation" => true,
        "Delete Automation" => p
            .tracks
            .get(p.selected_track)
            .is_some_and(|t| t.auto.has_lane()),
        "Show" => p
            .tracks
            .get(p.selected_track)
            .is_some_and(space_designer_room),
        "Mastering Glue - Small Room" | "Mastering Plate - Vocal Sheen" | "Mastering Wide Hall" => {
            master_has_room(p)
        }
        "Paste" => crate::region_edit::can_paste(p, &s.region_clip),
        "Paste Replace" => crate::region_edit::can_paste(p, &s.region_clip),
        "Select All" | "Invert Selection" => p.tracks.iter().any(|t| !t.regions.is_empty()),
        "Set Region Start to Playhead" => crate::region_commands::can_set_start(p, &s.tracks),
        "Set Region End to Playhead" => crate::region_commands::can_set_end(p, &s.tracks),
        "Repeat…" | "Rename Regions" => {
            crate::region_commands::rename_start(p, &s.tracks).is_some()
        }
        "Reverse Region" => crate::region_commands::can_reverse(p, &s.tracks),
        "Quantize Selected Events" => crate::region_edit::can_quantize(p, &s.tracks),
        "Pack Take Folder" => crate::take_pack::can_pack(p, &s.tracks),
        "Split at Playhead" => crate::marquee::can_split_any(p, &s.tracks),
        "Join" => crate::region_edit::can_join(p, &s.tracks),
        "Snap to Grid" | "Snap Regions to Relative Value" | "Snap Regions to Absolute Value" => {
            true
        }
        "Edit Groups…"
        | "Show Musical Typing"
        | "Plug-In Manager…"
        | "Plugin Manager"
        | "MultiMeter"
        | "Pitch Editor" => true,
        "Record / Record Repeat" | "Record Toggle" | "Record Project Settings" => true,
        "Record Enable" => p.tracks.get(p.selected_track).is_some(),
        "Score as MusicXML…" => p
            .tracks
            .iter()
            .any(|t| t.regions.iter().any(|r| !r.notes.is_empty())),
        "Undo History…" => true,
        "Audio File…" | "Other Projects…" => true,
        "Project or Section…" => !s.bouncing && p.tracks.iter().any(|t| !t.regions.is_empty()),
        "Track as Audio File…" => {
            !s.bouncing
                && p.tracks
                    .get(p.selected_track)
                    .is_some_and(|t| !t.regions.is_empty())
        }
        "All Tracks as Audio Files…" => {
            !s.bouncing && p.tracks.iter().any(|t| !t.regions.is_empty())
        }
        "Regions in Place…" | "Bounce In Place" | "Bounce and Join" => {
            !s.bouncing
                && s.tracks.selected.is_some_and(|(t, r)| {
                    p.tracks
                        .get(t)
                        .and_then(|t| t.regions.get(r))
                        .is_some_and(|r| r.audio.is_none() && !r.notes.is_empty())
                })
        }
        "Show in File Manager" => s.has_path,
        "Delete Track" => p.tracks.len() > 1,
        "Hide Track" => p.tracks.get(p.selected_track).is_some(),
        "Unhide All Tracks" => crate::track_commands::any_hidden(p),
        "Color Regions by Track Color" => crate::region_colors::can_reset(p, s),
        "Delete Unused Tracks" => crate::track_commands::can_delete_unused(p),
        "Release Track Stack" => p
            .tracks
            .get(p.selected_track)
            .is_some_and(|t| t.kind == TrackKind::Folder),
        "New Track with Duplicate Settings" => crate::track_selection::can_duplicate(p, &s.tracks),
        "Create Track Stack…" => crate::track_selection::can_create_stack(p, &s.tracks),
        "Toggle Track Mute" | "Toggle Track Solo" => p.tracks.get(p.selected_track).is_some(),
        "Go to Next Marker" => p.next_marker(p.position_beats).is_some(),
        "Go to Previous Marker" => p.previous_marker(p.position_beats).is_some(),
        "Delete Marker" | "Set Locators by Marker and Enable Cycle" => {
            crate::marker_edit::target(p, &s.tracks).is_some()
        }
        "Create Markers by Regions" => !s.tracks.selection().is_empty(),
        "Cut Section Between Locators"
        | "Insert Silence Between Locators"
        | "Repeat Section Between Locators" => crate::section_edit::can_apply(p),
        "Move Locators Forward by Cycle Length" => p.cycle_end > p.cycle_start,
        "Move Locators Backward by Cycle Length" => crate::cycle_area::can_move_back(p),
        "New Pattern Region" | "New Drummer Region" => p
            .tracks
            .get(p.selected_track)
            .is_some_and(|t| t.kind.has_regions() && t.kind != TrackKind::Audio),
        "Go to Next Region" | "Go to Previous Region" => {
            p.tracks.iter().any(|t| !t.regions.is_empty())
        }
        "New from Template…"
        | "New"
        | "New Tracks…"
        | "New Software Instrument Track"
        | "New Audio Track"
        | "New Aux Track"
        | "New VCA Track"
        | "New External MIDI Track"
        | "New Vocal Top Line Track"
        | "Vocal Top Line…"
        | "Go to Beginning"
        | "Go to End"
        | "Go to Position…"
        | "Go to Left Locator"
        | "Go to Right Locator"
        | "Play"
        | "Stop"
        | "Play from Left Locator"
        | "Rewind"
        | "Forward"
        | "Fast Rewind"
        | "Fast Forward"
        | "Cycle Mode"
        | "Zoom to Fit"
        | "Zoom In"
        | "Zoom Out"
        | "Zoom Horizontal In"
        | "Zoom Horizontal Out"
        | "Zoom Vertical In"
        | "Zoom Vertical Out"
        | "Record"
        | "Capture Recording"
        | "Quick Help"
        | "Show Quick Help"
        | "Show Toolbar"
        | "Show Project Notes"
        | "Show Media Browser"
        | "Show Piano Roll"
        | "Open Piano Roll"
        | "Show Mixer"
        | "Show Library"
        | "Show Inspector"
        | "Show List Editors"
        | "Check for Updates…"
        | "Check for Updates Automatically"
        | "Show Welcome Screen"
        | "About Dōri"
        | "Show Tempo Track"
        | "Show Marker Track"
        | "Show Signature Track"
        | "Show Global Tracks"
        | "Show Hidden Tracks"
        | "Show Colors"
        | "Show Editor"
        | "MIDI File…"
        | "MIDI FX…"
        | "Selection as MIDI File…"
        | "Create Marker"
        | "Create Marker Without Rounding"
        | "Mixer"
        | "Piano Roll"
        | "Bigger Interface"
        | "Smaller Interface"
        | "Actual Interface Size" => true,
        "Select Next Region" | "Add Next Region to Selection" => {
            crate::arrange_select::region_target(p, &s.tracks, true).is_some()
        }
        "Select Previous Region" | "Add Previous Region to Selection" => {
            crate::arrange_select::region_target(p, &s.tracks, false).is_some()
        }
        "Select Next Track" | "Add Next Track to Selection" => {
            crate::arrange_select::track_target(p, s.tracks.show_hidden, true).is_some()
        }
        "Select Previous Track" | "Add Previous Track to Selection" => {
            crate::arrange_select::track_target(p, s.tracks.show_hidden, false).is_some()
        }
        "Select Inside Locators" => {
            p.cycle_end > p.cycle_start
                && crate::arrange_select::any_shown_region(p, s.tracks.show_hidden)
        }
        "Select Overlapped Regions" => {
            !crate::arrange_select::overlapped(p, s.tracks.show_hidden).is_empty()
        }
        "Select Muted Regions" => !crate::arrange_select::muted(p, s.tracks.show_hidden).is_empty(),
        _ => {
            crate::region_commands::NudgeValue::from_menu(label).is_some()
                || buffer_frames(label).is_some()
        }
    }
}

/// Entries that always work.
const ALWAYS: &[&str] = &[
    "Project or Section…",
    "All Tracks as Audio Files…",
    "Pitch Editor",
    "New Software Instrument Track",
    "New Audio Track",
    "New Tracks…",
    "New from Template…",
    "New",
    "New Aux Track",
    "New VCA Track",
    "New External MIDI Track",
    "New Vocal Top Line Track",
    "Vocal Top Line…",
    "Go to Beginning",
    "Go to End",
    "Go to Position…",
    "Go to Left Locator",
    "Go to Right Locator",
    "Play",
    "Stop",
    "Play from Left Locator",
    "Rewind",
    "Forward",
    "Fast Rewind",
    "Fast Forward",
    "Cycle Mode",
    "Zoom to Fit",
    "Zoom In",
    "Zoom Out",
    "Zoom Horizontal In",
    "Zoom Horizontal Out",
    "Zoom Vertical In",
    "Zoom Vertical Out",
    "Record",
    "Capture Recording",
    "Show Toolbar",
    "Show Project Notes",
    "Show Media Browser",
    "Show Piano Roll",
    "Show Mixer",
    "Show Inspector",
    "Show List Editors",
    "Check for Updates…",
    "Check for Updates Automatically",
    "Show Editor",
    "Show Tempo Track",
    "Show Marker Track",
    "Show Signature Track",
    "Show Global Tracks",
    "Show Hidden Tracks",
    "Show Colors",
    "Show Editor",
    "MIDI File…",
    "MIDI FX…",
    "Selection as MIDI File…",
    "Create Marker",
    "Create Marker Without Rounding",
    "Toggle Zoom to Fit Selection or All Contents",
    "Set Nudge Value to Tick",
    "Set Nudge Value to Division",
    "Set Nudge Value to Beat",
    "Set Nudge Value to Bar",
    "Mixer",
    "Open Piano Roll",
    "Bigger Interface",
    "Smaller Interface",
    "Actual Interface Size",
    "Select Inside Locators",
    "Select Overlapped Regions",
    "Select Muted Regions",
    "Select Next Region",
    "Select Previous Region",
    "Add Next Region to Selection",
    "Add Previous Region to Selection",
    "Select Next Track",
    "Select Previous Track",
    "Add Next Track to Selection",
    "Add Previous Track to Selection",
];

/// Entries that work only in some states (see [`enabled`]).
const CONTEXTUAL: &[&str] = &[
    "Create Track Stack…",
    "Toggle Track Mute",
    "Toggle Track Solo",
    "Shuffle Left within Selection",
    "Shuffle Right within Selection",
    "Color Regions by Track Color",
    "Delete Unused Tracks",
    "Hide Track",
    "Unhide All Tracks",
    "New Track with Duplicate Settings",
    "Show Automation",
    "Delete Automation",
    "Undo",
    "Redo",
    "Delete Undo History",
    "Delete",
    "Deselect All",
    "Cut",
    "Copy",
    "Paste",
    "Select All",
    "Invert Selection",
    "Select All Following",
    "Select All Following of Same Track",
    "Play from Selection",
    "Set Locators by Regions",
    "Mute On/Off",
    "Loop On/Off",
    "Move to Playhead",
    "Nudge Left",
    "Nudge Right",
    "Split at Playhead",
    "Join",
    "Set Region Start to Playhead",
    "Set Region End to Playhead",
    "Repeat…",
    "Rename Regions",
    "Reverse Region",
    "Quantize Selected Events",
    "Pack Take Folder",
    "Rename Track",
    "Assign Track Color…",
    "Assign Track Icon…",
    "Configure Track Header…",
    "Show Master Track",
    "Show",
    "Mastering Glue - Small Room",
    "Mastering Plate - Vocal Sheen",
    "Mastering Wide Hall",
    "Delete Track",
    "Release Track Stack",
    "Go to Next Marker",
    "Go to Previous Marker",
    "Delete Marker",
    "Create Markers by Regions",
    "Set Locators by Marker and Enable Cycle",
    "Cut Section Between Locators",
    "Insert Silence Between Locators",
    "Repeat Section Between Locators",
    "Move Locators Forward by Cycle Length",
    "Move Locators Backward by Cycle Length",
    "New Pattern Region",
    "New Drummer Region",
    "Go to Next Region",
    "Go to Previous Region",
    "Regions in Place…",
    "Tracks in Place…",
    "Convert Regions to New Audio Files",
];

/// Whether `label` does anything at all.
pub(crate) fn implemented(label: &str) -> bool {
    if label == "-" {
        return false;
    }
    if let Some(children) = submenu(label) {
        return children.iter().any(|c| implemented(c));
    }
    label == "Open Recent"
        || FileCmd::from_menu(label).is_some()
        || SettingsTab::from_menu(label).is_some()
        || Lang::from_native_name(label).is_some()
        || ALWAYS.contains(&label)
        || CONTEXTUAL.contains(&label)
        || crate::arrange_nav::handles(label)
        || crate::select_similar::handles(label)
        || crate::marker_number::handles(label)
        || buffer_frames(label).is_some()
}

/// `items` without what does nothing, and without separators left at an end or in a row.
pub(crate) fn shown(items: &[&'static str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for &it in items {
        if it == "-" {
            if out.last().is_some_and(|l| *l != "-") {
                out.push(it);
            }
        } else if implemented(it) {
            out.push(it);
        }
    }
    if out.last() == Some(&"-") {
        out.pop();
    }
    out
}

/// Start of every region, in beats, sorted (for Go to Next / Previous Region).
fn region_starts(p: &Project) -> Vec<f64> {
    let mut starts: Vec<f64> = p
        .tracks
        .iter()
        .flat_map(|t| t.regions.iter())
        .map(|r| p.region_start_beats(r))
        .collect();
    starts.sort_by(f64::total_cmp);
    starts
}

/// Whether `label` shows a check mark (what it shows is visible).
pub(crate) fn checked(label: &str, p: &Project, s: &crate::UiState) -> bool {
    if let Some(on) = crate::arrange_nav::checked(label, &s.tracks) {
        return on;
    }
    match label {
        "Show Editor" => matches!(s.bottom, Bottom::PianoRoll | Bottom::PitchEditor),
        "Show Piano Roll" => s.bottom == Bottom::PianoRoll,
        "Pitch Editor" | "Show Pitch Editor" => s.bottom == Bottom::PitchEditor,
        "Show Mixer" => s.show_mixer && s.bottom == Bottom::Mixer,
        "Show Library" => s.show_library,
        "Show Inspector" => s.show_inspector,
        "Show List Editors" => s.show_lists,
        "Show Quick Help" => s.show_quick_help,
        "Show Toolbar" => s.show_toolbar,
        "Show Project Notes" => s.show_notes,
        "Show Media Browser" => s.browser.open == Some(crate::file_browser::Tab::Files),
        "Check for Updates Automatically" => s.update_auto,
        "Show Tempo Track" => s.tracks.show_tempo,
        "Show Marker Track" => s.tracks.show_markers,
        "Show Signature Track" => s.tracks.show_signatures,
        "Show Master Track" => p.show_master_track,
        "Snap to Grid" => s.tracks.grid.on,
        "Snap Regions to Relative Value" => s.tracks.grid.relative,
        "Snap Regions to Absolute Value" => !s.tracks.grid.relative,
        "Show Hidden Tracks" => s.tracks.show_hidden,
        "Show Global Tracks" => crate::track_commands::global_tracks_shown(s),
        _ if buffer_frames(label).is_some() => buffer_frames(label) == s.audio_prefs.buffer,
        _ if crate::region_commands::NudgeValue::from_menu(label).is_some() => {
            crate::region_commands::NudgeValue::from_menu(label) == Some(s.tracks.nudge)
        }
        _ if Lang::from_native_name(label).is_some() => {
            Lang::from_native_name(label) == Some(i18n::current())
        }
        // Always visible in this app.
        "Show Colors" => s.arrange.colors,
        "Show Track Header" | "Show Ruler" | "Show Grid" => true,
        _ => false,
    }
}

/// Track numbers shifted, so a remembered region or note selection may point at another track.
fn forget_region_selection(s: &mut crate::UiState) {
    s.tracks.clear_selection();
    s.piano.selected.clear();
}

/// Runs a menu entry (File entries are queued in `UiState::file_cmd` for `App`).
pub(crate) fn apply(label: &str, p: &mut Project, s: &mut crate::UiState) {
    if let Some(lang) = Lang::from_native_name(label) {
        i18n::choose(lang);
        return;
    }
    if crate::arrange_nav::apply(label, p, s) {
        return;
    }
    if crate::marker_number::apply(label, p, s) {
        return;
    }
    match label {
        _ if SettingsTab::from_menu(label).is_some() => {
            s.settings_tab = SettingsTab::from_menu(label)
        }
        "Rename Track"
        | "Assign Track Color…"
        | "Assign Track Icon…"
        | "Configure Track Header…" => {
            s.track_dialog = track_dialogs::open_for(label, p);
        }
        "Show Master Track" => p.show_master_track = !p.show_master_track,
        "Edit Groups…" => s.show_groups = true,
        // Window > Show Musical Typing (Cmd+K): the computer keyboard plays the selected
        // instrument; closing it lets go of every key.
        "Show Musical Typing" => {
            s.typing.open = !s.typing.open;
            if !s.typing.open {
                s.typing.release_all();
            }
        }
        "Plug-In Manager…" | "Plugin Manager" => s.show_plugin_manager = true,
        "MultiMeter" => s.show_multimeter = !s.show_multimeter,
        "Select All" => s.tracks.select_all(p),
        "Join" => crate::region_edit::join_selected(p, &mut s.tracks),
        "Show Mixer" | "Mixer" => {
            s.show_mixer = !(s.show_mixer && s.bottom == Bottom::Mixer);
            s.bottom = Bottom::Mixer;
        }
        "Show Library" => s.show_library = !s.show_library,
        "Show Inspector" => s.show_inspector = !s.show_inspector,
        "Show List Editors" => s.show_lists = !s.show_lists,
        "Show Quick Help" | "Quick Help" => s.show_quick_help = !s.show_quick_help,
        "Show Toolbar" => s.show_toolbar = !s.show_toolbar,
        "Show Project Notes" => s.show_notes = !s.show_notes,
        "Show Media Browser" => s.browser.toggle(crate::file_browser::Tab::Files),
        "Capture Recording" => s.capture_cmd = true,
        "Check for Updates…" => s.update_check = true,
        "Check for Updates Automatically" => {
            s.update_auto = !s.update_auto;
            crate::updater::set_auto_enabled(s.update_auto);
        }
        "Show Welcome Screen" => s.show_onboarding = true,
        "About Dōri" => s.show_about = true,
        "Show Tempo Track" => s.tracks.show_tempo = !s.tracks.show_tempo,
        "Show Marker Track" => s.tracks.show_markers = !s.tracks.show_markers,
        "Show Signature Track" => s.tracks.show_signatures = !s.tracks.show_signatures,
        // Logic: Create Marker puts it at the playhead, rounded to the nearest bar.
        "Create Marker" | "Create Marker Without Rounding" => {
            crate::marker_edit::create(p, p.position_beats, label == "Create Marker");
            s.tracks.selected_marker = None;
            s.tracks.show_markers = true;
        }
        "Create Markers by Regions" => {
            if crate::marker_edit::create_for_regions(p, &s.tracks).is_some() {
                s.tracks.selected_marker = None;
                s.tracks.show_markers = true;
            }
        }
        "Delete Marker" => crate::marker_edit::delete(p, &mut s.tracks),
        "Set Locators by Marker and Enable Cycle" => crate::marker_edit::set_locators(p, &s.tracks),
        "Toggle Zoom to Fit Selection or All Contents" => {
            let lanes_w = s.tracks.lanes_w;
            crate::tracks_area::zoom_to_fit(p, &mut s.tracks, lanes_w);
        }
        "Go to Next Marker" => {
            if let Some(b) = p.next_marker(p.position_beats) {
                p.position_beats = b;
            }
        }
        "Go to Previous Marker" => {
            if let Some(b) = p.previous_marker(p.position_beats) {
                p.position_beats = b;
            }
        }
        "Show Piano Roll" | "Piano Roll" => {
            s.bottom = if s.bottom == Bottom::PianoRoll {
                Bottom::Mixer
            } else {
                Bottom::PianoRoll
            }
        }
        "Pitch Editor" | "Show Pitch Editor" => {
            s.bottom = if s.bottom == Bottom::PitchEditor {
                Bottom::Mixer
            } else {
                Bottom::PitchEditor
            }
        }
        "New from Template…" => s.chooser = Some(crate::start_screen::Chooser::templates()),
        "New Tracks…" => {
            s.track_dialog = Some(track_dialogs::TrackDialog::NewTracks {
                kind: TrackKind::Instrument,
                count: 1,
            });
        }
        "New Track with Duplicate Settings" => {
            crate::track_selection::duplicate_tracks(p, s);
        }
        // Track > New: a software instrument track, as the + button above the tracks adds.
        "New" | "New Software Instrument Track" => {
            p.selected_track = p.add_track(TrackKind::Instrument);
        }
        "New Audio Track" => {
            p.selected_track = p.add_track(TrackKind::Audio);
        }
        "New VCA Track" => {
            p.add_track(TrackKind::Vca);
        }
        "New External MIDI Track" => {
            p.add_track(TrackKind::ExternalMidi);
        }
        "New Vocal Top Line Track" => crate::vocal::new_track(p, s),
        "Vocal Top Line…" => s.vocal.open = !s.vocal.open,
        "Hide Track" => crate::track_selection::hide_tracks(p, s),
        "Unhide All Tracks" => crate::track_commands::unhide_all(p),
        "Show Hidden Tracks" => s.tracks.show_hidden = !s.tracks.show_hidden,
        "Show Global Tracks" => crate::track_commands::toggle_global_tracks(s),
        "Show Colors" => s.arrange.colors = !s.arrange.colors,
        "Show Editor" => crate::track_commands::toggle_editor(p, s),
        "Color Regions by Track Color" => crate::region_colors::color_by_track(p, s),
        "Delete Unused Tracks" => {
            crate::track_commands::delete_unused(p, s);
        }
        "Toggle Track Mute" => crate::track_selection::toggle_mute(p, &s.tracks),
        "Toggle Track Solo" => crate::track_selection::toggle_solo(p, &s.tracks),
        "Set Region Start to Playhead" => {
            crate::region_commands::set_start_to_playhead(p, &s.tracks);
        }
        "Set Region End to Playhead" => {
            crate::region_commands::set_end_to_playhead(p, &s.tracks);
        }
        "Repeat…" => {
            s.track_dialog = Some(track_dialogs::TrackDialog::Repeat {
                copies: "1".into(),
                adjust: crate::region_commands::RepeatAdjust::Auto,
            });
        }
        "Rename Regions" => {
            if let Some(name) = crate::region_commands::rename_start(p, &s.tracks) {
                s.track_dialog = Some(track_dialogs::TrackDialog::RenameRegions { name });
            }
        }
        "Reverse Region" => {
            crate::region_commands::reverse(p, &s.tracks, None);
        }
        "Quantize Selected Events" => crate::region_edit::quantize_selected(p, &s.tracks),
        "Pack Take Folder" => crate::take_pack::pack(p, &mut s.tracks),
        _ if buffer_frames(label).is_some() => {
            s.audio_prefs.buffer = buffer_frames(label);
        }
        _ if crate::region_commands::NudgeValue::from_menu(label).is_some() => {
            if let Some(n) = crate::region_commands::NudgeValue::from_menu(label) {
                s.tracks.nudge = n;
                n.save();
            }
        }
        _ if crate::section_edit::SectionEdit::from_menu(label).is_some() => {
            if let Some(edit) = crate::section_edit::SectionEdit::from_menu(label) {
                crate::section_edit::apply(p, edit);
                forget_region_selection(s);
            }
        }
        "Move Locators Forward by Cycle Length" => crate::cycle_area::move_by_length(p, true),
        "Move Locators Backward by Cycle Length" => crate::cycle_area::move_by_length(p, false),
        "Play" => p.playing = true,
        "Stop" => crate::transport::stop(p),
        "Play from Selection" => {
            if let Some(at) = crate::region_edit::selection_start(p, &s.tracks) {
                p.position_beats = at;
                p.playing = true;
            }
        }
        "Play from Left Locator" => {
            p.position_beats = p.cycle_start;
            p.playing = true;
        }
        "Rewind" => crate::transport::rewind(p, 1),
        "Forward" => crate::transport::forward(p, 1),
        "Fast Rewind" => crate::transport::rewind(p, crate::transport::FAST_BARS),
        "Fast Forward" => crate::transport::forward(p, crate::transport::FAST_BARS),
        "Cycle Mode" => p.cycle = !p.cycle,
        "Go to Position…" => {
            s.track_dialog = Some(track_dialogs::TrackDialog::GoTo {
                text: track_dialogs::position_text(p, p.position_beats),
            });
        }
        "Set Locators by Regions" => crate::tracks_area::set_locators_by_region(p, &s.tracks),
        "Invert Selection" => crate::region_edit::invert_selection(p, &mut s.tracks),
        "Select All Following" => crate::region_edit::select_following(p, &mut s.tracks, false),
        "Select All Following of Same Track" => {
            crate::region_edit::select_following(p, &mut s.tracks, true)
        }
        "Mute On/Off" => {
            crate::region_edit::toggle_mute(p, &s.tracks, None);
        }
        "Loop On/Off" => {
            crate::region_edit::toggle_loop(p, &s.tracks);
        }
        "Move to Playhead" => crate::region_edit::move_to_playhead(p, &s.tracks),
        "Nudge Left" | "Nudge Right" => {
            crate::region_commands::nudge(p, s, label == "Nudge Right");
        }
        "Shuffle Left within Selection" | "Shuffle Right within Selection" => {
            crate::shuffle_edit::shuffle_selected(
                p,
                &s.tracks,
                label == "Shuffle Right within Selection",
            );
        }
        "Select Inside Locators" => crate::arrange_select::select_inside_locators(p, &mut s.tracks),
        "Select Overlapped Regions" => crate::arrange_select::select_overlapped(p, &mut s.tracks),
        "Select Muted Regions" => crate::arrange_select::select_muted(p, &mut s.tracks),
        "Select Similar Regions" | "Select Equal Regions" | "Select Same Colored Regions" => {
            crate::select_similar::apply(label, p, s);
        }
        "Select Next Region" => {
            crate::arrange_select::step_region(p, &mut s.tracks, true, false);
        }
        "Select Previous Region" => {
            crate::arrange_select::step_region(p, &mut s.tracks, false, false);
        }
        "Add Next Region to Selection" => {
            crate::arrange_select::step_region(p, &mut s.tracks, true, true);
        }
        "Add Previous Region to Selection" => {
            crate::arrange_select::step_region(p, &mut s.tracks, false, true);
        }
        "Select Next Track" => {
            crate::arrange_select::step_track(p, &mut s.tracks, true, false);
        }
        "Select Previous Track" => {
            crate::arrange_select::step_track(p, &mut s.tracks, false, false);
        }
        "Add Next Track to Selection" => {
            crate::arrange_select::step_track(p, &mut s.tracks, true, true);
        }
        "Add Previous Track to Selection" => {
            crate::arrange_select::step_track(p, &mut s.tracks, false, true);
        }
        "Open Piano Roll" => {
            s.bottom = Bottom::PianoRoll;
        }
        "MIDI FX…" => s.show_midi_fx = !s.show_midi_fx,
        // Mix > Space Designer > Show: the window of the selected track's Space Designer.
        "Show" => {
            let ti = p.selected_track;
            if let Some(t) = p.tracks.get_mut(ti) {
                show_space_designer(&mut t.inserts, s, ti, None);
            }
        }
        // The mastering presets: a Space Designer on Stereo Out, set to the preset.
        "Mastering Glue - Small Room" | "Mastering Plate - Vocal Sheen" | "Mastering Wide Hall" => {
            let preset = crate::space_designer_ui::MASTERING_PRESETS
                .iter()
                .position(|n| *n == label);
            show_space_designer(&mut p.master_inserts, s, crate::mixer::MASTER_STRIP, preset);
        }
        "MIDI File…" => s.midi_cmd = Some(crate::midi_file::MidiCmd::Import),
        "Audio File…" => s.import_audio_cmd = true,
        "Other Projects…" => s.open_band_cmd = true,
        "Selection as MIDI File…" => s.midi_cmd = Some(crate::midi_file::MidiCmd::Export),
        "Score as MusicXML…" => s.export_xml_cmd = true,
        "Project or Section…" => s.bounce_cmd = true,
        "Track as Audio File…" => s.export_track_cmd = true,
        "All Tracks as Audio Files…" => s.export_all_tracks_cmd = true,
        "Regions in Place…" | "Bounce In Place" => s.bounce_region_cmd = true,
        "Bounce and Join" => s.bounce_join_cmd = true,
        "Record Toggle" => p.recording = !p.recording,
        "Record Enable" => {
            if let Some(t) = p.tracks.get_mut(p.selected_track) {
                t.rec = !t.rec;
            }
        }
        "Record Project Settings" => s.settings_tab = Some(SettingsTab::Recording),
        "Show in File Manager" => s.reveal_cmd = true,
        "New Pattern Region" | "New Drummer Region" => {
            // On the selected track, at the bar the playhead is in.
            let bar = p.beats_to_bar(p.position_beats).floor() as f32 + 1.0;
            let track = p.selected_track;
            if let Some(ri) = p.add_generated_region(track, label == "New Drummer Region", bar) {
                s.tracks.select_only(track, ri);
            }
        }
        "New Aux Track" => {
            p.add_track(TrackKind::Aux);
        }
        "Undo" => s.undo_cmd = Some(UndoCmd::Undo),
        "Show Automation" => s.mixer.auto_editor.track = Some(p.selected_track),
        "Delete Automation" => {
            if let Some(t) = p.tracks.get_mut(p.selected_track) {
                let shown = t.auto.shown;
                t.auto.lane_mut(shown).clear();
                t.auto.prune();
            }
        }
        "Record / Record Repeat" => {
            // Record Repeat starts again where the cycle starts.
            if p.cycle {
                p.position_beats = p.cycle_start;
            }
            p.recording = true;
        }
        "Undo History…" => s.show_undo_history = true,
        "Redo" => s.undo_cmd = Some(UndoCmd::Redo),
        "Delete Undo History" => s.undo_cmd = Some(UndoCmd::Clear),
        "Delete" => {
            if s.tracks.range.active().is_some() {
                crate::marquee::delete(p, &mut s.tracks);
            } else {
                crate::tracks_area::delete_selected(p, &mut s.tracks);
            }
        }
        "Copy" => {
            if s.tracks.range.active().is_some() {
                crate::marquee::copy(p, &s.tracks, &mut s.region_clip);
            } else {
                crate::region_edit::copy_selected(p, &s.tracks, &mut s.region_clip);
            }
        }
        "Cut" => {
            if s.tracks.range.active().is_some() {
                crate::marquee::cut(p, &mut s.tracks, &mut s.region_clip);
            } else {
                crate::region_edit::cut_selected(p, &mut s.tracks, &mut s.region_clip);
            }
        }
        "Paste" => crate::region_edit::paste_at_playhead(p, &mut s.tracks, &s.region_clip),
        "Paste Replace" => crate::region_edit::paste_replace(p, &mut s.tracks, &s.region_clip),
        "Split at Playhead" => crate::marquee::split_any(p, &mut s.tracks),
        "Snap to Grid" => s.tracks.grid.on = !s.tracks.grid.on,
        "Snap Regions to Relative Value" => s.tracks.grid.relative = true,
        "Snap Regions to Absolute Value" => s.tracks.grid.relative = false,
        "Deselect All" => s.tracks.clear_selection(),
        "Go to End" => {
            p.position_beats = p
                .tracks
                .iter()
                .flat_map(|t| t.regions.iter())
                .map(|r| p.region_end_beats(r))
                .fold(0.0, f64::max);
        }
        "Go to Left Locator" => p.position_beats = p.cycle_start,
        "Go to Right Locator" => p.position_beats = p.cycle_end,
        "Go to Next Region" => {
            let at = p.position_beats + 1e-6;
            if let Some(b) = region_starts(p).into_iter().find(|b| *b > at) {
                p.position_beats = b;
            }
        }
        "Go to Previous Region" => {
            let at = p.position_beats - 1e-6;
            if let Some(b) = region_starts(p).into_iter().rev().find(|b| *b < at) {
                p.position_beats = b;
            }
        }
        "Zoom In" | "Zoom Horizontal In" => {
            crate::arrange_zoom::zoom_h(p, &mut s.tracks, crate::arrange_zoom::STEP);
        }
        "Zoom Out" | "Zoom Horizontal Out" => {
            crate::arrange_zoom::zoom_h(p, &mut s.tracks, 1.0 / crate::arrange_zoom::STEP);
        }
        "Zoom Vertical In" => {
            crate::arrange_zoom::zoom_v(p, s, crate::arrange_zoom::STEP);
        }
        "Zoom Vertical Out" => {
            crate::arrange_zoom::zoom_v(p, s, 1.0 / crate::arrange_zoom::STEP);
        }
        "Zoom to Fit" => {
            let w = s.tracks.lanes_w;
            crate::tracks_area::zoom_to_fit(p, &mut s.tracks, w);
        }
        "Create Track Stack…" => {
            crate::track_selection::create_stack(p, s);
        }
        "Bigger Interface" | "Smaller Interface" | "Actual Interface Size" => {
            let dir = match label {
                "Bigger Interface" => 1,
                "Smaller Interface" => -1,
                _ => 0,
            };
            s.ui_scale = crate::ui_scale::step(s.ui_scale, dir);
            crate::ui_scale::save(s.ui_scale);
        }
        "Delete Track" => {
            crate::track_selection::delete_tracks(p, s);
        }
        "Release Track Stack" => {
            if p.release_track_stack(p.selected_track) {
                forget_region_selection(s);
            }
        }
        "Go to Beginning" => p.position_beats = 0.0,
        "Record" => p.recording = !p.recording,
        _ => {
            if let Some(cmd) = FileCmd::from_menu(label) {
                s.file_cmd = Some(cmd);
            }
        }
    }
}

/// One menu entry that is always enabled (the Open Recent list).
/// Height of a menu entry and size of its text, in points (macOS menus).
const ROW_H: f32 = 19.0;
const FONT: f32 = 13.0;
/// Left column (check mark) and right padding of an entry.
const CHECK_W: f32 = 22.0;
const RIGHT_PAD: f32 = 10.0;
/// Gap between an entry's title and its key command or arrow.
const KEY_GAP: f32 = 28.0;

/// Width of a menu holding `labels` (and their key commands), so it is as wide as its longest
/// entry, like a macOS menu, instead of stretching across the window.
fn menu_width(ui: &egui::Ui, labels: &[String]) -> f32 {
    let font = FontId::proportional(FONT);
    let width = |t: &str| {
        ui.painter()
            .layout_no_wrap(t.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    let (mut title, mut key) = (0.0f32, 0.0f32);
    for it in labels.iter().filter(|l| *l != "-") {
        title = title.max(width(tr(it)));
        if let Some(k) = shortcut(it) {
            key = key.max(width(&ui.ctx().format_shortcut(&k)));
        }
        if submenu(it).is_some() || it == "Open Recent" {
            key = key.max(8.0);
        }
    }
    let gap = if key > 0.0 { KEY_GAP } else { 12.0 };
    (CHECK_W + title + gap + key + RIGHT_PAD).max(120.0)
}

/// One entry: check-mark column, title, then the key command or a submenu arrow on the right.
fn row(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    check: bool,
    key: Option<String>,
    arrow: bool,
    look: &Look,
) -> egui::Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), sense);
    let hot = enabled && resp.hovered();
    let g = ui.painter();
    if hot {
        g.rect_filled(rect.shrink2(vec2(0.0, 1.0)), 4.0, look.hover);
    }
    let color = match (enabled, hot) {
        (false, _) => look.dim,
        (true, true) => look.hover_text,
        (true, false) => look.text,
    };
    let font = FontId::proportional(FONT);
    if check {
        g.text(
            pos2(rect.left() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            "✔",
            FontId::proportional(11.0),
            color,
        );
    }
    g.text(
        pos2(rect.left() + CHECK_W, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font.clone(),
        color,
    );
    let right = pos2(rect.right() - RIGHT_PAD, rect.center().y);
    if arrow {
        let c = right + vec2(-3.0, 0.0);
        g.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-3.0, -4.0),
                c + vec2(3.0, 0.0),
                c + vec2(-3.0, 4.0),
            ],
            color,
            Stroke::NONE,
        ));
    } else if let Some(key) = key {
        let key_color = if hot { look.hover_text } else { look.dim };
        g.text(right, Align2::RIGHT_CENTER, key, font, key_color);
    }
    resp
}

fn separator(ui: &mut egui::Ui, look: &Look) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().line_segment(
        [pos2(rect.left() + 10.0, y), pos2(rect.right() - 10.0, y)],
        Stroke::new(1.0, look.separator),
    );
}

/// How a remembered project is listed: "Night Drive  (Music)", so equal names in different
/// folders can be told apart.
fn recent_label(path: &std::path::Path) -> String {
    let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("?");
    match path
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|d| d.to_str())
    {
        Some(folder) => format!("{name}  ({folder})"),
        None => name.to_string(),
    }
}

/// The Open Recent submenu: the remembered projects, then Clear Menu.
fn recent_items(ui: &mut egui::Ui, s: &mut crate::UiState, look: &Look) {
    let labels: Vec<String> = s.recent.paths().iter().map(|p| recent_label(p)).collect();
    let mut sized = labels.clone();
    sized.push(tr("Clear Menu").into());
    ui.set_width(menu_width(ui, &sized));
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut picked = None;
    for (label, path) in labels.iter().zip(s.recent.paths()) {
        if row(ui, label, true, false, None, false, look).clicked() {
            picked = Some(path.clone());
        }
    }
    if !labels.is_empty() {
        separator(ui, look);
    }
    let clear = row(
        ui,
        tr("Clear Menu"),
        !labels.is_empty(),
        false,
        None,
        false,
        look,
    );
    if let Some(path) = picked {
        s.recent_pick = Some(path);
        s.file_cmd = Some(FileCmd::OpenRecent);
        ui.close();
    } else if clear.clicked() {
        s.recent.clear();
        ui.close();
    }
}

/// The entries of one menu or submenu.
fn entries(ui: &mut egui::Ui, items: &[&str], p: &mut Project, s: &mut crate::UiState) {
    let look = look(s.light_menus);
    let labels: Vec<String> = items.iter().map(|it| it.to_string()).collect();
    ui.set_width(menu_width(ui, &labels));
    ui.spacing_mut().item_spacing.y = 0.0;
    for &it in items {
        if it == "-" {
            separator(ui, look);
            continue;
        }
        if it == "Open Recent" {
            let resp = row(ui, tr(it), true, false, None, true, look);
            egui::containers::menu::SubMenu::new().show(ui, &resp, |ui| recent_items(ui, s, look));
            continue;
        }
        if let Some(children) = submenu(it) {
            let on = children.iter().any(|c| enabled(c, p, s));
            let resp = row(ui, tr(it), on, false, None, true, look);
            if on {
                egui::containers::menu::SubMenu::new()
                    .show(ui, &resp, |ui| entries(ui, children, p, s));
            }
            continue;
        }
        let key = shortcut(it).map(|k| ui.ctx().format_shortcut(&k));
        let (on, check) = (enabled(it, p, s), checked(it, p, s));
        if row(ui, tr(it), on, check, key, false, look).clicked() {
            apply(it, p, s);
            ui.close();
        }
    }
}

fn menu(ui: &mut egui::Ui, title: &str, items: &[&str], p: &mut Project, s: &mut crate::UiState) {
    let title_text = egui::RichText::new(tr(title)).color(look(s.light_menus).bar_text);
    let (response, _inner) =
        egui::containers::menu::MenuButton::new(title_text).ui(ui, |ui| entries(ui, items, p, s));
    if s.shot_top_menu.as_deref() == Some(title) {
        egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
    }
}

/// Style of the bar buttons and of the drop-down menus for the current look.
fn menu_style(light: bool) -> impl Fn(&mut egui::Style) + Send + Sync + 'static {
    move |style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        let l = look(light);
        let hover_fill = if light {
            Color32::from_rgb(0xdc, 0xdc, 0xdc)
        } else {
            Color32::from_rgb(0x55, 0x55, 0x58)
        };
        let v = &mut style.visuals;
        v.window_fill = l.menu;
        v.window_stroke = Stroke::new(1.0, l.separator);
        v.menu_corner_radius = egui::CornerRadius::same(6);
        v.override_text_color = None;
        v.widgets.noninteractive.fg_stroke.color = l.text;
        v.widgets.inactive.fg_stroke.color = l.bar_text;
        v.widgets.hovered.weak_bg_fill = hover_fill;
        v.widgets.hovered.fg_stroke.color = l.bar_text;
        v.widgets.open.weak_bg_fill = hover_fill;
        v.widgets.open.fg_stroke.color = l.bar_text;
        v.widgets.active.fg_stroke.color = l.bar_text;
    }
}

/// Key commands of the View entries above (Y, I, X), unless a text field has focus.
pub fn handle_keys(ctx: &egui::Context, p: &mut Project, s: &mut crate::UiState) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    // While the Piano Roll's or the Tracks area's Tool menu is open, a plain letter picks a
    // tool (P Pencil, I Scissors), so the View keys wait.
    let view_keys: &[&str] = if s.piano.tool_menu_open() || s.tracks.tool_menu_open() {
        &[]
    } else {
        &[
            "Show Library",
            "Show Inspector",
            "Show Mixer",
            "Show Piano Roll",
            "Open Piano Roll",
            "Record / Mix Mode",
        ]
    };
    // A plain letter is not that letter with Shift (Shift-I and Shift-D are Logic's Invert
    // Selection and Deselect All, in `key_commands`).
    let shifted = ctx.input(|i| i.modifiers.shift);
    for &label in view_keys {
        // With Musical Typing open the letter keys are notes (Y, I and X included).
        if s.typing.open {
            break;
        }
        if let Some(k) = shortcut(label) {
            if shifted && !k.modifiers.shift {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&k)) {
                apply(label, p, s);
            }
        }
    }
    // Redo first: matching Cmd+Z ignores an extra Shift. Ctrl+Y is Redo on Windows and Linux.
    let redo = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    let redo_y = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
    let undo = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
    if ctx.input_mut(|i| i.consume_shortcut(&redo))
        || (!cfg!(target_os = "macos") && ctx.input_mut(|i| i.consume_shortcut(&redo_y)))
    {
        s.undo_cmd = Some(UndoCmd::Redo);
    } else if ctx.input_mut(|i| i.consume_shortcut(&undo)) {
        s.undo_cmd = Some(UndoCmd::Undo);
    }
    // Cmd/Ctrl + = also comes as Plus on keyboards where = needs no Shift.
    let bigger = KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus);
    if ctx.input_mut(|i| i.consume_shortcut(&bigger)) {
        apply("Bigger Interface", p, s);
    }
    for label in [
        "Bigger Interface",
        "Smaller Interface",
        "Actual Interface Size",
        "Show Musical Typing",
    ] {
        if let Some(k) = shortcut(label) {
            if ctx.input_mut(|i| i.consume_shortcut(&k)) {
                apply(label, p, s);
            }
        }
    }
}

/// The menu bar. Returns where its contents end, so the rest of the row can be used (on Linux
/// it holds the window buttons and moves the window, see `window_chrome`).
pub fn menu_bar(ui: &mut egui::Ui, p: &mut Project, s: &mut crate::UiState) -> f32 {
    let light = s.light_menus;
    let bar = egui::MenuBar::new()
        .style(menu_style(light))
        .config(egui::containers::menu::MenuConfig::new().style(menu_style(light)));
    bar.ui(ui, |ui| {
        ui.label(
            egui::RichText::new("Dōri")
                .strong()
                .color(look(light).bar_text),
        );
        for (title, items) in MENUS {
            menu(ui, title, items, p, s);
        }
        status_line(ui, p, s);
        ui.cursor().left()
    })
    .inner
}

/// Colour of the menu bar's text, for things drawn on the bar.
pub fn bar_text(light: bool) -> Color32 {
    look(light).bar_text
}

/// Whether [`status_line`] has anything to say.
pub fn has_status(p: &Project, s: &crate::UiState) -> bool {
    s.status.is_some() || p.tracks.len() > prowave_engine::MAX_TRACKS
}

/// The last error or warning, after the menus (or on its own line on macOS, where the menus
/// are in the system menu bar).
pub fn status_line(ui: &mut egui::Ui, p: &Project, s: &crate::UiState) {
    let warn = if s.light_menus {
        Color32::from_rgb(0xb0, 0x5a, 0x00)
    } else {
        Color32::from_rgb(0xff, 0xb0, 0x40)
    };
    if let Some(msg) = &s.status {
        ui.colored_label(warn, msg);
    }
    // The engine plays only its first MAX_TRACKS tracks; say so instead of failing silently.
    let max = prowave_engine::MAX_TRACKS;
    if p.tracks.len() > max {
        let text = trf(
            "Tracks {}–{} are silent (limit {})",
            &[&(max + 1), &p.tracks.len(), &max],
        );
        ui.colored_label(warn, text).on_hover_text(tr(
            "The audio engine plays up to 1024 tracks. Tracks after that are kept in the project but not played.",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_commands::content;

    /// Everything a menu entry can change, to compare before and after.
    fn state(p: &Project, s: &crate::UiState) -> String {
        format!(
            "{:?}",
            (
                content(p) == content(&Project::demo()),
                p.tracks.len(),
                (p.position_beats, p.recording, p.playing, p.cycle),
                (s.show_mixer, s.show_library, s.show_inspector, s.show_lists),
                (
                    s.update_check,
                    s.update_auto,
                    s.show_midi_fx,
                    s.settings_tab,
                    s.audio_prefs.buffer,
                ),
                (
                    s.capture_cmd,
                    s.show_quick_help,
                    s.show_toolbar,
                    s.show_notes,
                    s.browser.open
                ),
                s.bottom == Bottom::Mixer,
                (
                    s.file_cmd,
                    s.chooser.is_some(),
                    s.undo_cmd,
                    s.midi_cmd,
                    s.bounce_cmd,
                    s.export_track_cmd,
                    s.bounce_region_cmd,
                    s.bounce_join_cmd,
                    s.mixer.auto_editor.track,
                    // (a tuple holds twelve at most to print)
                    (
                        s.show_undo_history,
                        s.reveal_cmd,
                        s.import_audio_cmd,
                        s.export_xml_cmd,
                        s.open_band_cmd,
                    ),
                ),
                (
                    s.ui_scale,
                    (s.tracks.bar_w, s.tracks.row_h),
                    s.tracks.selected,
                    s.region_clip.len(),
                    s.tracks.extra.len(),
                    p.show_master_track,
                    p.header_hidden.len(),
                    s.track_dialog.is_some(),
                    p.tracks.get(p.selected_track).is_some_and(|t| t.rec),
                    (
                        s.show_groups,
                        s.typing.open,
                        s.vocal.open,
                        s.show_plugin_manager,
                        s.show_multimeter,
                        s.export_all_tracks_cmd,
                        s.show_about,
                        s.show_onboarding,
                    )
                ),
                (
                    s.tracks.show_hidden,
                    s.tracks.show_tempo,
                    s.tracks.show_markers,
                    s.tracks.show_signatures,
                    s.tracks.grid.on,
                    s.tracks.grid.relative,
                    s.arrange.colors,
                ),
                (i18n::chosen(), i18n::current()),
            )
        )
    }

    #[test]
    fn view_menu_checks_follow_the_panels() {
        let mut s = crate::UiState::new();
        let mut p = Project::demo();
        assert!(checked("Show Library", &p, &s) && checked("Show Mixer", &p, &s));
        apply("Show Library", &mut p, &mut s);
        assert!(!s.show_library && !checked("Show Library", &p, &s));
        apply("Show Piano Roll", &mut p, &mut s);
        assert!(checked("Show Piano Roll", &p, &s) && !checked("Show Mixer", &p, &s));
        apply("Show Mixer", &mut p, &mut s);
        assert!(checked("Show Mixer", &p, &s));
    }

    #[test]
    fn the_edit_groups_entry_is_available_and_opens_its_window() {
        let mut s = crate::UiState::new();
        let mut p = Project::demo();
        assert!(enabled("Edit Groups…", &p, &s), "usable even with no group");
        apply("Edit Groups…", &mut p, &mut s);
        assert!(s.show_groups);
    }

    #[test]
    fn file_entries_queue_their_command() {
        let mut s = crate::UiState::new();
        apply("Save As…", &mut Project::demo(), &mut s);
        assert_eq!(s.file_cmd, Some(FileCmd::SaveAs));
        assert!(shortcut("Show Library").is_some() && shortcut("Bounce").is_none());

        for (label, cmd) in [
            ("Save A Copy As…", FileCmd::SaveCopy),
            ("Revert to Saved", FileCmd::Revert),
        ] {
            assert!(all_entries().contains(&label), "{label} is in the menus");
            apply(label, &mut Project::demo(), &mut s);
            assert_eq!(s.file_cmd.take(), Some(cmd));
        }
    }

    #[test]
    fn every_enabled_entry_does_something() {
        // An entry that is enabled must be handled by `apply` (or open a file command):
        // a click must never silently do nothing.
        let (p, mut s) = (Project::demo(), crate::UiState::new());
        s.can_undo = true;
        s.can_redo = true;
        s.tracks.select_only(0, 0);
        for label in all_entries() {
            if !enabled(label, &p, &s) {
                continue;
            }
            let (mut p2, mut s2) = (Project::demo(), crate::UiState::new());
            s2.tracks.select_only(0, 0);
            let before = state(&p2, &s2);
            apply(label, &mut p2, &mut s2);
            // These two may already be where they would take you.
            // These may already be where they would take you.
            let may_stay = [
                "Go to Previous Region",
                "Go to Next Region",
                // The selected region may be the last one on its track.
                "Select All Following of Same Track",
                // The selected region may be at the start of the song, and so may the playhead.
                "Move to Playhead",
                "Nudge Left",
                // The selected region's notes may already sit on its grid.
                "Quantize Selected Events",
                "Rewind",
                "Fast Rewind",
                "Stop",
                "Actual Interface Size",
                "English (Default)",
                // The buffer sizes it starts with.
                "64 Samples",
                "Small",
                // The nudge value it starts with.
                "Set Nudge Value to Bar",
                "Set Nudge Value to Beat",
                "Set Nudge Value to Division",
                "Set Nudge Value to Tick",
                // Logic's default.
                "Snap Regions to Relative Value",
                // The selected region may be the only one like it.
                "Select Equal Regions",
                "Select Same Colored Regions",
                "Go to Beginning",
            ]
            .contains(&label);
            assert!(
                before != state(&p2, &s2) || may_stay,
                "{label} is enabled but does nothing"
            );
        }
        // And unimplemented ones are greyed, like the ones from the GitHub report.
        for label in ["Close", "Export", "Stem Splitter…"] {
            assert!(!enabled(label, &p, &s), "{label}");
        }
    }

    #[test]
    fn track_new_adds_an_instrument_track() {
        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        let n = p.tracks.len();
        apply("New", &mut p, &mut s);
        assert_eq!(p.tracks.len(), n + 1);
        assert_eq!(p.tracks[n].kind, TrackKind::Instrument);
    }

    #[test]
    fn undo_entries_follow_the_history_and_queue_their_command() {
        let (p, mut s) = (Project::demo(), crate::UiState::new());
        assert!(!enabled("Undo", &p, &s) && !enabled("Redo", &p, &s));
        s.can_undo = true;
        assert!(enabled("Undo", &p, &s) && enabled("Delete Undo History", &p, &s));
        apply("Undo", &mut Project::demo(), &mut s);
        assert_eq!(s.undo_cmd, Some(UndoCmd::Undo));
    }

    #[test]
    fn region_navigation_jumps_between_region_starts() {
        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        p.position_beats = 0.0;
        apply("Go to End", &mut p, &mut s);
        assert_eq!(
            p.position_beats,
            12.0 * 4.0,
            "the demo's regions end at bar 13"
        );
        apply("Go to Previous Region", &mut p, &mut s);
        assert_eq!(p.position_beats, 0.0);
    }

    #[test]
    fn space_designer_entries_open_its_window_and_the_mastering_presets_set_it() {
        use prowave_engine::{sd, FxKind};
        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        p.selected_track = 1;
        assert!(enabled("Show", &p, &s));
        let before_inserts = p.tracks[1].inserts.len();
        apply("Show", &mut p, &mut s);
        let slot = p.tracks[1]
            .inserts
            .iter()
            .position(|i| i.kind == FxKind::Reverb);
        assert!(
            slot.is_some(),
            "Show adds a Space Designer when there is none"
        );
        assert_eq!(s.mixer.fx_window, slot.map(|k| (1, k)));
        // A mastering preset goes on Stereo Out and sets the whole effect: a quiet wet level,
        // the dry sound at 0 dB.
        for (i, label) in crate::space_designer_ui::MASTERING_PRESETS
            .iter()
            .enumerate()
        {
            assert!(enabled(label, &p, &s));
            apply(label, &mut p, &mut s);
            assert_eq!(p.master_inserts.len(), 1, "one Space Designer, reused");
            assert_eq!(s.mixer.fx_window, Some((crate::mixer::MASTER_STRIP, 0)));
            let params = p.master_inserts[0].params;
            assert_eq!(params, crate::space_designer_ui::mastering_preset(i));
            assert!(
                params[sd::WET] <= -18.0 && params[sd::DRY] == 0.0,
                "{label}"
            );
        }
        assert_eq!(
            p.tracks[1].inserts.len(),
            before_inserts + 1,
            "tracks are left alone"
        );
    }

    #[test]
    fn new_aux_track_adds_an_aux_track() {
        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        assert!(TRACK.contains(&"New Aux Track"));
        let n = p.tracks.len();
        apply("New Aux Track", &mut p, &mut s);
        assert_eq!(p.tracks.len(), n + 1);
        assert_eq!(p.tracks[n].kind, TrackKind::Aux);
        assert_eq!(p.selected_track, n);
    }

    #[test]
    fn track_stack_entries_group_and_release_the_selected_track() {
        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        assert!(TRACK.contains(&"Create Track Stack…") && TRACK.contains(&"Release Track Stack"));
        p.selected_track = 2;
        s.tracks.select_only(2, 0);
        let n = p.tracks.len();
        apply("Create Track Stack…", &mut p, &mut s);
        assert_eq!(p.tracks.len(), n + 1);
        assert_eq!(
            (p.tracks[2].kind, p.tracks[3].indent),
            (TrackKind::Folder, 1)
        );
        assert_eq!(
            s.tracks.selected, None,
            "indexes moved, so the selection is dropped"
        );
        apply("Release Track Stack", &mut p, &mut s);
        assert_eq!(p.tracks.len(), n);
        assert!(p
            .tracks
            .iter()
            .all(|t| t.indent == 0 && t.kind != TrackKind::Folder));
    }

    #[test]
    fn recent_projects_are_listed_with_their_folder() {
        let label = recent_label(std::path::Path::new("/home/me/Music/Night Drive.prowave"));
        assert_eq!(label, "Night Drive  (Music)");
        assert_eq!(recent_label(std::path::Path::new("Solo.prowave")), "Solo");
    }

    #[test]
    fn the_file_menu_follows_logic_with_submenus() {
        for title in ["Revert to", "Import", "Export", "Bounce", "Share"] {
            assert!(FILE.contains(&title), "{title}");
            assert!(submenu(title).is_some(), "{title} opens a submenu");
        }
        // Every submenu is reachable from a menu.
        for (title, _) in SUBMENUS {
            assert!(
                MENUS.iter().any(|(_, items)| items.contains(title)),
                "{title}"
            );
        }
        // Revert to > Revert to Saved is enabled, so its submenu opens.
        let (p, s) = (Project::demo(), crate::UiState::new());
        assert!(submenu("Revert to")
            .unwrap()
            .iter()
            .any(|c| enabled(c, &p, &s)));
    }

    #[test]
    fn markers_are_created_at_the_nearest_bar_and_navigated() {
        let (mut p, mut s) = (Project::empty(), crate::UiState::new());
        assert!(!enabled("Go to Next Marker", &p, &s));
        p.position_beats = 5.0;
        apply("Create Marker", &mut p, &mut s);
        assert_eq!(p.markers[0].beat, 4.0, "bar 2 in 4/4");
        assert!(s.tracks.show_markers, "the Marker track opens to show it");
        p.position_beats = 15.0;
        apply("Create Marker", &mut p, &mut s);
        p.position_beats = 0.0;
        apply("Go to Next Marker", &mut p, &mut s);
        apply("Go to Next Marker", &mut p, &mut s);
        assert_eq!(p.position_beats, 16.0);
        apply("Go to Previous Marker", &mut p, &mut s);
        assert_eq!(p.position_beats, 4.0);
        apply("Delete Marker", &mut p, &mut s);
        assert_eq!(p.markers.len(), 1);
        assert_eq!(p.markers[0].beat, 16.0);
    }

    #[test]
    fn help_language_lists_every_language_by_its_own_name() {
        let all: Vec<&str> = Lang::ALL.iter().map(|l| l.native_name()).collect();
        assert_eq!(LANGUAGE, all.as_slice());
        assert!(HELP.contains(&"Language") && submenu("Language").is_some());

        let (mut p, mut s) = (Project::demo(), crate::UiState::new());
        assert!(
            checked("English (Default)", &p, &s),
            "English until another is picked"
        );
        apply("Deutsch", &mut p, &mut s);
        assert_eq!(i18n::current(), Lang::De);
        assert_eq!(i18n::chosen(), Some(Lang::De), "saved for the next start");
        assert!(checked("Deutsch", &p, &s) && !checked("English (Default)", &p, &s));
        assert_eq!(tr("Language"), "Sprache", "the menus switch at once");
        apply("English (Default)", &mut p, &mut s);
        assert!(checked("English (Default)", &p, &s));
    }
}
