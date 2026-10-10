//! Component library (docs/DESIGN.md §8). Every visible element is painted
//! here; stock egui widgets never appear. Interaction goes through
//! `Ui::interact`, which also gives each component an AccessKit node.

use super::icons::{Icon, IconCache};
use super::tokens::{Theme, Type, motion, radius, size, space};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Galley, Id, LayerId, Order, Painter, Pos2, Rect, Response,
    Sense, Stroke, StrokeKind, TextureId, Ui, WidgetInfo, WidgetType, epaint::RectShape, pos2,
    vec2,
};
use std::sync::Arc;

/// Shared paint context: theme, icons, and the blurred scene for glass.
pub struct Kit<'a> {
    pub theme: &'a Theme,
    pub icons: &'a mut IconCache,
    pub blur: TextureId,
    /// The rect the blur texture covers (the whole viewport).
    pub screen: Rect,
}

impl Kit<'_> {
    pub fn galley(
        &self,
        painter: &Painter,
        text: impl Into<String>,
        ty: Type,
        color: Color32,
    ) -> Arc<Galley> {
        painter.layout_no_wrap(text.into(), ty.font(), color)
    }

    pub fn text(
        &self,
        painter: &Painter,
        pos: Pos2,
        anchor: Align2,
        text: impl Into<String>,
        ty: Type,
        color: Color32,
    ) -> Rect {
        painter.text(pos, anchor, text.into(), ty.font(), color)
    }

    /// Floating glass: shadow, blurred backdrop, tint, hairline, and the 1 px
    /// "lit" top edge.
    pub fn glass(&self, painter: &Painter, rect: Rect, r: u8, elevation: u8) {
        let cr = CornerRadius::same(r);
        painter.add(self.theme.shadow(elevation).as_shape(rect, cr));
        let s = self.screen;
        let uv = Rect::from_min_max(
            pos2(
                (rect.min.x - s.min.x) / s.width(),
                (rect.min.y - s.min.y) / s.height(),
            ),
            pos2(
                (rect.max.x - s.min.x) / s.width(),
                (rect.max.y - s.min.y) / s.height(),
            ),
        );
        painter.add(RectShape::filled(rect, cr, Color32::WHITE).with_texture(self.blur, uv));
        let hair = hairline(painter.ctx());
        painter.rect(
            rect,
            cr,
            self.theme.glass_fill,
            Stroke::new(hair, self.theme.glass_stroke),
            StrokeKind::Inside,
        );
        let y = rect.min.y + hair * 1.5;
        painter.line_segment(
            [
                pos2(rect.min.x + r as f32, y),
                pos2(rect.max.x - r as f32, y),
            ],
            Stroke::new(hair, self.theme.glass_highlight),
        );
    }

    pub fn icon(&mut self, painter: &Painter, rect: Rect, icon: Icon, sz: f32, color: Color32) {
        self.icons.paint(painter, rect, icon, sz, color);
    }

    pub fn status_dot(&self, painter: &Painter, center: Pos2, color: Color32) {
        painter.circle_filled(
            center,
            size::STATUS_DOT / 2.0 + 2.5,
            color.gamma_multiply(0.22),
        );
        painter.circle_filled(center, size::STATUS_DOT / 2.0, color);
    }

    /// Hover/press/selected overlay for an interactive rect.
    pub fn state_fill(
        &self,
        painter: &Painter,
        rect: Rect,
        r: u8,
        resp: &Response,
        selected: bool,
    ) {
        let cr = CornerRadius::same(r);
        let t = painter.ctx().animate_bool_with_time(
            resp.id.with("hover"),
            resp.hovered(),
            motion::FAST,
        );
        if selected {
            painter.rect_filled(rect, cr, self.theme.selected);
        }
        if resp.is_pointer_button_down_on() {
            painter.rect_filled(rect, cr, self.theme.pressed);
        } else if t > 0.0 {
            painter.rect_filled(rect, cr, self.theme.hover.gamma_multiply(t));
        }
    }

    /// Visible keyboard focus: an accent ring outside the element's radius.
    pub fn focus_ring(&self, painter: &Painter, rect: Rect, r: u8, resp: &Response) {
        if resp.has_focus() {
            painter.rect_stroke(
                rect.expand(size::FOCUS_RING),
                CornerRadius::same(r + size::FOCUS_RING as u8),
                Stroke::new(size::FOCUS_RING, self.theme.accent),
                StrokeKind::Outside,
            );
        }
    }

    pub fn keycap(&self, painter: &Painter, pos: Pos2, label: &str) -> Rect {
        let g = self.galley(painter, label, Type::Caption, self.theme.text_secondary);
        let rect = Rect::from_min_size(pos, vec2(g.size().x + space::S2 + 2.0, size::KEYCAP_H));
        painter.rect(
            rect,
            CornerRadius::same(6),
            self.theme.hover,
            Stroke::new(hairline(painter.ctx()), self.theme.glass_stroke),
            StrokeKind::Inside,
        );
        painter.galley(rect.center() - g.size() / 2.0, g, self.theme.text_secondary);
        rect
    }

    /// Tooltip after a hover delay, with an optional shortcut keycap. Drawn on
    /// the tooltip layer so it floats above everything.
    pub fn tooltip(&self, ui: &Ui, resp: &Response, text: &str, shortcut: Option<&str>) {
        let ctx = ui.ctx();
        let key = resp.id.with("tooltip-since");
        let now = ctx.input(|i| i.time);
        if !resp.hovered() || resp.is_pointer_button_down_on() {
            ctx.data_mut(|d| d.remove::<f64>(key));
            return;
        }
        let since = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(key, || now));
        let wait = motion::TOOLTIP_DELAY as f64 - (now - since);
        if wait > 0.0 {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            return;
        }
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, resp.id.with("tooltip")));
        let g = self.galley(&painter, text, Type::Caption, self.theme.text_primary);
        let key_w = shortcut.map_or(0.0, |s| {
            self.galley(&painter, s, Type::Caption, self.theme.text_secondary)
                .size()
                .x
                + space::S2
                + 2.0
                + space::S2
        });
        let w = space::S3 * 2.0 + g.size().x + key_w;
        let h = size::KEYCAP_H + space::S2 * 2.0;
        let mut min = pos2(resp.rect.center().x - w / 2.0, resp.rect.max.y + space::S2);
        min.x = min.x.clamp(
            self.screen.min.x + space::S2,
            self.screen.max.x - w - space::S2,
        );
        if min.y + h > self.screen.max.y - space::S2 {
            min.y = resp.rect.min.y - space::S2 - h;
        }
        let rect = Rect::from_min_size(min, vec2(w, h));
        self.glass(&painter, rect, radius::SM, 2);
        painter.galley(
            pos2(rect.min.x + space::S3, rect.center().y - g.size().y / 2.0),
            g.clone(),
            self.theme.text_primary,
        );
        if let Some(s) = shortcut {
            self.keycap(
                &painter,
                pos2(
                    rect.min.x + space::S3 + g.size().x + space::S2,
                    rect.center().y - size::KEYCAP_H / 2.0,
                ),
                s,
            );
        }
    }
}

