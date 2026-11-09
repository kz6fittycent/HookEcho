//! SPC layers on the map (hookecho-core::spc): the chosen convective
//! outlook as translucent risk areas over the radar, watch outlines, and
//! dashed mesoscale discussion outlines. A click on any of them opens the
//! same card as a warning (warnings.rs).

use crate::design::components::Kit;
use crate::design::hazards;
use crate::model::Model;
use eframe::egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, epaint, pos2};
use hookecho_core::geo::to_mercator;
use hookecho_core::spc::{Discussion, Outlook, Product, Watch, ring_contains};
use lyon::path::Path;
use lyon::tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex};

/// One risk level's fill, tessellated once in normalized mercator.
struct AreaMesh {
    fill: Color32,
    stroke: Color32,
    vertices: Vec<[f32; 2]>,
    indices: Vec<u32>,
    /// Ring outlines, mercator.
    outlines: Vec<Vec<[f64; 2]>>,
}

/// The tessellated outlook, rebuilt when the outlook itself changes.
#[derive(Default)]
pub struct OutlookCache {
    key: Option<(Product, i64, usize)>,
    areas: Vec<AreaMesh>,
}

fn tessellate(o: &Outlook) -> Vec<AreaMesh> {
    let mut tess = FillTessellator::new();
    o.areas
        .iter()
        .map(|a| {
            let mut b = Path::builder();
            let mut outlines = Vec::new();
            for ring in a.polygons.iter().flatten() {
                let m: Vec<[f64; 2]> = ring.iter().map(|&(la, lo)| to_mercator(la, lo)).collect();
                b.begin(lyon::math::point(m[0][0] as f32, m[0][1] as f32));
                for p in &m[1..] {
                    b.line_to(lyon::math::point(p[0] as f32, p[1] as f32));
                }
                b.close();
                outlines.push(m);
            }
            let path = b.build();
            let mut buf: lyon::tessellation::VertexBuffers<[f32; 2], u32> = Default::default();
            // Mercator units are tiny (CONUS is ~0.1 across): tolerance to match.
            let opts = FillOptions::tolerance(1e-7).with_fill_rule(FillRule::EvenOdd);
            let ok = tess
                .tessellate_path(
                    &path,
                    &opts,
                    &mut BuffersBuilder::new(&mut buf, |v: FillVertex| v.position().to_array()),
                )
                .is_ok();
            AreaMesh {
                fill: hazards::outlook_fill(a.fill),
                stroke: hazards::outlook_stroke(a.stroke),
                vertices: if ok { buf.vertices } else { Vec::new() },
                indices: if ok { buf.indices } else { Vec::new() },
                outlines,
            }
        })
        .collect()
}

/// The outlook shown, if any.
pub fn shown_outlook(model: &Model) -> Option<&Outlook> {
    let p = Product::from_key(model.config.outlook.as_deref()?)?;
    model.layers.spc.outlook(p)
}

/// Draws the chosen outlook: fills, then outlines, lowest risk first.
pub fn draw_outlook(model: &Model, painter: &Painter, view: [f64; 2]) {
    let Some(o) = shown_outlook(model) else {
        return;
    };
    let mut cache = model.layers.outlook_mesh.borrow_mut();
    let key = (o.product, o.issued_ms, o.areas.len());
    if cache.key != Some(key) {
        cache.areas = tessellate(o);
        cache.key = Some(key);
    }
    let cam = model.camera;
    let screen = |m: [f64; 2]| {
        let p = cam.to_screen(m, view);
        pos2(p[0] as f32, p[1] as f32)
    };
    for a in &cache.areas {
        let mesh = Mesh {
            vertices: a
                .vertices
                .iter()
                .map(|v| epaint::Vertex {
                    pos: screen([v[0] as f64, v[1] as f64]),
                    uv: epaint::WHITE_UV,
                    color: a.fill,
                })
                .collect(),
            indices: a.indices.clone(),
            ..Default::default()
        };
        painter.add(Shape::mesh(mesh));
        for ring in &a.outlines {
            let pts: Vec<Pos2> = ring.iter().map(|&m| screen(m)).collect();
            painter.add(Shape::closed_line(
                pts,
                Stroke::new(hazards::OUTLOOK_STROKE, a.stroke),
            ));
        }
    }
}

pub fn visible_watches(model: &Model, now_ms: i64) -> impl Iterator<Item = &Watch> {
    let on = model.config.watches;
    model
        .layers
        .spc
        .watches
        .iter()
        .filter(move |w| on && w.expires_ms > now_ms)
}

pub fn visible_discussions(model: &Model, now_ms: i64) -> impl Iterator<Item = &Discussion> {
    let on = model.config.discussions;
    model
        .layers
        .spc
        .discussions
        .iter()
        .filter(move |d| on && d.expires_ms > now_ms)
}

