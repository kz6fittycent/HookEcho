//! Offscreen rendering for `--screenshots`: the same scene + egui UI, drawn
//! into a texture and saved as PNG. No window, no live data.

use crate::design::{
    self,
    icons::IconCache,
    tokens::{Theme, ThemeKind},
};
use crate::radar::Moment;
use crate::scene::{FrameParams, Scene};
use crate::screen::{self, Model, Textures};
use crate::view::Camera;
use eframe::egui::{self, ViewportId, ViewportInfo};
use eframe::wgpu;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use std::path::Path;

pub struct Shot {
    pub name: &'static str,
    pub theme: ThemeKind,
    pub moment: Moment,
    pub size_pts: [f32; 2],
    pub ppp: f32,
    pub camera: Camera,
    pub inspect: Option<(f64, f64)>,
}

pub fn run(
    model: &mut Model,
    scene: &mut Scene,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shots: &[Shot],
    out: &Path,
) {
    std::fs::create_dir_all(out).expect("create screenshot dir");
    let out_format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(device, out_format, RendererOptions::default());
    let ctx = egui::Context::default();
    design::install_fonts(&ctx);
    let mut icons = IconCache::default();
    let mut tex_ids: Option<Textures> = None;

    for shot in shots {
        let size_px = [
            (shot.size_pts[0] * shot.ppp).round() as u32,
            (shot.size_pts[1] * shot.ppp).round() as u32,
        ];
        model.moment = shot.moment;
        model.camera = shot.camera;
        model.inspect = shot.inspect;
        let theme = Theme::get(shot.theme);

        let changed = scene.ensure_targets(device, size_px);
        let t = scene.targets.as_ref().unwrap();
        tex_ids = Some(match (tex_ids.take(), changed) {
            (Some(ids), false) => ids,
            (Some(ids), true) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &t.scene_view,
                    wgpu::FilterMode::Linear,
                    ids.scene,
                );
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &t.blur_view,
                    wgpu::FilterMode::Linear,
                    ids.blur,
                );
                ids
            }
            (None, _) => Textures {
                scene: renderer.register_native_texture(
                    device,
                    &t.scene_view,
                    wgpu::FilterMode::Linear,
                ),
                blur: renderer.register_native_texture(
                    device,
                    &t.blur_view,
                    wgpu::FilterMode::Linear,
                ),
            },
        });
        let ids = tex_ids.as_ref().unwrap();

        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                shot.size_pts.into(),
            )),
            ..Default::default()
        };
        raw.viewports.insert(
            ViewportId::ROOT,
            ViewportInfo {
                native_pixels_per_point: Some(shot.ppp),
                ..Default::default()
            },
        );
        // Two passes: the first settles fonts and layout, the second is the frame we keep.
        let mut full = None;
        for _ in 0..2 {
            let o = ctx.run_ui(raw.clone(), |ui| {
                screen::paint(ui.ctx(), model, &theme, &mut icons, ids)
            });
            for (id, deltas) in &o.textures_delta.set {
                for delta in deltas {
                    renderer.update_texture(device, queue, *id, delta);
                }
            }
            for id in &o.textures_delta.free {
                renderer.free_texture(id);
            }
            full = Some(o);
        }
        let full = full.unwrap();
        let prims = ctx.tessellate(full.shapes, full.pixels_per_point);
        let sd = ScreenDescriptor {
            size_in_pixels: size_px,
            pixels_per_point: full.pixels_per_point,
        };

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shot"),
            size: wgpu::Extent3d {
                width: size_px[0],
                height: size_px[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: out_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut enc = device.create_command_encoder(&Default::default());
        scene.render(
            queue,
            &mut enc,
            &FrameParams {
                size_px,
                ppp: shot.ppp,
                camera: model.camera,
                radar_latlon: (model.site.lat, model.site.lon),
                moment: model.moment,
                theme: &theme,
            },
        );
        let mut cmds = renderer.update_buffers(device, queue, &mut enc, &prims, &sd);
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
            renderer.render(&mut pass.forget_lifetime(), &prims, &sd);
        }
        let row = (size_px[0] * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * size_px[1]) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: size_px[0],
                height: size_px[1],
                depth_or_array_layers: 1,
            },
        );
        cmds.push(enc.finish());
        queue.submit(cmds);
        buf.slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("poll");
        let data = buf.slice(..).get_mapped_range().expect("mapped range");
        let mut img = image::RgbaImage::new(size_px[0], size_px[1]);
        for y in 0..size_px[1] {
            let src = &data[(y * row) as usize..(y * row + size_px[0] * 4) as usize];
            img.as_mut()[(y * size_px[0] * 4) as usize..((y + 1) * size_px[0] * 4) as usize]
                .copy_from_slice(src);
        }
        drop(data);
        buf.unmap();
        let path = out.join(format!("{}.png", shot.name));
        img.save(&path).expect("save png");
        println!("wrote {}", path.display());
    }
}
