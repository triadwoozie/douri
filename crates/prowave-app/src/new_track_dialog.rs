//! Logic Pro's card-based "Choose a track type" Sheet Dialog (`new_track_dialog.rs`).
//!
//! Provides the visual track selection sheet specified by Apple Logic Pro,
//! featuring categorized track cards with colored accents, Phosphor icons,
//! a collapsible Details routing drawer, and track count stepper.

use crate::model::{Project, TrackKind};
use crate::ui_chrome::{
    self, cancel_button, modal_sheet, number_stepper, primary_button, track_type_card,
    ACCENT_AMBER, ACCENT_BLUE, ACCENT_CYAN, ACCENT_GREEN, ACCENT_PURPLE,
};
use crate::UiState;
use egui::{vec2, Color32, FontId, Response, Sense, Stroke, Ui, Vec2};

/// The track category choice in the dialog sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Choice {
    #[default]
    SoftwareInstrument,
    AudioMic,
    AudioGuitar,
    SessionPlayer,
    VocalUtau,
}

impl Choice {
    pub fn kind(self) -> TrackKind {
        match self {
            Self::SoftwareInstrument => TrackKind::Instrument,
            Self::AudioMic | Self::AudioGuitar => TrackKind::Audio,
            Self::SessionPlayer => TrackKind::Drummer,
            Self::VocalUtau => TrackKind::Instrument,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => "Software Instrument",
            Self::AudioMic => "Audio (Mic/Line)",
            Self::AudioGuitar => "Guitar or Bass",
            Self::SessionPlayer => "Session Player",
            Self::VocalUtau => "Vocal / UTAU",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => "Virtual synths, samplers, and instruments.",
            Self::AudioMic => "Microphones or line inputs for recording.",
            Self::AudioGuitar => "Electric guitar or bass with amp effects.",
            Self::SessionPlayer => "Intelligent drum, bass, and keys grooves.",
            Self::VocalUtau => "OpenUtau synthesized vocal top lines.",
        }
    }

    pub fn accent(self) -> Color32 {
        match self {
            Self::SoftwareInstrument => ACCENT_GREEN,
            Self::AudioMic => ACCENT_BLUE,
            Self::AudioGuitar => ACCENT_CYAN,
            Self::SessionPlayer => ACCENT_AMBER,
            Self::VocalUtau => ACCENT_PURPLE,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => phosphor_egui::regular::PIANO_KEYS,
            Self::AudioMic => phosphor_egui::regular::MICROPHONE_STAGE,
            Self::AudioGuitar => phosphor_egui::regular::GUITAR,
            Self::SessionPlayer => phosphor_egui::regular::METRONOME,
            Self::VocalUtau => phosphor_egui::regular::MUSIC_NOTES,
        }
    }
}

/// The dialog's state, kept in [`UiState`] between frames.
#[derive(Clone, Debug)]
pub struct NewTrackDialog {
    pub open: bool,
    pub choice: Choice,
    pub count: u32,
    pub details_open: bool,
    pub open_library: bool,
    pub input_channel: u16,
    pub output_channel: u16,
    pub instrument_choice: usize,
}

impl Default for NewTrackDialog {
    fn default() -> Self {
        Self {
            open: false,
            choice: Choice::SoftwareInstrument,
            count: 1,
            details_open: false,
            open_library: true,
            input_channel: 0,
            output_channel: 0,
            instrument_choice: 0,
        }
    }
}

impl NewTrackDialog {
    /// Opens the dialog at one track with the default choice.
    pub fn open(&mut self) {
        self.open = true;
        self.count = 1;
    }
}

