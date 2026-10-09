//! The main screen (docs/DESIGN.md §3), composed only from `design::components`.

use crate::basemap::Place;
use crate::design::components::{Kit, measure, row};
use crate::design::icons::{Icon, IconCache};
use crate::design::tokens::{
    Theme, Type, map_label_font, map_label_font_bold, radius, size, space,
};
use crate::palettes;
use crate::radar::{Moment, SweepData, beam_height_km};
use crate::view::{Camera, from_merc, range_bearing, to_merc};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Stroke, StrokeKind, TextureId, pos2,
    vec2,
};
use std::collections::HashMap;

pub struct Site {
    pub id: &'static str,
    pub city: &'static str,
    pub lat: f64,
    pub lon: f64,
    pub elevation_m: f64,
}

/// Prototype-only stand-ins (DESIGN gate screenshots): the sample volume is
/// archived, so "live" age and the loop's frame list are staged.
pub struct Staged {
    /// Frame times (seconds since midnight UTC) for the scrubber ticks.
    pub frames_utc: Vec<u32>,
    pub age_text: &'static str,
    pub utc_offset_h: i32,
    pub zone: &'static str,
}

pub struct Model {
    pub site: Site,
    pub staged: Staged,
    pub sweeps: HashMap<Moment, SweepData>,
    pub places: Vec<Place>,
    pub moment: Moment,
    pub camera: Camera,
    pub me: Option<(f64, f64)>,
    /// Where the inspector reads: pointer position or a pinned location.
    pub inspect: Option<(f64, f64)>,
    pub playing: bool,
    pub frame: usize,
}

pub struct Textures {
    pub scene: TextureId,
    pub blur: TextureId,
}

fn fmt_time(secs_utc: u32, offset_h: i32) -> String {
    let local = (secs_utc as i32 + offset_h * 3600).rem_euclid(86400);
    let (h, m) = (local / 3600, (local / 60) % 60);
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m:02} {ampm}")
}

fn km_to_mi(km: f64) -> f64 {
    km * 0.621_371
}

