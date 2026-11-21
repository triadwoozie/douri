//! Native Git Version Control & Project Alternatives panel for Dōri.
//!
//! Provides a right-side DAW sidebar for tracking project history, managing project alternatives
//! (Git branches), composing snapshots, inspecting musical diffs against HEAD, and restoring
//! checkpoints safely via background threading.

use crate::i18n::tr;
use crate::model::Project;
use crate::UiState;
use egui::{vec2, Color32, CornerRadius, RichText, Sense, Stroke, Ui};
use prowave_vcs::{MusicalChange, VcsTask};
use std::path::Path;

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

pub fn update(s: &mut UiState, p: &Project, project_path: Option<&Path>) {
    // Ensure worker is initialized
    if s.git_worker.is_none() {
        let repo_dir = project_path
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new("."));
        s.git_worker = Some(prowave_vcs::VcsWorker::new(repo_dir.to_path_buf()));
    }

    if let Some(ref mut worker) = s.git_worker {
        while let Some(event) = worker.poll() {
            match event {
                prowave_vcs::VcsEvent::CheckoutCompleted(target) => {
                    s.status = Some(format!("Switched to snapshot/alternative: {target}"));
                }
                prowave_vcs::VcsEvent::Error(err) => {
                    s.status = Some(format!("Git error: {err}"));
                }
                prowave_vcs::VcsEvent::Notice(msg) => {
                    s.status = Some(msg);
                }
                prowave_vcs::VcsEvent::StateUpdated(state) => {
                    // State is automatically synced to latest_state in worker.poll()
                }
            }
        }
    }

    // Trigger quick checkpoint if shortcut was pressed
    if s.git_trigger_checkpoint {
        s.git_trigger_checkpoint = false;
        quick_checkpoint(s, p, project_path);
    }
}

pub fn quick_checkpoint(s: &mut UiState, p: &Project, project_path: Option<&Path>) {
    if let Some(ref worker) = s.git_worker {
        let current_doc = Some(crate::file_commands::content(p));
        let project_file_rel = project_path
            .and_then(|p| p.file_name())
            .map(std::path::PathBuf::from);

        worker.send(VcsTask::QuickCheckpoint {
            current_doc,
            project_file_rel,
        });
    }
}

pub fn show(ui: &mut Ui, p: &mut Project, s: &mut UiState, project_path: Option<&Path>) {
    update(s, p, project_path);

    let panel_bg = Color32::from_rgb(0x18, 0x18, 0x1a);
    let card_bg = Color32::from_rgb(0x22, 0x25, 0x2a);
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
                    RichText::new(phosphor_egui::regular::GIT_BRANCH)
                        .size(18.0)
                        .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
                );
                ui.add_space(4.0);
                ui.strong(
                    RichText::new(tr("Version Control"))
                        .size(15.0)
                        .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("×").size(16.0)).frame(false))
                        .clicked()
                    {
                        s.show_git = false;
                    }
                });
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            let is_repo = s
                .git_worker
                .as_ref()
                .map_or(false, |w| w.latest_state.is_repo);

            if !is_repo {
                render_uninitialized(ui, s);
            } else {
                render_initialized(ui, p, s, project_path, card_bg, border_stroke);
            }
        });
}

