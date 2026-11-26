//! Native Collaborate Version History & Project Alternatives standalone window for Dōri.
//!
//! Provides a dedicated, spacious secondary OS window for tracking project version history,
//! managing project alternatives, composing snapshots, inspecting musical edits against the active
//! version, managing Studio Links (GitHub/GitLab remote URLs), and restoring versions cleanly via
//! background thread execution.

use crate::i18n::tr;
use crate::model::Project;
use crate::UiState;
use egui::{
    pos2, vec2, Color32, CornerRadius, RichText, Sense, Stroke, Ui, Vec2, ViewportBuilder,
    ViewportCommand, ViewportId,
};
use prowave_vcs::{MusicalChange, VcsTask, VcsUiState};
use std::path::{Path, PathBuf};

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

fn collaborate_viewport_size(monitor: Option<Vec2>) -> (Vec2, Option<egui::Pos2>) {
    let mon = monitor.unwrap_or(vec2(1920.0, 1080.0));
    let max_w = (mon.x - 48.0).max(640.0);
    let max_h = (mon.y - 64.0).max(480.0);
    let w = (mon.x * 0.58).clamp(760.0, 1120.0).min(max_w);
    let h = (mon.y * 0.68).clamp(520.0, 760.0).min(max_h);
    let size = vec2(w, h);
    let pos = pos2(((mon.x - w) * 0.5).max(24.0), ((mon.y - h) * 0.5).max(32.0));
    (size, Some(pos))
}

fn primary_btn(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_string())
            .color(Color32::from_rgb(0x11, 0x12, 0x15))
            .size(12.5)
            .strong(),
    )
    .fill(Color32::from_rgb(0xf2, 0xf3, 0xf5))
    .corner_radius(CornerRadius::same(6))
}

fn secondary_btn(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_string())
            .color(Color32::from_rgb(0xe6, 0xe8, 0xec))
            .size(12.0),
    )
    .fill(Color32::from_rgb(0x22, 0x24, 0x29))
    .stroke(Stroke::new(1.0, Color32::from_rgb(0x36, 0x39, 0x42)))
    .corner_radius(CornerRadius::same(6))
}

fn compact_secondary_btn(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_string())
            .color(Color32::from_rgb(0xe6, 0xe8, 0xec))
            .size(11.0),
    )
    .fill(Color32::from_rgb(0x22, 0x24, 0x29))
    .stroke(Stroke::new(1.0, Color32::from_rgb(0x36, 0x39, 0x42)))
    .corner_radius(CornerRadius::same(6))
}

