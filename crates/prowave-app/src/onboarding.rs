//! First-launch Onboarding Screen ("Welcome to Dōri").
//! Provides user choice between Complete Mode and Simplified Mode.

use egui::{pos2, vec2, Align2, Color32, FontId, Image, Rect, Sense, Stroke};

pub const ONBOARDING_FILE: &str = "onboarding.txt";
pub const MODE_FILE: &str = "mode.txt";

pub fn has_completed_onboarding() -> bool {
    crate::recent_files::config_dir()
        .map(|d| d.join(ONBOARDING_FILE))
        .is_some_and(|p| p.exists())
}

pub fn mark_onboarding_completed() {
    if let Some(path) = crate::recent_files::config_dir().map(|d| d.join(ONBOARDING_FILE)) {
        let _ = std::fs::write(path, "completed\n");
    }
}

pub fn load_simplified_mode() -> bool {
    crate::recent_files::config_dir()
        .map(|d| d.join(MODE_FILE))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map_or(false, |s| s.trim() == "simplified")
}

pub fn save_simplified_mode(simplified: bool) {
    if let Some(path) = crate::recent_files::config_dir().map(|d| d.join(MODE_FILE)) {
        let _ = std::fs::write(
            path,
            if simplified {
                "simplified\n"
            } else {
                "complete\n"
            },
        );
    }
}

/// Dynamically calculates the Welcome dialog viewport size based on monitor dimensions.
pub fn welcome_viewport_size(mon: Option<egui::Vec2>) -> egui::Vec2 {
    let m = mon.unwrap_or(egui::vec2(1600.0, 960.0));
    let w = (m.x * 0.36).clamp(460.0, 680.0);
    let h = (m.y * 0.28).clamp(240.0, 360.0);
    egui::vec2(w, h)
}

/// Renders the Welcome to Dōri screen directly inside a CentralPanel without window chrome or black void.
pub fn show_panel(ui: &mut egui::Ui, open: &mut bool, simplified_mode: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    render_welcome_content(ui, open, simplified_mode)
}

/// Dedicated secondary viewport modal shown over an existing open project workspace.
pub fn show(ctx: &egui::Context, open: &mut bool, simplified_mode: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    let mut chosen = None;
    let size = welcome_viewport_size(ctx.input(|i| i.viewport().monitor_size));
    let viewport_id = egui::ViewportId::from_hash_of("welcome_to_douri_modal");
    let builder = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size(size)
        .with_resizable(false)
        .with_decorations(false)
        .with_title("Welcome to Dōri");

    ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x1e, 0x1f, 0x23))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x2e, 0x31, 0x38)))
                    .inner_margin(egui::Margin {
                        left: 18,
                        right: 18,
                        top: 14,
                        bottom: 12,
                    }),
            )
            .show(ctx, |ui| {
                chosen = render_welcome_content(ui, open, simplified_mode);
            });
    });
    chosen
}

