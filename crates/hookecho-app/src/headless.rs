//! Offscreen app: the same `App::ui` as the window, drawn into a texture.
//! Used by `--screenshots`, `--smoke-test` and `--bench`; needs no display.

use crate::app::{App, Gpu};
use crate::design;
use eframe::egui::{self, ViewportId, ViewportInfo};
use egui_wgpu::{Renderer as EguiRenderer, RendererOptions, ScreenDescriptor};
use hookecho_core::paths::Dirs;
use hookecho_render::capture;

pub const OUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct Offscreen {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    egui: EguiRenderer,
    pub ctx: egui::Context,
    pub app: App,
    time: f64,
}

impl Offscreen {
    pub fn new(dirs: Option<Dirs>, size_pts: [f32; 2]) -> Result<Offscreen, String> {
        Self::with_source(dirs, size_pts, crate::app::Source::Sample)
    }

    pub fn with_source(
        dirs: Option<Dirs>,
        size_pts: [f32; 2],
        source: crate::app::Source,
    ) -> Result<Offscreen, String> {
        crate::bench::trace("process");
        let (adapter, device, queue) = capture::headless_device()?;
        crate::bench::trace("gpu device");
        let egui = EguiRenderer::new(&device, OUT_FORMAT, RendererOptions::default());
        let ctx = egui::Context::default();
        design::install_fonts(&ctx);
        crate::bench::trace("egui");
        let app = App::new(
            &Gpu {
                device: &device,
                queue: &queue,
            },
            dirs,
            [size_pts[0] as f64, size_pts[1] as f64],
            source,
        );
        Ok(Offscreen {
            adapter,
            device,
            queue,
            egui,
            ctx,
            app,
            time: 0.0,
        })
    }

    pub fn adapter_name(&self) -> String {
        let i = self.adapter.get_info();
        format!("{} ({:?})", i.name, i.backend)
    }

