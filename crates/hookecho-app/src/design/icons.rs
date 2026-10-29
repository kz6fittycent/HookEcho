//! Lucide icons (ISC), rasterized from their SVG paths at the exact physical
//! pixel size they're drawn at, then tinted (one white mask per size).

use super::tokens::size;
use crate::assets::asset_str;
use eframe::egui::{self, Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2};
use resvg::{tiny_skia, usvg};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Radar,
    Layers,
    Search,
    Play,
    Pause,
    ChevronDown,
    Star,
    X,
    Keyboard,
    Clock,
    Crosshair,
    MapPin,
    Palette,
    Settings,
    Gauge,
    Zap,
    Menu,
    Pin,
    PinOff,
    ChevronUp,
}

impl Icon {
    fn svg(self) -> &'static str {
        match self {
            Icon::Radar => asset_str!("icons/radar.svg"),
            Icon::Layers => asset_str!("icons/layers.svg"),
            Icon::Search => asset_str!("icons/search.svg"),
            Icon::Play => asset_str!("icons/play.svg"),
            Icon::Pause => asset_str!("icons/pause.svg"),
            Icon::ChevronDown => asset_str!("icons/chevron-down.svg"),
            Icon::Star => asset_str!("icons/star.svg"),
            Icon::X => asset_str!("icons/x.svg"),
            Icon::Keyboard => asset_str!("icons/keyboard.svg"),
            Icon::Clock => asset_str!("icons/clock.svg"),
            Icon::Crosshair => asset_str!("icons/crosshair.svg"),
            Icon::MapPin => asset_str!("icons/map-pin.svg"),
            Icon::Palette => asset_str!("icons/palette.svg"),
            Icon::Settings => asset_str!("icons/settings-2.svg"),
            Icon::Gauge => asset_str!("icons/gauge.svg"),
            Icon::Zap => asset_str!("icons/zap.svg"),
            Icon::Menu => asset_str!("icons/menu.svg"),
            Icon::Pin => asset_str!("icons/pin.svg"),
            Icon::PinOff => asset_str!("icons/pin-off.svg"),
            Icon::ChevronUp => asset_str!("icons/chevron-up.svg"),
        }
    }
}

#[derive(Default)]
pub struct IconCache {
    textures: HashMap<(Icon, u32), TextureHandle>,
}

impl IconCache {
    fn texture(&mut self, ctx: &egui::Context, icon: Icon, px: u32) -> &TextureHandle {
        self.textures.entry((icon, px)).or_insert_with(|| {
            // Lucide draws 24-unit icons with a 2-unit stroke; scale it so the
            // drawn stroke is size::ICON_STROKE logical px.
            let logical = px as f32 / ctx.pixels_per_point();
            let stroke = size::ICON_STROKE * 24.0 / logical;
            let svg = icon.svg().replace("currentColor", "#ffffff").replace(
                "stroke-width=\"2\"",
                &format!("stroke-width=\"{stroke:.3}\""),
            );
            let tree =
                usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("bundled icon svg");
            let mut pixmap = tiny_skia::Pixmap::new(px.max(1), px.max(1)).expect("icon size");
            let s = px as f32 / 24.0;
            resvg::render(
                &tree,
                tiny_skia::Transform::from_scale(s, s),
                &mut pixmap.as_mut(),
            );
            let image =
                ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data());
            ctx.load_texture(format!("icon-{icon:?}-{px}"), image, TextureOptions::LINEAR)
        })
    }

    /// Draws `icon` centered in `rect` at `size` logical px, tinted `color`,
    /// snapped to the physical pixel grid so strokes stay sharp.
    pub fn paint(&mut self, painter: &Painter, rect: Rect, icon: Icon, size: f32, color: Color32) {
        let ctx = painter.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let px = (size * ppp).round() as u32;
        let tex = self.texture(&ctx, icon, px);
        let c = rect.center();
        let half = size / 2.0;
        let min = pos2(
            ((c.x - half) * ppp).round() / ppp,
            ((c.y - half) * ppp).round() / ppp,
        );
        painter.image(
            tex.id(),
            Rect::from_min_size(min, egui::vec2(size, size)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            color,
        );
    }
}
