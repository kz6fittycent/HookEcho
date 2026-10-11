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

/// Where radar data comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    /// The bundled sample volume, offline and deterministic (screenshots,
    /// smoke test, bench).
    Sample,
    /// NOAA's real-time feed (the windowed app).
    Live,
}

/// Lets the feed thread wake the UI once egui's context exists.
type Waker = std::sync::Arc<std::sync::OnceLock<egui::Context>>;

pub struct App {
    pub model: Model,
    feed: Option<hookecho_core::live::Feed>,
    frames: Option<hookecho_core::frames::LoopLoader>,
    waker: Waker,
    watching: Option<String>,
    pub renderer: Renderer,
    icons: IconCache,
    textures: Option<(TextureId, TextureId)>,
    pending: Option<mpsc::Receiver<Result<Volume, String>>>,
    pub dirs: Option<Dirs>,
    pub theme_override: Option<ThemeKind>,
    /// Set once the first radar frame has been drawn (for cold-start timing).
    pub first_radar_frame: bool,
    /// Palette library version last uploaded to the renderer.
    palettes_applied: u64,
    /// The dual-pol mask setting the uploaded sweeps were made with.
    mask_applied: bool,
}

impl App {
    /// Builds the app on an existing device. Decoding of the bundled sample
    /// starts on a background thread; the map is usable immediately.
    pub fn new(gpu: &Gpu, dirs: Option<Dirs>, size: [f64; 2], source: Source) -> App {
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
        let waker: Waker = Default::default();
        let (pending, feed) = match source {
            Source::Sample => {
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
                (Some(rx), None)
            }
            Source::Live => {
                let cache = dirs.as_ref().map_or_else(
                    || std::env::temp_dir().join("hookecho"),
                    |d| d.cache.clone(),
                );
                let w = waker.clone();
                let wake = std::sync::Arc::new(move || {
                    if let Some(ctx) = w.get() {
                        ctx.request_repaint();
                    }
                });
                let w = waker.clone();
                let wake2 = std::sync::Arc::new(move || {
                    if let Some(ctx) = w.get() {
                        ctx.request_repaint();
                    }
                });
                let frames = hookecho_core::frames::LoopLoader::start(cache.clone(), wake2);
                (
                    None,
                    Some((hookecho_core::live::Feed::start(cache, wake), frames)),
                )
            }
        };
        let (feed, frames) = match feed {
            Some((f, l)) => (Some(f), Some(l)),
            None => (None, None),
        };
        let mut model = Model::new(config, initial, size);
        model.palettes = crate::palettes::Library::load(dirs.as_ref().map(|d| d.config.as_path()));
        App {
            model,
            feed,
            frames,
            waker,
            watching: None,
            renderer,
            icons: IconCache::default(),
            textures: None,
            pending,
            dirs,
            theme_override: None,
            first_radar_frame: false,
            // Built-in defaults are already on the GPU; anything else isn't.
            palettes_applied: u64::MAX,
            mask_applied: true,
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

    /// Color tables dropped on the window.
    fn take_dropped(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let bytes = f.bytes();
            let name = f
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("dropped file")
                .to_string();
            let config_dir = self.dirs.as_ref().map(|d| d.config.clone());
            let m = &mut self.model;
            let r = bytes.and_then(|b| {
                m.palettes
                    .import(&mut m.config, config_dir.as_deref(), &name, &b, m.moment)
            });
            let text = match r {
                Ok((moment, palette)) => {
                    m.moment = moment;
                    format!("Added color palette \u{201c}{palette}\u{201d}")
                }
                Err(e) => format!("Couldn't use {name}: {e}"),
            };
            m.notice = Some((text, ctx.input(|i| i.time) + model::NOTICE_S));
            self.save_config();
        }
    }

    /// The Settings tab's cache size (measured once per opening) and Clear.
    fn sheet_data(&mut self) {
        let Some(sheet) = &mut self.model.sheet else {
            return;
        };
        let Some(dirs) = &self.dirs else { return };
        let cache = hookecho_core::cache::ChunkCache::new(&dirs.cache, 0);
        if sheet.clear_cache {
            sheet.clear_cache = false;
            sheet.cache_bytes = None;
            if let Err(e) = cache.clear() {
                eprintln!("hookecho: clearing cache: {e}");
            }
        }
        if sheet.tab == model::SheetTab::Settings && sheet.cache_bytes.is_none() {
            sheet.cache_bytes = Some(cache.size());
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
    /// Follows the selected site and takes the feed's news.
    fn poll_feed(&mut self, ctx: &egui::Context) {
        let Some(feed) = &self.feed else { return };
        let _ = self.waker.set(ctx.clone());
        if self.watching.as_deref() != Some(self.model.site.id.as_str()) {
            feed.watch(&self.model.site.id);
            self.watching = Some(self.model.site.id.clone());
            self.model.feed = Some(hookecho_core::live::FeedState::Connecting);
        }
        while let Ok(e) = feed.events.try_recv() {
            match e {
                hookecho_core::live::FeedEvent::Update(u) => self.model.accept_live(u),
                hookecho_core::live::FeedEvent::State { site, state }
                    if site == self.model.site.id =>
                {
                    self.model.feed = Some(state);
                }
                _ => {}
            }
        }
        // Loop frames for the current site, tilt and live volume.
        if let Some(loader) = &self.frames {
            if let Some(r) = self.model.loop_request()
                && self.model.looping.request.as_ref() != Some(&r)
            {
                self.model.set_loop_request(r.clone());
                loader.request(r);
            }
            while let Ok(e) = loader.events.try_recv() {
                if let hookecho_core::frames::LoopEvent::Frame(f) = e {
                    self.model.accept_frame(f);
                }
            }
        }
        // The scan-age label is minute-grained: repaint at most every 30 s
        // when nothing else happens (KICKOFF §4: ~0% idle CPU).
        ctx.request_repaint_after(std::time::Duration::from_secs(30));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, gpu: &Gpu, egui_renderer: &mut egui_wgpu::Renderer) {
        self.poll_data();
        let ctx = ui.ctx().clone();
        self.poll_feed(&ctx);
        self.take_dropped(&ctx);
        let screen = ctx.content_rect();
        let theme = Theme::get(self.theme_kind(&ctx));
        screen::keys(ui, &mut self.model, screen);
        let now = ctx.input(|i| i.time);
        if let Some(next) = self.model.advance(now) {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((next - now).max(0.0)));
        }
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
        if self.palettes_applied != self.model.palettes.version {
            for m in hookecho_core::Moment::ALL {
                let p = self.model.palettes.active(&self.model.config, m).clone();
                self.renderer.set_palette(gpu.device, gpu.queue, m, &p);
            }
            self.palettes_applied = self.model.palettes.version;
        }
        if self.mask_applied != self.model.config.mask_dualpol {
            // Re-upload dual-pol sweeps with or without the mask.
            let renderer = &mut self.renderer;
            self.model.uploads.retain(|((_, _, m), id)| {
                let keep = !model::is_dualpol(*m);
                if !keep {
                    renderer.remove_sweep(*id);
                }
                keep
            });
            self.mask_applied = self.model.config.mask_dualpol;
        }
        let changed = self.renderer.ensure_targets(gpu.device, size_px);
        self.register_textures(gpu, egui_renderer, changed);
        // Radar layers: the previous volume's sweep under a sweep that's
        // still arriving, so new data sweeps over old instead of a blank wedge.
        let site = (self.model.site.lat, self.model.site.lon);
        let m = self.model.moment;
        let mut radar = Vec::new();
        for (which, sweep) in [
            (model::Which::Previous, self.model.underlay_sweep()),
            (model::Which::Current, self.model.current_sweep()),
        ] {
            if let Some(i) = sweep
                && let Some(id) = model::ensure_upload(
                    &mut self.model,
                    &mut self.renderer,
                    gpu.device,
                    gpu.queue,
                    which,
                    i,
                    m,
                )
            {
                radar.push((id, site));
            }
        }
        if !radar.is_empty() {
            self.first_radar_frame = true;
        }
        let mut style = theme.scene_style();
        let layers = self.model.config.map;
        style.lines.retain(|l| match l.0 {
            "county" => layers.counties,
            "road" => layers.roads,
            _ => true,
        });
        style.labels.visible = layers.labels;
        self.sheet_data();
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
                radar: &radar,
                style: &style,
                label_exclusions: &exclusions,
            },
        );
        gpu.queue.submit([enc.finish()]);
        self.renderer.end_frame();

        // UI.
        let (scene, blur) = self.textures.expect("registered");
        let mut kit = Kit {
            panels: Default::default(),
            theme: &theme,
            icons: &mut self.icons,
            blur,
            screen,
        };
        let before = self.model.config.clone();
        screen::paint(ui, &mut kit, &mut self.model, scene);
        self.model.chrome = kit
            .panels
            .take()
            .iter()
            .map(|r| [r.min.x, r.min.y, r.max.x, r.max.y])
            .collect();
        if before != self.model.config {
            self.save_config();
        }
        if let Some((_, until)) = &self.model.notice {
            if now >= *until {
                self.model.notice = None;
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - now));
            }
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
                Source::Live,
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
