//! Surface observation tokens (docs/DESIGN.md §2.4 data colors): the
//! station plot on the map and the station panel. The plot follows the
//! usual weather-map station model, simplified: temperature upper left,
//! dewpoint lower left, a wind barb from the station dot. Data colors, the
//! same in both themes, over a dark halo like the warning outlines.

use eframe::egui::{Color32, Vec2, vec2};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Temperature: the warm red weather maps use, lightened for a dark map.
pub const TEMP: Color32 = rgb(0xFF7A6B);
/// Dewpoint: green, as on surface analyses.
pub const DEWPOINT: Color32 = rgb(0x5EDC7F);
/// Wind barbs and the station dot.
pub const WIND: Color32 = rgb(0xF2F4F7);
pub const HALO: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 190);

/// Plots are drawn only zoomed in past this (ground km per logical px).
pub const MAX_KM_PER_PX: f64 = 2.5;
/// No two plots closer than this, px (the rest are skipped).
pub const MIN_SPACING: f32 = 64.0;
/// Kept clear of place labels around a plot: left, up/down, right of the
/// dot, px.
pub const LABEL_CLEAR: [f32; 3] = [34.0, 18.0, 22.0];
/// Click radius around a station dot, px.
pub const HIT_R: f32 = 14.0;

pub const DOT_R: f32 = 2.5;
pub const BARB_SHAFT: f32 = 26.0;
pub const BARB_FEATHER: f32 = 10.0;
pub const BARB_STEP: f32 = 4.5;
pub const BARB_STROKE: f32 = 1.5;
pub const HALO_EXTRA: f32 = 2.0;
/// Calm: a ring around the dot.
pub const CALM_R: f32 = 5.0;
/// Text offset from the dot, px (x left, y up/down).
pub const TEXT_DX: f32 = 6.0;
pub const TEXT_DY: f32 = 9.0;

/// The station panel (bottom left, above the timeline row).
pub const PANEL_W: f32 = 316.0;
pub const PANEL_LINE_H: f32 = 20.0;
pub const PANEL_VALUE_H: f32 = 34.0;
/// Offsets of the dark copies drawn under plot numbers, a 1 px halo.
pub const TEXT_HALO: [Vec2; 4] = [
    vec2(1.0, 1.0),
    vec2(-1.0, 1.0),
    vec2(1.0, -1.0),
    vec2(-1.0, -1.0),
];
