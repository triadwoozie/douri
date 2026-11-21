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
// Top chrome surfaces & dividers
pub const MENU_BAR_BG: Color32 = Color32::from_rgb(0x22, 0x22, 0x25); // #222225 menu bar
pub const CONTROL_BAR_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a); // #18181A control bar well
pub const SUB_TOOLBAR_BG: Color32 = Color32::from_rgb(0x22, 0x22, 0x25); // #222225 sub-toolbar
pub const TOP_DIVIDER: Color32 = Color32::from_rgb(0x11, 0x11, 0x13); // 1px #111113 dividers

pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xeb);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);

// Functional studio accents (Logic Pro 11+ desaturated pro-audio palette)
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x3a, 0x7b, 0xd5); // #3A7BD5 selection
pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(0x30, 0xd1, 0x58); // #30D158 play & audio level meters
pub const ACCENT_CRIMSON: Color32 = Color32::from_rgb(0xff, 0x45, 0x3a); // #FF453A record active
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xff, 0xd6, 0x0a); // #FFD60A cycle loop
pub const ACCENT_VIOLET: Color32 = Color32::from_rgb(0x8b, 0x5c, 0xf6); // #8B5CF6 synth tags / signature
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x06, 0xb6, 0xd4); // #06B6D4 bass tags

// Muted studio track colors
pub const TRACK_TEAL: Color32 = Color32::from_rgb(0x2a, 0x9d, 0x8f);
pub const TRACK_COBALT: Color32 = Color32::from_rgb(0x26, 0x46, 0x53);
pub const TRACK_INDIGO: Color32 = Color32::from_rgb(0x43, 0x61, 0xee);
pub const TRACK_VIOLET: Color32 = Color32::from_rgb(0x72, 0x09, 0xb7);
pub const TRACK_ROSE: Color32 = Color32::from_rgb(0xe6, 0x39, 0x46);
pub const TRACK_SAGE: Color32 = Color32::from_rgb(0x58, 0x81, 0x57);
pub const REGION: Color32 = Color32::from_rgb(0x1f, 0x1e, 0x1c);
pub const REGION_AUDIO: Color32 = Color32::from_rgb(0xa8, 0xa2, 0x95);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = CHROME_BG;
    v.window_fill = Color32::from_rgb(0x1a, 0x1b, 0x1e);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38));
    v.popup_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 2,
        color: Color32::from_black_alpha(160),
    };
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

    // Active states: soft translucent neutral highlight (rgba(255, 255, 255, 0.14))
    v.widgets.active.bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 36);
    v.widgets.active.weak_bg_fill = Color32::from_rgba_unmultiplied(255, 255, 255, 36);
    v.widgets.active.bg_stroke =
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20));
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(0xf5, 0xf5, 0xf7));

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

/// Font face helper for UI widgets and labels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Silk,
    SilkX,
    Body,
    Bold,
    Lcd,
}

impl Face {
    pub fn font(self, size: f32) -> egui::FontId {
        egui::FontId::proportional(size)
    }
}

pub fn font(face: Face, size: f32) -> egui::FontId {
    face.font(size)
}

pub fn tracking(_face: Face) -> f32 {
    0.0
}

pub const BONE: Color32 = Color32::from_rgb(0xe9, 0xe5, 0xdb);
pub const BONE_HI: Color32 = Color32::from_rgb(0xf4, 0xf1, 0xea);
pub const BONE_TOP: Color32 = Color32::from_rgb(0xfb, 0xf9, 0xf4);
pub const BONE2: Color32 = Color32::from_rgb(0xdc, 0xd7, 0xca);
pub const BONE3: Color32 = Color32::from_rgb(0xcf, 0xc8, 0xb8);
pub const SEAM: Color32 = Color32::from_rgb(0xaf, 0xa7, 0x96);
pub const SEAM_DK: Color32 = Color32::from_rgb(0x8f, 0x88, 0x7a);
pub const KEY_EDGE: Color32 = Color32::from_rgb(0xa4, 0x9d, 0x8e);
pub const KEY_LIP: Color32 = Color32::from_rgb(0x9f, 0x98, 0x89);
pub const INK: Color32 = Color32::from_rgb(0x14, 0x14, 0x13);
pub const INK2: Color32 = Color32::from_rgb(0x2e, 0x2c, 0x29);
pub const LCD: Color32 = Color32::from_rgb(0xb2, 0xbc, 0x9f);
pub const LCD_INK: Color32 = Color32::from_rgb(0x1a, 0x20, 0x13);
pub const LCD_GHOST: Color32 = Color32::from_rgba_premultiplied(1, 2, 1, 13);
pub const CAP_TOP: Color32 = Color32::from_rgb(0x34, 0x32, 0x2f);
pub const CAP: Color32 = Color32::from_rgb(0x1d, 0x1c, 0x1a);
pub const CAP_EDGE: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0a);
pub const ON_DARK: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf7);
pub const PRINT: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x9f, 0x0a);
pub const ORANGE_HI: Color32 = Color32::from_rgb(0xff, 0xb3, 0x40);
pub const ORANGE_DK: Color32 = Color32::from_rgb(0xd9, 0x77, 0x06);
pub const LED_OFF: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3c);
pub const METER_WELL: Color32 = Color32::from_rgb(0x18, 0x18, 0x1a);

pub const TRACK_PALETTE: [Color32; 16] = [
    TRACK_TEAL,
    TRACK_COBALT,
    TRACK_INDIGO,
    TRACK_VIOLET,
    TRACK_ROSE,
    TRACK_SAGE,
    ACCENT_BLUE,
    ACCENT_EMERALD,
    ACCENT_CRIMSON,
    ACCENT_AMBER,
    ACCENT_VIOLET,
    ACCENT_CYAN,
    Color32::from_rgb(0xd8, 0x5a, 0x9a),
    Color32::from_rgb(0xe6, 0x7e, 0x22),
    Color32::from_rgb(0x1a, 0xbc, 0x9c),
    Color32::from_rgb(0x9b, 0x59, 0xb6),
];

#[derive(Clone, Copy, Debug)]
pub struct RegionLook {
    pub header: Color32,
    pub header_ink: Color32,
    pub body: Color32,
    pub print: Color32,
    pub outline: Color32,
}

pub fn region_look(track: Color32, _audio: bool, selected: bool, muted: bool) -> RegionLook {
    if muted {
        return RegionLook {
            header: Color32::from_rgb(0x2a, 0x2a, 0x2e),
            header_ink: TEXT_MUTED,
            body: Color32::from_rgb(0x1e, 0x1e, 0x21),
            print: TEXT_MUTED,
            outline: if selected { ACCENT_BLUE } else { BORDER },
        };
    }
    let header = if selected { ACCENT_BLUE } else { track };
    RegionLook {
        header,
        header_ink: Color32::WHITE,
        body: CARD_BG,
        print: TEXT,
        outline: if selected { ACCENT_BLUE } else { BORDER },
    }
}
