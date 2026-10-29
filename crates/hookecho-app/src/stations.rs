//! Surface observations (hookecho-core::obs), the owner's "command
//! center": station plots on the map when zoomed in, and the station panel
//! (bottom left) with one station's temperature, dewpoint and humidity,
//! wind and gusts, pressure and its 3-hour trend, visibility and weather.
//! The panel follows the station nearest home (or the map center without
//! one); a click on a plot pins that station instead.

use crate::design::components::{self, Kit};
use crate::design::stations as st;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{self, Model};
use eframe::egui::{Align2, Id, Painter, Pos2, Rect, Shape, Stroke, Ui, pos2, vec2};
use hookecho_core::config::{Config, Units};
use hookecho_core::obs::{History, Ob};

/// Reports older than this aren't shown (a station that stopped reporting).
const STALE_MS: i64 = 3 * 3_600_000;

fn now_ms() -> i64 {
    crate::warnings::now_ms()
}

/// The station the panel shows: the pinned one, else the one nearest
/// home or the map center.
pub fn panel_station(model: &Model) -> Option<&Ob> {
    let obs = &model.layers.obs;
    if let Some(id) = &model.chosen_station
        && let Some(o) = obs.by_id(id)
    {
        return Some(o);
    }
    let (lat, lon) = model.config.home.unwrap_or_else(|| model.map_center());
    obs.nearest(lat, lon, now_ms() - STALE_MS)
}

// ------------------------------------------------------------------- units

fn temp(c: &Config, t_c: f32) -> String {
    match c.units {
        Units::Imperial => format!("{:.0}°", t_c * 9.0 / 5.0 + 32.0),
        Units::Metric => format!("{t_c:.0}°"),
    }
}

fn temp_unit(c: &Config) -> &'static str {
    match c.units {
        Units::Imperial => "F",
        Units::Metric => "C",
    }
}

fn speed(c: &Config, kt: u16) -> String {
    let (label, per_ms) = c.speed.resolve(c.units);
    let v = kt as f32 * 0.514_444 * per_ms;
    format!("{v:.0} {label}")
}

/// 16-point compass name for a direction the wind blows from.
fn compass(deg: u16) -> &'static str {
    const P: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    P[((deg as f32 / 22.5).round() as usize) % 16]
}

fn wind(c: &Config, o: &Ob) -> String {
    match (o.wind_kt, o.wind_dir) {
        (None, _) => "Wind missing".into(),
        (Some(0), _) => "Calm".into(),
        (Some(k), dir) => {
            let from = dir.map_or("Variable", compass);
            let mut s = format!("{from} {}", speed(c, k));
            if let Some(g) = o.gust_kt {
                s.push_str(&format!(", gusts {}", speed(c, g)));
            }
            s
        }
    }
}

fn pressure(c: &Config, hpa: f32) -> String {
    match c.units {
        Units::Imperial => format!("{:.2} in", hpa / 33.863_89),
        Units::Metric => format!("{hpa:.1} hPa"),
    }
}

/// "rising 0.05 in/3 h": steady within 0.5 hPa (about 0.015 in).
fn trend(c: &Config, d_hpa: f32) -> String {
    let word = if d_hpa.abs() < 0.5 {
        return "steady".into();
    } else if d_hpa > 0.0 {
        "rising"
    } else {
        "falling"
    };
    let amount = match c.units {
        Units::Imperial => format!("{:.2} in", d_hpa.abs() / 33.863_89),
        Units::Metric => format!("{:.1} hPa", d_hpa.abs()),
    };
    format!("{word} {amount}/3 h")
}

fn visibility(c: &Config, o: &Ob) -> Option<String> {
    let mi = o.vis_mi?;
    let plus = if o.vis_plus { "+" } else { "" };
    Some(match c.units {
        Units::Imperial if mi < 1.0 => format!("{mi:.2}{plus} mi"),
        Units::Imperial => format!("{mi:.0}{plus} mi"),
        Units::Metric => format!("{:.0}{plus} km", mi * 1.609_344),
    })
}

// --------------------------------------------------------------- map plots

/// Plots on screen, thinned to [`st::MIN_SPACING`]: (station, dot).
fn plots(model: &Model, screen: Rect, view: [f64; 2]) -> Vec<(&Ob, Pos2)> {
    if !model.config.station_plots || model.camera.km_per_px() > st::MAX_KM_PER_PX {
        return Vec::new();
    }
    let since = now_ms() - STALE_MS;
    let mut out: Vec<(&Ob, Pos2)> = Vec::new();
    // A fixed order (the file's, newest report first per station) keeps
    // the same stations chosen as the map pans.
    for o in model
        .layers
        .obs
        .stations
        .iter()
        .filter(|o| o.time_ms >= since)
    {
        let p = model.camera.latlon_to_screen(o.lat, o.lon, view);
        let p = pos2(p[0] as f32, p[1] as f32);
        if !screen.contains(p) {
            continue;
        }
        if out
            .iter()
            .any(|(_, q)| q.distance_sq(p) < st::MIN_SPACING * st::MIN_SPACING)
        {
            continue;
        }
        out.push((o, p));
    }
    out
}

