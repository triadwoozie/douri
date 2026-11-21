//! The menu entries for moving around the Tracks area: selecting with the arrows and the Edit >
//! Select commands (`arrange_select`), zooming (`arrange_zoom`) and the grid (`snap_grid`).
//! The zoom, Scroll in Play and grid entries are in the Tracks area's own View and Snap menus
//! (as in Logic); the horizontal and vertical zoom and Z are in Navigate too.
//! `menus` asks here first: whether an entry is one of these, whether it can be chosen now,
//! whether it shows a check mark, and runs it. Their keys are in `key_commands`.
use crate::arrange_select as sel;
use crate::arrange_zoom as zoom;
use crate::i18n::tr;
use crate::model::Project;
use crate::tracks_area::TracksView;
use crate::UiState;

/// Every entry this module runs.
const LABELS: &[&str] = &[
    "Select Next Region",
    "Select Previous Region",
    "Add Next Region to Selection",
    "Add Previous Region to Selection",
    "Select Next Track",
    "Select Previous Track",
    "Add Next Track to Selection",
    "Add Previous Track to Selection",
    "Select Inside Locators",
    "Select Overlapped Regions",
    "Select Muted Regions",
    "Zoom Horizontal In",
    "Zoom Horizontal Out",
    // The Toolbar's ZOOM IN and ZOOM OUT keys (`side_panes`): the horizontal zoom.
    "Zoom In",
    "Zoom Out",
    "Zoom Vertical In",
    "Zoom Vertical Out",
    "Toggle Zoom to Fit Selection or All Contents",
    "Scroll in Play",
    "Waveform Zoom In",
    "Waveform Zoom Out",
    "Snap to Grid",
    "Snap Regions to Relative Value",
    "Snap Regions to Absolute Value",
];

/// The entries that show a check mark.
const CHECKABLE: &[&str] = &[
    "Scroll in Play",
    "Snap to Grid",
    "Snap Regions to Relative Value",
    "Snap Regions to Absolute Value",
];

/// Whether `label` is one of these and shows a check mark.
pub(crate) fn checkable(label: &str) -> bool {
    CHECKABLE.contains(&label)
}

/// Whether the entry is on; `None` when it is not one of these.
pub(crate) fn checked(label: &str, v: &TracksView) -> Option<bool> {
    Some(match label {
        "Scroll in Play" => v.zoom.scroll_in_play,
        "Snap to Grid" => v.grid.on,
        "Snap Regions to Relative Value" => v.grid.relative,
        "Snap Regions to Absolute Value" => !v.grid.relative,
        _ => return None,
    })
}

/// The entries' text in the Tracks area's own menus (the toolbar's View and Snap).
pub(crate) fn text(label: &str) -> &'static str {
    match label {
        "Zoom Horizontal In" => tr("Zoom Horizontal In"),
        "Zoom Horizontal Out" => tr("Zoom Horizontal Out"),
        "Zoom Vertical In" => tr("Zoom Vertical In"),
        "Zoom Vertical Out" => tr("Zoom Vertical Out"),
        "Toggle Zoom to Fit Selection or All Contents" => {
            tr("Toggle Zoom to Fit Selection or All Contents")
        }
        "Scroll in Play" => tr("Scroll in Play"),
        "Waveform Zoom In" => tr("Waveform Zoom In"),
        "Waveform Zoom Out" => tr("Waveform Zoom Out"),
        "Snap to Grid" => tr("Snap to Grid"),
        "Snap Regions to Relative Value" => tr("Snap Regions to Relative Value"),
        "Snap Regions to Absolute Value" => tr("Snap Regions to Absolute Value"),
        _ => tr("Zoom"),
    }
}

/// One entry of the toolbar's menus, as the menu bar shows it: greyed when it cannot be
/// chosen, a check mark when it is on, its key at the right. Returns the label when clicked.
pub(crate) fn item(
    ui: &mut egui::Ui,
    p: &Project,
    v: &TracksView,
    label: &'static str,
) -> Option<&'static str> {
    let mut b = egui::Button::new(text(label));
    if let Some(on) = checked(label, v) {
        b = b.selected(on);
    }
    if let Some(c) = crate::key_commands::combo_of(label) {
        b = b.shortcut_text(crate::key_commands::format_shortcut(
            &c.shortcut(),
            crate::key_commands::is_mac_keyboard(),
        ));
    }
    let clicked = ui
        .add_enabled(enabled_v(label, p, v).unwrap_or(false), b)
        .clicked();
    if clicked {
        ui.close();
    }
    clicked.then_some(label)
}

