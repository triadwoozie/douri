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
            ui.ctx().set_cursor_icon(if resp.dragged() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
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

/// Hardware-style 2D XY Pad with recessed well, crosshairs, axis labels, and glowing puck.
pub fn daw_xy_pad(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    x_val: &mut f32,
    y_val: &mut f32,
    x_label: &str,
    y_label: &str,
    accent: Color32,
) -> bool {
    let g = ui.painter();
    g.rect_filled(rect, 4.0, Color32::from_rgb(0x13, 0x14, 0x18));
    g.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, Color32::from_rgb(0x28, 0x2a, 0x30)),
        egui::StrokeKind::Inside,
    );

    let inner = rect.shrink2(vec2(16.0, 14.0));
    let grid_color = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 35);
    // Center crosshairs
    g.line_segment(
        [
            pos2(inner.left(), inner.center().y),
            pos2(inner.right(), inner.center().y),
        ],
        Stroke::new(1.0, grid_color),
    );
    g.line_segment(
        [
            pos2(inner.center().x, inner.top()),
            pos2(inner.center().x, inner.bottom()),
        ],
        Stroke::new(1.0, grid_color),
    );

    // Axis labels
    if !y_label.is_empty() {
        g.text(
            pos2(rect.left() + 6.0, inner.top() + 4.0),
            Align2::LEFT_TOP,
            y_label,
            FontId::proportional(9.5),
            TEXT_DIM,
        );
    }
    if !x_label.is_empty() {
        g.text(
            pos2(inner.center().x, rect.bottom() - 3.0),
            Align2::CENTER_BOTTOM,
            x_label,
            FontId::proportional(9.5),
            TEXT_DIM,
        );
    }

    let resp = ui.interact(rect, id, Sense::click_and_drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(if resp.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    }

    let mut changed = false;
    *x_val = x_val.clamp(0.0, 1.0);
    *y_val = y_val.clamp(0.0, 1.0);

    if resp.dragged() || resp.clicked() {
        let fine = if ui.input(|i| i.modifiers.shift) {
            0.2
        } else {
            1.0
        };
        if let Some(ptr) = resp.interact_pointer_pos() {
            let target_x = ((ptr.x - inner.left()) / inner.width().max(1.0)).clamp(0.0, 1.0);
            let target_y = (1.0 - (ptr.y - inner.top()) / inner.height().max(1.0)).clamp(0.0, 1.0);
            if resp.clicked() {
                *x_val = target_x;
                *y_val = target_y;
            } else {
                let dx = resp.drag_delta().x / inner.width().max(1.0) * fine;
                let dy = -resp.drag_delta().y / inner.height().max(1.0) * fine;
                *x_val = (*x_val + dx).clamp(0.0, 1.0);
                *y_val = (*y_val + dy).clamp(0.0, 1.0);
            }
            changed = true;
        }
    }

    let puck_pos = pos2(
        inner.left() + inner.width() * *x_val,
        inner.bottom() - inner.height() * *y_val,
    );

    // Glowing puck
    let glow_color = Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 45);
    g.circle_filled(puck_pos, 11.0, glow_color);
    g.circle_stroke(puck_pos, 7.5, Stroke::new(1.5, accent));
    g.circle_filled(puck_pos, 5.0, accent);
    g.circle_filled(puck_pos, 2.0, Color32::WHITE);

    changed
}

/// Alias for daw_adsr_envelope editor widget.
pub fn daw_envelope_editor(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    a: &mut f32,
    d: &mut f32,
    s: &mut f32,
    r: &mut f32,
) -> bool {
    daw_adsr_envelope(ui, id, rect, a, d, s, r, 1.0, 1.0, 1.0)
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

/// Floor level in dBFS for VU / PPM meter ballistics (ported from `ouestlabs/audio-ui`).
pub const METER_FLOOR_DB: f32 = -60.0;

/// Converts a linear amplitude (`0.0..=1.0+`) to dBFS clamped to [`METER_FLOOR_DB`].
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 1e-6 {
        METER_FLOOR_DB
    } else {
        (20.0 * linear.log10()).max(METER_FLOOR_DB)
    }
}

