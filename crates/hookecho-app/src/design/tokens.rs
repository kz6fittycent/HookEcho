//! Design tokens (docs/DESIGN.md §2). The only place in the app where
//! colors, sizes, radii, type sizes and durations are spelled out; CI's
//! design-token check rejects literals elsewhere.

use eframe::egui::{Color32, FontFamily, FontId};
use hookecho_render::{LabelStyle, Rgba, SceneStyle};

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
    pub const S6: f32 = 32.0;
}

pub mod radius {
    /// Timeline track and ticks.
    pub const HAIR: u8 = 1;
    /// Timeline playhead.
    pub const XS: u8 = 2;
    pub const SM: u8 = 8;
    pub const MD: u8 = 12;
    pub const LG: u8 = 16;
    /// Pills (toggle tracks).
    pub const FULL: u8 = 255;
}

pub mod size {
    pub const CHIP_H: f32 = 56.0;
    pub const ICON_BUTTON: f32 = 40.0;
    pub const ICON_SM: f32 = 16.0;
    pub const ICON_MD: f32 = 20.0;
    pub const TIMELINE_H: f32 = 52.0;
    pub const TIMELINE_MAX_W: f32 = 720.0;
    pub const LEGEND_W: f32 = 248.0;
    pub const LEGEND_H: f32 = 60.0;
    /// Notice pill (e.g. "Added color palette ...").
    pub const NOTICE_H: f32 = 36.0;
    /// Palette picker: the preview bar under each palette's name.
    pub const PALETTE_PREVIEW_H: f32 = 6.0;
    pub const LEGEND_BAR_H: f32 = 8.0;
    pub const STATUS_DOT: f32 = 7.0;
    pub const COMPACT_BREAKPOINT: f32 = 1000.0;
    pub const ICON_STROKE: f32 = 1.75;
    pub const PICKER_W: f32 = 380.0;
    pub const PICKER_MAX_H: f32 = 420.0;
    pub const ROW_H: f32 = 40.0;
    pub const FIELD_H: f32 = 40.0;
    pub const INSPECTOR_W: f32 = 212.0;
    pub const TILT_CELL_W: f32 = 40.0;
    pub const TILT_CHIP_W: f32 = 56.0;
    pub const PRODUCT_ROW_H: f32 = 48.0;
    /// Half the gap between a product's name and its description line.
    pub const PRODUCT_LINE_GAP: f32 = 8.0;
    pub const HINT_H: f32 = 22.0;
    pub const SELECTION_BAR_W: f32 = 3.0;
    pub const TILT_CELL_H: f32 = 30.0;
    pub const KEYCAP_H: f32 = 20.0;
    pub const FOCUS_RING: f32 = 2.0;
    pub const HAIRLINE_PX: f32 = 1.0;
    pub const SCROLL_INDICATOR_W: f32 = 3.0;
    pub const MARKER_R: f32 = 5.0;
    pub const MARKER_STROKE: f32 = 1.5;
    /// Dark/light halo stroke under map markers so they read on any radar.
    pub const HALO_STROKE: f32 = 3.0;
    pub const RETICLE_R: f32 = 9.0;
    pub const RETICLE_DOT_R: f32 = 1.75;
    /// Clickable site pill on the overview map.
    pub const SITE_PILL: [f32; 2] = [44.0, 22.0];
    /// Space kept free of place labels around a marker label ([half w, half h], x offset).
    pub const MARKER_LABEL_CLEAR: [f32; 3] = [40.0, 10.0, 30.0];
    /// Baselines of the two text lines inside a chip.
    pub const CHIP_LINE1_Y: f32 = 20.0;
    pub const CHIP_LINE2_Y: f32 = 39.0;
    pub const CHIP_TEXT_GAP: f32 = 6.0;
    /// Sweep strip dots (DESIGN §3.1).
    pub const SWEEP_DOT_R: f32 = 2.5;
    pub const SWEEP_DOT_STEP: f32 = 7.0;
    pub const SWEEP_RING_STROKE: f32 = 1.0;
    pub const CELL_DOT_R: f32 = 3.0;
    /// Inspector card offset from the pointer.
    pub const INSPECTOR_OFFSET: f32 = 18.0;
    pub const INSPECTOR_VALUE_H: f32 = 28.0;
    pub const INSPECTOR_NOTE_H: f32 = 16.0;
    pub const INSPECTOR_ROW_H: f32 = 20.0;
    pub const PICKER_LABEL_W: f32 = 132.0;
    pub const PICKER_ID_COL: f32 = 52.0;
    pub const LEGEND_TITLE_Y: f32 = 15.0;
    pub const LEGEND_BAR_Y: f32 = 26.0;
    pub const LEGEND_TICK_GAP: f32 = 10.0;
    pub const LEGEND_STEPS: usize = 96;
    pub const LEGEND_TICKS: f32 = 5.0;
    pub const LEGEND_BAR_RADIUS: u8 = 2;
    /// Timeline scrubber (DESIGN §3.3).
    pub const TICK: [f32; 2] = [2.0, 10.0];
    pub const PLAYHEAD: [f32; 2] = [4.0, 20.0];
    pub const TRACK_H: f32 = 2.0;
    pub const TRACK_INSET: f32 = 4.0;
    pub const LIVE_BUTTON: [f32; 2] = [64.0, 28.0];
    /// Command palette (DESIGN.md §3.8).
    pub const COMMAND_W: f32 = 560.0;
    pub const COMMAND_MAX_H: f32 = 480.0;
    pub const COMMAND_FIELD_H: f32 = 48.0;
    /// Shortcuts sheet (DESIGN.md §3.9).
    pub const SHORTCUTS_W: f32 = 640.0;
    pub const SHORTCUT_KEY_COL: f32 = 64.0;
    pub const SHORTCUT_ROW_H: f32 = 28.0;
    /// Side sheet (DESIGN.md §3.6).
    pub const SHEET_W: f32 = 360.0;
    pub const SEGMENTED_H: f32 = 32.0;
    /// A settings row: label (and hint) on the left, control on the right.
    pub const SETTING_ROW_H: f32 = 48.0;
    pub const SECTION_GAP: f32 = 24.0;
    pub const TOGGLE: [f32; 2] = [32.0, 18.0];
    pub const TOGGLE_INSET: f32 = 2.0;
    /// Segmented control inside a settings row.
    pub const SETTING_SEGMENT_W: f32 = 64.0;
    pub const SETTING_SEGMENT_H: f32 = 28.0;
    pub const SPEED_BUTTON: [f32; 2] = [36.0, 28.0];
    pub const LIVE_DOT_X: f32 = 14.0;
    pub const LIVE_TEXT_X: f32 = 24.0;
}

