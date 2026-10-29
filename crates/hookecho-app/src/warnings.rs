//! NWS warnings on the map (hookecho-core::alerts): colored outlines under
//! the chrome, and a card with the full text when a warning is clicked.

use crate::design::components::{Kit, icon_button};
use crate::design::hazards;
use crate::design::icons::Icon;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{self, Model, Picker};
use eframe::egui::{
    self, Color32, Id, LayerId, Order, Painter, Pos2, Rect, Shape, Stroke, Ui, pos2, vec2,
};
use hookecho_core::alerts::Alert;

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Draws every shown warning's outline: least severe first, so tornado
/// warnings end up on top. Polygons off screen are skipped.
pub fn draw(kit: &Kit, model: &Model, painter: &Painter, view: [f64; 2]) {
    let screen = kit.screen;
    for a in model.visible_alerts(now_ms()).rev() {
        let color = hazards::color(a.event);
        let w = hazards::stroke(a.event);
        for ring in &a.rings {
            let pts: Vec<Pos2> = ring
                .iter()
                .map(|&(lat, lon)| {
                    let p = model.camera.latlon_to_screen(lat, lon, view);
                    pos2(p[0] as f32, p[1] as f32)
                })
                .collect();
            if !Rect::from_points(&pts).intersects(screen) {
                continue;
            }
            painter.add(Shape::closed_line(
                pts.clone(),
                Stroke::new(w + hazards::HALO, hazards::HALO_COLOR),
            ));
            painter.add(Shape::closed_line(pts, Stroke::new(w, color)));
        }
    }
}

/// A click on the map at (lat, lon): opens the card for the warnings
/// there, or closes it if there are none. Returns whether any were hit.
pub fn click(model: &mut Model, at: (f64, f64)) -> bool {
    let ids: Vec<String> = model
        .visible_alerts(now_ms())
        .filter(|a| a.contains(at.0, at.1))
        .map(|a| a.id.clone())
        .collect();
    if ids.is_empty() {
        if matches!(model.picker, Picker::Warnings { .. }) {
            model.picker = Picker::None;
        }
        return false;
    }
    model.picker = Picker::Warnings { ids, scroll: 0.0 };
    true
}

/// Wrapped text in `w` points.
fn wrapped(
    painter: &Painter,
    text: &str,
    ty: Type,
    color: Color32,
    w: f32,
) -> std::sync::Arc<egui::Galley> {
    painter.layout(text.to_string(), ty.font(), color, w)
}

