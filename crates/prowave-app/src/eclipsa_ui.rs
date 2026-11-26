//! Eclipsa Audio Spatial Renderer, 3D Object Panner, and Surround Panner windows.
//!
//! Layout and controls modeled after Logic's spatial workflow (`3D Object Panner`,
//! `Surround Panner`, and `Eclipsa Audio` Master Renderer) using `prowave_engine::eclipsa`.

use crate::model::{EclipsaTrackConfig, EclipsaTrackRole, Project};
use crate::UiState;
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui,
};
use prowave_engine::eclipsa::{
    compute_surround_gains, BinauralMode, MonitoringFormat, PannerMode, BED_SPEAKER_NAMES,
};

const BG_NAVY: Color32 = Color32::from_rgb(0x11, 0x1a, 0x24);
const PANEL_NAVY: Color32 = Color32::from_rgb(0x16, 0x21, 0x2c);
fn grid_line() -> Color32 {
    Color32::from_rgba_unmultiplied(0x88, 0xa2, 0xbc, 38)
}
const ACCENT_GREEN: Color32 = Color32::from_rgb(0x7c, 0xc8, 0x8e);
const ACCENT_BLUE: Color32 = Color32::from_rgb(0x5b, 0x9c, 0xd6);
const TEXT_MAIN: Color32 = Color32::from_rgb(0xe8, 0xee, 0xf5);
const TEXT_DIM: Color32 = Color32::from_rgb(0x8a, 0x9b, 0xad);
const PILL_ACTIVE: Color32 = Color32::from_rgb(0x12, 0x69, 0xcc);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CameraPreset {
    Angle,
    Left,
    Top,
    Right,
    Back,
    Custom,
}

impl CameraPreset {
    const ALL: [Self; 6] = [
        Self::Angle,
        Self::Left,
        Self::Top,
        Self::Right,
        Self::Back,
        Self::Custom,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Angle => "Angle",
            Self::Left => "Left",
            Self::Top => "Top",
            Self::Right => "Right",
            Self::Back => "Back",
            Self::Custom => "Custom",
        }
    }

    fn angles(self) -> (f32, f32) {
        match self {
            Self::Angle => (0.55, 0.36),
            Self::Left => (-1.57, 0.12),
            Self::Top => (0.0, 1.45),
            Self::Right => (1.57, 0.12),
            Self::Back => (3.14, 0.16),
            Self::Custom => (0.75, 0.45),
        }
    }
}

/// Renders the Eclipsa Audio Master Renderer window and Track Spatial Panner window.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    show_renderer(ctx, p, s);
    show_track_panner(ctx, p, s);
}

