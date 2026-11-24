// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! The plug-in's window: the instrument or effect as a Pro Wave hardware unit, as the program
//! draws it (PW-1, "Hardware"): the faceplate with the model code and name, the page keys, the
//! knobs with their LCD readouts, choice keys and menus, and a RESET key; Wave Draw's display
//! where its wave is drawn; FM6's SYX key, which opens a .syx bank and puts one of its voices
//! on the instrument; the presets in a menu on the faceplate.
//!
//! `theme.rs`, `hw.rs` and `gear.rs` are copies of the program's own (`prowave-app/src`), with
//! only what this crate's egui version (nih_plug_egui's) needs changed; the layout below follows
//! `native_fx_ui.rs` and `native_inst_ui.rs`. Every change goes to the host as a gesture
//! (begin, set, end), so hosts record it as automation and undo it in one step.

#[allow(dead_code, clippy::all)]
pub mod gear;
#[allow(dead_code, clippy::all)]
pub mod hw;
#[allow(dead_code, clippy::all)]
pub mod theme;

use crate::pages::{label, model_code, Page, Row};
use crate::params::PlugParams;
use gear::{Cap, Change, Control, LedAt, CELL_H, CELL_W};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, pos2, vec2, Align2, Pos2, Rect, Sense, Ui};
use nih_plug_egui::{create_egui_editor, EguiState};
use prowave_engine::native_inst::{fm6, wave_draw};
use prowave_engine::NativeFxInfo;
use std::sync::{Arc, Mutex};

/// Margin inside the faceplate.
const PAD: f32 = 26.0;
/// Width of a page key and the gap between two.
const PAGE_KEY_W: f32 = 62.0;
const PAGE_KEY_GAP: f32 = 6.0;
/// Height of the row of page keys, with the space under it.
const PAGE_ROW_H: f32 = 40.0;
/// Wave Draw's display and its row of shape keys.
const GRAPH_H: f32 = 150.0;
const SHAPE_ROW_H: f32 = 40.0;

fn draws_wave(info: &NativeFxInfo) -> bool {
    info.id == wave_draw::ID
}

/// Whether the instrument reads voices from .syx files (FM6).
fn loads_syx(info: &NativeFxInfo) -> bool {
    info.id == fm6::ID
}

/// The voices of a .syx bank: (name, parameter values).
pub type Bank = Vec<(String, Vec<f32>)>;

/// The voices in the .syx file at `path` (none when it cannot be read or is not a bank), as the
/// program reads them (`fm6::bank_voices`).
pub fn read_bank(path: &std::path::Path) -> Bank {
    std::fs::read(path)
        .map(|bytes| fm6::bank_voices(&bytes))
        .unwrap_or_default()
}

/// The edits that put `voice` (a bank voice: FM6's parameters 0 to 144) on the instrument,
/// one gesture per parameter that changes; the others (voices, level...) are kept, as in the
/// program.
fn take_voice(vals: &[i32], voice: &[f32]) -> Vec<Edit> {
    let mut out = Vec::new();
    for (k, (old, new)) in vals.iter().zip(voice).enumerate() {
        let new = new.round() as i32;
        if *old != new {
            set_once(&mut out, k, new);
        }
    }
    out
}

/// Width of the widest row.
fn unit_width(rows: &[Row]) -> f32 {
    let cells = rows
        .iter()
        .map(|r| {
            r.iter().map(|s| s.params.len()).sum::<usize>() as f32 + (r.len() as f32 - 1.0) * 0.25
        })
        .fold(0.0, f32::max);
    (cells * CELL_W + PAD * 2.0).max(560.0)
}

/// Height of the rows.
fn rows_height(rows: &[Row]) -> f32 {
    rows.iter()
        .map(|r| {
            let labelled = r.iter().any(|s| !s.label.is_empty());
            CELL_H + 10.0 + if labelled { 22.0 } else { 0.0 }
        })
        .sum()
}

/// Page keys along the top: one per page when there is more than one.
fn page_keys(pages: &[Page]) -> usize {
    if pages.len() > 1 {
        pages.len()
    } else {
        0
    }
}

/// Keys along the top: the page keys and FM6's SYX key.
fn top_keys(info: &NativeFxInfo, pages: &[Page]) -> usize {
    page_keys(pages) + usize::from(loads_syx(info))
}

