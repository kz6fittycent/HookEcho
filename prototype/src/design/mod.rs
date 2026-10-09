pub mod components;
pub mod icons;
pub mod tokens;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;
use tokens::family;

/// Registers Inter (and the tabular-figures instance) as the only UI fonts.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::empty();
    let fonts: [(&str, &'static [u8]); 7] = [
        (
            family::REGULAR,
            include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
        ),
        (
            family::MEDIUM,
            include_bytes!("../../../assets/fonts/Inter-Medium.ttf"),
        ),
        (
            family::SEMIBOLD,
            include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"),
        ),
        (
            family::BOLD,
            include_bytes!("../../../assets/fonts/Inter-Bold.ttf"),
        ),
        (
            family::TAB_REGULAR,
            include_bytes!("../../../assets/fonts/InterTabular-Regular.ttf"),
        ),
        (
            family::TAB_MEDIUM,
            include_bytes!("../../../assets/fonts/InterTabular-Medium.ttf"),
        ),
        (
            family::TAB_SEMIBOLD,
            include_bytes!("../../../assets/fonts/InterTabular-SemiBold.ttf"),
        ),
    ];
    for (name, bytes) in fonts {
        defs.font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
        defs.families
            .insert(FontFamily::Name(name.into()), vec![name.into()]);
    }
    // egui requires these two families to exist; both map to Inter so egui's
    // default font can never appear.
    defs.families
        .insert(FontFamily::Proportional, vec![family::REGULAR.into()]);
    defs.families
        .insert(FontFamily::Monospace, vec![family::TAB_REGULAR.into()]);
    ctx.set_fonts(defs);
}
