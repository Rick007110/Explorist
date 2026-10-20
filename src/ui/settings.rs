//! The Settings page. Changes apply instantly; nothing needs a restart.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, ScrollArea, Sense, Stroke,
    StrokeKind, TextEdit, vec2,
};

use crate::app::ExploristApp;
use crate::app::commands::Command;
use crate::app::keymap::Keymap;
use crate::config::{Accent, Density, Settings, StartIn, Theme};
use crate::fs::format::{self, DateFormat};
use crate::fs::places;
use crate::fs::sort::{SortColumn, SortDirection};
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, Palette, radius, text};
use crate::ui::widgets::{self, ButtonStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    Sorting,
    Keyboard,
    Git,
    Integrations,
    Advanced,
}

impl Section {
    const ALL: [Section; 7] = [
        Section::General,
        Section::Appearance,
        Section::Sorting,
        Section::Keyboard,
        Section::Git,
        Section::Integrations,
        Section::Advanced,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Sorting => "Sorting & view",
            Self::Keyboard => "Keyboard",
            Self::Git => "Git",
            Self::Integrations => "Integrations",
            Self::Advanced => "Advanced",
        }
    }
}

/// Settings page state kept between frames.
#[derive(Default)]
pub struct SettingsState {
    pub search: String,
    /// Scroll this section into view next frame.
    pub jump_to: Option<Section>,
    /// The section at the top of the page, highlighted in the nav.
    pub current: Option<Section>,
    /// The command waiting for a new shortcut.
    pub recording: Option<Command>,
    /// "Reset all settings" was clicked once; a second click confirms.
    pub confirm_reset: Option<std::time::Instant>,
    /// The app being added under Integrations.
    pub new_app: crate::fs::integrations::CustomApp,
}

/// What the page changed, applied after drawing.
#[derive(Default)]
struct Changes {
    settings: bool,
    pin_current: bool,
    unpin: Option<std::path::PathBuf>,
    close: bool,
    check_updates: bool,
}

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let mut changes = Changes::default();

    egui::Panel::left("settings-nav")
        .exact_size(260.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            Frame::NONE
                .fill(p.bg_1)
                .inner_margin(Margin::symmetric(12, 20)),
        )
        .show(ui, |ui| nav(ui, app, &p));

    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(p.bg_2))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("settings-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    Frame::NONE
                        .inner_margin(Margin {
                            left: 48,
                            right: 48,
                            top: 32,
                            bottom: 48,
                        })
                        .show(ui, |ui| {
                            ui.set_max_width(780.0);
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            content(ui, app, &p, &mut changes);
                        });
                });
        });

    if changes.pin_current {
        let here = app.browser.location().to_path_buf();
        app.pin(here);
    }
    if let Some(path) = changes.unpin {
        app.unpin(&path);
    }
    if changes.settings {
        app.apply_settings();
    }
    if changes.check_updates {
        app.check_for_updates(true);
    }
    if changes.close {
        app.view.settings_open = false;
    }
}