/// IEC 60268-17 / ANSI C16.5 VU meter ballistics (300 ms rise, 0.81 damping, ~1.3% overshoot),
/// ported from `ouestlabs/audio-ui` (`meter/ballistics.ts`).
#[derive(Clone, Copy, Debug)]
pub struct VuBallistics {
    pos: f32,
    vel: f32,
}

impl Default for VuBallistics {
    fn default() -> Self {
        Self {
            pos: METER_FLOOR_DB,
            vel: 0.0,
        }
    }
}

impl VuBallistics {
    /// Advances the VU needle/bar toward `target_db` over `dt` seconds.
    pub fn step(&mut self, target_db: f32, dt: f32) -> f32 {
        let target = target_db.clamp(METER_FLOOR_DB, 6.0);
        let dt = dt.clamp(0.0, 0.05);
        if dt <= 0.0 {
            return self.pos;
        }
        // 300 ms rise time, damping ratio zeta = 0.81 (~1.3% overshoot)
        let omega = 1.9 / 0.300;
        let zeta = 0.81;
        let steps = ((dt / (1.0 / 120.0)).ceil() as usize).max(1);
        let sub_dt = dt / steps as f32;
        for _ in 0..steps {
            let acc = omega * omega * (target - self.pos) - 2.0 * zeta * omega * self.vel;
            self.vel += acc * sub_dt;
            self.pos += self.vel * sub_dt;
        }
        self.pos = self.pos.clamp(METER_FLOOR_DB, 6.0);
        self.pos
    }

    /// Current smoothed level in dB.
    pub fn db(self) -> f32 {
        self.pos
    }

