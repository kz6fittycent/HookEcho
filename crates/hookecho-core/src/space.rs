//! Space weather (owner request, 2026-10-28) from NOAA's Space Weather
//! Prediction Center, services.swpc.noaa.gov (free JSON, documented at
//! https://www.swpc.noaa.gov/content/data-access):
//!
//! - `products/noaa-scales.json`: the NOAA R (radio blackout), S (solar
//!   radiation storm) and G (geomagnetic storm) levels now, and the
//!   forecast for the next three days.
//! - `products/noaa-planetary-k-index-forecast.json`: 3-hourly Kp, observed
//!   for the past week and predicted for three days.
//! - `json/planetary_k_index_1m.json`: the minute-by-minute estimated Kp.
//! - `products/geospace/propagated-solar-wind-1-hour.json`: solar wind at
//!   L1 (speed, density, Bz, Bt), with when it reaches Earth.
//! - `json/goes/primary/xray-flares-latest.json`: the current X-ray class
//!   and the latest flare.
//! - `products/alerts.json`: SWPC alerts, warnings and watches.
//! - `json/ovation_aurora_latest.json`: the OVATION aurora forecast, the
//!   chance of seeing aurora overhead on a 1° grid (fetched only while the
//!   aurora layer is on: it is ~900 KB).

use crate::poll::{Http, Source};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const BASE: &str = "https://services.swpc.noaa.gov";

/// One NOAA scale: its level now (0-5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scales {
    pub r: u8,
    pub s: u8,
    pub g: u8,
}

