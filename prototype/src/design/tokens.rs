//! Design tokens (docs/DESIGN.md §2). The only place colors, sizes, radii,
//! type sizes and durations are spelled out.

use eframe::egui::{Color32, FontFamily, FontId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeKind {
    Dark,
    Light,
}

pub mod space {
    pub const S1: f32 = 4.0;
    pub const S2: f32 = 8.0;
    pub const S3: f32 = 12.0;
    pub const S4: f32 = 16.0;
    pub const S5: f32 = 24.0;
}

pub mod radius {
    pub const SM: u8 = 8;
    pub const MD: u8 = 12;
    pub const LG: u8 = 16;
}

pub mod size {
    /// Height of top chips and the icon cluster.
    pub const CHIP_H: f32 = 56.0;
    pub const ICON_BUTTON: f32 = 40.0;
    pub const ICON_SM: f32 = 16.0;
    pub const ICON_MD: f32 = 20.0;
    pub const TIMELINE_H: f32 = 52.0;
    pub const TIMELINE_MAX_W: f32 = 720.0;
    pub const LEGEND_W: f32 = 248.0;
    pub const STATUS_DOT: f32 = 7.0;
    pub const COMPACT_BREAKPOINT: f32 = 1000.0;
    pub const ICON_STROKE: f32 = 1.75;
}

/// Font families registered in `design::fonts`.
pub mod family {
    pub const REGULAR: &str = "inter-400";
    pub const MEDIUM: &str = "inter-500";
    pub const SEMIBOLD: &str = "inter-600";
    pub const BOLD: &str = "inter-700";
    pub const TAB_REGULAR: &str = "inter-tab-400";
    pub const TAB_MEDIUM: &str = "inter-tab-500";
    pub const TAB_SEMIBOLD: &str = "inter-tab-600";
}

#[derive(Clone, Copy)]
pub enum Type {
    Caption,
    CaptionTab,
    Body,
    BodyTab,
    Label,
    LabelTab,
    Title,
    Value,
}

impl Type {
    pub fn font(self) -> FontId {
        let (size, fam) = match self {
            Type::Caption => (12.0, family::MEDIUM),
            Type::CaptionTab => (12.0, family::TAB_MEDIUM),
            Type::Body => (13.0, family::REGULAR),
            Type::BodyTab => (13.0, family::TAB_REGULAR),
            Type::Label => (15.0, family::SEMIBOLD),
            Type::LabelTab => (15.0, family::TAB_SEMIBOLD),
            Type::Title => (17.0, family::SEMIBOLD),
            Type::Value => (22.0, family::TAB_SEMIBOLD),
        };
        FontId::new(size, FontFamily::Name(fam.into()))
    }
}

/// Map label font (rendered by glyphon in the real renderer).
pub fn map_label_font(major: bool) -> FontId {
    if major {
        FontId::new(13.0, FontFamily::Name(family::SEMIBOLD.into()))
    } else {
        FontId::new(12.0, FontFamily::Name(family::MEDIUM.into()))
    }
}

pub fn map_label_font_bold() -> FontId {
    FontId::new(12.0, FontFamily::Name(family::BOLD.into()))
}

/// Linear RGBA in 0..1 for the GPU scene.
pub type Rgba = [f32; 4];

pub fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Unmultiplied color with alpha in percent.
fn rgba(hex: u32, alpha_pct: u32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (hex >> 16) as u8,
        (hex >> 8) as u8,
        hex as u8,
        (alpha_pct * 255 / 100) as u8,
    )
}

fn lin(c: Color32) -> Rgba {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [
        srgb_to_linear(r),
        srgb_to_linear(g),
        srgb_to_linear(b),
        a as f32 / 255.0,
    ]
}

#[derive(Clone)]
pub struct Theme {
    pub kind: ThemeKind,
    // Map
    pub map_land: Rgba,
    pub map_state: Rgba,
    pub map_county: Rgba,
    pub map_road: Rgba,
    pub map_label: Color32,
    pub map_label_halo: Color32,
    pub radar_alpha: f32,
    // Glass
    pub glass_fill: Color32,
    pub glass_stroke: Color32,
    pub glass_highlight: Color32,
    pub glass_shadow: Color32,
    pub blur_saturation: f32,
    /// Backdrop tone compression: the blurred scene is squeezed into
    /// [0, k] (dark) or [1-k, 1] (light) so glass text passes WCAG AA over
    /// any radar color.
    pub blur_tone: f32,
    // Text
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    // Accent + states
    pub accent: Color32,
    pub accent_on: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub track: Color32,
    // Status
    pub status_fresh: Color32,
    pub status_aging: Color32,
    pub status_stale: Color32,
    // Me marker
    pub me_ring: Color32,
}

