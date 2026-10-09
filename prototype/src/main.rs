//! Hook Echo, Phase 0 main-screen prototype (throwaway).
//!
//!   cargo run --release                     # window: drag to pan, scroll to zoom,
//!                                           # V velocity, T theme, I pin inspector
//!   cargo run --release -- --screenshots DIR

mod basemap;
mod design;
mod headless;
mod palettes;
mod radar;
mod scene;
mod screen;
mod view;

use design::icons::IconCache;
use design::tokens::{Theme, ThemeKind};
use eframe::egui;
use eframe::wgpu;
use radar::{Moment, Volume};
use scene::{FrameParams, Scene};
use screen::{Model, Site, Staged, Textures};
use std::collections::HashMap;
use std::path::PathBuf;
use view::Camera;

const SAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../assets/samples/KTLX20130520_201643_V06.gz"
);
/// Rough position of the Moore hook at 20:16 UTC; refined to the reflectivity
/// maximum (the debris ball) at load. "Me" is staged in Norman.
const HOOK_GUESS: (f64, f64) = (35.33, -97.53);
const ME: (f64, f64) = (35.2226, -97.4395);

fn build_model() -> (Model, Vec<basemap::LineLayer>) {
    let reg = nexrad_model::meta::registry::site_by_id("KTLX").expect("KTLX in registry");
    let site = Site {
        id: "KTLX",
        city: "Oklahoma City",
        lat: reg.latitude as f64,
        lon: reg.longitude as f64,
        elevation_m: reg.elevation_meters as f64,
    };
    let t0 = std::time::Instant::now();
    let vol = Volume::load(std::path::Path::new(SAMPLE)).unwrap_or_else(|e| {
        eprintln!("cannot load sample volume ({e}); run assets/samples/fetch.sh");
        std::process::exit(2);
    });
    let mut sweeps = HashMap::new();
    for m in [Moment::Reflectivity, Moment::Velocity] {
        let s = vol.lowest(m).expect("sweep");
        eprintln!(
            "{m:?}: elev {:.2}° radials {} gates {} gate {:.3} km first {:.3} km scale {} offset {}",
            s.elevation_deg, s.radials, s.gates, s.gate_km, s.first_gate_km, s.scale, s.offset
        );
        sweeps.insert(m, s);
    }
    eprintln!("decoded volume in {:?}", t0.elapsed());
    let hook = brightest_near(
        &sweeps[&Moment::Reflectivity],
        (site.lat, site.lon),
        HOOK_GUESS,
        10.0,
    );
    eprintln!("debris ball at {hook:?}");
    let lines = basemap::load_lines(include_bytes!("../data/basemap.bin"), (site.lat, site.lon));
    // Real KTLX volume start times leading up to the sample scan (Unidata listing).
    let frames_utc = [
        (19, 29, 49),
        (19, 34, 5),
        (19, 38, 21),
        (19, 42, 38),
        (19, 46, 55),
        (19, 51, 11),
        (19, 55, 27),
        (19, 59, 41),
        (20, 3, 56),
        (20, 8, 11),
        (20, 12, 29),
        (20, 16, 43),
    ]
    .map(|(h, m, s)| h * 3600 + m * 60 + s)
    .to_vec();
    let frame = frames_utc.len() - 1;
    let model = Model {
        site,
        staged: Staged {
            frames_utc,
            age_text: "1 min ago",
            utc_offset_h: -5,
            zone: "CDT",
        },
        sweeps,
        places: basemap::load_places(include_str!("../data/places.tsv")),
        moment: Moment::Reflectivity,
        camera: Camera::looking_at(35.30, -97.40, 70.0, 1280.0),
        me: Some(ME),
        inspect: Some(hook),
        playing: false,
        frame,
    };
    (model, lines)
}

/// Location of the strongest reflectivity within `radius_km` of `guess`.
fn brightest_near(
    s: &radar::SweepData,
    site: (f64, f64),
    guess: (f64, f64),
    radius_km: f64,
) -> (f64, f64) {
    let deg_lat = radius_km / 111.0;
    let deg_lon = deg_lat / guess.0.to_radians().cos();
    let mut best = (0u8, guess);
    for i in -40..=40 {
        for j in -40..=40 {
            let p = (
                guess.0 + deg_lat * i as f64 / 40.0,
                guess.1 + deg_lon * j as f64 / 40.0,
            );
            let (r, b) = view::range_bearing(site, p);
            if let Some(raw) = s.raw_at(r as f32, b as f32) {
                if raw > best.0 {
                    best = (raw, p);
                }
            }
        }
    }
    best.1
}

fn upload(
    scene: &mut Scene,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    model: &Model,
    lines: &[basemap::LineLayer],
) {
    for s in model.sweeps.values() {
        scene.upload_sweep(device, queue, s, &palettes::lut(s));
    }
    scene.upload_basemap(device, lines);
}

