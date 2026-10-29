//! Hook Echo design system (docs/DESIGN.md): tokens, fonts, icons and the
//! component library every screen is built from.

pub mod components;
pub mod hazards;
pub mod icons;
pub mod tokens;

use crate::assets;
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;
use tokens::family;

/// Registers Inter (and its tabular-figures instance) as the only UI fonts.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::empty();
    let fonts: [(&str, &'static [u8]); 7] = [
        (family::REGULAR, assets::INTER_REGULAR),
        (family::MEDIUM, assets::INTER_MEDIUM),
        (family::SEMIBOLD, assets::INTER_SEMIBOLD),
        (family::BOLD, assets::INTER_BOLD),
        (family::TAB_REGULAR, assets::INTER_TAB_REGULAR),
        (family::TAB_MEDIUM, assets::INTER_TAB_MEDIUM),
        (family::TAB_SEMIBOLD, assets::INTER_TAB_SEMIBOLD),
    ];
    for (name, bytes) in fonts {
        defs.font_data
            .insert(name.into(), Arc::new(FontData::from_static(bytes)));
        defs.families
            .insert(FontFamily::Name(name.into()), vec![name.into()]);
    }
    // egui requires both built-in families; both map to Inter so egui's own
    // default font can never appear.
    defs.families
        .insert(FontFamily::Proportional, vec![family::REGULAR.into()]);
    defs.families
        .insert(FontFamily::Monospace, vec![family::TAB_REGULAR.into()]);
    ctx.set_fonts(defs);
}

/// Font files handed to the renderer for map labels.
pub fn map_label_fonts() -> Vec<Vec<u8>> {
    vec![
        assets::INTER_MEDIUM.to_vec(),
        assets::INTER_SEMIBOLD.to_vec(),
    ]
}
