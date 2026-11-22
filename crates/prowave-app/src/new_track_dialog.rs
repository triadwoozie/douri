//! Logic Pro's card-based "Create New Track" Modal Sheet (`new_track_dialog.rs`).
//!
//! Provides the visual track selection sheet matching Apple Logic Pro's layout:
//! single clean header, 5 distinct cards with bold circular 44x44px colored badges,
//! generous padding preventing label clipping, sub-selector pills, quick Voicebank selector,
//! collapsible 2-column Details routing drawer, and track count stepper.

use crate::model::{Project, TrackKind};
use crate::ui_chrome::{
    self, cancel_button, modal_sheet, number_stepper, primary_button, ACCENT_AMBER, ACCENT_BLUE,
    ACCENT_CYAN, ACCENT_GREEN,
};
use crate::UiState;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind};

pub const ACCENT_INDIGO: Color32 = Color32::from_rgb(0x5E, 0x5C, 0xE6);
pub const ACCENT_ROSE: Color32 = Color32::from_rgb(0xFF, 0x37, 0x5F);

/// The 5 primary track card choices in the dialog sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CardChoice {
    #[default]
    SoftwareInstrument,
    Pattern,
    SessionPlayer,
    Audio,
    VocalUtau,
}

// Aliases for compatibility
pub type Choice = CardChoice;

impl CardChoice {
    pub fn kind(self) -> TrackKind {
        match self {
            Self::SoftwareInstrument => TrackKind::Instrument,
            Self::Pattern => TrackKind::Drummer,
            Self::SessionPlayer => TrackKind::Drummer,
            Self::Audio => TrackKind::Audio,
            Self::VocalUtau => TrackKind::Instrument,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => "MIDI / Instrument",
            Self::Pattern => "Pattern",
            Self::SessionPlayer => "Session Player",
            Self::Audio => "Audio",
            Self::VocalUtau => "Vocal / UTAU",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => "Virtual synths, samplers, and MIDI.",
            Self::Pattern => "Step sequencer beats and melodic grids.",
            Self::SessionPlayer => "Intelligent drum, bass, and keys grooves.",
            Self::Audio => "Microphones, line inputs, and guitar amps.",
            Self::VocalUtau => "OpenUtau synthesized vocal top lines.",
        }
    }

