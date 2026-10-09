//! Lucide icons (ISC), rasterized from their SVG paths at the exact physical
//! pixel size they're drawn at, then tinted. The real app tessellates the
//! paths with lyon instead; the prototype only needs them crisp.

use super::tokens::size;
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
    Navigation,
    Crosshair,
    Keyboard,
    Clock,
}

impl Icon {
    fn svg(self) -> &'static str {
        match self {
            Icon::Radar => include_str!("../../../assets/icons/radar.svg"),
            Icon::Layers => include_str!("../../../assets/icons/layers.svg"),
            Icon::Search => include_str!("../../../assets/icons/search.svg"),
            Icon::Play => include_str!("../../../assets/icons/play.svg"),
            Icon::Pause => include_str!("../../../assets/icons/pause.svg"),
            Icon::ChevronDown => include_str!("../../../assets/icons/chevron-down.svg"),
            Icon::Navigation => include_str!("../../../assets/icons/navigation.svg"),
            Icon::Crosshair => include_str!("../../../assets/icons/crosshair.svg"),
            Icon::Keyboard => include_str!("../../../assets/icons/keyboard.svg"),
            Icon::Clock => include_str!("../../../assets/icons/clock.svg"),
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
            // Lucide draws 24-unit icons with a 2-unit stroke; DESIGN.md asks for 1.75 px
            // at the drawn size, so scale the stroke accordingly.
            let logical = px as f32 / ctx.pixels_per_point();
            let stroke = size::ICON_STROKE * 24.0 / logical;
            let svg = icon.svg().replace("currentColor", "#ffffff").replace(
                "stroke-width=\"2\"",
                &format!("stroke-width=\"{stroke:.3}\""),
            );
            let tree =
                usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("valid icon svg");
            let mut pixmap = tiny_skia::Pixmap::new(px, px).expect("icon size");
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

    /// Draws `icon` centered in `rect` at `size` logical px, tinted `color`.
    pub fn paint(&mut self, painter: &Painter, rect: Rect, icon: Icon, size: f32, color: Color32) {
        let ctx = painter.ctx().clone();
        let px = (size * ctx.pixels_per_point()).round() as u32;
        let tex = self.texture(&ctx, icon, px);
        let half = size / 2.0;
        let c = rect.center();
        // Snap to the physical pixel grid so strokes stay sharp.
        let ppp = ctx.pixels_per_point();
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
