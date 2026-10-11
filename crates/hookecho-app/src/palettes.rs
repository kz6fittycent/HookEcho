//! The palette library: built-in palettes per product plus imported GRLevelX
//! color tables, kept as `<config>/palettes/<CODE>-<name>.pal`. The choice
//! per product lives in the config (`palettes`, product code → name).

use hookecho_core::Moment;
use hookecho_core::config::Config;
use hookecho_core::pal;
use hookecho_render::palette::Palette;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Choice {
    pub palette: Palette,
    pub imported: bool,
}

pub struct Library {
    choices: HashMap<Moment, Vec<Choice>>,
    /// Bumped on every change, so the app knows to re-upload LUTs.
    pub version: u64,
    /// The config directory imports are saved in (None: not persisted).
    config_dir: Option<PathBuf>,
}

/// Longest imported file accepted (real tables are a few KB).
const MAX_FILE: usize = 256 * 1024;

fn dir(config_dir: &Path) -> PathBuf {
    config_dir.join("palettes")
}

/// A file-name-safe palette name from a file name.
fn clean_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(s, _)| s)
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("");
    let s: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Imported".into() } else { s }
}

impl Library {
    /// Built-ins plus every readable table in the config directory.
    pub fn load(config_dir: Option<&Path>) -> Library {
        let mut lib = Library {
            choices: Moment::ALL
                .iter()
                .map(|&m| {
                    let v = Palette::builtins(m)
                        .into_iter()
                        .map(|palette| Choice {
                            palette,
                            imported: false,
                        })
                        .collect();
                    (m, v)
                })
                .collect(),
            version: 0,
            config_dir: config_dir.map(Path::to_path_buf),
        };
        let Some(rd) = config_dir.and_then(|d| std::fs::read_dir(dir(d)).ok()) else {
            return lib;
        };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for p in files {
            let Some(file) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some((_, name)) = file.strip_suffix(".pal").and_then(|s| s.split_once('-')) else {
                continue;
            };
            if let Ok(text) = std::fs::read_to_string(&p)
                && let Ok(t) = pal::parse(&text)
                && let Some(m) = t.product
            {
                lib.add(m, Palette::from_table(name, m, &t));
            }
        }
        lib
    }

    fn add(&mut self, m: Moment, palette: Palette) {
        let v = self.choices.entry(m).or_default();
        v.retain(|c| !(c.imported && c.palette.name == palette.name));
        v.push(Choice {
            palette,
            imported: true,
        });
        self.version += 1;
    }

    pub fn choices(&self, m: Moment) -> &[Choice] {
        self.choices.get(&m).map_or(&[], |v| v.as_slice())
    }