/// Polls background Collaborate worker events when Collaborate is enabled.
/// If `s.collaborate_enabled` is false, skips worker polling entirely for zero-overhead.
pub fn update(s: &mut UiState, p: &Project, project_path: Option<&Path>) {
    if !s.collaborate_enabled {
        return;
    }

    let Some(proj_path) = project_path else {
        s.collaborate_worker = None;
        return;
    };

    let repo_dir = proj_path.parent().unwrap_or(proj_path);
    if prowave_vcs::is_daw_source_dir(repo_dir) {
        s.collaborate_worker = None;
        return;
    }

    // Ensure worker is initialized for current project directory
    if let Some(ref worker) = s.collaborate_worker {
        if worker.repo_dir != repo_dir {
            s.collaborate_worker = Some(prowave_vcs::VcsWorker::new(repo_dir.to_path_buf()));
        }
    } else {
        s.collaborate_worker = Some(prowave_vcs::VcsWorker::new(repo_dir.to_path_buf()));
    }

    if let Some(ref mut worker) = s.collaborate_worker {
        while let Some(event) = worker.poll() {
            match event {
                prowave_vcs::VcsEvent::CheckoutCompleted(target) => {
                    s.status = Some(format!("Switched to version / alternative: {target}"));
                }
                prowave_vcs::VcsEvent::Error(_err) => {
                    // Collaborate errors are routed exclusively to the Collaborate window!
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

    let Some(proj_path) = project_path else {
        s.status = Some(tr("Save project to disk before taking snapshots").to_string());
        return;
    };

    let repo_dir = proj_path.parent().unwrap_or(proj_path);
    if prowave_vcs::is_daw_source_dir(repo_dir) {
        return;
    }

    if s.collaborate_worker.is_none() {
        s.collaborate_worker = Some(prowave_vcs::VcsWorker::new(repo_dir.to_path_buf()));
    }

    if let Some(ref worker) = s.collaborate_worker {
        let current_doc = Some(crate::file_commands::content(p));
        let project_file_rel = proj_path.file_name().map(PathBuf::from);

        worker.send(VcsTask::QuickCheckpoint {
            current_doc,
            project_file_rel,
        });
    }
}

/// Dedicated secondary viewport window for Collaborate & Version History.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut UiState, project_path: Option<&Path>) {
    update(s, p, project_path);

    if !s.show_collaborate {
        return;
    }

    let (size, pos) = collaborate_viewport_size(ctx.input(|i| i.viewport().monitor_size));
    let viewport_id = ViewportId::from_hash_of("douri_collaborate_window");
    let mut builder = ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size(vec2(760.0, 520.0))
        .with_resizable(true)
        .with_decorations(false)
        .with_transparent(true)
        .with_has_shadow(false)
        .with_title("Collaborate");

    if let Some(p) = pos {
        builder = builder.with_position(p);
    }

    ctx.show_viewport_immediate(viewport_id, builder, |ctx, _class| {
        let mut visuals = ctx.global_style().visuals.clone();
        visuals.window_shadow = egui::epaint::Shadow::NONE;
        visuals.popup_shadow = egui::epaint::Shadow::NONE;
        ctx.set_visuals(visuals);

        if ctx.input(|i| i.viewport().close_requested()) {
            s.show_collaborate = false;
        }

        let panel_bg = Color32::from_rgb(0x1c, 0x1c, 0x1e);
        let card_bg = Color32::from_rgb(0x25, 0x25, 0x27);
        let border_color = Color32::from_rgb(0x38, 0x38, 0x3c);
        let border_stroke = Stroke::new(1.0, border_color);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(egui::Margin::same(1)),
            )
            .show(ctx, |ui| {
                egui::Frame::new()
                    .corner_radius(CornerRadius::same(10))
                    .fill(panel_bg)
                    .stroke(border_stroke)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        render_window_chrome(ui, s);

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(12.0);

                        if !s.collaborate_enabled {
                            render_paused(ui, s, card_bg, border_stroke);
                            return;
                        }

                        if project_path.is_none() {
                            render_unsaved_project(ui, s, card_bg, border_stroke);
                            return;
                        }

                        let is_repo = s
                            .collaborate_worker
                            .as_ref()
                            .is_some_and(|w| w.latest_state.is_repo);

                        if !is_repo {
                            render_uninitialized(ui, s, card_bg, border_stroke);
                        } else {
                            render_active(ui, p, s, project_path, card_bg, border_stroke);
                        }
                    });
            });
    });
}

fn render_window_chrome(ui: &mut Ui, s: &mut UiState) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(phosphor_egui::regular::USERS)
                .size(18.0)
                .color(Color32::from_rgb(0xe6, 0xe8, 0xec)),
        );
        ui.add_space(4.0);
        ui.strong(
            RichText::new(tr("Collaborate"))
                .size(15.0)
                .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
        );

        ui.add_space(16.0);
        ui.checkbox(&mut s.collaborate_enabled, tr("Enable Collaborate"));
        ui.add_space(8.0);
        let ab_btn = ui.add(
            egui::Button::new(RichText::new("A/B Diff").size(11.0).color(Color32::WHITE))
                .fill(if s.git_ab_audition {
                    Color32::from_rgb(0x2e, 0xa0, 0x43)
                } else {
                    Color32::from_rgb(0x28, 0x2a, 0x32)
                })
                .corner_radius(CornerRadius::same(4)),
        );
        if ab_btn
            .on_hover_text("Audible git diff A/B Switcher")
            .clicked()
        {
            s.git_ab_audition = !s.git_ab_audition;
        }

        // Draggable window header region
        let drag_w = (ui.available_width() - 32.0).max(40.0);
        let (_, drag_resp) = ui.allocate_exact_size(vec2(drag_w, 24.0), Sense::drag());
        if drag_resp.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            let action = crate::ui_chrome::draw_window_controls(ui, is_max);
            match action {
                crate::ui_chrome::WindowControlAction::Minimize => {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                }
                crate::ui_chrome::WindowControlAction::Maximize => {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::Maximized(!is_max));
                }
                crate::ui_chrome::WindowControlAction::Close => {
                    s.show_collaborate = false;
                }
                crate::ui_chrome::WindowControlAction::None => {}
            }
        });
    });
}

