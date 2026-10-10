//! Hook Echo renderer: radar + basemap + labels into an sRGB texture, plus a
//! blurred, tone-compressed copy for glass panels. Draws to any surface or
//! offscreen; knows nothing about egui.

pub mod capture;
mod labels;
mod map;
pub mod palette;

use hookecho_core::Moment;
use hookecho_core::basemap::Basemap;
use hookecho_core::geo::{AZIMUTH_BINS, Camera, azimuth_index, to_mercator};
use hookecho_core::level2::Sweep;
pub use labels::LabelStyle;
use palette::{LUT_SIZE, Palette};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const BLUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BLUR_DIV: u32 = 4;

/// Linear RGBA.
pub type Rgba = [f32; 4];

/// Everything the renderer needs to know about the current theme. The app
/// builds this from its design tokens, so no UI colors live here.
#[derive(Clone)]
pub struct SceneStyle {
    pub water: Rgba,
    pub land: Rgba,
    pub lakes: Rgba,
    /// Line layers by name: (width in logical px, color, drawn over radar?).
    pub lines: Vec<(&'static str, f32, Rgba, bool)>,
    pub labels: LabelStyle,
    pub radar_alpha: f32,
    pub blur_sigma: f32,
    pub blur_saturation: f32,
    pub blur_tone: f32,
    pub blur_tone_light: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SweepId(u64);

/// What to draw this frame.
pub struct Frame<'a> {
    pub size_px: [u32; 2],
    pub ppp: f32,
    pub camera: Camera,
    pub radar: Option<(SweepId, (f64, f64))>,
    pub style: &'a SceneStyle,
    /// Screen areas (logical px, [x0, y0, x1, y1]) kept free of labels.
    pub label_exclusions: &'a [[f32; 4]],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MapU {
    viewport: [f32; 2],
    offset_px: [f32; 2],
    px_per_unit: f32,
    width_px: f32,
    _pad: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RadarU {
    viewport: [f32; 2],
    radar_px: [f32; 2],
    radar_merc: [f32; 2],
    px_per_merc: f32,
    radar_lat: f32,
    radar_lon: f32,
    elev: f32,
    first_gate_km: f32,
    gate_km: f32,
    gates: f32,
    scale: f32,
    offset: f32,
    lut_min: f32,
    lut_max: f32,
    alpha: f32,
    min_value: f32,
    _pad: f32,
    rf_color: [f32; 4],
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
    moment: Moment,
    elevation_deg: f32,
    first_gate_km: f32,
    gate_km: f32,
    gates: u32,
    scale: f32,
    offset: f32,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    bytes: u64,
}

struct Lut {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    min: f32,
    max: f32,
    rf: [u8; 4],
}

pub struct Targets {
    pub size: [u32; 2],
    pub scene: wgpu::Texture,
    pub scene_view: wgpu::TextureView,
    pub blur_view: wgpu::TextureView,
    tmp_view: wgpu::TextureView,
    passes: [(wgpu::Buffer, wgpu::BindGroup); 3],
}

pub struct Renderer {
    fill_pipe: wgpu::RenderPipeline,
    line_pipe: wgpu::RenderPipeline,
    radar_pipe: wgpu::RenderPipeline,
    down_pipe: wgpu::RenderPipeline,
    blur_pipe: wgpu::RenderPipeline,
    map_bgl: wgpu::BindGroupLayout,
    radar_bgl: wgpu::BindGroupLayout,
    blur_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    layers: Vec<map::GpuLayer>,
    labels: labels::Labels,
    luts: HashMap<Moment, Lut>,
    sweeps: HashMap<SweepId, GpuSweep>,
    next_id: u64,
    pub targets: Option<Targets>,
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
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

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl Renderer {
    /// `fonts`: font files for map labels (the app passes its bundled Inter).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        basemap: &Basemap,
        fonts: Vec<Vec<u8>>,
    ) -> Renderer {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hookecho-scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let frag = wgpu::ShaderStages::FRAGMENT;
        let map_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("map"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let uint = wgpu::TextureSampleType::Uint;
        let radar_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("radar"),
            entries: &[
                uniform_entry(0, frag),
                texture_entry(1, uint),
                texture_entry(2, uint),
                texture_entry(3, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur"),
            entries: &[
                uniform_entry(0, frag),
                texture_entry(1, wgpu::TextureSampleType::Float { filterable: true }),
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
        let fill_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        };
        let line_layout = wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
        };
        let fill_pipe = pipe(
            "fill",
            &map_bgl,
            "vs_fill",
            "fs_fill",
            &[Some(fill_layout)],
            SCENE_FORMAT,
            Some(ALPHA_BLEND),
        );
        let line_pipe = pipe(
            "line",
            &map_bgl,
            "vs_line",
            "fs_line",
            &[Some(line_layout)],
            SCENE_FORMAT,
            Some(ALPHA_BLEND),
        );
        let radar_pipe = pipe(
            "radar",
            &radar_bgl,
            "vs_fullscreen",
            "fs_radar",
            &[],
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
            ..Default::default()
        });
        let layers = map::upload(
            device,
            &map_bgl,
            basemap,
            std::mem::size_of::<MapU>() as u64,
        );
        let labels = labels::Labels::new(device, queue, SCENE_FORMAT, fonts, &basemap.places);
        let mut r = Renderer {
            fill_pipe,
            line_pipe,
            radar_pipe,
            down_pipe,
            blur_pipe,
            map_bgl,
            radar_bgl,
            blur_bgl,
            sampler,
            layers,
            labels,
            luts: HashMap::new(),
            sweeps: HashMap::new(),
            next_id: 0,
            targets: None,
        };
        for m in Moment::ALL {
            r.set_palette(device, queue, m, &Palette::default_for(m));
        }
        r
    }

    /// Sets the color table for a moment. Only the 4 KB LUT is re-uploaded;
    /// sweeps are never re-decoded or re-uploaded.
    pub fn set_palette(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        m: Moment,
        p: &Palette,
    ) {
        let data = p.lut();
        let extent = wgpu::Extent3d {
            width: LUT_SIZE as u32,
            height: 1,
            depth_or_array_layers: 1,
        };
        match self.luts.get_mut(&m) {
            Some(lut) => {
                queue.write_texture(
                    lut.texture.as_image_copy(),
                    &data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(LUT_SIZE as u32 * 4),
                        rows_per_image: None,
                    },
                    extent,
                );
                (lut.min, lut.max, lut.rf) = (p.min(), p.max(), p.range_folded);
            }
            None => {
                let texture = device.create_texture_with_data(
                    queue,
                    &wgpu::TextureDescriptor {
                        label: Some("lut"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &data,
                );
                let view = texture.create_view(&Default::default());
                self.luts.insert(
                    m,
                    Lut {
                        texture,
                        view,
                        min: p.min(),
                        max: p.max(),
                        rf: p.range_folded,
                    },
                );
            }
        }
    }

    /// Uploads one moment of one sweep, once (KICKOFF §4). Returns `None` if
    /// the sweep lacks the moment.
    pub fn upload_sweep(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sweep: &Sweep,
        m: Moment,
    ) -> Option<SweepId> {
        let f = sweep.field(m)?;
        let radials = f.radials() as u32;
        if radials == 0 || f.gates == 0 {
            return None;
        }
        let tex = |label: &str, w: u32, h: u32, data: &[u8]| {
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
                        format: wgpu::TextureFormat::R16Uint,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    data,
                )
                .create_view(&Default::default())
        };
        let raw = f.raw.to_u16();
        let gates = tex("gates", f.gates as u32, radials, bytemuck::cast_slice(&raw));
        let centers: Vec<f32> = sweep.radials.iter().map(|r| r.azimuth_deg).collect();
        let spacing: Vec<f32> = sweep
            .radials
            .iter()
            .map(|r| r.azimuth_spacing_deg)
            .collect();
        let index = azimuth_index(&centers, &spacing);
        let az = tex(
            "azimuth-index",
            AZIMUTH_BINS as u32,
            1,
            bytemuck::cast_slice(&index),
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("radar-u"),
            size: std::mem::size_of::<RadarU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lut = &self.luts[&m];
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
                    resource: wgpu::BindingResource::TextureView(&lut.view),
                },
            ],
        });
        let id = SweepId(self.next_id);
        self.next_id += 1;
        self.sweeps.insert(
            id,
            GpuSweep {
                moment: m,
                elevation_deg: sweep.elevation_deg,
                first_gate_km: f.first_gate_km,
                gate_km: f.gate_km,
                gates: f.gates as u32,
                scale: f.scale,
                offset: f.offset,
                uniform,
                bind,
                bytes: raw.len() as u64 * 2 + AZIMUTH_BINS as u64 * 2,
            },
        );
        Some(id)
    }

