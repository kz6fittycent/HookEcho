//! Storm Prediction Center products (KICKOFF §3): convective outlooks,
//! watches and mesoscale discussions, all from spc.noaa.gov's public feeds.
//!
//! - **Outlooks:** GeoJSON, one file per day and kind
//!   (`products/outlook/day1otlk_cat.nolyr.geojson`). Each feature is one
//!   risk level with SPC's own label, stroke and fill colors; "nolyr" means
//!   each level is its whole area, so drawing them in order stacks them.
//! - **Watches and MDs:** `products/watch/ActiveWW.kmz` and
//!   `products/md/ActiveMD.kmz` list the active ones as KML network links;
//!   each link is a small KMZ with the polygon, and the product's HTML page
//!   (`ww0501.html`, `md1691.html`) has the full text in its first `<pre>`.
//!
//! KMZ is a zip of one KML file (KML 2.2, OGC 07-147r2; zip, PKWARE
//! APPNOTE). Both are read here with just what these files use: stored or
//! deflated entries, and `Placemark`/`Polygon`/`coordinates`.

use crate::poll::{Http, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

pub const BASE: &str = "https://www.spc.noaa.gov/products";

/// The outlooks offered, in the order of the Layers list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Product {
    Day1Categorical,
    Day1Tornado,
    Day1Wind,
    Day1Hail,
    Day2Categorical,
    Day2Tornado,
    Day2Wind,
    Day2Hail,
    Day3Categorical,
    Day3Severe,
}

impl Product {
    pub const ALL: [Product; 10] = [
        Product::Day1Categorical,
        Product::Day1Tornado,
        Product::Day1Wind,
        Product::Day1Hail,
        Product::Day2Categorical,
        Product::Day2Tornado,
        Product::Day2Wind,
        Product::Day2Hail,
        Product::Day3Categorical,
        Product::Day3Severe,
    ];

    pub fn day(self) -> u8 {
        use Product::*;
        match self {
            Day1Categorical | Day1Tornado | Day1Wind | Day1Hail => 1,
            Day2Categorical | Day2Tornado | Day2Wind | Day2Hail => 2,
            Day3Categorical | Day3Severe => 3,
        }
    }

    /// The file's kind: `cat`, `torn`, `wind`, `hail`, or `prob` (day 3's
    /// any-severe probability).
    fn kind(self) -> &'static str {
        use Product::*;
        match self {
            Day1Categorical | Day2Categorical | Day3Categorical => "cat",
            Day1Tornado | Day2Tornado => "torn",
            Day1Wind | Day2Wind => "wind",
            Day1Hail | Day2Hail => "hail",
            Day3Severe => "prob",
        }
    }

    pub fn url(self) -> String {
        format!(
            "{BASE}/outlook/day{}otlk_{}.nolyr.geojson",
            self.day(),
            self.kind()
        )
    }

    /// "Categorical", "Tornado", ..., for "Day 1 · Tornado".
    pub fn name(self) -> &'static str {
        match self.kind() {
            "cat" => "Categorical",
            "torn" => "Tornado",
            "wind" => "Wind",
            "hail" => "Hail",
            _ => "Any severe",
        }
    }

    /// A stable key for the config file.
    pub fn key(self) -> String {
        format!("day{}-{}", self.day(), self.kind())
    }

    pub fn from_key(key: &str) -> Option<Product> {
        Product::ALL.into_iter().find(|p| p.key() == key)
    }
}

/// One risk level of an outlook: "MRGL" / "Marginal Risk", or "0.05" /
/// "5% Hail Risk", with SPC's colors for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub label: String,
    pub name: String,
    pub stroke: [u8; 3],
    pub fill: [u8; 3],
    /// Polygons, each an outer ring then any holes, as (lat, lon).
    pub polygons: Vec<Vec<Vec<(f64, f64)>>>,
}