/// The area each plot covers (its numbers, dot and barb), for keeping
/// place labels clear of it.
pub fn label_exclusions(model: &Model, screen: Rect, view: [f64; 2]) -> Vec<[f32; 4]> {
    plots(model, screen, view)
        .into_iter()
        .map(|(_, p)| {
            let r = st::LABEL_CLEAR;
            [p.x - r[0], p.y - r[1], p.x + r[2], p.y + r[1]]
        })
        .collect()
}

/// Wind barb from `p`: a shaft toward where the wind comes from, then
/// pennants (50), full feathers (10) and half feathers (5) from the tip,
/// on the clockwise side as drawn in the Northern Hemisphere.
fn barb(painter: &Painter, p: Pos2, dir: u16, kt: u16) {
    let a = (dir as f32).to_radians();
    let out = vec2(a.sin(), -a.cos());
    let side = vec2(-out.y, out.x);
    let tip = p + out * st::BARB_SHAFT;
    let mut lines: Vec<[Pos2; 2]> = vec![[p, tip]];
    let mut pennants: Vec<[Pos2; 3]> = Vec::new();
    // Round to the nearest 5 kt.
    let mut left = (kt + 2) / 5 * 5;
    let mut at = tip;
    let feather = |at: Pos2, len: f32| at + side * len + out * len * 0.35;
    while left >= 50 {
        let next = at - out * st::BARB_STEP * 1.4;
        pennants.push([at, feather(at, st::BARB_FEATHER), next]);
        at = next - out * st::BARB_STEP * 0.4;
        left -= 50;
    }
    while left >= 10 {
        lines.push([at, feather(at, st::BARB_FEATHER)]);
        at -= out * st::BARB_STEP;
        left -= 10;
    }
    if left >= 5 {
        // A lone half feather sits one step in from the tip.
        if at == tip {
            at -= out * st::BARB_STEP;
        }
        lines.push([at, feather(at, st::BARB_FEATHER / 2.0)]);
    }
    for (w, color) in [
        (st::BARB_STROKE + st::HALO_EXTRA, st::HALO),
        (st::BARB_STROKE, st::WIND),
    ] {
        for l in &lines {
            painter.line_segment(*l, Stroke::new(w, color));
        }
        for t in &pennants {
            painter.add(Shape::convex_polygon(
                t.to_vec(),
                color,
                Stroke::new(w - st::BARB_STROKE, color),
            ));
        }
    }
}

pub fn draw(kit: &Kit, model: &Model, painter: &Painter, view: [f64; 2]) {
    let c = &model.config;
    for (o, p) in plots(model, kit.screen, view) {
        match (o.wind_kt, o.wind_dir) {
            (Some(k), Some(d)) if k > 0 => barb(painter, p, d, k),
            (Some(0), _) => {
                painter.circle_stroke(
                    p,
                    st::CALM_R,
                    Stroke::new(st::BARB_STROKE + st::HALO_EXTRA, st::HALO),
                );
                painter.circle_stroke(p, st::CALM_R, Stroke::new(st::BARB_STROKE, st::WIND));
            }
            _ => {}
        }
        painter.circle_filled(p, st::DOT_R + st::HALO_EXTRA / 2.0, st::HALO);
        painter.circle_filled(p, st::DOT_R, st::WIND);
        for (v, dy, color, anchor) in [
            (o.temp_c, -st::TEXT_DY, st::TEMP, Align2::RIGHT_CENTER),
            (
                o.dewpoint_c,
                st::TEXT_DY,
                st::DEWPOINT,
                Align2::RIGHT_CENTER,
            ),
        ] {
            let Some(v) = v else { continue };
            let at = p + vec2(-st::TEXT_DX, dy);
            let text = temp(c, v);
            for d in st::TEXT_HALO {
                kit.text(painter, at + d, anchor, &text, Type::CaptionTab, st::HALO);
            }
            kit.text(painter, at, anchor, text, Type::CaptionTab, color);
        }
    }
}

/// A click near a plotted station pins it to the panel. Returns whether
/// one was hit.
pub fn click(model: &mut Model, screen: Rect, at: Pos2, view: [f64; 2]) -> bool {
    let hit = plots(model, screen, view)
        .into_iter()
        .filter(|(_, p)| p.distance(at) <= st::HIT_R)
        .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
        .map(|(o, _)| o.id.clone());
    match hit {
        Some(id) => {
            model.chosen_station = Some(id);
            model.config.station_panel = true;
            true
        }
        None => false,
    }
}