/// The panel between the header and the foot: (width, height).
fn body_size(info: &NativeFxInfo, pages: &[Page]) -> (f32, f32) {
    let keys = top_keys(info, pages);
    let keys_w = keys as f32 * (PAGE_KEY_W + PAGE_KEY_GAP) + PAD * 2.0;
    let width = pages
        .iter()
        .map(|p| unit_width(&p.rows))
        .fold(keys_w, f32::max);
    let rows_h = pages
        .iter()
        .map(|p| rows_height(&p.rows))
        .fold(0.0, f32::max);
    let keys_h = if keys > 0 { PAGE_ROW_H } else { 0.0 };
    let graph_h = if draws_wave(info) {
        GRAPH_H + SHAPE_ROW_H
    } else {
        0.0
    };
    (width, 14.0 + graph_h + keys_h + rows_h + 8.0)
}

/// The window's size in points.
pub fn size(info: &NativeFxInfo, _instrument: bool, pages: &[Page]) -> (u32, u32) {
    let (w, h) = body_size(info, pages);
    (
        w.ceil() as u32,
        (gear::HEADER_H + h + gear::FOOT_H).ceil() as u32,
    )
}

/// Lays `rows` out from `top_left`; returns the changes (as the program's `draw_rows`).
fn draw_rows(
    ui: &mut Ui,
    top_left: Pos2,
    width: f32,
    id: egui::Id,
    rows: &[Row],
    controls: &[Control],
) -> Vec<Change> {
    let p = ui.painter().clone();
    let mut out = Vec::new();
    let mut y = top_left.y;
    for row in rows {
        let labelled = row.iter().any(|s| !s.label.is_empty());
        let cells: f32 = row.iter().map(|s| s.params.len() as f32).sum::<f32>()
            + (row.len() as f32 - 1.0) * 0.25;
        let mut x = top_left.x + (width - cells * CELL_W) / 2.0;
        for s in row {
            let w = s.params.len() as f32 * CELL_W;
            if !s.label.is_empty() {
                gear::bracket(&p, pos2(x + 6.0, y + 8.0), w - 12.0, s.label);
            }
            let top = y + if labelled { 22.0 } else { 0.0 };
            out.extend(gear::control_grid(
                ui,
                pos2(x, top),
                s.params.len(),
                id,
                controls,
                &s.params,
                46.0,
            ));
            x += w + CELL_W * 0.25;
        }
        y += CELL_H + 10.0 + if labelled { 22.0 } else { 0.0 };
    }
    out
}

/// The point and its value (-100 to 100) under `pos` on a display `glass` showing `length`
/// points.
fn point_at(glass: Rect, pos: Pos2, length: usize) -> (usize, i32) {
    let x = ((pos.x - glass.left()) / glass.width().max(1.0)).clamp(0.0, 0.999_999);
    let y = ((glass.center().y - pos.y) / (glass.height() / 2.0).max(1.0)).clamp(-1.0, 1.0);
    ((x * length as f32) as usize, (y * 100.0).round() as i32)
}

