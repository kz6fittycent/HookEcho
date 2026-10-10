//! The application: model + renderer + design kit, drawn the same way in a
//! window (eframe) and offscreen (screenshots, smoke test, bench).

use crate::design::components::Kit;
use crate::design::icons::IconCache;
use crate::design::tokens::{Theme, ThemeKind};
use crate::model::{self, Loaded, Model};
use crate::{assets, design, screen};
use eframe::egui::{self, TextureId};
use hookecho_core::basemap;
use hookecho_core::config::{Config, ThemePref};
use hookecho_core::level2::Volume;
use hookecho_core::paths::Dirs;
use hookecho_core::sites;
use hookecho_render::{Frame, Renderer};
use std::sync::mpsc;

pub struct Gpu<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

pub struct App {
    pub model: Model,
    pub renderer: Renderer,
    icons: IconCache,
    textures: Option<(TextureId, TextureId)>,
    pending: Option<mpsc::Receiver<Result<Volume, String>>>,
    pub dirs: Option<Dirs>,
    pub theme_override: Option<ThemeKind>,
    /// Set once the first radar frame has been drawn (for cold-start timing).
    pub first_radar_frame: bool,
}

impl App {
    /// Builds the app on an existing device. Decoding of the bundled sample
    /// starts on a background thread; the map is usable immediately.
    pub fn new(gpu: &Gpu, dirs: Option<Dirs>, size: [f64; 2]) -> App {
        let config = dirs
            .as_ref()
            .map(|d| Config::load(&d.config_file()))
            .unwrap_or_default();
        let initial = config
            .site
            .as_deref()
            .and_then(sites::by_id)
            .unwrap_or_else(|| sites::by_id(assets::SAMPLE_SITE).expect("sample site"));
        let map = basemap::decode(assets::BASEMAP).expect("bundled basemap");
        crate::bench::trace("basemap decoded");
        let renderer = Renderer::new(gpu.device, gpu.queue, &map, design::map_label_fonts());
        drop(map);
        crate::bench::trace("renderer");
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("decode".into())
            .spawn(move || {
                let _ = tx.send(
                    hookecho_core::level2::decode_volume(assets::SAMPLE_VOLUME)
                        .map_err(|e| e.to_string()),
                );
            })
            .expect("spawn decoder");
        App {
            model: Model::new(config, initial, size),
            renderer,
            icons: IconCache::default(),
            textures: None,
            pending: Some(rx),
            dirs,
            theme_override: None,
            first_radar_frame: false,
        }
    }

    /// Blocks until the bundled sample is decoded (offscreen modes).
    pub fn wait_for_data(&mut self) {
        if let Some(rx) = self.pending.take()
            && let Ok(r) = rx.recv()
        {
            self.accept(r);
        }
    }

    fn accept(&mut self, r: Result<Volume, String>) {
        match r {
            Ok(v) => {
                crate::bench::trace("volume decoded");
                let site = sites::by_id(assets::SAMPLE_SITE).expect("sample site");
                self.model.loaded = Some(Loaded::new(site, v, true));
            }
            Err(e) => eprintln!("hookecho: sample volume: {e}"),
        }
    }

