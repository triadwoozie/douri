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

/// Interactive parameter readout label (`webaudio-param` behavior): clicking or double-clicking
/// opens an inline edit field parsing `Hz`, `kHz`, `ms`, `s`, `dB`, and `%` suffixes.
#[allow(clippy::too_many_arguments)]
pub fn daw_param_readout(
    ui: &mut Ui,
    g: &Painter,
    rect: Rect,
    id: Id,
    formatted: &str,
    unit: &str,
    min: f32,
    max: f32,
    show_value: bool,
    midi_cc: Option<u8>,
) -> Option<f32> {
    let mut editing = ui.ctx().data_mut(|d| d.get_temp::<String>(id));
    if let Some(mut buf) = editing.take() {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let resp = child.add(
            egui::TextEdit::singleline(&mut buf)
                .id(id.with("input"))
                .font(FontId::monospace(10.5))
                .align(egui::Align2::CENTER_CENTER)
                .desired_width(rect.width()),
        );
        if !resp.has_focus() && !resp.gained_focus() {
            resp.request_focus();
        }
        let enter = child.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = child.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            ui.ctx().data_mut(|d| d.remove::<String>(id));
            return None;
        }
        if enter || resp.lost_focus() {
            ui.ctx().data_mut(|d| d.remove::<String>(id));
            return crate::daw_knob::parse_param_text(&buf, unit, min, max);
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, buf));
        return None;
    }

    let resp = ui.interact(rect, id, Sense::click());
    if resp.clicked() || resp.double_clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(id, formatted.to_string()));
        return None;
    }

    if show_value || resp.hovered() {
        g.text(
            rect.center(),
            Align2::CENTER_CENTER,
            formatted,
            FontId::monospace(10.5),
            Color32::from_rgb(0xd8, 0xe6, 0xf8),
        );
    } else if let Some(cc) = midi_cc {
        g.text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!("CC {cc}"),
            FontId::monospace(9.5),
            Color32::from_rgb(0x8e, 0xc2, 0xff),
        );
    }
    None
}

/// Vertical hardware rocker switch (`OFF` / `ON` recessed slot with illuminated cap).
pub fn daw_rocker_switch(ui: &mut Ui, rect: Rect, id: Id, on: bool, accent_cyan: bool) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let g = ui.painter();
    g.rect_filled(rect, 4.0, Color32::from_rgb(0x14, 0x14, 0x16));
    g.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(0x30, 0x30, 0x34)),
        egui::StrokeKind::Inside,
    );
    let inner = rect.shrink(2.0);
    let half_h = inner.height() * 0.5;
    let cap = if on {
        Rect::from_min_size(inner.min, vec2(inner.width(), half_h))
    } else {
        Rect::from_min_size(
            pos2(inner.left(), inner.center().y),
            vec2(inner.width(), half_h),
        )
    };
    let on_col = if accent_cyan {
        Color32::from_rgb(0x51, 0xee, 0xfc)
    } else {
        Color32::from_rgb(0xf5, 0xa6, 0x23)
    };
    g.rect_filled(
        cap,
        3.0,
        if on {
            on_col
        } else {
            Color32::from_rgb(0x38, 0x38, 0x3e)
        },
    );
    g.text(
        pos2(inner.center().x, inner.top() + half_h * 0.5),
        Align2::CENTER_CENTER,
        "ON",
        FontId::proportional(8.0),
        if on {
            Color32::from_rgb(0x11, 0x12, 0x15)
        } else {
            TEXT_DIM
        },
    );
    g.text(
        pos2(inner.center().x, inner.bottom() - half_h * 0.5),
        Align2::CENTER_CENTER,
        "OFF",
        FontId::proportional(8.0),
        if !on { TEXT } else { TEXT_DIM },
    );
    resp.clicked()
}

