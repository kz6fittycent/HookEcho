//! NWS warnings on the map (hookecho-core::alerts): colored outlines under
//! the chrome, and a card with the full text when a warning is clicked. The
//! same card shows SPC watches, MDs and the outlook under the click
//! (spc.rs).

use crate::design::components::{self, Kit};
use crate::design::hazards;
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

/// A click on the map at (lat, lon): opens the card for the warnings,
/// watches, MDs and outlook there, or closes it if there are none. Returns
/// whether any were hit.
pub fn click(model: &mut Model, at: (f64, f64)) -> bool {
    let mut ids: Vec<String> = model
        .visible_alerts(now_ms())
        .filter(|a| a.contains(at.0, at.1))
        .map(|a| a.id.clone())
        .collect();
    ids.extend(crate::spc::hits(model, at));
    // (The dock is on screen when it has a width: not in a narrow window.)
    if crate::dock::wants_dock(model, crate::dock::Panel::Bulletins) && model.dock_inset > 0.0 {
        // Docked: the panel shows what was clicked (or, on open map, the
        // list of everything again).
        let hit = !ids.is_empty();
        model.dock_bulletins = ids;
        model
            .config
            .dock_collapsed
            .retain(|k| k != crate::dock::Panel::Bulletins.key());
        return hit;
    }
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

type Block = (Color32, Vec<(std::sync::Arc<egui::Galley>, f32)>);

/// Lays out the blocks for `entries` in `w` points of text (the bar takes
/// its own room to the left of that).
fn layout(kit: &Kit, painter: &Painter, entries: &[Entry], w: f32) -> Vec<Block> {
    let t = &kit.theme;
    entries
        .iter()
        .map(|e| {
            let mut parts = vec![
                (
                    wrapped(painter, &e.title, Type::Label, t.text_primary, w),
                    space::S1,
                ),
                (
                    wrapped(painter, &e.when, Type::Caption, t.text_secondary, w),
                    space::S1,
                ),
            ];
            if !e.area.is_empty() {
                parts.push((
                    wrapped(painter, &e.area, Type::Caption, t.text_tertiary, w),
                    space::S2,
                ));
            } else {
                parts.last_mut().expect("title").1 = space::S2;
            }
            for (i, body) in e.body.iter().filter(|b| !b.is_empty()).enumerate() {
                let color = if i == 0 {
                    t.text_primary
                } else {
                    t.text_secondary
                };
                parts.push((
                    wrapped(painter, &tidy(body), Type::Body, color, w),
                    space::S2,
                ));
            }
            (e.color, parts)
        })
        .collect()
}

fn blocks_height(blocks: &[Block]) -> f32 {
    blocks
        .iter()
        .map(|(_, parts)| parts.iter().map(|(g, gap)| g.size().y + gap).sum::<f32>())
        .sum::<f32>()
        + space::S3 * (blocks.len() as f32 - 1.0).max(0.0)
}

/// Paints laid-out blocks from `y`, each with its colored bar; returns the
/// height used.
fn paint_blocks(kit: &Kit, painter: &Painter, blocks: &[Block], x0: f32, x1: f32, y0: f32) -> f32 {
    let text_x = x0 + hazards::CARD_BAR_W + space::S2;
    let mut y = y0;
    for (i, (color, parts)) in blocks.iter().enumerate() {
        if i > 0 {
            kit.divider(painter, x0, x1, y + space::S3 / 2.0);
            y += space::S3;
        }
        let title_h = parts[0].0.size().y;
        painter.rect_filled(
            Rect::from_min_size(pos2(x0, y), vec2(hazards::CARD_BAR_W, title_h)),
            radius::XS,
            *color,
        );
        for (g, gap) in parts {
            painter.galley(pos2(text_x, y), g.clone(), kit.theme.text_primary);
            y += g.size().y + gap;
        }
    }
    y - y0
}

fn text_width(x0: f32, x1: f32) -> f32 {
    x1 - x0 - hazards::CARD_BAR_W - space::S2
}

/// The floating warning card: below the site chip on the left, with the
/// shared panel header. Each warning, watch or MD gets a colored bar, its
/// type, when it ends and who issued it, the area, then the full text.
/// Scrolls; Esc, close, or a click elsewhere on the map closes it.
pub fn card(ui: &mut Ui, kit: &mut Kit, model: &mut Model, top_left: Pos2, area: Rect) {
    use crate::dock::Panel;
    let Picker::Warnings { ids, mut scroll } = model.picker.clone() else {
        return;
    };
    let entries = entries(model, &ids);
    if entries.is_empty() {
        // Expired or withdrawn since it was opened.
        model.picker = Picker::None;
        return;
    }
    let pad = space::S3;
    let w = hazards::CARD_W.min(area.width() - 2.0 * (top_left.x - area.min.x));
    let max_h = hazards::CARD_MAX_H.min(area.max.y - top_left.y - space::S6);
    let layer = LayerId::new(Order::Foreground, Id::new("warning-card"));
    let painter = ui.ctx().layer_painter(layer);
    let (x0, x1) = (top_left.x + pad, top_left.x + w - pad);
    let blocks = layout(kit, &painter, &entries, text_width(x0, x1));
    let content_h = blocks_height(&blocks);
    let h = (pad + size::PANEL_HEADER_H + content_h + pad).min(max_h);
    let rect = Rect::from_min_size(top_left, vec2(w, h));
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    ui.interact(
        rect,
        Id::new("warning-card-bg"),
        egui::Sense::click_and_drag(),
    );
    kit.glass(&painter, rect, radius::LG, 2);
    let inner = rect.shrink(pad);
    let header = Rect::from_min_size(inner.min, vec2(inner.width(), size::PANEL_HEADER_H));
    let action = components::panel_header(
        &mut ui,
        kit,
        &painter,
        header,
        Id::new("warning-card-header"),
        Panel::Bulletins.title(),
        false,
        false,
        true,
    );
    let body = Rect::from_min_max(pos2(inner.min.x, header.max.y), inner.max);
    if components::pointer_over(&ui, rect) {
        scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    scroll = scroll.clamp(0.0, (content_h - body.height()).max(0.0));
    paint_blocks(
        kit,
        &painter.with_clip_rect(body),
        &blocks,
        body.min.x,
        body.max.x,
        body.min.y - scroll,
    );
    components::scroll_indicator(kit, &painter, body, scroll, content_h);
    match action {
        Some(components::PanelAction::Close) => model.picker = Picker::None,
        Some(a) => crate::dock::act(model, Panel::Bulletins, a),
        None => {
            if let Picker::Warnings { scroll: s, .. } = &mut model.picker {
                *s = scroll;
            }
        }
    }
}

/// Card ids of everything active and shown: warnings (most severe first),
/// then watches and MDs.
fn all_active(model: &Model) -> Vec<String> {
    let now = now_ms();
    let mut ids: Vec<String> = model.visible_alerts(now).map(|a| a.id.clone()).collect();
    ids.extend(crate::spc::visible_watches(model, now).map(|w| format!("ww:{}", w.number)));
    ids.extend(crate::spc::visible_discussions(model, now).map(|d| format!("md:{}", d.number)));
    ids
}

/// The docked panel's body: what was clicked on the map, in full; or, with
/// nothing chosen, everything active as a list to pick from.
pub fn dock_body(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &Painter,
    x0: f32,
    x1: f32,
    y0: f32,
) -> f32 {
    let t = kit.theme.clone();
    let live = painter.clip_rect().is_positive();
    let chosen = entries(model, &model.dock_bulletins.clone());
    let mut y = y0;
    if !chosen.is_empty() {
        let r = Rect::from_min_max(pos2(x0, y), pos2(x1, y + size::HINT_H));
        let label = "← Everything active";
        if live {
            let resp = components::interact(
                ui,
                r,
                Id::new("bulletins-all"),
                egui::WidgetType::Button,
                label,
                None,
            );
            if resp.clicked() {
                model.dock_bulletins.clear();
            }
            kit.text(
                painter,
                pos2(x0, r.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                Type::Caption,
                if resp.hovered() {
                    t.text_primary
                } else {
                    t.accent
                },
            );
        }
        y = r.max.y + space::S2;
        let blocks = layout(kit, painter, &chosen, text_width(x0, x1));
        y += paint_blocks(kit, painter, &blocks, x0, x1, y);
        return y + space::S2 - y0;
    }
    let ids = all_active(model);
    if ids.is_empty() {
        let msg = if model.config.warnings || model.config.watches || model.config.discussions {
            "No warnings, watches or discussions in effect"
        } else {
            "Warnings, watches and MDs are switched off in Layers"
        };
        let g = wrapped(painter, msg, Type::Body, t.text_secondary, x1 - x0);
        let gh = g.size().y;
        painter.galley(pos2(x0, y), g, t.text_secondary);
        return gh + space::S3;
    }
    // One row per item: its bar, type and end time; a click opens it.
    for (id, e) in ids.iter().zip(entries(model, &ids)) {
        let text_x = x0 + hazards::CARD_BAR_W + space::S2;
        let title = wrapped(painter, &e.title, Type::Body, t.text_primary, x1 - text_x);
        let when = wrapped(
            painter,
            &e.when,
            Type::Caption,
            t.text_tertiary,
            x1 - text_x,
        );
        let h = title.size().y + when.size().y + space::S2;
        let r = Rect::from_min_max(pos2(x0, y), pos2(x1, y + h));
        if live {
            let resp = components::interact(
                ui,
                r,
                Id::new(("bulletin", id.as_str())),
                egui::WidgetType::Button,
                &e.title,
                None,
            );
            kit.row_fill(painter, r, &resp, false);
            if resp.clicked() {
                model.dock_bulletins = vec![id.clone()];
            }
        }
        painter.rect_filled(
            Rect::from_min_size(
                pos2(x0, y + space::S1),
                vec2(hazards::CARD_BAR_W, h - space::S2),
            ),
            radius::XS,
            e.color,
        );
        let th = title.size().y;
        painter.galley(pos2(text_x, y + space::S1), title, t.text_primary);
        painter.galley(pos2(text_x, y + space::S1 + th), when, t.text_tertiary);
        y += h;
    }
    y + space::S2 - y0
}

/// One block of the card.
struct Entry {
    color: Color32,
    title: String,
    /// "Until 8:45 PM · NWS Norman OK".
    when: String,
    area: String,
    /// Paragraph groups: the first in primary text, the rest secondary.
    body: Vec<String>,
}

/// The card's blocks for `ids` (alert ids, "ww:N", "md:N", "otlk:lat,lon"),
/// skipping any no longer there.
fn entries(model: &Model, ids: &[String]) -> Vec<Entry> {
    let utc = model.config.utc;
    let until = |ms| format!("Until {}", model::clock_time(ms, utc));
    let spc = &model.layers.spc;
    ids.iter()
        .filter_map(|id| {
            if let Some(n) = id.strip_prefix("ww:") {
                let w = spc.watches.iter().find(|w| w.number.to_string() == n)?;
                let mut title = format!("{} {}", w.kind.name(), w.number);
                if w.pds {
                    title.push_str(" · PDS");
                }
                return Some(Entry {
                    color: hazards::watch_color(w.kind),
                    title,
                    when: format!("{} · Storm Prediction Center", until(w.expires_ms)),
                    area: String::new(),
                    body: vec![without_header(&w.text)],
                });
            }
            if let Some(n) = id.strip_prefix("md:") {
                let d = spc.discussions.iter().find(|d| d.number.to_string() == n)?;
                return Some(Entry {
                    color: hazards::MD_COLOR,
                    title: format!("Mesoscale Discussion {}", d.number),
                    when: format!("{} · Storm Prediction Center", until(d.expires_ms)),
                    area: [d.areas.as_str(), d.concerning.as_str()]
                        .iter()
                        .filter(|s| !s.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" · "),
                    body: vec![without_header(&d.text)],
                });
            }
            if let Some(at) = id.strip_prefix("otlk:") {
                let (lat, lon) = at.split_once(',')?;
                let o = crate::spc::shown_outlook(model)?;
                let a = o.at(lat.parse().ok()?, lon.parse().ok()?)?;
                let name = if a.name.is_empty() { &a.label } else { &a.name };
                return Some(Entry {
                    color: hazards::outlook_stroke(a.stroke),
                    title: format!("Day {} outlook · {name}", o.product.day()),
                    when: format!(
                        "{} to {} · SPC, {}",
                        model::clock_time(o.valid_ms, utc),
                        model::clock_time(o.expires_ms, utc),
                        o.forecaster
                    ),
                    area: String::new(),
                    body: Vec::new(),
                });
            }
            let a = model.layers.alerts.iter().find(|a| &a.id == id)?;
            Some(Entry {
                color: hazards::color(a.event),
                title: a.event.name().to_string(),
                when: format!("{} · {}", until(a.expires_ms), a.sender),
                area: a.area.clone(),
                body: vec![a.description.clone(), a.instruction.clone()],
            })
        })
        .collect()
}

/// SPC text without the paragraphs the card already shows above it: the
/// product header ("URGENT - IMMEDIATE BROADCAST REQUESTED", "Mesoscale
/// Discussion 1691 / NWS Storm Prediction Center ..."), and an MD's areas,
/// concern and valid time.
fn without_header(text: &str) -> String {
    const SKIP: [&str; 6] = [
        "URGENT",
        "BULLETIN",
        "Mesoscale Discussion",
        "Areas affected",
        "Concerning",
        "Valid",
    ];
    text.split("\n\n")
        .filter(|p| {
            let p = p.trim_start();
            !SKIP.iter().any(|s| p.starts_with(s))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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
    fn spc_headers_are_dropped() {
        let spc = crate::spc::sample().unwrap();
        let md = without_header(&spc.discussions[0].text);
        assert!(md.starts_with("Probability of Watch Issuance"), "{md}");
        let ww = tidy(&without_header(&spc.watches[0].text));
        assert!(
            ww.starts_with("The NWS Storm Prediction Center has issued"),
            "{ww}"
        );
    }

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
