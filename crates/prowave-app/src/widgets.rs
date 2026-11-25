//! Small Logic-style widgets shared by the local toolbars (Tracks area, Mixer).
use crate::i18n::tr;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    Ui,
};

pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xeb);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8e, 0x8e, 0x93);
pub const BTN_BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x2a);
pub const BLUE: Color32 = Color32::from_rgb(0x3a, 0x7b, 0xd5);

pub fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.3, c)
}

/// Whether `resp` asks for its right-click menu this frame: a right-click (or long touch), and
/// on macOS also a Control-click, as with a one-button mouse or a trackpad.
pub fn context_click(resp: &Response) -> bool {
    let control_click =
        cfg!(target_os = "macos") && resp.clicked() && resp.ctx.input(|i| i.modifiers.ctrl);
    resp.secondary_clicked() || control_click
}

/// Right-click menu at the pointer for `resp` (see [`context_click`]). Closes when an entry is
/// clicked, the pointer clicks elsewhere or Escape is pressed.
pub fn context_menu(resp: &Response, add_contents: impl FnOnce(&mut Ui)) {
    context_menu_shown(resp, false, add_contents);
}

/// [`context_menu`], or with `shown` the menu open over the middle of `resp` without a click
/// (only for the layout pictures, `--shots`).
pub fn context_menu_shown(resp: &Response, shown: bool, add_contents: impl FnOnce(&mut Ui)) {
    let open = if context_click(resp) || shown {
        Some(egui::SetOpenCommand::Bool(true))
    } else if resp.clicked() {
        Some(egui::SetOpenCommand::Bool(false))
    } else {
        None
    };
    let popup = egui::Popup::menu(resp).open_memory(open);
    let popup = if shown {
        popup.at_position(resp.rect.center())
    } else {
        popup.at_pointer_fixed()
    };
    popup.show(|ui| {
        ui.set_min_width(190.0);
        add_contents(ui);
    });
}

/// Text button with a small down chevron (Logic's "Edit ⌄"). Returns the response and width.
pub fn menu_title(
    ui: &Ui,
    g: &Painter,
    left: f32,
    cy: f32,
    label: &str,
    id: Id,
) -> (Response, f32) {
    let label = tr(label);
    let font = FontId::proportional(13.0);
    let w = g.layout_no_wrap(label.into(), font.clone(), TEXT).size().x + 22.0;
    let r = Rect::from_min_size(pos2(left, cy - 10.0), vec2(w, 20.0));
    let resp = ui.interact(r, id, Sense::click());
    if resp.hovered() {
        g.rect_filled(r, 4.0, Color32::from_rgb(0x30, 0x30, 0x35));
        g.rect_stroke(
            r,
            4.0,
            Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
            egui::StrokeKind::Inside,
        );
    }
    g.text(
        pos2(r.left() + 6.0, cy),
        Align2::LEFT_CENTER,
        label,
        font,
        TEXT,
    );
    let c = pos2(r.right() - 8.0, cy + 1.0);
    g.add(Shape::line(
        vec![
            c + vec2(-3.0, -1.5),
            c + vec2(0.0, 1.5),
            c + vec2(3.0, -1.5),
        ],
        stroke(TEXT_DIM),
    ));
    (resp, w)
}

/// Popup button with a double chevron (Logic's "Smart ⇕").
pub fn popup_button(ui: &Ui, g: &Painter, rect: Rect, id: Id, text: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    g.rect_filled(
        rect,
        5.0,
        if resp.hovered() {
            Color32::from_rgb(0x30, 0x30, 0x35)
        } else {
            BTN_BG
        },
    );
    g.rect_stroke(
        rect,
        5.0,
        Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)),
        egui::StrokeKind::Inside,
    );
    // Long values (some languages) end in "…" instead of running under the arrows.
    let font = FontId::proportional(12.5);
    let shown = crate::mixer::elide(g, text, &font, rect.width() - 26.0);
    g.text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        shown,
        font,
        TEXT,
    );
    let c = pos2(rect.right() - 9.0, rect.center().y);
    g.add(Shape::line(
        vec![
            c + vec2(-2.5, -1.5),
            c + vec2(0.0, -4.0),
            c + vec2(2.5, -1.5),
        ],
        stroke(TEXT),
    ));
    g.add(Shape::line(
        vec![c + vec2(-2.5, 1.5), c + vec2(0.0, 4.0), c + vec2(2.5, 1.5)],
        stroke(TEXT),
    ));
    resp
}

