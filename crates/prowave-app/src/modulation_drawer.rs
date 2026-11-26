//! Dockable Visual Modulation Matrix, LFO / Envelope Follower Drawer, and Animated Modulation Rings.

use egui::{pos2, vec2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, Ui};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModSourceKind {
    Lfo1,
    Lfo2,
    EnvFollower,
    Macro1,
    Macro2,
}

impl ModSourceKind {
    pub const ALL: [Self; 5] = [
        Self::Lfo1,
        Self::Lfo2,
        Self::EnvFollower,
        Self::Macro1,
        Self::Macro2,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lfo1 => "LFO 1",
            Self::Lfo2 => "LFO 2",
            Self::EnvFollower => "Env Follower",
            Self::Macro1 => "Macro 1",
            Self::Macro2 => "Macro 2",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            Self::Lfo1 => Color32::from_rgb(0x4a, 0x9e, 0xff),
            Self::Lfo2 => Color32::from_rgb(0x3a, 0xd1, 0x8a),
            Self::EnvFollower => Color32::from_rgb(0xff, 0x9b, 0x42),
            Self::Macro1 => Color32::from_rgb(0xb0, 0x6c, 0xff),
            Self::Macro2 => Color32::from_rgb(0xff, 0x5c, 0x8d),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModRoute {
    pub source: ModSourceKind,
    pub target_plugin: String,
    pub target_param: String,
    pub depth: f32, // -1.0..=1.0
}

#[derive(Clone, Debug)]
pub struct ModulationDrawerState {
    pub open: bool,
    pub lfo1_hz: f32,
    pub lfo2_hz: f32,
    pub env_attack_ms: f32,
    pub env_release_ms: f32,
    pub macro1: f32,
    pub macro2: f32,
    pub dragging_source: Option<ModSourceKind>,
    pub routes: Vec<ModRoute>,
}

impl Default for ModulationDrawerState {
    fn default() -> Self {
        Self {
            open: false,
            lfo1_hz: 1.5,
            lfo2_hz: 0.4,
            env_attack_ms: 10.0,
            env_release_ms: 120.0,
            macro1: 0.5,
            macro2: 0.5,
            dragging_source: None,
            routes: vec![
                ModRoute {
                    source: ModSourceKind::Lfo1,
                    target_plugin: "Channel EQ".to_string(),
                    target_param: "Mid Freq".to_string(),
                    depth: 0.35,
                },
                ModRoute {
                    source: ModSourceKind::EnvFollower,
                    target_plugin: "Compressor".to_string(),
                    target_param: "Threshold".to_string(),
                    depth: -0.45,
                },
            ],
        }
    }
}

impl ModulationDrawerState {
    pub fn sample_source(&self, kind: ModSourceKind, time_secs: f64) -> f32 {
        match kind {
            ModSourceKind::Lfo1 => 0.5 + 0.5 * ((time_secs as f32 * self.lfo1_hz * TAU).sin()),
            ModSourceKind::Lfo2 => 0.5 + 0.5 * ((time_secs as f32 * self.lfo2_hz * TAU).sin()),
            ModSourceKind::EnvFollower => 0.5 + 0.35 * ((time_secs as f32 * 3.1 * TAU).sin().abs()),
            ModSourceKind::Macro1 => self.macro1,
            ModSourceKind::Macro2 => self.macro2,
        }
    }
}

/// Draws a live animated modulation ring around any knob/parameter rect.
pub fn draw_modulation_ring(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
    value: f32,
) {
    let segments = 24;
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let sweep = value.clamp(0.0, 1.0) * TAU;
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let a = start_angle + sweep * (i as f32 / segments as f32);
        pts.push(pos2(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
}

/// Renders the dockable bottom Modulation Drawer.
pub fn show_drawer(ui: &mut Ui, state: &mut ModulationDrawerState, track_name: &str) {
    if !state.open {
        return;
    }
    let time = ui.ctx().input(|i| i.time);
    ui.ctx().request_repaint();

    egui::Panel::bottom("modulation_drawer")
        .resizable(true)
        .default_size(155.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Modulation Matrix — {track_name}"))
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✕").clicked() {
                        state.open = false;
                    }
                });
            });
            ui.separator();

            ui.horizontal(|ui| {
                // Sources column (drag-and-drop modulators)
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Sources (Drag to Route)")
                            .size(11.0)
                            .color(Color32::GRAY),
                    );
                    ui.horizontal(|ui| {
                        for src in ModSourceKind::ALL {
                            let val = state.sample_source(src, time);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(64.0, 56.0), Sense::click_and_drag());
                            if resp.drag_started() {
                                state.dragging_source = Some(src);
                            }
                            if resp.clicked() {
                                state.routes.push(ModRoute {
                                    source: src,
                                    target_plugin: "DSP / CLAP".to_string(),
                                    target_param: "Cutoff".to_string(),
                                    depth: 0.25,
                                });
                            }
                            let painter = ui.painter();
                            painter.rect_filled(
                                rect,
                                CornerRadius::same(6),
                                Color32::from_rgb(0x1e, 0x22, 0x2b),
                            );
                            let center = pos2(rect.center().x, rect.top() + 22.0);
                            draw_modulation_ring(painter, center, 13.0, src.color(), val);
                            painter.text(
                                pos2(rect.center().x, rect.bottom() - 10.0),
                                egui::Align2::CENTER_CENTER,
                                src.label(),
                                egui::FontId::proportional(10.0),
                                Color32::WHITE,
                            );
                        }
                    });
                });

                ui.separator();

                // Active modulation routes
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Active Modulation Routes")
                            .size(11.0)
                            .color(Color32::GRAY),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(88.0)
                        .show(ui, |ui| {
                            let mut remove_idx = None;
                            for (idx, route) in state.routes.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.colored_label(route.source.color(), route.source.label());
                                    ui.label("→");
                                    ui.label(format!(
                                        "{} :: {}",
                                        route.target_plugin, route.target_param
                                    ));
                                    ui.add(
                                        egui::Slider::new(&mut route.depth, -1.0..=1.0)
                                            .text("Depth"),
                                    );
                                    if ui.small_button("✕").clicked() {
                                        remove_idx = Some(idx);
                                    }
                                });
                            }
                            if let Some(i) = remove_idx {
                                state.routes.remove(i);
                            }
                        });
                });
            });
        });
}
