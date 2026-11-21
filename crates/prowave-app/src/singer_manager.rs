//! OpenUtau-inspired Singer Manager Dialog for Dōri.
//!
//! Provides a unified management sheet for UTAU voicebanks:
//! auto-discovery, one-click folder & .zip archive import with Shift-JIS decoding,
//! drag-and-drop import, singer card details with voicebank type badge (CV, VCV, CVVC),
//! sample and oto counts, resampler selection, and voicebank removal.

use crate::file_commands;
use crate::ui_chrome::{self, cancel_button, modal_sheet, primary_button};
use egui::{
    pos2, vec2, Align2, Color32, FontId, Id, Rect, Response, Sense, Stroke, StrokeKind, Ui,
};
use prowave_vocal::{
    default_singers_dir, discover_singers, import_voicebank_folder, import_voicebank_zip,
    remove_singer, SingerInfo, VoicebankType,
};
use std::path::{Path, PathBuf};

/// State of the Singer Manager Dialog.
#[derive(Clone, Debug)]
pub struct SingerManagerDialog {
    pub open: bool,
    pub singers: Vec<SingerInfo>,
    pub selected: Option<usize>,
    pub status: Option<String>,
    pub resampler_mode: usize,
    pub external_resampler_path: String,
}

impl Default for SingerManagerDialog {
    fn default() -> Self {
        Self {
            open: false,
            singers: Vec::new(),
            selected: None,
            status: None,
            resampler_mode: 0,
            external_resampler_path: String::new(),
        }
    }
}

impl SingerManagerDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self) {
        self.open = true;
        self.rescan();
    }

    pub fn rescan(&mut self) {
        self.singers = discover_singers(&[]);
        if self.selected.is_none() && !self.singers.is_empty() {
            self.selected = Some(0);
        } else if let Some(idx) = self.selected {
            if idx >= self.singers.len() {
                self.selected = if self.singers.is_empty() {
                    None
                } else {
                    Some(0)
                };
            }
        }
    }

    pub fn import_path(&mut self, path: &Path) -> Result<SingerInfo, String> {
        let is_zip = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));

        let res = if is_zip {
            import_voicebank_zip(path, None)
        } else {
            import_voicebank_folder(path, None)
        };

        match res {
            Ok(dest) => {
                self.rescan();
                // Select newly imported singer
                if let Some(pos) = self.singers.iter().position(|s| s.path == dest.path) {
                    self.selected = Some(pos);
                }
                let name = dest.name.clone();
                self.status = Some(format!("Successfully imported '{name}'"));
                Ok(dest)
            }
            Err(e) => {
                self.status = Some(format!("Import failed: {e}"));
                Err(e)
            }
        }
    }
}

