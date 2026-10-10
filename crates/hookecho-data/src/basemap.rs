//! `basemap`: Census + Natural Earth → assets/basemap/basemap.bin (HEBM v1,
//! see hookecho_core::basemap). All inputs are public domain (PROVENANCE.md).

use crate::fetch;
use hookecho_core::basemap::{self, Basemap, Layer, Place};
use lyon::math::point;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers};
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::path::Path;

const CENSUS: &str = "https://www2.census.gov/geo/tiger";
const NE: &str = "https://naciscdn.org/naturalearth/10m";

struct Sources {
    states: String,
    counties: String,
    roads: String,
    places: String,
    land: String,
    lakes: String,
    countries: String,
    provinces: String,
}

fn sources() -> Sources {
    Sources {
        states: format!("{CENSUS}/GENZ2024/shp/cb_2024_us_state_500k.zip"),
        counties: format!("{CENSUS}/GENZ2024/shp/cb_2024_us_county_500k.zip"),
        roads: format!("{CENSUS}/TIGER2024/PRIMARYROADS/tl_2024_us_primaryroads.zip"),
        places: "https://www2.census.gov/geo/docs/maps-data/data/gazetteer/2024_Gazetteer/2024_Gaz_place_national.zip".into(),
        land: format!("{NE}/physical/ne_10m_land.zip"),
        lakes: format!("{NE}/physical/ne_10m_lakes.zip"),
        countries: format!("{NE}/cultural/ne_10m_admin_0_boundary_lines_land.zip"),
        provinces: format!("{NE}/cultural/ne_10m_admin_1_states_provinces_lines.zip"),
    }
}

/// Regions that contain radar coverage (lat_min, lat_max, lon_min, lon_max):
/// North America and the Caribbean, and the western Pacific sites (Guam,
/// South Korea, Okinawa). Natural Earth layers are clipped to these so the
/// rest of the world doesn't ride along in the snap.
const REGIONS: [(f64, f64, f64, f64); 2] = [(5.0, 75.0, -180.0, -50.0), (5.0, 45.0, 115.0, 150.0)];

/// Simplification tolerances in degrees (~1 m per 1e-5°).
const TOL_BOUNDARY: f64 = 0.0004;
const TOL_ROAD: f64 = 0.0003;
const TOL_LAND: f64 = 0.004;
const TOL_LAKE: f64 = 0.002;

type Ring = Vec<(f64, f64)>; // (lat, lon)

