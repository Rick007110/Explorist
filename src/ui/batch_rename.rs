//! The batch rename dialog (Ctrl F2): pick a rule, check the preview,
//! rename. One Ctrl Z undoes the whole batch.

use std::path::PathBuf;

use egui::{
    Align2, Area, CornerRadius, Frame, Id, Key, Margin, Modifiers, Order, Pos2, Rect, ScrollArea,
    Sense, Stroke, vec2,
};

use crate::app::ExploristApp;
use crate::fs::batch::{self, CaseStyle, DateSource, Options, Planned, Rule, Status};
use crate::fs::entry::Entry;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius, text};
use crate::ui::widgets::{self, ButtonStyle};

const WIDTH: f32 = 820.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Template,
    Replace,
    Case,
    Regex,
}

/// The dialog's state while open.
pub struct BatchState {
    items: Vec<Entry>,
    folder_names: Vec<String>,
    mode: Mode,
    template: String,
    start: String,
    date: DateSource,
    find: String,
    replace: String,
    match_case: bool,
    case: CaseStyle,
    pattern: String,
    regex_replace: String,
    options: Options,
    /// Put the cursor in the mode's main field next frame.
    focus: bool,
}

impl BatchState {
    pub fn new(items: Vec<Entry>, folder_names: Vec<String>) -> Self {
        Self {
            items,
            folder_names,
            mode: Mode::Template,
            template: "{name}{ext}".to_owned(),
            start: "1".to_owned(),
            date: DateSource::Modified,
            find: String::new(),
            replace: String::new(),
            match_case: false,
            case: CaseStyle::Lower,
            pattern: String::new(),
            regex_replace: String::new(),
            options: Options {
                lowercase_ext: false,
                include_folders: true,
            },
            focus: true,
        }
    }

