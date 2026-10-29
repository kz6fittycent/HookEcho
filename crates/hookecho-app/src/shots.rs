//! `--screenshots DIR`: numbered PNGs from bundled data in both themes, so
//! screenshots never depend on live weather (KICKOFF §8).

use crate::design::tokens::ThemeKind;
use crate::headless::Offscreen;
use crate::model::{Inspect, Picker, Sheet, SheetTab};
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
    sheet: Option<SheetTab>,
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
            sheet: None,
        },
        Shot {
            name: "2-velocity",
            theme: ThemeKind::Dark,
            moment: Moment::Velocity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
            sheet: None,
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
            sheet: None,
        },
        Shot {
            name: "3b-site-picker-nearest",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 1400.0,
            picker: Picker::Site {
                query: String::new(),
                selected: 0,
                scroll: 0.0,
            },
            inspect: false,
            sheet: None,
        },
        Shot {
            name: "4-product-picker",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::Product,
            inspect: false,
            sheet: None,
        },
        Shot {
            name: "5-home-light",
            theme: ThemeKind::Light,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
            sheet: None,
        },
        Shot {
            name: "6-velocity-light",
            theme: ThemeKind::Light,
            moment: Moment::Velocity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
            sheet: None,
        },
        Shot {
            name: "7-correlation",
            theme: ThemeKind::Dark,
            moment: Moment::CorrelationCoefficient,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: true,
            sheet: None,
        },
        Shot {
            name: "8-compact",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: COMPACT,
            km_across: 56.0,
            picker: Picker::None,
            inspect: true,
            sheet: None,
        },
        Shot {
            name: "10-palettes",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::Palette,
            inspect: false,
            sheet: None,
        },
        Shot {
            name: "9-regional",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 520.0,
            picker: Picker::None,
            inspect: false,
            sheet: None,
        },
        Shot {
            name: "11-sheet-layers",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: false,
            sheet: Some(SheetTab::Layers),
        },
        Shot {
            name: "12-sheet-colors",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: false,
            sheet: Some(SheetTab::Colors),
        },
        Shot {
            name: "13-sheet-settings",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: false,
            sheet: Some(SheetTab::Settings),
        },
        Shot {
            name: "14-sheet-settings-light",
            theme: ThemeKind::Light,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::None,
            inspect: false,
            sheet: Some(SheetTab::Settings),
        },
        Shot {
            name: "15-command",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::Command {
                query: String::new(),
                selected: 0,
                scroll: 0.0,
            },
            inspect: false,
            sheet: None,
        },
        Shot {
            name: "16-command-query",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::Command {
                query: "vel".into(),
                selected: 0,
                scroll: 0.0,
            },
            inspect: false,
            sheet: None,
        },
        Shot {
            name: "17-shortcuts",
            theme: ThemeKind::Dark,
            moment: Moment::Reflectivity,
            size: SIZE,
            km_across: 70.0,
            picker: Picker::Shortcuts,
            inspect: false,
            sheet: None,
        },
    ];
    for s in shots {
        let m = &mut o.app.model;
        m.moment = s.moment;
        m.tilt_deg = 0.5;
        m.camera = Camera::looking_at(35.29, -97.42, s.km_across, s.size[0] as f64);
        m.motion.zoom_target = m.camera.scale;
        m.picker = s.picker;
        m.sheet = s.sheet.map(|tab| Sheet {
            // Deterministic: no real cache size in screenshots.
            cache_bytes: Some(184 * 1024 * 1024),
            ..Sheet::new(tab)
        });
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
    // The legend's hover readout: velocity, pointer at 70% along the bar.
    let m = &mut o.app.model;
    (m.moment, m.picker, m.sheet, m.inspect) = (Moment::Velocity, Picker::None, None, Inspect::Off);
    o.app.theme_override = Some(ThemeKind::Dark);
    o.frame(SIZE, PPP, Vec::new());
    if let Some(r) = o.ctx.read_response(eframe::egui::Id::new("legend")) {
        let p = r.rect.lerp_inside(eframe::egui::vec2(0.7, 0.5)); // design-ok: a fraction of the rect, not a size
        o.frame(SIZE, PPP, vec![eframe::egui::Event::PointerMoved(p)]);
    }
    let (w, h, px) = o.capture(SIZE, PPP);
    let path = dir.join("18-legend-hover.png");
    image::RgbaImage::from_raw(w, h, px)
        .ok_or("bad image")?
        .save(&path)
        .map_err(|e| e.to_string())?;
    println!("wrote {}", path.display());
    // Extra-large map text, regional view.
    o.frame(SIZE, PPP, vec![eframe::egui::Event::PointerGone]);
    let m = &mut o.app.model;
    m.moment = Moment::Reflectivity;
    m.config.label_scale = 1.3;
    m.camera = Camera::looking_at(35.29, -97.42, 520.0, SIZE[0] as f64);
    m.motion.zoom_target = m.camera.scale;
    let (w, h, px) = o.capture(SIZE, PPP);
    let path = dir.join("19-map-text-xl.png");
    image::RgbaImage::from_raw(w, h, px)
        .ok_or("bad image")?
        .save(&path)
        .map_err(|e| e.to_string())?;
    println!("wrote {}", path.display());
    o.app.model.config.label_scale = 1.0;
    // NWS warnings with one open (recorded answer, kept in force).
    if let Some(alerts) = crate::warnings::sample_alerts() {
        let m = &mut o.app.model;
        let at = (31.62, -104.6);
        m.layers.alerts = std::sync::Arc::new(alerts);
        m.camera = Camera::looking_at(at.0, at.1, 420.0, SIZE[0] as f64);
        m.motion.zoom_target = m.camera.scale;
        let hit = m
            .layers
            .alerts
            .iter()
            .find(|a| a.area == "Culberson, TX")
            .and_then(|a| a.rings.first())
            .map(|r| {
                let n = r.len() as f64;
                r.iter()
                    .fold((0.0, 0.0), |s, p| (s.0 + p.0 / n, s.1 + p.1 / n))
            });
        for (theme, name) in [
            (ThemeKind::Dark, "20-warnings"),
            (ThemeKind::Light, "21-warnings-light"),
        ] {
            if let Some(c) = hit {
                crate::warnings::click(&mut o.app.model, c);
            }
            o.app.theme_override = Some(theme);
            let (w, h, px) = o.capture(SIZE, PPP);
            let path = dir.join(format!("{name}.png"));
            image::RgbaImage::from_raw(w, h, px)
                .ok_or("bad image")?
                .save(&path)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", path.display());
        }
        o.app.model.picker = Picker::None;
        o.app.model.layers.alerts = Default::default();
    }
    // SPC: a categorical outlook, a tornado watch and an MD, the MD's card
    // open (recorded files, kept in force).
    if let Some(spc) = crate::spc::sample() {
        let m = &mut o.app.model;
        let md = spc.discussions[0].rings[0].clone();
        m.layers.spc = std::sync::Arc::new(spc);
        m.config.outlook = Some(hookecho_core::spc::Product::Day1Categorical.key());
        m.camera = Camera::looking_at(40.5, -84.0, 2600.0, SIZE[0] as f64);
        m.motion.zoom_target = m.camera.scale;
        let n = md.len() as f64;
        let c = md
            .iter()
            .fold((0.0, 0.0), |s, p| (s.0 + p.0 / n, s.1 + p.1 / n));
        for (theme, name) in [
            (ThemeKind::Dark, "22-spc"),
            (ThemeKind::Light, "23-spc-light"),
        ] {
            crate::warnings::click(&mut o.app.model, c);
            o.app.theme_override = Some(theme);
            let (w, h, px) = o.capture(SIZE, PPP);
            let path = dir.join(format!("{name}.png"));
            image::RgbaImage::from_raw(w, h, px)
                .ok_or("bad image")?
                .save(&path)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", path.display());
        }
        o.app.model.picker = Picker::None;
        o.app.model.config.outlook = None;
        o.app.model.layers.spc = Default::default();
    }
    // Surface observations: station plots around Oklahoma City and the
    // station panel (recorded reports, moved to now).
    if let Some(obs) = crate::stations::sample() {
        let m = &mut o.app.model;
        m.layers.obs = std::sync::Arc::new(obs);
        m.config.station_plots = true;
        m.config.station_panel = true;
        m.config.home = Some((35.47, -97.52));
        m.camera = Camera::looking_at(35.6, -97.4, 520.0, SIZE[0] as f64);
        m.motion.zoom_target = m.camera.scale;
        for (theme, name) in [
            (ThemeKind::Dark, "24-surface-obs"),
            (ThemeKind::Light, "25-surface-obs-light"),
        ] {
            o.app.theme_override = Some(theme);
            let (w, h, px) = o.capture(SIZE, PPP);
            let path = dir.join(format!("{name}.png"));
            image::RgbaImage::from_raw(w, h, px)
                .ok_or("bad image")?
                .save(&path)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", path.display());
        }
        o.app.model.config.station_plots = false;
        o.app.model.config.home = None;
    }
    // Space weather: the card open over the aurora forecast (recorded
    // SWPC answers, moved to now).
    if let Some(w) = crate::space::sample() {
        let m = &mut o.app.model;
        m.layers.space = std::sync::Arc::new(w);
        m.config.aurora = true;
        m.config.station_panel = false;
        m.camera = Camera::looking_at(60.0, -100.0, 6500.0, SIZE[0] as f64);
        m.motion.zoom_target = m.camera.scale;
        for (theme, name, card) in [
            (ThemeKind::Dark, "26-aurora", false),
            (ThemeKind::Dark, "27-space-weather", true),
            (ThemeKind::Light, "28-space-weather-light", true),
        ] {
            o.app.model.picker = if card {
                Picker::SpaceWeather { scroll: 0.0 }
            } else {
                Picker::None
            };
            o.app.theme_override = Some(theme);
            let (w, h, px) = o.capture(SIZE, PPP);
            let path = dir.join(format!("{name}.png"));
            image::RgbaImage::from_raw(w, h, px)
                .ok_or("bad image")?
                .save(&path)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", path.display());
        }
        o.app.model.picker = Picker::None;
        o.app.model.config.aurora = false;
        o.app.model.config.station_panel = true;
    }
    Ok(())
}

