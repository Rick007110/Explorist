//! Other apps' entries in the right-click menu (Settings → Integrations).

use std::ffi::OsString;
use std::path::PathBuf;

use crate::app::ExploristApp;
use crate::fs::integrations::{self, App, Context, Known};

/// The built-in apps, looked for once at startup.
pub struct Integrations {
    pub known: Vec<Known>,
    found: Vec<App>,
}

impl Integrations {
    pub fn detect() -> Self {
        let (known, found) = integrations::detect();
        Self { known, found }
    }
}

/// A menu entry ready to run.
pub struct MenuAction {
    pub label: String,
    program: PathBuf,
    args: Vec<OsString>,
}

impl ExploristApp {
    /// Apps in the menu: the built-in ones found and switched on, then the
    /// user's own.
    fn menu_apps(&self) -> Vec<App> {
        self.integrations
            .found
            .iter()
            .filter(|a| !self.settings.integrations_off.contains(&a.id))
            .cloned()
            .chain(
                self.settings
                    .custom_apps
                    .iter()
                    .filter(|c| !c.name.trim().is_empty())
                    .map(|c| c.to_app()),
            )
            .collect()
    }

    /// The entries that fit the selection, per app. Apps with several
    /// actions (the archivers) get a submenu, named by the `String`.
    pub(crate) fn integration_actions(&self) -> Vec<(Option<String>, Vec<MenuAction>)> {
        let selected: Vec<(PathBuf, bool)> = self
            .browser
            .selected_entries()
            .iter()
            .map(|e| (e.path.clone(), e.is_dir))
            .collect();
        let paths: Vec<PathBuf> = selected.iter().map(|(p, _)| p.clone()).collect();
        let ctx = Context {
            selection: &paths,
            dir: self.browser.location(),
        };
        let mut out = Vec::new();
        for app in self.menu_apps() {
            let actions: Vec<MenuAction> = app
                .actions
                .iter()
                .filter(|a| a.applies.fits(&selected))
                .map(|action| MenuAction {
                    label: ctx.label(action),
                    program: app.program.clone(),
                    args: ctx.args(action),
                })
                .collect();
            if !actions.is_empty() {
                out.push(((app.actions.len() > 1).then_some(app.name), actions));
            }
        }
        out
    }

    pub(crate) fn run_integration(&mut self, action: &MenuAction) {
        let dir = self.browser.location().to_path_buf();
        if let Err(err) = integrations::run(&action.program, &action.args, &dir) {
            self.toast_error(format!(
                "Couldn't start {}: {err}",
                action.program.display()
            ));
        }
    }
}
