//! Native Collaborate Version History & Project Alternatives panel for Dōri.
//!
//! Provides a right-side DAW sidebar for tracking project version history, managing project
//! alternatives, composing snapshots, inspecting musical edits against the active version,
//! managing Studio Links (GitHub/GitLab remote URLs), and restoring versions cleanly via
//! background thread execution.

use crate::i18n::tr;
use crate::model::Project;
use crate::UiState;
use egui::{vec2, Color32, CornerRadius, RichText, Sense, Stroke, Ui};
use prowave_vcs::{MusicalChange, VcsTask};
use std::path::{Path, PathBuf};

pub const WIDTH: f32 = 340.0;

fn format_relative_time(timestamp: i64) -> String {
    if timestamp == 0 {
        return tr("Unknown time").to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let diff = now - timestamp;
    if diff <= 60 {
        return tr("Just now").to_string();
    }
    if diff < 3600 {
        return format!("{}m ago", diff / 60);
    }
    if diff < 86400 {
        return format!("{}h ago", diff / 3600);
    }
    format!("{}d ago", diff / 86400)
}

/// Polls background Collaborate worker events when Collaborate is enabled.
/// If `s.collaborate_enabled` is false, skips worker polling entirely for zero-overhead.
pub fn update(s: &mut UiState, p: &Project, project_path: Option<&Path>) {
    if !s.collaborate_enabled {
        return;
    }

    // Ensure worker is initialized
    if s.collaborate_worker.is_none() {
        let repo_dir = project_path
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new("."));
        s.collaborate_worker = Some(prowave_vcs::VcsWorker::new(repo_dir.to_path_buf()));
    }

    if let Some(ref mut worker) = s.collaborate_worker {
        while let Some(event) = worker.poll() {
            match event {
                prowave_vcs::VcsEvent::CheckoutCompleted(target) => {
                    s.status = Some(format!("Switched to version / alternative: {target}"));
                }
                prowave_vcs::VcsEvent::Error(err) => {
                    s.status = Some(err);
                }
                prowave_vcs::VcsEvent::Notice(msg) => {
                    s.status = Some(msg);
                }
                prowave_vcs::VcsEvent::StateUpdated(_) => {}
            }
        }
    }

    // Trigger quick snapshot if shortcut was pressed
    if s.collaborate_trigger_snapshot {
        s.collaborate_trigger_snapshot = false;
        quick_snapshot(s, p, project_path);
    }
}

pub fn quick_snapshot(s: &mut UiState, p: &Project, project_path: Option<&Path>) {
    if !s.collaborate_enabled {
        return;
    }

    if s.collaborate_worker.is_none() {
        let repo_dir = project_path
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new("."));
        s.collaborate_worker = Some(prowave_vcs::VcsWorker::new(repo_dir.to_path_buf()));
    }

    if let Some(ref worker) = s.collaborate_worker {
        let current_doc = Some(crate::file_commands::content(p));
        let project_file_rel = project_path.and_then(|p| p.file_name()).map(PathBuf::from);

        worker.send(VcsTask::QuickCheckpoint {
            current_doc,
            project_file_rel,
        });
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState, project_path: Option<&Path>) {
    update(s, p, project_path);

    let panel_bg = Color32::from_rgb(0x1a, 0x1b, 0x1e);
    let card_bg = Color32::from_rgb(0x20, 0x22, 0x26);
    let border_color = Color32::from_rgb(0x2e, 0x31, 0x38);
    let border_stroke = Stroke::new(1.0, border_color);

    egui::Frame::new()
        .fill(panel_bg)
        .stroke(border_stroke)
        .inner_margin(12.0)
        .show(ui, |ui| {
            // Header Row
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(phosphor_egui::regular::USERS)
                        .size(17.0)
                        .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
                );
                ui.add_space(4.0);
                ui.strong(
                    RichText::new(tr("Collaborate"))
                        .size(14.5)
                        .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("×").size(16.0)).frame(false))
                        .clicked()
                    {
                        s.show_collaborate = false;
                    }

                    ui.checkbox(&mut s.collaborate_enabled, tr("Enable Collaborate"));
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            if !s.collaborate_enabled {
                render_paused(ui, s, card_bg, border_stroke);
                return;
            }

            let is_repo = s
                .collaborate_worker
                .as_ref()
                .is_some_and(|w| w.latest_state.is_repo);

            if !is_repo {
                render_uninitialized(ui, s, card_bg, border_stroke);
            } else {
                render_initialized(ui, p, s, project_path, card_bg, border_stroke);
            }
        });
}