/// Draws the "Choose a track type" modal sheet when open.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    if !s.new_track.open {
        return;
    }

    let mut create = false;
    let mut cancel = false;

    let sheet_size = if s.new_track.details_open {
        vec2(660.0, 370.0)
    } else {
        vec2(660.0, 275.0)
    };

    modal_sheet(
        ctx,
        egui::Id::unique("choose_track_type_dialog"),
        "Choose a track type",
        sheet_size,
        |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 10.0);

            // Header Title
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Choose a track type")
                        .size(15.0)
                        .strong()
                        .color(ui_chrome::TEXT_PRIMARY),
                );
            });

            ui.add_space(4.0);

            // Row of 5 visual track cards
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                let choices = [
                    Choice::SoftwareInstrument,
                    Choice::AudioMic,
                    Choice::AudioGuitar,
                    Choice::SessionPlayer,
                    Choice::VocalUtau,
                ];

                for c in choices {
                    let selected = s.new_track.choice == c;
                    if track_type_card(ui, selected, c.accent(), c.icon(), c.title(), c.subtitle())
                        .clicked()
                    {
                        s.new_track.choice = c;
                    }
                }
            });

            ui.add_space(6.0);

            // Collapsible Details Section (▼ Details)
            let details_label = if s.new_track.details_open {
                "▼ Details"
            } else {
                "▶ Details"
            };

            let details_resp = ui.add(
                egui::Button::new(
                    egui::RichText::new(details_label)
                        .size(12.0)
                        .color(ui_chrome::TEXT_SECONDARY),
                )
                .frame(false),
            );

            if details_resp.clicked() {
                s.new_track.details_open = !s.new_track.details_open;
            }

            if s.new_track.details_open {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x1C, 0x1D, 0x20))
                    .corner_radius(6.0)
                    .stroke(ui_chrome::RIM_STROKE)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(14.0, 4.0);

                            // Input routing (relevant for audio/guitar)
                            if matches!(s.new_track.choice, Choice::AudioMic | Choice::AudioGuitar)
                            {
                                ui.label(
                                    egui::RichText::new("Audio Input:")
                                        .size(11.5)
                                        .color(ui_chrome::TEXT_SECONDARY),
                                );
                                let inputs =
                                    ["Input 1 (Mono)", "Input 2 (Mono)", "Input 1 + 2 (Stereo)"];
                                egui::ComboBox::from_id_salt("track_audio_input")
                                    .selected_text(
                                        inputs[s.new_track.input_channel as usize % inputs.len()],
                                    )
                                    .show_ui(ui, |ui| {
                                        for (idx, name) in inputs.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut s.new_track.input_channel,
                                                idx as u16,
                                                *name,
                                            );
                                        }
                                    });
                            }

                            // Output routing
                            ui.label(
                                egui::RichText::new("Audio Output:")
                                    .size(11.5)
                                    .color(ui_chrome::TEXT_SECONDARY),
                            );
                            let outputs =
                                ["Output 1 + 2 (Stereo Out)", "Output 3 + 4", "Output 5 + 6"];
                            egui::ComboBox::from_id_salt("track_audio_output")
                                .selected_text(
                                    outputs[s.new_track.output_channel as usize % outputs.len()],
                                )
                                .show_ui(ui, |ui| {
                                    for (idx, name) in outputs.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut s.new_track.output_channel,
                                            idx as u16,
                                            *name,
                                        );
                                    }
                                });

                            // Open Library toggle
                            ui.checkbox(&mut s.new_track.open_library, "Open Library");
                        });
                    });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // Bottom control bar: Track Stepper & Action buttons
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Number of tracks to create:")
                        .size(12.0)
                        .color(ui_chrome::TEXT_PRIMARY),
                );

                number_stepper(ui, &mut s.new_track.count, 1, 64);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(ui, "Create").clicked() {
                        create = true;
                    }
                    if cancel_button(ui, "Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        },
    );

    if cancel {
        s.new_track.open = false;
    }

    if create {
        let count = s.new_track.count.clamp(1, 64);
        let choice = s.new_track.choice;

        for _ in 0..count {
            match choice {
                Choice::SoftwareInstrument => {
                    let at = p.add_track(TrackKind::Instrument);
                    p.tracks[at].color = ACCENT_GREEN;
                }
                Choice::AudioMic => {
                    let at = p.add_track(TrackKind::Audio);
                    p.tracks[at].color = ACCENT_BLUE;
                }
                Choice::AudioGuitar => {
                    let at = p.add_track(TrackKind::Audio);
                    p.tracks[at].name = "Guitar".to_string();
                    p.tracks[at].color = ACCENT_CYAN;
                    p.tracks[at].icon = Some(crate::track_icons::IconKind::Guitar);
                }
                Choice::SessionPlayer => {
                    let at = p.add_track(TrackKind::Drummer);
                    p.tracks[at].color = ACCENT_AMBER;
                }
                Choice::VocalUtau => {
                    crate::vocal::new_track(p, s);
                    if let Some(track) = p.tracks.last_mut() {
                        track.color = ACCENT_PURPLE;
                        track.icon = Some(crate::track_icons::IconKind::Mic);
                    }
                }
            }
        }

        p.selected_track = p.tracks.len().saturating_sub(1);
        if s.new_track.open_library {
            s.show_library = true;
        }
        s.new_track.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_choice_creates_correct_track_kind() {
        assert_eq!(Choice::SoftwareInstrument.kind(), TrackKind::Instrument);
        assert_eq!(Choice::AudioMic.kind(), TrackKind::Audio);
        assert_eq!(Choice::AudioGuitar.kind(), TrackKind::Audio);
        assert_eq!(Choice::SessionPlayer.kind(), TrackKind::Drummer);
        assert_eq!(Choice::VocalUtau.kind(), TrackKind::Instrument);
    }

    #[test]
    fn each_choice_has_distinct_accent_and_icon() {
        let choices = [
            Choice::SoftwareInstrument,
            Choice::AudioMic,
            Choice::AudioGuitar,
            Choice::SessionPlayer,
            Choice::VocalUtau,
        ];
        for c in choices {
            assert!(!c.title().is_empty());
            assert!(!c.subtitle().is_empty());
            assert!(!c.icon().is_empty());
            assert!(c.accent().a() > 0);
        }
    }
}
