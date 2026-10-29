//! Space weather tokens: NOAA scale levels and the aurora overlay. Data
//! colors, the same in both themes. The scale colors follow SWPC's own
//! scale graphics (yellow for minor up to deep red for extreme,
//! https://www.swpc.noaa.gov/noaa-scales-explanation); level 0 is drawn
//! in the theme's tertiary text instead.

use eframe::egui::Color32;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Levels 1-5.
pub fn scale_color(level: u8) -> Option<Color32> {
    match level {
        1 => Some(rgb(0xF6EB14)),
        2 => Some(rgb(0xFFC800)),
        3 => Some(rgb(0xFF9600)),
        4 => Some(rgb(0xFF0000)),
        5.. => Some(rgb(0xC80000)),
        0 => None,
    }
}

/// The aurora overlay: from faint green at the threshold, through yellow,
/// to red at 50% and above (the ramp of SWPC's aurora dashboard).
pub fn aurora_color(percent: u8) -> Color32 {
    let stops = [
        (5.0, [0x00, 0xC8, 0x50, 50]),
        (30.0, [0x8C, 0xF0, 0x28, 110]),
        (50.0, [0xFF, 0xDC, 0x00, 140]),
        (90.0, [0xFF, 0x28, 0x14, 170]),
    ];
    if percent < AURORA_MIN {
        return Color32::TRANSPARENT;
    }
    let p = percent as f32;
    let i = stops.iter().rposition(|s| s.0 <= p).unwrap_or(0);
    let (a, b) = (stops[i], stops[(i + 1).min(stops.len() - 1)]);
    let f = if b.0 > a.0 {
        ((p - a.0) / (b.0 - a.0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |k: usize| (a.1[k] as f32 + (b.1[k] as f32 - a.1[k] as f32) * f) as u8;
    Color32::from_rgba_unmultiplied(mix(0), mix(1), mix(2), mix(3))
}

/// Grid cells below this chance aren't drawn, percent.
pub const AURORA_MIN: u8 = 5;

/// The space weather card.
pub const CARD_W: f32 = 520.0;
pub const SCALE_CHIP: [f32; 2] = [104.0, 40.0];
pub const KP_CHART_H: f32 = 64.0;
pub const KP_BAR_GAP: f32 = 1.0;
/// Predicted Kp bars are drawn at this opacity.
pub const KP_PREDICTED_FADE: f32 = 0.45;
pub const LINE_H: f32 = 22.0;