pub fn paint(
    ctx: &egui::Context,
    model: &Model,
    theme: &Theme,
    icons: &mut IconCache,
    tex: &Textures,
) {
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let mut kit = Kit {
        painter: &painter,
        theme,
        icons,
        blur: tex.blur,
        screen,
    };
    let compact = screen.width() < size::COMPACT_BREAKPOINT;
    let margin = if compact { space::S3 } else { space::S4 };
    let half = [screen.center().x as f64, screen.center().y as f64];
    let project = |lat: f64, lon: f64| {
        let p = model.camera.to_screen(to_merc(lat, lon), half);
        pos2(p[0] as f32, p[1] as f32)
    };

    // Map: scene texture, then labels and markers (glyphon/GPU in the real renderer).
    painter.image(
        tex.scene,
        screen,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    paint_labels(&kit, model, &project, screen);
    paint_markers(&kit, model, &project);

    // Chrome.
    let site_rect = site_chip(&mut kit, model, pos2(margin, margin));
    product_chip(&mut kit, model, pos2(site_rect.max.x + space::S2, margin));
    top_right(&mut kit, pos2(screen.max.x - margin, margin));
    let legend_rect = legend(&mut kit, model, screen, margin, compact);
    timeline(&mut kit, model, screen, margin, compact, legend_rect);
    if let Some(at) = model.inspect {
        inspector(&mut kit, model, project(at.0, at.1), at);
    }
}

fn paint_labels(kit: &Kit, model: &Model, project: &impl Fn(f64, f64) -> Pos2, screen: Rect) {
    // Greedy declutter by land area (Census gazetteer ALAND as a size proxy).
    // The top chip row and bottom timeline band are pre-reserved so labels
    // never slide under the chrome.
    let mut taken: Vec<Rect> = vec![
        Rect::from_min_max(
            screen.min,
            pos2(
                screen.max.x,
                screen.min.y + space::S4 + size::CHIP_H + space::S2,
            ),
        ),
        Rect::from_min_max(
            pos2(
                screen.min.x,
                screen.max.y - space::S4 - size::TIMELINE_H - space::S2,
            ),
            screen.max,
        ),
    ];
    let km_per_pt = 2.0 * std::f64::consts::PI * 6371.0 * model.site.lat.to_radians().cos()
        / model.camera.scale;
    let min_area = (km_per_pt * km_per_pt * 2.0e5).max(4.0e6) as u64;
    for p in model
        .places
        .iter()
        .filter(|p| p.land_area >= min_area)
        .take(400)
    {
        let pos = project(p.lat, p.lon);
        if !screen.shrink(8.0).contains(pos) {
            continue;
        }
        let major = p.land_area > 300_000_000;
        let font = if major {
            map_label_font_bold()
        } else {
            map_label_font(false)
        };
        let g = kit
            .painter
            .layout_no_wrap(p.name.clone(), font, kit.theme.map_label);
        let rect = Rect::from_center_size(pos, g.size()).expand(6.0);
        if taken.iter().any(|r| r.intersects(rect)) {
            continue;
        }
        taken.push(rect);
        let origin = pos - g.size() / 2.0;
        let ppp = kit.painter.ctx().pixels_per_point();
        let d = 1.2 / ppp.max(1.0) * ppp.min(2.0) / 1.0;
        for (dx, dy) in [
            (-d, 0.0),
            (d, 0.0),
            (0.0, -d),
            (0.0, d),
            (-d, -d),
            (d, d),
            (-d, d),
            (d, -d),
        ] {
            kit.painter.galley_with_override_text_color(
                origin + vec2(dx, dy),
                g.clone(),
                kit.theme.map_label_halo,
            );
        }
        kit.painter.galley(origin, g, kit.theme.map_label);
    }
}

fn paint_markers(kit: &Kit, model: &Model, project: &impl Fn(f64, f64) -> Pos2) {
    let t = kit.theme;
    // Radar site: small hollow ring + ID.
    let rp = project(model.site.lat, model.site.lon);
    kit.painter
        .circle_stroke(rp, 5.0, Stroke::new(3.0, t.map_label_halo));
    kit.painter
        .circle_stroke(rp, 5.0, Stroke::new(1.5, t.map_label));
    kit.text(
        rp + vec2(9.0, 0.0),
        Align2::LEFT_CENTER,
        model.site.id,
        Type::Caption,
        t.map_label,
    );
    // Me: accent dot with a white ring and a soft halo.
    if let Some(me) = model.me {
        let p = project(me.0, me.1);
        kit.painter
            .circle_filled(p, 14.0, t.accent.gamma_multiply(0.18));
        kit.painter.circle_filled(p, 7.0, t.me_ring);
        kit.painter.circle_filled(p, 5.0, t.accent);
    }
}

fn site_chip(kit: &mut Kit, model: &Model, at: Pos2) -> Rect {
    let t = kit.theme;
    let pad = space::S3 + 2.0;
    let line1 = vec![
        (model.site.id.to_string(), Type::Label, t.text_primary, 0.0),
        (
            model.site.city.to_string(),
            Type::Body,
            t.text_secondary,
            6.0,
        ),
    ];
    let dist = model
        .me
        .map(|me| km_to_mi(range_bearing(me, (model.site.lat, model.site.lon)).0));
    let mut line2 = vec![(
        model.staged.age_text.to_string(),
        Type::CaptionTab,
        t.text_secondary,
        size::STATUS_DOT + 6.0,
    )];
    if let Some(d) = dist {
        line2.push(("·".into(), Type::Caption, t.text_tertiary, 6.0));
        line2.push((
            format!("{d:.0} mi away"),
            Type::CaptionTab,
            t.text_tertiary,
            6.0,
        ));
    }
    let text_x = pad + size::ICON_SM + space::S2;
    let w = text_x + measure(kit, &line1).max(measure(kit, &line2)) + pad;
    let rect = Rect::from_min_size(at, vec2(w, size::CHIP_H));
    kit.glass(rect, radius::MD, 1);
    if kit.hovered(rect) {
        kit.painter
            .rect_filled(rect, CornerRadius::same(radius::MD), t.hover);
    }
    let y1 = rect.min.y + 20.0;
    let y2 = rect.min.y + 39.0;
    kit.icon(
        Rect::from_center_size(pos2(rect.min.x + pad + 8.0, y1), vec2(16.0, 16.0)),
        Icon::Radar,
        size::ICON_SM,
        t.text_secondary,
    );
    row(kit, rect.min.x + text_x, y1, &line1);
    kit.status_dot(
        pos2(rect.min.x + text_x + size::STATUS_DOT / 2.0, y2),
        t.status_fresh,
    );
    row(kit, rect.min.x + text_x, y2, &line2);
    rect
}

fn product_name(m: Moment) -> &'static str {
    match m {
        Moment::Reflectivity => "Reflectivity",
        Moment::Velocity => "Velocity",
    }
}