fn render_paused(ui: &mut Ui, s: &mut UiState, card_bg: Color32, border_stroke: Stroke) {
    ui.add_space(16.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(phosphor_egui::regular::PAUSE_CIRCLE)
                        .size(32.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(8.0);
                ui.strong(
                    RichText::new(tr("Collaborate Paused"))
                        .size(13.5)
                        .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr(
                        "Collaborate & version history is paused to save CPU and disk I/O.",
                    ))
                    .size(11.5)
                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(12.0);

                let enable_btn = egui::Button::new(
                    RichText::new(tr("Enable Collaborate"))
                        .color(Color32::from_rgb(0x11, 0x12, 0x15))
                        .size(12.0)
                        .strong(),
                )
                .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
                .corner_radius(CornerRadius::same(6));

                if ui.add_sized(vec2(180.0, 30.0), enable_btn).clicked() {
                    s.collaborate_enabled = true;
                }
            });
        });
}

fn render_uninitialized(ui: &mut Ui, s: &mut UiState, card_bg: Color32, border_stroke: Stroke) {
    ui.add_space(20.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(phosphor_egui::regular::USERS)
                        .size(36.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(10.0);
                ui.strong(
                    RichText::new(tr("Project History"))
                        .size(14.5)
                        .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr(
                        "Keep track of all edits, manage project alternatives, and collaborate with your studio team.",
                    ))
                    .size(11.5)
                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(16.0);

                let init_btn = egui::Button::new(
                    RichText::new(tr("Enable Project History"))
                        .color(Color32::from_rgb(0x11, 0x12, 0x15))
                        .size(12.5)
                        .strong(),
                )
                .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
                .corner_radius(CornerRadius::same(6));

                if ui.add_sized(vec2(200.0, 32.0), init_btn).clicked() {
                    if let Some(ref worker) = s.collaborate_worker {
                        worker.send(VcsTask::Init);
                    }
                }
            });
        });
}