    pub fn accent(self) -> Color32 {
        match self {
            Self::SoftwareInstrument => ACCENT_GREEN,
            Self::Pattern => ACCENT_INDIGO,
            Self::SessionPlayer => ACCENT_AMBER,
            Self::Audio => ACCENT_BLUE,
            Self::VocalUtau => ACCENT_ROSE,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => phosphor_egui::regular::PIANO_KEYS,
            Self::Pattern => phosphor_egui::regular::GRID_FOUR,
            Self::SessionPlayer => phosphor_egui::regular::METRONOME,
            Self::Audio => phosphor_egui::regular::MICROPHONE_STAGE,
            Self::VocalUtau => phosphor_egui::regular::MUSIC_NOTES,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MidiSub {
    #[default]
    SoftwareInstrument,
    ExternalMidi,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SessionPlayerSub {
    #[default]
    Drummer,
    BassPlayer,
    KeyboardPlayer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AudioSub {
    #[default]
    MicLine,
    GuitarBass,
}

/// The dialog's state, kept in [`UiState`] between frames.
#[derive(Clone, Debug)]
pub struct NewTrackDialog {
    pub open: bool,
    pub choice: CardChoice,
    pub midi_sub: MidiSub,
    pub session_sub: SessionPlayerSub,
    pub audio_sub: AudioSub,
    pub vocal_voicebank: String,
    pub count: u32,
    pub details_open: bool,
    pub open_library: bool,
    pub load_default_patch: bool,
    pub input_channel: u16,
    pub output_channel: u16,
    pub instrument_choice: usize,
}

impl Default for NewTrackDialog {
    fn default() -> Self {
        Self {
            open: false,
            choice: CardChoice::SoftwareInstrument,
            midi_sub: MidiSub::SoftwareInstrument,
            session_sub: SessionPlayerSub::Drummer,
            audio_sub: AudioSub::MicLine,
            vocal_voicebank: String::new(),
            count: 1,
            details_open: false,
            open_library: true,
            load_default_patch: true,
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

/// Draws the "Create New Track" modal sheet when open.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    if !s.new_track.open {
        return;
    }

    let mut create = false;
    let mut cancel = false;

    let sheet_size = if s.new_track.details_open {
        vec2(760.0, 440.0)
    } else {
        vec2(760.0, 320.0)
    };

    modal_sheet(
        ctx,
        egui::Id::unique("create_new_track_dialog"),
        "Create New Track",
        sheet_size,
        |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 10.0);

            // Row of 5 visual track cards with circular 44x44px badges
            let card_w = 138.0;
            let card_h = 168.0;

            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                const CARDS: [CardChoice; 5] = [
                    CardChoice::SoftwareInstrument,
                    CardChoice::Pattern,
                    CardChoice::SessionPlayer,
                    CardChoice::Audio,
                    CardChoice::VocalUtau,
                ];

                for c in CARDS {
                    let selected = s.new_track.choice == c;
                    let accent = c.accent();
                    let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                    let painter = ui.painter();

                    if resp.clicked() {
                        s.new_track.choice = c;
                    }

                    // Background fill & border
                    let bg_color = if selected {
                        Color32::from_rgb(0x22, 0x25, 0x2b)
                    } else if resp.hovered() {
                        Color32::from_rgb(0x24, 0x27, 0x2e)
                    } else {
                        Color32::from_rgb(0x1a, 0x1c, 0x20)
                    };

                    let border_stroke = if selected {
                        Stroke::new(1.5, Color32::from_rgb(0x2b, 0x6c, 0xb0))
                    } else if resp.hovered() {
                        Stroke::new(1.0, Color32::from_rgb(0x3e, 0x42, 0x4c))
                    } else {
                        Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38))
                    };

                    painter.rect(rect, 8.0, bg_color, border_stroke, StrokeKind::Inside);

                    // Circular 44x44px badge at top center
                    let circle_center = pos2(rect.center().x, rect.top() + 32.0);
                    let badge_bg = if selected {
                        Color32::from_rgb(0x2b, 0x6c, 0xb0)
                    } else if resp.hovered() {
                        Color32::from_rgb(0x28, 0x2b, 0x32)
                    } else {
                        Color32::from_rgb(0x20, 0x22, 0x26)
                    };
                    let badge_border = if selected {
                        Color32::from_rgb(0x3b, 0x82, 0xf6)
                    } else {
                        Color32::from_rgb(0x2e, 0x31, 0x38)
                    };
                    painter.circle_filled(circle_center, 22.0, badge_bg);
                    painter.circle_stroke(circle_center, 22.0, Stroke::new(1.0, badge_border));

                    // Monochrome Phosphor icon centered inside circular badge
                    painter.text(
                        circle_center,
                        Align2::CENTER_CENTER,
                        c.icon(),
                        FontId::proportional(22.0),
                        if selected {
                            Color32::WHITE
                        } else {
                            Color32::from_rgb(0x9a, 0xa0, 0xac)
                        },
                    );

                    // Title
                    let title_pos = pos2(rect.center().x, rect.top() + 66.0);
                    painter.text(
                        title_pos,
                        Align2::CENTER_CENTER,
                        c.title(),
                        FontId::proportional(12.5),
                        if selected {
                            ui_chrome::TEXT_PRIMARY
                        } else {
                            Color32::from_rgb(0xDC, 0xDC, 0xE0)
                        },
                    );

                    // Sub-selector / controls area inside the card
                    let sub_rect = Rect::from_min_max(
                        pos2(rect.left() + 6.0, rect.top() + 84.0),
                        pos2(rect.right() - 6.0, rect.bottom() - 6.0),
                    );

                    let mut sub_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(sub_rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                    );

                    match c {
                        CardChoice::SoftwareInstrument => {
                            let inst = s.new_track.midi_sub == MidiSub::SoftwareInstrument;
                            if sub_ui
                                .selectable_label(inst, "Instrument")
                                .on_hover_text("Virtual synths & samplers")
                                .clicked()
                            {
                                s.new_track.midi_sub = MidiSub::SoftwareInstrument;
                                s.new_track.choice = c;
                            }
                            if sub_ui
                                .selectable_label(!inst, "External MIDI")
                                .on_hover_text("External hardware MIDI synth/sound module")
                                .clicked()
                            {
                                s.new_track.midi_sub = MidiSub::ExternalMidi;
                                s.new_track.choice = c;
                            }
                        }
                        CardChoice::Pattern => {
                            sub_ui.label(
                                egui::RichText::new("Step Sequencer")
                                    .size(11.0)
                                    .color(ui_chrome::TEXT_SECONDARY),
                            );
                            sub_ui.label(
                                egui::RichText::new("Drum/synth patterns")
                                    .size(9.5)
                                    .color(ui_chrome::TEXT_MUTED),
                            );
                        }
                        CardChoice::SessionPlayer => {
                            if sub_ui
                                .selectable_label(
                                    s.new_track.session_sub == SessionPlayerSub::Drummer,
                                    "Drummer",
                                )
                                .clicked()
                            {
                                s.new_track.session_sub = SessionPlayerSub::Drummer;
                                s.new_track.choice = c;
                            }
                            if sub_ui
                                .selectable_label(
                                    s.new_track.session_sub == SessionPlayerSub::BassPlayer,
                                    "Bass Player",
                                )
                                .clicked()
                            {
                                s.new_track.session_sub = SessionPlayerSub::BassPlayer;
                                s.new_track.choice = c;
                            }
                        }
                        CardChoice::Audio => {
                            if sub_ui
                                .selectable_label(
                                    s.new_track.audio_sub == AudioSub::MicLine,
                                    "Mic / Line",
                                )
                                .clicked()
                            {
                                s.new_track.audio_sub = AudioSub::MicLine;
                                s.new_track.choice = c;
                            }
                            if sub_ui
                                .selectable_label(
                                    s.new_track.audio_sub == AudioSub::GuitarBass,
                                    "Guitar / Bass",
                                )
                                .clicked()
                            {
                                s.new_track.audio_sub = AudioSub::GuitarBass;
                                s.new_track.choice = c;
                            }
                        }
                        CardChoice::VocalUtau => {
                            sub_ui.label(
                                egui::RichText::new("OpenUtau Synth")
                                    .size(10.5)
                                    .color(ui_chrome::TEXT_SECONDARY),
                            );
                            let singers = prowave_vocal::discover_singers(&[]);
                            let cur_singer = if s.new_track.vocal_voicebank.is_empty() {
                                "Default (Built-in)"
                            } else {
                                &s.new_track.vocal_voicebank
                            };
                            egui::ComboBox::from_id_salt("card_voicebank_combo")
                                .selected_text(cur_singer)
                                .width(115.0)
                                .show_ui(&mut sub_ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            s.new_track.vocal_voicebank.is_empty(),
                                            "Default (Built-in)",
                                        )
                                        .clicked()
                                    {
                                        s.new_track.vocal_voicebank.clear();
                                    }
                                    for sng in &singers {
                                        let sel = s.new_track.vocal_voicebank == sng.name;
                                        if ui.selectable_label(sel, &sng.name).clicked() {
                                            s.new_track.vocal_voicebank = sng.name.clone();
                                        }
                                    }
                                });
                        }
                    }
                }
            });

