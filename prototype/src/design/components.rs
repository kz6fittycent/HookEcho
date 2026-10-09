//! Component library (docs/DESIGN.md §8), prototype subset. Everything on
//! screen is drawn through these; no stock egui widgets.

use super::icons::{Icon, IconCache};
use super::tokens::{Theme, Type, radius, size, space};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Galley, Painter, Pos2, Rect, Stroke, StrokeKind,
    TextureId, epaint::RectShape, pos2, vec2,
};
use std::sync::Arc;

/// Everything a component needs to paint itself.
pub struct Kit<'a> {
    pub painter: &'a Painter,
    pub theme: &'a Theme,
    pub icons: &'a mut IconCache,
    /// The blurred scene, for glass backdrops.
    pub blur: TextureId,
    /// Full screen rect in points (the blur texture covers exactly this).
    pub screen: Rect,
}

impl Kit<'_> {
    pub fn galley(&self, text: impl Into<String>, ty: Type, color: Color32) -> Arc<Galley> {
        self.painter.layout_no_wrap(text.into(), ty.font(), color)
    }

    pub fn text(
        &self,
        pos: Pos2,
        anchor: Align2,
        text: impl Into<String>,
        ty: Type,
        color: Color32,
    ) -> Rect {
        self.painter
            .text(pos, anchor, text.into(), ty.font(), color)
    }

    /// A floating glass surface: shadow, blurred backdrop, tint, hairline and
    /// the 1 px "lit" top highlight.
    pub fn glass(&self, rect: Rect, r: u8, elevation: u8) {
        let cr = CornerRadius::same(r);
        self.painter
            .add(self.theme.shadow(elevation).as_shape(rect, cr));
        let uv = Rect::from_min_max(
            pos2(
                (rect.min.x - self.screen.min.x) / self.screen.width(),
                (rect.min.y - self.screen.min.y) / self.screen.height(),
            ),
            pos2(
                (rect.max.x - self.screen.min.x) / self.screen.width(),
                (rect.max.y - self.screen.min.y) / self.screen.height(),
            ),
        );
        self.painter
            .add(RectShape::filled(rect, cr, Color32::WHITE).with_texture(self.blur, uv));
        self.painter.rect(
            rect,
            cr,
            self.theme.glass_fill,
            Stroke::new(
                1.0 / self.painter.ctx().pixels_per_point(),
                self.theme.glass_stroke,
            ),
            StrokeKind::Inside,
        );
        // Inner top highlight, inset by the corner radius so it reads as a lit edge.
        let ppp = self.painter.ctx().pixels_per_point();
        let y = rect.min.y + 1.0 / ppp + 0.5 / ppp;
        self.painter.line_segment(
            [
                pos2(rect.min.x + r as f32, y),
                pos2(rect.max.x - r as f32, y),
            ],
            Stroke::new(1.0 / ppp, self.theme.glass_highlight),
        );
    }

    pub fn icon(&mut self, rect: Rect, icon: Icon, size: f32, color: Color32) {
        self.icons.paint(self.painter, rect, icon, size, color);
    }

    pub fn status_dot(&self, center: Pos2, color: Color32) {
        self.painter.circle_filled(
            center,
            size::STATUS_DOT / 2.0 + 2.5,
            color.gamma_multiply(0.22),
        );
        self.painter
            .circle_filled(center, size::STATUS_DOT / 2.0, color);
    }

    /// Icon button inside a glass cluster. `hovered` comes from the caller's
    /// hit test.
    pub fn icon_button(&mut self, rect: Rect, icon: Icon, hovered: bool, selected: bool) {
        if selected {
            self.painter
                .rect_filled(rect, CornerRadius::same(radius::SM), self.theme.selected);
        } else if hovered {
            self.painter
                .rect_filled(rect, CornerRadius::same(radius::SM), self.theme.hover);
        }
        let color = if selected {
            self.theme.accent
        } else if hovered {
            self.theme.text_primary
        } else {
            self.theme.text_secondary
        };
        self.icon(rect, icon, size::ICON_MD, color);
    }

    pub fn keycap(&self, pos: Pos2, label: &str) -> Rect {
        let g = self.galley(label, Type::Caption, self.theme.text_secondary);
        let rect = Rect::from_min_size(pos, vec2(g.size().x + space::S2 + 2.0, 20.0));
        self.painter.rect(
            rect,
            CornerRadius::same(6),
            self.theme.hover,
            Stroke::new(1.0, self.theme.glass_stroke),
            StrokeKind::Inside,
        );
        self.painter
            .galley(rect.center() - g.size() / 2.0, g, self.theme.text_secondary);
        rect
    }

    /// Hover state helper for painted controls.
    pub fn hovered(&self, rect: Rect) -> bool {
        self.painter
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| rect.contains(p))
    }
}

/// Horizontal run of differently-styled text pieces, laid out left to right.
pub fn row(kit: &Kit, mut x: f32, center_y: f32, parts: &[(String, Type, Color32, f32)]) -> f32 {
    for (text, ty, color, gap_before) in parts {
        x += gap_before;
        let g = kit.galley(text.clone(), *ty, *color);
        let w = g.size().x;
        kit.painter
            .galley(pos2(x, center_y - g.size().y / 2.0), g, *color);
        x += w;
    }
    x
}

pub fn measure(kit: &Kit, parts: &[(String, Type, Color32, f32)]) -> f32 {
    parts
        .iter()
        .map(|(t, ty, c, gap)| gap + kit.galley(t.clone(), *ty, *c).size().x)
        .sum()
}

pub fn hairline(ctx: &egui::Context) -> f32 {
    1.0 / ctx.pixels_per_point()
}
