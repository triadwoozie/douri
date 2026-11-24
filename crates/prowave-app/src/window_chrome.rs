//! Windows and Linux: the window has no title bar from the system (on Linux, GNOME and other
//! Wayland desktops would draw a tall header above the app). The menu bar row is the top of the
//! window instead: minimize, maximize and close buttons sit at its right end, dragging its
//! empty part moves the window (snapping to screen edges like a title bar), double-clicking it
//! maximizes, and the window's edges resize it.
//!
//! macOS keeps its own window buttons and the menu bar at the top of the screen.
use crate::i18n::tr;
use egui::viewport::ResizeDirection;
use egui::{pos2, vec2, Color32, CursorIcon, Pos2, Rect, Sense, Stroke, ViewportCommand};

/// Whether Pro Wave draws its own window controls (everywhere but macOS).
pub const CUSTOM: bool = !cfg!(target_os = "macos");

/// Width of each window button and of the grab zone along the window's edges.
const BUTTON_W: f32 = 40.0;
const EDGE: f32 = 5.0;

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

/// Draws the window buttons at the right end of `bar` and turns the free part of the bar
/// (from `free_left`) into the window's drag handle. `fg` is the bar's text colour.
pub fn bar_controls(ui: &mut egui::Ui, bar: Rect, free_left: f32, _fg: Color32) {
    let ctx = ui.ctx().clone();
    let is_max = maximized(&ctx);
    let controls_w = 3.0 * 28.0 + 2.0 * 4.0;
    let buttons_left = bar.right() - controls_w - 6.0;

    // Free part of the bar: drag to move, double-click to maximize or restore.
    if buttons_left > free_left {
        let free = Rect::from_min_max(pos2(free_left, bar.top()), pos2(buttons_left, bar.bottom()));
        let resp = ui.interact(
            free,
            egui::Id::unique("window_drag"),
            Sense::click_and_drag(),
        );
        if resp.double_clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!is_max));
        } else if resp.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }

    let controls_rect = Rect::from_min_size(
        pos2(buttons_left, bar.center().y - 13.0),
        vec2(controls_w, 26.0),
    );
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(controls_rect));
    let action = crate::ui_chrome::draw_window_controls(&mut child_ui, is_max);
    match action {
        crate::ui_chrome::WindowControlAction::Minimize => {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }
        crate::ui_chrome::WindowControlAction::Maximize => {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!is_max));
        }
        crate::ui_chrome::WindowControlAction::Close => {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        crate::ui_chrome::WindowControlAction::None => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Button {
    Minimize,
    Maximize,
    Close,
}

impl Button {
    fn label(self, maximized: bool) -> &'static str {
        match self {
            Button::Minimize => tr("Minimize"),
            Button::Maximize if maximized => tr("Restore"),
            Button::Maximize => tr("Maximize"),
            Button::Close => tr("Close"),
        }
    }

    /// The usual thin-line glyphs: a dash, a square (two overlapping when maximized), a cross.
    fn draw(self, g: &egui::Painter, c: Pos2, ink: Color32, maximized: bool) {
        let s = Stroke::new(1.0, ink);
        match self {
            Button::Minimize => {
                g.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], s);
            }
            Button::Maximize if maximized => {
                g.rect_stroke(
                    Rect::from_center_size(c + vec2(-1.5, 1.5), vec2(8.0, 8.0)),
                    0.0,
                    s,
                    egui::StrokeKind::Middle,
                );
                g.line_segment([c + vec2(-2.5, -4.5), c + vec2(4.5, -4.5)], s);
                g.line_segment([c + vec2(4.5, -4.5), c + vec2(4.5, 2.5)], s);
            }
            Button::Maximize => {
                g.rect_stroke(
                    Rect::from_center_size(c, vec2(10.0, 10.0)),
                    0.0,
                    s,
                    egui::StrokeKind::Middle,
                );
            }
            Button::Close => {
                g.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], s);
                g.line_segment([c + vec2(5.0, -5.0), c + vec2(-5.0, 5.0)], s);
            }
        }
    }
}

/// Which window edge or corner `p` is on, within `edge` points of `window`'s border.
fn edge_at(p: Pos2, window: Rect, edge: f32) -> Option<ResizeDirection> {
    if !window.expand(0.5).contains(p) {
        return None;
    }
    let left = p.x - window.left() <= edge;
    let right = window.right() - p.x <= edge;
    let top = p.y - window.top() <= edge;
    let bottom = window.bottom() - p.y <= edge;
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (_, _, _, true) => ResizeDirection::South,
        _ => return None,
    })
}

fn cursor(dir: ResizeDirection) -> CursorIcon {
    match dir {
        ResizeDirection::North => CursorIcon::ResizeNorth,
        ResizeDirection::South => CursorIcon::ResizeSouth,
        ResizeDirection::East => CursorIcon::ResizeEast,
        ResizeDirection::West => CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
    }
}

/// Lets the window's edges resize it (it has no system frame to grab). Call once per frame,
/// after the panels, so the resize cursor wins over theirs.
pub fn resize_edges(ctx: &egui::Context) {
    if !CUSTOM || maximized(ctx) {
        return;
    }
    let (pos, pressed) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_pressed()));
    let Some(dir) = pos.and_then(|p| edge_at(p, ctx.content_rect(), EDGE)) else {
        return;
    };
    ctx.set_cursor_icon(cursor(dir));
    if pressed {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_corners_are_found_and_the_inside_is_not() {
        let w = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0));
        assert_eq!(edge_at(pos2(500.0, 400.0), w, EDGE), None);
        assert_eq!(
            edge_at(pos2(2.0, 400.0), w, EDGE),
            Some(ResizeDirection::West)
        );
        assert_eq!(
            edge_at(pos2(998.0, 400.0), w, EDGE),
            Some(ResizeDirection::East)
        );
        assert_eq!(
            edge_at(pos2(500.0, 1.0), w, EDGE),
            Some(ResizeDirection::North)
        );
        assert_eq!(
            edge_at(pos2(500.0, 799.0), w, EDGE),
            Some(ResizeDirection::South)
        );
        assert_eq!(
            edge_at(pos2(1.0, 1.0), w, EDGE),
            Some(ResizeDirection::NorthWest)
        );
        assert_eq!(
            edge_at(pos2(999.0, 799.0), w, EDGE),
            Some(ResizeDirection::SouthEast)
        );
        assert_eq!(
            edge_at(pos2(1200.0, 400.0), w, EDGE),
            None,
            "outside the window"
        );
    }

    #[test]
    fn the_maximize_button_says_what_it_will_do() {
        assert_eq!(Button::Maximize.label(false), "Maximize");
        assert_eq!(Button::Maximize.label(true), "Restore");
    }
}