impl Theme {
    pub fn get(kind: ThemeKind) -> Theme {
        match kind {
            ThemeKind::Dark => Theme {
                kind,
                map_land: lin(rgb(0x15181D)),
                map_state: lin(rgba(0xFFFFFF, 40)),
                map_county: lin(rgba(0xFFFFFF, 11)),
                map_road: lin(rgba(0x8FA3BF, 30)),
                map_label: rgba(0xE6E9EE, 82),
                map_label_halo: rgba(0x15181D, 80),
                radar_alpha: 0.92,
                glass_fill: rgba(0x1B1F26, 66),
                glass_stroke: rgba(0xFFFFFF, 9),
                glass_highlight: rgba(0xFFFFFF, 5),
                glass_shadow: rgba(0x000000, 45),
                blur_saturation: 1.25,
                blur_tone: 0.16,
                text_primary: rgb(0xF3F5F8),
                text_secondary: rgb(0xCDD2DA),
                text_tertiary: rgb(0xB3BAC4),
                accent: rgb(0x5C9DFF),
                accent_on: rgb(0x0B1A33),
                hover: rgba(0xFFFFFF, 6),
                selected: rgba(0x5C9DFF, 18),
                track: rgba(0xFFFFFF, 14),
                status_fresh: rgb(0x3CCF8E),
                status_aging: rgb(0xF2B640),
                status_stale: rgb(0xFF6363),
                me_ring: rgb(0xFFFFFF),
            },
            ThemeKind::Light => Theme {
                kind,
                map_land: lin(rgb(0xECEEF1)),
                map_state: lin(rgba(0x1D2430, 45)),
                map_county: lin(rgba(0x1D2430, 14)),
                map_road: lin(rgba(0x4F6480, 30)),
                map_label: rgba(0x1D2430, 85),
                map_label_halo: rgba(0xECEEF1, 85),
                radar_alpha: 0.88,
                glass_fill: rgba(0xFFFFFF, 70),
                glass_stroke: rgba(0x0B1220, 10),
                glass_highlight: rgba(0xFFFFFF, 60),
                glass_shadow: rgba(0x0B1220, 16),
                blur_saturation: 1.05,
                blur_tone: 0.35,
                text_primary: rgb(0x121722),
                text_secondary: rgb(0x4A5466),
                text_tertiary: rgb(0x626C7D),
                accent: rgb(0x2167E0),
                accent_on: rgb(0xFFFFFF),
                hover: rgba(0x0B1220, 5),
                selected: rgba(0x2167E0, 14),
                track: rgba(0x0B1220, 12),
                status_fresh: rgb(0x13985C),
                status_aging: rgb(0xB87800),
                status_stale: rgb(0xD93636),
                me_ring: rgb(0xFFFFFF),
            },
        }
    }

    pub fn shadow(&self, level: u8) -> eframe::egui::epaint::Shadow {
        let (y, blur) = match level {
            1 => (4, 14),
            2 => (8, 24),
            _ => (16, 40),
        };
        eframe::egui::epaint::Shadow {
            offset: [0, y],
            blur,
            spread: 0,
            color: self.glass_shadow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: [f32; 3]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    fn over(fg: Color32, bg: [f32; 3]) -> [f32; 3] {
        let l = lin(fg);
        [0, 1, 2].map(|i| l[i] * l[3] + bg[i] * (1.0 - l[3]))
    }

    fn contrast(a: [f32; 3], b: [f32; 3]) -> f32 {
        let (la, lb) = (luminance(a) + 0.05, luminance(b) + 0.05);
        la.max(lb) / la.min(lb)
    }

    /// Text on glass must pass WCAG AA against the worst-case backdrop
    /// (glass over pure black and over pure white radar colors).
    #[test]
    fn text_contrast_meets_aa() {
        for kind in [ThemeKind::Dark, ThemeKind::Light] {
            let t = Theme::get(kind);
            // Worst cases after tone compression (see scene.wgsl fs_blur).
            let k = t.blur_tone;
            let extremes = match kind {
                ThemeKind::Dark => [[0.0; 3], [k; 3]],
                ThemeKind::Light => [[1.0 - k; 3], [1.0; 3]],
            };
            for backdrop in extremes {
                let surface = over(t.glass_fill, backdrop);
                for (name, fg, min) in [
                    ("primary", t.text_primary, 4.5),
                    ("secondary", t.text_secondary, 4.5),
                    ("tertiary", t.text_tertiary, 4.5),
                ] {
                    let c = contrast(lin(fg)[..3].try_into().unwrap(), surface);
                    assert!(c >= min, "{kind:?} {name} on {backdrop:?}: {c:.2}");
                }
            }
        }
    }
}