fn render_paused(ui: &mut Ui, s: &mut UiState, card_bg: Color32, border_stroke: Stroke) {
    ui.add_space(40.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(24.0)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(phosphor_egui::regular::PAUSE_CIRCLE)
                        .size(40.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(12.0);
                ui.strong(
                    RichText::new(tr("Collaborate Paused"))
                        .size(15.0)
                        .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(tr(
                        "Collaborate & version history is paused to save CPU and disk I/O.",
                    ))
                    .size(12.5)
                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(16.0);

                if ui
                    .add_sized(vec2(200.0, 32.0), primary_btn(&tr("Enable Collaborate")))
                    .clicked()
                {
                    s.collaborate_enabled = true;
                }
            });
        });
}

fn render_unsaved_project(ui: &mut Ui, s: &mut UiState, card_bg: Color32, border_stroke: Stroke) {
    ui.add_space(40.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(24.0)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(phosphor_egui::regular::FLOPPY_DISK)
                        .size(44.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(12.0);
                ui.strong(
                    RichText::new(tr("Save Project First"))
                        .size(16.0)
                        .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(tr(
                        "Please save your project to disk to enable Collaborate, track version history, and connect a Studio Link.",
                    ))
                    .size(12.5)
                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(20.0);

                if ui
                    .add_sized(vec2(200.0, 34.0), primary_btn(&tr("Save Project…")))
                    .clicked()
                {
                    s.file_cmd = Some(crate::file_commands::FileCmd::SaveAs);
                }
            });
        });
}

fn render_uninitialized(ui: &mut Ui, s: &mut UiState, card_bg: Color32, border_stroke: Stroke) {
    ui.add_space(40.0);
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(24.0)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(phosphor_egui::regular::USERS)
                        .size(44.0)
                        .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(12.0);
                ui.strong(
                    RichText::new(tr("Project History"))
                        .size(16.0)
                        .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(tr(
                        "Keep track of all edits, manage project alternatives, and collaborate with your studio team.",
                    ))
                    .size(12.5)
                    .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                );
                ui.add_space(20.0);

                if ui
                    .add_sized(
                        vec2(220.0, 34.0),
                        primary_btn(&tr("Enable Project History")),
                    )
                    .clicked()
                {
                    if let Some(ref worker) = s.collaborate_worker {
                        worker.send(VcsTask::Init);
                    }
                }
            });
        });
}