pub mod motion {
    /// Hover, press, focus, tooltips (seconds).
    pub const FAST: f32 = 0.150;
    /// Panels, sheets, pickers.
    pub const BASE: f32 = 0.200;
    /// Map zoom settle time constant.
    pub const ZOOM_TAU: f32 = 0.070;
    /// Fling velocity half-life.
    pub const FLING_HALF_LIFE: f32 = 0.120;
    /// Delay before a tooltip appears.
    pub const TOOLTIP_DELAY: f32 = 0.5;
    /// Delay before the inspector appears at rest.
    pub const INSPECTOR_DELAY: f32 = 0.12;
}

pub mod family {
    pub const REGULAR: &str = "inter-400";
    pub const MEDIUM: &str = "inter-500";
    pub const SEMIBOLD: &str = "inter-600";
    pub const BOLD: &str = "inter-700";
    pub const TAB_REGULAR: &str = "inter-tab-400";
    pub const TAB_MEDIUM: &str = "inter-tab-500";
    pub const TAB_SEMIBOLD: &str = "inter-tab-600";
    /// Family name inside the font files (for glyphon map labels).
    pub const MAP_LABELS: &str = "Inter";
}

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Debug)]
pub struct Theme {
    pub kind: ThemeKind,
    pub map_water: Color32,
    pub map_land: Color32,
    pub map_state: Color32,
    pub map_county: Color32,
    pub map_road: Color32,
    pub map_country: Color32,
    pub map_label: Color32,
    pub map_label_halo: Color32,
    pub radar_alpha: f32,
    pub glass_fill: Color32,
    pub glass_stroke: Color32,
    pub glass_highlight: Color32,
    pub glass_shadow: Color32,
    pub blur_saturation: f32,
    pub blur_tone: f32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub accent: Color32,
    pub accent_on: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub track: Color32,
    pub status_fresh: Color32,
    pub status_aging: Color32,
    pub status_stale: Color32,
    pub me_ring: Color32,
}

