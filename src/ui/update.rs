//! The update card: shown bottom-right when a newer release is out, and
//! while it downloads and installs.

use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{Align2, Area, CornerRadius, Frame, Id, Margin, Order, Stroke, vec2};

use crate::app::ExploristApp;
use crate::fs::format;
use crate::ui::theme::{radius, text};
use crate::ui::widgets::{self, ButtonStyle};
use crate::update::{self, CheckResult, InstallEvent, Release};

/// Checking, offering and installing updates.
#[derive(Default)]
pub struct UpdateState {
    checking: Option<Receiver<CheckResult>>,
    /// Started from Settings: report every outcome, not just news.
    manual: bool,
    /// What the last check found, for Settings.
    pub last_result: Option<CheckResult>,
    /// The release on offer in the card.
    offer: Option<Release>,
    installing: Option<Receiver<InstallEvent>>,
    progress: (u64, u64),
    installed: bool,
    error: Option<String>,
}

impl UpdateState {
    pub fn is_checking(&self) -> bool {
        self.checking.is_some()
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl ExploristApp {
    /// At startup: check if allowed and the last check was over a day ago.
    pub(crate) fn maybe_check_for_updates(&mut self) {
        let due = self
            .settings
            .updates
            .last_checked
            .is_none_or(|t| now_secs() - t > 24 * 60 * 60);
        if self.settings.updates.check_automatically && due {
            self.check_for_updates(false);
        }
    }

    pub(crate) fn check_for_updates(&mut self, manual: bool) {
        if self.update.checking.is_some() {
            return;
        }
        let ctx = self.ctx.clone();
        self.update.checking = Some(update::check_in_background(move || ctx.request_repaint()));
        self.update.manual = manual;
        self.settings.updates.last_checked = Some(now_secs());
        self.save_settings();
    }

    fn poll_updates(&mut self) {
        if let Some(rx) = &self.update.checking
            && let Ok(result) = rx.try_recv()
        {
            self.update.checking = None;
            if let CheckResult::Available(release) = &result {
                let skipped = self.settings.updates.skipped_version.as_deref()
                    == Some(release.version.as_str());
                if self.update.manual || !skipped {
                    self.update.offer = Some(release.clone());
                    self.update.error = None;
                }
            }
            self.update.last_result = Some(result);
        }
        if let Some(rx) = &self.update.installing {
            while let Ok(event) = rx.try_recv() {
                match event {
                    InstallEvent::Progress(done, total) => self.update.progress = (done, total),
                    InstallEvent::Done => {
                        self.update.installing = None;
                        self.update.installed = true;
                        break;
                    }
                    InstallEvent::Failed(err) => {
                        self.update.installing = None;
                        self.update.error = Some(err);
                        break;
                    }
                }
            }
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    app.poll_updates();
    let Some(release) = app.update.offer.clone() else {
        return;
    };
    let p = app.palette;

    enum Action {
        Install(String),
        OpenPage,
        Later,
        Skip,
        Restart,
    }
    let mut action = None;

    Area::new(Id::new("update-card"))
        .order(Order::Foreground)
        .anchor(
            Align2::RIGHT_BOTTOM,
            vec2(-16.0, -(crate::ui::theme::size::STATUS_BAR + 16.0)),
        )
        .show(ctx, |ui| {
            Frame::NONE
                .fill(p.raised)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_float())
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(340.0);
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    let title = if app.update.installed {
                        format!("Explorist {} is installed", release.version)
                    } else {
                        format!("Explorist {} is available", release.version)
                    };
                    ui.label(
                        egui::RichText::new(title)
                            .font(text::heading())
                            .color(p.text),
                    );
                    ui.label(
                        egui::RichText::new(format!("You have {}.", update::CURRENT))
                            .font(text::caption())
                            .color(p.muted),
                    );
                    let (points, more) = update::note_points(&release.notes, 5);
                    let mut notes = points
                        .iter()
                        .map(|p| format!("•  {p}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if more {
                        notes.push_str("\n…and more");
                    }
                    if !notes.is_empty() && !app.update.installed {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(notes)
                                    .font(text::body())
                                    .color(p.text_2),
                            )
                            .wrap(),
                        );
                    }

                    if let Some(err) = &app.update.error {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("Update failed: {err}"))
                                    .font(text::caption())
                                    .color(p.error),
                            )
                            .wrap(),
                        );
                    }

                    if app.update.installing.is_some() {
                        let (done, total) = app.update.progress;
                        let fraction = if total > 0 {
                            done as f32 / total as f32
                        } else {
                            0.0
                        };
                        let (bar, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), 4.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(bar, 2.0, p.scroll_track);
                        let mut fill = bar;
                        fill.set_width(bar.width() * fraction.clamp(0.0, 1.0));
                        ui.painter().rect_filled(fill, 2.0, p.accent);
                        let label = if total > 0 {
                            format!(
                                "Downloading… {} of {}",
                                format::size(done),
                                format::size(total)
                            )
                        } else {
                            format!("Downloading… {}", format::size(done))
                        };
                        ui.label(
                            egui::RichText::new(label)
                                .font(crate::ui::fonts::mono(11.5))
                                .color(p.text_2),
                        );
                        return;
                    }

                    ui.horizontal(|ui| {
                        if app.update.installed {
                            if widgets::button(ui, &p, "Restart now", ButtonStyle::Primary, None)
                                .clicked()
                            {
                                action = Some(Action::Restart);
                            }
                            if widgets::button(ui, &p, "Later", ButtonStyle::Ghost, None).clicked()
                            {
                                action = Some(Action::Later);
                            }
                            return;
                        }
                        match (&release.download, true) {
                            (Some(url), true) => {
                                if widgets::button(ui, &p, "Update now", ButtonStyle::Primary, None)
                                    .clicked()
                                {
                                    action = Some(Action::Install(url.clone()));
                                }
                            }
                            _ => {
                                if widgets::button(
                                    ui,
                                    &p,
                                    "Open release page",
                                    ButtonStyle::Primary,
                                    None,
                                )
                                .clicked()
                                {
                                    action = Some(Action::OpenPage);
                                }
                            }
                        }
                        if widgets::button(ui, &p, "Later", ButtonStyle::Ghost, None).clicked() {
                            action = Some(Action::Later);
                        }
                    });
                    if !app.update.installed {
                        ui.horizontal(|ui| {
                            let notes = ui.add(
                                egui::Label::new(
                                    egui::RichText::new("Release notes")
                                        .font(text::caption())
                                        .color(p.accent)
                                        .underline(),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if notes.clicked() {
                                action = Some(Action::OpenPage);
                            }
                            ui.add_space(8.0);
                            let skip = ui.add(
                                egui::Label::new(
                                    egui::RichText::new("Skip this version")
                                        .font(text::caption())
                                        .color(p.muted)
                                        .underline(),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if skip.clicked() {
                                action = Some(Action::Skip);
                            }
                        });
                    }
                });
        });

    match action {
        Some(Action::Install(url)) => {
            let wake = ctx.clone();
            app.update.error = None;
            app.update.progress = (0, 0);
            app.update.installing = Some(update::install_in_background(url, move || {
                wake.request_repaint()
            }));
        }
        Some(Action::OpenPage) => {
            if let Err(err) = open::that_detached(&release.page) {
                app.toast_error(format!("Couldn’t open the release page: {err}"));
            }
        }
        Some(Action::Later) => app.update.offer = None,
        Some(Action::Skip) => {
            app.settings.updates.skipped_version = Some(release.version.clone());
            app.update.offer = None;
            app.save_settings();
        }
        Some(Action::Restart) => match update::restart() {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(err) => app.toast_error(format!(
                "Couldn’t restart: {err}. Close and reopen Explorist."
            )),
        },
        None => {}
    }
}