fn screenshots(dir: PathBuf) {
    let (mut model, lines) = build_model();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .expect("GPU adapter");
    eprintln!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("device");
    let mut scene = Scene::new(&device);
    upload(&mut scene, &device, &queue, &model, &lines);

    let wide = Camera::looking_at(35.30, -97.40, 70.0, 1280.0);
    let compact = Camera::looking_at(35.30, -97.42, 56.0, 900.0);
    let shots = [
        (
            "1-home-dark",
            ThemeKind::Dark,
            Moment::Reflectivity,
            [1280.0, 800.0],
            wide,
        ),
        (
            "2-velocity-dark",
            ThemeKind::Dark,
            Moment::Velocity,
            [1280.0, 800.0],
            wide,
        ),
        (
            "3-home-light",
            ThemeKind::Light,
            Moment::Reflectivity,
            [1280.0, 800.0],
            wide,
        ),
        (
            "4-velocity-light",
            ThemeKind::Light,
            Moment::Velocity,
            [1280.0, 800.0],
            wide,
        ),
        (
            "5-compact-900x600-dark",
            ThemeKind::Dark,
            Moment::Reflectivity,
            [900.0, 600.0],
            compact,
        ),
    ]
    .map(|(name, theme, moment, size_pts, camera)| headless::Shot {
        name,
        theme,
        moment,
        size_pts,
        ppp: 2.0,
        camera,
        inspect: model.inspect,
    });
    headless::run(&mut model, &mut scene, &device, &queue, &shots, &dir);
}

struct App {
    model: Model,
    theme: ThemeKind,
    icons: IconCache,
    scene: Scene,
    tex: Option<Textures>,
    pinned: bool,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let half = [screen.center().x as f64, screen.center().y as f64];
        ctx.input(|i| {
            if i.pointer.primary_down() && i.pointer.delta() != egui::Vec2::ZERO {
                let d = i.pointer.delta();
                self.model.camera.center[0] -= d.x as f64 / self.model.camera.scale;
                self.model.camera.center[1] -= d.y as f64 / self.model.camera.scale;
            }
            let scroll = i.smooth_scroll_delta.y + (i.zoom_delta() - 1.0) * 200.0;
            if scroll != 0.0 {
                if let Some(p) = i.pointer.hover_pos() {
                    self.model.camera.zoom_at(
                        (scroll as f64 / 300.0).exp(),
                        [p.x as f64, p.y as f64],
                        half,
                    );
                }
            }
            if i.key_pressed(egui::Key::V) {
                self.model.moment = match self.model.moment {
                    Moment::Reflectivity => Moment::Velocity,
                    Moment::Velocity => Moment::Reflectivity,
                };
            }
            if i.key_pressed(egui::Key::T) {
                self.theme = match self.theme {
                    ThemeKind::Dark => ThemeKind::Light,
                    ThemeKind::Light => ThemeKind::Dark,
                };
            }
            if i.key_pressed(egui::Key::I) {
                self.pinned = !self.pinned;
            }
            if i.key_pressed(egui::Key::Space) {
                self.model.playing = !self.model.playing;
            }
            if !self.pinned && !i.pointer.primary_down() {
                self.model.inspect = i
                    .pointer
                    .hover_pos()
                    .map(|p| screen::unproject(&self.model.camera, screen, p));
            }
        });

        let rs = frame.wgpu_render_state().expect("wgpu backend");
        let ppp = ctx.pixels_per_point();
        let size_px = [
            (screen.width() * ppp).round() as u32,
            (screen.height() * ppp).round() as u32,
        ];
        let changed = self.scene.ensure_targets(&rs.device, size_px);
        {
            let t = self.scene.targets.as_ref().unwrap();
            let mut r = rs.renderer.write();
            match &self.tex {
                None => {
                    self.tex = Some(Textures {
                        scene: r.register_native_texture(
                            &rs.device,
                            &t.scene_view,
                            wgpu::FilterMode::Linear,
                        ),
                        blur: r.register_native_texture(
                            &rs.device,
                            &t.blur_view,
                            wgpu::FilterMode::Linear,
                        ),
                    });
                }
                Some(ids) if changed => {
                    r.update_egui_texture_from_wgpu_texture(
                        &rs.device,
                        &t.scene_view,
                        wgpu::FilterMode::Linear,
                        ids.scene,
                    );
                    r.update_egui_texture_from_wgpu_texture(
                        &rs.device,
                        &t.blur_view,
                        wgpu::FilterMode::Linear,
                        ids.blur,
                    );
                }
                _ => {}
            }
        }
        let theme = Theme::get(self.theme);
        let mut enc = rs.device.create_command_encoder(&Default::default());
        self.scene.render(
            &rs.queue,
            &mut enc,
            &FrameParams {
                size_px,
                ppp,
                camera: self.model.camera,
                radar_latlon: (self.model.site.lat, self.model.site.lon),
                moment: self.model.moment,
                theme: &theme,
            },
        );
        rs.queue.submit([enc.finish()]);
        screen::paint(
            &ctx,
            &self.model,
            &theme,
            &mut self.icons,
            self.tex.as_ref().unwrap(),
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--screenshots") {
        let dir = args
            .get(i + 1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("screenshots"));
        screenshots(dir);
        return;
    }
    let (model, lines) = build_model();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hook Echo (prototype)")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "hookecho-prototype",
        options,
        Box::new(move |cc| {
            design::install_fonts(&cc.egui_ctx);
            let rs = cc.wgpu_render_state.as_ref().expect("wgpu backend");
            let mut scene = Scene::new(&rs.device);
            upload(&mut scene, &rs.device, &rs.queue, &model, &lines);
            Ok(Box::new(App {
                model,
                theme: ThemeKind::Dark,
                icons: IconCache::default(),
                scene,
                tex: None,
                pinned: false,
            }))
        }),
    )
    .expect("run app");
}