/// Circular tactile LED pushbutton with glowing green/amber status dot.
pub fn daw_led_button(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    id: Id,
    on: bool,
    amber: bool,
) -> bool {
    let rect = Rect::from_center_size(center, vec2(radius * 2.2, radius * 2.2));
    let resp = ui.interact(rect, id, Sense::click());
    let g = ui.painter();
    g.circle_filled(
        center + vec2(0.0, 1.0),
        radius,
        Color32::from_black_alpha(100),
    );
    g.circle_filled(
        center,
        radius,
        if resp.hovered() {
            Color32::from_rgb(0x32, 0x32, 0x38)
        } else {
            Color32::from_rgb(0x26, 0x26, 0x2a)
        },
    );
    g.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_rgb(0x42, 0x42, 0x48)),
    );
    let led_col = if on {
        if amber {
            Color32::from_rgb(0xf5, 0xa6, 0x23)
        } else {
            Color32::from_rgb(0x34, 0xc7, 0x59)
        }
    } else {
        Color32::from_rgb(0x1a, 0x1a, 0x1d)
    };
    g.circle_filled(center, (radius * 0.34).max(2.5), led_col);
    resp.clicked()
}

/// Backlit hardware toggle plate (e.g. `DRIVE`, `FREEZE`, `SYNC`).
pub fn daw_backlit_plate(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    label: &str,
    on: bool,
    amber: bool,
) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let g = ui.painter();
    let active_col = if amber {
        Color32::from_rgb(0xe8, 0x9b, 0x20)
    } else {
        Color32::from_rgb(0x12, 0x69, 0xcc)
    };
    g.rect_filled(
        rect,
        5.0,
        if on {
            active_col
        } else if resp.hovered() {
            Color32::from_rgb(0x2e, 0x2e, 0x33)
        } else {
            Color32::from_rgb(0x24, 0x24, 0x28)
        },
    );
    g.rect_stroke(
        rect,
        5.0,
        Stroke::new(1.0, Color32::from_rgb(0x38, 0x38, 0x3e)),
        egui::StrokeKind::Inside,
    );
    g.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        if on && amber {
            Color32::from_rgb(0x12, 0x12, 0x14)
        } else if on {
            Color32::WHITE
        } else {
            TEXT_DIM
        },
    );
    resp.clicked()
}

/// Radial stepped mode selector with clickable perimeter mode labels and status LEDs.
pub fn daw_radial_selector(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    options: &[&str],
    selected: usize,
) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    let g = ui.painter();
    let center = rect.center();
    let dial_r = (rect.height().min(rect.width()) * 0.24).clamp(14.0, 22.0);
    let label_r = dial_r + 18.0;
    let count = options.len();
    let mut next = None;

    for (i, label) in options.iter().enumerate() {
        let frac = if count > 1 {
            i as f32 / (count - 1) as f32
        } else {
            0.5
        };
        let a = (-135.0 + frac * 270.0 - 90.0).to_radians();
        let dir = vec2(a.cos(), a.sin());
        let led_pos = center + dir * (dial_r + 6.0);
        let is_on = i == selected.min(count - 1);
        g.circle_filled(
            led_pos,
            2.2,
            if is_on {
                Color32::from_rgb(0x51, 0xee, 0xfc)
            } else {
                Color32::from_rgb(0x3a, 0x3a, 0x40)
            },
        );
        let lbl_pos = center + dir * label_r;
        let hit = Rect::from_center_size(lbl_pos, vec2(44.0, 14.0));
        let resp = ui.interact(hit, id.with(("opt", i)), Sense::click());
        g.text(
            lbl_pos,
            Align2::CENTER_CENTER,
            *label,
            FontId::proportional(9.5),
            if is_on || resp.hovered() {
                TEXT
            } else {
                TEXT_DIM
            },
        );
        if resp.clicked() {
            next = Some(i);
        }
    }

    g.circle_filled(center, dial_r, Color32::from_rgb(0x26, 0x26, 0x2a));
    g.circle_stroke(
        center,
        dial_r,
        Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3a, 0x40)),
    );
    let sel_frac = if count > 1 {
        selected.min(count - 1) as f32 / (count - 1) as f32
    } else {
        0.5
    };
    let ptr_a = (-135.0 + sel_frac * 270.0 - 90.0).to_radians();
    let ptr_d = vec2(ptr_a.cos(), ptr_a.sin());
    g.line_segment(
        [
            center + ptr_d * (dial_r * 0.28),
            center + ptr_d * (dial_r - 2.0),
        ],
        Stroke::new(2.0, Color32::WHITE),
    );
    next
}