fn nav(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette) {
    let full = ui.max_rect();
    ui.painter().vline(
        full.right() + 12.0 - 0.5,
        full.y_range().expand(20.0),
        Stroke::new(1.0, p.border_subtle),
    );
    ui.spacing_mut().item_spacing.y = 2.0;

    // Search box.
    let (field, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().rect(
        field,
        CornerRadius::same(radius::SM),
        p.surface,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    icons::paint(
        ui.painter(),
        Rect::from_min_size(
            Pos2::new(field.left() + 10.0, field.center().y - 8.0),
            vec2(16.0, 16.0),
        ),
        Icon::Filter,
        16.0,
        p.muted,
    );
    let inner = Rect::from_center_size(
        Pos2::new(field.center().x + 14.0, field.center().y),
        vec2(field.width() - 48.0, 18.0),
    );
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.add(
        TextEdit::singleline(&mut app.view.settings.search)
            .hint_text("Search settings")
            .font(text::body())
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .desired_width(inner.width()),
    );
    ui.add_space(14.0);

    for section in Section::ALL {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        let active = app.view.settings.current == Some(section);
        let fill = if active {
            p.pressed
        } else if response.hovered() {
            p.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::SM), fill);
        let font = if active {
            text::body_medium()
        } else {
            text::body()
        };
        ui.painter().text(
            Pos2::new(rect.left() + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            section.title(),
            font,
            if active { p.text } else { p.text_2 },
        );
        if response.clicked() {
            app.view.settings.search.clear();
            app.view.settings.jump_to = Some(section);
        }
    }

    let version = format!("Explorist {}", env!("CARGO_PKG_VERSION"));
    ui.painter().text(
        Pos2::new(full.left() + 10.0, full.bottom() - 8.0),
        Align2::LEFT_CENTER,
        version,
        crate::ui::fonts::mono(11.5),
        p.muted,
    );
}

/// Whether a row matches the search box.
fn shown(app: &ExploristApp, title: &str, description: &str) -> bool {
    matches(&app.view.settings.search, title, description)
}

fn matches(query: &str, title: &str, description: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || title.to_lowercase().contains(&query)
        || description.to_lowercase().contains(&query)
}

fn content(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let viewport_top = ui.clip_rect().top();
    let mut current = None;
    let mut first = true;
    for section in Section::ALL {
        let rows = section_rows(section);
        if !rows.iter().any(|(t, d)| shown(app, t, d)) {
            continue;
        }
        if !first {
            ui.add_space(40.0);
        }
        first = false;
        let top = ui.cursor().top();
        if top <= viewport_top + 80.0 || current.is_none() {
            current = Some(section);
        }
        heading(ui, p, section, app.view.settings.jump_to == Some(section));
        if app.view.settings.jump_to == Some(section) {
            app.view.settings.jump_to = None;
        }
        match section {
            Section::General => general(ui, app, p, changes),
            Section::Appearance => appearance(ui, app, p, changes),
            Section::Sorting => sorting(ui, app, p, changes),
            Section::Keyboard => keyboard(ui, app, p, changes),
            Section::Git => git(ui, app, p, changes),
            Section::Integrations => integrations(ui, app, p, changes),
            Section::Advanced => advanced(ui, app, p, changes),
        }
    }
    if first {
        ui.label(
            egui::RichText::new("No settings match your search.")
                .font(text::body())
                .color(p.text_2),
        );
    }
    app.view.settings.current = current;
}

fn heading(ui: &mut egui::Ui, p: &Palette, section: Section, scroll_here: bool) {
    let galley = ui
        .painter()
        .layout_no_wrap(section.title().to_owned(), text::title(), p.text);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    ui.painter().galley(rect.min, galley, p.text);
    if section == Section::Appearance {
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
        ui.painter().text(
            r.left_center(),
            Align2::LEFT_CENTER,
            "Changes apply instantly. Nothing here needs a restart.",
            text::body(),
            p.text_2,
        );
    }
    ui.add_space(16.0);
    if scroll_here {
        ui.scroll_to_rect(rect, Some(egui::Align::TOP));
    }
}

/// Titles and descriptions per section, for search.
fn section_rows(section: Section) -> Vec<(&'static str, &'static str)> {
    match section {
        Section::General => vec![
            ("Start in", "The folder Explorist opens with."),
            ("Show hidden files", "Hidden and system items. Also Ctrl H."),
            ("Pinned folders", "Shown under Favorites in the sidebar."),
            ("Updates", "Check for a new version once a day."),
        ],
        Section::Appearance => vec![
            (
                "Theme",
                "Match system follows your system light/dark setting.",
            ),
            (
                "Accent",
                "Used for selection, focus and primary actions only.",
            ),
            ("Row density", "Compact fits about 25% more rows."),
            (
                "Reduce motion",
                "On by default when Windows animations are turned off.",
            ),
        ],
        Section::Sorting => vec![
            (
                "Default sort",
                "Folders you sort by hand remember their own order.",
            ),
            ("Folders first", "Group folders above files in every sort."),
            ("Natural number order", "file2 before file10."),
            (
                "Date format",
                "Applies to the Modified column and status bar.",
            ),
            (
                "Folder sizes",
                "Show how big each folder is in the Size column. Worked out in the background.",
            ),
        ],
        Section::Keyboard => vec![(
            "Keyboard shortcuts",
            "Rename, filter, new folder, shortcut keys",
        )],
        Section::Git => vec![(
            "Show git status",
            "Badges for changed files and the branch in the status bar. Needs git installed.",
        )],
        Section::Integrations => vec![
            (
                "Apps in the right-click menu",
                "WinRAR, 7-Zip, Sublime Text, Notepad++ and VS Code, when installed. Switch off any you don't want.",
            ),
            (
                "Your own apps",
                "Any other program. In its arguments, {files} is the selected items, {file} the first one and {dir} the open folder.",
            ),
        ],
        Section::Advanced => vec![
            ("Settings file", "Where these settings are saved."),
            ("Reset all settings", "Go back to the defaults."),
        ],
    }
}

/// One setting: title and description on the left, the control on the right.
fn row(
    ui: &mut egui::Ui,
    query: &str,
    p: &Palette,
    title: &str,
    description: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    if !matches(query, title, description) {
        return;
    }
    let width = ui.available_width();
    let top = ui.cursor().top();
    ui.painter().hline(
        ui.cursor().left()..=ui.cursor().left() + width,
        top + 0.5,
        Stroke::new(1.0, p.border_subtle),
    );
    ui.add_space(16.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(32.0, 0.0);
        ui.vertical(|ui| {
            ui.set_width(220.0);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                egui::RichText::new(title)
                    .font(text::body_medium())
                    .color(p.text),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(description)
                        .font(crate::ui::fonts::regular(12.0))
                        .color(p.muted),
                )
                .wrap(),
            );
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            control(ui);
        });
    });
    ui.add_space(16.0);
}