            ui.add_space(4.0);

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
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            // Column 1: Audio / Instrument Routing
                            ui.vertical(|ui| {
                                ui.set_width(340.0);
                                if s.new_track.choice == CardChoice::Audio {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Audio Input:")
                                                .size(11.5)
                                                .color(ui_chrome::TEXT_SECONDARY),
                                        );
                                        let inputs = [
                                            "Input 1 (Mono)",
                                            "Input 2 (Mono)",
                                            "Input 1 + 2 (Stereo)",
                                        ];
                                        egui::ComboBox::from_id_salt("track_audio_input")
                                            .selected_text(
                                                inputs[s.new_track.input_channel as usize
                                                    % inputs.len()],
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
                                    });
                                } else {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Default Instrument:")
                                                .size(11.5)
                                                .color(ui_chrome::TEXT_SECONDARY),
                                        );
                                        const INSTRUMENTS: [&str; 4] = [
                                            "Vintage Electric Piano",
                                            "Sampler (Grand Piano)",
                                            "808 Bass",
                                            "Drum Machine",
                                        ];
                                        egui::ComboBox::from_id_salt("track_default_instrument")
                                            .selected_text(
                                                INSTRUMENTS[s.new_track.instrument_choice
                                                    % INSTRUMENTS.len()],
                                            )
                                            .show_ui(ui, |ui| {
                                                for (idx, name) in INSTRUMENTS.iter().enumerate() {
                                                    ui.selectable_value(
                                                        &mut s.new_track.instrument_choice,
                                                        idx,
                                                        *name,
                                                    );
                                                }
                                            });
                                    });
                                }

                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Audio Output:")
                                            .size(11.5)
                                            .color(ui_chrome::TEXT_SECONDARY),
                                    );
                                    let outputs = [
                                        "Output 1 + 2 (Stereo Out)",
                                        "Output 3 + 4",
                                        "Output 5 + 6",
                                    ];
                                    egui::ComboBox::from_id_salt("track_audio_output")
                                        .selected_text(
                                            outputs[s.new_track.output_channel as usize
                                                % outputs.len()],
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
                                });
                            });

                            ui.separator();

                            // Column 2: Checkboxes & Shortcuts
                            ui.vertical(|ui| {
                                ui.set_width(320.0);
                                ui.checkbox(&mut s.new_track.open_library, "Open Sound Library");
                                ui.checkbox(
                                    &mut s.new_track.load_default_patch,
                                    "Load Default Patch",
                                );

                                if s.new_track.choice == CardChoice::VocalUtau {
                                    if ui.button("Manage UTAU Singers…").clicked() {
                                        s.singer_manager.open();
                                    }
                                }
                            });
                        });
                    });
            }

            ui.add_space(6.0);
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
                CardChoice::SoftwareInstrument => {
                    if s.new_track.midi_sub == MidiSub::ExternalMidi {
                        let at = p.add_track(TrackKind::ExternalMidi);
                        p.tracks[at].name = "External MIDI".to_string();
                        p.tracks[at].icon = Some(crate::track_icons::IconKind::ExternalMidi);
                    } else {
                        let at = p.add_track(TrackKind::Instrument);
                        p.tracks[at].color = ACCENT_GREEN;
                    }
                }
                CardChoice::Pattern => {
                    let at = p.add_track(TrackKind::Drummer);
                    p.tracks[at].name = "Pattern".to_string();
                    p.tracks[at].color = ACCENT_INDIGO;
                }
                CardChoice::SessionPlayer => match s.new_track.session_sub {
                    SessionPlayerSub::Drummer => {
                        let at = p.add_track(TrackKind::Drummer);
                        p.tracks[at].color = ACCENT_AMBER;
                    }
                    SessionPlayerSub::BassPlayer => {
                        let at = p.add_track(TrackKind::Instrument);
                        p.tracks[at].name = "Bass Player".to_string();
                        p.tracks[at].color = ACCENT_AMBER;
                    }
                    SessionPlayerSub::KeyboardPlayer => {
                        let at = p.add_track(TrackKind::Instrument);
                        p.tracks[at].name = "Keyboard Player".to_string();
                        p.tracks[at].color = ACCENT_AMBER;
                    }
                },
                CardChoice::Audio => match s.new_track.audio_sub {
                    AudioSub::MicLine => {
                        let at = p.add_track(TrackKind::Audio);
                        p.tracks[at].color = ACCENT_BLUE;
                    }
                    AudioSub::GuitarBass => {
                        let at = p.add_track(TrackKind::Audio);
                        p.tracks[at].name = "Guitar".to_string();
                        p.tracks[at].color = ACCENT_CYAN;
                        p.tracks[at].icon = Some(crate::track_icons::IconKind::Guitar);
                    }
                },
                CardChoice::VocalUtau => {
                    crate::vocal::new_track(p, s);
                    let len = p.tracks.len();
                    if len >= 2 {
                        if !s.new_track.vocal_voicebank.is_empty() {
                            if let Some(crate::vocal::VocalRole::Score(set)) =
                                &mut p.tracks[len - 2].vocal
                            {
                                set.voice = s.new_track.vocal_voicebank.clone();
                                set.singer = crate::vocal::Singer::OpenUtau;
                            }
                            p.tracks[len - 2].name =
                                format!("{} Top Line", s.new_track.vocal_voicebank);
                            p.tracks[len - 1].name =
                                format!("{} Vocal", s.new_track.vocal_voicebank);
                        }
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
        assert_eq!(CardChoice::SoftwareInstrument.kind(), TrackKind::Instrument);
        assert_eq!(CardChoice::Pattern.kind(), TrackKind::Drummer);
        assert_eq!(CardChoice::SessionPlayer.kind(), TrackKind::Drummer);
        assert_eq!(CardChoice::Audio.kind(), TrackKind::Audio);
        assert_eq!(CardChoice::VocalUtau.kind(), TrackKind::Instrument);
    }

    #[test]
    fn each_choice_has_distinct_accent_and_icon() {
        const CARDS: [CardChoice; 5] = [
            CardChoice::SoftwareInstrument,
            CardChoice::Pattern,
            CardChoice::SessionPlayer,
            CardChoice::Audio,
            CardChoice::VocalUtau,
        ];
        for c in CARDS {
            assert!(!c.title().is_empty());
            assert!(!c.subtitle().is_empty());
            assert!(!c.icon().is_empty());
            assert!(c.accent().a() > 0);
        }
    }

    #[test]
    fn new_track_dialog_defaults_and_options() {
        let dlg = NewTrackDialog::default();
        assert!(!dlg.open);
        assert_eq!(dlg.count, 1);
        assert!(dlg.open_library);
        assert_eq!(dlg.choice, CardChoice::SoftwareInstrument);
    }
}
