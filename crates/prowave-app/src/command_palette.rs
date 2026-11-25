//! Fuzzy-search Command Palette (`Ctrl+K` or `:`) wired to `menus.rs` and `xtask status`.
//!
//! Active menu entries execute immediately; unimplemented entries are greyed out with a
//! "Request / Vote" link to GitHub issues.

use crate::menus;
use crate::model::Project;
use crate::UiState;
use egui::{Color32, CornerRadius, Key, RichText, Stroke, Ui};

#[derive(Clone, Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub query: String,
    pub selected_idx: usize,
    pub request_focus: bool,
}

impl CommandPaletteState {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.query.clear();
            self.selected_idx = 0;
            self.request_focus = true;
        }
    }
}

fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q = query.to_ascii_lowercase();
    let c = candidate.to_ascii_lowercase();
    if c.contains(&q) {
        return Some(100 - (c.len() as i32 - q.len() as i32).max(0));
    }
    let mut qi = q.chars().peekable();
    let mut score = 0;
    for ch in c.chars() {
        if qi.peek() == Some(&ch) {
            qi.next();
            score += 10;
        }
    }
    if qi.peek().is_none() {
        Some(score)
    } else {
        None
    }
}

/// Renders the Command Palette modal if open. Returns `Some(menu_label)` if an active command was triggered.
pub fn show(ctx: &egui::Context, state: &mut CommandPaletteState, s: &UiState, p: &Project) -> Option<String> {
    if !state.open {
        return None;
    }

    let mut triggered: Option<String> = None;

    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        state.open = false;
        return None;
    }

    let mut entries: Vec<(&'static str, &'static str, bool, i32)> = Vec::new();
    for &(menu_name, items) in &menus::MENUS {
        for &label in items {
            if label == "-" || label == "Open Recent" || label == "Language" {
                continue;
            }
            if let Some(score) = fuzzy_score(&state.query, label) {
                let is_enabled = menus::enabled(label, p, s);
                entries.push((menu_name, label, is_enabled, score));
            }
        }
    }
    entries.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| b.2.cmp(&a.2)));
    entries.truncate(14);

    if !entries.is_empty() {
        state.selected_idx = state.selected_idx.min(entries.len() - 1);
        if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
            state.selected_idx = (state.selected_idx + 1) % entries.len();
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
            state.selected_idx = (state.selected_idx + entries.len() - 1) % entries.len();
        }
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            let (_, label, is_enabled, _) = entries[state.selected_idx];
            if is_enabled {
                triggered = Some(label.to_string());
                state.open = false;
            }
        }
    }

    egui::Window::new("Command Palette")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
        .fixed_size([520.0, 360.0])
        .frame(
            egui::Frame::window(&ctx.style_of(egui::Theme::Dark))
                .fill(Color32::from_rgb(0x16, 0x18, 0x1e))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3a, 0x3e, 0x4a)))
                .corner_radius(CornerRadius::same(10)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("⌘").color(Color32::from_rgb(0x6b, 0xa8, 0xff)).strong());
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .hint_text("Type a command (Ctrl+K / :) — active & roadmap menu entries...")
                        .desired_width(f32::INFINITY),
                );
                if state.request_focus {
                    resp.request_focus();
                    state.request_focus = false;
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (idx, &(menu_name, label, is_enabled, _)) in entries.iter().enumerate() {
                    let selected = idx == state.selected_idx;
                    let bg = if selected {
                        Color32::from_rgb(0x25, 0x2a, 0x36)
                    } else {
                        Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(bg)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("[{menu_name}]"))
                                        .size(11.0)
                                        .color(Color32::from_rgb(0x7c, 0x82, 0x90)),
                                );
                                if is_enabled {
                                    if ui
                                        .selectable_label(selected, RichText::new(label).color(Color32::WHITE))
                                        .clicked()
                                    {
                                        triggered = Some(label.to_string());
                                        state.open = false;
                                    }
                                } else {
                                    ui.label(
                                        RichText::new(label)
                                            .color(Color32::from_rgb(0x66, 0x6a, 0x74)),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let url = format!(
                                            "https://github.com/aathmaj/douri/issues/new?title=Feature+Request:+{}",
                                            label.replace(' ', "+")
                                        );
                                        ui.hyperlink_to(
                                            RichText::new("Request / Vote")
                                                .size(10.5)
                                                .color(Color32::from_rgb(0x5b, 0x9b, 0xd5)),
                                            url,
                                        );
                                    });
                                }
                            });
                        });
                }
            });
        });

    triggered
}