/// Wave Draw's display (as the program's `wave_graph`): the cycle's points, drawn by dragging
/// across it, and the keys that draw a whole shape. Returns the points that changed, as
/// (parameter, value).
fn wave_graph(ui: &mut Ui, rect: Rect, id: egui::Id, vals: &[i32]) -> Vec<(usize, i32)> {
    use wave_draw as wd;
    let mut changed = Vec::new();
    let length = (vals[wd::LENGTH].max(0) as usize).clamp(4, wd::POINTS);
    let screen = Rect::from_min_size(rect.min, vec2(rect.width(), GRAPH_H - 10.0));
    let p = ui.painter().clone();
    let glass = gear::lcd(&p, screen);
    let resp = ui.interact(glass, id.with("wave"), Sense::click_and_drag());
    if let Some(pos) = resp
        .interact_pointer_pos()
        .filter(|_| resp.is_pointer_button_down_on())
    {
        let (i, v) = point_at(glass, pos, length);
        if vals[wd::WAVE + i] != v {
            changed.push((wd::WAVE + i, v));
        }
    }
    let faint = egui::Stroke::new(1.0_f32, gear::alpha(gear::LCD_INK, 0.12));
    p.line_segment([glass.left_center(), glass.right_center()], faint);
    let step = glass.width() / length as f32;
    let y_of = |v: i32| glass.center().y - v as f32 / 100.0 * (glass.height() / 2.0 - 4.0);
    let mut line = Vec::with_capacity(length * 2);
    for i in 0..length {
        let v = changed
            .iter()
            .find(|(k, _)| *k == wd::WAVE + i)
            .map_or(vals[wd::WAVE + i], |(_, v)| *v);
        line.push(pos2(glass.left() + i as f32 * step, y_of(v)));
        line.push(pos2(glass.left() + (i + 1) as f32 * step, y_of(v)));
    }
    p.with_clip_rect(glass).add(egui::Shape::line(
        line,
        egui::Stroke::new(1.8_f32, gear::LCD_INK),
    ));
    let keys = wd::SHAPES.len() as f32;
    let x0 = rect.left() + (rect.width() - keys * (PAGE_KEY_W + PAGE_KEY_GAP)) / 2.0;
    for (n, name) in wd::SHAPES.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(
                x0 + n as f32 * (PAGE_KEY_W + PAGE_KEY_GAP),
                rect.top() + GRAPH_H,
            ),
            vec2(PAGE_KEY_W, 28.0),
        );
        let key = gear::key(
            ui,
            r,
            id.with(("shape", n)),
            &name.to_uppercase(),
            Cap::Bone,
            LedAt::None,
            false,
            false,
        );
        if key.clicked() {
            changed.clear();
            for i in 0..wd::POINTS {
                let v = wd::shape(name, i);
                if vals[wd::WAVE + i] != v {
                    changed.push((wd::WAVE + i, v));
                }
            }
        }
    }
    changed
}

/// What the window keeps between frames.
pub struct State {
    page: usize,
    /// The parameters being turned (a gesture the host has been told of).
    turning: Vec<bool>,
    /// FM6: the voices of the .syx file opened last.
    bank: Bank,
    /// FM6: a bank read on the file dialog's thread, taken by the next frame.
    incoming: Arc<Mutex<Option<Bank>>>,
    /// FM6: the file dialog is open.
    choosing: Arc<std::sync::atomic::AtomicBool>,
}