fn product_chip(kit: &mut Kit, model: &Model, at: Pos2) -> Rect {
    let t = kit.theme;
    let pad = space::S3 + 2.0;
    let elev = model
        .sweeps
        .get(&model.moment)
        .map_or(0.5, |s| s.elevation_deg);
    let line1 = vec![
        (
            product_name(model.moment).to_string(),
            Type::Label,
            t.text_primary,
            0.0,
        ),
        (format!("{elev:.1}°"), Type::LabelTab, t.text_secondary, 6.0),
    ];
    let line2 = vec![(
        "Super-res · Level II".to_string(),
        Type::Caption,
        t.text_tertiary,
        0.0,
    )];
    // Progressive sweep indicator: one dot per tilt in the VCP (in / arriving / pending).
    let tilts = 14;
    let dots_w = tilts as f32 * 7.0;
    let w = pad
        + measure(kit, &line1).max(measure(kit, &line2) + space::S3 + dots_w)
        + space::S2
        + size::ICON_SM
        + pad;
    let rect = Rect::from_min_size(at, vec2(w, size::CHIP_H));
    kit.glass(rect, radius::MD, 1);
    if kit.hovered(rect) {
        kit.painter
            .rect_filled(rect, CornerRadius::same(radius::MD), t.hover);
    }
    let y1 = rect.min.y + 20.0;
    let y2 = rect.min.y + 39.0;
    row(kit, rect.min.x + pad, y1, &line1);
    let end = row(kit, rect.min.x + pad, y2, &line2);
    let mut x = end + space::S3;
    for i in 0..tilts {
        let c = pos2(x + 2.5, y2);
        match i {
            0..=10 => {
                kit.painter.circle_filled(c, 2.5, t.text_secondary);
            }
            11 => {
                kit.painter
                    .circle_stroke(c, 2.25, Stroke::new(1.0, t.accent));
                kit.painter.circle_filled(c, 1.25, t.accent);
            }
            _ => {
                kit.painter
                    .circle_stroke(c, 2.25, Stroke::new(1.0, t.text_tertiary));
            }
        }
        x += 7.0;
    }
    kit.icon(
        Rect::from_center_size(pos2(rect.max.x - pad - 8.0, y1), vec2(16.0, 16.0)),
        Icon::ChevronDown,
        size::ICON_SM,
        t.text_secondary,
    );
    rect
}

fn top_right(kit: &mut Kit, top_right: Pos2) -> Rect {
    let b = size::ICON_BUTTON;
    let pad = (size::CHIP_H - b) / 2.0;
    let w = pad * 2.0 + b * 2.0 + space::S1;
    let rect = Rect::from_min_size(pos2(top_right.x - w, top_right.y), vec2(w, size::CHIP_H));
    kit.glass(rect, radius::MD, 1);
    let r1 = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(b, b));
    let r2 = r1.translate(vec2(b + space::S1, 0.0));
    let (h1, h2) = (kit.hovered(r1), kit.hovered(r2));
    kit.icon_button(r1, Icon::Layers, h1, false);
    kit.icon_button(r2, Icon::Search, h2, false);
    rect
}

