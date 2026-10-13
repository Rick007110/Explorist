//! Bundled Geist fonts.
//!
//! egui has no font weights, so each weight we use is registered as its own
//! font family. Geist is the default proportional font and Geist Mono the
//! default monospace font; egui's built-in fonts stay behind them as
//! fallbacks for symbols Geist lacks.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const GEIST_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Geist-Medium.ttf");
const GEIST_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Geist-SemiBold.ttf");
const GEIST_MONO_REGULAR: &[u8] = include_bytes!("../../assets/fonts/GeistMono-Regular.ttf");
const GEIST_MONO_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/GeistMono-Medium.ttf");

const MEDIUM: &str = "geist-medium";
const SEMIBOLD: &str = "geist-semibold";
const MONO_MEDIUM: &str = "geist-mono-medium";

/// Registers the Geist fonts with egui.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let fallback_proportional = fonts.families[&FontFamily::Proportional].clone();
    let fallback_mono = fonts.families[&FontFamily::Monospace].clone();

    for (name, bytes) in [
        ("geist-regular", GEIST_REGULAR),
        (MEDIUM, GEIST_MEDIUM),
        (SEMIBOLD, GEIST_SEMIBOLD),
        ("geist-mono-regular", GEIST_MONO_REGULAR),
        (MONO_MEDIUM, GEIST_MONO_MEDIUM),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    let with_fallback = |primary: &str, fallback: &[String]| {
        std::iter::once(primary.to_owned())
            .chain(fallback.iter().cloned())
            .collect::<Vec<_>>()
    };

    fonts.families.insert(
        FontFamily::Proportional,
        with_fallback("geist-regular", &fallback_proportional),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        with_fallback("geist-mono-regular", &fallback_mono),
    );
    for (family, fallback) in [
        (MEDIUM, &fallback_proportional),
        (SEMIBOLD, &fallback_proportional),
        (MONO_MEDIUM, &fallback_mono),
    ] {
        fonts.families.insert(
            FontFamily::Name(family.into()),
            with_fallback(family, fallback),
        );
    }

    fonts
}

/// Geist Regular (weight 400).
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Geist Medium (weight 500).
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// Geist SemiBold (weight 600).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Geist Mono Regular, used for paths, sizes, dates and shortcuts.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// Geist Mono Medium.
pub fn mono_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_MEDIUM.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_starts_with_a_bundled_font() {
        let fonts = definitions();
        for family in fonts.families.values() {
            let primary = &family[0];
            assert!(primary.starts_with("geist"), "family starts with {primary}");
            assert!(fonts.font_data.contains_key(primary));
        }
    }

    #[test]
    fn named_families_are_registered() {
        let fonts = definitions();
        for id in [medium(13.0), semibold(13.0), mono_medium(13.0)] {
            assert!(fonts.families.contains_key(&id.family));
        }
    }
}