fn render_welcome_content(
    ui: &mut egui::Ui,
    open: &mut bool,
    simplified_mode: &mut bool,
) -> Option<bool> {
    let mut chosen_mode = None;
    let ctx = ui.ctx().clone();

    // Draggable background
    let bg_rect = ui.max_rect();
    let bg_resp = ui.interact(bg_rect, egui::Id::unique("welcome_bg_drag"), Sense::drag());
    if bg_resp.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    // Top-right clean close 'x' button
    let close_r = Rect::from_center_size(
        pos2(bg_rect.right() - 14.0, bg_rect.top() + 14.0),
        vec2(20.0, 20.0),
    );
    let close_resp = ui.interact(close_r, egui::Id::unique("welcome_close_x"), Sense::click());
    if close_resp.hovered() {
        ui.painter()
            .circle_filled(close_r.center(), 10.0, Color32::from_white_alpha(20));
    }
    let stroke_color = if close_resp.hovered() {
        Color32::WHITE
    } else {
        Color32::from_rgb(0x9a, 0x9a, 0xa2)
    };
    let x_stroke = Stroke::new(1.2, stroke_color);
    ui.painter().line_segment(
        [
            close_r.center() + vec2(-4.0, -4.0),
            close_r.center() + vec2(4.0, 4.0),
        ],
        x_stroke,
    );
    ui.painter().line_segment(
        [
            close_r.center() + vec2(4.0, -4.0),
            close_r.center() + vec2(-4.0, 4.0),
        ],
        x_stroke,
    );
    if close_resp.clicked() {
        *open = false;
        // On standalone launch without an open project, closing exits the app cleanly
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    ui.horizontal(|ui| {
        // Left Column: Dōri vinyl icon (80x80, r = 16px)
        ui.allocate_ui_with_layout(
            vec2(84.0, 190.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(4.0);
                if let Ok(icon) = eframe::icon_data::from_png_bytes(crate::APP_ICON_PNG) {
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [icon.width as usize, icon.height as usize],
                        &icon.rgba,
                    );
                    let texture = ctx.load_texture(
                        "onboarding_douri_icon",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    ui.add(
                        Image::new(&texture)
                            .fit_to_exact_size(vec2(80.0, 80.0))
                            .tint(Color32::WHITE)
                            .corner_radius(16.0),
                    );
                }
            },
        );

        ui.add_space(14.0);

        // Right Column: Bold header & concise prose
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr("Welcome to Dōri"))
                    .font(FontId::proportional(20.0))
                    .strong()
                    .color(Color32::from_rgb(0xf5, 0xf5, 0xf7)),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Dōri gives you two interface modes tailored for your production style:",
                )
                .font(FontId::proportional(11.5))
                .color(Color32::from_rgb(0xaa, 0xaa, 0xb0)),
            );
            ui.add_space(6.0);

            // Complete Mode bullet
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("• {}:", crate::i18n::tr("Complete Mode")))
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(Color32::from_rgb(0xea, 0xee, 0xf4)),
                );
                ui.label(
                    egui::RichText::new(
                        "Enables full pro tools, advanced routing, mixer slots, and MCP server options.",
                    )
                    .font(FontId::proportional(10.5))
                    .color(Color32::from_rgb(0xba, 0xba, 0xc2)),
                );
            });
            ui.add_space(3.0);

            // Simplified Mode bullet
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("• {}:", crate::i18n::tr("Simplified Mode")))
                        .font(FontId::proportional(11.0))
                        .strong()
                        .color(Color32::from_rgb(0xea, 0xee, 0xf4)),
                );
                ui.label(
                    egui::RichText::new(
                        "Streamlines tools, inspector panels, and menus across the app for core music production.",
                    )
                    .font(FontId::proportional(10.5))
                    .color(Color32::from_rgb(0xba, 0xba, 0xc2)),
                );
            });
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        "Switch modes anytime in Settings → Advanced.",
                    )
                    .font(FontId::proportional(10.0))
                    .color(Color32::from_rgb(0x82, 0x82, 0x8a)),
                );
                ui.add_space(6.0);
                let learn_btn = egui::Button::new(
                    egui::RichText::new(crate::i18n::tr("Learn More…"))
                        .font(FontId::proportional(10.0))
                        .color(Color32::from_rgb(0xd6, 0xd6, 0xdc)),
                )
                .fill(Color32::from_rgb(0x2a, 0x2b, 0x30))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x3e, 0x40, 0x48)))
                .corner_radius(4.0)
                .min_size(vec2(76.0, 18.0));
                if ui.add(learn_btn).clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab("https://github.com/triadwoozie/douri"));
                }
            });
        });
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Footer buttons: Simplified Mode & Complete Mode
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let complete_btn = egui::Button::new(
            egui::RichText::new(crate::i18n::tr("Complete Mode"))
                .font(FontId::proportional(12.0))
                .strong()
                .color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(0x0a, 0x84, 0xff))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x20, 0x90, 0xff)))
        .corner_radius(6.0)
        .min_size(vec2(120.0, 26.0));

        if ui.add(complete_btn).clicked() {
            *simplified_mode = false;
            save_simplified_mode(false);
            mark_onboarding_completed();
            *open = false;
            chosen_mode = Some(false);
        }

        ui.add_space(8.0);

        let simplified_btn = egui::Button::new(
            egui::RichText::new(crate::i18n::tr("Simplified Mode"))
                .font(FontId::proportional(12.0))
                .strong()
                .color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(0x22, 0x25, 0x2a))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x34, 0x38, 0x42)))
        .corner_radius(6.0)
        .min_size(vec2(120.0, 26.0));

        if ui.add(simplified_btn).clicked() {
            *simplified_mode = true;
            save_simplified_mode(true);
            mark_onboarding_completed();
            *open = false;
            chosen_mode = Some(true);
        }
    });

    chosen_mode
}