fn legend(kit: &mut Kit, model: &Model, screen: Rect, margin: f32, compact: bool) -> Rect {
    let t = kit.theme;
    let h = 60.0;
    let bottom = if compact {
        screen.max.y - margin - size::TIMELINE_H - space::S2
    } else {
        screen.max.y - margin
    };
    let rect = Rect::from_min_max(
        pos2(screen.max.x - margin - size::LEGEND_W, bottom - h),
        pos2(screen.max.x - margin, bottom),
    );
    kit.glass(rect, radius::MD, 1);
    let pad = space::S3;
    let (unit, lo, hi, ticks, to_unit): (&str, f32, f32, Vec<f32>, f32) = match model.moment {
        Moment::Reflectivity => (
            "dBZ",
            10.0,
            75.0,
            vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0],
            1.0,
        ),
        Moment::Velocity => (
            "kt",
            -45.0,
            45.0,
            vec![-80.0, -40.0, 0.0, 40.0, 80.0],
            1.943_844,
        ),
    };
    kit.text(
        pos2(rect.min.x + pad, rect.min.y + 15.0),
        Align2::LEFT_CENTER,
        product_name(model.moment),
        Type::Caption,
        t.text_secondary,
    );
    kit.text(
        pos2(rect.max.x - pad, rect.min.y + 15.0),
        Align2::RIGHT_CENTER,
        unit,
        Type::Caption,
        t.text_tertiary,
    );
    let bar = Rect::from_min_max(
        pos2(rect.min.x + pad, rect.min.y + 26.0),
        pos2(rect.max.x - pad, rect.min.y + 34.0),
    );
    let stops = palettes::stops(model.moment);
    let mut mesh = Mesh::default();
    let n = 96;
    for i in 0..n {
        let a = i as f32 / n as f32;
        let b = (i + 1) as f32 / n as f32;
        let c = palettes::sample(stops, lo + (hi - lo) * (a + b) / 2.0);
        // Show the palette fully opaque; its alpha only matters over the map.
        let col = Color32::from_rgb(c[0], c[1], c[2]);
        let r = Rect::from_min_max(
            pos2(bar.lerp_inside(vec2(a, 0.0)).x, bar.min.y),
            pos2(bar.lerp_inside(vec2(b, 0.0)).x, bar.max.y),
        );
        mesh.add_colored_rect(r, col);
    }
    kit.painter.add(mesh);
    kit.painter.rect_stroke(
        bar,
        CornerRadius::same(2),
        Stroke::new(1.0, t.glass_stroke),
        StrokeKind::Outside,
    );
    for v in ticks {
        let native = v / to_unit;
        let x = bar.min.x + (native - lo) / (hi - lo) * bar.width();
        let label = if v > 0.0 && model.moment == Moment::Velocity {
            format!("+{v:.0}")
        } else {
            format!("{v:.0}")
        };
        kit.text(
            pos2(x, bar.max.y + 10.0),
            Align2::CENTER_CENTER,
            label,
            Type::CaptionTab,
            t.text_tertiary,
        );
    }
    rect
}