    /// Normalized `0.0..=1.0` position across `[METER_FLOOR_DB, 0.0]`.
    pub fn norm(self) -> f32 {
        ((self.pos - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
    }
}

/// IEC 60268-10 Type I PPM ballistics (5 ms attack, 20 dB / 1.5 s linear release),
/// ported from `ouestlabs/audio-ui` (`meter/ballistics.ts`).
#[derive(Clone, Copy, Debug)]
pub struct PpmBallistics {
    level_db: f32,
}

impl Default for PpmBallistics {
    fn default() -> Self {
        Self {
            level_db: METER_FLOOR_DB,
        }
    }
}

impl PpmBallistics {
    /// Advances the PPM level toward `target_db` over `dt` seconds.
    pub fn step(&mut self, target_db: f32, dt: f32) -> f32 {
        let target = target_db.clamp(METER_FLOOR_DB, 6.0);
        let dt = dt.clamp(0.0, 0.05);
        if target > self.level_db {
            // 5 ms attack time constant
            let alpha = 1.0 - (-dt / 0.005).exp();
            self.level_db += (target - self.level_db) * alpha;
        } else {
            // 20 dB / 1.5 s release rate
            let drop = (20.0 / 1.5) * dt;
            self.level_db = (self.level_db - drop).max(target).max(METER_FLOOR_DB);
        }
        self.level_db
    }

    /// Current smoothed level in dB.
    pub fn db(self) -> f32 {
        self.level_db
    }

    /// Normalized `0.0..=1.0` position across `[METER_FLOOR_DB, 0.0]`.
    pub fn norm(self) -> f32 {
        ((self.level_db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
    }
}

/// 45°-rotated Mid/Side stereo goniometer (vectorscope) with trail history,
/// ported from `ouestlabs/audio-ui` (`stereo-field.tsx`).
pub fn daw_goniometer(
    ui: &mut Ui,
    rect: Rect,
    left: f32,
    right: f32,
    history: &mut Vec<(f32, f32)>,
) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 6.0, Color32::from_rgb(0x12, 0x15, 0x1a));
    g.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x2e, 0x33, 0x3d)),
        egui::StrokeKind::Inside,
    );

    let center = rect.center();
    let radius = (rect.width().min(rect.height()) * 0.5) - 10.0;
    if radius <= 4.0 {
        return;
    }

    // Outer circle & 45-degree L/R + M/S crosshairs
    let grid_col = Color32::from_rgba_unmultiplied(0x90, 0x98, 0xa6, 42);
    g.circle_stroke(center, radius, Stroke::new(1.0, grid_col));
    let diag = radius * std::f32::consts::FRAC_1_SQRT_2;
    g.line_segment(
        [
            pos2(center.x - diag, center.y - diag),
            pos2(center.x + diag, center.y + diag),
        ],
        Stroke::new(1.0, grid_col),
    );
    g.line_segment(
        [
            pos2(center.x + diag, center.y - diag),
            pos2(center.x - diag, center.y + diag),
        ],
        Stroke::new(1.0, grid_col),
    );
    g.line_segment(
        [
            pos2(center.x, center.y - radius),
            pos2(center.x, center.y + radius),
        ],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x90, 0x98, 0xa6, 24)),
    );
    g.line_segment(
        [
            pos2(center.x - radius, center.y),
            pos2(center.x + radius, center.y),
        ],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x90, 0x98, 0xa6, 24)),
    );

    // Axis labels (+L, +R, M, S)
    let label_col = Color32::from_rgb(0x78, 0x80, 0x8e);
    g.text(
        pos2(center.x, rect.top() + 7.0),
        Align2::CENTER_CENTER,
        "M",
        FontId::proportional(8.5),
        label_col,
    );
    g.text(
        pos2(center.x - diag - 3.0, center.y - diag),
        Align2::RIGHT_CENTER,
        "L",
        FontId::proportional(8.5),
        label_col,
    );
    g.text(
        pos2(center.x + diag + 3.0, center.y - diag),
        Align2::LEFT_CENTER,
        "R",
        FontId::proportional(8.5),
        label_col,
    );

    // Push current (l, r) point to trail history (capped at 48 samples)
    let l = left.clamp(0.0, 1.5);
    let r = right.clamp(0.0, 1.5);
    history.push((l, r));
    if history.len() > 48 {
        history.drain(0..history.len() - 48);
    }

    // 45-degree rotation: x = (r - l) * FRAC_1_SQRT_2, y = -(l + r) * FRAC_1_SQRT_2
    let to_pt = |lv: f32, rv: f32| -> Pos2 {
        let mid = (lv + rv) * std::f32::consts::FRAC_1_SQRT_2;
        let side = (rv - lv) * std::f32::consts::FRAC_1_SQRT_2;
        pos2(
            center.x + side.clamp(-1.0, 1.0) * radius,
            center.y - (mid * 0.7).clamp(-1.0, 1.0) * radius,
        )
    };

    let n = history.len();
    for (idx, &(lv, rv)) in history.iter().enumerate() {
        let age = (idx + 1) as f32 / n.max(1) as f32;
        let alpha = (age * age * 190.0) as u8;
        let pt = to_pt(lv, rv);
        g.circle_filled(
            pt,
            1.4 + age * 1.4,
            Color32::from_rgba_unmultiplied(0x38, 0xd9, 0xa9, alpha),
        );
    }

    if l > 0.002 || r > 0.002 {
        let head = to_pt(l, r);
        g.line_segment(
            [center, head],
            Stroke::new(1.5, Color32::from_rgba_unmultiplied(0x38, 0xd9, 0xa9, 170)),
        );
        g.circle_filled(head, 3.0, Color32::from_rgb(0x64, 0xf0, 0xc8));
    }
}