fn render_initialized(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    project_path: Option<&Path>,
    card_bg: Color32,
    border_stroke: Stroke,
) {
    let latest_state = if let Some(ref worker) = s.collaborate_worker {
        worker.latest_state.clone()
    } else {
        return;
    };

    // Notice or Error Banner
    if let Some(ref err) = latest_state.error_msg {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(0x3d, 0x18, 0x18))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0xff, 0x45, 0x3a)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(err)
                        .size(11.5)
                        .color(Color32::from_rgb(0xff, 0x80, 0x80)),
                );
            });
        ui.add_space(4.0);
    } else if let Some(ref notice) = latest_state.notice_msg {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(0x1a, 0x2e, 0x22))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x30, 0xd1, 0x58)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(notice)
                        .size(11.5)
                        .color(Color32::from_rgb(0x80, 0xe6, 0xa0)),
                );
            });
        ui.add_space(4.0);
    }

    // Status Indicator & Refresh
    ui.horizontal(|ui| {
        let (dot_color, status_text) =
            if latest_state.status.is_clean && latest_state.changes.is_empty() {
                (Color32::from_rgb(0x30, 0xd1, 0x58), tr("All edits saved"))
            } else {
                (
                    Color32::from_rgb(0xff, 0xd6, 0x0a),
                    tr("Recent edits in progress"),
                )
            };

        ui.label(RichText::new("●").size(10.0).color(dot_color));
        ui.label(
            RichText::new(status_text)
                .size(11.5)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(phosphor_egui::regular::ARROWS_CLOCKWISE).size(13.0),
                    )
                    .frame(false),
                )
                .on_hover_text(tr("Refresh Status"))
                .clicked()
            {
                if let Some(ref worker) = s.collaborate_worker {
                    worker.send(VcsTask::Refresh {
                        current_doc: Some(crate::file_commands::content(p)),
                        project_file_rel: project_path
                            .and_then(|p| p.file_name())
                            .map(PathBuf::from),
                    });
                }
            }
        });
    });

    ui.add_space(4.0);

    // Alternative Dropdown
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Alternative:"))
                .size(12.0)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
        egui::ComboBox::from_id_salt("collaborate_alt_selector")
            .selected_text(
                RichText::new(&latest_state.current_branch)
                    .size(12.0)
                    .strong(),
            )
            .width(170.0)
            .show_ui(ui, |ui| {
                for branch in &latest_state.branches {
                    let is_current = branch.name == latest_state.current_branch;
                    if ui.selectable_label(is_current, &branch.name).clicked() && !is_current {
                        if let Some(ref worker) = s.collaborate_worker {
                            worker.send(VcsTask::Checkout {
                                target: branch.name.clone(),
                            });
                        }
                    }
                }
                ui.separator();
                if ui
                    .selectable_label(false, tr("+ New Alternative…"))
                    .clicked()
                {
                    s.collaborate_creating_alt = true;
                    s.collaborate_alt_input.clear();
                }
            });
    });

    // Inline New Alternative creator
    if s.collaborate_creating_alt {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(card_bg)
            .stroke(border_stroke)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(tr("New Alternative Name:"))
                        .size(11.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut s.collaborate_alt_input)
                            .desired_width(140.0),
                    );
                    let create_btn = egui::Button::new(RichText::new(tr("Create")).size(11.5))
                        .fill(Color32::from_rgb(0x22, 0x24, 0x29))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(0x36, 0x39, 0x42)))
                        .corner_radius(CornerRadius::same(6));

                    if ui.add(create_btn).clicked() {
                        let name = s.collaborate_alt_input.trim().to_string();
                        if !name.is_empty() {
                            if let Some(ref worker) = s.collaborate_worker {
                                worker.send(VcsTask::CreateBranch { name });
                            }
                            s.collaborate_creating_alt = false;
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        s.collaborate_creating_alt = false;
                    }
                });
            });
    }

    // Inline "New Alternative from Version"
    if let Some(from_rev) = s.collaborate_branch_from_rev.clone() {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(card_bg)
            .stroke(border_stroke)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                let short_rev = &from_rev[..7.min(from_rev.len())];
                ui.label(
                    RichText::new(format!(
                        "{}: {short_rev}",
                        tr("New Alternative from Version")
                    ))
                    .size(11.0)
                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut s.collaborate_alt_input)
                            .hint_text(tr("Alternative name…"))
                            .desired_width(130.0),
                    );
                    let create_btn = egui::Button::new(RichText::new(tr("Create")).size(11.5))
                        .fill(Color32::from_rgb(0x22, 0x24, 0x29))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(0x36, 0x39, 0x42)))
                        .corner_radius(CornerRadius::same(6));

                    if ui.add(create_btn).clicked() {
                        let name = s.collaborate_alt_input.trim().to_string();
                        if !name.is_empty() {
                            if let Some(ref worker) = s.collaborate_worker {
                                worker.send(VcsTask::CreateBranchFrom {
                                    name,
                                    from_rev: from_rev.clone(),
                                });
                            }
                            s.collaborate_branch_from_rev = None;
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        s.collaborate_branch_from_rev = None;
                    }
                });
            });
    }

    ui.add_space(6.0);

    // Action Row (Get Updates, Share Updates, Studio Link…)
    ui.horizontal(|ui| {
        let sec_btn = |text: &'static str| {
            egui::Button::new(
                RichText::new(tr(text))
                    .size(11.5)
                    .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
            )
            .fill(Color32::from_rgb(0x22, 0x24, 0x29))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x36, 0x39, 0x42)))
            .corner_radius(CornerRadius::same(6))
        };

        if ui.add(sec_btn("Get Updates")).clicked() {
            if let Some(ref worker) = s.collaborate_worker {
                worker.send(VcsTask::Pull);
            }
        }

        if ui.add(sec_btn("Share Updates")).clicked() {
            if let Some(ref worker) = s.collaborate_worker {
                worker.send(VcsTask::Push);
            }
        }

        if ui.add(sec_btn("Studio Link…")).clicked() {
            s.collaborate_showing_link_modal = !s.collaborate_showing_link_modal;
        }
    });

    // Studio Link Inline Form
    if s.collaborate_showing_link_modal {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(card_bg)
            .stroke(border_stroke)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(tr("Studio Link (Online / GitHub):"))
                        .size(11.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(2.0);
                ui.add(
                    egui::TextEdit::singleline(&mut s.collaborate_link_input)
                        .hint_text("username/project or https://github.com/…")
                        .desired_width(ui.available_width()),
                );

                let norm_res = prowave_vcs::normalize_studio_link(&s.collaborate_link_input);
                if !s.collaborate_link_input.trim().is_empty() {
                    ui.add_space(2.0);
                    match &norm_res {
                        Ok(norm) => {
                            ui.label(
                                RichText::new(format!("Preview: {norm}"))
                                    .size(10.5)
                                    .color(Color32::from_rgb(0x80, 0xe6, 0xa0)),
                            );
                        }
                        Err(e) => {
                            ui.label(
                                RichText::new(e)
                                    .size(10.5)
                                    .color(Color32::from_rgb(0xff, 0x80, 0x80)),
                            );
                        }
                    }
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let save_link_btn = egui::Button::new(
                        RichText::new(tr("Save Link"))
                            .color(Color32::from_rgb(0x11, 0x12, 0x15))
                            .size(11.5)
                            .strong(),
                    )
                    .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
                    .corner_radius(CornerRadius::same(6));

                    let valid = norm_res.is_ok();
                    if ui.add_enabled(valid, save_link_btn).clicked() {
                        if let Ok(url) = norm_res {
                            if let Some(ref worker) = s.collaborate_worker {
                                worker.send(VcsTask::SetRemote { url });
                            }
                            s.collaborate_showing_link_modal = false;
                        }
                    }

                    if ui.button(tr("Cancel")).clicked() {
                        s.collaborate_showing_link_modal = false;
                    }
                });
            });
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Snapshot Composer
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr("Save Version"))
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
            );
            ui.add_space(4.0);

            ui.add(
                egui::TextEdit::singleline(&mut s.collaborate_note_input)
                    .hint_text(tr("What changed? (e.g., Tightened chorus drums)…"))
                    .desired_width(ui.available_width()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let save_btn = egui::Button::new(
                    RichText::new(tr("Save Version"))
                        .color(Color32::from_rgb(0x11, 0x12, 0x15))
                        .size(12.0)
                        .strong(),
                )
                .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
                .corner_radius(CornerRadius::same(6));

                let has_msg = !s.collaborate_note_input.trim().is_empty();
                if ui.add_enabled(has_msg, save_btn).clicked() {
                    let msg = s.collaborate_note_input.trim().to_string();
                    if let Some(ref worker) = s.collaborate_worker {
                        worker.send(VcsTask::Commit {
                            message: msg,
                            current_doc: Some(crate::file_commands::content(p)),
                            project_file_rel: project_path
                                .and_then(|p| p.file_name())
                                .map(PathBuf::from),
                        });
                        s.collaborate_note_input.clear();
                    }
                }

                let quick_btn = egui::Button::new(
                    RichText::new(tr("Quick Snapshot"))
                        .size(12.0)
                        .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
                )
                .fill(Color32::from_rgb(0x22, 0x24, 0x29))
                .stroke(Stroke::new(1.0, Color32::from_rgb(0x36, 0x39, 0x42)))
                .corner_radius(CornerRadius::same(6));

                if ui
                    .add(quick_btn)
                    .on_hover_text("Ctrl+Alt+S / Cmd+Alt+S")
                    .clicked()
                {
                    quick_snapshot(s, p, project_path);
                }
            });
        });

    ui.add_space(10.0);

    // Recent Edits List
    let num_changes = latest_state.changes.len();
    ui.horizontal(|ui| {
        ui.strong(
            RichText::new(format!("{} ({num_changes})", tr("Recent Edits")))
                .size(12.5)
                .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
        );

        if num_changes > 0 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if s.collaborate_revert_confirm {
                    if ui
                        .button(
                            RichText::new(tr("Confirm Discard"))
                                .color(Color32::from_rgb(0xff, 0x45, 0x3a)),
                        )
                        .clicked()
                    {
                        if let Some(ref worker) = s.collaborate_worker {
                            worker.send(VcsTask::RevertUnsaved);
                        }
                        s.collaborate_revert_confirm = false;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        s.collaborate_revert_confirm = false;
                    }
                } else {
                    let discard_btn = egui::Button::new(
                        RichText::new(tr("Discard Unsaved Edits"))
                            .size(11.0)
                            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                    )
                    .frame(false);

                    if ui.add(discard_btn).clicked() {
                        s.collaborate_revert_confirm = true;
                    }
                }
            });
        }
    });

    ui.add_space(4.0);

    if num_changes == 0 {
        ui.label(
            RichText::new(tr("All edits saved to current version."))
                .size(11.5)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
    } else {
        egui::Frame::new()
            .fill(card_bg)
            .stroke(border_stroke)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                for change in latest_state.changes.iter().take(8) {
                    let (indicator, color) = match change {
                        MusicalChange::TrackAdded { .. } => {
                            ("+", Color32::from_rgb(0x30, 0xd1, 0x58))
                        }
                        MusicalChange::TrackRemoved { .. } => {
                            ("-", Color32::from_rgb(0xff, 0x45, 0x3a))
                        }
                        MusicalChange::RegionModified { .. } => {
                            ("~", Color32::from_rgb(0x3a, 0x7b, 0xd5))
                        }
                        MusicalChange::TempoChanged { .. } => {
                            ("~", Color32::from_rgb(0xff, 0xd6, 0x0a))
                        }
                        MusicalChange::MixerChanged { .. } => {
                            ("~", Color32::from_rgb(0x8b, 0x5c, 0xf6))
                        }
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(indicator).size(12.0).strong().color(color));
                        ui.label(
                            RichText::new(change.description())
                                .size(11.5)
                                .color(Color32::from_rgb(0xdd, 0xdd, 0xe0)),
                        );
                    });
                }
                if num_changes > 8 {
                    ui.label(
                        RichText::new(format!("+ {} more changes…", num_changes - 8))
                            .size(11.0)
                            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                    );
                }
            });
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    // Version History Timeline
    let commits = &latest_state.commits;
    ui.strong(
        RichText::new(format!("{} ({})", tr("Version History"), commits.len()))
            .size(12.5)
            .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
    );
    ui.add_space(4.0);

    if commits.is_empty() {
        ui.label(
            RichText::new(tr("No versions saved yet."))
                .size(11.5)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
    } else {
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .show(ui, |ui| {
                for commit in commits {
                    egui::Frame::new()
                        .fill(card_bg)
                        .stroke(border_stroke)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&commit.message)
                                        .size(12.0)
                                        .strong()
                                        .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&commit.short_hash)
                                        .monospace()
                                        .size(10.5)
                                        .color(Color32::from_rgb(0x3a, 0x7b, 0xd5)),
                                );
                                ui.label(
                                    RichText::new(format_relative_time(commit.timestamp))
                                        .size(10.5)
                                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(tr("Open Version")).size(10.5),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            if let Some(ref worker) = s.collaborate_worker {
                                                worker.send(VcsTask::Checkout {
                                                    target: commit.hash.clone(),
                                                });
                                            }
                                        }

                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(tr("New Alternative from Here"))
                                                        .size(10.5),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            s.collaborate_branch_from_rev =
                                                Some(commit.hash.clone());
                                            s.collaborate_alt_input.clear();
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(3.0);
                }
            });
    }
}