pub fn run(out: &Path, cache: &Path, baseline: Option<&Path>) -> Result<String, String> {
    let client = fetch::client()?;
    let src = sources();
    let get = |url: &str| fetch::cached(&client, url, cache);

    let states = polygons(&get(&src.states)?, None)?;
    let counties = polygons(&get(&src.counties)?, None)?;
    let roads = polylines(&get(&src.roads)?, None)?;
    let countries = clip_lines(&polylines(&get(&src.countries)?, None)?);
    let provinces = polylines(&get(&src.provinces)?, Some(("ADM0_A3", &["CAN", "MEX"])))?;
    let land = clip_polygons(&polygons(&get(&src.land)?, None)?);
    let lakes = clip_polygons(&polygons(&get(&src.lakes)?, None)?);

    let map = Basemap {
        layers: vec![
            fill("land", &land, TOL_LAND)?,
            fill("lakes", &lakes, TOL_LAKE)?,
            Layer::Lines {
                name: "county".into(),
                lines: arcs(&counties, TOL_BOUNDARY),
            },
            Layer::Lines {
                name: "road".into(),
                lines: simplify_all(&roads, TOL_ROAD),
            },
            Layer::Lines {
                name: "province".into(),
                lines: simplify_all(&provinces, TOL_BOUNDARY),
            },
            Layer::Lines {
                name: "country".into(),
                lines: simplify_all(&countries, TOL_BOUNDARY),
            },
            Layer::Lines {
                name: "state".into(),
                lines: arcs(&states, TOL_BOUNDARY),
            },
        ],
        places: places(&get(&src.places)?)?,
    };
    let bytes = basemap::encode(&map);
    // Encoding quantizes to 1e-5°, so check idempotence instead: what we
    // write must decode and re-encode to the identical bytes.
    let map = basemap::decode(&bytes).map_err(|e| e.to_string())?;
    if basemap::encode(&map) != bytes {
        return Err("encoded basemap doesn't round-trip".into());
    }
    if let Some(b) = baseline {
        check_baseline(&map, b)?;
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(out, &bytes).map_err(|e| e.to_string())?;
    Ok(format!(
        "wrote {} ({} KB): {}",
        out.display(),
        bytes.len() / 1024,
        summary(&map)
    ))
}

fn summary(map: &Basemap) -> String {
    let mut parts: Vec<String> = map
        .layers
        .iter()
        .map(|l| match l {
            Layer::Lines { name, lines } => {
                format!("{name} {} pts", lines.iter().map(Vec::len).sum::<usize>())
            }
            Layer::Fill { name, indices, .. } => format!("{name} {} tris", indices.len() / 3),
        })
        .collect();
    parts.push(format!("{} places", map.places.len()));
    parts.join(", ")
}

/// Refuses output that dropped a layer, lost >10% of a layer's geometry, or
/// lost places, compared with the previously bundled basemap.
fn check_baseline(new: &Basemap, path: &Path) -> Result<(), String> {
    let old =
        basemap::decode(&std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?)
            .map_err(|e| format!("baseline: {e}"))?;
    let size = |l: &Layer| match l {
        Layer::Lines { lines, .. } => lines.iter().map(Vec::len).sum::<usize>(),
        Layer::Fill { indices, .. } => indices.len(),
    };
    for o in &old.layers {
        let n = new
            .layers
            .iter()
            .find(|l| l.name() == o.name())
            .ok_or(format!("refusing: layer {} lost", o.name()))?;
        if (size(n) as f64) < size(o) as f64 * 0.9 {
            return Err(format!(
                "refusing: layer {} shrank from {} to {}",
                o.name(),
                size(o),
                size(n)
            ));
        }
    }
    let names: HashSet<(&str, &str)> = new
        .places
        .iter()
        .map(|p| (p.name.as_str(), p.state.as_str()))
        .collect();
    let lost = old
        .places
        .iter()
        .filter(|p| !names.contains(&(p.name.as_str(), p.state.as_str())))
        .count();
    if lost > old.places.len() / 100 {
        return Err(format!("refusing: {lost} places lost vs baseline"));
    }
    Ok(())
}

// ---------------------------------------------------------------- reading

/// Reads the `.shp` + `.dbf` pair out of a zip in memory.
fn shapes(zip_path: &Path) -> Result<Vec<(shapefile::Shape, shapefile::dbase::Record)>, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = |ext: &str| -> Result<Vec<u8>, String> {
        let name = names(&zip)
            .into_iter()
            .find(|n| n.to_ascii_lowercase().ends_with(ext))
            .ok_or(format!("{}: no {ext}", zip_path.display()))?;
        let mut buf = Vec::new();
        zip.by_name(&name)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        Ok(buf)
    };
    let (shp, dbf) = (entry(".shp")?, entry(".dbf")?);
    let shape_reader = shapefile::ShapeReader::new(Cursor::new(shp)).map_err(|e| e.to_string())?;
    let dbase_reader =
        shapefile::dbase::Reader::new(Cursor::new(dbf)).map_err(|e| e.to_string())?;
    shapefile::Reader::new(shape_reader, dbase_reader)
        .read()
        .map_err(|e| e.to_string())
}

fn names<R: Read + std::io::Seek>(zip: &zip::ZipArchive<R>) -> Vec<String> {
    zip.file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .collect()
}

fn keep(rec: &shapefile::dbase::Record, filter: Option<(&str, &[&str])>) -> bool {
    let Some((field, values)) = filter else {
        return true;
    };
    match rec.get(field) {
        Some(shapefile::dbase::FieldValue::Character(Some(v))) => values.contains(&v.trim()),
        _ => false,
    }
}

/// Polygons as lists of rings (outer and holes together; fills use even-odd).
fn polygons(zip: &Path, filter: Option<(&str, &[&str])>) -> Result<Vec<Vec<Ring>>, String> {
    Ok(shapes(zip)?
        .into_iter()
        .filter(|(_, r)| keep(r, filter))
        .filter_map(|(s, _)| match s {
            shapefile::Shape::Polygon(p) => Some(
                p.rings()
                    .iter()
                    .map(|r| r.points().iter().map(|pt| (pt.y, pt.x)).collect())
                    .collect(),
            ),
            _ => None,
        })
        .collect())
}

