//! The Marker track (Logic Pro: Global Tracks > Marker), shown above the tracks with Track > Show
//! Marker Track. Each marker is a named, coloured section from its start to the next marker.
//!
//! - Double-click an empty spot (or Navigate > Create Marker): new marker there.
//! - Click a marker: the playhead goes to its start. Drag it to move it (snapped to the grid).
//! - Double-click a marker, or right-click > Rename Marker: edit its name.
//! - Right-click: colour, or Delete Marker.
use crate::i18n::tr;
use crate::model::{Project, MARKER_COLORS};
use crate::theme;
use crate::widgets::context_menu;
use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};

/// Height of the Marker track, in points.
pub const HEIGHT: f32 = 24.0;

const BG: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
const HEADER_BG: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3a);

/// What a right-click or double-click asked for, run after drawing.
enum Action {
    Rename(usize),
    Color(usize, Color32),
    Delete(usize),
}

/// The id of the + key in the header (tests find it on screen with this).
pub(crate) fn add_key_id() -> egui::Id {
    egui::Id::unique("marker_track_add")
}

/// The id of marker `i` on screen.
pub(crate) fn marker_id(i: usize) -> egui::Id {
    egui::Id::unique(("marker", i))
}

/// Draws the Marker track's `header` and `lane` and handles its editing. `x_of` / `beat_of`
/// convert between beats and screen x as in the Tracks area; `snap` snaps a position.
/// `selected` is the selected marker. Returns whether a marker was selected now (the Tracks
/// area then deselects its regions).
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    p: &mut Project,
    header: Rect,
    lane: Rect,
    x_of: impl Fn(f64) -> f32,
    beat_of: impl Fn(f32) -> f64,
    snap: &dyn Fn(f64) -> f64,
    selected: &mut Option<usize>,
) -> bool {
    let mut picked = false;
    let rename_id = egui::Id::unique("marker_track_rename");
    let grab_id = egui::Id::unique("marker_track_grab");

    // ---- header
    let gh = ui.painter_at(header);
    gh.rect_filled(header, 0.0, HEADER_BG);
    gh.line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(0x22)),
    );
    gh.text(
        pos2(header.left() + 12.0, header.center().y),
        Align2::LEFT_CENTER,
        tr("Marker"),
        FontId::proportional(13.0),
        Color32::from_gray(0xdc),
    );
    if let Some(i) = p.marker_at(p.position_beats) {
        gh.text(
            pos2(header.right() - 32.0, header.center().y),
            Align2::RIGHT_CENTER,
            &p.markers[i].name,
            FontId::proportional(11.0),
            Color32::from_gray(0xa8),
        );
    }

    // The + key (Logic's Marker track header): Create Marker at the playhead.
    let add = Rect::from_center_size(
        pos2(header.right() - 14.0, header.center().y),
        vec2(20.0, 20.0),
    );
    let add_resp = ui.interact(add, add_key_id(), Sense::click());
    gh.text(
        add.center(),
        Align2::CENTER_CENTER,
        "+",
        FontId::proportional(14.0),
        Color32::from_gray(0xdc),
    );
    if add_resp.clicked() {
        *selected = Some(crate::marker_edit::create(p, p.position_beats, true));
        picked = true;
    }

    // ---- lane: double-click an empty spot to add a marker
    let g = ui.painter_at(lane);
    g.rect_filled(lane, 0.0, BG);
    let lane_resp = ui.interact(lane, egui::Id::unique("marker_track_lane"), Sense::click());
    if lane_resp.double_clicked() {
        if let Some(pt) = lane_resp.interact_pointer_pos() {
            let i = p.add_marker(snap(beat_of(pt.x)));
            *selected = Some(i);
            picked = true;
            ui.ctx()
                .data_mut(|m| m.insert_temp(rename_id, (i, p.markers[i].name.clone())));
        }
    }

    // ---- markers: each one runs to the next
    let mut action = None;
    let mut moved = false;
    for i in 0..p.markers.len() {
        let start = p.markers[i].beat;
        let x0 = x_of(start);
        let x1 = p
            .markers
            .get(i + 1)
            .map_or(lane.right() + 1.0, |n| x_of(n.beat));
        if x1 < lane.left() || x0 > lane.right() {
            continue;
        }
        let block = Rect::from_min_max(pos2(x0, lane.top() + 2.0), pos2(x1, lane.bottom() - 2.0));
        let resp = ui.interact(
            block.intersect(lane),
            egui::Id::unique(("marker", i)),
            Sense::click_and_drag(),
        );
        if resp.clicked() || resp.drag_started() {
            if resp.clicked() {
                p.position_beats = start;
            }
            *selected = Some(i);
            picked = true;
        }
        if resp.double_clicked() {
            action = Some(Action::Rename(i));
        }
        if resp.drag_started() {
            if let Some(pt) = resp.interact_pointer_pos() {
                ui.ctx()
                    .data_mut(|m| m.insert_temp(grab_id, beat_of(pt.x) - start));
            }
        }
        if resp.dragged() {
            if let Some(pt) = resp.interact_pointer_pos() {
                let grab = ui.ctx().data(|m| m.get_temp::<f64>(grab_id)).unwrap_or(0.0);
                p.markers[i].beat = snap((beat_of(pt.x) - grab).max(0.0));
            }
        }
        if resp.drag_stopped() {
            moved = true;
        }
        context_menu(&resp, |ui| {
            if ui.button(tr("Rename Marker")).clicked() {
                action = Some(Action::Rename(i));
            }
            ui.horizontal(|ui| {
                for c in MARKER_COLORS {
                    let (r, sw) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                    ui.painter().rect_filled(r.shrink(1.0), 3.0, c);
                    if sw.clicked() {
                        action = Some(Action::Color(i, c));
                    }
                }
            });
            ui.separator();
            if ui.button(tr("Delete Marker")).clicked() {
                action = Some(Action::Delete(i));
            }
        });

        let m = &p.markers[i];
        let current = p.marker_at(p.position_beats) == Some(i);
        let is_selected = *selected == Some(i);
        let fill = if is_selected {
            theme::ORANGE
        } else if current {
            m.color
        } else {
            m.color.gamma_multiply(0.7)
        };
        g.rect_filled(block, 3.0, fill);
        g.line_segment(
            [pos2(x0, lane.top()), pos2(x0, lane.bottom())],
            Stroke::new(1.5, m.color),
        );
        let label = g.layout_no_wrap(m.name.clone(), FontId::proportional(12.0), Color32::WHITE);
        let text_x = x0.max(lane.left()) + 5.0;
        if text_x + 10.0 < x1 {
            g.with_clip_rect(block.shrink(1.0).intersect(lane)).galley(
                pos2(text_x, block.center().y - label.size().y / 2.0),
                label,
                Color32::WHITE,
            );
        }
    }
    if moved {
        // Sorting may give the selected marker another index: follow it.
        let held = selected
            .and_then(|i| p.markers.get(i))
            .map(|m| (m.beat, m.name.clone()));
        p.tidy_markers();
        if let Some((beat, name)) = held {
            *selected = p
                .markers
                .iter()
                .position(|m| (m.beat - beat).abs() <= 1e-6 && m.name == name);
        }
    }

    match action {
        Some(Action::Rename(i)) => {
            let name = p.markers[i].name.clone();
            ui.ctx().data_mut(|m| m.insert_temp(rename_id, (i, name)));
        }
        Some(Action::Color(i, c)) => p.markers[i].color = c,
        Some(Action::Delete(i)) => {
            p.markers.remove(i);
            *selected = None;
        }
        None => {}
    }

    // ---- renaming: a text field over the marker; Enter or clicking away keeps it
    if let Some((i, mut text)) = ui.ctx().data(|m| m.get_temp::<(usize, String)>(rename_id)) {
        let Some(m) = p.markers.get(i) else {
            ui.ctx()
                .data_mut(|m| m.remove::<(usize, String)>(rename_id));
            return picked;
        };
        let x0 = x_of(m.beat).max(lane.left());
        let r = Rect::from_min_size(pos2(x0, lane.top() + 1.0), vec2(160.0, HEIGHT - 2.0));
        let edit = ui.put(
            r,
            egui::TextEdit::singleline(&mut text)
                .id(rename_id.with("field"))
                .char_limit(64)
                .font(FontId::proportional(12.0)),
        );
        if !edit.has_focus() && !edit.lost_focus() {
            edit.request_focus();
        }
        let (enter, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if escape {
            ui.ctx()
                .data_mut(|m| m.remove::<(usize, String)>(rename_id));
        } else if enter || edit.lost_focus() {
            let name = text.trim();
            if !name.is_empty() {
                p.markers[i].name = name.to_string();
            }
            ui.ctx()
                .data_mut(|m| m.remove::<(usize, String)>(rename_id));
        } else {
            ui.ctx().data_mut(|m| m.insert_temp(rename_id, (i, text)));
        }
    }

    // ---- playhead
    let x = x_of(p.position_beats);
    if x >= lane.left() && x <= lane.right() {
        g.line_segment(
            [pos2(x, lane.top()), pos2(x, lane.bottom())],
            Stroke::new(1.0, Color32::WHITE),
        );
    }
    g.line_segment(
        [lane.left_bottom(), lane.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(0x1a)),
    );
    picked
}