impl State {
    pub fn new(params: usize) -> Self {
        State {
            page: 0,
            turning: vec![false; params],
            bank: Vec::new(),
            incoming: Arc::new(Mutex::new(None)),
            choosing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Opens the file dialog on a thread of its own, so the host's window thread keeps going
    /// (rfd runs the panel on the main thread's queue); the bank arrives in `incoming`.
    fn choose_bank(&self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        if self.choosing.swap(true, Ordering::SeqCst) {
            return;
        }
        let (incoming, choosing, ctx) = (self.incoming.clone(), self.choosing.clone(), ctx.clone());
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .add_filter("SysEx", &["syx"])
                .pick_file();
            if let Some(path) = file {
                let bank = read_bank(&path);
                if let Ok(mut slot) = incoming.lock() {
                    *slot = Some(bank);
                }
            }
            choosing.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }
}

/// Paints the faceplate's header across `rect`: "Pro Wave", the model code and the name, and
/// the presets' menu at the right when there are presets. Returns the preset picked.
fn header(ui: &mut Ui, rect: Rect, id: egui::Id, info: &NativeFxInfo) -> Option<usize> {
    let p = ui.painter().clone();
    let cy = rect.top() + 32.0;
    gear::silk(
        &p,
        pos2(rect.left() + 26.0, rect.top() + 14.0),
        Align2::LEFT_CENTER,
        "Pro Wave",
        9.5,
        gear::PRINT,
    );
    let m = gear::text_at(
        &p,
        pos2(rect.left() + 25.0, cy + 6.0),
        Align2::LEFT_CENTER,
        &model_code(info.id).to_uppercase(),
        gear::silk_heavy(28.0),
        gear::INK,
        0.04,
    );
    let sq = Rect::from_center_size(pos2(m.right() + 13.0, cy + 5.0), vec2(9.0, 9.0));
    p.rect_filled(sq, 1.0, gear::ORANGE);
    let name_x = sq.right() + 9.0;
    let mut right = rect.right() - 22.0;
    let mut picked = None;
    if !info.presets.is_empty() {
        let names: Vec<&str> = info.presets.iter().map(|pr| pr.name).collect();
        let r = Rect::from_min_size(pos2(right - 190.0, cy - 13.0), vec2(190.0, 28.0));
        // The menu shows "PRESET" until one is picked (a preset sets some parameters only).
        let mut all = vec!["Preset"];
        all.extend(names.iter().copied());
        if let Some(i) = gear::choice_field(ui, r, id.with("presets"), &all, 0, true) {
            picked = i.checked_sub(1);
        }
        right = r.left() - 12.0;
    }
    let nm = gear::fit(
        &p,
        &info.name.to_uppercase(),
        14.0,
        (right - name_x).max(20.0),
    );
    gear::text_at(
        &p,
        pos2(name_x, cy + 6.5),
        Align2::LEFT_CENTER,
        &nm,
        gear::silk_font(14.5),
        gear::INK2,
        0.15,
    );
    gear::seam_h(
        &p,
        rect.left() + 1.0,
        rect.right() - 1.0,
        rect.top() + gear::HEADER_H - 2.0,
    );
    picked
}

/// A change the window makes to a parameter: a gesture's start, a value, its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Begin(usize),
    Set(usize, i32),
    End(usize),
}

/// Sets parameter `k` to `v` as one gesture.
fn set_once(out: &mut Vec<Edit>, k: usize, v: i32) {
    out.extend([Edit::Begin(k), Edit::Set(k, v), Edit::End(k)]);
}

/// One frame of the window for `params`' plug-in, its parameters at `vals`; returns the edits.
pub fn draw(ctx: &egui::Context, params: &PlugParams, st: &mut State, vals: &[i32]) -> Vec<Edit> {
    let mut out = Vec::new();
    let info = params.info;
    let pages = &params.pages;
    let n = info.params.len();
    let page = st.page.min(pages.len().saturating_sub(1));
    let controls: Vec<Control> = (0..n)
        .map(|k| {
            let param = &info.params[k];
            let span = (param.max - param.min).max(1e-6);
            let v = vals[k] as f32;
            Control {
                label: label(info, params.instrument, &pages[page], k),
                norm: ((v - param.min) / span).clamp(0.0, 1.0),
                default: ((param.default - param.min) / span).clamp(0.0, 1.0),
                text: crate::params::value_text(info, k, vals[k]),
                choices: crate::params::choices(info, k),
                choice: (v - param.min).round().max(0.0) as usize,
                automated: None,
                enabled: true,
            }
        })
        .collect();
    let (width, body_h) = body_size(info, pages);
    let id = egui::Id::new("prowave-plug");
    if let Some(bank) = st.incoming.lock().ok().and_then(|mut s| s.take()) {
        st.bank = bank;
        // The voices' menu opens by itself on a bank just read.
        if !st.bank.is_empty() {
            ctx.memory_mut(|m| m.open_popup(id.with("syx-menu")));
        }
    }
    let frame = egui::Frame::NONE.fill(gear::BONE);
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let full = ui.max_rect();
        hw::chassis(ui.painter(), full);
        let (hr, _) = ui.allocate_exact_size(vec2(width, gear::HEADER_H), Sense::hover());
        if let Some(i) = header(ui, hr, id, info) {
            for (k, v) in info.presets[i].values {
                if *k < n {
                    set_once(&mut out, *k, *v as i32);
                }
            }
        }
        let (rect, _) = ui.allocate_exact_size(vec2(width, body_h), Sense::hover());
        let mut y = rect.top() + 14.0;
        if draws_wave(info) {
            let area = Rect::from_min_size(
                pos2(rect.left() + PAD, y),
                vec2(width - PAD * 2.0, GRAPH_H + SHAPE_ROW_H),
            );
            for (k, v) in wave_graph(ui, area, id, vals) {
                set_once(&mut out, k, v);
            }
            y += GRAPH_H + SHAPE_ROW_H;
        }
        let keys = top_keys(info, pages);
        if keys > 0 {
            let x0 = rect.left() + (width - keys as f32 * (PAGE_KEY_W + PAGE_KEY_GAP)) / 2.0;
            let pages_shown = if page_keys(pages) > 0 { pages.len() } else { 0 };
            for (b, pg) in pages.iter().enumerate().take(pages_shown) {
                let r = Rect::from_min_size(
                    pos2(x0 + b as f32 * (PAGE_KEY_W + PAGE_KEY_GAP), y),
                    vec2(PAGE_KEY_W, 28.0),
                );
                let on = b == page;
                let key = gear::key(
                    ui,
                    r,
                    id.with(("page", b)),
                    &pg.key,
                    Cap::Bone,
                    LedAt::Left,
                    on,
                    on,
                );
                if key.clicked() {
                    st.page = b;
                }
            }
            if loads_syx(info) {
                // The SYX key: a .syx bank's voices, and the voices of the file opened last.
                let r = Rect::from_min_size(
                    pos2(x0 + pages_shown as f32 * (PAGE_KEY_W + PAGE_KEY_GAP), y),
                    vec2(PAGE_KEY_W, 28.0),
                );
                let key = gear::key(
                    ui,
                    r,
                    id.with("syx"),
                    "SYX",
                    Cap::Dark,
                    LedAt::None,
                    false,
                    false,
                );
                let menu = id.with("syx-menu");
                if key.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(menu));
                }
                let mut picked = None;
                let mut choose = false;
                egui::popup::popup_below_widget(
                    ui,
                    menu,
                    &key,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
                        ui.set_min_width(170.0);
                        if ui.button("Load .syx…").clicked() {
                            choose = true;
                            ui.memory_mut(|m| m.close_popup());
                        }
                        if !st.bank.is_empty() {
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .max_height(320.0)
                                .show(ui, |ui| {
                                    for (i, (name, _)) in st.bank.iter().enumerate() {
                                        if ui.button(format!("{:02} {name}", i + 1)).clicked() {
                                            picked = Some(i);
                                            ui.memory_mut(|m| m.close_popup());
                                        }
                                    }
                                });
                        }
                    },
                );
                if choose {
                    st.choose_bank(ui.ctx());
                }
                if let Some(i) = picked {
                    out.extend(take_voice(vals, &st.bank[i].1));
                }
            }
            y += PAGE_ROW_H;
        }
        let rows = &pages[page].rows;
        let changes = draw_rows(
            ui,
            pos2(rect.left() + PAD, y),
            width - PAD * 2.0,
            id,
            rows,
            &controls,
        );
        for ch in changes {
            let Some(param) = info.params.get(ch.index) else {
                continue;
            };
            let v = match ch.choice {
                Some(i) => param.min as i32 + i as i32,
                None => (param.min + (param.max - param.min) * ch.norm).round() as i32,
            };
            if ch.choice.is_some() {
                set_once(&mut out, ch.index, v);
                continue;
            }
            if !st.turning[ch.index] {
                out.push(Edit::Begin(ch.index));
                st.turning[ch.index] = true;
            }
            out.push(Edit::Set(ch.index, v));
            if ch.ended {
                out.push(Edit::End(ch.index));
                st.turning[ch.index] = false;
            }
        }
        // The foot: RESET puts every parameter back to its default.
        let (fr, _) = ui.allocate_exact_size(vec2(width, gear::FOOT_H), Sense::hover());
        let pnt = ui.painter().clone();
        gear::seam_h(&pnt, fr.left() + 1.0, fr.right() - 1.0, fr.top());
        let cy = fr.top() + fr.height() / 2.0 - 2.0;
        let r = Rect::from_min_size(pos2(fr.right() - 26.0 - 84.0, cy - 13.0), vec2(84.0, 26.0));
        let reset = gear::key(
            ui,
            r,
            id.with("reset"),
            "Reset",
            Cap::Bone,
            LedAt::None,
            false,
            false,
        );
        if reset.clicked() {
            for (k, p) in info.params.iter().enumerate() {
                set_once(&mut out, k, p.default as i32);
            }
        }
        gear::corner_screws(ui.painter(), full);
    });
    out
}