// ------------------------------------------------------------------- panel

/// The station panel's lines below the big temperature.
fn panel_lines(c: &Config, o: &Ob, h: Option<&History>) -> Vec<String> {
    let mut v = Vec::new();
    v.push(wind(c, o));
    if let Some(p) = o.altimeter_hpa {
        let d = h
            .filter(|h| h.id == o.id)
            .and_then(History::trend_hpa)
            .or(o.tendency_hpa);
        v.push(match d {
            Some(d) => format!("Pressure {}, {}", pressure(c, p), trend(c, d)),
            None => format!("Pressure {}", pressure(c, p)),
        });
    }
    let mut vis = visibility(c, o).map(|s| format!("Visibility {s}"));
    if !o.wx.is_empty() {
        vis = Some(match vis {
            Some(s) => format!("{s} · {}", o.wx),
            None => o.wx.clone(),
        });
    }
    if let Some(s) = vis {
        v.push(s);
    }
    v
}

/// The station panel's body: the station, its temperature, dew point and
/// humidity, wind, pressure and trend, visibility and weather, and how old
/// the report is. Drawn from `y` between `x0` and `x1`; returns its height.
pub fn body(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &Painter,
    x0: f32,
    x1: f32,
    y0: f32,
) -> f32 {
    let t = kit.theme.clone();
    let Some(o) = panel_station(model).cloned() else {
        let msg = if model.layers.obs_trouble.is_some() {
            "Can't reach aviationweather.gov; retrying"
        } else {
            "Waiting for observations…"
        };
        kit.text(
            painter,
            pos2(x0, y0 + st::PANEL_LINE_H / 2.0),
            Align2::LEFT_CENTER,
            msg,
            Type::Body,
            t.text_secondary,
        );
        return st::PANEL_LINE_H + space::S2;
    };
    let c = model.config.clone();
    let history = model.layers.obs.station.clone();
    let name = history
        .as_ref()
        .filter(|h| h.id == o.id)
        .map(|h| h.name.clone())
        .unwrap_or_default();
    let lines = panel_lines(&c, &o, history.as_ref());
    let clip = painter.with_clip_rect(painter.clip_rect().intersect(Rect::from_min_max(
        pos2(x0, painter.clip_rect().min.y),
        pos2(x1, painter.clip_rect().max.y),
    )));
    let x = x0;
    let mut y = y0 + st::PANEL_LINE_H / 2.0;
    let id_rect = kit.text(
        &clip,
        pos2(x, y),
        Align2::LEFT_CENTER,
        &o.id,
        Type::Label,
        t.text_primary,
    );
    // "Oklahoma City/Will Rogers Intl, OK, US" → "Oklahoma City/Will Rogers Intl".
    let short = name.split(", ").next().unwrap_or_default();
    kit.text(
        &clip,
        pos2(id_rect.max.x + space::S2, y),
        Align2::LEFT_CENTER,
        short,
        Type::Caption,
        t.text_secondary,
    );
    y += st::PANEL_LINE_H / 2.0 + st::PANEL_VALUE_H / 2.0;
    let big = o.temp_c.map_or("—".to_string(), |v| {
        format!("{}{}", temp(&c, v), temp_unit(&c))
    });
    let big_rect = kit.text(
        &clip,
        pos2(x, y),
        Align2::LEFT_CENTER,
        big,
        Type::Value,
        t.text_primary,
    );
    if let Some(td) = o.dewpoint_c {
        let mut s = format!("Dew point {}", temp(&c, td));
        if let Some(tc) = o.temp_c {
            s.push_str(&format!(
                " · RH {:.0}%",
                hookecho_core::obs::relative_humidity(tc, td)
            ));
        }
        kit.text(
            &clip,
            pos2(big_rect.max.x + space::S3, y),
            Align2::LEFT_CENTER,
            s,
            Type::Caption,
            t.text_secondary,
        );
    }
    y += st::PANEL_VALUE_H / 2.0 + st::PANEL_LINE_H / 2.0;
    for s in &lines {
        kit.text(
            &clip,
            pos2(x, y),
            Align2::LEFT_CENTER,
            s,
            Type::Body,
            t.text_primary,
        );
        y += st::PANEL_LINE_H;
    }
    let age = ((now_ms() - o.time_ms) / 1000).max(0);
    let mut foot = format!(
        "{} · {}",
        model::ago(age.max(120)),
        model::clock_time(o.time_ms, c.utc)
    );
    let chosen = model.chosen_station.is_some();
    foot.push_str(if chosen {
        " · chosen on the map"
    } else if c.home.is_some() {
        " · nearest home"
    } else {
        " · nearest map center"
    });
    kit.text(
        &clip,
        pos2(x, y),
        Align2::LEFT_CENTER,
        foot,
        Type::Caption,
        t.text_tertiary,
    );
    y += st::PANEL_LINE_H / 2.0;
    // (A measuring pass, with nothing visible, takes no clicks.)
    if chosen && painter.clip_rect().is_positive() {
        // Back to following the nearest station.
        let r = Rect::from_min_max(pos2(x, y), pos2(x1, y + st::PANEL_LINE_H));
        let resp = components::interact(
            ui,
            r,
            Id::new("station-nearest"),
            eframe::egui::WidgetType::Button,
            "Use the nearest station",
            None,
        );
        kit.text(
            &clip,
            pos2(x, r.center().y),
            Align2::LEFT_CENTER,
            "Use the nearest station",
            Type::Caption,
            if resp.hovered() {
                t.text_primary
            } else {
                t.accent
            },
        );
        if resp.clicked() {
            model.chosen_station = None;
        }
        y = r.max.y;
    } else if chosen {
        y += st::PANEL_LINE_H;
    }
    y + space::S2 - y0
}

