//! Application state and the top-level eframe app.

pub mod browser;
pub mod selection;
pub mod undo;

use crate::ui::fonts;

pub struct ExploristApp {}

impl ExploristApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);
        Self {}
    }
}

impl eframe::App for ExploristApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Explorist");
        });
    }
}