/// `--live-screenshot FILE [SITE]` (developer check): the real live feed,
/// offscreen. Waits up to 90 s for network data, then saves one PNG.
pub fn live(file: &Path, site: Option<&str>) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("hookecho-live-{}", std::process::id()));
    let dirs = hookecho_core::paths::Dirs {
        config: dir.join("config"),
        cache: dir.join("cache"),
    };
    let mut o = Offscreen::with_source(Some(dirs), SIZE, crate::app::Source::Live)?;
    o.app.model.reduced_motion = true;
    if let Some(s) = site.and_then(hookecho_core::sites::by_id) {
        o.app.model.select_site(s, [SIZE[0] as f64, SIZE[1] as f64]);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    while std::time::Instant::now() < deadline {
        o.frame(SIZE, 1.0, Vec::new());
        // Live data, plus a few loop frames (or 45 s of trying).
        let frames = o.app.model.looping.frames.len();
        let waited = 90
            - deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_secs();
        if let Some(l) = o.app.model.loaded_here()
            && (frames >= 3 || waited > 45)
        {
            if let Some(i) = &l.live {
                eprintln!(
                    "{}: {} chunks, complete {}, {} sweeps, {frames} loop frames",
                    i.prefix,
                    i.chunks,
                    i.complete,
                    l.volume.sweeps.len()
                );
            }
            if let Some(i) = o.app.model.current_sweep()
                && let Some(f) = l.volume.sweeps[i].field(hookecho_core::Moment::Reflectivity)
            {
                let p = o
                    .app
                    .model
                    .palettes
                    .active(&o.app.model.config, hookecho_core::Moment::Reflectivity);
                eprintln!(
                    "live sweep: {:.1} km² of reflectivity echo (under {} reads as no rain)",
                    crate::model::echo_area_km2(&l.volume.sweeps[i], f, p),
                    crate::model::QUIET_KM2
                );
            }
            // Show a loop frame so the timeline's playhead isn't at live.
            if frames >= 2 {
                o.app.model.show_pos(frames - 2);
            }
            o.app.model.camera = Camera::looking_at(
                o.app.model.site.lat,
                o.app.model.site.lon,
                460.0,
                SIZE[0] as f64,
            );
            o.app.model.motion.zoom_target = o.app.model.camera.scale;
            let (w, h, px) = o.capture(SIZE, PPP);
            image::RgbaImage::from_raw(w, h, px)
                .ok_or("bad image")?
                .save(file)
                .map_err(|e| e.to_string())?;
            println!("wrote {}", file.display());
            let _ = std::fs::remove_dir_all(&dir);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Err(format!(
        "no live data within 90 s (feed state {:?})",
        o.app.model.feed
    ))
}