/// Interactive draggable ADSR breakpoint envelope editor, ported from
/// `ouestlabs/audio-ui` (`envelope/envelope.ts`). Returns `true` if any parameter changed.
#[allow(clippy::too_many_arguments)]
pub fn daw_adsr_envelope(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    attack: &mut f32,
    decay: &mut f32,
    sustain: &mut f32,
    release: &mut f32,
    max_a: f32,
    max_d: f32,
    max_r: f32,
) -> bool {
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let g = ui.painter_at(rect);

    g.rect_filled(rect, 6.0, Color32::from_rgb(0x14, 0x18, 0x1e));
    g.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x2c, 0x32, 0x3d)),
        egui::StrokeKind::Inside,
    );

    let inner = rect.shrink2(vec2(10.0, 10.0));
    if inner.width() <= 20.0 || inner.height() <= 16.0 {
        return false;
    }

    // Subtle 4x4 grid
    let grid_col = Color32::from_rgba_unmultiplied(0x8a, 0x94, 0xa6, 24);
    for i in 1..4 {
        let t = i as f32 / 4.0;
        let x = inner.left() + inner.width() * t;
        let y = inner.top() + inner.height() * t;
        g.line_segment(
            [pos2(x, inner.top()), pos2(x, inner.bottom())],
            Stroke::new(1.0, grid_col),
        );
        g.line_segment(
            [pos2(inner.left(), y), pos2(inner.right(), y)],
            Stroke::new(1.0, grid_col),
        );
    }

    // Allocate normalized horizontal segments: Attack (0..0.30), Decay (0..0.30), Sustain hold (0.15), Release (0..0.25)
    let a_norm = (*attack / max_a.max(0.001)).clamp(0.0, 1.0);
    let d_norm = (*decay / max_d.max(0.001)).clamp(0.0, 1.0);
    let s_norm = (*sustain).clamp(0.0, 1.0);
    let r_norm = (*release / max_r.max(0.001)).clamp(0.0, 1.0);

    let x0 = inner.left();
    let xa = x0 + inner.width() * (0.04 + a_norm * 0.26);
    let xd = xa + inner.width() * (0.04 + d_norm * 0.26);
    let xs = xd + inner.width() * 0.14;
    let xr = (xs + inner.width() * (0.04 + r_norm * 0.22)).min(inner.right());

    let y_bot = inner.bottom();
    let y_top = inner.top();
    let ys = y_bot - s_norm * inner.height();

    let p_start = pos2(x0, y_bot);
    let p_att = pos2(xa, y_top);
    let p_dec = pos2(xd, ys);
    let p_sus = pos2(xs, ys);
    let p_rel = pos2(xr, y_bot);

    // Drag interaction: lock active node on pointer press, update on drag
    let mut changed = false;
    let active_id = id.with("active_node");
    if resp.drag_started() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let nodes = [(0usize, p_att), (1, p_dec), (2, p_rel)];
            let nearest = nodes
                .iter()
                .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
                .map(|n| n.0);
            ui.ctx()
                .memory_mut(|m| m.data.insert_temp(active_id, nearest));
        }
    }
    if resp.dragged() {
        let active: Option<usize> = ui
            .ctx()
            .memory(|m| m.data.get_temp(active_id).unwrap_or(None));
        if let Some(pos) = resp.interact_pointer_pos() {
            match active {
                Some(0) => {
                    let frac = ((pos.x - x0) / inner.width() - 0.04) / 0.26;
                    *attack = (frac.clamp(0.0, 1.0) * max_a).max(0.001);
                    changed = true;
                }
                Some(1) => {
                    let frac_x = ((pos.x - xa) / inner.width() - 0.04) / 0.26;
                    *decay = (frac_x.clamp(0.0, 1.0) * max_d).max(0.001);
                    *sustain = ((y_bot - pos.y) / inner.height()).clamp(0.0, 1.0);
                    changed = true;
                }
                Some(2) => {
                    let frac = ((pos.x - xs) / inner.width() - 0.04) / 0.22;
                    *release = (frac.clamp(0.0, 1.0) * max_r).max(0.01);
                    changed = true;
                }
                _ => {}
            }
        }
    }
    if resp.drag_stopped() {
        ui.ctx()
            .memory_mut(|m| m.data.insert_temp::<Option<usize>>(active_id, None));
    }

    // Filled polygon under ADSR curve
    let fill_col = Color32::from_rgba_unmultiplied(0x38, 0xd9, 0xa9, 36);
    let poly = vec![p_start, p_att, p_dec, p_sus, p_rel];
    g.add(Shape::convex_polygon(poly.clone(), fill_col, Stroke::NONE));
    g.add(Shape::line(
        poly,
        Stroke::new(2.0, Color32::from_rgb(0x38, 0xd9, 0xa9)),
    ));

    // Breakpoint handles
    for (idx, pt) in [(0usize, p_att), (1, p_dec), (2, p_rel)] {
        let hovered = resp.hover_pos().is_some_and(|hp| hp.distance(pt) <= 9.0);
        let active_node: Option<usize> = ui
            .ctx()
            .memory(|m| m.data.get_temp(active_id).unwrap_or(None));
        let is_active = active_node == Some(idx);
        let r = if is_active || hovered { 5.5 } else { 4.2 };
        g.circle_filled(pt, r, Color32::from_rgb(0x14, 0x18, 0x1e));
        g.circle_stroke(
            pt,
            r,
            Stroke::new(
                2.0,
                if is_active {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(0x38, 0xd9, 0xa9)
                },
            ),
        );
    }

    // Compact readout in top-right
    g.text(
        pos2(inner.right() - 2.0, inner.top() + 2.0),
        Align2::RIGHT_TOP,
        format!(
            "A {:.0}ms · D {:.0}ms · S {:.0}% · R {:.0}ms",
            *attack * 1000.0,
            *decay * 1000.0,
            *sustain * 100.0,
            *release * 1000.0
        ),
        FontId::proportional(9.5),
        TEXT_DIM,
    );

    changed
}

