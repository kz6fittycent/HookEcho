//! Space weather (hookecho-core::space): the OVATION aurora forecast as a
//! translucent overlay on the map, and the space weather card (Menu →
//! "Space weather", or Layers): the NOAA R/S/G levels, Kp now and the Kp
//! chart for the past three days and the three ahead, solar wind, X-rays
//! and the latest flare, the three-day outlook and SWPC's latest alerts.

use crate::design::components::Kit;
use crate::design::space as sp;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{self, Model, Picker};
use eframe::egui::{
    self, Align2, Color32, Id, LayerId, Mesh, Order, Painter, Rect, Shape, Ui, epaint, pos2, vec2,
};
use hookecho_core::geo::to_mercator;
use hookecho_core::space::{Aurora, KpKind, SpaceWeather, g_for_kp};

/// The aurora grid as quads in normalized mercator, rebuilt when a new
/// forecast arrives.
#[derive(Default)]
pub struct AuroraCache {
    forecast_ms: Option<i64>,
    vertices: Vec<([f32; 2], Color32)>,
    indices: Vec<u32>,
}

/// The grid points as vertices, colored by their chance, joined into quads
/// wherever a corner reaches [`sp::AURORA_MIN`]: colors blend smoothly
/// between points and fade out at the edges.
fn build(a: &Aurora) -> AuroraCache {
    let mut c = AuroraCache {
        forecast_ms: Some(a.forecast_ms),
        ..Default::default()
    };
    let mut index = std::collections::HashMap::new();
    let mut vertex = |c: &mut AuroraCache, lon: usize, lat: i64| -> u32 {
        *index.entry((lon, lat)).or_insert_with(|| {
            let p = a.grid[(lon % 360) * 181 + (lat + 90) as usize];
            // Longitudes as -180..180, with 360 at the seam's far side.
            let lo = if lon >= 180 {
                lon as f64 - 360.0
            } else {
                lon as f64
            };
            let m = to_mercator((lat as f64).clamp(-85.0, 85.0), lo);
            c.vertices
                .push(([m[0] as f32, m[1] as f32], sp::aurora_color(p)));
            (c.vertices.len() - 1) as u32
        })
    };
    for lon in 0..360 {
        for lat in -89..89i64 {
            let corners = [
                (lon, lat),
                (lon + 1, lat),
                (lon + 1, lat + 1),
                (lon, lat + 1),
            ];
            if corners
                .iter()
                .all(|&(lo, la)| a.grid[(lo % 360) * 181 + (la + 90) as usize] < sp::AURORA_MIN)
            {
                continue;
            }
            // 179 → 180 crosses from +179° to -180°: draw that column as
            // 179 → 180 on the positive side instead.
            let v: Vec<u32> = corners
                .iter()
                .map(|&(lo, la)| {
                    if lon == 179 && lo == 180 {
                        let p = a.grid[180 * 181 + (la + 90) as usize];
                        let m = to_mercator((la as f64).clamp(-85.0, 85.0), 180.0);
                        c.vertices
                            .push(([m[0] as f32, m[1] as f32], sp::aurora_color(p)));
                        (c.vertices.len() - 1) as u32
                    } else {
                        vertex(&mut c, lo, la)
                    }
                })
                .collect();
            c.indices.extend([v[0], v[1], v[2], v[0], v[2], v[3]]);
        }
    }
    c
}

pub fn draw_aurora(model: &Model, painter: &Painter, view: [f64; 2]) {
    if !model.config.aurora {
        return;
    }
    let Some(a) = &model.layers.space.aurora else {
        return;
    };
    let mut cache = model.layers.aurora_mesh.borrow_mut();
    if cache.forecast_ms != Some(a.forecast_ms) {
        *cache = build(a);
    }
    let cam = model.camera;
    let mesh = Mesh {
        vertices: cache
            .vertices
            .iter()
            .map(|(m, color)| {
                let p = cam.to_screen([m[0] as f64, m[1] as f64], view);
                epaint::Vertex {
                    pos: pos2(p[0] as f32, p[1] as f32),
                    uv: epaint::WHITE_UV,
                    color: *color,
                }
            })
            .collect(),
        indices: cache.indices.clone(),
        ..Default::default()
    };
    painter.add(Shape::mesh(mesh));
}

// --------------------------------------------------------------- the card

fn scale_name(letter: char) -> &'static str {
    match letter {
        'R' => "Radio blackouts",
        'S' => "Radiation storms",
        _ => "Geomagnetic storms",
    }
}

