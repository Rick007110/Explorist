//! Bottom-center toast above the status bar. One at a time; a new one
//! replaces the old. Undoable actions toast with Undo instead of asking
//! for confirmation first.

use egui::{Align2, Area, CornerRadius, Frame, Id, Margin, Order, Pos2, Sense, Stroke, vec2};

use crate::app::{ExploristApp, ToastKind};
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, size, text};
use crate::ui::widgets;

/// Shows the toast, if any. Returns whether the pointer is over it, which
/// pauses its timer.
pub fn show(ctx: &egui::Context, app: &mut ExploristApp) -> bool {
    let p = app.palette;
    let Some(toast) = &app.view.toast else {
        return false;
    };
    let message = toast.message.clone();
    let kind = toast.kind;
    let undo = toast.undo;

    let mut dismiss = false;
    let mut run_undo = false;
    let response = Area::new(Id::new("toast"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -(size::STATUS_BAR + 16.0)))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(p.raised)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(10))
                .shadow(p.shadow_float())
                .inner_margin(Margin {
                    left: 14,
                    right: 8,
                    top: 8,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let (icon, color) = match kind {
                            ToastKind::Success => (Icon::Check, p.success),
                            ToastKind::Error => (Icon::Warning, p.error),
                        };
                        let (rect, _) = ui.allocate_exact_size(vec2(16.0, 28.0), Sense::hover());
                        icons::paint(ui.painter(), rect, icon, 16.0, color);

                        let galley =
                            ui.painter()
                                .layout(message.clone(), text::body(), p.text, 560.0);
                        let (rect, _) = ui.allocate_exact_size(
                            galley.size().max(vec2(0.0, 28.0)),
                            Sense::hover(),
                        );
                        ui.painter().galley(
                            Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0),
                            galley,
                            p.text,
                        );

                        if undo {
                            let kbd = widgets::kbd_galley(ui, &p, "Ctrl Z");
                            let label = ui.painter().layout_no_wrap(
                                "Undo".to_owned(),
                                crate::ui::fonts::semibold(12.5),
                                p.accent,
                            );
                            let w = 10.0 + label.size().x + 8.0 + widgets::kbd_width(&kbd) + 8.0;
                            let (rect, r) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
                            let tint = p.accent_alpha(if p.dark { 0.14 } else { 0.10 });
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(radius::SM),
                                if r.hovered() {
                                    p.accent_alpha(0.24)
                                } else {
                                    tint
                                },
                            );
                            let label_w = label.size().x;
                            ui.painter().galley(
                                Pos2::new(
                                    rect.left() + 10.0,
                                    rect.center().y - label.size().y / 2.0,
                                ),
                                label,
                                p.accent,
                            );
                            widgets::paint_kbd(
                                ui.painter(),
                                &p,
                                kbd,
                                rect.left() + 10.0 + label_w + 8.0,
                                rect.center().y,
                                false,
                            );
                            run_undo = r.clicked();
                        }
                        dismiss = widgets::icon_button_sized(
                            ui,
                            &p,
                            Icon::Close,
                            "Dismiss",
                            true,
                            false,
                            24.0,
                            12.0,
                        )
                        .clicked();
                    });
                });
        })
        .response;

    if run_undo {
        app.view.toast = None;
        app.undo();
    } else if dismiss {
        app.view.toast = None;
    }
    response.hovered() || response.contains_pointer()
}