fn general(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let query = app.view.settings.search.clone();
    let rows = section_rows(Section::General);
    let mut s = app.settings.clone();

    row(ui, &query, p, rows[0].0, rows[0].1, |ui| {
        let selected = match s.start_in {
            StartIn::Home => 0,
            StartIn::LastFolder => 1,
        };
        if let Some(i) = widgets::segmented(ui, p, &["Home folder", "Last folder opened"], selected)
        {
            s.start_in = if i == 0 {
                StartIn::Home
            } else {
                StartIn::LastFolder
            };
        }
    });
    row(ui, &query, p, rows[1].0, rows[1].1, |ui| {
        widgets::switch(ui, p, &mut s.show_hidden);
    });
    row(ui, &query, p, rows[2].0, rows[2].1, |ui| {
        if s.pinned.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Nothing pinned yet. Right-click a folder and choose Pin to Favorites.",
                )
                .font(text::body())
                .color(p.text_2),
            );
        }
        for path in &s.pinned {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if widgets::icon_button_sized(ui, p, Icon::Close, "Unpin", true, false, 24.0, 12.0)
                    .clicked()
                {
                    changes.unpin = Some(path.clone());
                }
                ui.label(
                    egui::RichText::new(path.display().to_string())
                        .font(text::mono())
                        .color(p.text),
                );
            });
        }
        if widgets::button(ui, p, "Pin current folder", ButtonStyle::Secondary, None).clicked() {
            changes.pin_current = true;
        }
    });
    let checking = app.update.is_checking();
    let status = update_status(app);
    row(ui, &query, p, rows[3].0, rows[3].1, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            widgets::switch(ui, p, &mut s.updates.check_automatically);
            ui.label(
                egui::RichText::new("Check for updates automatically")
                    .font(text::body())
                    .color(p.text),
            );
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let label = if checking { "Checking…" } else { "Check now" };
            if widgets::button(ui, p, label, ButtonStyle::Secondary, None).clicked() && !checking {
                changes.check_updates = true;
            }
            ui.label(
                egui::RichText::new(status)
                    .font(crate::ui::fonts::regular(12.0))
                    .color(p.muted),
            );
        });
    });
    commit(app, s, changes);
}