    /// The chosen palette for a moment, or its default.
    pub fn active<'a>(&'a self, config: &Config, m: Moment) -> &'a Palette {
        let v = self.choices(m);
        config
            .palettes
            .get(m.code())
            .and_then(|name| v.iter().find(|c| &c.palette.name == name))
            .or(v.first())
            .map(|c| &c.palette)
            .expect("every moment has a default palette")
    }

    pub fn select(&mut self, config: &mut Config, m: Moment, name: &str) {
        if self.active(config, m).name != name {
            config.palettes.insert(m.code().into(), name.into());
            self.version += 1;
        }
    }

    /// Removes an imported palette (and its saved file). A product using it
    /// goes back to its default.
    pub fn remove(&mut self, config: &mut Config, m: Moment, name: &str) -> Result<(), String> {
        let v = self.choices.entry(m).or_default();
        let before = v.len();
        v.retain(|c| !(c.imported && c.palette.name == name));
        if v.len() == before {
            return Err(format!("no imported palette named {name}"));
        }
        if config.palettes.get(m.code()).is_some_and(|n| n == name) {
            config.palettes.remove(m.code());
        }
        self.version += 1;
        if let (Some(d), Some(code)) = (&self.config_dir, pal::product_code(m)) {
            let f = dir(d).join(format!("{code}-{name}.pal"));
            match std::fs::remove_file(&f) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(format!("{}: {e}", f.display()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Every product back to its built-in default (imports stay listed).
    pub fn restore_defaults(&mut self, config: &mut Config) {
        if !config.palettes.is_empty() {
            config.palettes.clear();
            self.version += 1;
        }
    }

    /// Imports a dropped color table. Files without `Product:` go to
    /// `fallback` (the product on screen). Saves a copy when `config_dir`
    /// is set, selects it, and returns (product, palette name).
    pub fn import(
        &mut self,
        config: &mut Config,
        config_dir: Option<&Path>,
        file_name: &str,
        bytes: &[u8],
        fallback: Moment,
    ) -> Result<(Moment, String), String> {
        if bytes.len() > MAX_FILE {
            return Err("file is too large for a color table".into());
        }
        let text = String::from_utf8_lossy(bytes);
        let t = pal::parse(&text).map_err(|e| e.to_string())?;
        let m = t.product.unwrap_or(fallback);
        let code = pal::product_code(m).ok_or("this product has no color tables")?;
        let mut name = clean_name(file_name);
        if self
            .choices(m)
            .iter()
            .any(|c| !c.imported && c.palette.name == name)
        {
            name.push_str(" (imported)");
        }
        if let Some(d) = config_dir {
            let d = dir(d);
            std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
            // The product goes first; a later Product: line can't undo it.
            let saved = format!("Product: {code}\n{text}");
            std::fs::write(d.join(format!("{code}-{name}.pal")), saved)
                .map_err(|e| e.to_string())?;
        }
        self.add(m, Palette::from_table(&name, m, &t));
        self.select(config, m, &name);
        Ok((m, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_select_and_reload() {
        let base = std::env::temp_dir().join(format!("hookecho-pal-{}", std::process::id()));
        let mut config = Config::default();
        let mut lib = Library::load(Some(&base));
        assert_eq!(
            lib.active(&config, Moment::Velocity),
            &Palette::default_for(Moment::Velocity)
        );
        // No Product: line, so it goes to the product on screen.
        let (m, name) = lib
            .import(
                &mut config,
                Some(&base),
                "/home/x/My Vel!.pal",
                b"Units: KTS\nColor: -50 0 0 255\nColor: 50 255 0 0\n",
                Moment::Velocity,
            )
            .unwrap();
        assert_eq!((m, name.as_str()), (Moment::Velocity, "My Vel_"));
        assert_eq!(lib.active(&config, m).name, "My Vel_");
        let v = lib.version;
        lib.select(&mut config, m, "My Vel_");
        assert_eq!(lib.version, v, "re-selecting the same palette is no change");

        let reloaded = Library::load(Some(&base));
        {
            // Removing deletes the file and falls back to the default.
            let mut c = config.clone();
            let mut l = Library::load(Some(&base));
            l.remove(&mut c, m, "My Vel_").unwrap();
            assert_eq!(l.active(&c, m), &Palette::default_for(Moment::Velocity));
            assert!(
                l.remove(&mut c, m, "Hook Echo Diverging").is_err(),
                "built-ins stay"
            );
            assert_eq!(
                Library::load(Some(&base))
                    .choices(m)
                    .iter()
                    .filter(|c| c.imported)
                    .count(),
                0
            );
            // Put it back for the checks below.
            lib.import(
                &mut config,
                Some(&base),
                "/home/x/My Vel!.pal",
                b"Units: KTS\nColor: -50 0 0 255\nColor: 50 255 0 0\n",
                Moment::Velocity,
            )
            .unwrap();
        }
        let mut restored = (config.clone(), Library::load(None));
        restored.1.restore_defaults(&mut restored.0);
        assert_eq!(
            restored.1.active(&restored.0, m),
            &Palette::default_for(Moment::Velocity)
        );
        assert_eq!(reloaded.active(&config, m), lib.active(&config, m));
        assert_eq!(reloaded.choices(m).iter().filter(|c| c.imported).count(), 1);

        // Same name as a built-in.
        let (m, name) = lib
            .import(
                &mut config,
                None,
                "Grayscale.pal",
                b"Product: BR\nColor: 0 0 0 0\nColor: 70 255 255 255\n",
                Moment::Velocity,
            )
            .unwrap();
        assert_eq!(
            (m, name.as_str()),
            (Moment::Reflectivity, "Grayscale (imported)")
        );
        assert!(
            lib.import(&mut config, None, "x.pal", b"nothing", Moment::Reflectivity)
                .is_err()
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
