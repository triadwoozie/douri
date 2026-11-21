//! Shared Logic Pro Dark Mode Chrome Kit (`ui_chrome.rs`).
//!
//! Provides unified styling for dialog sheets, card selectors, menus, popovers,
//! and standard controls matching Apple Logic Pro's macOS dark interface.

use egui::{
    epaint::Shadow, pos2, vec2, Align, Align2, Color32, FontId, Id, Layout, Rect, Response, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};

// --- Neutral Charcoal & Accent Palette
pub const SURFACE_BG: Color32 = Color32::from_rgb(0x24, 0x25, 0x28);
pub const CARD_BG: Color32 = Color32::from_rgb(0x1B, 0x1C, 0x1E);
pub const CARD_BG_HOVER: Color32 = Color32::from_rgb(0x28, 0x29, 0x2D);
pub const CARD_BG_SELECTED: Color32 = Color32::from_rgb(0x2A, 0x2C, 0x32);
pub const RIM_COLOR: Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 28);
pub const RIM_STROKE: Stroke = Stroke {
    width: 1.0,
    color: RIM_COLOR,
};

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x93);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x63, 0x63, 0x66);

pub const PRIMARY_BTN_BG: Color32 = Color32::from_rgb(0x2E, 0x6B, 0xB8);
pub const PRIMARY_BTN_HOVER: Color32 = Color32::from_rgb(0x3B, 0x7A, 0xCC);
pub const CANCEL_BTN_BG: Color32 = Color32::from_rgb(0x3A, 0x3B, 0x3F);
pub const CANCEL_BTN_HOVER: Color32 = Color32::from_rgb(0x48, 0x49, 0x4E);

pub const ACCENT_GREEN: Color32 = Color32::from_rgb(0x30, 0xD1, 0x58);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0x0A, 0x84, 0xFF);
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x64, 0xD2, 0xFF);
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(0xFF, 0x9F, 0x0A);
pub const ACCENT_PURPLE: Color32 = Color32::from_rgb(0xBF, 0x5A, 0xF2);

/// Draws a centered modal sheet window with Logic Pro dark-mode chrome.
pub fn modal_sheet<R>(
    ctx: &egui::Context,
    id: Id,
    title: &str,
    size: Vec2,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let mut result = None;

    egui::Window::new(title)
        .id(id)
        .fixed_size(size)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, -10.0))
        .frame(
            egui::Frame::new()
                .fill(SURFACE_BG)
                .corner_radius(12.0)
                .stroke(RIM_STROKE)
                .shadow(Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 2,
                    color: Color32::from_black_alpha(160),
                })
                .inner_margin(16.0),
        )
        .show(ctx, |ui| {
            result = Some(add_contents(ui));
        });

    result
}

/// Visual card button for `Choose a track type` sheet.
pub fn track_type_card(
    ui: &mut Ui,
    selected: bool,
    accent: Color32,
    icon_glyph: &str,
    title: &str,
    subtitle: &str,
) -> Response {
    let card_w = 118.0;
    let card_h = 136.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
    let painter = ui.painter();

    // Background fill & border
    let bg_color = if selected {
        CARD_BG_SELECTED
    } else if resp.hovered() {
        CARD_BG_HOVER
    } else {
        CARD_BG
    };

    let border_stroke = if selected {
        Stroke::new(1.5, accent)
    } else if resp.hovered() {
        Stroke::new(1.0, Color32::from_white_alpha(50))
    } else {
        Stroke::new(1.0, RIM_COLOR)
    };

    painter.rect(
        rect,
        10.0,
        bg_color,
        border_stroke,
        StrokeKind::Inside,
    );

    // Top accent indicator when selected
    if selected {
        let top_strip = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top()),
            pos2(rect.right() - 8.0, rect.top() + 3.0),
        );
        painter.rect_filled(top_strip, 1.5, accent);
    }

    // Centered 34px icon badge
    let icon_center = pos2(rect.center().x, rect.top() + 42.0);
    let icon_color = if selected {
        accent
    } else if resp.hovered() {
        TEXT_PRIMARY
    } else {
        Color32::from_rgb(0xC7, 0xC7, 0xCC)
    };

    painter.text(
        icon_center,
        Align2::CENTER_CENTER,
        icon_glyph,
        FontId::proportional(34.0),
        icon_color,
    );

    // Title
    let title_pos = pos2(rect.center().x, rect.top() + 78.0);
    painter.text(
        title_pos,
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(12.5),
        if selected { TEXT_PRIMARY } else { Color32::from_rgb(0xDC, 0xDC, 0xE0) },
    );

    // Subtitle / description (word-wrapped across ~104px)
    let sub_rect = Rect::from_min_max(
        pos2(rect.left() + 6.0, rect.top() + 94.0),
        pos2(rect.right() - 6.0, rect.bottom() - 6.0),
    );
    let mut sub_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(sub_rect)
            .layout(Layout::top_down(Align::Center)),
    );
    sub_ui.label(
        egui::RichText::new(subtitle)
            .size(10.0)
            .color(if selected { Color32::from_rgb(0xA0, 0xA0, 0xA8) } else { TEXT_MUTED }),
    );

    resp
}