fn timeline(kit: &mut Kit, model: &Model, screen: Rect, margin: f32, compact: bool, legend: Rect) {
    let t = kit.theme;
    let h = size::TIMELINE_H;
    let y = screen.max.y - margin - h;
    let rect = if compact {
        Rect::from_min_size(
            pos2(screen.min.x + margin, y),
            vec2(screen.width() - 2.0 * margin, h),
        )
    } else {
        let w = size::TIMELINE_MAX_W.min(screen.width() - 2.0 * margin);
        let mut x = screen.center().x - w / 2.0;
        // Never collide with the legend: slide left, then shrink.
        let limit = legend.min.x - space::S2;
        if x + w > limit {
            x = (limit - w).max(screen.min.x + margin);
        }
        Rect::from_min_size(pos2(x, y), vec2(w.min(limit - x), h))
    };
    kit.glass(rect, radius::MD, 1);
    let b = size::ICON_BUTTON;
    let pad = (h - b) / 2.0;
    let play = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(b, b));
    let hov = kit.hovered(play);
    kit.icon_button(
        play,
        if model.playing {
            Icon::Pause
        } else {
            Icon::Play
        },
        hov,
        false,
    );

    // Right side: frame time, speed, live.
    let cy = rect.center().y;
    let live_w = 64.0;
    let live = Rect::from_min_size(
        pos2(rect.max.x - pad - live_w, cy - 14.0),
        vec2(live_w, 28.0),
    );
    let is_live = model.frame + 1 == model.staged.frames_utc.len();
    kit.painter.rect_filled(
        live,
        CornerRadius::same(radius::SM),
        if is_live { t.selected } else { t.hover },
    );
    kit.status_dot(
        pos2(live.min.x + 14.0, cy),
        if is_live { t.accent } else { t.text_tertiary },
    );
    kit.text(
        pos2(live.min.x + 24.0, cy),
        Align2::LEFT_CENTER,
        "Live",
        Type::Caption,
        if is_live { t.accent } else { t.text_secondary },
    );
    let speed = kit.text(
        pos2(live.min.x - space::S3, cy),
        Align2::RIGHT_CENTER,
        "1×",
        Type::CaptionTab,
        t.text_secondary,
    );
    let frame_secs = model.staged.frames_utc[model.frame];
    let time = kit.text(
        pos2(speed.min.x - space::S4, cy),
        Align2::RIGHT_CENTER,
        format!(
            "{} {}",
            fmt_time(frame_secs, model.staged.utc_offset_h),
            model.staged.zone
        ),
        Type::LabelTab,
        t.text_primary,
    );

    // Scrubber: ticks spaced by real scan time so data gaps are visible.
    let track = Rect::from_min_max(
        pos2(play.max.x + space::S3, cy - 1.0),
        pos2(time.min.x - space::S5, cy + 1.0),
    );
    kit.painter
        .rect_filled(track, CornerRadius::same(1), t.track);
    let f = &model.staged.frames_utc;
    let (t0, t1) = (f[0] as f32, *f.last().unwrap() as f32);
    let x_of = |s: u32| track.min.x + 4.0 + (s as f32 - t0) / (t1 - t0) * (track.width() - 8.0);
    let cur_x = x_of(f[model.frame]);
    kit.painter.rect_filled(
        Rect::from_min_max(track.min, pos2(cur_x, track.max.y)),
        CornerRadius::same(1),
        t.accent.gamma_multiply(0.55),
    );
    for (i, &s) in f.iter().enumerate() {
        let x = x_of(s);
        let tick = Rect::from_center_size(pos2(x, cy), vec2(2.0, 10.0));
        let col = if i <= model.frame {
            t.text_secondary
        } else {
            t.text_tertiary
        };
        kit.painter.rect_filled(tick, CornerRadius::same(1), col);
    }
    let head = Rect::from_center_size(pos2(cur_x, cy), vec2(4.0, 20.0));
    kit.painter
        .rect_filled(head, CornerRadius::same(2), t.accent);
}

