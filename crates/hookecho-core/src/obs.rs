//! Surface observations (the owner's "command center"): METARs from the
//! NOAA Aviation Weather Center, aviationweather.gov.
//!
//! - **Every station:** the AWC's cache file
//!   `data/cache/metars.cache.csv.gz`, the latest report from each of about
//!   5,000 stations, rebuilt every minute (≈ 270 KB). Polled every three
//!   minutes with a conditional GET.
//! - **One station's last hours:** the Data API,
//!   `api/data/metar?ids=KOKC&hours=4&format=json`, for the station panel's
//!   name and pressure trend. Fetched for the one station the panel shows.
//!
//! Both are documented at https://aviationweather.gov/data/api/. Units are
//! the reports' own: °C, knots, statute miles; pressure in hPa (the CSV's
//! altimeter is inHg and is converted).

use crate::poll::{Http, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const CACHE_URL: &str = "https://aviationweather.gov/data/cache/metars.cache.csv.gz";
pub const API: &str = "https://aviationweather.gov/api/data/metar";

const HPA_PER_INHG: f32 = 33.863_89;

/// One station's latest report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ob {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    pub time_ms: i64,
    pub temp_c: Option<f32>,
    pub dewpoint_c: Option<f32>,
    /// Degrees true the wind blows from; `None` when calm, variable or
    /// missing.
    pub wind_dir: Option<u16>,
    pub wind_kt: Option<u16>,
    pub gust_kt: Option<u16>,
    /// Statute miles; `vis_plus` for "10+" (at least that far).
    pub vis_mi: Option<f32>,
    pub vis_plus: bool,
    /// Altimeter setting, hPa.
    pub altimeter_hpa: Option<f32>,
    pub slp_hpa: Option<f32>,
    /// The report's own 3-hour pressure change, hPa, when it carries one.
    pub tendency_hpa: Option<f32>,
    /// Present weather, "-RA BR".
    pub wx: String,
    /// "VFR", "MVFR", "IFR", "LIFR".
    pub category: String,
    pub raw: String,
}

/// A station's recent reports, newest first, with its name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub id: String,
    /// "Oklahoma City/Will Rogers Intl, OK, US".
    pub name: String,
    pub reports: Vec<Ob>,
}

impl History {
    /// Pressure change over about three hours, hPa: from the altimeter
    /// readings in the history when they span 2.5 hours or more, else from
    /// the newest report's own tendency group.
    pub fn trend_hpa(&self) -> Option<f32> {
        let now = self.reports.first()?;
        let p = now.altimeter_hpa?;
        let then = self
            .reports
            .iter()
            .filter(|r| now.time_ms - r.time_ms >= 150 * 60_000)
            .min_by_key(|r| (now.time_ms - r.time_ms - 180 * 60_000).abs());
        match then.and_then(|r| r.altimeter_hpa) {
            Some(q) => Some(p - q),
            None => now.tendency_hpa,
        }
    }
}

/// Everything the feed has.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Obs {
    pub stations: Vec<Ob>,
    /// The panel station's history, once fetched.
    pub station: Option<History>,
}

impl Obs {
    /// The station nearest (lat, lon) with a report newer than `since_ms`.
    pub fn nearest(&self, lat: f64, lon: f64, since_ms: i64) -> Option<&Ob> {
        self.stations
            .iter()
            .filter(|o| o.time_ms >= since_ms)
            .min_by(|a, b| {
                let d = |o: &Ob| crate::geo::range_bearing((lat, lon), (o.lat, o.lon)).0;
                d(a).total_cmp(&d(b))
            })
    }

    pub fn by_id(&self, id: &str) -> Option<&Ob> {
        self.stations.iter().find(|o| o.id == id)
    }
}

/// Dewpoint and temperature to relative humidity, percent (Magnus formula,
/// Alduchov and Eskridge 1996 coefficients).
pub fn relative_humidity(t_c: f32, td_c: f32) -> f32 {
    let e = |t: f32| (17.625 * t / (243.04 + t)).exp();
    (100.0 * e(td_c) / e(t_c)).clamp(0.0, 100.0)
}