/// The floating station panel: bottom left of the map area, above the
/// timeline row.
pub fn floating(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &Painter,
    area: Rect,
    m: f32,
) {
    use crate::dock::Panel;
    if !model.config.station_panel || crate::dock::docked(model, kit.screen, Panel::Station) {
        return;
    }
    let pad = space::S3;
    let w = st::PANEL_W.min(area.width() - 2.0 * m);
    let x0 = area.min.x + m;
    // Measure the body with nothing visible, then draw it for real.
    let hidden = painter.with_clip_rect(Rect::NOTHING);
    let body_h = body(ui, kit, model, &hidden, x0 + pad, x0 + w - pad, 0.0);
    let h = pad + size::PANEL_HEADER_H + body_h + pad - space::S2;
    let bottom = area.max.y - m - size::TIMELINE_H - space::S2;
    let rect = Rect::from_min_size(pos2(x0, bottom - h), vec2(w, h));
    ui.interact(rect, Id::new("station-panel"), eframe::egui::Sense::click());
    kit.glass(painter, rect, radius::LG, 1);
    let inner = rect.shrink(pad);
    let header = Rect::from_min_size(inner.min, vec2(inner.width(), size::PANEL_HEADER_H));
    let action = components::panel_header(
        ui,
        kit,
        painter,
        header,
        Id::new("station-panel-header"),
        Panel::Station.title(),
        false,
        false,
        true,
    );
    let clip = painter.with_clip_rect(rect);
    body(
        ui,
        kit,
        model,
        &clip,
        inner.min.x,
        inner.max.x,
        header.max.y,
    );
    if let Some(a) = action {
        crate::dock::act(model, Panel::Station, a);
    }
}

/// The recorded observations in tests/data/metar (a trimmed cache file and
/// Oklahoma City's last four hours, 2026-10-29), moved to now, for
/// screenshots and tests. `None` outside a source checkout.
pub fn sample() -> Option<hookecho_core::obs::Obs> {
    use hookecho_core::obs;
    let read = |name: &str| {
        std::fs::read(format!(
            "{}/../../tests/data/metar/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    };
    let mut stations = obs::parse_cache(&read("metars-2026-10-29T1514Z.csv.gz")?).ok()?;
    let mut station = obs::parse_history(&read("KOKC-hours4-2026-10-29T1500Z.json")?).ok()?;
    let newest = stations.iter().map(|o| o.time_ms).max()?;
    let shift = now_ms() - newest;
    for o in stations.iter_mut().chain(station.reports.iter_mut()) {
        o.time_ms += shift;
    }
    Some(obs::Obs {
        stations,
        station: Some(station),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_words() {
        let mut c = Config::default();
        assert_eq!(temp(&c, 20.0), "68°");
        assert_eq!(compass(0), "N");
        assert_eq!(compass(190), "S");
        assert_eq!(compass(350), "N");
        assert_eq!(compass(225), "SW");
        assert_eq!(speed(&c, 10), "10 kt");
        assert_eq!(pressure(&c, 1013.25), "29.92 in");
        assert_eq!(trend(&c, 1.7), "rising 0.05 in/3 h");
        assert_eq!(trend(&c, -0.2), "steady");
        c.units = Units::Metric;
        assert_eq!(temp(&c, 20.0), "20°");
        assert_eq!(pressure(&c, 1013.25), "1013.2 hPa");
        assert_eq!(trend(&c, -2.04), "falling 2.0 hPa/3 h");
    }
}