    fn rule(&self) -> Rule {
        match self.mode {
            Mode::Template => Rule::Template {
                template: self.template.clone(),
                start: self.start.trim().parse().unwrap_or(1),
                date: self.date,
            },
            Mode::Replace => Rule::Replace {
                find: self.find.clone(),
                replace: self.replace.clone(),
                match_case: self.match_case,
            },
            Mode::Case => Rule::Case(self.case),
            Mode::Regex => Rule::Regex {
                pattern: self.pattern.clone(),
                replace: self.regex_replace.clone(),
            },
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(state) = &mut app.view.batch else {
        return;
    };
    let p = app.palette;
    let plan = batch::plan(
        &state.items,
        &state.rule(),
        state.options,
        &state.folder_names,
    );
    let ready = plan.as_ref().map_or(0, |list| {
        list.iter().filter(|x| x.status == Status::Ready).count()
    });

    let (escape, confirm) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::Escape),
            i.consume_key(Modifiers::CTRL, Key::Enter),
        )
    });
    if escape {
        app.view.batch = None;
        return;
    }

    let screen = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(Order::Middle, Id::new("batch-scrim")))
        .rect_filled(screen, 0.0, p.scrim);

    let mut close = false;
    let mut run = confirm && ready > 0;
    let folder = app.browser.location().display().to_string();
    Area::new(Id::new("batch-rename"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::new(
            screen.center().x - WIDTH / 2.0,
            screen.top() + 84.0,
        ))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                    // Header.
                    let (head, _) = ui.allocate_exact_size(vec2(WIDTH, 62.0), Sense::hover());
                    let count = state.items.len();
                    let title = if count == 1 {
                        "Rename 1 item".to_owned()
                    } else {
                        format!("Rename {count} items")
                    };
                    ui.painter().text(
                        Pos2::new(head.left() + 24.0, head.top() + 20.0 + 11.0),
                        Align2::LEFT_CENTER,
                        title,
                        text::heading(),
                        p.text,
                    );
                    ui.painter().text(
                        Pos2::new(head.left() + 24.0, head.top() + 42.0 + 9.0),
                        Align2::LEFT_CENTER,
                        &folder,
                        crate::ui::fonts::mono(12.0),
                        p.muted,
                    );
                    let x = Rect::from_center_size(
                        Pos2::new(head.right() - 20.0 - 14.0, head.top() + 34.0),
                        vec2(28.0, 28.0),
                    );
                    let xr = ui
                        .interact(x, ui.id().with("batch-close"), Sense::click())
                        .on_hover_text("Close (Esc)");
                    if xr.hovered() {
                        ui.painter()
                            .rect_filled(x, CornerRadius::same(radius::SM), p.hover);
                    }
                    icons::paint(ui.painter(), x, Icon::Close, 12.0, p.text_2);
                    close |= xr.clicked();

                    Frame::NONE
                        .inner_margin(Margin {
                            left: 24,
                            right: 24,
                            top: 0,
                            bottom: 0,
                        })
                        .show(ui, |ui| {
                            ui.set_width(WIDTH - 48.0);
                            ui.spacing_mut().item_spacing = vec2(8.0, 14.0);
                            controls(ui, &p, state);
                            match &plan {
                                Ok(list) => preview_table(ui, &p, list),
                                Err(err) => {
                                    ui.label(
                                        egui::RichText::new(err).font(text::body()).color(p.error),
                                    );
                                }
                            }
                        });

                    // Footer.
                    ui.add_space(20.0);
                    let (foot, _) = ui.allocate_exact_size(vec2(WIDTH, 62.0), Sense::hover());
                    ui.painter().rect_filled(
                        foot,
                        CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: radius::LG,
                            se: radius::LG,
                        },
                        p.bg_1,
                    );
                    ui.painter().hline(
                        foot.x_range(),
                        foot.top() + 0.5,
                        Stroke::new(1.0, p.border),
                    );
                    let skipped = plan.as_ref().map_or(0, |list| {
                        list.iter()
                            .filter(|x| {
                                matches!(
                                    x.status,
                                    Status::Taken | Status::Duplicate | Status::Invalid(_)
                                )
                            })
                            .count()
                    });
                    let mut summary = match ready {
                        0 => "Nothing to rename yet".to_owned(),
                        1 => "1 will be renamed".to_owned(),
                        n => format!("{n} will be renamed"),
                    };
                    if skipped > 0 {
                        summary.push_str(&format!(" · {skipped} skipped"));
                    }
                    if ready > 0 {
                        summary.push_str(". You can undo with Ctrl Z");
                    }
                    ui.painter().text(
                        Pos2::new(foot.left() + 24.0, foot.center().y),
                        Align2::LEFT_CENTER,
                        summary,
                        crate::ui::fonts::regular(12.5),
                        p.text_2,
                    );

                    let label = match ready {
                        1 => "Rename 1 item".to_owned(),
                        n => format!("Rename {n} items"),
                    };
                    let primary_w = widgets::button_width(
                        ui,
                        &p,
                        &label,
                        ButtonStyle::Primary,
                        Some("Ctrl Enter"),
                    );
                    let cancel_w =
                        widgets::button_width(ui, &p, "Cancel", ButtonStyle::Ghost, None);
                    let right = foot.right() - 20.0;
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(Rect::from_min_max(
                                Pos2::new(
                                    right - primary_w - 12.0 - cancel_w,
                                    foot.center().y - 16.0,
                                ),
                                Pos2::new(right, foot.center().y + 16.0),
                            ))
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    child.spacing_mut().item_spacing.x = 12.0;
                    close |= widgets::button(&mut child, &p, "Cancel", ButtonStyle::Ghost, None)
                        .clicked();
                    child.add_enabled_ui(ready > 0, |ui| {
                        run |= widgets::button(
                            ui,
                            &p,
                            &label,
                            ButtonStyle::Primary,
                            Some("Ctrl Enter"),
                        )
                        .clicked()
                            && ready > 0;
                    });
                });
        });

    if close {
        app.view.batch = None;
    } else if run && let Ok(list) = plan {
        app.view.batch = None;
        app.run_batch_rename(&list);
    }
}