fn date_label(date: &str) -> String {
    // "2026-10-31" → "Oct 31".
    const M: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut it = date.split('-').skip(1);
    match (
        it.next().and_then(|m| m.parse::<usize>().ok()),
        it.next().and_then(|d| d.parse::<u32>().ok()),
    ) {
        (Some(m @ 1..=12), Some(d)) => format!("{} {d}", M[m - 1]),
        _ => date.to_string(),
    }
}

fn pct(p: Option<u8>) -> String {
    p.map_or("—".into(), |p| format!("{p}%"))
}

/// The card's text lines below the chart: (text, primary).
fn lines(w: &SpaceWeather, utc: bool) -> Vec<(String, bool)> {
    let mut v = Vec::new();
    if let Some(s) = &w.wind {
        let mut t = "Solar wind".to_string();
        if let Some(x) = s.speed_km_s {
            t.push_str(&format!(" {x:.0} km/s"));
        }
        if let Some(x) = s.density_cm3 {
            t.push_str(&format!(" · {x:.1} p/cm³"));
        }
        if let Some(x) = s.bz_nt {
            t.push_str(&format!(" · Bz {x:+.1} nT"));
        }
        if let Some(x) = s.bt_nt {
            t.push_str(&format!(" · Bt {x:.1} nT"));
        }
        v.push((t, true));
    }
    if let Some(x) = &w.xray {
        let mut t = format!("X-rays {}", x.class);
        if let Some(f) = &x.latest_flare {
            t.push_str(&format!(" · last flare {}", f.max_class));
            if let Some(ms) = f.max_ms {
                t.push_str(&format!(" at {}", model::clock_time(ms, utc)));
            }
        }
        v.push((t, true));
    }
    for d in &w.days {
        v.push((
            format!(
                "{}: G{} · radio blackout {} minor, {} major · radiation storm {}",
                date_label(&d.date),
                d.g,
                pct(d.r_minor),
                pct(d.r_major),
                pct(d.s),
            ),
            false,
        ));
    }
    v
}