impl Theme {
    pub fn get(kind: ThemeKind) -> Theme {
        match kind {
            ThemeKind::Dark => Theme {
                kind,
                map_water: rgb(0x0D1014),
                map_land: rgb(0x15181D),
                map_state: rgba(0xFFFFFF, 40),
                map_county: rgba(0xFFFFFF, 11),
                map_road: rgba(0x8FA3BF, 30),
                map_country: rgba(0xFFFFFF, 50),
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
                accent: rgb(0x78AEFF),
                accent_on: rgb(0x0B1A33),
                hover: rgba(0xFFFFFF, 6),
                pressed: rgba(0xFFFFFF, 10),
                selected: rgba(0x78AEFF, 18),
                track: rgba(0xFFFFFF, 14),
                status_fresh: rgb(0x3CCF8E),
                status_aging: rgb(0xF2B640),
                status_stale: rgb(0xFF6363),
                me_ring: rgb(0xFFFFFF),
            },
            ThemeKind::Light => Theme {
                kind,
                map_water: rgb(0xD9DEE5),
                map_land: rgb(0xECEEF1),
                map_state: rgba(0x1D2430, 45),
                map_county: rgba(0x1D2430, 14),
                map_road: rgba(0x4F6480, 30),
                map_country: rgba(0x1D2430, 55),
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
                pressed: rgba(0x0B1220, 9),
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

    /// What the renderer needs from this theme.
    pub fn scene_style(&self) -> SceneStyle {
        let srgba = |c: Color32| c.to_srgba_unmultiplied();
        SceneStyle {
            water: lin(self.map_water),
            land: lin(self.map_land),
            lakes: lin(self.map_water),
            lines: vec![
                ("county", 0.75, lin(self.map_county), false),
                ("road", 1.0, lin(self.map_road), false),
                ("province", 1.0, lin(self.map_state), true),
                ("country", 1.5, lin(self.map_country), true),
                ("state", 1.25, lin(self.map_state), true),
            ],
            labels: LabelStyle {
                color: srgba(self.map_label),
                halo: srgba(self.map_label_halo),
                size: 12.0,
                family: family::MAP_LABELS,
                visible: true,
            },
            radar_alpha: self.radar_alpha,
            blur_sigma: 18.0,
            blur_saturation: self.blur_saturation,
            blur_tone: self.blur_tone,
            blur_tone_light: self.kind == ThemeKind::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_lin(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    fn to_gamma(l: f32) -> f32 {
        if l <= 0.003_130_8 {
            l * 12.92
        } else {
            1.055 * l.powf(1.0 / 2.4) - 0.055
        }
    }

    /// Unmultiplied sRGB (0..1) and alpha.
    fn srgb(c: Color32) -> ([f32; 3], f32) {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        (
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0],
            a as f32 / 255.0,
        )
    }

    /// egui composites in sRGB (gamma) space into a gamma framebuffer, so
    /// blending is modeled there, exactly as it reaches the screen.
    fn over(fg: Color32, bg: [f32; 3]) -> [f32; 3] {
        let (f, a) = srgb(fg);
        [0, 1, 2].map(|i| f[i] * a + bg[i] * (1.0 - a))
    }

    fn luminance(g: [f32; 3]) -> f32 {
        0.2126 * to_lin(g[0]) + 0.7152 * to_lin(g[1]) + 0.0722 * to_lin(g[2])
    }

    fn ratio(a: [f32; 3], b: [f32; 3]) -> f32 {
        let (la, lb) = (luminance(a) + 0.05, luminance(b) + 0.05);
        la.max(lb) / la.min(lb)
    }

    /// WCAG AA (4.5:1) for every text color on glass, against the worst-case
    /// backdrop the tone-compressed blur can produce (docs/DESIGN.md §2.4,
    /// §2.6): [0, k] in dark mode, [1-k, 1] in light mode (linear light).
    /// Accent text is only ever used on plain glass; on hover and selected
    /// fills, labels use primary text.
    #[test]
    fn text_contrast_meets_aa() {
        for kind in [ThemeKind::Dark, ThemeKind::Light] {
            let t = Theme::get(kind);
            let k = t.blur_tone;
            let extremes = match kind {
                ThemeKind::Dark => [0.0, k],
                ThemeKind::Light => [1.0 - k, 1.0],
            };
            for b in extremes {
                let backdrop = [to_gamma(b); 3];
                let glass = over(t.glass_fill, backdrop);
                let text = |c: Color32| srgb(c).0;
                for (name, fg) in [
                    ("primary", t.text_primary),
                    ("secondary", t.text_secondary),
                    ("tertiary", t.text_tertiary),
                    ("accent", t.accent),
                ] {
                    let c = ratio(text(fg), glass);
                    assert!(c >= 4.5, "{kind:?} {name} on glass (backdrop {b}): {c:.2}");
                }
                for (state, fill) in [
                    ("hover", t.hover),
                    ("pressed", t.pressed),
                    ("selected", t.selected),
                ] {
                    let c = ratio(text(t.text_primary), over(fill, glass));
                    assert!(
                        c >= 4.5,
                        "{kind:?} primary on {state} (backdrop {b}): {c:.2}"
                    );
                }
                // Text on the accent fill (selected tilt cell, primary buttons).
                let c = ratio(text(t.accent_on), over(t.accent, glass));
                assert!(c >= 4.5, "{kind:?} accent_on on accent: {c:.2}");
            }
        }
    }
}