fn controls(ui: &mut egui::Ui, p: &Palette, state: &mut BatchState) {
    let modes = [Mode::Template, Mode::Replace, Mode::Case, Mode::Regex];
    let selected = modes.iter().position(|m| *m == state.mode).unwrap_or(0);
    if let Some(i) = widgets::segmented(
        ui,
        p,
        &["Template", "Find & replace", "Change case", "Regex"],
        selected,
    ) {
        state.mode = modes[i];
        state.focus = true;
    }
    let full = ui.available_width();
    match state.mode {
        Mode::Template => {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                widgets::field_label(ui, p, "Template");
                let main =
                    widgets::text_field(ui, p, &mut state.template, full, true, "{name}{ext}");
                if std::mem::take(&mut state.focus) {
                    main.request_focus();
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        egui::RichText::new("Insert")
                            .font(crate::ui::fonts::regular(12.0))
                            .color(p.muted),
                    );
                    for token in ["{name}", "{n}", "{n:02}", "{date}", "{ext}", "{parent}"] {
                        if chip(ui, p, token) {
                            state.template.push_str(token);
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "Start at");
                    widgets::text_field(ui, p, &mut state.start, 72.0, true, "1");
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "{date} comes from");
                    let sel = match state.date {
                        DateSource::Modified => 0,
                        DateSource::Created => 1,
                    };
                    if let Some(i) =
                        widgets::segmented(ui, p, &["Date modified", "Date created"], sel)
                    {
                        state.date = if i == 0 {
                            DateSource::Modified
                        } else {
                            DateSource::Created
                        };
                    }
                });
                options(ui, p, &mut state.options);
            });
        }
        Mode::Replace => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let half = (full - 16.0 - 160.0) / 2.0;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "Find");
                    let main = widgets::text_field(ui, p, &mut state.find, half, true, "IMG_");
                    if std::mem::take(&mut state.focus) {
                        main.request_focus();
                    }
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "Replace with");
                    widgets::text_field(ui, p, &mut state.replace, half, true, "");
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "Match case");
                    widgets::switch(ui, p, &mut state.match_case);
                });
            });
            options(ui, p, &mut state.options);
        }
        Mode::Case => {
            let styles = [
                CaseStyle::Lower,
                CaseStyle::Upper,
                CaseStyle::Title,
                CaseStyle::Sentence,
            ];
            let sel = styles.iter().position(|s| *s == state.case).unwrap_or(0);
            if let Some(i) = widgets::segmented(
                ui,
                p,
                &["lower case", "UPPER CASE", "Title Case", "Sentence case"],
                sel,
            ) {
                state.case = styles[i];
            }
            options(ui, p, &mut state.options);
        }
        Mode::Regex => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let half = (full - 16.0) / 2.0;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "Pattern (name without extension)");
                    let main = widgets::text_field(ui, p, &mut state.pattern, half, true, r"(\d+)");
                    if std::mem::take(&mut state.focus) {
                        main.request_focus();
                    }
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, "Replace with ($1 for groups)");
                    widgets::text_field(ui, p, &mut state.regex_replace, half, true, "");
                });
            });
            options(ui, p, &mut state.options);
        }
    }
}

fn options(ui: &mut egui::Ui, p: &Palette, options: &mut Options) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add_space(4.0);
        widgets::switch(ui, p, &mut options.lowercase_ext);
        ui.label(
            egui::RichText::new("Lowercase extensions")
                .font(crate::ui::fonts::regular(12.5))
                .color(p.text),
        );
        ui.add_space(12.0);
        widgets::switch(ui, p, &mut options.include_folders);
        ui.label(
            egui::RichText::new("Include folders")
                .font(crate::ui::fonts::regular(12.5))
                .color(p.text),
        );
    });
}

fn chip(ui: &mut egui::Ui, p: &Palette, label: &str) -> bool {
    let g = ui
        .painter()
        .layout_no_wrap(label.to_owned(), crate::ui::fonts::mono(11.5), p.text_2);
    let (rect, response) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 24.0), Sense::click());
    let fill = if response.hovered() {
        p.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(5),
        fill,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - g.size() / 2.0, g, p.text_2);
    response.clicked()
}