/// A square rect for an icon of `sz` centered at `c`.
pub fn icon_rect(c: Pos2, sz: f32) -> Rect {
    Rect::from_center_size(c, vec2(sz, sz))
}

/// Tilt state for the sweep strip and picker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DotState {
    In,
    Arriving,
    Pending,
}

impl Kit<'_> {
    /// One sweep-strip dot: filled (in), accent ring with core (arriving),
    /// hollow (pending). `current` marks the shown tilt.
    pub fn sweep_dot(&self, painter: &Painter, c: Pos2, state: DotState, current: bool) {
        let t = self.theme;
        let r = size::SWEEP_DOT_R;
        let ring = r - size::SWEEP_RING_STROKE / 2.0;
        match state {
            DotState::In => {
                painter.circle_filled(c, r, if current { t.accent } else { t.text_secondary });
            }
            DotState::Arriving => {
                painter.circle_stroke(c, ring, Stroke::new(size::SWEEP_RING_STROKE, t.accent));
                painter.circle_filled(c, r / 2.0, t.accent);
            }
            DotState::Pending => {
                painter.circle_stroke(
                    c,
                    ring,
                    Stroke::new(size::SWEEP_RING_STROKE, t.text_tertiary),
                );
            }
        }
    }

    /// A target dot inside a grid cell.
    pub fn cell_dot(&self, painter: &Painter, c: Pos2, color: Color32) {
        painter.circle_filled(c, size::CELL_DOT_R, color);
    }

    /// Hollow marker for data that isn't there (picker cells).
    pub fn empty_dot(&self, painter: &Painter, c: Pos2) {
        let ring = size::SWEEP_DOT_R - size::SWEEP_RING_STROKE / 2.0;
        painter.circle_stroke(
            c,
            ring,
            Stroke::new(
                size::SWEEP_RING_STROKE,
                self.theme.text_tertiary.gamma_multiply(0.5),
            ),
        );
    }

    /// Ring marker on the map (radar sites), haloed so it reads on radar.
    pub fn ring_marker(&self, painter: &Painter, p: Pos2, color: Color32) {
        painter.circle_stroke(
            p,
            size::MARKER_R,
            Stroke::new(size::HALO_STROKE, self.theme.map_label_halo),
        );
        painter.circle_stroke(p, size::MARKER_R, Stroke::new(size::MARKER_STROKE, color));
    }

    /// Inspector target reticle.
    pub fn reticle(&self, painter: &Painter, p: Pos2) {
        let t = self.theme;
        painter.circle_stroke(
            p,
            size::RETICLE_R,
            Stroke::new(size::HALO_STROKE, t.map_label_halo),
        );
        painter.circle_stroke(
            p,
            size::RETICLE_R,
            Stroke::new(size::MARKER_STROKE, t.text_primary),
        );
        painter.circle_filled(p, size::RETICLE_DOT_R, t.text_primary);
    }

    /// Horizontal color bar from radar palette data (the palette is data,
    /// not a UI color, so its RGB values arrive here as bytes).
    pub fn color_bar(&self, painter: &Painter, bar: Rect, sample: impl Fn(f32) -> [u8; 4]) {
        let mut mesh = egui::Mesh::default();
        let n = size::LEGEND_STEPS;
        for i in 0..n {
            let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            let c = sample((a + b) / 2.0);
            let r = Rect::from_min_max(
                pos2(bar.lerp_inside(vec2(a, 0.0)).x, bar.min.y),
                pos2(bar.lerp_inside(vec2(b, 0.0)).x, bar.max.y),
            );
            mesh.add_colored_rect(r, Color32::from_rgb(c[0], c[1], c[2]));
        }
        painter.add(mesh);
        painter.rect_stroke(
            bar,
            CornerRadius::same(size::LEGEND_BAR_RADIUS),
            Stroke::new(hairline(painter.ctx()), self.theme.glass_stroke),
            StrokeKind::Outside,
        );
    }

    /// Accent bar on the leading edge of a selected row: selection shown
    /// without relying on color contrast of the text (contrast test).
    pub fn selection_bar(&self, painter: &Painter, row: Rect) {
        let bar = Rect::from_min_size(
            pos2(row.min.x, row.min.y + space::S2),
            vec2(size::SELECTION_BAR_W, row.height() - 2.0 * space::S2),
        );
        painter.rect_filled(bar, CornerRadius::same(radius::XS), self.theme.accent);
    }

    /// A hairline horizontal divider.
    pub fn divider(&self, painter: &Painter, x0: f32, x1: f32, y: f32) {
        painter.line_segment(
            [pos2(x0, y), pos2(x1, y)],
            Stroke::new(hairline(painter.ctx()), self.theme.glass_stroke),
        );
    }

    /// Selection fill for a list row (inset by a hairline so rows separate).
    pub fn row_fill(&self, painter: &Painter, rect: Rect, resp: &Response, selected: bool) {
        self.state_fill(
            painter,
            rect.shrink2(vec2(0.0, hairline(painter.ctx()))),
            radius::SM,
            resp,
            selected,
        );
    }
}

