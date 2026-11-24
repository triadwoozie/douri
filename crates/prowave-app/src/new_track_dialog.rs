//! Logic Pro's card-based "Create New Track" Modal Sheet (`new_track_dialog.rs`).
//!
//! Provides the visual 2×2 track selection sheet matching Apple Logic Pro's layout:
//! compact centered sheet (~440px wide), top-left Cancel action, centered bold title,
//! 2×2 grid of rounded cards with circular colored badges (`100% phosphor-egui` icons),
//! subtitle / interactive Session Player dropdown pill, circular `...` options button,
//! collapsible routing Details drawer, number stepper, and bottom Sound Library card.

use crate::model::{Project, TrackKind};
use crate::UiState;
use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind};

pub const ACCENT_GREEN: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x25, 0x63, 0xEB);
pub const ACCENT_INDIGO: Color32 = Color32::from_rgb(0x63, 0x66, 0xF1);
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xD9, 0x77, 0x06);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x06, 0xB6, 0xD4);
pub const ACCENT_ROSE: Color32 = Color32::from_rgb(0xFF, 0x37, 0x5F);

/// The 4 primary track card choices in the dialog sheet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CardChoice {
    #[default]
    SoftwareInstrument,
    Audio,
    Pattern,
    SessionPlayer,
}

impl CardChoice {
    pub fn kind(self) -> TrackKind {
        match self {
            Self::SoftwareInstrument => TrackKind::Instrument,
            Self::Audio => TrackKind::Audio,
            Self::Pattern => TrackKind::Drummer,
            Self::SessionPlayer => TrackKind::Drummer,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => "MIDI",
            Self::Audio => "Audio",
            Self::Pattern => "Pattern",
            Self::SessionPlayer => "Session Player",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => "Software Instrument",
            Self::Audio => "Default Input",
            Self::Pattern => "Software Instrument",
            Self::SessionPlayer => "Keyboard Player",
        }
    }