fn screen_ring(model: &Model, ring: &[(f64, f64)], view: [f64; 2]) -> Vec<Pos2> {
    ring.iter()
        .map(|&(lat, lon)| {
            let p = model.camera.latlon_to_screen(lat, lon, view);
            pos2(p[0] as f32, p[1] as f32)
        })
        .collect()
}

/// Watch and MD outlines, under the warnings: MDs dashed, then watches.
pub fn draw_outlines(kit: &Kit, model: &Model, painter: &Painter, view: [f64; 2]) {
    let now = crate::warnings::now_ms();
    let on_screen = |pts: &[Pos2]| Rect::from_points(pts).intersects(kit.screen);
    for d in visible_discussions(model, now) {
        for ring in &d.rings {
            let mut pts = screen_ring(model, ring, view);
            if !on_screen(&pts) {
                continue;
            }
            if pts.first() != pts.last()
                && let Some(&p) = pts.first()
            {
                pts.push(p);
            }
            let w = hazards::MD_STROKE;
            painter.add(Shape::line(
                pts.clone(),
                Stroke::new(w + hazards::HALO, hazards::HALO_COLOR),
            ));
            painter.extend(Shape::dashed_line(
                &pts,
                Stroke::new(w, hazards::MD_COLOR),
                hazards::MD_DASH,
                hazards::MD_GAP,
            ));
        }
    }
    for w in visible_watches(model, now) {
        let color = hazards::watch_color(w.kind);
        let width = hazards::watch_stroke(w.pds);
        for ring in &w.rings {
            let pts = screen_ring(model, ring, view);
            if !on_screen(&pts) {
                continue;
            }
            painter.add(Shape::closed_line(
                pts.clone(),
                Stroke::new(width + hazards::HALO, hazards::HALO_COLOR),
            ));
            painter.add(Shape::closed_line(pts, Stroke::new(width, color)));
        }
    }
}

/// Card ids for what's at (lat, lon): watches ("ww:501"), MDs ("md:1691"),
/// then the outlook's top risk there ("otlk").
pub fn hits(model: &Model, at: (f64, f64)) -> Vec<String> {
    let now = crate::warnings::now_ms();
    let inside = |rings: &[Vec<(f64, f64)>]| rings.iter().any(|r| ring_contains(r, at.0, at.1));
    let mut ids: Vec<String> = visible_watches(model, now)
        .filter(|w| inside(&w.rings))
        .map(|w| format!("ww:{}", w.number))
        .collect();
    ids.extend(
        visible_discussions(model, now)
            .filter(|d| inside(&d.rings))
            .map(|d| format!("md:{}", d.number)),
    );
    if shown_outlook(model).is_some_and(|o| o.at(at.0, at.1).is_some()) {
        ids.push(format!("otlk:{:.4},{:.4}", at.0, at.1));
    }
    ids
}

/// The recorded SPC files in tests/data/spc (tornado watch 501 over Upper
/// Michigan and Wisconsin, MD 1691 over the Mid-Atlantic, a day 2
/// categorical outlook shown as day 1), kept in force for an hour from now,
/// for screenshots and tests. `None` outside a source checkout.
pub fn sample() -> Option<hookecho_core::spc::Spc> {
    use hookecho_core::spc::{self, WatchKind};
    let read = |name: &str| {
        std::fs::read(format!(
            "{}/../../tests/data/spc/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .ok()
    };
    let kml = |name: &str| -> Option<String> {
        String::from_utf8(spc::unzip_first(&read(name)?).ok()?).ok()
    };
    let text = |name: &str| spc::page_text(&String::from_utf8_lossy(&read(name)?)).into();
    let until = crate::warnings::now_ms() + 3_600_000;
    let mut outlook = spc::parse_outlook(
        Product::Day1Categorical,
        &read("day2otlk_cat-2026-10-29.geojson")?,
    )
    .ok()?;
    outlook.valid_ms = crate::warnings::now_ms() - 3_600_000;
    outlook.expires_ms = until;
    let md_text: Option<String> = text("md1691.html");
    let ww_text: Option<String> = text("ww0501.html");
    Some(spc::Spc {
        outlooks: vec![outlook],
        watches: vec![Watch {
            number: 501,
            kind: WatchKind::Tornado,
            pds: false,
            expires_ms: until,
            rings: spc::kml_rings(&kml("WW0501_SAW.kmz")?),
            text: ww_text?,
        }],
        discussions: vec![Discussion {
            number: 1691,
            areas: "portions of the Mid-Atlantic".into(),
            concerning: "Severe potential...Watch likely".into(),
            expires_ms: until,
            rings: spc::kml_rings(&kml("MD1691.kmz")?),
            text: md_text?,
        }],
    })
}