pub fn hairline(ctx: &egui::Context) -> f32 {
    size::HAIRLINE_PX / ctx.pixels_per_point()
}

/// An interactive area with an AccessKit label (chips, rows, cells).
pub fn interact(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    kind: WidgetType,
    label: &str,
    selected: Option<bool>,
) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let label = label.to_string();
    resp.widget_info(|| match selected {
        Some(s) => WidgetInfo::selected(kind, true, s, &label),
        None => WidgetInfo::labeled(kind, true, &label),
    });
    resp
}

/// Icon button (36–40 px hit target, 20 px icon) with tooltip and shortcut.
#[allow(clippy::too_many_arguments)]
pub fn icon_button(
    ui: &mut Ui,
    kit: &mut Kit,
    painter: &Painter,
    rect: Rect,
    id: Id,
    icon: Icon,
    label: &str,
    shortcut: Option<&str>,
    selected: bool,
) -> Response {
    let resp = interact(ui, rect, id, WidgetType::Button, label, Some(selected));
    kit.state_fill(painter, rect, radius::SM, &resp, selected);
    let color = if selected {
        kit.theme.accent
    } else if resp.hovered() {
        kit.theme.text_primary
    } else {
        kit.theme.text_secondary
    };
    kit.icon(painter, rect, icon, size::ICON_MD, color);
    kit.focus_ring(painter, rect, radius::SM, &resp);
    kit.tooltip(ui, &resp, label, shortcut);
    resp
}

/// Lays out styled text runs left to right, vertically centered on `cy`.
/// Returns the x after the last run.
pub fn row(
    kit: &Kit,
    painter: &Painter,
    mut x: f32,
    cy: f32,
    parts: &[(String, Type, Color32, f32)],
) -> f32 {
    for (text, ty, color, gap) in parts {
        x += gap;
        let g = kit.galley(painter, text.clone(), *ty, *color);
        let w = g.size().x;
        painter.galley(pos2(x, cy - g.size().y / 2.0), g, *color);
        x += w;
    }
    x
}