// ------------------------------------------------------------------ parsing

/// One CSV record: comma separated, fields optionally in double quotes
/// with `""` for a quote (RFC 4180).
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Parses the AWC cache file (gzip or plain CSV). Rows without a station,
/// position or time are skipped.
pub fn parse_cache(bytes: &[u8]) -> Result<Vec<Ob>, String> {
    let mut text = String::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take(64 << 20)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
    } else {
        text = String::from_utf8_lossy(bytes).into_owned();
    }
    let mut lines = text.lines();
    let header = csv_fields(lines.next().ok_or("empty file")?);
    // The first column of each name: the file repeats sky_cover and
    // cloud_base_ft_agl.
    let col = |name: &str| header.iter().position(|h| h == name);
    let need = |name: &str| col(name).ok_or_else(|| format!("no {name} column"));
    let (raw, id, time, lat, lon) = (
        need("raw_text")?,
        need("station_id")?,
        need("observation_time")?,
        need("latitude")?,
        need("longitude")?,
    );
    let opt = |name| col(name);
    let (t, td, wd, ws, wg, vis, alt, slp, wx, cat, tend) = (
        opt("temp_c"),
        opt("dewpoint_c"),
        opt("wind_dir_degrees"),
        opt("wind_speed_kt"),
        opt("wind_gust_kt"),
        opt("visibility_statute_mi"),
        opt("altim_in_hg"),
        opt("sea_level_pressure_mb"),
        opt("wx_string"),
        opt("flight_category"),
        opt("three_hr_pressure_tendency_mb"),
    );
    let mut out = Vec::new();
    for line in lines {
        let f = csv_fields(line);
        let s = |i: Option<usize>| i.and_then(|i| f.get(i)).map_or("", |s| s.trim());
        let num = |i: Option<usize>| s(i).parse::<f32>().ok();
        let int = |i: Option<usize>| s(i).parse::<u16>().ok();
        let (Ok(la), Ok(lo)) = (s(Some(lat)).parse::<f64>(), s(Some(lon)).parse::<f64>()) else {
            continue;
        };
        let Some(time_ms) = crate::alerts::parse_time(s(Some(time))) else {
            continue;
        };
        if s(Some(id)).is_empty() {
            continue;
        }
        let (vis_mi, vis_plus) = visibility(s(vis));
        let wind_kt = int(ws);
        out.push(Ob {
            id: s(Some(id)).to_string(),
            lat: la,
            lon: lo,
            time_ms,
            temp_c: num(t),
            dewpoint_c: num(td),
            wind_dir: int(wd).filter(|_| wind_kt.is_some_and(|k| k > 0)),
            wind_kt,
            gust_kt: int(wg),
            vis_mi,
            vis_plus,
            altimeter_hpa: num(alt).map(|a| a * HPA_PER_INHG),
            slp_hpa: num(slp),
            tendency_hpa: num(tend),
            wx: s(wx).to_string(),
            category: s(cat).to_string(),
            raw: s(Some(raw)).to_string(),
        });
    }
    Ok(out)
}

/// "10+", "6+", "0.25", or a JSON number.
fn visibility(s: &str) -> (Option<f32>, bool) {
    match s.strip_suffix('+') {
        Some(v) => (v.parse().ok(), true),
        None => (s.parse().ok(), false),
    }
}