fn show_renderer(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    if !s.show_eclipsa_renderer {
        return;
    }
    let mut open = true;
    let frame = egui::Frame::NONE
        .fill(BG_NAVY)
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x28, 0x36, 0x46)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12));

    crate::liquid_glass::glass_window("Master · Eclipsa Audio")
        .id(egui::Id::unique("eclipsa_renderer_window"))
        .open(&mut open)
        .resizable(false)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(590.0);
            // Top bar: Power button + Monitoring Format
            ui.horizontal(|ui| {
                let pwr_col = if p.eclipsa_master.enabled {
                    PILL_ACTIVE
                } else {
                    Color32::from_rgb(0x2b, 0x35, 0x42)
                };
                if ui
                    .add(
                        egui::Button::new(RichText::new("⏻").size(13.0).color(Color32::WHITE))
                            .fill(pwr_col)
                            .corner_radius(CornerRadius::same(12))
                            .min_size(vec2(24.0, 24.0)),
                    )
                    .clicked()
                {
                    p.eclipsa_master.enabled = !p.eclipsa_master.enabled;
                }
                ui.add_space(16.0);
                ui.label(
                    RichText::new("Monitoring Format:")
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                egui::ComboBox::from_id_salt("eclipsa_monitoring_fmt")
                    .selected_text(p.eclipsa_master.monitoring.label())
                    .show_ui(ui, |ui| {
                        for fmt in MonitoringFormat::ALL {
                            ui.selectable_value(&mut p.eclipsa_master.monitoring, fmt, fmt.label());
                        }
                    });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(RichText::new("Track Panner…").size(11.0))
                        .clicked()
                    {
                        s.show_eclipsa_panner = true;
                    }
                });
            });

            ui.add_space(10.0);

            // Main body: Left 3D room cube + Right Surround Bed & 3D Objects list
            ui.horizontal_top(|ui| {
                // Left: 3D wireframe room + camera presets
                ui.vertical(|ui| {
                    ui.set_width(340.0);
                    let cam_id = egui::Id::unique("eclipsa_cam_preset");
                    let mut preset: CameraPreset = ui
                        .ctx()
                        .data(|d| d.get_temp(cam_id).unwrap_or(CameraPreset::Angle));
                    let yaw_id = egui::Id::unique("eclipsa_cam_yaw");
                    let pitch_id = egui::Id::unique("eclipsa_cam_pitch");
                    let (mut yaw, mut pitch) = ui
                        .ctx()
                        .data(|d| d.get_temp(yaw_id).zip(d.get_temp(pitch_id)))
                        .unwrap_or_else(|| preset.angles());

                    let (room_rect, room_resp) =
                        ui.allocate_exact_size(vec2(336.0, 250.0), Sense::drag());
                    if room_resp.dragged() {
                        let d = room_resp.drag_delta();
                        yaw += d.x * 0.01;
                        pitch = (pitch + d.y * 0.01).clamp(0.05, 1.48);
                        preset = CameraPreset::Custom;
                    }
                    draw_3d_room(ui, room_rect, yaw, pitch, p);

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        for cp in CameraPreset::ALL {
                            let active = preset == cp;
                            let btn = egui::Button::new(
                                RichText::new(cp.label()).size(10.5).color(if active {
                                    Color32::WHITE
                                } else {
                                    TEXT_DIM
                                }),
                            )
                            .fill(if active {
                                Color32::from_rgb(0x3a, 0x48, 0x56)
                            } else {
                                Color32::from_rgb(0x1c, 0x26, 0x32)
                            })
                            .corner_radius(CornerRadius::same(3));
                            if ui.add(btn).clicked() {
                                preset = cp;
                                (yaw, pitch) = cp.angles();
                            }
                        }
                    });
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(cam_id, preset);
                        d.insert_temp(yaw_id, yaw);
                        d.insert_temp(pitch_id, pitch);
                    });
                });

                ui.add_space(10.0);

                // Right: Surround Bed + 3D Objects Binaural Render list
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Surround Bed").size(10.5).color(TEXT_DIM));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new("Binaural Render:")
                                                .size(10.0)
                                                .color(TEXT_DIM),
                                        );
                                    },
                                );
                            });
                            ui.add_space(2.0);
                            for (i, name) in BED_SPEAKER_NAMES.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(*name).size(11.0).color(TEXT_MAIN));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let mode = &mut p.eclipsa_master.bed_binaural[i];
                                            egui::ComboBox::from_id_salt(("bed_bin", i))
                                                .width(56.0)
                                                .selected_text(mode.label())
                                                .show_ui(ui, |ui| {
                                                    for m in BinauralMode::ALL {
                                                        ui.selectable_value(mode, m, m.label());
                                                    }
                                                });
                                        },
                                    );
                                });
                            }

                            ui.add_space(8.0);
                            ui.label(RichText::new("3D Objects:").size(10.5).color(TEXT_DIM));
                            ui.add_space(2.0);
                            let mut obj_count = 0usize;
                            for (ti, trk) in p.tracks.iter().enumerate() {
                                let cfg = p.eclipsa_tracks.entry(ti).or_default();
                                if cfg.role != EclipsaTrackRole::Object3D {
                                    continue;
                                }
                                obj_count += 1;
                                ui.horizontal(|ui| {
                                    if ui
                                        .selectable_label(
                                            p.selected_track == ti,
                                            RichText::new(&trk.name).size(11.0).color(TEXT_MAIN),
                                        )
                                        .clicked()
                                    {
                                        p.selected_track = ti;
                                        s.show_eclipsa_panner = true;
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            egui::ComboBox::from_id_salt(("obj_bin", ti))
                                                .width(56.0)
                                                .selected_text(cfg.object.binaural.label())
                                                .show_ui(ui, |ui| {
                                                    for m in BinauralMode::ALL {
                                                        ui.selectable_value(
                                                            &mut cfg.object.binaural,
                                                            m,
                                                            m.label(),
                                                        );
                                                    }
                                                });
                                        },
                                    );
                                });
                            }
                            if obj_count == 0 {
                                ui.label(
                                    RichText::new("No 3D Object tracks yet")
                                        .size(10.5)
                                        .color(TEXT_DIM),
                                );
                            }
                        });
                });
            });

            ui.add_space(6.0);
            ui.separator();
            let obj_total = p
                .eclipsa_tracks
                .values()
                .filter(|c| c.role == EclipsaTrackRole::Object3D)
                .count();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Eclipsa Audio").size(12.0).color(TEXT_MAIN));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("Input Object Channels: {} of 118", obj_total * 2))
                            .size(10.5)
                            .color(TEXT_DIM),
                    );
                });
            });
        });

    if !open {
        s.show_eclipsa_renderer = false;
    }
}