fn render_active(
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

    // Dismissible Error Banner
    if let Some(ref err) = latest_state.error_msg {
        let mut dismiss = false;
        egui::Frame::new()
            .fill(Color32::from_rgb(0x34, 0x19, 0x19))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x8a, 0x2e, 0x2e)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin {
                left: 10,
                right: 8,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(phosphor_egui::regular::WARNING_CIRCLE)
                            .size(14.0)
                            .color(Color32::from_rgb(0xff, 0x6b, 0x6b)),
                    );
                    ui.add_space(2.0);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            RichText::new("×")
                                .size(14.0)
                                .color(Color32::from_rgb(0xff, 0xaa, 0xaa)),
                        )
                        .frame(false);
                        if ui.add(close_btn).on_hover_text("Dismiss").clicked() {
                            dismiss = true;
                        }

                        let msg_w = (ui.available_width() - 8.0).max(60.0);
                        let label = egui::Label::new(
                            RichText::new(err)
                                .size(11.5)
                                .color(Color32::from_rgb(0xff, 0xaa, 0xaa)),
                        )
                        .truncate()
                        .wrap_mode(egui::TextWrapMode::Truncate);

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add_sized(vec2(msg_w, 18.0), label).on_hover_text(err);
                        });
                    });
                });
            });
        if dismiss {
            if let Some(ref mut worker) = s.collaborate_worker {
                worker.latest_state.error_msg = None;
            }
        }
        ui.add_space(8.0);
    } else if let Some(ref notice) = latest_state.notice_msg {
        let mut dismiss = false;
        egui::Frame::new()
            .fill(Color32::from_rgb(0x18, 0x2b, 0x1f))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x2d, 0x6a, 0x3e)))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin {
                left: 10,
                right: 8,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(phosphor_egui::regular::CHECK_CIRCLE)
                            .size(14.0)
                            .color(Color32::from_rgb(0x4e, 0xcd, 0x76)),
                    );
                    ui.add_space(2.0);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            RichText::new("×")
                                .size(14.0)
                                .color(Color32::from_rgb(0x9a, 0xeb, 0xb2)),
                        )
                        .frame(false);
                        if ui.add(close_btn).on_hover_text("Dismiss").clicked() {
                            dismiss = true;
                        }

                        let msg_w = (ui.available_width() - 8.0).max(60.0);
                        let label = egui::Label::new(
                            RichText::new(notice)
                                .size(11.5)
                                .color(Color32::from_rgb(0x9a, 0xeb, 0xb2)),
                        )
                        .truncate()
                        .wrap_mode(egui::TextWrapMode::Truncate);

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add_sized(vec2(msg_w, 18.0), label).on_hover_text(notice);
                        });
                    });
                });
            });
        if dismiss {
            if let Some(ref mut worker) = s.collaborate_worker {
                worker.latest_state.notice_msg = None;
            }
        }
        ui.add_space(8.0);
    }

    let total_w = ui.available_width();
    let gap = 16.0;
    let left_w = ((total_w - gap) * 0.42).clamp(310.0, 480.0);
    let right_w = total_w - gap - left_w;

    ui.horizontal_top(|ui| {
        // Left Column (~42%)
        ui.vertical(|ui| {
            ui.set_width(left_w);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_left_column(
                        ui,
                        p,
                        s,
                        project_path,
                        &latest_state,
                        card_bg,
                        border_stroke,
                    );
                });
        });

        ui.add_space(gap);

        // Right Column (~58%)
        ui.vertical(|ui| {
            ui.set_width(right_w);
            render_right_column(
                ui,
                p,
                s,
                project_path,
                &latest_state,
                card_bg,
                border_stroke,
            );
        });
    });
}

fn render_left_column(
    ui: &mut Ui,
    p: &mut Project,
    s: &mut UiState,
    project_path: Option<&Path>,
    latest_state: &VcsUiState,
    card_bg: Color32,
    border_stroke: Stroke,
) {
    // Alternatives & Status Header
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
                .size(12.0)
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

    ui.add_space(6.0);

    // Alternative Selector
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Alternative:"))
                .size(12.0)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
        egui::ComboBox::from_id_salt("collaborate_alt_selector_window")
            .selected_text(
                RichText::new(&latest_state.current_branch)
                    .size(12.0)
                    .strong(),
            )
            .width(180.0)
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
                    if ui.add(compact_secondary_btn(&tr("Create"))).clicked() {
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

    ui.add_space(8.0);

    // Sync Action Row
    ui.horizontal(|ui| {
        if ui.add(secondary_btn(&tr("Get Updates"))).clicked() {
            if let Some(ref worker) = s.collaborate_worker {
                worker.send(VcsTask::Pull);
            }
        }

        if ui.add(secondary_btn(&tr("Share Updates"))).clicked() {
            if let Some(ref worker) = s.collaborate_worker {
                worker.send(VcsTask::Push);
            }
        }

        if ui.add(secondary_btn(&tr("Studio Link…"))).clicked() {
            s.collaborate_showing_link_modal = !s.collaborate_showing_link_modal;
            if s.collaborate_showing_link_modal {
                if let Some(ref url) = latest_state.remote_url {
                    s.collaborate_link_input = url.clone();
                } else {
                    s.collaborate_link_input.clear();
                }
            }
        }
    });

    // Inline Studio Link Form
    if s.collaborate_showing_link_modal {
        ui.add_space(4.0);
        egui::Frame::new()
            .fill(card_bg)
            .stroke(border_stroke)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                if let Some(ref current_link) = latest_state.remote_url {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{}: {current_link}", tr("Current Link")))
                                .size(11.0)
                                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
                        );
                        if ui.add(compact_secondary_btn(&tr("Remove Link"))).clicked() {
                            if let Some(ref worker) = s.collaborate_worker {
                                worker.send(VcsTask::RemoveRemote);
                            }
                            s.collaborate_link_input.clear();
                        }
                    });
                    ui.add_space(6.0);
                }

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
                    let valid = norm_res.is_ok();
                    if ui
                        .add_enabled(valid, primary_btn(&tr("Save Link")))
                        .clicked()
                    {
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

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    // Save Version Card
    egui::Frame::new()
        .fill(card_bg)
        .stroke(border_stroke)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr("Save Version"))
                    .size(12.5)
                    .strong()
                    .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
            );
            ui.add_space(4.0);

            ui.add(
                egui::TextEdit::singleline(&mut s.collaborate_note_input)
                    .hint_text(tr("What changed? (e.g., Tightened chorus drums)…"))
                    .desired_width(ui.available_width()),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let has_msg = !s.collaborate_note_input.trim().is_empty();
                if ui
                    .add_enabled(has_msg, primary_btn(&tr("Save Version")))
                    .clicked()
                {
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

                if ui
                    .add(secondary_btn(&tr("Quick Snapshot")))
                    .on_hover_text("Ctrl+Alt+S / Cmd+Alt+S")
                    .clicked()
                {
                    quick_snapshot(s, p, project_path);
                }
            });
        });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(10.0);

    // Recent Edits Panel
    let num_changes = latest_state.changes.len();
    ui.horizontal(|ui| {
        ui.strong(
            RichText::new(format!("{} ({num_changes})", tr("Recent Edits")))
                .size(12.5)
                .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
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
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for change in &latest_state.changes {
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
                    });
            });
    }
}