/// Velocity/hit-intensity proportional pad meter bar & glow overlay, inspired by
/// `ouestlabs/audio-ui` (`pad/pad.ts`).
pub fn daw_pad_meter_glow(g: &Painter, rect: Rect, level: f32, active: bool, accent: Color32) {
    let lv = level.clamp(0.0, 1.0);
    if lv > 0.01 {
        let alpha = (lv * if active { 72.0 } else { 44.0 }) as u8;
        g.rect_filled(
            rect.shrink(1.5),
            5.0,
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
        );
        // Bottom velocity/activity meter strip
        let bar_bg = Rect::from_min_max(
            pos2(rect.left() + 6.0, rect.bottom() - 4.5),
            pos2(rect.right() - 6.0, rect.bottom() - 2.0),
        );
        g.rect_filled(bar_bg, 1.0, Color32::from_rgb(0x1b, 0x1e, 0x24));
        let fill_w = bar_bg.width() * lv;
        g.rect_filled(
            Rect::from_min_size(bar_bg.min, vec2(fill_w, bar_bg.height())),
            1.0,
            accent,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_to_db_maps_unity_and_silence() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 1e-4);
        assert!((linear_to_db(0.0) - METER_FLOOR_DB).abs() < 1e-4);
        assert!((linear_to_db(0.5) - (-6.0206)).abs() < 0.01);
    }

    #[test]
    fn vu_ballistics_rises_toward_target_and_settles() {
        let mut vu = VuBallistics::default();
        assert_eq!(vu.db(), METER_FLOOR_DB);
        for _ in 0..60 {
            vu.step(0.0, 1.0 / 60.0);
        }
        assert!(
            vu.db() > -3.0,
            "VU should settle near 0 dB after 1s, got {}",
            vu.db()
        );
        assert!(vu.norm() > 0.95);
    }

    #[test]
    fn ppm_ballistics_fast_attack_and_controlled_release() {
        let mut ppm = PpmBallistics::default();
        // 5 ms attack should jump quickly toward 0 dB
        ppm.step(0.0, 0.02);
        assert!(
            ppm.db() > -2.0,
            "PPM attack should be fast, got {}",
            ppm.db()
        );

        // Release at 20 dB / 1.5 s: after 0.75 s should drop ~10 dB
        let before = ppm.db();
        for _ in 0..15 {
            ppm.step(METER_FLOOR_DB, 0.05);
        }
        let drop = before - ppm.db();
        assert!(
            (drop - 10.0).abs() < 0.5,
            "expected ~10 dB drop, got {drop}"
        );
    }
}