fn show_track_panner(ctx: &egui::Context, p: &mut Project, s: &mut UiState) {
    if !s.show_eclipsa_panner {
        return;
    }
    let ti = p.selected_track.min(p.tracks.len().saturating_sub(1));
    let track_name = p
        .tracks
        .get(ti)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "Track".to_string());
    let cfg = p.eclipsa_tracks.entry(ti).or_default().clone();
    let mut edited = cfg.clone();
    let mut open = true;

    let frame = egui::Frame::NONE
        .fill(BG_NAVY)
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x28, 0x36, 0x46)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12));

    crate::liquid_glass::glass_window(format!("{track_name} · Spatial Panner"))
        .id(egui::Id::unique("eclipsa_track_panner_window"))
        .open(&mut open)
        .resizable(false)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(276.0);
            ui.horizontal(|ui| {
                for (role, lbl) in [
                    (EclipsaTrackRole::Object3D, "3D Object Panner"),
                    (EclipsaTrackRole::SurroundBed, "Surround Panner"),
                ] {
                    let active = edited.role == role;
                    if ui
                        .add(
                            egui::Button::new(RichText::new(lbl).size(10.5).color(if active {
                                Color32::WHITE
                            } else {
                                TEXT_DIM
                            }))
                            .fill(if active { PILL_ACTIVE } else { PANEL_NAVY })
                            .corner_radius(CornerRadius::same(4)),
                        )
                        .clicked()
                    {
                        edited.role = role;
                    }
                }
            });

            ui.add_space(8.0);
            match edited.role {
                EclipsaTrackRole::Object3D | EclipsaTrackRole::Off => {
                    draw_3d_object_panner(ui, &mut edited);
                }
                EclipsaTrackRole::SurroundBed => {
                    draw_surround_panner(ui, &mut edited);
                }
            }
        });

    if edited != cfg {
        p.eclipsa_tracks.insert(ti, edited);
    }
    if !open {
        s.show_eclipsa_panner = false;
    }
}