/// Parses a Data API answer (`format=json`) for one station, newest first.
pub fn parse_history(json: &[u8]) -> Result<History, String> {
    let v: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let rows = v.as_array().ok_or("not an array")?;
    let f = |r: &Value, k: &str| r[k].as_f64().map(|x| x as f32);
    let u = |r: &Value, k: &str| r[k].as_u64().map(|x| x as u16);
    let mut reports: Vec<Ob> = rows
        .iter()
        .filter_map(|r| {
            let (vis_mi, vis_plus) = match &r["visib"] {
                Value::String(s) => visibility(s),
                v => (v.as_f64().map(|x| x as f32), false),
            };
            let wind_kt = u(r, "wspd");
            Some(Ob {
                id: r["icaoId"].as_str()?.to_string(),
                lat: r["lat"].as_f64()?,
                lon: r["lon"].as_f64()?,
                time_ms: r["obsTime"].as_i64()? * 1000,
                temp_c: f(r, "temp"),
                dewpoint_c: f(r, "dewp"),
                // "VRB" is a string; calm is 0 at 0 kt.
                wind_dir: u(r, "wdir").filter(|_| wind_kt.is_some_and(|k| k > 0)),
                wind_kt,
                gust_kt: u(r, "wgst"),
                vis_mi,
                vis_plus,
                altimeter_hpa: f(r, "altim"),
                slp_hpa: f(r, "slp"),
                tendency_hpa: f(r, "presTend"),
                wx: r["wxString"].as_str().unwrap_or_default().to_string(),
                category: r["fltCat"].as_str().unwrap_or_default().to_string(),
                raw: r["rawOb"].as_str().unwrap_or_default().to_string(),
            })
        })
        .collect();
    reports.sort_by_key(|r| std::cmp::Reverse(r.time_ms));
    let first = reports.first().ok_or("no reports")?;
    Ok(History {
        id: first.id.clone(),
        name: rows
            .iter()
            .find_map(|r| r["name"].as_str())
            .unwrap_or_default()
            .to_string(),
        reports,
    })
}

// ---------------------------------------------------------------- the feed

/// The station whose history the panel wants; shared with the UI.
pub type Wanted = Arc<Mutex<Option<String>>>;

pub struct ObsSource {
    wanted: Wanted,
    data: Obs,
    bulk_due_ms: i64,
    station_due_ms: i64,
}

const BULK_EVERY_MS: i64 = 3 * 60_000;
const STATION_EVERY_MS: i64 = 10 * 60_000;

impl ObsSource {
    pub fn new(wanted: Wanted) -> ObsSource {
        ObsSource {
            wanted,
            data: Obs::default(),
            bulk_due_ms: 0,
            station_due_ms: 0,
        }
    }
}

impl Source for ObsSource {
    type Data = Obs;
    const NAME: &'static str = "obs";

