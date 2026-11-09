//! NWS warnings (KICKOFF §3): the storm-based warnings in the National
//! Weather Service's public alerts API, `api.weather.gov/alerts/active`
//! (GeoJSON, documented at https://www.weather.gov/documentation/services-web-api).
//! Each of these warnings carries its own polygon; county-based products
//! (watches, advisories) have no geometry here; watches are drawn from
//! SPC's outlines instead (`crate::spc`).
//!
//! Polled once a minute through [`crate::poll`] (conditional GETs, the
//! last answer kept on disk for a cold start). The NWS asks for a
//! descriptive User-Agent; ours is [`crate::s3::USER_AGENT`].

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const ENDPOINT: &str = "https://api.weather.gov/alerts/active";

/// Warning types drawn on the map, most severe first (the drawing order is
/// the reverse, so the most severe ends up on top).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Event {
    Tornado,
    ExtremeWind,
    SevereThunderstorm,
    FlashFlood,
    SnowSquall,
    DustStorm,
    SpecialMarine,
}

impl Event {
    pub const ALL: [Event; 7] = [
        Event::Tornado,
        Event::ExtremeWind,
        Event::SevereThunderstorm,
        Event::FlashFlood,
        Event::SnowSquall,
        Event::DustStorm,
        Event::SpecialMarine,
    ];

    /// The API's `event` name.
    pub fn name(self) -> &'static str {
        match self {
            Event::Tornado => "Tornado Warning",
            Event::ExtremeWind => "Extreme Wind Warning",
            Event::SevereThunderstorm => "Severe Thunderstorm Warning",
            Event::FlashFlood => "Flash Flood Warning",
            Event::SnowSquall => "Snow Squall Warning",
            Event::DustStorm => "Dust Storm Warning",
            Event::SpecialMarine => "Special Marine Warning",
        }
    }

    /// A stable key for the config file.
    pub fn key(self) -> &'static str {
        match self {
            Event::Tornado => "tornado",
            Event::ExtremeWind => "extreme-wind",
            Event::SevereThunderstorm => "severe-thunderstorm",
            Event::FlashFlood => "flash-flood",
            Event::SnowSquall => "snow-squall",
            Event::DustStorm => "dust-storm",
            Event::SpecialMarine => "special-marine",
        }
    }

    pub fn from_name(name: &str) -> Option<Event> {
        Event::ALL.into_iter().find(|e| e.name() == name)
    }
}

/// One warning with its polygon(s).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub event: Event,
    /// "NWS Midland/Odessa TX".
    pub sender: String,
    /// Counties or marine zones, "Culberson, TX".
    pub area: String,
    pub headline: String,
    pub description: String,
    pub instruction: String,
    pub sent_ms: i64,
    pub expires_ms: i64,
    /// Outer rings as (lat, lon); the API's GeoJSON is (lon, lat).
    pub rings: Vec<Vec<(f64, f64)>>,
}

impl Alert {
    /// Whether (lat, lon) is inside one of the rings (even-odd rule; the
    /// polygons are a few counties across, so plain lat/lon is fine).
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        self.rings.iter().any(|ring| {
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
        })
    }

    pub fn active(&self, now_ms: i64) -> bool {
        self.expires_ms > now_ms
    }
}

/// The URL polled: actual (not test) alerts of the types above only, which
/// keeps the answer to tens of KB.
pub fn url() -> String {
    let events: Vec<String> = Event::ALL
        .iter()
        .map(|e| e.name().replace(' ', "%20"))
        .collect();
    format!("{ENDPOINT}?status=actual&event={}", events.join(","))
}