/// Back chevron at the start of a local toolbar (dim: nothing to go back to yet).
pub fn back_chevron(g: &Painter, at: egui::Pos2) {
    g.add(Shape::line(
        vec![
            at + vec2(3.0, -5.0),
            at + vec2(-2.0, 0.0),
            at + vec2(3.0, 5.0),
        ],
        stroke(TEXT_DIM),
    ));
}

/// Entries that exist in Logic but are not implemented yet are drawn disabled.
/// Undo and Redo entries for the local Edit menus, greyed when there is nothing to undo or
/// redo. The one picked goes to `pick` (and from there to `UiState::undo_cmd`).
pub fn undo_items(
    ui: &mut Ui,
    can_undo: bool,
    can_redo: bool,
    pick: &mut Option<crate::undo::UndoCmd>,
) {
    use egui::{Key, KeyboardShortcut, Modifiers};
    let key = |m, k| {
        crate::key_commands::format_shortcut(
            &KeyboardShortcut::new(m, k),
            crate::key_commands::is_mac_keyboard(),
        )
    };
    let (undo_key, redo_key) = (
        key(Modifiers::COMMAND, Key::Z),
        key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
    );
    if ui
        .add_enabled(
            can_undo,
            egui::Button::new(tr("Undo")).shortcut_text(undo_key),
        )
        .clicked()
    {
        *pick = Some(crate::undo::UndoCmd::Undo);
        ui.close();
    }
    if ui
        .add_enabled(
            can_redo,
            egui::Button::new(tr("Redo")).shortcut_text(redo_key),
        )
        .clicked()
    {
        *pick = Some(crate::undo::UndoCmd::Redo);
        ui.close();
    }
}

pub fn todo_item(ui: &mut Ui, label: &str) {
    ui.add_enabled(false, egui::Button::new(tr(label)));
}

/// Horizontal slider (thin groove, round knob). Returns the new 0..1 value when dragged.
pub fn slider_h(
    ui: &Ui,
    g: &Painter,
    rect: Rect,
    id: Id,
    value: f32,
    enabled: bool,
) -> Option<f32> {
    let cy = rect.center().y;
    let v = value.clamp(0.0, 1.0);
    let groove = Rect::from_min_max(pos2(rect.left(), cy - 2.5), pos2(rect.right(), cy + 2.5));
    g.rect_filled(groove, 2.5, Color32::from_rgb(0x14, 0x15, 0x18));
    g.rect_stroke(
        groove,
        2.5,
        Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38)),
        egui::StrokeKind::Inside,
    );
    let knob = pos2(rect.left() + 6.0 + (rect.width() - 12.0) * v, cy);
    if enabled && v > 0.0 {
        let active_rail =
            Rect::from_min_max(pos2(groove.left() + 1.0, cy - 1.5), pos2(knob.x, cy + 1.5));
        g.rect_filled(active_rail, 1.5, Color32::from_rgb(0x2b, 0x6c, 0xb0));
    }
    let (fill, edge) = if enabled {
        (
            Color32::from_rgb(0xd0, 0xd4, 0xdc),
            Color32::from_rgb(0x3e, 0x42, 0x4c),
        )
    } else {
        (
            Color32::from_rgb(0x6a, 0x70, 0x7a),
            Color32::from_rgb(0x2e, 0x31, 0x38),
        )
    };
    g.circle_filled(knob, 6.0, fill);
    g.circle_stroke(knob, 6.0, Stroke::new(1.0, edge));
    if !enabled {
        return None;
    }
    let resp = ui.interact(rect.expand2(vec2(2.0, 5.0)), id, Sense::click_and_drag());
    resp.interact_pointer_pos()
        .map(|p| ((p.x - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0))
}