fn appearance(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let query = app.view.settings.search.clone();
    let rows = section_rows(Section::Appearance);
    let mut s = app.settings.clone();

    row(ui, &query, p, rows[0].0, rows[0].1, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (theme, label) in [
                (Theme::Dark, "Dark"),
                (Theme::Light, "Light"),
                (Theme::System, "Match system"),
            ] {
                if theme_card(ui, p, theme, label, s.theme == theme, s.accent) {
                    s.theme = theme;
                }
            }
        });
    });
    row(ui, &query, p, rows[1].0, rows[1].1, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for (accent, label) in [
                (Accent::Teal, "Mint"),
                (Accent::Blue, "Periwinkle"),
                (Accent::Orange, "Amber"),
                (Accent::Purple, "Lilac"),
            ] {
                let color = theme::accent_color(accent, p.dark);
                if widgets::swatch(ui, p, color, s.accent == accent, label) {
                    s.accent = accent;
                }
            }
        });
    });
    row(ui, &query, p, rows[2].0, rows[2].1, |ui| {
        let selected = match s.density {
            Density::Comfortable => 0,
            Density::Compact => 1,
        };
        if let Some(i) =
            widgets::segmented(ui, p, &["Comfortable · 30 px", "Compact · 24 px"], selected)
        {
            s.density = if i == 0 {
                Density::Comfortable
            } else {
                Density::Compact
            };
        }
    });
    let mut reduce = app.reduce_motion();
    row(ui, &query, p, rows[3].0, rows[3].1, |ui| {
        if widgets::switch(ui, p, &mut reduce) {
            s.reduce_motion = Some(reduce);
        }
    });
    commit(app, s, changes);
}

/// A theme preview card: a tiny sidebar and list in that theme's colors.
fn theme_card(
    ui: &mut egui::Ui,
    p: &Palette,
    theme: Theme,
    label: &str,
    chosen: bool,
    accent: Accent,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(160.0, 104.0), Sense::click());
    let painter = ui.painter();
    let fill = if chosen {
        p.surface
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    painter.rect(
        rect,
        CornerRadius::same(radius::MD),
        fill,
        Stroke::new(1.0, if chosen { p.accent } else { p.border }),
        StrokeKind::Inside,
    );
    if chosen {
        painter.rect_stroke(
            rect,
            CornerRadius::same(radius::MD),
            Stroke::new(3.0, p.accent_alpha(0.22)),
            StrokeKind::Outside,
        );
    }
    let preview = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(rect.width() - 16.0, 64.0));
    let draw = |area: Rect, pal: &Palette| {
        painter.rect_filled(area, 0.0, pal.bg_2);
        let side = Rect::from_min_size(area.min, vec2(area.width() * 0.28, area.height()));
        painter.rect_filled(side, 0.0, pal.bg_1);
        let x = side.right() + 8.0;
        let w = area.right() - x - 8.0;
        for (i, (frac, h, accent_bar)) in [
            (0.7, 4.0, false),
            (0.85, 6.0, true),
            (0.6, 4.0, false),
            (0.75, 4.0, false),
        ]
        .into_iter()
        .enumerate()
        {
            let y = area.top() + 8.0 + i as f32 * 11.0;
            let color = if accent_bar {
                pal.accent_alpha(if pal.dark { 0.45 } else { 0.3 })
            } else {
                pal.scroll_thumb
            };
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(x, y), vec2(w * frac, h)),
                CornerRadius::same(2),
                color,
            );
        }
    };
    match theme {
        Theme::Dark => draw(preview, &Palette::dark(accent)),
        Theme::Light => draw(preview, &Palette::light(accent)),
        Theme::System => {
            let (l, r) = preview.split_left_right_at_fraction(0.5);
            painter.rect_filled(l, 0.0, Palette::light(accent).bg_2);
            painter.rect_filled(r, 0.0, Palette::dark(accent).bg_2);
        }
    }
    painter.rect_stroke(
        preview,
        CornerRadius::same(5),
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    painter.text(
        Pos2::new(rect.left() + 10.0, rect.bottom() - 16.0),
        Align2::LEFT_CENTER,
        label,
        crate::ui::fonts::medium(12.5),
        p.text,
    );
    if chosen {
        icons::paint(
            painter,
            Rect::from_center_size(
                Pos2::new(rect.right() - 18.0, rect.bottom() - 16.0),
                vec2(14.0, 14.0),
            ),
            Icon::Check,
            14.0,
            p.accent,
        );
    }
    response.clicked()
}