fn draw_3d_object_panner(ui: &mut Ui, cfg: &mut EclipsaTrackConfig) {
    let o = &mut cfg.object;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Left/Right: {:+.3}", o.left_right))
                .size(10.0)
                .color(TEXT_DIM),
        );
        ui.label(
            RichText::new(format!("Back/Front: {:+.3}", o.back_front))
                .size(10.0)
                .color(TEXT_DIM),
        );
        ui.label(
            RichText::new(format!("El: {:+.2}", o.elevation))
                .size(10.0)
                .color(TEXT_DIM),
        );
    });
    ui.horizontal(|ui| {
        ui.label(RichText::new("Size:").size(10.0).color(TEXT_DIM));
        ui.add(
            egui::DragValue::new(&mut o.size)
                .range(0.0..=1.0)
                .speed(0.01),
        );
        ui.label(RichText::new("Spread:").size(10.0).color(TEXT_DIM));
        ui.add(
            egui::DragValue::new(&mut o.spread_deg)
                .range(0.0..=180.0)
                .speed(1.0)
                .suffix("°"),
        );
    });
    ui.add_space(6.0);

    // Top-down 4x4 XY grid (Front / Back / Left / Right)
    let (xy_rect, xy_resp) = ui.allocate_exact_size(vec2(268.0, 200.0), Sense::click_and_drag());
    let grid = xy_rect.shrink2(vec2(26.0, 18.0));
    if (xy_resp.dragged() || xy_resp.clicked()) && xy_resp.interact_pointer_pos().is_some() {
        if let Some(pos) = xy_resp.interact_pointer_pos() {
            o.left_right = (((pos.x - grid.left()) / grid.width()) * 2.0 - 1.0).clamp(-1.0, 1.0);
            o.back_front = (1.0 - ((pos.y - grid.top()) / grid.height()) * 2.0).clamp(-1.0, 1.0);
        }
    }
    let g = ui.painter_at(xy_rect);
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let x = grid.left() + grid.width() * t;
        let y = grid.top() + grid.height() * t;
        g.line_segment(
            [pos2(x, grid.top()), pos2(x, grid.bottom())],
            Stroke::new(1.0, grid_line()),
        );
        g.line_segment(
            [pos2(grid.left(), y), pos2(grid.right(), y)],
            Stroke::new(1.0, grid_line()),
        );
    }
    g.text(
        pos2(grid.center().x, xy_rect.top() + 8.0),
        Align2::CENTER_CENTER,
        "Front",
        FontId::proportional(10.0),
        TEXT_DIM,
    );
    g.text(
        pos2(grid.center().x, xy_rect.bottom() - 8.0),
        Align2::CENTER_CENTER,
        "Back",
        FontId::proportional(10.0),
        TEXT_DIM,
    );
    g.text(
        pos2(xy_rect.left() + 12.0, grid.center().y),
        Align2::CENTER_CENTER,
        "Left",
        FontId::proportional(10.0),
        TEXT_DIM,
    );
    g.text(
        pos2(xy_rect.right() - 12.0, grid.center().y),
        Align2::CENTER_CENTER,
        "Right",
        FontId::proportional(10.0),
        TEXT_DIM,
    );

    // Listener head silhouette at center
    g.circle_filled(grid.center(), 9.0, Color32::from_rgb(0x4a, 0x56, 0x64));
    g.circle_filled(
        grid.center() + vec2(0.0, -7.0),
        3.0,
        Color32::from_rgb(0x63, 0x71, 0x82),
    );

    let px = grid.left() + (o.left_right * 0.5 + 0.5) * grid.width();
    let py = grid.top() + (0.5 - o.back_front * 0.5) * grid.height();
    let spread_px = (o.spread_deg / 180.0) * (grid.width() * 0.25);
    g.circle_filled(
        pos2((px - spread_px).clamp(grid.left(), grid.right()), py),
        7.0,
        ACCENT_BLUE,
    );
    g.circle_filled(
        pos2((px + spread_px).clamp(grid.left(), grid.right()), py),
        7.0,
        ACCENT_GREEN,
    );
    g.circle_filled(pos2(px, py), 4.5, Color32::from_rgb(0xb4, 0xf2, 0xc2));

    ui.add_space(6.0);

    // Bottom 4x2 elevation grid (Top / Ear Level)
    let (el_rect, el_resp) = ui.allocate_exact_size(vec2(268.0, 96.0), Sense::click_and_drag());
    let el_grid = el_rect.shrink2(vec2(26.0, 10.0));
    if (el_resp.dragged() || el_resp.clicked()) && el_resp.interact_pointer_pos().is_some() {
        if let Some(pos) = el_resp.interact_pointer_pos() {
            o.elevation = (1.0 - (pos.y - el_grid.top()) / el_grid.height()).clamp(0.0, 1.0);
        }
    }
    let ge = ui.painter_at(el_rect);
    for i in 0..=4 {
        let x = el_grid.left() + el_grid.width() * (i as f32 / 4.0);
        ge.line_segment(
            [pos2(x, el_grid.top()), pos2(x, el_grid.bottom())],
            Stroke::new(1.0, grid_line()),
        );
    }
    for j in 0..=2 {
        let y = el_grid.top() + el_grid.height() * (j as f32 / 2.0);
        ge.line_segment(
            [pos2(el_grid.left(), y), pos2(el_grid.right(), y)],
            Stroke::new(1.0, grid_line()),
        );
    }
    let ey = el_grid.bottom() - o.elevation.clamp(0.0, 1.0) * el_grid.height();
    ge.line_segment(
        [pos2(el_grid.left(), ey), pos2(el_grid.right(), ey)],
        Stroke::new(1.5, ACCENT_GREEN),
    );
    ge.circle_filled(
        pos2(px.clamp(el_grid.left(), el_grid.right()), ey),
        5.0,
        ACCENT_GREEN,
    );
    ge.text(
        pos2(el_rect.right() - 2.0, el_grid.top() + 6.0),
        Align2::RIGHT_CENTER,
        "Top",
        FontId::proportional(9.5),
        TEXT_DIM,
    );
    ge.text(
        pos2(el_rect.right() - 2.0, el_grid.bottom() - 6.0),
        Align2::RIGHT_CENTER,
        "Ear Level",
        FontId::proportional(9.5),
        TEXT_DIM,
    );
}