/// Logic's rotary knob: dark bezel, light grey domed body, a white pointer, and two small tick
/// marks flanking the bottom. `value` runs from -1 (full left) to 1 (full right), 0 = up.
pub fn draw_knob(g: &Painter, c: Pos2, r: f32, value: f32) {
    // Soft shadow and dark bezel.
    g.circle_filled(c + vec2(0.0, 1.0), r + 2.2, Color32::from_black_alpha(80));
    g.circle_filled(c, r + 1.6, Color32::from_gray(0x2a));
    // Domed body: darker edge, lighter centre and a highlight up and to the left.
    g.circle_filled(c, r, Color32::from_gray(0x7c));
    g.circle_filled(
        c + vec2(-r * 0.12, -r * 0.18),
        r * 0.8,
        Color32::from_gray(0x92),
    );
    g.circle_filled(
        c + vec2(-r * 0.2, -r * 0.3),
        r * 0.45,
        Color32::from_gray(0xa2),
    );
    g.circle_stroke(c, r, Stroke::new(1.0, Color32::from_gray(0x56)));
    // Ticks beside the lower half of the bezel (about 5 and 7 o'clock).
    for deg in [-140.0f32, 140.0] {
        let a = (deg - 90.0).to_radians();
        let d = vec2(a.cos(), a.sin());
        g.line_segment(
            [c + d * (r + 3.4), c + d * (r + 5.6)],
            Stroke::new(1.0, Color32::from_gray(0xb4)),
        );
    }
    // Pointer.
    let a = (-90.0 + value.clamp(-1.0, 1.0) * 135.0).to_radians();
    let d = vec2(a.cos(), a.sin());
    g.line_segment(
        [c + d * (r * 0.18), c + d * (r * 0.9)],
        Stroke::new(1.8, Color32::from_gray(0xf6)),
    );
}

/// Precision radial-arc pan dial with background guide arc, dynamic colored swept arc,
/// center top notch, and obsidian tactile inner dial with indicator.
pub fn draw_radial_pan(g: &Painter, c: Pos2, r: f32, value: f32) {
    let arc_r = r + 1.5;
    // Outer track arc (-135 deg to +135 deg, relative to 12 o'clock / -90 deg)
    let total_steps = 24;
    let base_pts: Vec<Pos2> = (0..=total_steps)
        .map(|i| {
            let frac = i as f32 / total_steps as f32;
            let deg = -135.0 + frac * 270.0;
            let rad = (deg - 90.0).to_radians();
            c + vec2(rad.cos(), rad.sin()) * arc_r
        })
        .collect();
    g.add(Shape::line(
        base_pts,
        Stroke::new(2.2, Color32::from_rgb(0x34, 0x34, 0x39)),
    ));

    // Center 12 o'clock notch
    let top_notch = c + vec2(0.0, -arc_r);
    g.line_segment(
        [top_notch + vec2(0.0, -1.0), top_notch + vec2(0.0, 1.5)],
        Stroke::new(1.0, Color32::from_rgb(0x8e, 0x8e, 0x93)),
    );

    // Active swept arc from 12 o'clock to current pan
    let val_clamped = value.clamp(-1.0, 1.0);
    if val_clamped.abs() > 0.02 {
        let swept_steps = 12;
        let arc_col = Color32::from_rgb(0x3a, 0x7b, 0xd5);
        let active_pts: Vec<Pos2> = (0..=swept_steps)
            .map(|i| {
                let frac = i as f32 / swept_steps as f32;
                let deg = frac * val_clamped * 135.0;
                let rad = (deg - 90.0).to_radians();
                c + vec2(rad.cos(), rad.sin()) * arc_r
            })
            .collect();
        g.add(Shape::line(active_pts, Stroke::new(2.4, arc_col)));
    }

    // Solid dark-metallic inner dial knob body with metallic rim and crisp white radial pointer
    let inner_r = (r - 2.0).max(4.0);
    g.circle_filled(c + vec2(0.5, 1.0), inner_r, Color32::from_black_alpha(95));
    g.circle_filled(c, inner_r, Color32::from_rgb(0x24, 0x26, 0x2a));
    g.circle_stroke(
        c,
        inner_r,
        Stroke::new(1.0, Color32::from_rgb(0x46, 0x48, 0x50)),
    );
    let ptr_rad = (val_clamped * 135.0 - 90.0).to_radians();
    g.line_segment(
        [
            c + vec2(ptr_rad.cos(), ptr_rad.sin()) * (inner_r * 0.25),
            c + vec2(ptr_rad.cos(), ptr_rad.sin()) * (inner_r - 1.5),
        ],
        Stroke::new(1.5, Color32::from_rgb(0xf2, 0xf3, 0xf7)),
    );

    // Pointer indicator
    let a = (-90.0 + val_clamped * 135.0).to_radians();
    let d = vec2(a.cos(), a.sin());
    g.line_segment(
        [c + d * (inner_r * 0.25), c + d * (inner_r - 1.0)],
        Stroke::new(1.6, Color32::from_rgb(0xe6, 0xe6, 0xeb)),
    );
}