fn polylines(zip: &Path, filter: Option<(&str, &[&str])>) -> Result<Vec<Ring>, String> {
    let mut out = Vec::new();
    for (s, r) in shapes(zip)? {
        if !keep(&r, filter) {
            continue;
        }
        if let shapefile::Shape::Polyline(l) = s {
            out.extend(
                l.parts()
                    .iter()
                    .map(|part| part.iter().map(|pt| (pt.y, pt.x)).collect::<Ring>()),
            );
        }
    }
    Ok(out)
}

fn places(zip_path: &Path) -> Result<Vec<Place>, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let name = names(&zip)
        .into_iter()
        .find(|n| n.ends_with(".txt"))
        .ok_or("gazetteer: no .txt")?;
    let mut raw = Vec::new();
    zip.by_name(&name)
        .map_err(|e| e.to_string())?
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    // The gazetteer is Latin-1; map bytes straight to chars.
    let text: String = raw.iter().map(|&b| b as char).collect();
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or("gazetteer: empty")?
        .split('\t')
        .map(str::trim)
        .collect();
    let col = |n: &str| {
        header
            .iter()
            .position(|h| *h == n)
            .ok_or(format!("gazetteer: no {n} column"))
    };
    let (usps, nm, funcstat, aland, lat, lon) = (
        col("USPS")?,
        col("NAME")?,
        col("FUNCSTAT")?,
        col("ALAND")?,
        col("INTPTLAT")?,
        col("INTPTLONG")?,
    );
    let mut out = Vec::new();
    for l in lines {
        let f: Vec<&str> = l.split('\t').map(str::trim).collect();
        if f.len() < header.len() {
            continue;
        }
        // A = active incorporated place, S = census designated place.
        let cdp = match f[funcstat] {
            "A" => false,
            "S" => true,
            _ => continue,
        };
        let (Ok(la), Ok(lo), Ok(area)) = (
            f[lat].parse::<f64>(),
            f[lon].parse::<f64>(),
            f[aland].parse::<u64>(),
        ) else {
            continue;
        };
        let hectares = area / 10_000;
        out.push(Place {
            name: strip_lsad(f[nm]).to_string(),
            state: f[usps].to_string(),
            lat: la,
            lon: lo,
            // Unincorporated places rank below incorporated ones of the same size.
            rank: if cdp { hectares / 2 } else { hectares },
        });
    }
    if out.len() < 20_000 {
        return Err(format!(
            "gazetteer: only {} places parsed; refusing",
            out.len()
        ));
    }
    out.sort_by(|a, b| b.rank.cmp(&a.rank).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// "Moore city" → "Moore"; the Gazetteer appends the legal/statistical
/// area description (LSAD) to every name.
fn strip_lsad(name: &str) -> &str {
    const SUFFIXES: [&str; 12] = [
        " city and borough",
        " unified government (balance)",
        " consolidated government (balance)",
        " metropolitan government (balance)",
        " municipality",
        " comunidad",
        " zona urbana",
        " village",
        " borough",
        " city",
        " town",
        " CDP",
    ];
    SUFFIXES
        .iter()
        .find_map(|s| name.strip_suffix(s))
        .unwrap_or(name)
}

// ---------------------------------------------------------------- geometry

type Key = (i64, i64);

fn key(p: (f64, f64)) -> Key {
    ((p.0 * 1e5).round() as i64, (p.1 * 1e5).round() as i64)
}

/// Unique boundary arcs from polygons that share edges (counties, states):
/// each shared segment is kept once, then segments are chained into polylines
/// that break at junctions, and each chain is simplified with its endpoints
/// fixed so neighbors still meet.
fn arcs(polys: &[Vec<Ring>], tol: f64) -> Vec<Ring> {
    let mut segs: HashSet<(Key, Key)> = HashSet::new();
    for ring in polys.iter().flatten() {
        for w in ring.windows(2) {
            let (a, b) = (key(w[0]), key(w[1]));
            if a != b {
                segs.insert(if a < b { (a, b) } else { (b, a) });
            }
        }
    }
    let mut adj: HashMap<Key, Vec<Key>> = HashMap::new();
    for &(a, b) in &segs {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    }
    for v in adj.values_mut() {
        v.sort();
    }
    let mut used: HashSet<(Key, Key)> = HashSet::with_capacity(segs.len());
    let norm = |a: Key, b: Key| if a < b { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    let walk = |start: Key, next: Key, used: &mut HashSet<(Key, Key)>| {
        let mut chain = vec![start, next];
        used.insert(norm(start, next));
        let (mut prev, mut cur) = (start, next);
        while adj[&cur].len() == 2 {
            let n = if adj[&cur][0] == prev {
                adj[&cur][1]
            } else {
                adj[&cur][0]
            };
            if !used.insert(norm(cur, n)) {
                break;
            }
            chain.push(n);
            (prev, cur) = (cur, n);
        }
        chain
            .into_iter()
            .map(|(a, b)| (a as f64 / 1e5, b as f64 / 1e5))
            .collect::<Ring>()
    };
    // Chains between junctions/endpoints first, then leftover closed loops.
    let mut nodes: Vec<Key> = adj.keys().copied().collect();
    nodes.sort();
    for pass_junctions in [true, false] {
        for &n in &nodes {
            if pass_junctions == (adj[&n].len() == 2) {
                continue;
            }
            for &m in &adj[&n].clone() {
                if !used.contains(&norm(n, m)) {
                    let c = walk(n, m, &mut used);
                    out.push(simplify(&c, tol));
                }
            }
        }
    }
    out
}

fn inside(p: (f64, f64), r: (f64, f64, f64, f64)) -> bool {
    p.0 >= r.0 && p.0 <= r.1 && p.1 >= r.2 && p.1 <= r.3
}

/// Splits polylines at region exits, keeping only the parts inside a region.
fn clip_lines(lines: &[Ring]) -> Vec<Ring> {
    let mut out = Vec::new();
    for l in lines {
        for &r in &REGIONS {
            let mut run: Ring = Vec::new();
            for &p in l {
                if inside(p, r) {
                    run.push(p);
                } else if run.len() > 1 {
                    out.push(std::mem::take(&mut run));
                } else {
                    run.clear();
                }
            }
            if run.len() > 1 {
                out.push(run);
            }
        }
    }
    out
}

/// Sutherland–Hodgman clip of each ring against each (convex, axis-aligned)
/// region. Rings may gain edges along the region border; the fill hides them
/// because the border lies outside every radar's coverage.
fn clip_polygons(polys: &[Vec<Ring>]) -> Vec<Vec<Ring>> {
    let mut out = Vec::new();
    for poly in polys {
        for &r in &REGIONS {
            let rings: Vec<Ring> = poly
                .iter()
                .map(|ring| clip_ring(ring, r))
                .filter(|c| c.len() >= 4)
                .collect();
            if !rings.is_empty() {
                out.push(rings);
            }
        }
    }
    out
}

fn clip_ring(ring: &Ring, (lat0, lat1, lon0, lon1): (f64, f64, f64, f64)) -> Ring {
    type Edge = (
        fn((f64, f64), f64) -> bool,
        fn((f64, f64), (f64, f64), f64) -> (f64, f64),
        f64,
    );
    let lerp_lat = |a: (f64, f64), b: (f64, f64), lat: f64| {
        (lat, a.1 + (b.1 - a.1) * (lat - a.0) / (b.0 - a.0))
    };
    let lerp_lon = |a: (f64, f64), b: (f64, f64), lon: f64| {
        (a.0 + (b.0 - a.0) * (lon - a.1) / (b.1 - a.1), lon)
    };
    let edges: [Edge; 4] = [
        (|p, v| p.0 >= v, lerp_lat, lat0),
        (|p, v| p.0 <= v, lerp_lat, lat1),
        (|p, v| p.1 >= v, lerp_lon, lon0),
        (|p, v| p.1 <= v, lerp_lon, lon1),
    ];
    let mut pts: Ring = ring.clone();
    for (inside, cross, v) in edges {
        if pts.is_empty() {
            break;
        }
        let input = std::mem::take(&mut pts);
        for (i, &cur) in input.iter().enumerate() {
            let prev = input[(i + input.len() - 1) % input.len()];
            match (inside(cur, v), inside(prev, v)) {
                (true, true) => pts.push(cur),
                (true, false) => {
                    pts.push(cross(prev, cur, v));
                    pts.push(cur);
                }
                (false, true) => pts.push(cross(prev, cur, v)),
                (false, false) => {}
            }
        }
    }
    if let (Some(&f), Some(&l)) = (pts.first(), pts.last())
        && f != l
    {
        pts.push(f);
    }
    pts
}

fn simplify_all(lines: &[Ring], tol: f64) -> Vec<Ring> {
    lines
        .iter()
        .filter(|l| l.len() > 1)
        .map(|l| simplify(l, tol))
        .collect()
}

/// Douglas–Peucker, with longitude scaled by cos(latitude) so the tolerance
/// is roughly isotropic on the ground.
fn simplify(pts: &[(f64, f64)], tol: f64) -> Ring {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let c = pts[0].0.to_radians().cos().max(0.05);
    let xy = |p: (f64, f64)| (p.1 * c, p.0);
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (xy(pts[a]), xy(pts[b]));
        let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
        let len2 = dx * dx + dy * dy;
        let mut best = (0.0, 0usize);
        for (i, &pt) in pts.iter().enumerate().take(b).skip(a + 1) {
            let p = xy(pt);
            let d = if len2 == 0.0 {
                ((p.0 - pa.0).powi(2) + (p.1 - pa.1).powi(2)).sqrt()
            } else {
                ((p.0 - pa.0) * dy - (p.1 - pa.1) * dx).abs() / len2.sqrt()
            };
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    pts.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// Simplified polygons tessellated into triangles with lyon (even-odd fill,
/// so holes in the rings stay holes).
fn fill(name: &str, polys: &[Vec<Ring>], tol: f64) -> Result<Layer, String> {
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    for poly in polys {
        let mut b = LyonPath::builder();
        let mut any = false;
        for ring in poly {
            let r = simplify(ring, tol);
            if r.len() < 4 {
                continue;
            }
            // x = lon, y = lat; lyon only cares about topology here.
            b.begin(point(r[0].1 as f32, r[0].0 as f32));
            for p in &r[1..] {
                b.line_to(point(p.1 as f32, p.0 as f32));
            }
            b.end(true);
            any = true;
        }
        if !any {
            continue;
        }
        let path = b.build();
        tess.tessellate_path(
            &path,
            &FillOptions::even_odd().with_tolerance(0.0001),
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()),
        )
        .map_err(|e| format!("{name}: tessellation failed: {e:?}"))?;
    }
    Ok(Layer::Fill {
        name: name.into(),
        vertices: buffers
            .vertices
            .iter()
            .map(|v| (v[1] as f64, v[0] as f64))
            .collect(),
        indices: buffers.indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsad_suffixes_are_stripped() {
        assert_eq!(strip_lsad("Moore city"), "Moore");
        assert_eq!(strip_lsad("Bethel Acres town"), "Bethel Acres");
        assert_eq!(strip_lsad("Juneau city and borough"), "Juneau");
        assert_eq!(strip_lsad("Abbeville CDP"), "Abbeville");
    }

    #[test]
    fn shared_edges_become_one_arc() {
        // Two unit squares sharing the edge x=1.
        let sq = |x: f64| vec![(0.0, x), (0.0, x + 1.0), (1.0, x + 1.0), (1.0, x), (0.0, x)];
        let polys = vec![vec![sq(0.0)], vec![sq(1.0)]];
        let a = arcs(&polys, 0.0);
        let segments: usize = a.iter().map(|l| l.len() - 1).sum();
        assert_eq!(segments, 7, "8 square edges, one shared");
    }

    #[test]
    fn simplify_keeps_endpoints_and_drops_collinear() {
        let line = vec![(0.0, 0.0), (0.0, 0.5), (0.0, 1.0), (0.5, 2.0)];
        let s = simplify(&line, 0.01);
        assert_eq!(s, vec![(0.0, 0.0), (0.0, 1.0), (0.5, 2.0)]);
    }
}
