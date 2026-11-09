//! Hazard colors (docs/DESIGN.md §2.4): the NWS's own map colors, so a
//! warning reads the way it does on weather.gov. Source: "Weather.gov Help -
//! Hazards Map", https://www.weather.gov/help-map (docs/PROVENANCE.md).
//! They're data colors, the same in both themes; a dark halo under each
//! outline keeps the light ones readable on the light basemap.

use eframe::egui::Color32;
use hookecho_core::alerts::Event;
use hookecho_core::spc::WatchKind;

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

/// Watch outlines, in the same NWS hazard map colors (tornado watch
/// yellow, severe thunderstorm watch pale violet red).
pub fn watch_color(k: WatchKind) -> Color32 {
    match k {
        WatchKind::Tornado => rgb(0xFFFF00),
        WatchKind::SevereThunderstorm => rgb(0xDB7093),
    }
}

/// Watch outline width: heavier for "particularly dangerous situation"
/// watches.
pub fn watch_stroke(pds: bool) -> f32 {
    if pds { 3.0 } else { 2.0 }
}

/// Mesoscale discussions have no NWS map color. SPC's own graphics draw
/// them blue; this is a light blue that stays clear of every warning and
/// watch color, dashed so it reads as "discussion", not "in effect".
pub const MD_COLOR: Color32 = rgb(0x4FC3F7);
pub const MD_STROKE: f32 = 2.0;
pub const MD_DASH: f32 = 8.0;
pub const MD_GAP: f32 = 5.0;

/// Outlook risk areas use SPC's own colors from the feed; the fill is
/// translucent so the radar stays readable through it.
const OUTLOOK_FILL_ALPHA: u8 = 64;

/// An outlook level's outline color, from the feed's `stroke`.
pub fn outlook_stroke(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// An outlook level's translucent fill, from the feed's `fill`.
pub fn outlook_fill(c: [u8; 3]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], OUTLOOK_FILL_ALPHA)
}
pub const OUTLOOK_STROKE: f32 = 1.5;