/// Parses an alerts GeoJSON FeatureCollection. Features without a polygon,
/// of other types, or with bad times are skipped rather than failing the
/// whole answer.
pub fn parse(json: &[u8]) -> Result<Vec<Alert>, String> {
    let v: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let features = v["features"]
        .as_array()
        .ok_or_else(|| "no features array".to_string())?;
    let text = |p: &Value, k: &str| p[k].as_str().unwrap_or_default().trim().to_string();
    let mut out: Vec<Alert> = features
        .iter()
        .filter_map(|f| {
            let p = &f["properties"];
            let event = Event::from_name(p["event"].as_str()?)?;
            let rings = rings(&f["geometry"]);
            if rings.is_empty() {
                return None;
            }
            Some(Alert {
                id: text(p, "id"),
                event,
                sender: text(p, "senderName"),
                area: text(p, "areaDesc"),
                headline: text(p, "headline"),
                description: text(p, "description"),
                instruction: text(p, "instruction"),
                sent_ms: parse_time(p["sent"].as_str()?)?,
                // `ends` is when the hazard ends; `expires` when the message
                // does. Prefer the first, as the warning's "until" time.
                expires_ms: p["ends"]
                    .as_str()
                    .and_then(parse_time)
                    .or_else(|| parse_time(p["expires"].as_str()?))?,
                rings,
            })
        })
        .collect();
    out.sort_by(|a, b| a.event.cmp(&b.event).then(b.sent_ms.cmp(&a.sent_ms)));
    Ok(out)
}

/// Outer rings of a GeoJSON Polygon or MultiPolygon, as (lat, lon).
fn rings(g: &Value) -> Vec<Vec<(f64, f64)>> {
    let ring = |r: &Value| -> Option<Vec<(f64, f64)>> {
        let pts: Vec<(f64, f64)> = r
            .as_array()?
            .iter()
            .filter_map(|c| Some((c[1].as_f64()?, c[0].as_f64()?)))
            .collect();
        (pts.len() >= 3).then_some(pts)
    };
    let coords = &g["coordinates"];
    match g["type"].as_str() {
        Some("Polygon") => coords.get(0).and_then(ring).into_iter().collect(),
        Some("MultiPolygon") => coords
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|poly| poly.get(0).and_then(ring))
            .collect(),
        _ => Vec::new(),
    }
}

/// RFC 3339 / ISO 8601 with a numeric offset or `Z`, as the API sends
/// ("2026-10-28T20:12:00-05:00"), to ms since the epoch.
pub fn parse_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Skip fractional seconds.
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    let offset_s = match b.get(i)? {
        b'Z' => 0,
        sign @ (b'+' | b'-') => {
            let oh = num(i + 1..i + 3)?;
            let om = num(i + 4..i + 6)?;
            let o = oh * 3600 + om * 60;
            if *sign == b'-' { -o } else { o }
        }
        _ => return None,
    };
    let days = crate::tdwr::days_from_civil(y, mo, d);
    Some((days * 86_400 + h * 3600 + mi * 60 + sec - offset_s) * 1000)
}

/// Polls the alerts API once a minute.
#[derive(Default)]
pub struct AlertSource {
    /// Whether an answer has been parsed, so an unchanged one isn't resent.
    seen: bool,
}

impl crate::poll::Source for AlertSource {
    type Data = Vec<Alert>;
    const NAME: &'static str = "alerts";

