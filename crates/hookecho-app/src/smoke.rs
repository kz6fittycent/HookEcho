//! `--smoke-test` (KICKOFF §8): start, load the bundled sample volume, render
//! one frame offscreen, check the config and cache dirs were created, exit 0.

use crate::headless::Offscreen;
use hookecho_core::paths::Dirs;

pub fn run() -> Result<(), String> {
    let dirs = Dirs::resolve().ok_or("no home directory")?;
    dirs.create()
        .map_err(|e| format!("creating {}: {e}", dirs.config.display()))?;
    let mut o = Offscreen::new(Some(dirs.clone()), [1280.0, 800.0])?;
    o.app.wait_for_data();
    let (_, _, px) = o.capture([1280.0, 800.0], 1.0);
    if !o.app.first_radar_frame {
        return Err("no radar frame was drawn".into());
    }
    // A blank or single-color frame means rendering silently failed.
    let first = &px[..4];
    if px.as_chunks::<4>().0.iter().all(|p| p == first) {
        return Err("rendered frame is a single color".into());
    }
    o.app.save_config();
    for d in [&dirs.config, &dirs.cache] {
        if !d.is_dir() {
            return Err(format!("{} was not created", d.display()));
        }
    }
    if !dirs.config_file().is_file() {
        return Err(format!("{} was not written", dirs.config_file().display()));
    }
    println!(
        "smoke test passed on {} (config {}, cache {})",
        o.adapter_name(),
        dirs.config.display(),
        dirs.cache.display()
    );
    Ok(())
}