/// The editor for `params`' plug-in.
pub fn create(params: Arc<PlugParams>) -> Option<Box<dyn Editor>> {
    let state: Arc<EguiState> = params.editor_state.clone();
    create_egui_editor(
        state,
        State::new(params.values.len()),
        |ctx, _| theme::apply(ctx),
        move |ctx, setter, st| {
            let vals: Vec<i32> = params.values.iter().map(|p| p.value()).collect();
            for e in draw(ctx, &params, st, &vals) {
                match e {
                    Edit::Begin(k) => setter.begin_set_parameter(&params.values[k]),
                    Edit::Set(k, v) => setter.set_parameter(&params.values[k], v),
                    Edit::End(k) => setter.end_set_parameter(&params.values[k]),
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every plug-in's window draws every page without a panic, a press on each knob's
    /// centre turns into a whole gesture, and RESET sets every parameter.
    #[test]
    fn every_window_draws_every_page() {
        let all = prowave_engine::native_instruments()
            .iter()
            .map(|i| (i, true))
            .chain(prowave_engine::native_effects().iter().map(|i| (i, false)));
        for (info, instrument) in all {
            let params = PlugParams::new(info, instrument);
            let (w, h) = size(info, instrument, &params.pages);
            let vals: Vec<i32> = info.params.iter().map(|p| p.default as i32).collect();
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut st = State::new(info.params.len());
            let screen = Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32));
            let frame = |ctx: &egui::Context, st: &mut State, events: Vec<egui::Event>| {
                let input = egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                };
                let mut edits = Vec::new();
                let out = ctx.run(input, |ctx| edits = draw(ctx, &params, st, &vals));
                assert!(!out.shapes.is_empty(), "{} draws nothing", info.id);
                edits
            };
            for page in 0..params.pages.len() {
                st.page = page;
                frame(&ctx, &mut st, Vec::new());
                frame(&ctx, &mut st, Vec::new());
            }
            // RESET, bottom right.
            st.page = 0;
            let at = pos2(w as f32 - 26.0 - 42.0, h as f32 - gear::FOOT_H / 2.0 - 2.0);
            let click = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(&ctx, &mut st, vec![egui::Event::PointerMoved(at)]);
            frame(&ctx, &mut st, vec![click(true)]);
            let edits = frame(&ctx, &mut st, vec![click(false)]);
            let sets = edits.iter().filter(|e| matches!(e, Edit::Set(..))).count();
            assert_eq!(sets, info.params.len(), "{}: RESET", info.id);
        }
    }

    /// FM6's SYX key: a .syx bank is read as the program reads it, the key opens a menu with
    /// the bank's voices, and picking one sets the voice's parameters (0 to 144) as gestures and
    /// leaves the rest.
    #[test]
    fn fm6_takes_a_voice_from_a_bank() {
        let info = prowave_engine::native_instrument_info(fm6::ID).expect("fm6");
        // A bank made as the engine's own test makes one.
        let mut file = vec![0xf0, 0x43, 0x00, 0x09, 0x20, 0x00];
        for v in 0..32u32 {
            for k in 0..128u32 {
                file.push(((v * 7 + k * 13) % 100) as u8);
            }
        }
        file.extend([0x00, 0xf7]);
        let dir = std::env::temp_dir().join(format!("prowave-plug-syx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bank.syx");
        std::fs::write(&path, &file).unwrap();
        let bank = read_bank(&path);
        assert_eq!(bank.len(), 32);
        assert!(read_bank(&dir.join("none.syx")).is_empty());
        std::fs::remove_dir_all(&dir).ok();

        let params = PlugParams::new(info, true);
        let (w, h) = size(info, true, &params.pages);
        let vals: Vec<i32> = info.params.iter().map(|p| p.default as i32).collect();
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut st = State::new(info.params.len());
        // As if the file dialog's thread had read it.
        *st.incoming.lock().unwrap() = Some(bank.clone());
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(w as f32, h as f32));
        let frame = |st: &mut State, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut edits = Vec::new();
            let out = ctx.run(input, |ctx| edits = draw(ctx, &params, st, &vals));
            (edits, out)
        };
        let (_, out) = frame(&mut st, Vec::new());
        assert_eq!(st.bank.len(), 32);
        // The voices' menu opened by itself and is drawn (its rows are text shapes).
        assert!(ctx.memory(|m| m.is_popup_open(egui::Id::new("prowave-plug").with("syx-menu"))));
        assert!(!out.shapes.is_empty());
        // Picking goes through take_voice: every voice parameter that differs, one gesture each.
        let edits = take_voice(&vals, &bank[2].1);
        let changed = bank[2]
            .1
            .iter()
            .zip(&vals)
            .filter(|(n, o)| n.round() as i32 != **o)
            .count();
        assert_eq!(edits.len(), changed * 3);
        assert!(edits.iter().all(|e| match e {
            Edit::Begin(k) | Edit::End(k) | Edit::Set(k, _) => *k < fm6::VOICE_PARAMS,
        }));
    }
}