fn render_right_column(
    ui: &mut Ui,
    _p: &mut Project,
    s: &mut UiState,
    _project_path: Option<&Path>,
    latest_state: &VcsUiState,
    card_bg: Color32,
    border_stroke: Stroke,
) {
    let commits = &latest_state.commits;
    ui.strong(
        RichText::new(format!("{} ({})", tr("Version History"), commits.len()))
            .size(13.0)
            .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
    );
    ui.add_space(6.0);

    // Inline "New Alternative from Version"
    if let Some(from_rev) = s.collaborate_branch_from_rev.clone() {
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
                            .desired_width(180.0),
                    );
                    if ui.add(primary_btn(&tr("Create Alternative"))).clicked() {
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
        ui.add_space(8.0);
    }

    if commits.is_empty() {
        ui.label(
            RichText::new(tr("No versions saved yet."))
                .size(12.0)
                .color(Color32::from_rgb(0x8e, 0x8e, 0x93)),
        );
    } else {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for commit in commits {
                    egui::Frame::new()
                        .fill(card_bg)
                        .stroke(border_stroke)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(compact_secondary_btn(&tr(
                                                "New Alternative from Here",
                                            )))
                                            .clicked()
                                        {
                                            s.collaborate_branch_from_rev =
                                                Some(commit.hash.clone());
                                            s.collaborate_alt_input.clear();
                                        }

                                        if ui
                                            .add(compact_secondary_btn(&tr("Open Version")))
                                            .clicked()
                                        {
                                            if let Some(ref worker) = s.collaborate_worker {
                                                worker.send(VcsTask::Checkout {
                                                    target: commit.hash.clone(),
                                                });
                                            }
                                        }

                                        let label = egui::Label::new(
                                            RichText::new(&commit.message)
                                                .size(12.5)
                                                .strong()
                                                .color(Color32::from_rgb(0xf2, 0xf3, 0xf5)),
                                        )
                                        .truncate()
                                        .wrap_mode(egui::TextWrapMode::Truncate);

                                        let title_w = (ui.available_width() - 8.0).max(40.0);
                                        ui.with_layout(
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add_sized(vec2(title_w, 20.0), label)
                                                    .on_hover_text(&commit.message);
                                            },
                                        );
                                    },
                                );
                            });

                            ui.add_space(2.0);

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
                            });
                        });
                    ui.add_space(4.0);
                }
            });
    }
}