fn sorting(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let query = app.view.settings.search.clone();
    let rows = section_rows(Section::Sorting);
    let mut s = app.settings.clone();

    row(ui, &query, p, rows[0].0, rows[0].1, |ui| {
        let columns = [
            SortColumn::Name,
            SortColumn::Size,
            SortColumn::Modified,
            SortColumn::Type,
        ];
        let selected = columns
            .iter()
            .position(|c| *c == s.sort.column)
            .unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, p, &["Name", "Size", "Modified", "Type"], selected)
        {
            s.sort.column = columns[i];
        }
        let selected = match s.sort.direction {
            SortDirection::Ascending => 0,
            SortDirection::Descending => 1,
        };
        if let Some(i) = widgets::segmented(ui, p, &["Ascending", "Descending"], selected) {
            s.sort.direction = if i == 0 {
                SortDirection::Ascending
            } else {
                SortDirection::Descending
            };
        }
        if !s.folder_sorts.is_empty() {
            let note = match s.folder_sorts.len() {
                1 => "1 folder has its own sort.".to_owned(),
                n => format!("{n} folders have their own sort."),
            };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(
                    egui::RichText::new(note)
                        .font(crate::ui::fonts::regular(12.0))
                        .color(p.muted),
                );
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("Forget them")
                                .font(crate::ui::fonts::regular(12.0))
                                .color(p.accent)
                                .underline(),
                        )
                        .sense(Sense::click()),
                    )
                    .clicked()
                {
                    s.folder_sorts.clear();
                }
            });
        }
    });
    row(ui, &query, p, rows[1].0, rows[1].1, |ui| {
        widgets::switch(ui, p, &mut s.sort.folders_first);
    });
    row(ui, &query, p, rows[2].0, rows[2].1, |ui| {
        widgets::switch(ui, p, &mut s.sort.natural);
    });
    row(ui, &query, p, rows[3].0, rows[3].1, |ui| {
        let now = std::time::SystemTime::now();
        let labels: Vec<String> = DateFormat::ALL
            .iter()
            .map(|f| format::date(now, *f))
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let selected = DateFormat::ALL
            .iter()
            .position(|f| *f == s.date_format)
            .unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, p, &labels, selected) {
            s.date_format = DateFormat::ALL[i];
        }
    });
    row(ui, &query, p, rows[4].0, rows[4].1, |ui| {
        widgets::switch(ui, p, &mut s.folder_sizes);
    });
    commit(app, s, changes);
}

fn keyboard(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let rows = section_rows(Section::Keyboard);
    if !shown(app, rows[0].0, rows[0].1)
        && !Command::CUSTOMIZABLE
            .iter()
            .any(|c| shown(app, c.label(), ""))
    {
        return;
    }
    let map = Keymap::new(&app.settings.shortcuts);
    let recording = app.view.settings.recording;
    let width = ui.available_width();

    // Table.
    let (head, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
    ui.painter().rect(
        head,
        CornerRadius {
            nw: radius::MD,
            ne: radius::MD,
            sw: 0,
            se: 0,
        },
        p.bg_1,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    ui.painter().text(
        Pos2::new(head.left() + 14.0, head.center().y),
        Align2::LEFT_CENTER,
        "Action",
        text::label(),
        p.text_2,
    );
    ui.painter().text(
        Pos2::new(head.right() - 72.0 - 200.0, head.center().y),
        Align2::LEFT_CENTER,
        "Shortcut",
        text::label(),
        p.text_2,
    );
    for command in Command::CUSTOMIZABLE {
        if !shown(app, command.label(), "") && !shown(app, rows[0].0, rows[0].1) {
            continue;
        }
        let (rect, _) = ui.allocate_exact_size(vec2(width, 38.0), Sense::hover());
        let is_recording = recording == Some(command);
        if is_recording {
            ui.painter().rect_filled(rect, 0.0, p.accent_alpha(0.08));
        }
        ui.painter().vline(
            rect.left() + 0.5,
            rect.y_range(),
            Stroke::new(1.0, p.border),
        );
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0, p.border),
        );
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, p.border_subtle),
        );
        ui.painter().text(
            Pos2::new(rect.left() + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            command.label(),
            text::body(),
            p.text,
        );

        let keys_left = rect.right() - 72.0 - 200.0;
        if is_recording {
            ui.painter().text(
                Pos2::new(keys_left, rect.center().y),
                Align2::LEFT_CENTER,
                "Press new shortcut…",
                text::body(),
                p.accent,
            );
        } else if let Some(label) = map.label(command) {
            let mut x = keys_left;
            for part in label.split(' ') {
                let g = widgets::kbd_galley(ui, p, part);
                let r = widgets::paint_kbd(ui.painter(), p, g, x, rect.center().y, false);
                x = r.right() + 4.0;
            }
        } else {
            ui.painter().text(
                Pos2::new(keys_left, rect.center().y),
                Align2::LEFT_CENTER,
                "None",
                text::body(),
                p.muted,
            );
        }
        let btn = Rect::from_center_size(
            Pos2::new(rect.right() - 40.0, rect.center().y),
            vec2(56.0, 26.0),
        );
        let response = ui.interact(
            btn,
            ui.id().with(("edit-key", command.id())),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter()
                .rect_filled(btn, CornerRadius::same(radius::SM), p.hover);
        }
        ui.painter().text(
            btn.center(),
            Align2::CENTER_CENTER,
            if is_recording { "Cancel" } else { "Edit" },
            crate::ui::fonts::medium(12.5),
            p.text_2,
        );
        if response.clicked() {
            app.view.settings.recording = if is_recording { None } else { Some(command) };
        }
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let customized = !app.settings.shortcuts.is_empty();
        if customized
            && widgets::button(ui, p, "Reset to defaults", ButtonStyle::Secondary, None).clicked()
        {
            app.settings.shortcuts.clear();
            changes.settings = true;
        }
        ui.label(
            egui::RichText::new("Esc cancels recording. Ctrl C, X and V are fixed.")
                .font(crate::ui::fonts::regular(12.0))
                .color(p.muted),
        );
    });
}