/// The space weather body: when it's from, the R/S/G levels, Kp now and
/// its chart, solar wind, X-rays, the three-day outlook and the latest
/// alerts. Drawn from `y` between `x0` and `x1` (the painter clipped to
/// what's visible); returns its height.
pub fn body(kit: &mut Kit, model: &Model, p: &Painter, x0: f32, x1: f32, y0: f32) -> f32 {
    let t = kit.theme.clone();
    let w = model.layers.space.clone();
    let utc = model.config.utc;
    let now = crate::warnings::now_ms();
    let text_w = x1 - x0;
    let x = x0;
    let mut y = y0;
    if w.kp.is_empty() && w.kp_now.is_none() {
        let msg = if model.layers.space_trouble.is_some() {
            "Can't reach NOAA SWPC right now; retrying"
        } else {
            "Loading from NOAA SWPC…"
        };
        kit.text(
            p,
            pos2(x, y + sp::LINE_H / 2.0),
            Align2::LEFT_CENTER,
            msg,
            Type::Body,
            t.text_secondary,
        );
        return sp::LINE_H + space::S2;
    }
    if let Some((ms, _)) = w.kp_now {
        kit.text(
            p,
            pos2(x, y + sp::LINE_H / 2.0),
            Align2::LEFT_CENTER,
            format!("NOAA SWPC · {}", model::clock_time(ms, utc)),
            Type::Caption,
            t.text_tertiary,
        );
        y += sp::LINE_H + space::S1;
    }

    // R, S, G chips, sharing the width.
    let chip_w = ((text_w - 2.0 * space::S2) / 3.0).min(sp::SCALE_CHIP[0]);
    let mut cx = x;
    for (letter, level) in [('R', w.scales.r), ('S', w.scales.s), ('G', w.scales.g)] {
        let r = Rect::from_min_size(pos2(cx, y), vec2(chip_w, sp::SCALE_CHIP[1]));
        let color = sp::scale_color(level);
        p.rect_filled(r, radius::SM, t.track);
        if let Some(c) = color {
            p.rect_filled(
                Rect::from_min_size(r.min, vec2(size::SELECTION_BAR_W, r.height())),
                radius::XS,
                c,
            );
        }
        kit.text(
            p,
            r.center() - egui::Vec2::Y * space::S2,
            Align2::CENTER_CENTER,
            format!("{letter}{level}"),
            Type::LabelTab,
            color.unwrap_or(t.text_primary),
        );
        kit.text(
            p,
            r.center() + egui::Vec2::Y * (space::S2 + space::S1),
            Align2::CENTER_CENTER,
            scale_name(letter).split(' ').next().unwrap_or_default(),
            Type::Caption,
            t.text_tertiary,
        );
        cx = r.max.x + space::S2;
    }
    y += sp::SCALE_CHIP[1] + space::S3;

    // Kp now, and the chart.
    let mut kp_line = match w.kp_now {
        Some((_, k)) => format!("Kp {k:.1} now (G{})", g_for_kp(k)),
        None => "Kp —".to_string(),
    };
    if let Some(m) = w.kp_max_ahead(now) {
        kp_line.push_str(&format!(" · up to {m:.1} ahead (G{})", g_for_kp(m)));
    }
    let g = p.layout(kp_line, Type::BodyTab.font(), t.text_primary, text_w);
    let gh = g.size().y;
    p.galley(pos2(x, y), g, t.text_primary);
    y += gh + space::S1;
    let from = now - 3 * 86_400_000;
    let bars: Vec<_> = w.kp.iter().filter(|k| k.time_ms >= from).collect();
    if !bars.is_empty() {
        let chart = Rect::from_min_size(pos2(x, y), vec2(text_w, sp::KP_CHART_H));
        p.rect_filled(chart, radius::XS, t.track);
        let bw = chart.width() / bars.len() as f32;
        for (i, k) in bars.iter().enumerate() {
            let hgt = chart.height() * (k.kp / 9.0).clamp(0.02, 1.0);
            let bar = Rect::from_min_max(
                pos2(
                    chart.min.x + i as f32 * bw + sp::KP_BAR_GAP,
                    chart.max.y - hgt,
                ),
                pos2(
                    chart.min.x + (i + 1) as f32 * bw - sp::KP_BAR_GAP,
                    chart.max.y,
                ),
            );
            let mut c = sp::scale_color(g_for_kp(k.kp)).unwrap_or(t.accent);
            if k.kind == KpKind::Predicted {
                c = c.gamma_multiply(sp::KP_PREDICTED_FADE);
            }
            p.rect_filled(bar, 0.0, c);
            if k.time_ms <= now && now < k.time_ms + 3 * 3_600_000 {
                let nx = bar.center().x;
                p.line_segment(
                    [pos2(nx, chart.min.y), pos2(nx, chart.max.y)],
                    egui::Stroke::new(size::HAIRLINE_PX, t.text_primary),
                );
            }
        }
        kit.text(
            p,
            chart.min + vec2(space::S1, space::S1),
            Align2::LEFT_TOP,
            "Past 3 days · next 3 days (faded)",
            Type::Caption,
            t.text_tertiary,
        );
        y += sp::KP_CHART_H + space::S3;
    }
    for (s, primary) in lines(&w, utc) {
        let c = if primary {
            t.text_primary
        } else {
            t.text_secondary
        };
        let g = p.layout(s, Type::Body.font(), c, text_w);
        let gh = g.size().y;
        p.galley(pos2(x, y), g, c);
        y += gh + space::S1;
    }
    let eyebrow = size::ROW_H * 0.6;
    kit.text(
        p,
        pos2(x, y + eyebrow / 2.0),
        Align2::LEFT_CENTER,
        "LATEST ALERTS",
        Type::Caption,
        t.text_tertiary,
    );
    y += eyebrow;
    // The newest of each headline, up to five.
    let mut seen = Vec::new();
    let alerts: Vec<_> = w
        .alerts
        .iter()
        .filter(|a| {
            let new = !seen.contains(&a.title);
            seen.push(a.title.clone());
            new
        })
        .take(5)
        .collect();
    if alerts.is_empty() {
        kit.text(
            p,
            pos2(x, y),
            Align2::LEFT_TOP,
            "None",
            Type::Body,
            t.text_secondary,
        );
        y += sp::LINE_H;
    }
    for a in alerts {
        let g = p.layout(
            format!("{} · {}", a.title, model::clock_time(a.issued_ms, utc)),
            Type::Body.font(),
            t.text_secondary,
            text_w,
        );
        let gh = g.size().y;
        p.galley(pos2(x, y), g, t.text_secondary);
        y += gh + space::S1;
    }
    y + space::S2 - y0
}

