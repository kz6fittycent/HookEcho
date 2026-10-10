//! User settings, stored as TOML in the config directory. Unknown or missing
//! keys fall back to defaults so old files never break newer versions.

use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemePref,
    pub units: Units,
    pub reduced_motion: bool,
    /// Last viewed site; the app reopens on it.
    pub site: Option<String>,
    pub favorites: Vec<String>,
    /// Frames in the loop, including the live one (10, 30 or 50).
    pub loop_frames: usize,
    /// Loop speed multiplier (0.5, 1 or 2).
    pub loop_speed: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: ThemePref::Dark,
            units: Units::Imperial,
            reduced_motion: false,
            site: None,
            favorites: Vec::new(),
            loop_frames: 10,
            loop_speed: 1.0,
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
    fn round_trip_and_partial_files() {
        let dir = std::env::temp_dir().join(format!("hookecho-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        let c = Config {
            site: Some("KTLX".into()),
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