fn git(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let query = app.view.settings.search.clone();
    let rows = section_rows(Section::Git);
    let mut s = app.settings.clone();
    row(ui, &query, p, rows[0].0, rows[0].1, |ui| {
        widgets::switch(ui, p, &mut s.git_status);
    });
    commit(app, s, changes);
}

fn integrations(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    use crate::fs::integrations::{Applies, CustomApp};

    let query = app.view.settings.search.clone();
    let rows = section_rows(Section::Integrations);
    let mut s = app.settings.clone();
    let muted = |text: String| {
        egui::RichText::new(text)
            .font(crate::ui::fonts::regular(12.0))
            .color(p.muted)
    };

    let known = app.integrations.known.clone();
    row(ui, &query, p, rows[0].0, rows[0].1, |ui| {
        for app in &known {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let mut on = app.found.is_some() && !s.integrations_off.contains(app.id);
                ui.add_enabled_ui(app.found.is_some(), |ui| {
                    if widgets::switch(ui, p, &mut on) {
                        if on {
                            s.integrations_off.remove(app.id);
                        } else {
                            s.integrations_off.insert(app.id.to_owned());
                        }
                    }
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        egui::RichText::new(app.name)
                            .font(text::body())
                            .color(p.text),
                    );
                    ui.label(muted(app.found.as_ref().map_or_else(
                        || "Not installed".to_owned(),
                        |path| path.display().to_string(),
                    )));
                });
            });
        }
    });

    let form = &mut app.view.settings.new_app;
    row(ui, &query, p, rows[1].0, rows[1].1, |ui| {
        let mut remove = None;
        for (i, custom) in s.custom_apps.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.vertical(|ui| {
                    ui.set_width(300.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        egui::RichText::new(&custom.name)
                            .font(text::body())
                            .color(p.text),
                    );
                    ui.label(muted(format!(
                        "{} {} · {}",
                        custom.program.display(),
                        if custom.args.trim().is_empty() {
                            "{files}"
                        } else {
                            custom.args.trim()
                        },
                        custom.applies.label()
                    )));
                });
                if widgets::button(ui, p, "Remove", ButtonStyle::Secondary, None).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            s.custom_apps.remove(i);
        }
        if !s.custom_apps.is_empty() {
            ui.add_space(8.0);
        }

        widgets::field_label(ui, p, "Name");
        widgets::text_field(ui, p, &mut form.name, 300.0, false, "e.g. Kate");
        widgets::field_label(ui, p, "Program");
        ui.horizontal(|ui| {
            let mut program = form.program.to_string_lossy().into_owned();
            if widgets::text_field(ui, p, &mut program, 300.0, true, "").changed() {
                form.program = program.into();
            }
            if widgets::button(ui, p, "Browse…", ButtonStyle::Secondary, None).clicked()
                && let Some(file) = rfd::FileDialog::new()
                    .set_title("Choose a program")
                    .pick_file()
            {
                if form.name.trim().is_empty() {
                    form.name = file
                        .file_stem()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
                form.program = file;
            }
        });
        widgets::field_label(ui, p, "Arguments");
        widgets::text_field(ui, p, &mut form.args, 300.0, true, "{files}");
        widgets::field_label(ui, p, "Shows for");
        let kinds = [
            Applies::Any,
            Applies::Files,
            Applies::Folders,
            Applies::Archives,
        ];
        let labels = kinds.map(Applies::label);
        let selected = kinds.iter().position(|k| *k == form.applies).unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, p, &labels, selected) {
            form.applies = kinds[i];
        }
        let name = form.name.trim();
        let taken = s.custom_apps.iter().any(|c| c.name == name);
        let ready = !name.is_empty() && !form.program.as_os_str().is_empty() && !taken;
        if taken {
            ui.label(muted(format!("There's already an app called “{name}”.")));
        }
        if ui
            .add_enabled_ui(ready, |ui| {
                widgets::button(ui, p, "Add app", ButtonStyle::Primary, None)
            })
            .inner
            .clicked()
        {
            let mut added = std::mem::take(form);
            added.name = added.name.trim().to_owned();
            s.custom_apps.push(added);
            *form = CustomApp::default();
        }
    });
    commit(app, s, changes);
}

