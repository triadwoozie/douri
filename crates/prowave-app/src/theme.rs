//! Deep matte Obsidian palette & Liquid Glass theme for Dōri.
use egui::{Color32, CornerRadius, Stroke};

// Primary obsidian surfaces
pub const CHROME_BG: Color32 = Color32::from_rgb(0x0d, 0x0f, 0x14); // #0D0F14 outer chrome/header
pub const CANVAS_BG: Color32 = Color32::from_rgb(0x13, 0x16, 0x1e); // #13161E timeline & mixer canvas
pub const CARD_BG: Color32 = Color32::from_rgb(0x1a, 0x1e, 0x29);   // #1A1E29 cards & headers
pub const BORDER: Color32 = Color32::from_rgb(0x25, 0x2a, 0x37);    // #252A37 subtle 1px borders
pub const BORDER_STROKE: Stroke = Stroke { width: 1.0, color: BORDER };

// Legacy aliases for existing views
pub const BG: Color32 = CHROME_BG;
pub const PANEL: Color32 = CARD_BG;
pub const GRID_BG: Color32 = CANVAS_BG;
pub const TEXT: Color32 = Color32::from_rgb(0xeb, 0xed, 0xf0);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8e, 0x95, 0xa5);

// Vibrant accents
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x2d, 0x7f, 0xf9);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x00, 0xd2, 0xff);
pub const ACCENT_VIOLET: Color32 = Color32::from_rgb(0x9d, 0x4e, 0xdd);
pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
pub const ACCENT_CRIMSON: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = CHROME_BG;
    v.window_fill = CARD_BG;
    v.extreme_bg_color = CANVAS_BG;
    v.faint_bg_color = Color32::from_rgba_premultiplied(0x1a, 0x1e, 0x29, 140);
    v.override_text_color = Some(TEXT);
    
    // Non-interactive widgets
    v.widgets.noninteractive.bg_fill = CARD_BG;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    
    // Inactive buttons / pills
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x20, 0x25, 0x33);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x1a, 0x1e, 0x29);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    
    // Hovered states with subtle cyan glow
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x28, 0x30, 0x42);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x22, 0x2a, 0x3b);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_CYAN);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    
    // Active states
    v.widgets.active.bg_fill = ACCENT_BLUE;
    v.widgets.active.weak_bg_fill = ACCENT_BLUE;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_CYAN);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    
    // Selection
    v.selection.bg_fill = Color32::from_rgba_premultiplied(0x2d, 0x7f, 0xf9, 120);
    v.selection.stroke = Stroke::new(1.0, ACCENT_CYAN);
    
    // Corner radius
    let r = CornerRadius::same(6);
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
