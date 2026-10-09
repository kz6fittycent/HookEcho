//! wgpu scene renderer: radar + basemap into an sRGB texture, then a
//! downsampled separable Gaussian blur of it for the glass panels.

use crate::basemap::{LineLayer, LineVertex};
use crate::design::tokens::{Rgba, Theme};
use crate::radar::{AZ_BINS, Moment, SweepData};
use crate::view::{Camera, to_merc};
use eframe::wgpu::{self, util::DeviceExt};
use std::collections::HashMap;

pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const BLUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BLUR_DIV: u32 = 4;
/// Backdrop blur sigma in logical px (docs/DESIGN.md §2.6).
const BLUR_SIGMA_PT: f32 = 18.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RadarU {
    viewport: [f32; 2],
    radar_px: [f32; 2],
    radar_merc: [f32; 2],
    px_per_merc: f32,
    radar_lat: f32,
    radar_lon: f32,
    first_gate_km: f32,
    gate_km: f32,
    gates: f32,
    inv_cos_elev: f32,
    alpha: f32,
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LineU {
    viewport: [f32; 2],
    offset_px: [f32; 2],
    px_per_unit: f32,
    width_px: f32,
    _pad: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurU {
    texel: [f32; 2],
    dir: [f32; 2],
    sigma: f32,
    saturation: f32,
    tone: f32,
    tone_light: f32,
}

struct GpuSweep {
    data: SweepMeta,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

struct SweepMeta {
    first_gate_km: f32,
    gate_km: f32,
    gates: usize,
    elevation_deg: f32,
}

struct GpuLayer {
    name: String,
    origin: [f64; 2],
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    count: u32,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

pub struct Targets {
    pub size: [u32; 2],
    pub scene: wgpu::Texture,
    pub scene_view: wgpu::TextureView,
    pub blur_view: wgpu::TextureView,
    tmp_view: wgpu::TextureView,
    down_bind: wgpu::BindGroup,
    h_bind: wgpu::BindGroup,
    v_bind: wgpu::BindGroup,
    down_u: wgpu::Buffer,
    h_u: wgpu::Buffer,
    v_u: wgpu::Buffer,
}

pub struct FrameParams<'a> {
    pub size_px: [u32; 2],
    pub ppp: f32,
    pub camera: Camera,
    pub radar_latlon: (f64, f64),
    pub moment: Moment,
    pub theme: &'a Theme,
}

pub struct Scene {
    radar_pipe: wgpu::RenderPipeline,
    line_pipe: wgpu::RenderPipeline,
    down_pipe: wgpu::RenderPipeline,
    blur_pipe: wgpu::RenderPipeline,
    radar_bgl: wgpu::BindGroupLayout,
    line_bgl: wgpu::BindGroupLayout,
    blur_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    sweeps: HashMap<Moment, GpuSweep>,
    layers: Vec<GpuLayer>,
    pub targets: Option<Targets>,
}

fn uniform_entry(binding: u32, vis: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: vis,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn tex_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

const ALPHA_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

impl Scene {
    pub fn new(device: &wgpu::Device) -> Scene {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let frag = wgpu::ShaderStages::FRAGMENT;
        let uint = wgpu::TextureSampleType::Uint;
        let float = |filterable| wgpu::TextureSampleType::Float { filterable };
        let radar_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("radar"),
            entries: &[
                uniform_entry(0, frag),
                tex_entry(1, uint),
                tex_entry(2, uint),
                tex_entry(3, float(false)),
            ],
        });
        let line_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("line"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur"),
            entries: &[
                uniform_entry(0, frag),
                tex_entry(1, float(true)),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: frag,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipe = |label: &str,
                    bgl: &wgpu::BindGroupLayout,
                    vs: &str,
                    fs: &str,
                    buffers: &[Option<wgpu::VertexBufferLayout>],
                    format: wgpu::TextureFormat,
                    blend: Option<wgpu::BlendState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(bgl)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let line_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32],
        };
        let radar_pipe = pipe(
            "radar",
            &radar_bgl,
            "vs_fullscreen",
            "fs_radar",
            &[],
            SCENE_FORMAT,
            Some(ALPHA_BLEND),
        );
        let line_pipe = pipe(
            "line",
            &line_bgl,
            "vs_line",
            "fs_line",
            &[Some(line_layout)],
            SCENE_FORMAT,
            Some(ALPHA_BLEND),
        );
        let down_pipe = pipe(
            "down",
            &blur_bgl,
            "vs_fullscreen",
            "fs_downsample",
            &[],
            BLUR_FORMAT,
            None,
        );
        let blur_pipe = pipe(
            "blur",
            &blur_bgl,
            "vs_fullscreen",
            "fs_blur",
            &[],
            BLUR_FORMAT,
            None,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        Scene {
            radar_pipe,
            line_pipe,
            down_pipe,
            blur_pipe,
            radar_bgl,
            line_bgl,
            blur_bgl,
            sampler,
            sweeps: HashMap::new(),
            layers: Vec::new(),
            targets: None,
        }
    }

    /// Uploads a sweep once; switching products afterwards is a bind-group swap.
    pub fn upload_sweep(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        s: &SweepData,
        lut: &[u8],
    ) {
        let tex = |label: &str, w: u32, h: u32, format: wgpu::TextureFormat, data: &[u8]| {
            device
                .create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    data,
                )
                .create_view(&Default::default())
        };
        let gates = tex(
            "gates",
            s.gates as u32,
            s.radials as u32,
            wgpu::TextureFormat::R8Uint,
            &s.raw,
        );
        let az = tex(
            "az",
            AZ_BINS as u32,
            1,
            wgpu::TextureFormat::R16Uint,
            bytemuck::cast_slice(&s.az_lookup),
        );
        let lut = tex("lut", 256, 1, wgpu::TextureFormat::Rgba8UnormSrgb, lut);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("radar-u"),
            size: std::mem::size_of::<RadarU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("radar"),
            layout: &self.radar_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gates),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&az),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&lut),
                },
            ],
        });
        let data = SweepMeta {
            first_gate_km: s.first_gate_km,
            gate_km: s.gate_km,
            gates: s.gates,
            elevation_deg: s.elevation_deg,
        };
        self.sweeps.insert(
            s.moment,
            GpuSweep {
                data,
                uniform,
                bind,
            },
        );
    }

    pub fn upload_basemap(&mut self, device: &wgpu::Device, layers: &[LineLayer]) {
        for l in layers {
            let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&l.name),
                contents: bytemuck::cast_slice(&l.geometry.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&l.name),
                contents: bytemuck::cast_slice(&l.geometry.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("line-u"),
                size: std::mem::size_of::<LineU>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("line"),
                layout: &self.line_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            self.layers.push(GpuLayer {
                name: l.name.clone(),
                origin: l.origin,
                vbuf,
                ibuf,
                count: l.geometry.indices.len() as u32,
                uniform,
                bind,
            });
        }
    }

    /// (Re)creates render targets. Returns true if they changed.
    pub fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) -> bool {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        let mk = |label: &str, w: u32, h: u32, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let scene = mk("scene", size[0], size[1], SCENE_FORMAT);
        let (bw, bh) = (size[0].div_ceil(BLUR_DIV), size[1].div_ceil(BLUR_DIV));
        let blur = mk("blur", bw, bh, BLUR_FORMAT);
        let tmp = mk("blur-tmp", bw, bh, BLUR_FORMAT);
        let scene_view = scene.create_view(&Default::default());
        let blur_view = blur.create_view(&Default::default());
        let tmp_view = tmp.create_view(&Default::default());
        let ub = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blur-u"),
                size: std::mem::size_of::<BlurU>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let (down_u, h_u, v_u) = (ub(), ub(), ub());
        let bg = |u: &wgpu::Buffer, src: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur"),
                layout: &self.blur_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: u.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        // scene → blur (downsample), blur → tmp (horizontal), tmp → blur (vertical).
        let down_bind = bg(&down_u, &scene_view);
        let h_bind = bg(&h_u, &blur_view);
        let v_bind = bg(&v_u, &tmp_view);
        self.targets = Some(Targets {
            size,
            scene,
            scene_view,
            blur_view,
            tmp_view,
            down_bind,
            h_bind,
            v_bind,
            down_u,
            h_u,
            v_u,
        });
        true
    }

    pub fn render(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, p: &FrameParams) {
        let t = self.targets.as_ref().expect("targets");
        let [w, h] = p.size_px;
        let viewport = [w as f32, h as f32];
        let half = [w as f64 / 2.0, h as f64 / 2.0];
        let px_per_merc = p.camera.scale * p.ppp as f64;
        let to_px = |m: [f64; 2]| {
            [
                (m[0] - p.camera.center[0]) * px_per_merc + half[0],
                (m[1] - p.camera.center[1]) * px_per_merc + half[1],
            ]
        };

        let land: Rgba = p.theme.map_land;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: land[0] as f64,
                            g: land[1] as f64,
                            b: land[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });

            // Counties and roads under the radar; state lines over it so they stay readable.
            let style = |name: &str| -> (f32, Rgba) {
                match name {
                    "state" => (1.25, p.theme.map_state),
                    "county" => (0.75, p.theme.map_county),
                    _ => (1.0, p.theme.map_road),
                }
            };
            let draw_layer = |pass: &mut wgpu::RenderPass, l: &GpuLayer| {
                let (width, color) = style(&l.name);
                let o = to_px(l.origin);
                let u = LineU {
                    viewport,
                    offset_px: [o[0] as f32, o[1] as f32],
                    px_per_unit: (px_per_merc / 1.0e4) as f32,
                    width_px: width * p.ppp,
                    _pad: [0.0; 2],
                    color,
                };
                queue.write_buffer(&l.uniform, 0, bytemuck::bytes_of(&u));
                pass.set_pipeline(&self.line_pipe);
                pass.set_bind_group(0, &l.bind, &[]);
                pass.set_vertex_buffer(0, l.vbuf.slice(..));
                pass.set_index_buffer(l.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..l.count, 0, 0..1);
            };
            for l in self.layers.iter().filter(|l| l.name != "state") {
                draw_layer(&mut pass, l);
            }

            if let Some(s) = self.sweeps.get(&p.moment) {
                let rm = to_merc(p.radar_latlon.0, p.radar_latlon.1);
                let rp = to_px(rm);
                let u = RadarU {
                    viewport,
                    radar_px: [rp[0] as f32, rp[1] as f32],
                    radar_merc: [rm[0] as f32, rm[1] as f32],
                    px_per_merc: px_per_merc as f32,
                    radar_lat: p.radar_latlon.0.to_radians() as f32,
                    radar_lon: p.radar_latlon.1.to_radians() as f32,
                    first_gate_km: s.data.first_gate_km,
                    gate_km: s.data.gate_km,
                    gates: s.data.gates as f32,
                    inv_cos_elev: 1.0 / s.data.elevation_deg.to_radians().cos(),
                    alpha: p.theme.radar_alpha,
                    _pad: [0.0; 2],
                };
                queue.write_buffer(&s.uniform, 0, bytemuck::bytes_of(&u));
                pass.set_pipeline(&self.radar_pipe);
                pass.set_bind_group(0, &s.bind, &[]);
                pass.draw(0..3, 0..1);
            }

            for l in self.layers.iter().filter(|l| l.name == "state") {
                draw_layer(&mut pass, l);
            }
        }

        // Blur chain.
        let (bw, bh) = (w.div_ceil(BLUR_DIV) as f32, h.div_ceil(BLUR_DIV) as f32);
        let sigma = BLUR_SIGMA_PT * p.ppp / BLUR_DIV as f32;
        let light = if p.theme.kind == crate::design::tokens::ThemeKind::Light {
            1.0
        } else {
            0.0
        };
        let write = |buf: &wgpu::Buffer, texel: [f32; 2], dir: [f32; 2], last: bool| {
            let (saturation, tone) = if last {
                (p.theme.blur_saturation, p.theme.blur_tone)
            } else {
                (1.0, 0.0)
            };
            let u = BlurU {
                texel,
                dir,
                sigma,
                saturation,
                tone,
                tone_light: light,
            };
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&u));
        };
        write(
            &t.down_u,
            [1.0 / w as f32, 1.0 / h as f32],
            [0.0, 0.0],
            false,
        );
        write(&t.h_u, [1.0 / bw, 1.0 / bh], [1.0, 0.0], false);
        write(&t.v_u, [1.0 / bw, 1.0 / bh], [0.0, 1.0], true);
        let steps: [(&wgpu::TextureView, &wgpu::BindGroup, &wgpu::RenderPipeline); 3] = [
            (&t.blur_view, &t.down_bind, &self.down_pipe),
            (&t.tmp_view, &t.h_bind, &self.blur_pipe),
            (&t.blur_view, &t.v_bind, &self.blur_pipe),
        ];
        for (target, bind, pipe) in steps {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