/// One forecast day.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DayOutlook {
    /// "2026-10-31".
    pub date: String,
    /// Chance of an R1-R2 (minor) and R3+ (major) radio blackout, percent.
    pub r_minor: Option<u8>,
    pub r_major: Option<u8>,
    /// Chance of an S1+ radiation storm, percent.
    pub s: Option<u8>,
    /// The geomagnetic storm level predicted.
    pub g: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum KpKind {
    Observed,
    Estimated,
    Predicted,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kp {
    /// Start of the 3-hour period.
    pub time_ms: i64,
    pub kp: f32,
    pub kind: KpKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolarWind {
    /// When this was measured at L1, and when it arrives at Earth.
    pub time_ms: i64,
    pub arrives_ms: Option<i64>,
    pub speed_km_s: Option<f32>,
    pub density_cm3: Option<f32>,
    /// Magnetic field, nT: Bz (southward is negative, the one that drives
    /// storms) and the total, Bt.
    pub bz_nt: Option<f32>,
    pub bt_nt: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flare {
    pub max_class: String,
    pub begin_ms: Option<i64>,
    pub max_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Xray {
    pub time_ms: i64,
    /// "B1.9".
    pub class: String,
    pub latest_flare: Option<Flare>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpaceAlert {
    /// SWPC's product code, "K04W".
    pub code: String,
    pub issued_ms: i64,
    /// "WARNING: Geomagnetic K-index of 4 expected".
    pub title: String,
    pub message: String,
}

/// OVATION aurora forecast: percent chance, 0-100, on a 1° grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aurora {
    pub observed_ms: i64,
    pub forecast_ms: i64,
    /// 360 longitudes (0..359 °E) × 181 latitudes (-90..90), longitude
    /// major: `grid[lon * 181 + (lat + 90)]`.
    pub grid: Vec<u8>,
}

impl Aurora {
    pub fn at(&self, lat: f64, lon: f64) -> u8 {
        let lon = (lon.round() as i64).rem_euclid(360) as usize;
        let lat = (lat.round() as i64).clamp(-90, 90);
        self.grid
            .get(lon * 181 + (lat + 90) as usize)
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpaceWeather {
    pub scales: Scales,
    pub days: Vec<DayOutlook>,
    pub kp: Vec<Kp>,
    /// The latest minute estimate: (time, Kp).
    pub kp_now: Option<(i64, f32)>,
    pub wind: Option<SolarWind>,
    pub xray: Option<Xray>,
    /// Newest first.
    pub alerts: Vec<SpaceAlert>,
    pub aurora: Option<Aurora>,
}

impl SpaceWeather {
    /// The highest Kp predicted from now on.
    pub fn kp_max_ahead(&self, now_ms: i64) -> Option<f32> {
        self.kp
            .iter()
            .filter(|k| k.kind == KpKind::Predicted && k.time_ms + 3 * 3_600_000 > now_ms)
            .map(|k| k.kp)
            .max_by(f32::total_cmp)
    }
}

/// The NOAA G level for a Kp: G1 at 5 ... G5 at 9 (NOAA Space Weather
/// Scales).
pub fn g_for_kp(kp: f32) -> u8 {
    (kp.floor() as i32 - 4).clamp(0, 5) as u8
}

// ------------------------------------------------------------------ parsing

/// SWPC times: "2026-10-29T15:18:00Z", "2026-10-29T15:18:00" (UTC, no
/// zone) or "2026-10-28 16:38:01.100".
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim().replacen(' ', "T", 1);
    let has_zone = s.ends_with('Z') || s[s.len().saturating_sub(6)..].contains(['+', '-']);
    if has_zone && s.len() > 19 {
        crate::alerts::parse_time(&s)
    } else {
        crate::alerts::parse_time(&format!("{s}Z"))
    }
}

fn json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

/// A number that SWPC may send as a number or a string.
fn num(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => n.as_f64().map(|x| x as f32),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn parse_scales(bytes: &[u8]) -> Result<(Scales, Vec<DayOutlook>), String> {
    let v = json(bytes)?;
    let level = |d: &Value, k: &str| num(&d[k]["Scale"]).map_or(0, |x| x as u8);
    let now = &v["0"];
    if !now.is_object() {
        return Err("no current scales".into());
    }
    let scales = Scales {
        r: level(now, "R"),
        s: level(now, "S"),
        g: level(now, "G"),
    };
    let days = ["1", "2", "3"]
        .iter()
        .filter_map(|k| {
            let d = &v[*k];
            Some(DayOutlook {
                date: d["DateStamp"].as_str()?.to_string(),
                r_minor: num(&d["R"]["MinorProb"]).map(|x| x as u8),
                r_major: num(&d["R"]["MajorProb"]).map(|x| x as u8),
                s: num(&d["S"]["Prob"]).map(|x| x as u8),
                g: level(d, "G"),
            })
        })
        .collect();
    Ok((scales, days))
}

pub fn parse_kp_forecast(bytes: &[u8]) -> Result<Vec<Kp>, String> {
    let v = json(bytes)?;
    let rows = v.as_array().ok_or("not an array")?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(Kp {
                time_ms: parse_time(r["time_tag"].as_str()?)?,
                kp: num(&r["kp"])?,
                kind: match r["observed"].as_str()? {
                    "observed" => KpKind::Observed,
                    "estimated" => KpKind::Estimated,
                    _ => KpKind::Predicted,
                },
            })
        })
        .collect())
}

/// The newest minute estimate of Kp.
pub fn parse_kp_1m(bytes: &[u8]) -> Result<Option<(i64, f32)>, String> {
    let v = json(bytes)?;
    let rows = v.as_array().ok_or("not an array")?;
    Ok(rows.iter().rev().find_map(|r| {
        Some((
            parse_time(r["time_tag"].as_str()?)?,
            num(&r["estimated_kp"])?,
        ))
    }))
}

/// The newest row of the propagated solar wind table (a header row, then
/// rows of values; any value may be null).
pub fn parse_solar_wind(bytes: &[u8]) -> Result<Option<SolarWind>, String> {
    let v = json(bytes)?;
    let rows = v.as_array().ok_or("not an array")?;
    let header: Vec<&str> = rows
        .first()
        .and_then(Value::as_array)
        .ok_or("no header row")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (t, arr, speed, dens, bz, bt) = (
        col("time_tag").ok_or("no time_tag")?,
        col("propagated_time_tag"),
        col("speed"),
        col("density"),
        col("bz"),
        col("bt"),
    );
    // The newest row with a speed or a field: the last minutes are often
    // still empty.
    Ok(rows[1..].iter().rev().find_map(|r| {
        let r = r.as_array()?;
        let get = |i: Option<usize>| i.and_then(|i| r.get(i)).and_then(num);
        let w = SolarWind {
            time_ms: parse_time(r.get(t)?.as_str()?)?,
            arrives_ms: arr
                .and_then(|i| r.get(i))
                .and_then(Value::as_str)
                .and_then(parse_time),
            speed_km_s: get(speed),
            density_cm3: get(dens),
            bz_nt: get(bz),
            bt_nt: get(bt),
        };
        (w.speed_km_s.is_some() || w.bz_nt.is_some()).then_some(w)
    }))
}

pub fn parse_flares(bytes: &[u8]) -> Result<Option<Xray>, String> {
    let v = json(bytes)?;
    let Some(r) = v.as_array().and_then(|a| a.first()) else {
        return Ok(None);
    };
    let t = |k: &str| r[k].as_str().and_then(parse_time);
    let Some(time_ms) = t("time_tag") else {
        return Ok(None);
    };
    Ok(Some(Xray {
        time_ms,
        class: r["current_class"].as_str().unwrap_or_default().to_string(),
        latest_flare: r["max_class"].as_str().map(|c| Flare {
            max_class: c.to_string(),
            begin_ms: t("begin_time"),
            max_ms: t("max_time"),
            end_ms: t("end_time"),
        }),
    }))
}

/// The headline of an SWPC message: its first "ALERT:", "WARNING:",
/// "WATCH:", "SUMMARY:", "EXTENDED WARNING:", "CONTINUED ALERT:" or
/// "CANCEL ..." line.
fn alert_title(message: &str) -> Option<String> {
    const HEADS: [&str; 7] = [
        "ALERT:",
        "WARNING:",
        "WATCH:",
        "SUMMARY:",
        "EXTENDED WARNING:",
        "CONTINUED ALERT:",
        "CANCEL",
    ];
    message
        .lines()
        .map(str::trim)
        .find(|l| HEADS.iter().any(|h| l.starts_with(h)))
        .map(str::to_string)
}

pub fn parse_alerts(bytes: &[u8]) -> Result<Vec<SpaceAlert>, String> {
    let v = json(bytes)?;
    let rows = v.as_array().ok_or("not an array")?;
    let mut out: Vec<SpaceAlert> = rows
        .iter()
        .filter_map(|r| {
            let message = r["message"].as_str()?.replace("\r\n", "\n");
            Some(SpaceAlert {
                code: r["product_id"].as_str().unwrap_or_default().to_string(),
                issued_ms: parse_time(r["issue_datetime"].as_str()?)?,
                title: alert_title(&message)?,
                message,
            })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.issued_ms));
    Ok(out)
}

pub fn parse_aurora(bytes: &[u8]) -> Result<Aurora, String> {
    let v = json(bytes)?;
    let coords = v["coordinates"].as_array().ok_or("no coordinates")?;
    let mut grid = vec![0u8; 360 * 181];
    for c in coords {
        let (Some(lon), Some(lat), Some(p)) = (c[0].as_i64(), c[1].as_i64(), c[2].as_i64()) else {
            continue;
        };
        if (0..360).contains(&lon) && (-90..=90).contains(&lat) {
            grid[lon as usize * 181 + (lat + 90) as usize] = p.clamp(0, 100) as u8;
        }
    }
    let t = |k: &str| v[k].as_str().and_then(parse_time).unwrap_or(0);
    Ok(Aurora {
        observed_ms: t("Observation Time"),
        forecast_ms: t("Forecast Time"),
        grid,
    })
}

// ---------------------------------------------------------------- the feed

/// Polls SWPC every five minutes; the aurora grid only while `aurora` is
/// set (shared with the UI).
pub struct SpaceSource {
    pub aurora: Arc<AtomicBool>,
    data: SpaceWeather,
}

impl SpaceSource {
    pub fn new(aurora: Arc<AtomicBool>) -> SpaceSource {
        SpaceSource {
            aurora,
            data: SpaceWeather::default(),
        }
    }
}

impl Source for SpaceSource {
    type Data = SpaceWeather;
    const NAME: &'static str = "space";

    fn every(&self) -> Duration {
        Duration::from_secs(300)
    }

    async fn fetch(&mut self, http: &mut Http) -> Result<Option<SpaceWeather>, String> {
        let mut next = self.data.clone();
        let mut trouble = Vec::new();
        let mut get = async |path: &str| -> Option<Vec<u8>> {
            match http
                .get(&format!("{BASE}/{path}"), "application/json")
                .await
            {
                Ok(g) => Some(g.body.to_vec()),
                Err(e) => {
                    trouble.push(format!("{path}: {e}"));
                    None
                }
            }
        };
        if let Some(b) = get("products/noaa-scales.json").await
            && let Ok((s, d)) = parse_scales(&b)
        {
            (next.scales, next.days) = (s, d);
        }
        if let Some(b) = get("products/noaa-planetary-k-index-forecast.json").await
            && let Ok(k) = parse_kp_forecast(&b)
        {
            next.kp = k;
        }
        if let Some(b) = get("json/planetary_k_index_1m.json").await
            && let Ok(k) = parse_kp_1m(&b)
        {
            next.kp_now = k;
        }
        if let Some(b) = get("products/geospace/propagated-solar-wind-1-hour.json").await
            && let Ok(w) = parse_solar_wind(&b)
        {
            next.wind = w.or(next.wind);
        }
        if let Some(b) = get("json/goes/primary/xray-flares-latest.json").await
            && let Ok(x) = parse_flares(&b)
        {
            next.xray = x.or(next.xray);
        }
        if let Some(b) = get("products/alerts.json").await
            && let Ok(a) = parse_alerts(&b)
        {
            next.alerts = a;
        }
        if self.aurora.load(Ordering::Relaxed) {
            if let Some(b) = get("json/ovation_aurora_latest.json").await
                && let Ok(a) = parse_aurora(&b)
            {
                next.aurora = Some(a);
            }
        } else {
            next.aurora = None;
        }
        if !trouble.is_empty() && next == self.data {
            return Err(trouble.join("; "));
        }
        if next == self.data {
            return Ok(None);
        }
        self.data = next.clone();
        Ok(Some(next))
    }

    fn save(&self, data: &SpaceWeather) -> Option<Vec<u8>> {
        // Without the aurora grid: it is big and goes stale fast.
        serde_json::to_vec(&SpaceWeather {
            aurora: None,
            ..data.clone()
        })
        .ok()
    }

    fn load(&self, bytes: &[u8]) -> Option<SpaceWeather> {
        serde_json::from_slice(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/../../tests/data/swpc/{name}-2026-10-29.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn times() {
        let z = crate::alerts::parse_time("2026-10-29T15:18:00Z");
        assert_eq!(parse_time("2026-10-29T15:18:00Z"), z);
        assert_eq!(parse_time("2026-10-29T15:18:00"), z);
        assert_eq!(parse_time("2026-10-29 15:18:00.100"), z);
        assert_eq!(parse_time("nope"), None);
    }

    /// Real answers from 2026-10-29 about 15:23 UTC: quiet, with a G1
    /// storm forecast for Oct 31.
    #[test]
    fn scales() {
        let (s, d) = parse_scales(&data("noaa-scales")).unwrap();
        assert_eq!(s, Scales::default());
        assert_eq!(d.len(), 3);
        assert_eq!(d[2].date, "2026-10-31");
        assert_eq!(d[2].g, 1);
        assert_eq!((d[0].r_minor, d[0].s), (Some(1), Some(1)));
        assert!(parse_scales(b"{}").is_err());
    }

    #[test]
    fn kp() {
        let k = parse_kp_forecast(&data("noaa-planetary-k-index-forecast")).unwrap();
        assert!(k.len() > 50);
        assert_eq!(k[0].kp, 2.33);
        assert_eq!(k[0].kind, KpKind::Observed);
        assert_eq!(k.last().unwrap().kind, KpKind::Predicted);
        let now = parse_time("2026-10-29T15:23:00").unwrap();
        let w = SpaceWeather {
            kp: k,
            ..Default::default()
        };
        assert!(w.kp_max_ahead(now).unwrap() >= 5.0, "the G1 forecast");
        let (t, kp) = parse_kp_1m(&data("planetary_k_index_1m")).unwrap().unwrap();
        assert!(t > now - 3_600_000);
        assert!((0.0..=9.0).contains(&kp));
        assert_eq!(g_for_kp(4.67), 0);
        assert_eq!(g_for_kp(5.0), 1);
        assert_eq!(g_for_kp(9.0), 5);
    }

    #[test]
    fn solar_wind_and_xrays() {
        let w = parse_solar_wind(&data("geospace_propagated-solar-wind-1-hour"))
            .unwrap()
            .unwrap();
        assert!(w.speed_km_s.unwrap() > 300.0);
        assert!(w.arrives_ms.unwrap() > w.time_ms);
        let x = parse_flares(&data("goes_primary_xray-flares-latest"))
            .unwrap()
            .unwrap();
        assert_eq!(x.class, "B1.9");
        let f = x.latest_flare.unwrap();
        assert_eq!(f.max_class, "B6.4");
        assert_eq!(f.max_ms, parse_time("2026-10-28T12:52:00Z"));
        assert_eq!(parse_flares(b"[]").unwrap(), None);
    }

    #[test]
    fn alerts() {
        let a = parse_alerts(&data("alerts")).unwrap();
        assert!(a.len() > 50);
        assert_eq!(a[0].code, "A20F");
        assert_eq!(a[0].title, "WATCH: Geomagnetic Storm Category G1 Predicted");
        assert!(a[0].message.contains("Oct 31:  G1 (Minor)"));
        assert!(a.windows(2).all(|w| w[0].issued_ms >= w[1].issued_ms));
    }

    #[test]
    fn aurora() {
        let a = parse_aurora(&data("ovation_aurora")).unwrap();
        assert_eq!(a.grid.len(), 360 * 181);
        assert!(a.forecast_ms > a.observed_ms);
        // Somewhere over northern Canada is likely; the tropics never.
        let best = (40..=90)
            .flat_map(|lat| (-180..0).map(move |lon| (lat, lon)))
            .map(|(lat, lon)| a.at(lat as f64, lon as f64))
            .max()
            .unwrap();
        assert!(best > 5);
        assert_eq!(a.at(20.0, -100.0), 0);
        assert_eq!(a.at(65.0, -100.0), a.at(65.0, 260.0), "lon wraps");
    }

    /// Against the real SWPC: `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_space() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut http = Http::new().unwrap();
        let mut s = SpaceSource::new(Arc::new(AtomicBool::new(true)));
        let d = rt.block_on(s.fetch(&mut http)).unwrap().unwrap();
        assert!(!d.kp.is_empty() && d.kp_now.is_some() && d.aurora.is_some());
    }
}
