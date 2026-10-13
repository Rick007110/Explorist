//! Resizing the undecorated window from its edges.

use egui::{CursorIcon, ResizeDirection, ViewportCommand};

/// How close to an edge the pointer must be to resize.
const GRIP: f32 = 5.0;

/// Lets the user resize the window by dragging its edges and corners, since
/// the custom title bar replaces the system frame.
pub fn resize_handles(ctx: &egui::Context, maximized: bool) {
    if maximized {
        return;
    }
    let screen = ctx.content_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let left = pos.x - screen.left() < GRIP;
    let right = screen.right() - pos.x < GRIP;
    let top = pos.y - screen.top() < GRIP;
    let bottom = screen.bottom() - pos.y < GRIP;

    let direction = match (left, right, top, bottom) {
        (true, _, true, _) => ResizeDirection::NorthWest,
        (_, true, true, _) => ResizeDirection::NorthEast,
        (true, _, _, true) => ResizeDirection::SouthWest,
        (_, true, _, true) => ResizeDirection::SouthEast,
        (true, ..) => ResizeDirection::West,
        (_, true, ..) => ResizeDirection::East,
        (_, _, true, _) => ResizeDirection::North,
        (_, _, _, true) => ResizeDirection::South,
        _ => return,
    };
    let cursor = match direction {
        ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
        ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
    };
    ctx.set_cursor_icon(cursor);
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}
