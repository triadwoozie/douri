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

/// Renders the Welcome to Dōri screen directly inside a CentralPanel without window chrome or black void.
pub fn show_panel(ui: &mut egui::Ui, open: &mut bool, simplified_mode: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    render_welcome_content(ui, open, simplified_mode)
}

/// Modal version shown over an existing open project workspace.
pub fn show(ctx: &egui::Context, open: &mut bool, simplified_mode: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    let mut chosen_mode = None;
    let full = ctx.input(|i| i.content_rect());

    // Dim background behind modal
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::unique("onboarding_dim_layer"),
    ));
    painter.rect_filled(full, 0.0, Color32::from_black_alpha(160));

    let modal_w = 600.0f32.min(full.width() - 32.0);
    let modal_h = 350.0f32.min(full.height() - 32.0);

    let frame = egui::Frame::new()
        .fill(Color32::from_rgb(0x20, 0x22, 0x26))
        .corner_radius(12.0)
        .stroke(Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 31),
        ))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(120),
        })
        .inner_margin(egui::Margin {
            left: 20,
            right: 20,
            top: 18,
            bottom: 14,
        });

    egui::Window::new("onboarding_welcome_dialog")
        .title_bar(false)
        .frame(frame)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .fixed_size(vec2(modal_w, modal_h))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            chosen_mode = render_welcome_content(ui, open, simplified_mode);
        });

    chosen_mode
}

fn render_welcome_content(
    ui: &mut egui::Ui,
    open: &mut bool,
    simplified_mode: &mut bool,
) -> Option<bool> {
    let mut chosen_mode = None;
    let ctx = ui.ctx().clone();

    ui.horizontal(|ui| {
        // Left Column: Dōri vinyl icon at full brightness (88x88, r = 18px)
        ui.allocate_ui_with_layout(
            vec2(96.0, 240.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                ui.add_space(8.0);
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
                            .fit_to_exact_size(vec2(84.0, 84.0))
                            .tint(Color32::WHITE)
                            .corner_radius(18.0),
                    );
                }
            },
        );

        ui.add_space(14.0);

        // Right Column: Bold header & concise prose
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(crate::i18n::tr("Welcome to Dōri"))
                    .font(FontId::proportional(21.0))
                    .strong()
                    .color(Color32::from_rgb(0xf5, 0xf5, 0xf7)),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Dōri gives you two interface modes tailored for your production style:",
                )
                .font(FontId::proportional(12.0))
                .color(Color32::from_rgb(0xaa, 0xaa, 0xb0)),
            );
            ui.add_space(8.0);

            // Complete Mode bullet
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("• {}:", crate::i18n::tr("Complete Mode")))
                        .font(FontId::proportional(11.5))
                        .strong()
                        .color(Color32::from_rgb(0xea, 0xee, 0xf4)),
                );
                ui.label(
                    egui::RichText::new(
                        "Enables full pro tools, advanced routing, mixer slots, and MCP server options.",
                    )
                    .font(FontId::proportional(11.0))
                    .color(Color32::from_rgb(0xba, 0xba, 0xc2)),
                );
            });
            ui.add_space(4.0);

            // Simplified Mode bullet
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("• {}:", crate::i18n::tr("Simplified Mode")))
                        .font(FontId::proportional(11.5))
                        .strong()
                        .color(Color32::from_rgb(0xea, 0xee, 0xf4)),
                );
                ui.label(
                    egui::RichText::new(
                        "Streamlines tools, inspector panels, and menus across the app for core music production.",
                    )
                    .font(FontId::proportional(11.0))
                    .color(Color32::from_rgb(0xba, 0xba, 0xc2)),
                );
            });
            ui.add_space(8.0);

            ui.label(
                egui::RichText::new(
                    "You can switch between modes at any time in Settings → Advanced.",
                )
                .font(FontId::proportional(10.5))
                .color(Color32::from_rgb(0x82, 0x82, 0x8a)),
            );
            ui.add_space(8.0);

            // Neutral Learn More… pill (#3A3A3E)
            let learn_btn = egui::Button::new(
                egui::RichText::new(crate::i18n::tr("Learn More…"))
                    .font(FontId::proportional(11.0))
                    .color(Color32::from_rgb(0xd6, 0xd6, 0xdc)),
            )
            .fill(Color32::from_rgb(0x3a, 0x3a, 0x3e))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x48, 0x48, 0x4e)))
            .corner_radius(4.0)
            .min_size(vec2(86.0, 22.0));
            if ui.add(learn_btn).clicked() {
                ctx.open_url(egui::OpenUrl::new_tab("https://github.com/triadwoozie/douri"));
            }
        });
    });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // Footer buttons: Simplified Mode & Complete Mode
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let complete_btn = egui::Button::new(
            egui::RichText::new(crate::i18n::tr("Complete Mode"))
                .font(FontId::proportional(12.5))
                .strong()
                .color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(0x0a, 0x84, 0xff))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x20, 0x90, 0xff)))
        .corner_radius(6.0)
        .min_size(vec2(130.0, 28.0));

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
                .font(FontId::proportional(12.5))
                .strong()
                .color(Color32::WHITE),
        )
        .fill(Color32::from_rgb(0x45, 0x45, 0x49))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x56, 0x56, 0x5c)))
        .corner_radius(6.0)
        .min_size(vec2(130.0, 28.0));

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