    pub fn accent(self) -> Color32 {
        match self {
            Self::SoftwareInstrument => ACCENT_GREEN,
            Self::Audio => ACCENT_BLUE,
            Self::Pattern => ACCENT_INDIGO,
            Self::SessionPlayer => ACCENT_AMBER,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::SoftwareInstrument => phosphor_egui::regular::MUSIC_NOTES,
            Self::Audio => phosphor_egui::regular::WAVEFORM,
            Self::Pattern => phosphor_egui::regular::SQUARES_FOUR,
            Self::SessionPlayer => phosphor_egui::regular::PIANO_KEYS,
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
    KeyboardPlayer,
    Drummer,
    BassPlayer,
}

impl SessionPlayerSub {
    pub fn label(self) -> &'static str {
        match self {
            Self::KeyboardPlayer => "Keyboard Player",
            Self::Drummer => "Drummer",
            Self::BassPlayer => "Bass Player",
        }
    }
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
            session_sub: SessionPlayerSub::KeyboardPlayer,
            audio_sub: AudioSub::MicLine,
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
    let mut open_sound_library_directly = false;

    let win_w = 440.0;
    egui::Window::new("create_new_track_dialog")
        .id(egui::Id::unique("create_new_track_modal"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -10.0))
        .fixed_size(vec2(
            win_w,
            if s.new_track.details_open {
                450.0
            } else {
                330.0
            },
        ))
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x1C, 0x1D, 0x21))
                .corner_radius(CornerRadius::same(2))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3c)))
                .shadow(egui::Shadow::NONE)
                .inner_margin(16.0),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 10.0);

            // Header row: Cancel (left) | Title (center)
            ui.horizontal(|ui| {
                let cancel_resp = ui.add(
                    egui::Button::new(
                        egui::RichText::new("Cancel")
                            .color(Color32::from_rgb(0x3B, 0x82, 0xF6))
                            .size(13.5),
                    )
                    .frame(false),
                );
                if cancel_resp.clicked() {
                    cancel = true;
                }

                let cancel_w = cancel_resp.rect.width();
                let avail_w = ui.available_width();
                // Offset title so it stays centered relative to modal width
                ui.add_space((avail_w - cancel_w) * 0.5 - 60.0);
                ui.label(
                    egui::RichText::new("Create New Track")
                        .size(15.0)
                        .strong()
                        .color(Color32::WHITE),
                );
            });

            ui.add_space(4.0);

            // 2×2 Card Grid:
            // Row 1: MIDI (Software Instrument) | Audio
            // Row 2: Pattern (Software Instrument) | Session Player
            const GRID_CARDS: [[CardChoice; 2]; 2] = [
                [CardChoice::SoftwareInstrument, CardChoice::Audio],
                [CardChoice::Pattern, CardChoice::SessionPlayer],
            ];

            let card_w = (ui.available_width() - 10.0) / 2.0;
            let card_h = 108.0;

            for row in GRID_CARDS {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 0.0);

                    for c in row {
                        let (rect, card_resp) =
                            ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                        let painter = ui.painter();

                        // Background and hover stroke
                        let is_hovered = card_resp.hovered();
                        let bg_color = if is_hovered {
                            Color32::from_rgb(0x2F, 0x31, 0x37)
                        } else {
                            Color32::from_rgb(0x26, 0x27, 0x2B)
                        };
                        let border_color = if is_hovered {
                            Color32::from_rgb(0x3E, 0x42, 0x4C)
                        } else {
                            Color32::from_rgb(0x2E, 0x31, 0x38)
                        };
                        painter.rect(
                            rect,
                            10.0,
                            bg_color,
                            Stroke::new(1.0, border_color),
                            StrokeKind::Inside,
                        );

                        // Top-left circular colored badge (36×36px)
                        let badge_center = pos2(rect.left() + 26.0, rect.top() + 26.0);
                        painter.circle_filled(badge_center, 16.0, c.accent());
                        painter.text(
                            badge_center,
                            Align2::CENTER_CENTER,
                            c.icon(),
                            FontId::proportional(18.0),
                            Color32::WHITE,
                        );

                        // Title
                        let title_pos = pos2(rect.left() + 50.0, rect.top() + 18.0);
                        painter.text(
                            title_pos,
                            Align2::LEFT_TOP,
                            c.title(),
                            FontId::proportional(14.0),
                            Color32::WHITE,
                        );

                        // Subtitle or interactive selector
                        let sub_pos = pos2(rect.left() + 50.0, rect.top() + 38.0);
                        if c == CardChoice::SessionPlayer {
                            // Inline selector pill for Session Player sub-type
                            let pill_rect = Rect::from_min_size(
                                pos2(rect.left() + 50.0, rect.top() + 38.0),
                                vec2(112.0, 22.0),
                            );
                            let pill_resp = ui.interact(
                                pill_rect,
                                egui::Id::unique(("new_track", "session_pill")),
                                Sense::click(),
                            );
                            let pill_bg = if pill_resp.hovered() {
                                Color32::from_rgb(0x3B, 0x3E, 0x46)
                            } else {
                                Color32::from_rgb(0x32, 0x35, 0x3C)
                            };
                            painter.rect(
                                pill_rect,
                                5.0,
                                pill_bg,
                                Stroke::new(1.0, Color32::from_rgb(0x42, 0x46, 0x50)),
                                StrokeKind::Inside,
                            );
                            painter.text(
                                pos2(pill_rect.left() + 6.0, pill_rect.center().y),
                                Align2::LEFT_CENTER,
                                s.new_track.session_sub.label(),
                                FontId::proportional(10.5),
                                Color32::from_rgb(0xDC, 0xDC, 0xE0),
                            );
                            painter.text(
                                pos2(pill_rect.right() - 8.0, pill_rect.center().y),
                                Align2::RIGHT_CENTER,
                                phosphor_egui::regular::CARET_DOWN,
                                FontId::proportional(10.0),
                                Color32::from_rgb(0x9C, 0xA3, 0xAF),
                            );

                            egui::Popup::menu(&pill_resp).show(|ui| {
                                for sub in [
                                    SessionPlayerSub::KeyboardPlayer,
                                    SessionPlayerSub::Drummer,
                                    SessionPlayerSub::BassPlayer,
                                ] {
                                    let active = s.new_track.session_sub == sub;
                                    if ui.selectable_label(active, sub.label()).clicked() {
                                        s.new_track.session_sub = sub;
                                        ui.close();
                                    }
                                }
                            });
                        } else {
                            let sub_text = match c {
                                CardChoice::SoftwareInstrument => {
                                    if s.new_track.midi_sub == MidiSub::ExternalMidi {
                                        "External MIDI"
                                    } else {
                                        "Software Instrument"
                                    }
                                }
                                CardChoice::Audio => "Default Input",
                                CardChoice::Pattern => "Software Instrument",
                                CardChoice::SessionPlayer => "Keyboard Player",
                            };
                            painter.text(
                                sub_pos,
                                Align2::LEFT_TOP,
                                sub_text,
                                FontId::proportional(11.0),
                                Color32::from_rgb(0x9C, 0xA3, 0xAF),
                            );
                        }

                        // Bottom-right circular "..." details button
                        let dots_r = Rect::from_center_size(
                            pos2(rect.right() - 20.0, rect.bottom() - 20.0),
                            vec2(24.0, 24.0),
                        );
                        let dots_resp = ui.interact(
                            dots_r,
                            egui::Id::unique(("new_track_dots", c as u8)),
                            Sense::click(),
                        );
                        let dots_bg = if dots_resp.hovered() {
                            Color32::from_rgb(0x3B, 0x3E, 0x48)
                        } else {
                            Color32::from_rgb(0x30, 0x33, 0x3A)
                        };
                        painter.circle_filled(dots_r.center(), 11.0, dots_bg);
                        painter.circle_stroke(
                            dots_r.center(),
                            11.0,
                            Stroke::new(1.0, Color32::from_rgb(0x42, 0x46, 0x50)),
                        );
                        painter.text(
                            dots_r.center(),
                            Align2::CENTER_CENTER,
                            phosphor_egui::regular::DOTS_THREE,
                            FontId::proportional(14.0),
                            Color32::from_rgb(0x3B, 0x82, 0xF6),
                        );

                        if dots_resp.clicked() {
                            s.new_track.choice = c;
                            s.new_track.details_open = !s.new_track.details_open;
                        } else if card_resp.clicked() && !dots_resp.hovered() {
                            s.new_track.choice = c;
                            create = true;
                        }
                    }
                });
            }

            // Collapsible Details Drawer
            if s.new_track.details_open {
                ui.add_space(2.0);
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x18, 0x19, 0x1C))
                    .corner_radius(8.0)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x2E, 0x31, 0x38)))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 6.0);

                        if s.new_track.choice == CardChoice::Audio {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Audio Input:")
                                        .size(11.5)
                                        .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                                );
                                let inputs =
                                    ["Input 1 (Mono)", "Input 2 (Mono)", "Input 1 + 2 (Stereo)"];
                                egui::ComboBox::from_id_salt("track_audio_input_2x2")
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
                            });
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Default Instrument:")
                                        .size(11.5)
                                        .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                                );
                                const INSTRUMENTS: [&str; 4] = [
                                    "Vintage Electric Piano",
                                    "Sampler (Grand Piano)",
                                    "808 Bass",
                                    "Drum Machine",
                                ];
                                egui::ComboBox::from_id_salt("track_default_inst_2x2")
                                    .selected_text(
                                        INSTRUMENTS
                                            [s.new_track.instrument_choice % INSTRUMENTS.len()],
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

                            if s.new_track.choice == CardChoice::SoftwareInstrument {
                                ui.horizontal(|ui| {
                                    let mut ext_midi =
                                        s.new_track.midi_sub == MidiSub::ExternalMidi;
                                    if ui.checkbox(&mut ext_midi, "External MIDI Device").changed()
                                    {
                                        s.new_track.midi_sub = if ext_midi {
                                            MidiSub::ExternalMidi
                                        } else {
                                            MidiSub::SoftwareInstrument
                                        };
                                    }
                                });
                            }
                        }

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Audio Output:")
                                    .size(11.5)
                                    .color(Color32::from_rgb(0x9C, 0xA3, 0xAF)),
                            );
                            let outputs =
                                ["Output 1 + 2 (Stereo Out)", "Output 3 + 4", "Output 5 + 6"];
                            egui::ComboBox::from_id_salt("track_audio_output_2x2")
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
                        });

                        ui.horizontal(|ui| {
                            ui.checkbox(&mut s.new_track.open_library, "Open Sound Library");
                            ui.add_space(8.0);
                            ui.checkbox(&mut s.new_track.load_default_patch, "Load Default Patch");
                        });
                    });
            }

            ui.add_space(4.0);

            // Number of Tracks Row with Stepper
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Number of Tracks")
                        .size(12.5)
                        .color(Color32::from_rgb(0xD1, 0xD5, 0xDB)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);

                    // Plus button
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(phosphor_egui::regular::PLUS).size(12.0),
                            )
                            .min_size(vec2(24.0, 24.0)),
                        )
                        .clicked()
                    {
                        s.new_track.count = s.new_track.count.saturating_add(1).min(64);
                    }

                    // Stepper count box
                    let box_rect =
                        Rect::from_min_size(ui.cursor().min - vec2(36.0, 0.0), vec2(36.0, 24.0));
                    let painter = ui.painter();
                    painter.rect(
                        box_rect,
                        4.0,
                        Color32::from_rgb(0x26, 0x27, 0x2B),
                        Stroke::new(1.0, Color32::from_rgb(0x3E, 0x42, 0x4C)),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        box_rect.center(),
                        Align2::CENTER_CENTER,
                        format!("{}", s.new_track.count),
                        FontId::monospace(13.0),
                        Color32::WHITE,
                    );
                    ui.allocate_exact_size(vec2(36.0, 24.0), Sense::hover());

                    // Minus button
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(phosphor_egui::regular::MINUS).size(12.0),
                            )
                            .min_size(vec2(24.0, 24.0)),
                        )
                        .clicked()
                    {
                        s.new_track.count = s.new_track.count.saturating_sub(1).max(1);
                    }
                });
            });

            ui.add_space(4.0);

            // Bottom Full-Width Browser Card: Patches, Loops, Samples...
            let bottom_card_w = ui.available_width();
            let (b_rect, b_resp) =
                ui.allocate_exact_size(vec2(bottom_card_w, 42.0), Sense::click());
            let b_painter = ui.painter();

            let b_bg = if b_resp.hovered() {
                Color32::from_rgb(0x2F, 0x31, 0x37)
            } else {
                Color32::from_rgb(0x26, 0x27, 0x2B)
            };
            let b_border = if b_resp.hovered() {
                Color32::from_rgb(0x3E, 0x42, 0x4C)
            } else {
                Color32::from_rgb(0x2E, 0x31, 0x38)
            };
            b_painter.rect(
                b_rect,
                10.0,
                b_bg,
                Stroke::new(1.0, b_border),
                StrokeKind::Inside,
            );

            // Folder badge
            let folder_center = pos2(b_rect.left() + 24.0, b_rect.center().y);
            b_painter.circle_filled(folder_center, 13.0, Color32::from_rgb(0x37, 0x41, 0x51));
            b_painter.text(
                folder_center,
                Align2::CENTER_CENTER,
                phosphor_egui::regular::FOLDER_OPEN,
                FontId::proportional(14.0),
                Color32::WHITE,
            );

            // Text
            b_painter.text(
                pos2(b_rect.left() + 46.0, b_rect.center().y),
                Align2::LEFT_CENTER,
                "Patches, Loops, Samples...",
                FontId::proportional(12.5),
                Color32::WHITE,
            );

            // Chevron right
            b_painter.text(
                pos2(b_rect.right() - 16.0, b_rect.center().y),
                Align2::RIGHT_CENTER,
                phosphor_egui::regular::CARET_RIGHT,
                FontId::proportional(13.0),
                Color32::from_rgb(0x9C, 0xA3, 0xAF),
            );

            if b_resp.clicked() {
                open_sound_library_directly = true;
            }
        });

    if cancel {
        s.new_track.open = false;
        return;
    }

    if open_sound_library_directly {
        s.show_library = true;
        s.new_track.open = false;
        return;
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
                        p.tracks[at].name = "Inst 1".to_string();
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
                        p.tracks[at].name = "Drummer".to_string();
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
                        p.tracks[at].name = "Audio 1".to_string();
                        p.tracks[at].color = ACCENT_BLUE;
                    }
                    AudioSub::GuitarBass => {
                        let at = p.add_track(TrackKind::Audio);
                        p.tracks[at].name = "Guitar".to_string();
                        p.tracks[at].color = ACCENT_CYAN;
                        p.tracks[at].icon = Some(crate::track_icons::IconKind::Guitar);
                    }
                },
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
    }

    #[test]
    fn each_choice_has_distinct_accent_and_icon() {
        const CARDS: [CardChoice; 4] = [
            CardChoice::SoftwareInstrument,
            CardChoice::Audio,
            CardChoice::Pattern,
            CardChoice::SessionPlayer,
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