/// Primary confirmation button (Logic Pro deep blue, soft rounded capsule).
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(86.0, 26.0), Sense::click());
    let bg = if resp.is_pointer_button_down_on() {
        PRIMARY_BTN_BG.gamma_multiply(0.8)
    } else if resp.hovered() {
        PRIMARY_BTN_HOVER
    } else {
        PRIMARY_BTN_BG
    };

    ui.painter().rect(
        rect,
        6.0,
        bg,
        Stroke::new(1.0, Color32::from_white_alpha(35)),
        StrokeKind::Inside,
    );

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        Color32::WHITE,
    );

    resp
}

/// Secondary / Cancel button (Neutral charcoal pill).
pub fn cancel_button(ui: &mut Ui, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(76.0, 26.0), Sense::click());
    let bg = if resp.is_pointer_button_down_on() {
        CANCEL_BTN_BG.gamma_multiply(0.8)
    } else if resp.hovered() {
        CANCEL_BTN_HOVER
    } else {
        CANCEL_BTN_BG
    };

    ui.painter().rect(
        rect,
        6.0,
        bg,
        Stroke::new(1.0, Color32::from_white_alpha(20)),
        StrokeKind::Inside,
    );

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        TEXT_PRIMARY,
    );

    resp
}

/// Numeric stepper (+ / - buttons and value box).
pub fn number_stepper(ui: &mut Ui, val: &mut u32, min: u32, max: u32) -> Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);

        // Minus button
        let (minus_r, minus_resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
        let minus_bg = if minus_resp.hovered() {
            Color32::from_rgb(0x38, 0x39, 0x3E)
        } else {
            Color32::from_rgb(0x28, 0x29, 0x2D)
        };
        ui.painter().rect(
            minus_r,
            4.0,
            minus_bg,
            Stroke::new(1.0, RIM_COLOR),
            StrokeKind::Inside,
        );
        ui.painter().text(
            minus_r.center() + vec2(0.0, -1.0),
            Align2::CENTER_CENTER,
            "−",
            FontId::proportional(14.0),
            TEXT_PRIMARY,
        );
        if minus_resp.clicked() && *val > min {
            *val -= 1;
        }

        // Value box
        let (val_r, val_resp) = ui.allocate_exact_size(vec2(38.0, 22.0), Sense::click());
        ui.painter().rect(
            val_r,
            4.0,
            Color32::from_rgb(0x18, 0x19, 0x1B),
            Stroke::new(1.0, RIM_COLOR),
            StrokeKind::Inside,
        );
        ui.painter().text(
            val_r.center(),
            Align2::CENTER_CENTER,
            val.to_string(),
            FontId::proportional(12.0),
            TEXT_PRIMARY,
        );

        // Plus button
        let (plus_r, plus_resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
        let plus_bg = if plus_resp.hovered() {
            Color32::from_rgb(0x38, 0x39, 0x3E)
        } else {
            Color32::from_rgb(0x28, 0x29, 0x2D)
        };
        ui.painter().rect(
            plus_r,
            4.0,
            plus_bg,
            Stroke::new(1.0, RIM_COLOR),
            StrokeKind::Inside,
        );
        ui.painter().text(
            plus_r.center() + vec2(0.0, -0.5),
            Align2::CENTER_CENTER,
            "+",
            FontId::proportional(14.0),
            TEXT_PRIMARY,
        );
        if plus_resp.clicked() && *val < max {
            *val += 1;
        }

        val_resp
    })
    .inner
}

/// Standard Logic Pro popover / context menu frame.
pub fn popover_menu_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x22, 0x23, 0x26))
        .corner_radius(8.0)
        .stroke(Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 25)))
        .shadow(Shadow {
            offset: [0, 6],
            blur: 16,
            spread: 1,
            color: Color32::from_black_alpha(140),
        })
        .inner_margin(4.0)
}

/// Menu row with right-aligned shortcut.
pub fn menu_row(
    ui: &mut Ui,
    label: &str,
    shortcut: Option<&str>,
    selected: bool,
    enabled: bool,
) -> Response {
    let row_h = 24.0;
    let avail_w = ui.available_width().max(180.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(avail_w, row_h), Sense::click());

    if !enabled {
        ui.painter().text(
            pos2(rect.left() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.5),
            TEXT_MUTED,
        );
        return resp;
    }

    let bg = if selected {
        Color32::from_rgb(0x2E, 0x6B, 0xB8)
    } else if resp.hovered() {
        Color32::from_rgba_premultiplied(255, 255, 255, 24)
    } else {
        Color32::TRANSPARENT
    };

    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 5.0, bg);
    }

    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        if selected { Color32::WHITE } else { TEXT_PRIMARY },
    );

    if let Some(sc) = shortcut {
        ui.painter().text(
            pos2(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            sc,
            FontId::proportional(11.5),
            if selected { Color32::from_white_alpha(200) } else { TEXT_SECONDARY },
        );
    }

    resp
}