fn draw_surround_panner(ui: &mut Ui, cfg: &mut EclipsaTrackConfig) {
    let s = &mut cfg.surround;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Angle: {:.1}°", s.angle_deg))
                .size(10.0)
                .color(TEXT_DIM),
        );
        ui.label(
            RichText::new(format!("Diversity: {:.2}", s.diversity))
                .size(10.0)
                .color(TEXT_DIM),
        );
        ui.label(
            RichText::new(format!("Spread: +{:.0}°", s.spread_deg))
                .size(10.0)
                .color(TEXT_DIM),
        );
    });
    ui.add_space(4.0);

    let (ring_rect, ring_resp) =
        ui.allocate_exact_size(vec2(268.0, 210.0), Sense::click_and_drag());
    let center = ring_rect.center();
    let radius = 78.0;

    if (ring_resp.dragged() || ring_resp.clicked()) && ring_resp.interact_pointer_pos().is_some() {
        if let Some(pos) = ring_resp.interact_pointer_pos() {
            let dx = pos.x - center.x;
            let dy = center.y - pos.y;
            s.angle_deg = dx.atan2(dy).to_degrees();
            s.diversity = (1.0 - (dx.hypot(dy) / radius).clamp(0.0, 1.0)).clamp(0.0, 1.0);
        }
    }

    let g = ui.painter_at(ring_rect);
    for r_scale in [0.35, 0.68, 1.0] {
        g.circle_stroke(center, radius * r_scale, Stroke::new(1.0, grid_line()));
    }
    g.line_segment(
        [
            pos2(center.x - radius, center.y),
            pos2(center.x + radius, center.y),
        ],
        Stroke::new(1.0, grid_line()),
    );
    g.line_segment(
        [
            pos2(center.x, center.y - radius),
            pos2(center.x, center.y + radius),
        ],
        Stroke::new(1.0, grid_line()),
    );

    // 8 speaker dots around ring + live gain highlights
    let gains = compute_surround_gains(s);
    let spk_angles_deg = [-30.0_f32, 30.0, 0.0, 180.0, -90.0, 90.0, -135.0, 135.0];
    for (i, deg) in spk_angles_deg.iter().enumerate() {
        let a = deg.to_radians();
        let pt = pos2(
            center.x + a.sin() * (radius + 14.0),
            center.y - a.cos() * (radius + 14.0),
        );
        let active = gains[i] > 0.25;
        g.circle_filled(pt, 4.5, if active { ACCENT_GREEN } else { ACCENT_BLUE });
    }

    // Active arc + draggable puck
    let dist = (1.0 - s.diversity) * radius;
    let rad = s.angle_deg.to_radians();
    let puck = pos2(center.x + rad.sin() * dist, center.y - rad.cos() * dist);
    g.circle_filled(puck, 6.0, ACCENT_GREEN);

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut s.mode, PannerMode::Planar, "Planar");
        ui.selectable_value(&mut s.mode, PannerMode::Spherical, "Spherical");
    });
    ui.add_space(4.0);
    ui.add(
        egui::Slider::new(&mut s.center_db, -60.0..=0.0)
            .text("Center Level")
            .suffix(" dB"),
    );
    ui.add(
        egui::Slider::new(&mut s.lfe_db, -60.0..=0.0)
            .text("LFE Level")
            .suffix(" dB"),
    );
}