fn preview_table(ui: &mut egui::Ui, p: &Palette, list: &[Planned]) {
    let width = ui.available_width();
    let cols = |r: Rect| {
        let inner = r.shrink2(vec2(14.0, 0.0));
        let status_w = 160.0;
        let arrow_w = 24.0;
        let half = (inner.width() - status_w - arrow_w - 36.0) / 2.0;
        let before = Rect::from_min_size(inner.min, vec2(half, inner.height()));
        let arrow = Rect::from_min_size(
            Pos2::new(before.right() + 12.0, inner.top()),
            vec2(arrow_w, inner.height()),
        );
        let after = Rect::from_min_size(
            Pos2::new(arrow.right() + 12.0, inner.top()),
            vec2(half, inner.height()),
        );
        let status = Rect::from_min_max(Pos2::new(after.right() + 12.0, inner.top()), inner.max);
        (before, arrow, after, status)
    };
    Frame::NONE
        .fill(p.bg_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::MD))
        .show(ui, |ui| {
            ui.set_width(width - 2.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(vec2(width - 2.0, 32.0), Sense::hover());
            ui.painter().hline(
                head.x_range(),
                head.bottom() - 0.5,
                Stroke::new(1.0, p.border_subtle),
            );
            let (b, _, a, s) = cols(head);
            for (r, label) in [(b, "Before"), (a, "After"), (s, "Status")] {
                ui.painter().text(
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    label,
                    crate::ui::fonts::regular(12.0),
                    p.text_2,
                );
            }
            ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    for item in list {
                        let (row, _) =
                            ui.allocate_exact_size(vec2(width - 2.0, 30.0), Sense::hover());
                        let warn = matches!(item.status, Status::Taken | Status::Duplicate);
                        let bad = matches!(item.status, Status::Invalid(_));
                        if warn {
                            ui.painter()
                                .rect_filled(row, 0.0, p.warning.gamma_multiply(0.08));
                        }
                        let (b, a_rect, a, s) = cols(row);
                        let mono = crate::ui::fonts::mono(12.5);
                        let g = widgets::truncated(
                            ui,
                            &item.old_name,
                            mono.clone(),
                            p.text_2,
                            b.width(),
                        );
                        widgets::paint_aligned(ui.painter(), b, g, Align2::LEFT_CENTER, p.text_2);
                        ui.painter().text(
                            a_rect.left_center(),
                            Align2::LEFT_CENTER,
                            "→",
                            mono.clone(),
                            p.faint,
                        );
                        let color = match item.status {
                            Status::Ready => p.accent,
                            Status::Unchanged => p.muted,
                            Status::Taken | Status::Duplicate => p.warning,
                            Status::Invalid(_) => p.error,
                        };
                        let g = widgets::truncated(ui, &item.new_name, mono, color, a.width());
                        widgets::paint_aligned(ui.painter(), a, g, Align2::LEFT_CENTER, color);
                        let status_color = if warn {
                            p.warning
                        } else if bad {
                            p.error
                        } else {
                            p.muted
                        };
                        let mut x = s.left();
                        if warn || bad {
                            icons::paint(
                                ui.painter(),
                                Rect::from_min_size(
                                    Pos2::new(x, s.center().y - 7.0),
                                    vec2(14.0, 14.0),
                                ),
                                Icon::Warning,
                                14.0,
                                status_color,
                            );
                            x += 20.0;
                        }
                        let g = widgets::truncated(
                            ui,
                            &item.status.label(),
                            crate::ui::fonts::regular(12.0),
                            status_color,
                            s.right() - x,
                        );
                        widgets::paint_aligned(
                            ui.painter(),
                            Rect::from_min_max(Pos2::new(x, s.top()), s.max),
                            g,
                            Align2::LEFT_CENTER,
                            status_color,
                        );
                    }
                    ui.add_space(4.0);
                });
        });
}

impl ExploristApp {
    /// Opens the dialog for the selection, or every item if none.
    pub(crate) fn open_batch_rename(&mut self) {
        let mut items: Vec<Entry> = self
            .browser
            .selected_entries()
            .into_iter()
            .cloned()
            .collect();
        if items.is_empty() {
            let rows = self.browser.rows();
            items = (0..rows.len())
                .filter_map(|r| rows.get(r).cloned())
                .collect();
        }
        if items.is_empty() {
            return;
        }
        let rows = self.browser.rows();
        let names: Vec<String> = rows.entries.iter().map(|e| e.name.clone()).collect();
        self.view.batch = Some(BatchState::new(items, names));
    }

    pub(crate) fn run_batch_rename(&mut self, plan: &[Planned]) {
        let pairs: Vec<(PathBuf, String)> = plan
            .iter()
            .filter(|x| x.status == Status::Ready)
            .map(|x| (x.from.clone(), x.new_name.clone()))
            .collect();
        let outcome = batch::rename_all(&pairs);
        self.browser.refresh();
        if let Some((_, first)) = outcome.done.first() {
            self.browser.select_when_loaded(first.clone());
        }
        if let Some((path, err)) = outcome.failed.first() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.toast_error(format!(
                "{} renamed; “{name}” failed: {err}",
                outcome.done.len()
            ));
        } else {
            let n = outcome.done.len();
            self.toast(
                if n == 1 {
                    "Renamed 1 item".to_owned()
                } else {
                    format!("Renamed {n} items")
                },
                true,
            );
        }
        if !outcome.done.is_empty() {
            self.carry_folder_icons(&outcome.done);
            self.push_undo(crate::app::undo::Action::BatchRename {
                pairs: outcome.done,
            });
        }
    }
}
