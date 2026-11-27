//! The small panes behind control bar buttons: the Toolbar, the Note Pad and Quick Help.
use crate::i18n::tr;
use crate::model::Project;

/// Width of the Note Pad.
pub const NOTES_WIDTH: f32 = 260.0;
/// Height of the Toolbar row.
pub const TOOLBAR_HEIGHT: f32 = 30.0;

/// The longest note a project keeps, in characters.
pub const MAX_NOTES: usize = 100_000;

/// What the Toolbar offers: Logic Pro quick-action buttons wired to real menu commands.
const TOOLBAR: &[(&str, &str, &str)] = &[
    ("Split at Playhead", "Split at Playhead", "tb_split"),
    ("Join Regions", "Join", "tb_join"),
    ("Mute Region", "Mute On/Off", "tb_mute"),
    ("Loop Region", "Loop On/Off", "tb_loop"),
    ("Color", "Show Colors", "tb_color"),
    ("Bounce", "Project or Section…", "tb_bounce"),
    (
        "Set Locators by Selection",
        "Set Locators by Regions",
        "tb_locators",
    ),
    ("Zoom to Fit", "Zoom to Fit", "tb_zoom_fit"),
];

/// The Toolbar under the control bar (Logic: View > Show Toolbar).
pub fn toolbar(ui: &mut egui::Ui, p: &mut Project, s: &mut crate::UiState) {
    ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
    ui.horizontal_centered(|ui| {
        ui.add_space(8.0);
        for (i, &(label, cmd, help_id)) in TOOLBAR.iter().enumerate() {
            if i == 4 || i == 6 {
                ui.separator();
            }
            let on = crate::menus::enabled(cmd, p, s);
            let resp = ui.add_enabled(on, egui::Button::new(tr(label)));
            crate::quick_help::hover(ui.ctx(), &resp, help_id);
            if resp.clicked() {
                crate::menus::apply(cmd, p, s);
            }
        }
    });
}

/// The Note Pad: free text saved with the project (Logic: Project Notes).
pub fn note_pad(ui: &mut egui::Ui, p: &mut Project) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.strong(tr("Notes"));
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_sized(
            ui.available_size(),
            egui::TextEdit::multiline(&mut p.notes).char_limit(MAX_NOTES),
        );
    });
}