pub fn measure(kit: &Kit, painter: &Painter, parts: &[(String, Type, Color32, f32)]) -> f32 {
    parts
        .iter()
        .map(|(t, ty, c, gap)| gap + kit.galley(painter, t.clone(), *ty, *c).size().x)
        .sum()
}

/// Single-line text field. State (the text) is owned by the caller; returns
/// true if the text changed. The caret doesn't blink, so an open picker
/// costs no idle CPU.
pub fn text_field(
    ui: &mut Ui,
    kit: &mut Kit,
    painter: &Painter,
    rect: Rect,
    id: Id,
    text: &mut String,
    placeholder: &str,
) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    if resp.clicked() {
        resp.request_focus();
    }
    let value = text.clone();
    resp.widget_info(|| WidgetInfo::text_edit(true, value.clone(), value.clone(), placeholder));
    let mut changed = false;
    if resp.has_focus() {
        // Typing is captured even with modifiers released; Escape/Enter/arrows
        // are left for the owner (picker navigation).
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: false,
                    horizontal_arrows: true,
                    vertical_arrows: false,
                    escape: false,
                },
            )
        });
        ui.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Text(t) if !t.chars().any(char::is_control) => {
                        text.push_str(t);
                        changed = true;
                    }
                    egui::Event::Key {
                        key: egui::Key::Backspace,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if modifiers.command || modifiers.alt {
                            let trimmed = text.trim_end();
                            let cut = trimmed.rfind(' ').map_or(0, |p| p + 1);
                            text.truncate(cut);
                        } else {
                            text.pop();
                        }
                        changed = true;
                    }
                    _ => {}
                }
            }
        });
    }
    let t = kit.theme;
    painter.rect(
        rect,
        CornerRadius::same(radius::SM),
        t.hover,
        Stroke::new(hairline(painter.ctx()), t.glass_stroke),
        StrokeKind::Inside,
    );
    let icon_rect = Rect::from_center_size(
        pos2(
            rect.min.x + space::S3 + size::ICON_SM / 2.0,
            rect.center().y,
        ),
        vec2(size::ICON_SM, size::ICON_SM),
    );
    kit.icon(
        painter,
        icon_rect,
        Icon::Search,
        size::ICON_SM,
        t.text_tertiary,
    );
    let x = icon_rect.max.x + space::S2;
    if text.is_empty() {
        kit.text(
            painter,
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            placeholder,
            Type::Body,
            t.text_tertiary,
        );
        if resp.has_focus() {
            caret(painter, pos2(x, rect.center().y), t.accent);
        }
    } else {
        let r = kit.text(
            painter,
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            text.clone(),
            Type::Body,
            t.text_primary,
        );
        if resp.has_focus() {
            caret(painter, pos2(r.max.x + 1.0, rect.center().y), t.accent);
        }
    }
    kit.focus_ring(painter, rect, radius::SM, &resp);
    changed
}

fn caret(painter: &Painter, at: Pos2, color: Color32) {
    painter.line_segment(
        [at - vec2(0.0, 8.0), at + vec2(0.0, 8.0)],
        Stroke::new(1.5, color),
    );
}

/// A popover above `anchor`, right-aligned with it (for bottom chrome).
pub fn popover_above(screen: Rect, anchor: Rect, w: f32, h: f32) -> Rect {
    let x = (anchor.max.x - w).max(screen.min.x + space::S3);
    let bottom = anchor.min.y - space::S2;
    let h = h.min(bottom - screen.min.y - space::S3);
    Rect::from_min_max(pos2(x, bottom - h), pos2(x + w, bottom))
}

/// Where a popover anchored under `anchor` goes, clamped to the screen.
pub fn popover_rect(screen: Rect, anchor: Rect, w: f32, h: f32) -> Rect {
    let x = anchor.min.x.clamp(
        screen.min.x + space::S3,
        (screen.max.x - w - space::S3).max(screen.min.x + space::S3),
    );
    let y = anchor.max.y + space::S2;
    let h = h.min(screen.max.y - y - space::S3);
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// Thin overlay scroll indicator (no stock scroll bar).
pub fn scroll_indicator(kit: &Kit, painter: &Painter, area: Rect, offset: f32, content_h: f32) {
    if content_h <= area.height() {
        return;
    }
    let frac = area.height() / content_h;
    let h = (area.height() * frac).max(24.0);
    let y =
        area.min.y + (area.height() - h) * (offset / (content_h - area.height())).clamp(0.0, 1.0);
    let r = Rect::from_min_size(
        pos2(area.max.x - size::SCROLL_INDICATOR_W - 2.0, y),
        vec2(size::SCROLL_INDICATOR_W, h),
    );
    painter.rect_filled(r, CornerRadius::same(2), kit.theme.track);
}