impl Area {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        self.polygons.iter().any(|p| {
            p.iter()
                .filter(|ring| ring_contains(ring, lat, lon))
                .count()
                % 2
                == 1
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outlook {
    pub product: Product,
    pub issued_ms: i64,
    pub valid_ms: i64,
    pub expires_ms: i64,
    pub forecaster: String,
    /// Lowest risk first, the drawing order. Empty when SPC has none
    /// ("Less Than 2% All Areas").
    pub areas: Vec<Area>,
}

impl Outlook {
    /// The highest risk level at (lat, lon).
    pub fn at(&self, lat: f64, lon: f64) -> Option<&Area> {
        self.areas.iter().rev().find(|a| a.contains(lat, lon))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatchKind {
    Tornado,
    SevereThunderstorm,
}

impl WatchKind {
    pub fn name(self) -> &'static str {
        match self {
            WatchKind::Tornado => "Tornado Watch",
            WatchKind::SevereThunderstorm => "Severe Thunderstorm Watch",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watch {
    pub number: u32,
    pub kind: WatchKind,
    /// "Particularly dangerous situation" watches.
    pub pds: bool,
    pub expires_ms: i64,
    /// SPC's outline ("an approximation to the watch area"), as (lat, lon).
    pub rings: Vec<Vec<(f64, f64)>>,
    /// The watch text, empty until it has been fetched.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Discussion {
    pub number: u32,
    /// "portions of the Mid-Atlantic".
    pub areas: String,
    /// "Severe potential...Watch likely".
    pub concerning: String,
    pub expires_ms: i64,
    pub rings: Vec<Vec<(f64, f64)>>,
    pub text: String,
}

/// Everything SPC has right now.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spc {
    pub outlooks: Vec<Outlook>,
    /// Tornado watches first, then by number.
    pub watches: Vec<Watch>,
    pub discussions: Vec<Discussion>,
}

impl Spc {
    pub fn outlook(&self, p: Product) -> Option<&Outlook> {
        self.outlooks.iter().find(|o| o.product == p)
    }
}

/// Even-odd point in ring, (lat, lon).
pub fn ring_contains(ring: &[(f64, f64)], lat: f64, lon: f64) -> bool {
    let mut inside = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (yi, xi) = ring[i];
        let (yj, xj) = ring[j];
        if (yi > lat) != (yj > lat) && lon < (xj - xi) * (lat - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// ---------------------------------------------------------------- outlooks

fn hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Parses one outlook file. Features without geometry (SPC's "Less Than 2%
/// All Areas") leave the outlook with no areas.
pub fn parse_outlook(product: Product, json: &[u8]) -> Result<Outlook, String> {
    let v: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let features = v["features"]
        .as_array()
        .ok_or_else(|| "no features array".to_string())?;
    let time = |p: &Value, k: &str| p[k].as_str().and_then(crate::alerts::parse_time);
    let first = features.first().map(|f| &f["properties"]);
    let mut out = Outlook {
        product,
        issued_ms: first.and_then(|p| time(p, "ISSUE_ISO")).unwrap_or(0),
        valid_ms: first.and_then(|p| time(p, "VALID_ISO")).unwrap_or(0),
        expires_ms: first.and_then(|p| time(p, "EXPIRE_ISO")).unwrap_or(0),
        forecaster: first
            .and_then(|p| p["FORECASTER"].as_str())
            .unwrap_or_default()
            .to_string(),
        areas: Vec::new(),
    };
    for f in features {
        let p = &f["properties"];
        let polygons = polygons(&f["geometry"]);
        if polygons.is_empty() {
            continue;
        }
        let (Some(stroke), Some(fill)) = (
            p["stroke"].as_str().and_then(hex_color),
            p["fill"].as_str().and_then(hex_color),
        ) else {
            continue;
        };
        out.areas.push(Area {
            label: p["LABEL"].as_str().unwrap_or_default().to_string(),
            name: p["LABEL2"].as_str().unwrap_or_default().to_string(),
            stroke,
            fill,
            polygons,
        });
    }
    Ok(out)
}

/// A GeoJSON Polygon or MultiPolygon as polygons of rings (outer first),
/// (lat, lon).
fn polygons(g: &Value) -> Vec<Vec<Vec<(f64, f64)>>> {
    let ring = |r: &Value| -> Option<Vec<(f64, f64)>> {
        let pts: Vec<(f64, f64)> = r
            .as_array()?
            .iter()
            .filter_map(|c| Some((c[1].as_f64()?, c[0].as_f64()?)))
            .collect();
        (pts.len() >= 3).then_some(pts)
    };
    let polygon = |p: &Value| -> Option<Vec<Vec<(f64, f64)>>> {
        let rings: Vec<_> = p.as_array()?.iter().filter_map(ring).collect();
        (!rings.is_empty()).then_some(rings)
    };
    let coords = &g["coordinates"];
    match g["type"].as_str() {
        Some("Polygon") => polygon(coords).into_iter().collect(),
        Some("MultiPolygon") => coords
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(polygon)
            .collect(),
        _ => Vec::new(),
    }
}

// --------------------------------------------------------------- KMZ / KML

/// The first file in a zip archive (a KMZ holds one KML), stored or
/// deflated. Reads local headers only, which these files always have
/// sizes in.
pub fn unzip_first(zip: &[u8]) -> Result<Vec<u8>, String> {
    let u16le = |o: usize| zip.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32le = |o: usize| {
        zip.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if u32le(0) != Some(0x0403_4b50) {
        return Err("not a zip file".into());
    }
    let bad = || "truncated zip file".to_string();
    let flags = u16le(6).ok_or_else(bad)?;
    let method = u16le(8).ok_or_else(bad)?;
    let size = u32le(18).ok_or_else(bad)? as usize;
    let name_len = u16le(26).ok_or_else(bad)? as usize;
    let extra_len = u16le(28).ok_or_else(bad)? as usize;
    if flags & 0x08 != 0 && size == 0 {
        return Err("zip entry without a size in its header".into());
    }
    let start = 30 + name_len + extra_len;
    let data = zip.get(start..start + size).ok_or_else(bad)?;
    match method {
        0 => Ok(data.to_vec()),
        8 => {
            let mut out = Vec::new();
            flate2::read::DeflateDecoder::new(data)
                .take(16 << 20)
                .read_to_end(&mut out)
                .map_err(|e| e.to_string())?;
            Ok(out)
        }
        m => Err(format!("zip compression method {m}")),
    }
}

/// The text of each `<tag>…</tag>` in `s`, in order (no nesting of the
/// same tag, which KML's `name`/`coordinates`/`href` never have).
fn elements<'a>(s: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        let Some(j) = after.find(&close) else { break };
        out.push(after[..j].trim());
        rest = &after[j + close.len()..];
    }
    out
}

fn element<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    elements(s, tag).into_iter().next()
}

fn strip_cdata(s: &str) -> &str {
    s.trim()
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
        .unwrap_or(s)
        .trim()
}

/// A network link in an Active list: its name ("Tornado Watch 501 Active
/// Till 20260721/0400UTC") and the KMZ it points at.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub name: String,
    pub href: String,
}

/// The network links in `ActiveWW.kml` / `ActiveMD.kml`. None when nothing
/// is active.
pub fn parse_active(kml: &str) -> Vec<Link> {
    elements(kml, "NetworkLink")
        .into_iter()
        .filter_map(|l| {
            Some(Link {
                name: element(l, "name")?.to_string(),
                // SPC still writes some links as http://.
                href: element(l, "href")?.replacen("http://", "https://", 1),
            })
        })
        .collect()
}

/// The polygons in a KML file, as (lat, lon); KML is "lon,lat[,alt]".
pub fn kml_rings(kml: &str) -> Vec<Vec<(f64, f64)>> {
    elements(kml, "outerBoundaryIs")
        .into_iter()
        .filter_map(|b| element(b, "coordinates"))
        .map(|c| {
            c.split_whitespace()
                .filter_map(|t| {
                    let mut it = t.split(',').map(str::parse::<f64>);
                    let lon = it.next()?.ok()?;
                    let lat = it.next()?.ok()?;
                    Some((lat, lon))
                })
                .collect::<Vec<_>>()
        })
        .filter(|r| r.len() >= 3)
        .collect()
}

/// "Tornado Watch 501 Active Till 20260721/0400UTC".
fn parse_watch_link(name: &str) -> Option<(WatchKind, u32, i64)> {
    let (kind, rest) = if let Some(r) = name.strip_prefix("Tornado Watch ") {
        (WatchKind::Tornado, r)
    } else {
        (
            WatchKind::SevereThunderstorm,
            name.strip_prefix("Severe Thunderstorm Watch ")?,
        )
    };
    let (num, rest) = rest.split_once(' ')?;
    let till = rest.strip_prefix("Active Till ")?.trim_end_matches("UTC");
    let (ymd, hm) = till.trim().split_once('/')?;
    let n = |s: &str, r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let ms = utc_ms(
        n(ymd, 0..4)?,
        n(ymd, 4..6)?,
        n(ymd, 6..8)?,
        n(hm, 0..2)?,
        n(hm, 2..4)?,
    )?;
    Some((kind, num.parse().ok()?, ms))
}

/// "MD 1691 Active Till 1930 UTC": the number.
fn parse_md_link(name: &str) -> Option<u32> {
    name.strip_prefix("MD ")?.split(' ').next()?.parse().ok()
}

fn utc_ms(y: i64, mo: i64, d: i64, h: i64, mi: i64) -> Option<i64> {
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 24 || mi > 59 {
        return None;
    }
    Some((crate::tdwr::days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60) * 1000)
}

/// A "DDHHMMZ" time as SPC writes them ("Valid 211753Z - 211930Z"): the
/// instant with that day of month nearest to `near_ms`.
fn ddhhmm_near(s: &str, near_ms: i64) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (d, h, mi) = (n(0..2)?, n(2..4)?, n(4..6)?);
    let (y, mo) = crate::tdwr::civil_from_days(near_ms.div_euclid(86_400_000));
    [(y, mo - 1), (y, mo), (y, mo + 1)]
        .into_iter()
        .map(|(y, m)| match m {
            0 => (y - 1, 12),
            13 => (y + 1, 1),
            m => (y, m),
        })
        .filter_map(|(y, m)| utc_ms(y, m, d, h, mi))
        .min_by_key(|t| (t - near_ms).abs())
}

/// Areas, concern and end time from an MD's KML description.
fn parse_md_description(kml: &str, now_ms: i64) -> (String, String, Option<i64>) {
    let desc = element(kml, "description").map(strip_cdata).unwrap_or("");
    let line = |key: &str| {
        desc.split("<br />")
            .map(str::trim)
            .find_map(|l| l.strip_prefix(key))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let valid = line("Valid");
    let ends = valid
        .split_once('-')
        .and_then(|(_, e)| ddhhmm_near(e.trim(), now_ms));
    (line("Areas affected..."), line("Concerning..."), ends)
}

/// The product text in an SPC HTML page: the first `<pre>`, without its
/// tags and entities.
pub fn page_text(html: &str) -> String {
    // Untrimmed, so the first line keeps its indent for the dedent below.
    let Some(pre) = html
        .split_once("<pre>")
        .and_then(|(_, r)| r.split_once("</pre>"))
        .map(|(p, _)| p)
    else {
        return String::new();
    };
    let mut out = String::with_capacity(pre.len());
    let mut in_tag = false;
    for c in pre.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    // Dedent: SPC indents every line by three spaces.
    let indent = out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    out.lines()
        .map(|l| l.get(indent..).unwrap_or(l.trim_start()).trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------- the feed

/// Polls SPC: the active watch and MD lists every minute, the outlooks
/// every ten. Each watch's or MD's text is fetched once.
pub struct SpcSource {
    now: fn() -> i64,
    data: Spc,
    outlooks_due_ms: i64,
    /// Product page text by page URL.
    texts: HashMap<String, String>,
}

const OUTLOOK_EVERY_MS: i64 = 10 * 60_000;

impl SpcSource {
    pub fn new() -> SpcSource {
        SpcSource {
            now: crate::time::now_ms,
            data: Spc::default(),
            outlooks_due_ms: 0,
            texts: HashMap::new(),
        }
    }

    async fn text(&mut self, http: &mut Http, url: String) -> String {
        if let Some(t) = self.texts.get(&url) {
            return t.clone();
        }
        // The text is a nicety; the polygon stands without it.
        match http.get(&url, "text/html").await {
            Ok(g) => {
                let t = page_text(&String::from_utf8_lossy(&g.body));
                self.texts.insert(url, t.clone());
                t
            }
            Err(_) => String::new(),
        }
    }

    async fn watches(&mut self, http: &mut Http) -> Result<Vec<Watch>, String> {
        let list = http
            .get(&format!("{BASE}/watch/ActiveWW.kmz"), "*/*")
            .await?;
        let kml = String::from_utf8_lossy(&unzip_first(&list.body)?).into_owned();
        let mut out = Vec::new();
        for link in parse_active(&kml) {
            let Some((kind, number, expires_ms)) = parse_watch_link(&link.name) else {
                continue;
            };
            let Ok(g) = http.get(&link.href, "*/*").await else {
                continue;
            };
            let Ok(kml) = unzip_first(&g.body) else {
                continue;
            };
            let rings = kml_rings(&String::from_utf8_lossy(&kml));
            if rings.is_empty() {
                continue;
            }
            let text = self
                .text(http, format!("{BASE}/watch/ww{number:04}.html"))
                .await;
            out.push(Watch {
                number,
                kind,
                pds: text.contains("PARTICULARLY DANGEROUS SITUATION"),
                expires_ms,
                rings,
                text,
            });
        }
        out.sort_by_key(|w| (w.kind != WatchKind::Tornado, w.number));
        Ok(out)
    }

    async fn discussions(&mut self, http: &mut Http) -> Result<Vec<Discussion>, String> {
        let list = http.get(&format!("{BASE}/md/ActiveMD.kmz"), "*/*").await?;
        let kml = String::from_utf8_lossy(&unzip_first(&list.body)?).into_owned();
        let now = (self.now)();
        let mut out = Vec::new();
        for link in parse_active(&kml) {
            let Some(number) = parse_md_link(&link.name) else {
                continue;
            };
            let Ok(g) = http.get(&link.href, "*/*").await else {
                continue;
            };
            let Ok(kml) = unzip_first(&g.body) else {
                continue;
            };
            let kml = String::from_utf8_lossy(&kml);
            let rings = kml_rings(&kml);
            if rings.is_empty() {
                continue;
            }
            let (areas, concerning, ends) = parse_md_description(&kml, now);
            let text = self
                .text(http, format!("{BASE}/md/md{number:04}.html"))
                .await;
            out.push(Discussion {
                number,
                areas,
                concerning,
                // An MD in the active list is in force; without a readable
                // end time, give it the usual two hours.
                expires_ms: ends.unwrap_or(now + 2 * 3_600_000),
                rings,
                text,
            });
        }
        out.sort_by_key(|d| std::cmp::Reverse(d.number));
        Ok(out)
    }
}

impl Default for SpcSource {
    fn default() -> Self {
        SpcSource::new()
    }
}

impl Source for SpcSource {
    type Data = Spc;
    const NAME: &'static str = "spc";

    fn every(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&mut self, http: &mut Http) -> Result<Option<Spc>, String> {
        let mut next = self.data.clone();
        let mut trouble = Vec::new();
        match self.watches(http).await {
            Ok(w) => next.watches = w,
            Err(e) => trouble.push(format!("watches: {e}")),
        }
        match self.discussions(http).await {
            Ok(d) => next.discussions = d,
            Err(e) => trouble.push(format!("discussions: {e}")),
        }
        let now = (self.now)();
        if now >= self.outlooks_due_ms {
            let mut outlooks = Vec::new();
            for p in Product::ALL {
                match http.get(&p.url(), "application/geo+json").await {
                    Ok(g) => match parse_outlook(p, &g.body) {
                        Ok(o) => outlooks.push(o),
                        Err(e) => trouble.push(format!("{}: {e}", p.key())),
                    },
                    Err(e) => trouble.push(format!("{}: {e}", p.key())),
                }
            }
            // Keep the last good copy of any outlook that failed.
            for o in &self.data.outlooks {
                if !outlooks.iter().any(|n| n.product == o.product) {
                    outlooks.push(o.clone());
                }
            }
            next.outlooks = outlooks;
            self.outlooks_due_ms = now + OUTLOOK_EVERY_MS;
        }
        // Forget the text of products no longer active.
        let pages: Vec<String> = next
            .watches
            .iter()
            .map(|w| format!("{BASE}/watch/ww{:04}.html", w.number))
            .chain(
                next.discussions
                    .iter()
                    .map(|d| format!("{BASE}/md/md{:04}.html", d.number)),
            )
            .collect();
        self.texts.retain(|k, _| pages.contains(k));
        http.retain(|u| {
            !(u.contains("/watch/WW") || u.contains("/md/MD"))
                || next
                    .watches
                    .iter()
                    .any(|w| u.contains(&format!("WW{:04}", w.number)))
                || next
                    .discussions
                    .iter()
                    .any(|d| u.contains(&format!("MD{:04}", d.number)))
                || pages.iter().any(|p| p == u)
        });
        if !trouble.is_empty() && next == self.data {
            return Err(trouble.join("; "));
        }
        if next == self.data {
            return Ok(None);
        }
        self.data = next.clone();
        Ok(Some(next))
    }

    fn save(&self, data: &Spc) -> Option<Vec<u8>> {
        serde_json::to_vec(data).ok()
    }

    fn load(&self, bytes: &[u8]) -> Option<Spc> {
        serde_json::from_slice(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../tests/data/spc/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn kmz(name: &str) -> String {
        String::from_utf8(unzip_first(&data(name)).unwrap()).unwrap()
    }

    /// A real day 2 categorical outlook (issued 2026-10-29 05:16 UTC):
    /// general thunderstorms over much of the east, a marginal risk inside.
    #[test]
    fn categorical_outlook() {
        let o = parse_outlook(
            Product::Day2Categorical,
            &data("day2otlk_cat-2026-10-29.geojson"),
        )
        .unwrap();
        assert_eq!(o.forecaster, "Leitman");
        assert_eq!(
            o.valid_ms,
            crate::alerts::parse_time("2026-10-30T12:00:00Z").unwrap()
        );
        assert_eq!(o.areas.len(), 2);
        assert_eq!(o.areas[0].label, "TSTM");
        assert_eq!(o.areas[0].stroke, [0x55, 0xBB, 0x55]);
        assert_eq!(o.areas[0].polygons.len(), 2);
        assert_eq!(o.areas[1].name, "Marginal Risk");
        // The marginal area's own vertex average is inside it, and then
        // the top risk there is marginal; the Pacific has nothing.
        let r = &o.areas[1].polygons[0][0];
        let n = r.len() as f64;
        let c = r
            .iter()
            .fold((0.0, 0.0), |s, p| (s.0 + p.0 / n, s.1 + p.1 / n));
        assert_eq!(o.at(c.0, c.1).map(|a| a.label.as_str()), Some("MRGL"));
        assert!(o.at(35.0, -140.0).is_none());
    }

    #[test]
    fn empty_and_probabilistic_outlooks() {
        let t = parse_outlook(
            Product::Day1Tornado,
            &data("day1otlk_torn-2026-10-29.geojson"),
        )
        .unwrap();
        assert!(t.areas.is_empty(), "less than 2% all areas");
        assert_eq!(t.forecaster, "Hart");
        let p = parse_outlook(
            Product::Day3Severe,
            &data("day2otlk_prob-2026-10-29.geojson"),
        )
        .unwrap();
        let labels: Vec<_> = p.areas.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["0.05", "0.15"]);
        assert!(parse_outlook(Product::Day1Hail, b"{}").is_err());
    }

    #[test]
    fn outlook_holes_are_not_inside() {
        let json = br##"{"features": [{"properties": {"LABEL": "SLGT", "LABEL2": "Slight Risk",
            "stroke": "#DDAA00", "fill": "#FFE066"},
            "geometry": {"type": "Polygon", "coordinates": [
                [[-100,30],[-90,30],[-90,40],[-100,40],[-100,30]],
                [[-96,34],[-94,34],[-94,36],[-96,36],[-96,34]]]}}]}"##;
        let o = parse_outlook(Product::Day1Categorical, json).unwrap();
        assert!(o.at(32.0, -98.0).is_some());
        assert!(o.at(35.0, -95.0).is_none(), "inside the hole");
    }

    #[test]
    fn active_lists() {
        let ww = parse_active(&kmz("ActiveWW-2026-07-21T0035Z.kmz"));
        assert_eq!(ww.len(), 4);
        assert_eq!(
            ww[0].href,
            "https://www.spc.noaa.gov/products/watch/WW0501_SAW.kmz"
        );
        let (kind, n, till) = parse_watch_link(&ww[0].name).unwrap();
        assert_eq!((kind, n), (WatchKind::Tornado, 501));
        assert_eq!(
            till,
            crate::alerts::parse_time("2026-07-21T04:00:00Z").unwrap()
        );
        let (kind, n, _) = parse_watch_link(&ww[1].name).unwrap();
        assert_eq!((kind, n), (WatchKind::SevereThunderstorm, 505));
        assert!(parse_active(&kmz("ActiveWW-none.kmz")).is_empty());

        let md = parse_active(&kmz("ActiveMD-2026-07-21T1815Z.kmz"));
        assert_eq!(md.len(), 3);
        assert_eq!(
            md[0].href,
            "https://www.spc.noaa.gov/products/md/MD1691.kmz"
        );
        assert_eq!(parse_md_link(&md[2].name), Some(1689));
    }

    #[test]
    fn watch_and_md_polygons() {
        let w = kml_rings(&kmz("WW0501_SAW.kmz"));
        assert_eq!(
            w,
            vec![vec![
                (46.71, -86.67),
                (44.22, -87.46),
                (44.22, -90.48),
                (46.71, -89.84),
                (46.71, -86.67)
            ]]
        );
        let kml = kmz("MD1691.kmz");
        let m = kml_rings(&kml);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].len(), 20);
        assert!(ring_contains(&m[0], 38.0, -77.5), "central Virginia");
        let now = crate::alerts::parse_time("2026-07-21T18:15:00Z").unwrap();
        let (areas, concerning, ends) = parse_md_description(&kml, now);
        assert_eq!(areas, "portions of the Mid-Atlantic");
        assert_eq!(concerning, "Severe potential...Watch likely");
        assert_eq!(ends, crate::alerts::parse_time("2026-07-21T19:30:00Z"));
    }

    #[test]
    fn day_of_month_times_resolve_near_now() {
        let t = |s| crate::alerts::parse_time(s).unwrap();
        // Late on the 31st, "010130Z" is the next morning, next month.
        assert_eq!(
            ddhhmm_near("010130Z", t("2026-10-31T23:00:00Z")),
            Some(t("2026-11-01T01:30:00Z"))
        );
        assert_eq!(
            ddhhmm_near("312300Z", t("2027-01-01T00:30:00Z")),
            Some(t("2026-12-31T23:00:00Z"))
        );
        assert_eq!(ddhhmm_near("garbage", 0), None);
    }

    #[test]
    fn page_texts() {
        let w = page_text(&String::from_utf8(data("ww0501.html")).unwrap());
        assert!(w.starts_with("SEL1"), "{w}");
        assert!(w.contains("Tornado Watch Number 501"));
        assert!(w.contains("CONTINUE...WW 500..."), "links unwrapped");
        assert!(!w.contains("WOUS64 KWNS 202048"), "only the first <pre>");
        let m = page_text(&String::from_utf8(data("md1691.html")).unwrap());
        assert!(m.starts_with("Mesoscale Discussion 1691"));
        assert!(m.contains("\nSUMMARY...Multiple rounds"), "dedented");
    }

    #[test]
    fn zip_errors() {
        assert!(unzip_first(b"").is_err());
        assert!(unzip_first(b"PK\x03\x04").is_err());
        let mut z = data("MD1691.kmz");
        z.truncate(60);
        assert!(unzip_first(&z).is_err());
    }

    #[test]
    fn product_keys_round_trip() {
        for p in Product::ALL {
            assert_eq!(Product::from_key(&p.key()), Some(p));
        }
        assert_eq!(
            Product::Day3Severe.url(),
            "https://www.spc.noaa.gov/products/outlook/day3otlk_prob.nolyr.geojson"
        );
    }

    /// Against the real SPC: `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_spc() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut http = Http::new().unwrap();
        let mut s = SpcSource::new();
        let d = rt.block_on(s.fetch(&mut http)).unwrap().unwrap();
        assert_eq!(d.outlooks.len(), Product::ALL.len());
        assert!(rt.block_on(s.fetch(&mut http)).unwrap().is_none());
    }
}