fn advanced(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, changes: &mut Changes) {
    let query = app.view.settings.search.clone();
    let rows = section_rows(Section::Advanced);
    let path = Settings::default_path();
    row(ui, &query, p, rows[0].0, rows[0].1, |ui| {
        let shown_path = path
            .as_ref()
            .map_or_else(|| "Not available".to_owned(), |p| p.display().to_string());
        ui.label(
            egui::RichText::new(shown_path)
                .font(text::mono())
                .color(p.text),
        );
        if let Some(dir) = path.as_ref().and_then(|p| p.parent())
            && widgets::button(ui, p, "Open folder", ButtonStyle::Secondary, None).clicked()
        {
            let _ = std::fs::create_dir_all(dir);
            app.browser.navigate(dir.to_path_buf());
            changes.close = true;
        }
    });
    let armed = app
        .view
        .settings
        .confirm_reset
        .is_some_and(|t| t.elapsed().as_secs_f32() < 4.0);
    row(ui, &query, p, rows[1].0, rows[1].1, |ui| {
        let label = if armed {
            "Click again to reset everything"
        } else {
            "Reset all settings"
        };
        if widgets::button(ui, p, label, ButtonStyle::Destructive, None).clicked() {
            if armed {
                app.settings = Settings::default();
                app.view.settings.confirm_reset = None;
                app.places = places::standard_places();
                changes.settings = true;
            } else {
                app.view.settings.confirm_reset = Some(std::time::Instant::now());
            }
        }
    });
    if armed {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(500));
    }
}

/// One line about the last update check, for the Updates row.
fn update_status(app: &ExploristApp) -> String {
    use crate::update::{CURRENT, CheckResult};
    let when = app
        .settings
        .updates
        .last_checked
        .and_then(|t| {
            std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(t.max(0) as u64))
        })
        .map(|t| format::date(t, app.settings.date_format));
    let result = match &app.update.last_result {
        Some(CheckResult::Available(r)) => {
            format!("{} is available (you have {CURRENT})", r.version)
        }
        Some(CheckResult::UpToDate) => format!("You have the latest version, {CURRENT}"),
        Some(CheckResult::NoReleases) => "No releases found yet".to_owned(),
        Some(CheckResult::Failed(err)) => format!("Couldn’t check: {err}"),
        None => format!("Version {CURRENT}"),
    };
    match when {
        Some(when) => format!("{result} · last checked {when}"),
        None => result,
    }
}

/// Stores `s` if anything changed.
fn commit(app: &mut ExploristApp, s: Settings, changes: &mut Changes) {
    if s != app.settings {
        app.settings = s;
        changes.settings = true;
    }
}
