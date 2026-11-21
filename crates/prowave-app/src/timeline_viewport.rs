//! Unified Timeline Viewport Controller & Interaction Engine (`timeline_viewport.rs`).
//!
//! Provides cursor-anchored 2D zoom, middle-click panning, and gesture management
//! across Tracks Area, Piano Roll, Step Sequencer, and linear editors.

use egui::{vec2, CursorIcon, Modifiers, Pos2, Rect, Ui, Vec2};

/// Explicit locked gestures during mouse drags so fast pointer movements
/// never drop note edges, slip off scrubbers, or flicker cursors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActiveGesture {
    #[default]
    None,
    MiddlePan,
    ScrubRuler,
    MoveNotes,
    ResizeNoteLeft,
    ResizeNoteRight,
    EditVelocity,
    Marquee,
}

/// A 2D canvas viewport with horizontal time/beats and vertical tracks/pitches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineViewport {
    /// Horizontal scroll offset in points/pixels.
    pub scroll_x: f32,
    /// Vertical scroll offset in points/pixels.
    pub scroll_y: f32,
    /// Horizontal zoom factor (points per beat or points per bar).
    pub zoom_x: f32,
    /// Vertical row height (track height or note height).
    pub row_h: f32,
}

impl TimelineViewport {
    pub fn new(scroll_x: f32, scroll_y: f32, zoom_x: f32, row_h: f32) -> Self {
        Self {
            scroll_x,
            scroll_y,
            zoom_x: zoom_x.max(1.0),
            row_h: row_h.max(2.0),
        }
    }

    /// Zooms horizontally keeping the beat under `cursor_x` pinned in place.
    pub fn zoom_horizontal_anchored(
        &mut self,
        cursor_x: f32,
        canvas_left: f32,
        factor: f32,
        min_zoom: f32,
        max_zoom: f32,
    ) {
        if self.zoom_x <= 0.0 || factor <= 0.0 {
            return;
        }
        let anchor_beat = (cursor_x - canvas_left + self.scroll_x) / self.zoom_x;
        let new_zoom = (self.zoom_x * factor).clamp(min_zoom, max_zoom);
        self.scroll_x = (anchor_beat * new_zoom - (cursor_x - canvas_left)).max(0.0);
        self.zoom_x = new_zoom;
    }

    /// Zooms vertically keeping the row/pitch under `cursor_y` pinned in place.
    pub fn zoom_vertical_anchored(
        &mut self,
        cursor_y: f32,
        canvas_top: f32,
        factor: f32,
        min_h: f32,
        max_h: f32,
    ) {
        if self.row_h <= 0.0 || factor <= 0.0 {
            return;
        }
        let anchor_row = (cursor_y - canvas_top + self.scroll_y) / self.row_h;
        let new_h = (self.row_h * factor).clamp(min_h, max_h);
        self.scroll_y = (anchor_row * new_h - (cursor_y - canvas_top)).max(0.0);
        self.row_h = new_h;
    }

    /// Handles middle-mouse 2D panning with `CursorIcon::Grabbing`.
    /// Returns true if middle-click panning is active.
    pub fn handle_middle_pan(
        &mut self,
        ui: &Ui,
        canvas: Rect,
        max_scroll_y: f32,
        gesture: &mut ActiveGesture,
    ) -> bool {
        let (middle_down, hover_pos, delta) = ui.input(|i| {
            (
                i.pointer.middle_down(),
                i.pointer.hover_pos(),
                i.pointer.delta(),
            )
        });

        if middle_down {
            if *gesture == ActiveGesture::None && hover_pos.is_some_and(|p| canvas.contains(p)) {
                *gesture = ActiveGesture::MiddlePan;
            }
        } else if *gesture == ActiveGesture::MiddlePan {
            *gesture = ActiveGesture::None;
        }

        if *gesture == ActiveGesture::MiddlePan {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            self.scroll_x = (self.scroll_x - delta.x).max(0.0);
            self.scroll_y = (self.scroll_y - delta.y).clamp(0.0, max_scroll_y.max(0.0));
            true
        } else {
            false
        }
    }