    /// Wakes often so a new panel station is fetched promptly; each part
    /// keeps its own schedule.
    fn every(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn fetch(&mut self, http: &mut Http) -> Result<Option<Obs>, String> {
        let now = crate::time::now_ms();
        let mut next = self.data.clone();
        let mut trouble = None;
        if now >= self.bulk_due_ms {
            match http.get(CACHE_URL, "*/*").await {
                Ok(g) if g.changed || next.stations.is_empty() => match parse_cache(&g.body) {
                    Ok(s) => {
                        next.stations = s;
                        self.bulk_due_ms = now + BULK_EVERY_MS;
                    }
                    Err(e) => trouble = Some(format!("bad answer: {e}")),
                },
                Ok(_) => self.bulk_due_ms = now + BULK_EVERY_MS,
                Err(e) => trouble = Some(e),
            }
        }
        let wanted = self.wanted.lock().map(|w| w.clone()).unwrap_or_default();
        let have = next.station.as_ref().map(|h| h.id.clone());
        match wanted {
            Some(id) if have.as_ref() != Some(&id) || now >= self.station_due_ms => {
                let url = format!("{API}?ids={id}&hours=4&format=json");
                match http.get(&url, "application/json").await {
                    Ok(g) => {
                        // An empty answer (no recent reports): keep none.
                        next.station = parse_history(&g.body).ok();
                        self.station_due_ms = now + STATION_EVERY_MS;
                    }
                    Err(e) => trouble = Some(e),
                }
                http.retain(|u| !u.starts_with(API) || u == url);
            }
            None => next.station = None,
            _ => {}
        }
        if let Some(t) = trouble
            && next == self.data
        {
            return Err(t);
        }
        if next == self.data {
            return Ok(None);
        }
        self.data = next.clone();
        Ok(Some(next))
    }

    fn save(&self, data: &Obs) -> Option<Vec<u8>> {
        serde_json::to_vec(data).ok()
    }

    fn load(&self, bytes: &[u8]) -> Option<Obs> {
        serde_json::from_slice(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../tests/data/metar/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn csv_quotes() {
        assert_eq!(
            csv_fields(r#""a,b",c,,"say ""hi""""#),
            ["a,b", "c", "", "say \"hi\""]
        );
    }

    /// A trimmed real cache file (2026-10-29 15:14 UTC): 20 stations from
    /// around the world, then Oklahoma, Kansas and north Texas.
    #[test]
    fn cache_file() {
        let obs = parse_cache(&data("metars-2026-10-29T1514Z.csv.gz")).unwrap();
        assert_eq!(obs.len(), 159);
        let o = obs.iter().find(|o| o.id == "KSXT").unwrap();
        assert_eq!((o.temp_c, o.dewpoint_c), (Some(2.8), Some(2.2)));
        assert_eq!((o.vis_mi, o.vis_plus), (Some(0.25), false));
        assert_eq!(o.wx, "FG");
        assert_eq!(o.category, "LIFR");
        assert!((o.altimeter_hpa.unwrap() - 1015.2).abs() < 0.1);
        let gusty = obs.iter().find(|o| o.id == "CYMT").unwrap();
        assert_eq!(
            (gusty.wind_dir, gusty.wind_kt, gusty.gust_kt),
            (Some(210), Some(11), Some(19))
        );
        assert_eq!((gusty.vis_mi, gusty.vis_plus), (Some(9.0), false));
        let ten_plus = obs.iter().find(|o| o.vis_plus).unwrap();
        assert!(ten_plus.vis_mi.is_some());
        // Nearest to downtown Oklahoma City.
        let ok = Obs {
            stations: obs.clone(),
            station: None,
        };
        let n = ok.nearest(35.47, -97.52, 0).unwrap();
        assert!(
            ["KOKC", "KPWA", "KTIK", "KOUN"].contains(&n.id.as_str()),
            "{}",
            n.id
        );
        assert!(parse_cache(b"").is_err());
        assert!(parse_cache(b"a,b\n1,2").is_err());
    }

    #[test]
    fn station_history() {
        let h = parse_history(&data("KOKC-hours4-2026-10-29T1500Z.json")).unwrap();
        assert_eq!(h.id, "KOKC");
        assert_eq!(h.name, "Oklahoma City/Will Rogers Intl, OK, US");
        assert_eq!(h.reports.len(), 4);
        assert!(h.reports[0].time_ms > h.reports[3].time_ms, "newest first");
        assert_eq!(h.reports[0].temp_c, Some(19.4));
        assert!(h.reports[0].vis_plus);
        // 1021.1 now against 1019.4 three hours before.
        assert!((h.trend_hpa().unwrap() - 1.7).abs() < 0.01);
        // With only the newest report, its own 3-hour group (1.9).
        let one = History {
            reports: h.reports[..1].to_vec(),
            ..h.clone()
        };
        assert_eq!(one.trend_hpa(), Some(1.9));
        assert!(parse_history(b"[]").is_err());
    }

    #[test]
    fn humidity() {
        assert!((relative_humidity(20.0, 20.0) - 100.0).abs() < 0.01);
        assert!((relative_humidity(30.0, 10.0) - 28.9).abs() < 0.5);
    }

    /// Against the real AWC: `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_obs() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut http = Http::new().unwrap();
        let wanted: Wanted = Arc::new(Mutex::new(Some("KOKC".into())));
        let mut s = ObsSource::new(wanted);
        let d = rt.block_on(s.fetch(&mut http)).unwrap().unwrap();
        assert!(d.stations.len() > 3000);
        assert_eq!(d.station.unwrap().id, "KOKC");
    }
}
