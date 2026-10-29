//! Hazard colors (docs/DESIGN.md §2.4): the NWS's own map colors, so a
//! warning reads the way it does on weather.gov. Source: "Weather.gov Help -
//! Hazards Map", https://www.weather.gov/help-map (docs/PROVENANCE.md).
//! They're data colors, the same in both themes; a dark halo under each
//! outline keeps the light ones readable on the light basemap.

use eframe::egui::Color32;
use hookecho_core::alerts::Event;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn color(e: Event) -> Color32 {
    match e {
        Event::Tornado => rgb(0xFF0000),
        Event::ExtremeWind => rgb(0xFF8C00),
        Event::SevereThunderstorm => rgb(0xFFA500),
        // The NWS table has flash flood as dark red (8B0000), which is lost
        // next to tornado red on a dark map. DESIGN.md §2.4 makes it green,
        // as radar viewers usually do; this is the table's flood green.
        Event::FlashFlood => rgb(0x00FF00),
        Event::SnowSquall => rgb(0xC71585),
        Event::DustStorm => rgb(0xFFE4C4),
        Event::SpecialMarine => rgb(0xFFA500),
    }
}

/// Outline width, points. Tornado warnings get a heavier line so they stand
/// out where warnings overlap (DESIGN.md §2.7's static alternative to a
/// pulsing outline).
pub fn stroke(e: Event) -> f32 {
    match e {
        Event::Tornado | Event::ExtremeWind => 3.0,
        _ => 2.0,
    }
}

/// Extra width of the dark halo under an outline, points.
pub const HALO: f32 = 2.0;

/// The halo color: near-black, mostly opaque.
pub const HALO_COLOR: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 170);

/// The warning card (a glass card like the pickers, DESIGN.md §3.7):
/// width, height cap, and the colored bar beside each warning's title.
pub const CARD_W: f32 = 380.0;
pub const CARD_MAX_H: f32 = 460.0;
pub const CARD_BAR_W: f32 = 4.0;