pub fn show(ctx: &egui::Context, dlg: &mut SingerManagerDialog) {
    if !dlg.open {
        return;
    }

    modal_sheet(
        ctx,
        Id::unique("manage_utau_singers_modal"),
        "Manage UTAU Singers",
        vec2(720.0, 500.0),
        |ui| {
            // Status banner if present
            if let Some(ref st) = dlg.status {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(st)
                            .size(11.5)
                            .color(ui_chrome::ACCENT_GREEN),
                    );
                });
                ui.add_space(4.0);
            }

            // Title and action toolbar
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Manage UTAU Singers")
                        .size(15.0)
                        .strong()
                        .color(ui_chrome::TEXT_PRIMARY),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Open Singers Folder").clicked() {
                        let dir = default_singers_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("explorer").arg(&dir).spawn();
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(&dir).spawn();
                        #[cfg(all(unix, not(target_os = "macos")))]
                        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                    }
                    if ui.button("Rescan").clicked() {
                        dlg.rescan();
                        dlg.status = Some("Voicebanks rescanned".to_string());
                    }
                    if ui.button("Import .zip…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("UTAU Voicebank Archive", &["zip"])
                            .pick_file()
                        {
                            let _ = dlg.import_path(&path);
                        }
                    }
                    if ui.button("Import Folder…").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            let _ = dlg.import_path(&folder);
                        }
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // 2-Column Layout: Left (Installed singers list), Right (Singer Inspector)
            let avail = ui.available_size() - vec2(0.0, 42.0); // leave room for footer
            let left_w = 230.0;

            ui.horizontal(|ui| {
                // Left Column: Installed Singer Cards
                egui::Frame::new()
                    .fill(ui_chrome::CARD_BG)
                    .corner_radius(8.0)
                    .stroke(ui_chrome::RIM_STROKE)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.set_width(left_w);
                        ui.set_height(avail.y);

                        ui.label(
                            egui::RichText::new(format!(
                                "Installed Singers ({})",
                                dlg.singers.len()
                            ))
                            .size(11.5)
                            .strong()
                            .color(ui_chrome::TEXT_SECONDARY),
                        );
                        ui.add_space(4.0);

                        if dlg.singers.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "No voicebanks installed.\nClick 'Import .zip…' or drag and drop a voicebank folder or archive.",
                                )
                                .size(11.0)
                                .color(ui_chrome::TEXT_MUTED),
                            );
                        } else {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for (i, s) in dlg.singers.iter().enumerate() {
                                    let is_sel = dlg.selected == Some(i);
                                    let bg = if is_sel {
                                        ui_chrome::CARD_BG_SELECTED
                                    } else {
                                        Color32::TRANSPARENT
                                    };
                                    let resp = ui
                                        .scope_builder(
                                            egui::UiBuilder::new()
                                                .sense(Sense::click()),
                                            |ui| {
                                                ui.painter().rect_filled(
                                                    ui.available_rect_before_wrap(),
                                                    6.0,
                                                    bg,
                                                );
                                                ui.horizontal(|ui| {
                                                    // Type badge
                                                    let badge_color = match s.vb_type {
                                                        VoicebankType::CV => ui_chrome::ACCENT_GREEN,
                                                        VoicebankType::VCV => ui_chrome::ACCENT_PURPLE,
                                                        VoicebankType::CVVC => ui_chrome::ACCENT_AMBER,
                                                    };
                                                    ui.colored_label(
                                                        badge_color,
                                                        format!("[{}]", s.vb_type.label()),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(&s.name)
                                                            .size(12.5)
                                                            .color(if is_sel {
                                                                Color32::WHITE
                                                            } else {
                                                                ui_chrome::TEXT_PRIMARY
                                                            }),
                                                    );
                                                });
                                            },
                                        )
                                        .response;

                                    if resp.clicked() {
                                        dlg.selected = Some(i);
                                    }
                                }
                            });
                        }
                    });

                // Right Column: Details of Selected Singer
                egui::Frame::new()
                    .fill(ui_chrome::CARD_BG)
                    .corner_radius(8.0)
                    .stroke(ui_chrome::RIM_STROKE)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_width(avail.x - left_w - 16.0);
                        ui.set_height(avail.y);

                        if let Some(sel_idx) = dlg.selected {
                            if let Some(singer) = dlg.singers.get(sel_idx) {
                                ui.horizontal(|ui| {
                                    // Portrait or circular icon badge
                                    let (rect, _) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::hover());
                                    ui.painter().circle_filled(
                                        rect.center(),
                                        28.0,
                                        Color32::from_rgb(0x32, 0x33, 0x38),
                                    );
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        phosphor_egui::regular::MICROPHONE_STAGE,
                                        FontId::proportional(28.0),
                                        ui_chrome::ACCENT_PURPLE,
                                    );

                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(&singer.name)
                                                .size(16.0)
                                                .strong()
                                                .color(ui_chrome::TEXT_PRIMARY),
                                        );
                                        let author_txt = if singer.author.is_empty() {
                                            "Unknown Author".to_string()
                                        } else {
                                            format!("Author: {}", singer.author)
                                        };
                                        ui.label(
                                            egui::RichText::new(author_txt)
                                                .size(11.5)
                                                .color(ui_chrome::TEXT_SECONDARY),
                                        );
                                    });
                                });

                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);

                                egui::Grid::new("singer_details_grid")
                                    .num_columns(2)
                                    .spacing(vec2(16.0, 8.0))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new("Voicebank Type:").color(ui_chrome::TEXT_MUTED));
                                        let (lbl, col) = match singer.vb_type {
                                            VoicebankType::CV => ("CV (Consonant-Vowel)", ui_chrome::ACCENT_GREEN),
                                            VoicebankType::VCV => ("VCV (Vowel-Consonant-Vowel Continuous)", ui_chrome::ACCENT_PURPLE),
                                            VoicebankType::CVVC => ("CVVC (Consonant-Vowel Vowel-Consonant)", ui_chrome::ACCENT_AMBER),
                                        };
                                        ui.colored_label(col, lbl);
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Oto Aliases:").color(ui_chrome::TEXT_MUTED));
                                        ui.label(format!("{} entries", singer.oto_count));
                                        ui.end_row();

                                        ui.label(egui::RichText::new("WAV Samples:").color(ui_chrome::TEXT_MUTED));
                                        ui.label(format!("{} files", singer.sample_count));
                                        ui.end_row();

                                        ui.label(egui::RichText::new("Folder Location:").color(ui_chrome::TEXT_MUTED));
                                        ui.label(
                                            egui::RichText::new(singer.path.display().to_string())
                                                .size(10.5)
                                                .color(ui_chrome::TEXT_SECONDARY),
                                        );
                                        ui.end_row();
                                    });

                                ui.add_space(12.0);
                                ui.separator();
                                ui.add_space(8.0);

                                // Resampler Setting
                                ui.label(
                                    egui::RichText::new("Resampler Engine")
                                        .size(12.0)
                                        .strong()
                                        .color(ui_chrome::TEXT_PRIMARY),
                                );
                                ui.horizontal(|ui| {
                                    const RESAMPLERS: [&str; 2] = ["Built-in WORLD (Pure-Rust)", "External CLI Resampler"];
                                    egui::ComboBox::from_id_salt("singer_resampler_choice")
                                        .selected_text(RESAMPLERS[dlg.resampler_mode])
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut dlg.resampler_mode, 0, RESAMPLERS[0]);
                                            ui.selectable_value(&mut dlg.resampler_mode, 1, RESAMPLERS[1]);
                                        });
                                 });

                                if dlg.resampler_mode == 1 {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("CLI Path:").color(ui_chrome::TEXT_MUTED));
                                        ui.text_edit_singleline(&mut dlg.external_resampler_path);
                                        if ui.button("Browse…").clicked() {
                                            if let Some(bin) = rfd::FileDialog::new().pick_file() {
                                                dlg.external_resampler_path = bin.display().to_string();
                                            }
                                        }
                                    });
                                }

                                ui.add_space(16.0);
                                if ui.button(egui::RichText::new("Delete Voicebank").color(crate::theme::ACCENT_CRIMSON)).clicked() {
                                    let path = singer.path.clone();
                                    if let Err(e) = remove_singer(&path) {
                                        dlg.status = Some(format!("Error deleting: {e}"));
                                    } else {
                                        dlg.rescan();
                                        dlg.status = Some("Voicebank deleted".to_string());
                                    }
                                }
                            }
                        } else {
                            ui.label(
                                egui::RichText::new("Select a singer from the list on the left.")
                                    .color(ui_chrome::TEXT_MUTED),
                            );
                        }
                    });
            });

            // Footer
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_button(ui, "Done").clicked() {
                    dlg.open = false;
                }
            });
        },
    );
}