/// Vertical ruled multi-slider bank (`rel` relative drag or `abs` jump mode).
pub fn daw_xyz_sliders(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    labels: &[&str],
    values: &mut [f32],
    relative_drag: bool,
) -> bool {
    let n = labels.len().min(values.len()).max(1);
    let col_w = rect.width() / n as f32;
    let g = ui.painter();
    let mut changed = false;

    for i in 0..n {
        let col = Rect::from_min_size(
            pos2(rect.left() + i as f32 * col_w, rect.top()),
            vec2(col_w, rect.height()),
        );
        let track = Rect::from_center_size(
            pos2(col.center().x, col.center().y - 6.0),
            vec2(4.0, (col.height() - 26.0).max(24.0)),
        );
        let hit = track.expand2(vec2(10.0, 4.0));
        let resp = ui.interact(hit, id.with(i), Sense::click_and_drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        let v = &mut values[i];
        *v = v.clamp(0.0, 1.0);
        if resp.dragged() {
            let fine = if ui.input(|inp| inp.modifiers.shift) {
                0.1
            } else {
                1.0
            };
            if relative_drag {
                *v = (*v - resp.drag_delta().y / track.height() * fine).clamp(0.0, 1.0);
            } else if let Some(ptr) = resp.interact_pointer_pos() {
                *v = (1.0 - (ptr.y - track.top()) / track.height()).clamp(0.0, 1.0);
            }
            changed = true;
        }
        g.rect_filled(track, 2.0, Color32::from_rgb(0x18, 0x18, 0x1a));
        let fill_h = track.height() * *v;
        let fill = Rect::from_min_max(pos2(track.left(), track.bottom() - fill_h), track.max);
        g.rect_filled(fill, 2.0, Color32::from_rgb(0x12, 0x69, 0xcc));
        let thumb_y = track.bottom() - fill_h;
        g.rect_filled(
            Rect::from_center_size(pos2(track.center().x, thumb_y), vec2(14.0, 6.0)),
            2.0,
            Color32::from_rgb(0xe5, 0xe5, 0xea),
        );
        g.text(
            pos2(col.center().x, col.bottom() - 8.0),
            Align2::CENTER_CENTER,
            labels[i],
            FontId::proportional(9.5),
            TEXT_DIM,
        );
    }
    changed
}

/// Dark emerald/cyan phosphor waveform/LFO scope (`#0c1f1d` screen, `#38d9a9` trace).
pub fn daw_phosphor_scope(g: &Painter, rect: Rect, samples: &[f32]) {
    g.rect_filled(rect, 6.0, Color32::from_rgb(0x0c, 0x1f, 0x1d));
    g.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x1b, 0x3d, 0x38)),
        egui::StrokeKind::Inside,
    );
    let inner = rect.shrink(4.0);
    let grid_col = Color32::from_rgba_unmultiplied(0x38, 0xd9, 0xa9, 28);
    g.line_segment(
        [
            pos2(inner.left(), inner.center().y),
            pos2(inner.right(), inner.center().y),
        ],
        Stroke::new(1.0, grid_col),
    );
    g.line_segment(
        [
            pos2(inner.center().x, inner.top()),
            pos2(inner.center().x, inner.bottom()),
        ],
        Stroke::new(1.0, grid_col),
    );
    if samples.len() >= 2 {
        let pts: Vec<Pos2> = samples
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let t = i as f32 / (samples.len() - 1) as f32;
                let y = inner.center().y - s.clamp(-1.0, 1.0) * (inner.height() * 0.44);
                pos2(inner.left() + t * inner.width(), y)
            })
            .collect();
        g.add(Shape::line(
            pts,
            Stroke::new(1.6, Color32::from_rgb(0x38, 0xd9, 0xa9)),
        ));
    }
}