    /// Renders one frame and returns the output texture (RGBA8).
    pub fn frame(
        &mut self,
        size_pts: [f32; 2],
        ppp: f32,
        events: Vec<egui::Event>,
    ) -> wgpu::Texture {
        let size_px = [
            (size_pts[0] * ppp).round() as u32,
            (size_pts[1] * ppp).round() as u32,
        ];
        // Advance time well past any hover/tooltip delay so frames are settled.
        self.time += 1.0;
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size_pts.into())),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        raw.viewports.insert(
            ViewportId::ROOT,
            ViewportInfo {
                native_pixels_per_point: Some(ppp),
                ..Default::default()
            },
        );
        let gpu = Gpu {
            device: &self.device,
            queue: &self.queue,
        };
        let (app, egui_r) = (&mut self.app, &mut self.egui);
        let mut out = self.ctx.run_ui(raw, |ui| app.ui(ui, &gpu, egui_r));
        for (id, deltas) in &out.textures_delta.set {
            for d in deltas {
                self.egui.update_texture(&self.device, &self.queue, *id, d);
            }
        }
        let prims = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        let sd = ScreenDescriptor {
            size_in_pixels: size_px,
            pixels_per_point: out.pixels_per_point,
        };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-ui"),
            size: wgpu::Extent3d {
                width: size_px[0],
                height: size_px[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        let mut cmds = self
            .egui
            .update_buffers(&self.device, &self.queue, &mut enc, &prims, &sd);
        {
            let pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.egui.render(&mut pass.forget_lifetime(), &prims, &sd);
        }
        cmds.push(enc.finish());
        self.queue.submit(cmds);
        for id in &out.textures_delta.free {
            self.egui.free_texture(id);
        }
        // Applied above; egui checks (in debug builds) that deltas aren't dropped unhandled.
        out.textures_delta.clear();
        target
    }

    /// Renders until the UI is settled (two frames), returns RGBA bytes.
    pub fn capture(&mut self, size_pts: [f32; 2], ppp: f32) -> (u32, u32, Vec<u8>) {
        self.frame(size_pts, ppp, Vec::new());
        let t = self.frame(size_pts, ppp, Vec::new());
        (
            t.width(),
            t.height(),
            capture::read_rgba8(&self.device, &self.queue, &t),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Picker;
    use hookecho_core::Moment;

    const SIZE: [f32; 2] = [1280.0, 800.0];

    /// Clicks the widget `id` drew last frame (press and release in one
    /// frame: the harness advances time 1 s per frame).
    fn click(o: &mut Offscreen, id: egui::Id) {
        let r = o
            .ctx
            .read_response(id)
            .unwrap_or_else(|| panic!("{id:?} not drawn"))
            .rect;
        let p = r.center();
        let ev = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        o.frame(SIZE, 1.0, vec![egui::Event::PointerMoved(p)]);
        o.frame(SIZE, 1.0, vec![ev(true), ev(false)]);
        o.frame(SIZE, 1.0, Vec::new());
    }

    #[test]
    fn pickers_respond_to_clicks() {
        let Ok(mut o) = Offscreen::new(None, SIZE) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        o.app.wait_for_data();
        o.frame(SIZE, 1.0, Vec::new());
        click(&mut o, egui::Id::new("product-chip"));
        assert_eq!(o.app.model.picker, Picker::Product, "chip opens the picker");
        click(&mut o, egui::Id::new(("product", Moment::Velocity.code())));
        assert_eq!(o.app.model.moment, Moment::Velocity, "row click applies");
        assert_eq!(o.app.model.picker, Picker::None, "and closes");

        click(&mut o, egui::Id::new("legend"));
        assert_eq!(o.app.model.picker, Picker::Palette);
        click(&mut o, egui::Id::new(("palette", "VEL", "Grayscale")));
        assert_eq!(
            o.app
                .model
                .palettes
                .active(&o.app.model.config, Moment::Velocity)
                .name,
            "Grayscale"
        );
    }

    #[test]
    fn imported_palette_can_be_removed() {
        let Ok(mut o) = Offscreen::new(None, SIZE) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        o.app.wait_for_data();
        let m = &mut o.app.model;
        m.palettes
            .import(
                &mut m.config,
                None,
                "Mine.pal",
                b"Product: BR\nColor: 0 0 0 0\nColor: 70 255 255 255\n",
                Moment::Reflectivity,
            )
            .unwrap();
        assert_eq!(
            m.palettes.active(&m.config, Moment::Reflectivity).name,
            "Mine"
        );
        o.frame(SIZE, 1.0, Vec::new());
        click(&mut o, egui::Id::new("legend"));
        click(&mut o, egui::Id::new(("palette-remove", "REF", "Mine")));
        let m = &o.app.model;
        assert!(
            m.palettes
                .choices(Moment::Reflectivity)
                .iter()
                .all(|c| !c.imported)
        );
        assert_eq!(
            m.palettes.active(&m.config, Moment::Reflectivity).name,
            "Hook Echo Standard",
            "back to the default"
        );
    }

    #[test]
    fn side_sheet_controls_respond() {
        let Ok(mut o) = Offscreen::new(None, SIZE) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        o.app.wait_for_data();
        o.frame(SIZE, 1.0, Vec::new());
        click(&mut o, egui::Id::new("sheet-button"));
        assert!(o.app.model.sheet.is_some(), "button opens the sheet");
        click(&mut o, egui::Id::new(("sheet", "roads")));
        assert!(!o.app.model.config.map.roads, "toggle flips");
        click(&mut o, egui::Id::new("sheet-tabs").with(2));
        assert_eq!(
            o.app.model.sheet.as_ref().map(|s| s.tab),
            Some(crate::model::SheetTab::Settings)
        );
        click(&mut o, egui::Id::new(("sheet", "utc")).with(1));
        assert!(o.app.model.config.utc);
        click(&mut o, egui::Id::new(("sheet", "frames")).with(1));
        assert_eq!(o.app.model.config.loop_frames, 30);
        click(&mut o, egui::Id::new("sheet-button"));
        assert!(o.app.model.sheet.is_none(), "and closes it");
    }

    fn key(o: &mut Offscreen, key: egui::Key, modifiers: egui::Modifiers) {
        let ev = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        o.frame(SIZE, 1.0, vec![ev(true), ev(false)]);
        o.frame(SIZE, 1.0, Vec::new());
    }

    #[test]
    fn command_palette_runs_typed_commands() {
        let Ok(mut o) = Offscreen::new(None, SIZE) else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        o.app.wait_for_data();
        o.frame(SIZE, 1.0, Vec::new());
        click(&mut o, egui::Id::new("command-button"));
        assert!(
            matches!(o.app.model.picker, Picker::Command { .. }),
            "button opens it"
        );
        o.frame(SIZE, 1.0, vec![egui::Event::Text("velocity".into())]);
        key(&mut o, egui::Key::Enter, Default::default());
        assert_eq!(
            o.app.model.moment,
            Moment::Velocity,
            "Enter runs the top match"
        );
        assert_eq!(o.app.model.picker, Picker::None, "and closes");

        key(&mut o, egui::Key::K, egui::Modifiers::COMMAND);
        assert!(
            matches!(o.app.model.picker, Picker::Command { .. }),
            "Ctrl+K opens it"
        );
        o.frame(SIZE, 1.0, vec![egui::Event::Text("kfws".into())]);
        key(&mut o, egui::Key::Enter, Default::default());
        assert_eq!(o.app.model.site.id, "KFWS", "sites by ID");

        key(&mut o, egui::Key::Questionmark, Default::default());
        assert_eq!(
            o.app.model.picker,
            Picker::Shortcuts,
            "? opens the shortcuts"
        );
        key(&mut o, egui::Key::Escape, Default::default());
        assert_eq!(o.app.model.picker, Picker::None);
    }
}
