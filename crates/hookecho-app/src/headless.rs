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
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &gpu, egui_r));
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