    fn poll_data(&mut self) {
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(r) => {
                    self.pending = None;
                    self.accept(r);
                }
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub fn theme_kind(&self, ctx: &egui::Context) -> ThemeKind {
        if let Some(t) = self.theme_override {
            return t;
        }
        match self.model.config.theme {
            ThemePref::Dark => ThemeKind::Dark,
            ThemePref::Light => ThemeKind::Light,
            ThemePref::System => match ctx.system_theme() {
                Some(egui::Theme::Light) => ThemeKind::Light,
                _ => ThemeKind::Dark,
            },
        }
    }

    /// One frame: input → scene render → UI. `egui_renderer` is where the
    /// scene textures get registered for the UI to draw.
    pub fn ui(&mut self, ui: &mut egui::Ui, gpu: &Gpu, egui_renderer: &mut egui_wgpu::Renderer) {
        self.poll_data();
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let theme = Theme::get(self.theme_kind(&ctx));
        screen::keys(ui, &mut self.model, screen);
        let animating = screen::map_input(ui, &mut self.model, screen);
        if animating || self.pending.is_some() {
            ctx.request_repaint();
        }

        // Scene.
        let ppp = ctx.pixels_per_point();
        let size_px = [
            (screen.width() * ppp).round().max(1.0) as u32,
            (screen.height() * ppp).round().max(1.0) as u32,
        ];
        let changed = self.renderer.ensure_targets(gpu.device, size_px);
        self.register_textures(gpu, egui_renderer, changed);
        let radar = match self.model.current_sweep() {
            Some(i) => {
                let m = self.model.moment;
                model::ensure_upload(
                    &mut self.model,
                    &mut self.renderer,
                    gpu.device,
                    gpu.queue,
                    i,
                    m,
                )
                .map(|id| (id, (self.model.site.lat, self.model.site.lon)))
            }
            None => None,
        };
        if radar.is_some() {
            self.first_radar_frame = true;
        }
        let style = theme.scene_style();
        let exclusions = screen::label_exclusions(&self.model, screen);
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        self.renderer.render(
            gpu.device,
            gpu.queue,
            &mut enc,
            &Frame {
                size_px,
                ppp,
                camera: self.model.camera,
                radar,
                style: &style,
                label_exclusions: &exclusions,
            },
        );
        gpu.queue.submit([enc.finish()]);
        self.renderer.end_frame();

        // UI.
        let (scene, blur) = self.textures.expect("registered");
        let mut kit = Kit {
            theme: &theme,
            icons: &mut self.icons,
            blur,
            screen,
        };
        let before = (
            self.model.site.id.clone(),
            self.model.config.favorites.clone(),
        );
        screen::paint(ui, &mut kit, &mut self.model, scene);
        if before
            != (
                self.model.site.id.clone(),
                self.model.config.favorites.clone(),
            )
        {
            self.save_config();
        }
    }

    fn register_textures(&mut self, gpu: &Gpu, r: &mut egui_wgpu::Renderer, changed: bool) {
        let t = self.renderer.targets.as_ref().expect("targets");
        match self.textures {
            None => {
                let scene =
                    r.register_native_texture(gpu.device, &t.scene_view, wgpu::FilterMode::Linear);
                let blur =
                    r.register_native_texture(gpu.device, &t.blur_view, wgpu::FilterMode::Linear);
                self.textures = Some((scene, blur));
            }
            Some((scene, blur)) if changed => {
                r.update_egui_texture_from_wgpu_texture(
                    gpu.device,
                    &t.scene_view,
                    wgpu::FilterMode::Linear,
                    scene,
                );
                r.update_egui_texture_from_wgpu_texture(
                    gpu.device,
                    &t.blur_view,
                    wgpu::FilterMode::Linear,
                    blur,
                );
            }
            _ => {}
        }
    }

    pub fn save_config(&self) {
        if let Some(d) = &self.dirs
            && let Err(e) = d
                .create()
                .and_then(|_| self.model.config.save(&d.config_file()))
        {
            eprintln!("hookecho: saving config: {e}");
        }
    }
}

/// eframe wrapper for the windowed app.
pub struct Windowed {
    pub app: Option<App>,
}

impl eframe::App for Windowed {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let rs = frame.wgpu_render_state().expect("wgpu backend").clone();
        let gpu = Gpu {
            device: &rs.device,
            queue: &rs.queue,
        };
        if self.app.is_none() {
            let r = ui.ctx().content_rect();
            self.app = Some(App::new(
                &gpu,
                Dirs::resolve(),
                [r.width() as f64, r.height() as f64],
            ));
        }
        let app = self.app.as_mut().unwrap();
        let mut renderer = rs.renderer.write();
        app.ui(ui, &gpu, &mut renderer);
    }

    fn on_exit(&mut self) {
        if let Some(a) = &self.app {
            a.save_config();
        }
    }
}
