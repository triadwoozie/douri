//! First-launch Onboarding Screen ("Welcome to Dōri").
//! Provides user choice between Complete Mode and Simplified Mode.

use egui::{vec2, Align2, Color32, FontId, Image, Sense, Stroke};

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

pub fn show(ctx: &egui::Context, open: &mut bool, simplified_mode: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    let mut close = false;
    let mut chosen_mode = None;
    let full = ctx.input(|i| i.content_rect());
    // Subtle backdrop dim
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::unique("onboarding_dim"),
    ));
    painter.rect_filled(full, 0.0, Color32::from_black_alpha(150));

    let window_w = 640.0f32.min(full.width() - 32.0);
    let window_h = 380.0f32.min(full.height() - 32.0);

    crate::liquid_glass::glass_window(crate::i18n::tr("Welcome to Dōri"))
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .fixed_size(vec2(window_w, window_h))
        .show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                // Left Column: App Icon
                ui.allocate_ui_with_layout(
                    vec2(130.0, 240.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.add_space(16.0);
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
                            ui.add(Image::new(&texture).max_width(112.0).max_height(112.0).corner_radius(22.0));
                        }
                    },
                );

                ui.add_space(12.0);

                // Right Column: Text & Modes description
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("Welcome to Dōri"))
                            .font(FontId::proportional(22.0))
                            .strong()
                            .color(Color32::from_rgb(0xea, 0xee, 0xf4)),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new(
                            "Dōri gives you two interface modes tailored for your production style:",
                        )
                        .font(FontId::proportional(12.5))
                        .color(Color32::from_rgb(0xaa, 0xaa, 0xb0)),
                    );
                    ui.add_space(8.0);

                    // Complete Mode bullet
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("• {}:", crate::i18n::tr("Complete Mode")))
                                .font(FontId::proportional(12.5))
                                .strong()
                                .color(Color32::from_rgb(0x3a, 0x7b, 0xd5)),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Enables the full set of professional tools, advanced routing, full Mixer slots, and MCP server options.",
                            )
                            .font(FontId::proportional(12.0))
                            .color(Color32::from_rgb(0xcc, 0xcc, 0xd2)),
                        );
                    });
                    ui.add_space(6.0);

                    // Simplified Mode bullet
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("• {}:", crate::i18n::tr("Simplified Mode")))
                                .font(FontId::proportional(12.5))
                                .strong()
                                .color(Color32::from_rgb(0x2e, 0xbd, 0x6b)),
                        );
                        ui.label(
                            egui::RichText::new(
                                "Streamlines tools, inspector panels, and menus across the app for focused core music production.",
                            )
                            .font(FontId::proportional(12.0))
                            .color(Color32::from_rgb(0xcc, 0xcc, 0xd2)),
                        );
                    });
                    ui.add_space(10.0);

                    ui.label(
                        egui::RichText::new("You can switch between modes at any time in Settings → Advanced.")
                            .font(FontId::proportional(11.0))
                            .color(Color32::from_rgb(0x88, 0x88, 0x90)),
                    );
                    ui.add_space(8.0);

                    if ui.link(crate::i18n::tr("Learn More…")).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab("https://github.com/triadwoozie/douri"));
                    }
                });
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // Bottom Footer
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let complete_btn = egui::Button::new(
                    egui::RichText::new(crate::i18n::tr("Complete Mode"))
                        .font(FontId::proportional(13.0))
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(Color32::from_rgb(0x3a, 0x7b, 0xd5))
                .corner_radius(6.0)
                .min_size(vec2(130.0, 30.0));

                if ui.add(complete_btn).clicked() {
                    *simplified_mode = false;
                    save_simplified_mode(false);
                    mark_onboarding_completed();
                    close = true;
                    chosen_mode = Some(false);
                }

                ui.add_space(8.0);

                let simplified_btn = egui::Button::new(
                    egui::RichText::new(crate::i18n::tr("Simplified Mode"))
                        .font(FontId::proportional(13.0))
                        .color(Color32::from_rgb(0xea, 0xee, 0xf4)),
                )
                .fill(Color32::from_rgb(0x26, 0x26, 0x2a))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x34, 0x34, 0x39)))
                .corner_radius(6.0)
                .min_size(vec2(130.0, 30.0));

                if ui.add(simplified_btn).clicked() {
                    *simplified_mode = true;
                    save_simplified_mode(true);
                    mark_onboarding_completed();
                    close = true;
                    chosen_mode = Some(true);
                }
            });
        });

    if close {
        *open = false;
    }

    chosen_mode
}
