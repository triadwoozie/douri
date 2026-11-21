//! Phosphor icon support for Dōri, providing crisp vector icons with optical centering
//! and Regular / Fill / Bold font variant switching.

use egui::{pos2, Color32, FontFamily, FontId, Painter, Rect};

pub use phosphor_egui::fill;
pub use phosphor_egui::regular;

// Icon glyph constants
pub const ARCHIVE_BOX: &str = regular::ARCHIVE_BOX;
pub const INFO: &str = regular::INFO;
pub const QUESTION: &str = regular::QUESTION;
pub const APP_WINDOW: &str = regular::APP_WINDOW;
pub const SLIDERS: &str = regular::SLIDERS;
pub const FADERS: &str = regular::FADERS;
pub const SCISSORS: &str = regular::SCISSORS;
pub const LIST_BULLETS: &str = regular::LIST_BULLETS;
pub const NOTE_BLANK: &str = regular::NOTE_BLANK;
pub const REPEAT: &str = regular::REPEAT;
pub const FOLDER_OPEN: &str = regular::FOLDER_OPEN;

pub const SKIP_BACK: &str = regular::SKIP_BACK;
pub const REWIND: &str = regular::REWIND;
pub const FAST_FORWARD: &str = regular::FAST_FORWARD;
pub const STOP: &str = regular::STOP;
pub const PLAY: &str = regular::PLAY;
pub const RECORD: &str = regular::RECORD;
pub const ARROWS_CLOCKWISE: &str = regular::ARROWS_CLOCKWISE;
pub const SLIDERS_HORIZONTAL: &str = regular::SLIDERS_HORIZONTAL;
pub const METRONOME: &str = regular::METRONOME;
pub const SPEAKER_HIGH: &str = regular::SPEAKER_HIGH;

pub const MAGNIFYING_GLASS: &str = regular::MAGNIFYING_GLASS;
pub const PIANO_KEYS: &str = regular::PIANO_KEYS;
pub const MICROPHONE_STAGE: &str = regular::MICROPHONE_STAGE;
pub const GUITAR: &str = regular::GUITAR;
pub const MUSIC_NOTES: &str = regular::MUSIC_NOTES;
pub const WAVEFORM: &str = regular::WAVEFORM;

pub const CURSOR: &str = regular::CURSOR;
pub const PENCIL_SIMPLE: &str = regular::PENCIL_SIMPLE;
pub const MAGNET: &str = regular::MAGNET;
pub const PLUS: &str = regular::PLUS;
pub const COPY: &str = regular::COPY;
pub const POWER: &str = regular::POWER;
pub const LINK: &str = regular::LINK;

fn add_font_bytes_as_family(fonts: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]) {
    fonts
        .font_data
        .insert(name.into(), egui::FontData::from_static(bytes).into());

    let mut keys = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    keys.insert(keys.len().min(1), name.into());
    fonts
        .families
        .insert(egui::FontFamily::Name(name.into()), keys);
}

fn add_font_bytes_to_fonts(fonts: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]) {
    add_font_bytes_as_family(fonts, name, bytes);

    if let Some(font_keys) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        let at = font_keys.len().min(1);
        font_keys.insert(at, name.into());
    }
}

/// Registers Phosphor font variants into `FontDefinitions`.
pub fn install_fonts(fonts: &mut egui::FontDefinitions) {
    add_font_bytes_to_fonts(fonts, "phosphor", phosphor_egui::bytes::regular::FONT);
    add_font_bytes_as_family(fonts, "phosphor-fill", phosphor_egui::bytes::fill::FONT);
    add_font_bytes_as_family(fonts, "phosphor-bold", phosphor_egui::bytes::bold::FONT);
}

/// Renders a Phosphor glyph optically centered inside `rect`.
/// When `fill` is true, the Fill font variant is used; otherwise Regular is used.
pub fn paint(painter: &Painter, rect: Rect, glyph: &str, size: f32, color: Color32, fill: bool) {
    let family = if fill {
        FontFamily::Name("phosphor-fill".into())
    } else {
        FontFamily::Name("phosphor".into())
    };
    let font_id = FontId::new(size, family);
    let galley = painter.layout_no_wrap(glyph.to_string(), font_id, color);
    let center = rect.center();
    let pos = pos2(
        (center.x - galley.size().x * 0.5).round(),
        (center.y - galley.size().y * 0.5).round(),
    );
    painter.galley(pos, galley, color);
}

/// Renders a Phosphor glyph optically centered with the Bold font variant.
pub fn paint_bold(painter: &Painter, rect: Rect, glyph: &str, size: f32, color: Color32) {
    let font_id = FontId::new(size, FontFamily::Name("phosphor-bold".into()));
    let galley = painter.layout_no_wrap(glyph.to_string(), font_id, color);
    let center = rect.center();
    let pos = pos2(
        (center.x - galley.size().x * 0.5).round(),
        (center.y - galley.size().y * 0.5).round(),
    );
    painter.galley(pos, galley, color);
}

/// Convenience helper for Regular style.
pub fn paint_regular(painter: &Painter, rect: Rect, glyph: &str, size: f32, color: Color32) {
    paint(painter, rect, glyph, size, color, false);
}

/// Convenience helper for Fill style.
pub fn paint_fill(painter: &Painter, rect: Rect, glyph: &str, size: f32, color: Color32) {
    paint(painter, rect, glyph, size, color, true);
}
