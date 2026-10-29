//! Space weather (hookecho-core::space): the OVATION aurora forecast as a
//! translucent overlay on the map, and the space weather card (Menu →
//! "Space weather", or Layers): the NOAA R/S/G levels, Kp now and the Kp
//! chart for the past three days and the three ahead, solar wind, X-rays
//! and the latest flare, the three-day outlook and SWPC's latest alerts.

use crate::design::components::{Kit, icon_button};
use crate::design::icons::Icon;
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

pub fn card(ui: &mut Ui, kit: &mut Kit, model: &mut Model) {
    let Picker::SpaceWeather { mut scroll } = model.picker else {
        return;
    };
    let t = kit.theme.clone();
    let w = model.layers.space.clone();
    let utc = model.config.utc;
    let now = crate::warnings::now_ms();
    let pad = space::S4;
    let screen = kit.screen;
    let width = sp::CARD_W.min(screen.width() - 2.0 * space::S3);
    let text_w = width - 2.0 * pad;
    let layer = LayerId::new(Order::Foreground, Id::new("space-weather"));
    let painter = ui.ctx().layer_painter(layer);
    let body = lines(&w, utc);
    // Alerts: the newest of each headline, up to five.
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
        .map(|a| {
            painter.layout(
                format!("{} · {}", a.title, model::clock_time(a.issued_ms, utc)),
                Type::Body.font(),
                t.text_secondary,
                text_w,
            )
        })
        .collect();
    let body_g: Vec<_> = body
        .iter()
        .map(|(s, primary)| {
            let c = if *primary {
                t.text_primary
            } else {
                t.text_secondary
            };
            painter.layout(s.clone(), Type::Body.font(), c, text_w)
        })
        .collect();
    let eyebrow = size::ROW_H * 0.6;
    let content_h = size::ROW_H
        + sp::SCALE_CHIP[1]
        + space::S3
        + sp::LINE_H
        + sp::KP_CHART_H
        + space::S3
        + body_g.iter().map(|g| g.size().y + space::S1).sum::<f32>()
        + eyebrow
        + alerts.iter().map(|g| g.size().y + space::S1).sum::<f32>();
    let top = screen.min.y + space::S6;
    let h = (pad + content_h + pad).min(screen.max.y - top - space::S3);
    let rect = Rect::from_min_size(pos2(screen.center().x - width / 2.0, top), vec2(width, h));
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    ui.interact(rect, Id::new("space-bg"), egui::Sense::click_and_drag());
    kit.glass(&painter, rect, radius::LG, 3);
    let area = rect.shrink(pad);
    if crate::design::components::pointer_over(&ui, rect) {
        scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    scroll = scroll.clamp(0.0, (content_h - area.height()).max(0.0));
    let p = painter.with_clip_rect(area);
    let x = area.min.x;
    let mut y = area.min.y - scroll;

    kit.text(
        &p,
        pos2(x, y + size::ROW_H / 2.0),
        Align2::LEFT_CENTER,
        "Space weather",
        Type::Title,
        t.text_primary,
    );
    if let Some((ms, _)) = w.kp_now {
        kit.text(
            &p,
            pos2(area.max.x - size::ICON_BUTTON, y + size::ROW_H / 2.0),
            Align2::RIGHT_CENTER,
            format!("NOAA SWPC · {}", model::clock_time(ms, utc)),
            Type::Caption,
            t.text_tertiary,
        );
    }
    y += size::ROW_H;

    // R, S, G chips.
    let mut cx = x;
    for (letter, level) in [('R', w.scales.r), ('S', w.scales.s), ('G', w.scales.g)] {
        let r = Rect::from_min_size(pos2(cx, y), vec2(sp::SCALE_CHIP[0], sp::SCALE_CHIP[1]));
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
            &p,
            r.center() - egui::Vec2::Y * space::S2,
            Align2::CENTER_CENTER,
            format!("{letter}{level}"),
            Type::LabelTab,
            color.unwrap_or(t.text_primary),
        );
        kit.text(
            &p,
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
        kp_line.push_str(&format!(" · up to {m:.1} expected (G{})", g_for_kp(m)));
    }
    kit.text(
        &p,
        pos2(x, y + sp::LINE_H / 2.0),
        Align2::LEFT_CENTER,
        kp_line,
        Type::BodyTab,
        t.text_primary,
    );
    y += sp::LINE_H;
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
            &p,
            chart.min + vec2(space::S1, space::S1),
            Align2::LEFT_TOP,
            "Past 3 days · next 3 days (faded)",
            Type::Caption,
            t.text_tertiary,
        );
    }
    y += sp::KP_CHART_H + space::S3;
    for g in body_g {
        let gh = g.size().y;
        p.galley(pos2(x, y), g, t.text_primary);
        y += gh + space::S1;
    }
    kit.text(
        &p,
        pos2(x, y + eyebrow / 2.0),
        Align2::LEFT_CENTER,
        "LATEST ALERTS",
        Type::Caption,
        t.text_tertiary,
    );
    y += eyebrow;
    if alerts.is_empty() {
        kit.text(
            &p,
            pos2(x, y),
            Align2::LEFT_TOP,
            "None",
            Type::Body,
            t.text_secondary,
        );
    }
    for g in alerts {
        let gh = g.size().y;
        p.galley(pos2(x, y), g, t.text_secondary);
        y += gh + space::S1;
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
        Id::new("space-close"),
        Icon::X,
        "Close",
        Some("Esc"),
        false,
    )
    .clicked();
    if closed {
        model.picker = Picker::None;
    } else if let Picker::SpaceWeather { scroll: s } = &mut model.picker {
        *s = scroll;
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