    pub fn remove_sweep(&mut self, id: SweepId) {
        self.sweeps.remove(&id);
    }

    /// GPU bytes held by radar sweeps and basemap geometry (for budgets).
    pub fn gpu_bytes(&self) -> (u64, u64) {
        (
            self.sweeps.values().map(|s| s.bytes).sum(),
            self.layers.iter().map(|l| l.bytes).sum(),
        )
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
        let pass = |src: &wgpu::TextureView| {
            let u = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blur-u"),
                size: std::mem::size_of::<BlurU>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
            });
            (u, bind)
        };
        // scene → blur (downsample), blur → tmp (horizontal), tmp → blur (vertical).
        let passes = [pass(&scene_view), pass(&blur_view), pass(&tmp_view)];
        self.targets = Some(Targets {
            size,
            scene,
            scene_view,
            blur_view,
            tmp_view,
            passes,
        });
        true
    }

    /// Records the scene and blur passes. Call `ensure_targets` first.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        f: &Frame,
    ) {
        let style = f.style;
        let [w, h] = f.size_px;
        let viewport = [w as f32, h as f32];
        let px_per_merc = f.camera.scale * f.ppp as f64;
        let to_px = |m: [f64; 2]| {
            [
                (m[0] - f.camera.center[0]) * px_per_merc + w as f64 / 2.0,
                (m[1] - f.camera.center[1]) * px_per_merc + h as f64 / 2.0,
            ]
        };
        let o = to_px(map::ORIGIN);
        let map_u = |width_pt: f32, color: Rgba| MapU {
            viewport,
            offset_px: [o[0] as f32, o[1] as f32],
            px_per_unit: (px_per_merc / map::UNIT) as f32,
            width_px: width_pt * f.ppp,
            _pad: [0.0; 2],
            color,
        };
        for l in &self.layers {
            let u = match l.name.as_str() {
                "land" => Some(map_u(0.0, style.land)),
                "lakes" => Some(map_u(0.0, style.lakes)),
                name => style
                    .lines
                    .iter()
                    .find(|s| s.0 == name)
                    .map(|s| map_u(s.1, s.2)),
            };
            if let Some(u) = u {
                queue.write_buffer(&l.uniform, 0, bytemuck::bytes_of(&u));
            }
        }
        if let Some((id, site)) = f.radar
            && let Some(s) = self.sweeps.get(&id)
        {
            let lut = &self.luts[&s.moment];
            let rm = to_mercator(site.0, site.1);
            let rp = to_px(rm);
            let rf = lut.rf;
            let u = RadarU {
                viewport,
                radar_px: [rp[0] as f32, rp[1] as f32],
                radar_merc: [rm[0] as f32, rm[1] as f32],
                px_per_merc: px_per_merc as f32,
                radar_lat: site.0.to_radians() as f32,
                radar_lon: site.1.to_radians() as f32,
                elev: s.elevation_deg.to_radians(),
                first_gate_km: s.first_gate_km,
                gate_km: s.gate_km,
                gates: s.gates as f32,
                scale: s.scale,
                offset: s.offset,
                lut_min: lut.min,
                lut_max: lut.max,
                alpha: style.radar_alpha,
                min_value: f32::MIN,
                _pad: 0.0,
                rf_color: [
                    srgb_to_linear(rf[0]),
                    srgb_to_linear(rf[1]),
                    srgb_to_linear(rf[2]),
                    rf[3] as f32 / 255.0,
                ],
            };
            queue.write_buffer(&s.uniform, 0, bytemuck::bytes_of(&u));
        }
        self.labels.prepare(
            device,
            queue,
            f.size_px,
            f.ppp,
            &f.camera,
            &style.labels,
            f.label_exclusions,
        );

        let t = self.targets.as_ref().expect("ensure_targets before render");
        {
            let water = style.water;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: water[0] as f64,
                            g: water[1] as f64,
                            b: water[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let draw = |pass: &mut wgpu::RenderPass, l: &map::GpuLayer| match &l.geometry {
                map::Geometry::Fill { vbuf, ibuf, count } => {
                    pass.set_pipeline(&self.fill_pipe);
                    pass.set_bind_group(0, &l.bind, &[]);
                    pass.set_vertex_buffer(0, vbuf.slice(..));
                    pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..*count, 0, 0..1);
                }
                map::Geometry::Lines { segments, count } => {
                    pass.set_pipeline(&self.line_pipe);
                    pass.set_bind_group(0, &l.bind, &[]);
                    pass.set_vertex_buffer(0, segments.slice(..));
                    pass.draw(0..6, 0..*count);
                }
            };
            let over = |name: &str| style.lines.iter().find(|s| s.0 == name).map(|s| s.3);
            // Fills, then lines that sit under the radar, then the radar, then
            // the lines that must stay readable over it, then labels.
            for l in self
                .layers
                .iter()
                .filter(|l| matches!(l.geometry, map::Geometry::Fill { .. }))
            {
                draw(&mut pass, l);
            }
            for l in self.layers.iter().filter(|l| over(&l.name) == Some(false)) {
                draw(&mut pass, l);
            }
            if let Some(s) = f.radar.and_then(|(id, _)| self.sweeps.get(&id)) {
                pass.set_pipeline(&self.radar_pipe);
                pass.set_bind_group(0, &s.bind, &[]);
                pass.draw(0..3, 0..1);
            }
            for l in self.layers.iter().filter(|l| over(&l.name) == Some(true)) {
                draw(&mut pass, l);
            }
            self.labels.render(&mut pass);
        }

        let (bw, bh) = (w.div_ceil(BLUR_DIV) as f32, h.div_ceil(BLUR_DIV) as f32);
        let sigma = style.blur_sigma * f.ppp / BLUR_DIV as f32;
        let light = if style.blur_tone_light { 1.0 } else { 0.0 };
        // (texel size, direction, final pass?, target, pipeline)
        type Step<'a> = (
            [f32; 2],
            [f32; 2],
            bool,
            &'a wgpu::TextureView,
            &'a wgpu::RenderPipeline,
        );
        let steps: [Step; 3] = [
            (
                [1.0 / w as f32, 1.0 / h as f32],
                [0.0, 0.0],
                false,
                &t.blur_view,
                &self.down_pipe,
            ),
            (
                [1.0 / bw, 1.0 / bh],
                [1.0, 0.0],
                false,
                &t.tmp_view,
                &self.blur_pipe,
            ),
            (
                [1.0 / bw, 1.0 / bh],
                [0.0, 1.0],
                true,
                &t.blur_view,
                &self.blur_pipe,
            ),
        ];
        for ((texel, dir, last, target, pipe), (u, bind)) in steps.into_iter().zip(&t.passes) {
            let (saturation, tone) = if last {
                (style.blur_saturation, style.blur_tone)
            } else {
                (1.0, 0.0)
            };
            queue.write_buffer(
                u,
                0,
                bytemuck::bytes_of(&BlurU {
                    texel,
                    dir,
                    sigma,
                    saturation,
                    tone,
                    tone_light: light,
                }),
            );
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

    /// Call after the frame is submitted, to free unused glyphs.
    pub fn end_frame(&mut self) {
        self.labels.trim();
    }

    pub fn map_layout(&self) -> &wgpu::BindGroupLayout {
        &self.map_bgl
    }
}
