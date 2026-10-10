//! Per-user directories (XDG on Linux, the platform conventions elsewhere).
//! Under snap confinement `$HOME` is already the snap's private home.

use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Dirs {
    pub config: PathBuf,
    pub cache: PathBuf,
}

impl Dirs {
    /// Resolves the standard directories. `HOOKECHO_HOME` overrides the
    /// base for tests and CI (config/ and cache/ beneath it).
    pub fn resolve() -> Option<Dirs> {
        if let Some(base) = std::env::var_os("HOOKECHO_HOME") {
            let base = PathBuf::from(base);
            return Some(Dirs {
                config: base.join("config"),
                cache: base.join("cache"),
            });
        }
        let p = ProjectDirs::from("io.github", "kz6fittycent", "HookEcho")?;
        Some(Dirs {
            config: p.config_dir().to_path_buf(),
            cache: p.cache_dir().to_path_buf(),
        })
    }

    pub fn create(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config)?;
        std::fs::create_dir_all(&self.cache)
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }
}