    /// Handles Wayland-safe wheel zooming and 2D scrolling.
    pub fn handle_wheel_zoom_and_pan(
        &mut self,
        ui: &Ui,
        canvas: Rect,
        max_scroll_y: f32,
        min_zoom: f32,
        max_zoom: f32,
        min_h: f32,
        max_h: f32,
    ) {
        let hover = ui.input(|i| i.pointer.hover_pos());
        let Some(pt) = hover.filter(|p| canvas.contains(*p)) else {
            return;
        };

        let (delta, mods) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers));
        if delta == Vec2::ZERO {
            return;
        }

        let is_ctrl = mods.ctrl || mods.command;
        let is_alt = mods.alt;

        // Vertical zoom (Alt + Wheel or Ctrl + Shift + Wheel)
        if (is_alt && delta.y != 0.0) || (is_ctrl && mods.shift && delta.y != 0.0) {
            let factor = 1.0 + delta.y * 0.003;
            self.zoom_vertical_anchored(pt.y, canvas.top(), factor, min_h, max_h);
        }
        // Horizontal zoom (Ctrl / Cmd + Wheel)
        else if is_ctrl && delta.y != 0.0 {
            let factor = 1.0 + delta.y * 0.003;
            self.zoom_horizontal_anchored(pt.x, canvas.left(), factor, min_zoom, max_zoom);
        }
        // Horizontal scroll (Shift + Wheel or horizontal trackpad scroll)
        else if mods.shift || delta.x != 0.0 {
            let dx = if mods.shift && delta.x == 0.0 {
                delta.y
            } else {
                delta.x
            };
            self.scroll_x = (self.scroll_x - dx).max(0.0);
        }
        // Plain Wheel: vertical scroll
        else if delta.y != 0.0 {
            self.scroll_y = (self.scroll_y - delta.y).clamp(0.0, max_scroll_y.max(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_anchored_horizontal_zoom_keeps_beat_pinned() {
        let mut vp = TimelineViewport::new(100.0, 0.0, 50.0, 20.0);
        let cursor_x = 350.0;
        let canvas_left = 50.0;

        // Before zoom: cursor is at 350 - 50 = 300px from left edge.
        // Beat under cursor = (300 + 100) / 50 = 8.0 beats.
        let beat_before = (cursor_x - canvas_left + vp.scroll_x) / vp.zoom_x;
        assert_eq!(beat_before, 8.0);

        // Zoom in by 1.5x
        vp.zoom_horizontal_anchored(cursor_x, canvas_left, 1.5, 10.0, 500.0);
        assert_eq!(vp.zoom_x, 75.0);

        // After zoom: beat under cursor must STILL be exactly 8.0!
        let beat_after = (cursor_x - canvas_left + vp.scroll_x) / vp.zoom_x;
        assert!((beat_after - 8.0).abs() < 1e-5);
    }

    #[test]
    fn cursor_anchored_vertical_zoom_keeps_row_pinned() {
        let mut vp = TimelineViewport::new(0.0, 40.0, 50.0, 20.0);
        let cursor_y = 140.0;
        let canvas_top = 20.0;

        // Before zoom: cursor is at 140 - 20 = 120px from top edge.
        // Row under cursor = (120 + 40) / 20 = 8.0 rows.
        let row_before = (cursor_y - canvas_top + vp.scroll_y) / vp.row_h;
        assert_eq!(row_before, 8.0);

        // Zoom in by 1.25x
        vp.zoom_vertical_anchored(cursor_y, canvas_top, 1.25, 10.0, 100.0);
        assert_eq!(vp.row_h, 25.0);

        // After zoom: row under cursor must STILL be exactly 8.0!
        let row_after = (cursor_y - canvas_top + vp.scroll_y) / vp.row_h;
        assert!((row_after - 8.0).abs() < 1e-5);
    }

    #[test]
    fn scroll_offsets_never_fall_below_zero() {
        let mut vp = TimelineViewport::new(10.0, 10.0, 50.0, 20.0);
        vp.scroll_x = (vp.scroll_x - 50.0).max(0.0);
        vp.scroll_y = (vp.scroll_y - 50.0).max(0.0);
        assert_eq!(vp.scroll_x, 0.0);
        assert_eq!(vp.scroll_y, 0.0);
    }
}