fn draw_3d_room(ui: &Ui, rect: Rect, yaw: f32, pitch: f32, p: &Project) {
    let g = ui.painter_at(rect);
    g.rect_filled(rect, 6.0, Color32::from_rgb(0x13, 0x1d, 0x28));
    g.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, Color32::from_rgb(0x26, 0x35, 0x45)),
        StrokeKind::Inside,
    );

    let center = rect.center() + vec2(0.0, 10.0);
    let scale = 92.0;
    let project = |x: f32, y: f32, z: f32| -> Pos2 {
        let rx = x * yaw.cos() - y * yaw.sin();
        let ry = x * yaw.sin() + y * yaw.cos();
        let py = ry * pitch.cos() - z * pitch.sin();
        pos2(
            center.x + rx * scale,
            center.y - (z * pitch.cos() + ry * pitch.sin()) * scale * 0.85 - py * 0.08,
        )
    };

    // Floor grid (-1..1 in x,y at z=0)
    for i in 0..=4 {
        let t = -1.0 + 0.5 * i as f32;
        g.line_segment(
            [project(t, -1.0, 0.0), project(t, 1.0, 0.0)],
            Stroke::new(1.0, grid_line()),
        );
        g.line_segment(
            [project(-1.0, t, 0.0), project(1.0, t, 0.0)],
            Stroke::new(1.0, grid_line()),
        );
    }
    // Cube edges (z=0 and z=1)
    let edge_col = Color32::from_rgba_unmultiplied(0xa4, 0xb8, 0xcc, 90);
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        g.line_segment(
            [project(a.0, a.1, 0.0), project(b.0, b.1, 0.0)],
            Stroke::new(1.2, edge_col),
        );
        g.line_segment(
            [project(a.0, a.1, 1.0), project(b.0, b.1, 1.0)],
            Stroke::new(1.2, edge_col),
        );
        g.line_segment(
            [project(a.0, a.1, 0.0), project(a.0, a.1, 1.0)],
            Stroke::new(1.2, edge_col),
        );
    }

    // Listener head at floor center
    let head = project(0.0, 0.0, 0.15);
    g.circle_filled(head, 8.0, Color32::from_rgb(0x52, 0x60, 0x70));

    // Live 3D Object spheres
    for (ti, cfg) in &p.eclipsa_tracks {
        if cfg.role != EclipsaTrackRole::Object3D {
            continue;
        }
        let pt = project(
            cfg.object.left_right,
            cfg.object.back_front,
            cfg.object.elevation.clamp(0.0, 1.0),
        );
        let col = if p.selected_track == *ti {
            ACCENT_GREEN
        } else {
            ACCENT_BLUE
        };
        g.add(Shape::circle_filled(pt, 5.5, col));
        g.circle_stroke(pt, 5.5, Stroke::new(1.0, Color32::WHITE));
    }
}
