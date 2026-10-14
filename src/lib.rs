//! Explorist: a fast, keyboard-friendly file manager.
//!
//! The crate is split into a library and a thin binary so the file-system
//! logic can be tested without starting the UI.

pub mod app;
pub mod config;
pub mod fs;
pub mod ui;
pub mod update;
