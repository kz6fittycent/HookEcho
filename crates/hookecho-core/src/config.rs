//! User settings, stored as TOML in the config directory. Unknown or missing
//! keys fall back to defaults so old files never break newer versions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePref {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Imperial,
    Metric,
}

/// Wind speed units for velocity and spectrum width. `Auto` follows the
/// distance units: knots with miles (as chasers read velocity), m/s with
/// kilometers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    #[default]
    Auto,
    Knots,
    Mph,
    Kmh,
    Ms,
}

impl Speed {
    /// (label, factor from m/s) with `Auto` resolved against `units`.
    pub fn resolve(self, units: Units) -> (&'static str, f32) {
        match (self, units) {
            (Speed::Knots, _) | (Speed::Auto, Units::Imperial) => ("kt", 1.943_844),
            (Speed::Mph, _) => ("mph", 2.236_936),
            (Speed::Kmh, _) => ("km/h", 3.6),
            (Speed::Ms, _) | (Speed::Auto, Units::Metric) => ("m/s", 1.0),
        }
    }
}

/// Basemap layers the user can hide (side sheet → Layers).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MapLayers {
    pub counties: bool,
    pub roads: bool,
    pub labels: bool,
}

impl Default for MapLayers {
    fn default() -> Self {
        MapLayers {
            counties: true,
            roads: true,
            labels: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemePref,
    pub units: Units,
    pub reduced_motion: bool,
    /// Last viewed site; the app reopens on it.
    pub site: Option<String>,
    /// Home point (lat, lon), set from the map. A first run opens on the
    /// radar nearest to it, and "Nearest radar" measures from it.
    pub home: Option<(f64, f64)>,
    pub favorites: Vec<String>,
    /// Frames in the loop, including the live one (10, 30 or 50).
    pub loop_frames: usize,
    /// Loop speed multiplier (0.5, 1 or 2).
    pub loop_speed: f32,
    /// Chosen palette per product (product code, e.g. "REF" → palette name).
    pub palettes: BTreeMap<String, String>,
    /// Show times in UTC instead of local time.
    pub utc: bool,
    pub map: MapLayers,
    pub speed: Speed,
    /// Hide CC, ZDR and PHI where there's no real echo (clear-air noise).
    pub mask_dualpol: bool,
    /// Map label size multiplier (0.85, 1, 1.15 or 1.3).
    pub label_scale: f32,
    /// Offer the FAA's airport radars (TDWR) in the picker and on the map.
    pub tdwr: bool,
    /// Show an airport radar's long-range reflectivity (225 nm, 300 m gates)
    /// instead of its short-range one (48 nm, 150 m gates).
    pub tdwr_long_range: bool,
    /// Draw NWS warnings on the map (and poll for them).
    pub warnings: bool,
    /// Warning types switched off, by `alerts::Event::key`.
    pub hidden_warnings: Vec<String>,
    /// Draw SPC tornado and severe thunderstorm watches.
    pub watches: bool,
    /// Draw SPC mesoscale discussions.
    pub discussions: bool,
    /// The SPC outlook drawn under the radar, by `spc::Product::key`; none
    /// by default.
    pub outlook: Option<String>,
    /// Station plots (temperature, dewpoint, wind) on the map.
    pub station_plots: bool,
    /// The station panel: the nearest station's conditions.
    pub station_panel: bool,
    /// The OVATION aurora forecast on the map.
    pub aurora: bool,
    /// The GOES band drawn under the radar, by `goes::Band::key`; none by
    /// default.
    pub satellite: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: ThemePref::Dark,
            units: Units::Imperial,
            reduced_motion: false,
            site: None,
            home: None,
            favorites: Vec::new(),
            loop_frames: 10,
            loop_speed: 1.0,
            palettes: BTreeMap::new(),
            utc: false,
            map: MapLayers::default(),
            speed: Speed::Auto,
            mask_dualpol: true,
            label_scale: 1.0,
            tdwr: true,
            tdwr_long_range: false,
            warnings: true,
            hidden_warnings: Vec::new(),
            watches: true,
            discussions: true,
            outlook: None,
            station_plots: false,
            station_panel: true,
            aurora: false,
            satellite: None,
        }
    }
}

impl Config {
    /// Loads `path`, or returns defaults if it is missing or unreadable.
    pub fn load(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes atomically (temp file + rename) so a crash never truncates it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_units() {
        assert_eq!(Speed::Auto.resolve(Units::Imperial).0, "kt");
        assert_eq!(Speed::Auto.resolve(Units::Metric).0, "m/s");
        assert_eq!(Speed::Mph.resolve(Units::Metric), ("mph", 2.236_936));
    }

    #[test]
    fn round_trip_and_partial_files() {
        let dir = std::env::temp_dir().join(format!("hookecho-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        let c = Config {
            site: Some("KTLX".into()),
            home: Some((33.1, -96.75)),
            favorites: vec!["KFWS".into()],
            ..Default::default()
        };
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p), c);
        std::fs::write(&p, "theme = \"light\"\nunknown_key = 3\n").unwrap();
        let partial = Config::load(&p);
        assert_eq!(partial.theme, ThemePref::Light);
        assert_eq!(partial.site, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
