//! `--screenshots DIR`: numbered PNGs from bundled data in both themes, so
//! screenshots never depend on live weather (KICKOFF §8).

use crate::design::tokens::ThemeKind;
use crate::headless::Offscreen;
use crate::model::{Inspect, Picker};
use hookecho_core::Moment;
use hookecho_core::geo::Camera;
use std::path::Path;

const SIZE: [f32; 2] = [1280.0, 800.0];
const COMPACT: [f32; 2] = [900.0, 600.0];
const PPP: f32 = 2.0;

/// The Moore debris ball: strongest 0.5° reflectivity near the hook.
fn debris_ball(o: &Offscreen) -> (f64, f64) {
    let Some(l) = o.app.model.loaded_here() else {
        return (35.32, -97.52);
    };
    let Some(sweep) = l.sweep_for(Moment::Reflectivity, 0.5) else {
        return (35.32, -97.52);
    };
    let guess = (35.33, -97.53);
    let mut best = (f32::MIN, guess);
    for i in -40..=40 {
        for j in -40..=40 {
            let p = (guess.0 + i as f64 * 0.0025, guess.1 + j as f64 * 0.003);
            if let Some(v) = l.probe(sweep, Moment::Reflectivity, p).value
                && v > best.0
            {
                best = (v, p);
            }
        }
    }
    best.1
}

struct Shot {
    name: &'static str,
    theme: ThemeKind,
    moment: Moment,
    size: [f32; 2],
    km_across: f64,
    picker: Picker,
    inspect: bool,
}

pub fn run(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Screenshots never touch the user's real config.
    let mut o = Offscreen::new(None, SIZE)?;
    eprintln!("adapter: {}", o.adapter_name());
    o.app.wait_for_data();
    o.app.model.reduced_motion = true;
    let ball = debris_ball(&o);
    let shots = [
        Shot {
            name: "1-home",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
        },
        Shot {
            name: "2-velocity",
            theme: ThemeKind::Dark,
            moment: Moment::Velocity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
        },
        Shot {
            name: "3-site-picker",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 1400.0,
            picker: Picker::Site {
                query: "ok".into(),
                selected: 1,
                scroll: 0.0,
            },
            inspect: false,
        },
        Shot {
            name: "4-product-picker",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::Product { row: 4, col: 0 },
            inspect: false,
        },
        Shot {
            name: "5-home-light",
            theme: ThemeKind::Light,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
        },
        Shot {
            name: "6-velocity-light",
            theme: ThemeKind::Light,
            moment: Moment::Velocity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
        },
        Shot {
            name: "7-correlation",
            theme: ThemeKind::Dark,
            moment: Moment::CorrelationCoefficient,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
        },
        Shot {
            name: "8-compact",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: COMPACT,
            km_across: 56.0,
            picker: Picker::None,
            inspect: true,
        },
        Shot {
            name: "9-regional",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 520.0,
            picker: Picker::None,
            inspect: false,
        },
    ];
    for s in shots {
        let m = &mut o.app.model;
        m.moment = s.moment;
        m.tilt_deg = 0.5;
        m.camera = Camera::looking_at(35.29, -97.42, s.km_across, s.size[0] as f64);
        m.motion.zoom_target = m.camera.scale;
        m.picker = s.picker;
        m.inspect = if s.inspect {
            Inspect::Pinned(ball)
        } else {
            Inspect::Off
        };
        o.app.theme_override = Some(s.theme);
        let (w, h, px) = o.capture(s.size, PPP);
        let path = dir.join(format!("{}.png", s.name));
        image::RgbaImage::from_raw(w, h, px)
            .ok_or("bad image")?
            .save(&path)
            .map_err(|e| e.to_string())?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