/// The warning card: below the site chip on the left. Each warning gets a
/// colored bar, its type, when it ends and who issued it, the area, then
/// the NWS text. Scrolls; Esc, the close button, or a click elsewhere on
/// the map closes it.
pub fn card(ui: &mut Ui, kit: &mut Kit, model: &mut Model, top_left: Pos2) {
    let Picker::Warnings { ids, mut scroll } = model.picker.clone() else {
        return;
    };
    let alerts: Vec<Alert> = ids
        .iter()
        .filter_map(|id| model.alerts.iter().find(|a| &a.id == id).cloned())
        .collect();
    if alerts.is_empty() {
        // Expired or withdrawn since it was opened.
        model.picker = Picker::None;
        return;
    }
    let t = kit.theme.clone();
    let screen = kit.screen;
    let pad = space::S3;
    let w = hazards::CARD_W.min(screen.width() - 2.0 * (top_left.x - screen.min.x));
    let max_h = hazards::CARD_MAX_H.min(screen.max.y - top_left.y - space::S6);
    let layer = LayerId::new(Order::Foreground, Id::new("warning-card"));
    let painter = ui.ctx().layer_painter(layer);
    let text_x = top_left.x + pad + hazards::CARD_BAR_W + space::S2;
    let text_w = top_left.x + w - pad - text_x;

    // Lay out first, to size the card to its content.
    let utc = model.config.utc;
    let blocks: Vec<_> = alerts
        .iter()
        .map(|a| {
            let until = format!(
                "Until {} · {}",
                model::clock_time(a.expires_ms, utc),
                a.sender
            );
            let mut parts = vec![
                (
                    wrapped(
                        &painter,
                        a.event.name(),
                        Type::Label,
                        t.text_primary,
                        text_w,
                    ),
                    space::S1,
                ),
                (
                    wrapped(&painter, &until, Type::Caption, t.text_secondary, text_w),
                    space::S1,
                ),
                (
                    wrapped(&painter, &a.area, Type::Caption, t.text_tertiary, text_w),
                    space::S2,
                ),
                (
                    wrapped(
                        &painter,
                        &tidy(&a.description),
                        Type::Body,
                        t.text_primary,
                        text_w,
                    ),
                    space::S2,
                ),
            ];
            if !a.instruction.is_empty() {
                parts.push((
                    wrapped(
                        &painter,
                        &tidy(&a.instruction),
                        Type::Body,
                        t.text_secondary,
                        text_w,
                    ),
                    space::S2,
                ));
            }
            (a.event, parts)
        })
        .collect();
    let content_h: f32 = blocks
        .iter()
        .map(|(_, parts)| parts.iter().map(|(g, gap)| g.size().y + gap).sum::<f32>())
        .sum::<f32>()
        + space::S3 * (blocks.len() as f32 - 1.0);
    let h = (pad + content_h + pad).min(max_h);
    let rect = Rect::from_min_size(top_left, vec2(w, h));
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    ui.interact(
        rect,
        Id::new("warning-card-bg"),
        egui::Sense::click_and_drag(),
    );
    kit.glass(&painter, rect, radius::LG, 2);

    let area = rect.shrink(pad);
    if ui.rect_contains_pointer(rect) {
        scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    scroll = scroll.clamp(0.0, (content_h - area.height()).max(0.0));
    let clipped = painter.with_clip_rect(area);
    let mut y = area.min.y - scroll;
    for (i, (event, parts)) in blocks.iter().enumerate() {
        if i > 0 {
            kit.divider(&clipped, area.min.x, area.max.x, y + space::S3 / 2.0);
            y += space::S3;
        }
        let title_h = parts[0].0.size().y;
        clipped.rect_filled(
            Rect::from_min_size(pos2(area.min.x, y), vec2(hazards::CARD_BAR_W, title_h)),
            radius::XS,
            hazards::color(*event),
        );
        for (g, gap) in parts {
            clipped.galley(pos2(text_x, y), g.clone(), t.text_primary);
            y += g.size().y + gap;
        }
    }
    crate::design::components::scroll_indicator(kit, &painter, area, scroll, content_h);

    let close = Rect::from_min_size(
        pos2(
            rect.max.x - space::S1 - size::ICON_BUTTON,
            rect.min.y + space::S1,
        ),
        vec2(size::ICON_BUTTON, size::ICON_BUTTON),
    );
    let closed = icon_button(
        &mut ui,
        kit,
        &painter,
        close,
        Id::new("warning-card-close"),
        Icon::X,
        "Close",
        Some("Esc"),
        false,
    )
    .clicked();
    if closed {
        model.picker = Picker::None;
    } else if let Picker::Warnings { scroll: s, .. } = &mut model.picker {
        *s = scroll;
    }
}

/// The recorded NWS answer in tests/data/alerts (seven flash flood
/// warnings in west Texas and New Mexico), kept in force for an hour from
/// now, for screenshots and tests. `None` outside a source checkout.
pub fn sample_alerts() -> Option<Vec<Alert>> {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/data/alerts/active-2026-10-29T0136Z.json"
    );
    let mut a = hookecho_core::alerts::parse(&std::fs::read(p).ok()?).ok()?;
    for x in &mut a {
        x.expires_ms = now_ms() + 3_600_000;
    }
    Some(a)
}

/// The NWS text is hard-wrapped at ~69 columns with blank lines between
/// paragraphs: rejoin each paragraph so it wraps to the card instead. A
/// leading product code line ("FFWMAF") is dropped.
pub fn tidy(text: &str) -> String {
    let code = |p: &str| {
        let p = p.trim();
        (4..=10).contains(&p.len())
            && p.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    };
    let text = match text.split_once("\n\n") {
        Some((first, rest)) if code(first) => rest,
        _ => text,
    };
    text.split("\n\n")
        .map(|p| {
            p.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_rejoins_paragraphs() {
        let t = "FFWMAF\n\nThe National Weather Service in Midland/Odessa has issued a\n\n* Flash Flood Warning for...\nNorthern Culberson County in southwestern Texas...\n\n\n";
        assert_eq!(
            tidy(t),
            "The National Weather Service in Midland/Odessa has issued a\n\n* Flash Flood Warning for... Northern Culberson County in southwestern Texas..."
        );
        assert_eq!(tidy("SOURCE...Radar.\n\nMore"), "SOURCE...Radar.\n\nMore");
    }
}