    fn every(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn fetch(&mut self, http: &mut crate::poll::Http) -> Result<Option<Vec<Alert>>, String> {
        let got = http.get(&url(), "application/geo+json").await?;
        if !got.changed && self.seen {
            return Ok(None);
        }
        let a = parse(&got.body).map_err(|e| format!("bad answer: {e}"))?;
        self.seen = true;
        Ok(Some(a))
    }

    fn save(&self, data: &Vec<Alert>) -> Option<Vec<u8>> {
        serde_json::to_vec(data).ok()
    }

    fn load(&self, bytes: &[u8]) -> Option<Vec<Alert>> {
        serde_json::from_slice(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Alert> {
        let p = format!(
            "{}/../../tests/data/alerts/active-2026-10-29T0136Z.json",
            env!("CARGO_MANIFEST_DIR")
        );
        parse(&std::fs::read(p).unwrap()).unwrap()
    }

    #[test]
    fn times() {
        assert_eq!(
            parse_time("2026-10-28T20:12:00-05:00"),
            Some(1_793_236_320_000)
        );
        assert_eq!(
            parse_time("2026-10-29T01:12:00Z"),
            parse_time("2026-10-28T20:12:00-05:00")
        );
        assert_eq!(
            parse_time("2026-10-29T01:12:00.250+00:00"),
            Some(1_793_236_320_000)
        );
        assert_eq!(parse_time("2026-13-29T01:12:00Z"), None);
        assert_eq!(parse_time("garbage"), None);
        assert_eq!(parse_time(""), None);
    }

    /// A real answer (2026-10-29 01:36 UTC): seven flash flood warnings in
    /// west Texas and New Mexico, each with a polygon.
    #[test]
    fn real_answer() {
        let a = sample();
        assert_eq!(a.len(), 7);
        let first = a.iter().find(|a| a.area == "Culberson, TX").unwrap();
        assert_eq!(first.event, Event::FlashFlood);
        assert_eq!(first.sender, "NWS Midland/Odessa TX");
        assert_eq!(
            first.expires_ms,
            parse_time("2026-10-28T23:15:00-05:00").unwrap()
        );
        assert!(first.description.contains("Flash Flood Warning"));
        let ring = &first.rings[0];
        assert!(ring.len() >= 4);
        // Every vertex is in Culberson County's neighborhood: (lat, lon).
        assert!(
            ring.iter()
                .all(|&(lat, lon)| (30.0..33.0).contains(&lat) && (-106.0..-103.0).contains(&lon))
        );
        // Its own centroid is inside; a point in Dallas is not.
        let n = ring.len() as f64;
        let c = ring
            .iter()
            .fold((0.0, 0.0), |s, p| (s.0 + p.0 / n, s.1 + p.1 / n));
        assert!(first.contains(c.0, c.1));
        assert!(!first.contains(32.78, -96.8));
    }

    #[test]
    fn skips_what_it_cannot_draw() {
        let json = br#"{"features": [
            {"properties": {"event": "Tornado Warning", "sent": "2026-10-28T20:12:00-05:00",
                "ends": "2026-10-28T20:45:00-05:00", "id": "a"},
             "geometry": {"type": "Polygon", "coordinates": [[[-97,35],[-96,35],[-96,36],[-97,35]]]}},
            {"properties": {"event": "Tornado Watch", "sent": "2026-10-28T20:12:00-05:00",
                "expires": "2026-10-28T23:00:00-05:00"}, "geometry": null},
            {"properties": {"event": "Severe Thunderstorm Warning", "sent": "bad"},
             "geometry": {"type": "Polygon", "coordinates": [[[-97,35],[-96,35],[-96,36]]]}},
            {"properties": {"event": "Flash Flood Warning", "sent": "2026-10-28T20:12:00-05:00",
                "expires": "2026-10-28T23:00:00-05:00"},
             "geometry": {"type": "MultiPolygon", "coordinates": [
                [[[-97,35],[-96,35],[-96,36]]], [[[-90,30],[-89,30],[-89,31]]]]}}
        ]}"#;
        let a = parse(json).unwrap();
        assert_eq!(a.len(), 2, "the watch and the bad time are skipped");
        assert_eq!(a[0].event, Event::Tornado, "most severe first");
        assert_eq!(a[1].rings.len(), 2);
        assert_eq!(a[0].expires_ms, parse_time("2026-10-29T01:45:00Z").unwrap());
        assert!(a[0].active(a[0].sent_ms) && !a[0].active(a[0].expires_ms));
        assert!(parse(b"{}").is_err());
        assert!(parse(b"not json").is_err());
    }

    #[test]
    fn url_names_every_type() {
        let u = url();
        assert!(u.starts_with("https://api.weather.gov/alerts/active?status=actual&event="));
        assert!(u.contains("Tornado%20Warning,Extreme%20Wind%20Warning"));
        assert!(!u.contains(' '));
    }

    /// Against the real API: `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_alerts() {
        use crate::poll::Source;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut http = crate::poll::Http::new().unwrap();
        let mut s = AlertSource::default();
        rt.block_on(s.fetch(&mut http)).unwrap().unwrap();
        // Usually unchanged straight after; a new warning in between is fine.
        rt.block_on(s.fetch(&mut http)).unwrap();
    }
}