/// The floating space weather card: centered in the map area, with the
/// shared panel header; scrolls when it doesn't fit.
pub fn card(ui: &mut Ui, kit: &mut Kit, model: &mut Model, area: Rect) {
    use crate::dock::Panel;
    let Picker::SpaceWeather { mut scroll } = model.picker else {
        return;
    };
    let pad = space::S4;
    let width = sp::CARD_W.min(area.width() - 2.0 * space::S3);
    let layer = LayerId::new(Order::Foreground, Id::new("space-weather"));
    let painter = ui.ctx().layer_painter(layer);
    let x0 = area.center().x - width / 2.0;
    let content_h = body(
        kit,
        model,
        &painter.with_clip_rect(Rect::NOTHING),
        x0 + pad,
        x0 + width - pad,
        0.0,
    );
    let top = area.min.y + space::S6;
    let h = (pad + size::PANEL_HEADER_H + content_h + pad).min(area.max.y - top - space::S3);
    let rect = Rect::from_min_size(pos2(x0, top), vec2(width, h));
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    ui.interact(rect, Id::new("space-bg"), egui::Sense::click_and_drag());
    kit.glass(&painter, rect, radius::LG, 3);
    let inner = rect.shrink(pad);
    let header = Rect::from_min_size(inner.min, vec2(inner.width(), size::PANEL_HEADER_H));
    let action = crate::design::components::panel_header(
        &mut ui,
        kit,
        &painter,
        header,
        Id::new("space-header"),
        Panel::Space.title(),
        false,
        false,
        true,
    );
    let area = Rect::from_min_max(pos2(inner.min.x, header.max.y), inner.max);
    if crate::design::components::pointer_over(&ui, area) {
        scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    scroll = scroll.clamp(0.0, (content_h - area.height()).max(0.0));
    body(
        kit,
        model,
        &painter.with_clip_rect(area),
        area.min.x,
        area.max.x,
        area.min.y - scroll,
    );
    crate::design::components::scroll_indicator(kit, &painter, area, scroll, content_h);
    match action {
        Some(crate::design::components::PanelAction::Close) => model.picker = Picker::None,
        Some(a) => crate::dock::act(model, Panel::Space, a),
        None => {
            if let Picker::SpaceWeather { scroll: s } = &mut model.picker {
                *s = scroll;
            }
        }
    }
}

/// The recorded SWPC answers in tests/data/swpc (2026-10-29, quiet with a
/// G1 forecast for Oct 31; the aurora grid trimmed to the Americas north of
/// 40°), for screenshots and tests. `None` outside a source checkout.
pub fn sample() -> Option<SpaceWeather> {
    use hookecho_core::space as s;
    let read = |name: &str| {
        std::fs::read(format!(
            "{}/../../tests/data/swpc/{name}-2026-10-29.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    };
    let (scales, days) = s::parse_scales(&read("noaa-scales")?).ok()?;
    let mut w = SpaceWeather {
        scales,
        days,
        kp: s::parse_kp_forecast(&read("noaa-planetary-k-index-forecast")?).ok()?,
        kp_now: s::parse_kp_1m(&read("planetary_k_index_1m")?).ok()?,
        wind: s::parse_solar_wind(&read("geospace_propagated-solar-wind-1-hour")?).ok()?,
        xray: s::parse_flares(&read("goes_primary_xray-flares-latest")?).ok()?,
        alerts: s::parse_alerts(&read("alerts")?).ok()?,
        aurora: Some(s::parse_aurora(&read("ovation_aurora")?).ok()?),
    };
    // Move it all to now.
    let shift = crate::warnings::now_ms() - w.kp_now?.0;
    if let Some(k) = &mut w.kp_now {
        k.0 += shift;
    }
    for k in &mut w.kp {
        k.time_ms += shift;
    }
    for a in &mut w.alerts {
        a.issued_ms += shift;
    }
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_text() {
        let w = sample().unwrap();
        let l = lines(&w, true);
        assert!(l[0].0.starts_with("Solar wind "), "{l:?}");
        assert!(l[1].0.starts_with("X-rays B1.9 · last flare B6.4 at "));
        assert!(l[4].0.starts_with("Oct 31: G1"), "{l:?}");
        assert_eq!(date_label("2026-10-31"), "Oct 31");
        let c = build(w.aurora.as_ref().unwrap());
        assert!(!c.indices.is_empty() && c.indices.len().is_multiple_of(6));
    }
}