fn render_uninitialized(ui: &mut Ui, s: &mut UiState) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(phosphor_egui::regular::GIT_FORK)
                .size(36.0)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
        ui.add_space(12.0);
        ui.strong(
            RichText::new(tr("No Version History"))
                .size(15.0)
                .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(tr(
                "Track project history, create musical alternatives (branches), and save instant checkpoints.",
            ))
            .size(12.0)
            .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
        ui.add_space(16.0);

        let init_btn = egui::Button::new(
            RichText::new(tr("Initialize Version History"))
                .color(Color32::from_rgb(0x11, 0x12, 0x15))
                .size(13.0)
                .strong(),
        )
        .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
        .corner_radius(CornerRadius::same(6));

        if ui.add_sized(vec2(220.0, 32.0), init_btn).clicked() {
            if let Some(ref worker) = s.git_worker {
                worker.send(VcsTask::Init);
            }
        }
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
    let latest_state = if let Some(ref worker) = s.git_worker {
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
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(err)
                            .size(11.5)
                            .color(Color32::from_rgb(0xff, 0x80, 0x80)),
                    );
                });
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

    // Branch & Alternatives Bar
    ui.horizontal(|ui| {
        let (dot_color, status_text) =
            if latest_state.status.is_clean && latest_state.changes.is_empty() {
                (Color32::from_rgb(0x30, 0xd1, 0x58), tr("Clean"))
            } else {
                (Color32::from_rgb(0xff, 0xd6, 0x0a), tr("Modified"))
            };

        ui.label(RichText::new("●").size(11.0).color(dot_color));
        ui.label(
            RichText::new(status_text)
                .size(11.0)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Refresh button
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(phosphor_egui::regular::ARROWS_CLOCKWISE).size(13.0),
                    )
                    .frame(false),
                )
                .clicked()
            {
                if let Some(ref worker) = s.git_worker {
                    worker.send(VcsTask::Refresh {
                        current_doc: Some(crate::file_commands::content(p)),
                        project_file_rel: project_path
                            .and_then(|p| p.file_name())
                            .map(std::path::PathBuf::from),
                    });
                }
            }
            // Push button
            if ui
                .add(
                    egui::Button::new(RichText::new(phosphor_egui::regular::ARROW_UP).size(13.0))
                        .frame(false),
                )
                .on_hover_text(tr("Push to remote"))
                .clicked()
            {
                if let Some(ref worker) = s.git_worker {
                    worker.send(VcsTask::Push);
                }
            }
            // Pull button
            if ui
                .add(
                    egui::Button::new(RichText::new(phosphor_egui::regular::ARROW_DOWN).size(13.0))
                        .frame(false),
                )
                .on_hover_text(tr("Pull from remote"))
                .clicked()
            {
                if let Some(ref worker) = s.git_worker {
                    worker.send(VcsTask::Pull);
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
        egui::ComboBox::from_id_salt("vcs_branch_selector")
            .selected_text(
                RichText::new(&latest_state.current_branch)
                    .size(12.0)
                    .strong(),
            )
            .width(160.0)
            .show_ui(ui, |ui| {
                for branch in &latest_state.branches {
                    let is_current = branch.name == latest_state.current_branch;
                    if ui.selectable_label(is_current, &branch.name).clicked() && !is_current {
                        if let Some(ref worker) = s.git_worker {
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
                    s.git_creating_branch = true;
                    s.git_branch_input.clear();
                }
            });
    });

    if s.git_creating_branch {
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
                        egui::TextEdit::singleline(&mut s.git_branch_input).desired_width(140.0),
                    );
                    if ui.button(tr("Create")).clicked() {
                        let name = s.git_branch_input.trim().to_string();
                        if !name.is_empty() {
                            if let Some(ref worker) = s.git_worker {
                                worker.send(VcsTask::CreateBranch { name });
                            }
                            s.git_creating_branch = false;
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        s.git_creating_branch = false;
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
                RichText::new(tr("Create Snapshot"))
                    .size(12.0)
                    .strong()
                    .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
            );
            ui.add_space(4.0);

            ui.add(
                egui::TextEdit::singleline(&mut s.git_commit_input)
                    .hint_text(tr("Snapshot message (e.g. Added guitar solo)…"))
                    .desired_width(ui.available_width()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let save_btn = egui::Button::new(
                    RichText::new(tr("Save Snapshot"))
                        .color(Color32::from_rgb(0x11, 0x12, 0x15))
                        .size(12.0)
                        .strong(),
                )
                .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
                .corner_radius(CornerRadius::same(6));

                let has_msg = !s.git_commit_input.trim().is_empty();
                if ui.add_enabled(has_msg, save_btn).clicked() {
                    let msg = s.git_commit_input.trim().to_string();
                    if let Some(ref worker) = s.git_worker {
                        worker.send(VcsTask::Commit {
                            message: msg,
                            current_doc: Some(crate::file_commands::content(p)),
                            project_file_rel: project_path
                                .and_then(|p| p.file_name())
                                .map(std::path::PathBuf::from),
                        });
                        s.git_commit_input.clear();
                    }
                }

                let quick_btn = egui::Button::new(RichText::new(tr("Quick Checkpoint")).size(12.0))
                    .corner_radius(CornerRadius::same(6));
                if ui
                    .add(quick_btn)
                    .on_hover_text("Ctrl+Alt+S / Cmd+Alt+S")
                    .clicked()
                {
                    quick_checkpoint(s, p, project_path);
                }
            });
        });

    ui.add_space(10.0);

    // Musical Changes List
    let num_changes = latest_state.changes.len();
    ui.horizontal(|ui| {
        ui.strong(
            RichText::new(format!("{} ({num_changes})", tr("Musical Changes")))
                .size(12.5)
                .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
        );

        if num_changes > 0 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if s.git_revert_confirm {
                    if ui
                        .button(
                            RichText::new(tr("Confirm Revert"))
                                .color(Color32::from_rgb(0xff, 0x45, 0x3a)),
                        )
                        .clicked()
                    {
                        if let Some(ref worker) = s.git_worker {
                            worker.send(VcsTask::RevertUnsaved);
                        }
                        s.git_revert_confirm = false;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        s.git_revert_confirm = false;
                    }
                } else if ui
                    .add(egui::Button::new(RichText::new(tr("Revert")).size(11.0)).frame(false))
                    .clicked()
                {
                    s.git_revert_confirm = true;
                }
            });
        }
    });

    ui.add_space(4.0);

    if num_changes == 0 {
        ui.label(
            RichText::new(tr("Working tree matches last snapshot."))
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
                    let (icon, color) = match change {
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
                        ui.label(RichText::new(icon).size(12.0).strong().color(color));
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
    ui.strong(
        RichText::new(tr("Version History"))
            .size(12.5)
            .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
    );
    ui.add_space(4.0);

    let commits = &latest_state.commits;
    if commits.is_empty() {
        ui.label(
            RichText::new(tr("No snapshots committed yet."))
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
                                        .color(Color32::from_rgb(0xe6, 0xe6, 0xeb)),
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
                                                    RichText::new(tr("Open")).size(10.5),
                                                )
                                                .frame(false),
                                            )
                                            .clicked()
                                        {
                                            if let Some(ref worker) = s.git_worker {
                                                worker.send(VcsTask::Checkout {
                                                    target: commit.hash.clone(),
                                                });
                                            }
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