fn inspector(kit: &mut Kit, model: &Model, p: Pos2, at: (f64, f64)) {
    let t = kit.theme;
    let Some(sweep) = model.sweeps.get(&model.moment) else {
        return;
    };
    let site = (model.site.lat, model.site.lon);
    let (rng_km, brg) = range_bearing(site, at);
    let raw = sweep.raw_at(rng_km as f32, brg as f32);
    let (value, unit, note) = match (raw, model.moment) {
        (Some(1), Moment::Velocity) => ("RF".to_string(), "", Some("Range folded")),
        (Some(r), Moment::Reflectivity) if r > 1 => {
            (format!("{:.1}", sweep.value(r).unwrap()), "dBZ", None)
        }
        (Some(r), Moment::Velocity) if r > 1 => {
            let kt = sweep.value(r).unwrap() * 1.943_844;
            let note = if kt >= 0.0 {
                "Away from radar"
            } else {
                "Toward radar"
            };
            (format!("{:+.0}", kt), "kt", Some(note))
        }
        _ => ("—".to_string(), "", Some("No echo")),
    };
    let slant = rng_km as f32 / sweep.elevation_deg.to_radians().cos();
    let arl_ft = beam_height_km(slant, sweep.elevation_deg) * 3280.84;

    // Target reticle.
    kit.painter
        .circle_stroke(p, 9.0, Stroke::new(3.0, t.map_label_halo));
    kit.painter
        .circle_stroke(p, 9.0, Stroke::new(1.5, t.text_primary));
    kit.painter.circle_filled(p, 1.75, t.text_primary);

    let rows: Vec<(String, String)> = {
        let mut v = vec![
            (
                "Beam height".to_string(),
                format!("{:.0} ft ARL", (arl_ft / 10.0).round() * 10.0),
            ),
            (
                format!("From {}", model.site.id),
                format!("{:.1} mi  {:03.0}°", km_to_mi(rng_km), brg),
            ),
        ];
        if let Some(me) = model.me {
            let (d, b) = range_bearing(me, at);
            v.push((
                "From you".to_string(),
                format!("{:.1} mi  {:03.0}°", km_to_mi(d), b),
            ));
        }
        v
    };
    let pad = space::S3;
    let w = 212.0;
    let h = pad
        + 28.0
        + if note.is_some() { 16.0 } else { 0.0 }
        + space::S2
        + rows.len() as f32 * 20.0
        + pad
        - 4.0;
    let screen = kit.screen;
    let mut min = p + vec2(18.0, 18.0);
    if min.x + w > screen.max.x - space::S4 {
        min.x = p.x - 18.0 - w;
    }
    if min.y + h > screen.max.y - space::S4 - size::TIMELINE_H {
        min.y = p.y - 18.0 - h;
    }
    let rect = Rect::from_min_size(min, vec2(w, h));
    kit.glass(rect, radius::MD, 2);
    let mut y = rect.min.y + pad + 14.0;
    let end = row(
        kit,
        rect.min.x + pad,
        y,
        &[(value, Type::Value, t.text_primary, 0.0)],
    );
    kit.text(
        pos2(end + 5.0, y + 3.0),
        Align2::LEFT_CENTER,
        unit,
        Type::Body,
        t.text_secondary,
    );
    // Arrow toward the radar (Lucide navigation, rotated by painting a
    // triangle instead; the real component rotates the vector path).
    let to_radar = ((brg + 180.0) % 360.0).to_radians() as f32;
    let c = pos2(rect.max.x - pad - 8.0, y);
    let dir = vec2(to_radar.sin(), -to_radar.cos());
    let perp = vec2(-dir.y, dir.x);
    kit.painter.add(egui::Shape::convex_polygon(
        vec![
            c + dir * 7.0,
            c - dir * 5.0 + perp * 5.0,
            c - dir * 2.5,
            c - dir * 5.0 - perp * 5.0,
        ],
        t.text_secondary,
        Stroke::NONE,
    ));
    y += 18.0;
    if let Some(n) = note {
        kit.text(
            pos2(rect.min.x + pad, y + 4.0),
            Align2::LEFT_CENTER,
            n,
            Type::Caption,
            t.text_secondary,
        );
        y += 16.0;
    }
    y += space::S2;
    kit.painter.line_segment(
        [
            pos2(rect.min.x + pad, y - 4.0),
            pos2(rect.max.x - pad, y - 4.0),
        ],
        Stroke::new(1.0, t.glass_stroke),
    );
    for (k, v) in rows {
        y += 10.0;
        kit.text(
            pos2(rect.min.x + pad, y),
            Align2::LEFT_CENTER,
            k,
            Type::Caption,
            t.text_tertiary,
        );
        kit.text(
            pos2(rect.max.x - pad, y),
            Align2::RIGHT_CENTER,
            v,
            Type::CaptionTab,
            t.text_primary,
        );
        y += 10.0;
    }
}

/// Screen point → (lat, lon).
pub fn unproject(camera: &Camera, screen: Rect, p: Pos2) -> (f64, f64) {
    from_merc(camera.to_merc(
        [p.x as f64, p.y as f64],
        [screen.center().x as f64, screen.center().y as f64],
    ))
}