/// Whether `label` is one of the entries run here.
pub(crate) fn handles(label: &str) -> bool {
    LABELS.contains(&label)
}

/// Whether the entry can be chosen now; `None` when it is not one of these.
pub(crate) fn enabled(label: &str, p: &Project, s: &UiState) -> Option<bool> {
    enabled_v(label, p, &s.tracks)
}

fn enabled_v(label: &str, p: &Project, v: &TracksView) -> Option<bool> {
    Some(match label {
        "Select Next Region" | "Add Next Region to Selection" => {
            sel::region_target(p, v, true).is_some()
        }
        "Select Previous Region" | "Add Previous Region to Selection" => {
            sel::region_target(p, v, false).is_some()
        }
        "Select Next Track" | "Add Next Track to Selection" => {
            sel::track_target(p, v.show_hidden, true).is_some()
        }
        "Select Previous Track" | "Add Previous Track to Selection" => {
            sel::track_target(p, v.show_hidden, false).is_some()
        }
        "Select Inside Locators" => {
            p.cycle_end > p.cycle_start && sel::any_shown_region(p, v.show_hidden)
        }
        "Select Overlapped Regions" => !sel::overlapped(p, v.show_hidden).is_empty(),
        "Select Muted Regions" => !sel::muted(p, v.show_hidden).is_empty(),
        "Zoom Horizontal In" | "Zoom In" => v.bar_w < zoom::BAR_W.1,
        "Zoom Horizontal Out" | "Zoom Out" => v.bar_w > zoom::BAR_W.0,
        "Zoom Vertical In" => v.row_h < zoom::ROW_H.1,
        "Zoom Vertical Out" => v.row_h > zoom::ROW_H.0,
        "Toggle Zoom to Fit Selection or All Contents" => zoom::can_fit(p, v),
        "Waveform Zoom In" => v.zoom.wave_zoom < zoom::WAVE_MAX,
        "Waveform Zoom Out" => v.zoom.wave_zoom > 1.0,
        "Scroll in Play"
        | "Snap to Grid"
        | "Snap Regions to Relative Value"
        | "Snap Regions to Absolute Value" => true,
        _ => return None,
    })
}

/// Runs the entry; `false` when it is not one of these.
pub(crate) fn apply(label: &str, p: &mut Project, s: &mut UiState) -> bool {
    let v = &mut s.tracks;
    match label {
        "Select Next Region" => {
            sel::step_region(p, v, true, false);
        }
        "Select Previous Region" => {
            sel::step_region(p, v, false, false);
        }
        "Add Next Region to Selection" => {
            sel::step_region(p, v, true, true);
        }
        "Add Previous Region to Selection" => {
            sel::step_region(p, v, false, true);
        }
        "Select Next Track" => {
            sel::step_track(p, v, true, false);
        }
        "Select Previous Track" => {
            sel::step_track(p, v, false, false);
        }
        "Add Next Track to Selection" => {
            sel::step_track(p, v, true, true);
        }
        "Add Previous Track to Selection" => {
            sel::step_track(p, v, false, true);
        }
        "Select Inside Locators" => sel::select_inside_locators(p, v),
        "Select Overlapped Regions" => sel::select_overlapped(p, v),
        "Select Muted Regions" => sel::select_muted(p, v),
        "Zoom Horizontal In" | "Zoom In" => zoom::zoom_h(p, v, zoom::STEP),
        "Zoom Horizontal Out" | "Zoom Out" => zoom::zoom_h(p, v, 1.0 / zoom::STEP),
        "Zoom Vertical In" => zoom::zoom_v(p, s, zoom::STEP),
        "Zoom Vertical Out" => zoom::zoom_v(p, s, 1.0 / zoom::STEP),
        "Toggle Zoom to Fit Selection or All Contents" => zoom::toggle_fit(p, s),
        "Scroll in Play" => v.zoom.scroll_in_play = !v.zoom.scroll_in_play,
        "Waveform Zoom In" => zoom::zoom_wave(v, true),
        "Waveform Zoom Out" => zoom::zoom_wave(v, false),
        "Snap to Grid" => v.grid.on = !v.grid.on,
        "Snap Regions to Relative Value" => v.grid.relative = true,
        "Snap Regions to Absolute Value" => v.grid.relative = false,
        _ => return false,
    }
    true
}
