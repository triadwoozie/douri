//! Authentic Neutral Charcoal Pro-Audio Palette (Logic Pro 11+ aesthetic) for Dōri.
use egui::{Color32, CornerRadius, Stroke};

// Primary neutral graphite/charcoal surfaces
pub const CHROME_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a); // #18181A window chrome & top bar
pub const CANVAS_BG: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x21); // #1E1E21 arrangement & mixer canvas
pub const CARD_BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x2a); // #26262A cards, track headers, browser rows
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x30, 0x30, 0x35); // #303035 hover state
pub const BORDER: Color32 = Color32::from_rgb(0x34, 0x34, 0x39); // #343439 subtle 1px dividers
pub const GRID_LINE: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x2e); // #2A2A2E beat/bar grid lines
pub const BORDER_STROKE: Stroke = Stroke {
    width: 1.0,
    color: BORDER,
};

// Aliases for DAW panels
pub const BG: Color32 = CHROME_BG;
pub const PANEL: Color32 = CARD_BG;
pub const GRID_BG: Color32 = CANVAS_BG;
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xeb);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);

// Functional studio accents (Logic Pro 11+ desaturated pro-audio palette)
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x3a, 0x7b, 0xd5); // #3A7BD5 active view toggle pills
pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(0x2e, 0xbd, 0x6b); // #2EBD6B play & audio level meters
pub const ACCENT_CRIMSON: Color32 = Color32::from_rgb(0xe0, 0x48, 0x48); // #E04848 record active
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xe5, 0xa9, 0x3b); // #E5A93B solo & cycle loop
pub const ACCENT_VIOLET: Color32 = Color32::from_rgb(0x8b, 0x5c, 0xf6); // #8B5CF6 synth tags / signature
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x06, 0xb6, 0xd4); // #06B6D4 bass tags

// Muted studio track colors
pub const TRACK_TEAL: Color32 = Color32::from_rgb(0x2a, 0x9d, 0x8f);
pub const TRACK_COBALT: Color32 = Color32::from_rgb(0x26, 0x46, 0x53);
pub const TRACK_INDIGO: Color32 = Color32::from_rgb(0x43, 0x61, 0xee);
pub const TRACK_VIOLET: Color32 = Color32::from_rgb(0x72, 0x09, 0xb7);
pub const TRACK_ROSE: Color32 = Color32::from_rgb(0xe6, 0x39, 0x46);
pub const TRACK_SAGE: Color32 = Color32::from_rgb(0x58, 0x81, 0x57);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = CHROME_BG;
    v.window_fill = CARD_BG;
    v.extreme_bg_color = CANVAS_BG;
    v.faint_bg_color = Color32::from_rgba_premultiplied(0x26, 0x26, 0x2a, 160);
    v.override_text_color = Some(TEXT);

    // Non-interactive widgets
    v.widgets.noninteractive.bg_fill = CARD_BG;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);

    // Inactive buttons / pills
    v.widgets.inactive.bg_fill = CARD_BG;
    v.widgets.inactive.weak_bg_fill = CARD_BG;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);

    // Hovered states with clean neutral brightening
    v.widgets.hovered.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = CARD_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x42, 0x42, 0x48));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // Active states: studio blue fill
    v.widgets.active.bg_fill = ACCENT_BLUE;
    v.widgets.active.weak_bg_fill = ACCENT_BLUE;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_BLUE);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // Selection
    v.selection.bg_fill = Color32::from_rgba_premultiplied(0x3a, 0x7b, 0xd5, 130);
    v.selection.stroke = Stroke::new(1.0, ACCENT_BLUE);

    // Corner radius
    let r = CornerRadius::same(5);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;

    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.window_margin = egui::Margin::same(10);
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(12.5));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.5));
    });
}
