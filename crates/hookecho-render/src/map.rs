//! Basemap GPU geometry: fills as indexed triangles, lines as instanced
//! segments. Positions are (mercator - ORIGIN) * 1e4 so f32 keeps ~2 m
//! precision anywhere in the covered regions.

use hookecho_core::basemap::{Basemap, Layer};
use hookecho_core::geo::to_mercator;
use wgpu::util::DeviceExt;

pub const ORIGIN: [f64; 2] = [0.5, 0.5];
pub const UNIT: f64 = 1.0e4;

fn pos(lat: f64, lon: f64) -> [f32; 2] {
    let m = to_mercator(lat, lon);
    [
        ((m[0] - ORIGIN[0]) * UNIT) as f32,
        ((m[1] - ORIGIN[1]) * UNIT) as f32,
    ]
}

pub enum Geometry {
    Fill {
        vbuf: wgpu::Buffer,
        ibuf: wgpu::Buffer,
        count: u32,
    },
    Lines {
        segments: wgpu::Buffer,
        count: u32,
    },
}

pub struct GpuLayer {
    pub name: String,
    pub geometry: Geometry,
    pub uniform: wgpu::Buffer,
    pub bind: wgpu::BindGroup,
    pub bytes: u64,
}

pub fn upload(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    map: &Basemap,
    uniform_size: u64,
) -> Vec<GpuLayer> {
    map.layers
        .iter()
        .map(|layer| {
            let (geometry, bytes) = match layer {
                Layer::Fill {
                    vertices, indices, ..
                } => {
                    let v: Vec<[f32; 2]> = vertices.iter().map(|&(la, lo)| pos(la, lo)).collect();
                    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(layer.name()),
                        contents: bytemuck::cast_slice(&v),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(layer.name()),
                        contents: bytemuck::cast_slice(indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                    let bytes = (v.len() * 8 + indices.len() * 4) as u64;
                    (
                        Geometry::Fill {
                            vbuf,
                            ibuf,
                            count: indices.len() as u32,
                        },
                        bytes,
                    )
                }
                Layer::Lines { lines, .. } => {
                    let mut segs: Vec<[f32; 4]> = Vec::new();
                    for l in lines {
                        let p: Vec<[f32; 2]> = l.iter().map(|&(la, lo)| pos(la, lo)).collect();
                        segs.extend(p.windows(2).map(|w| [w[0][0], w[0][1], w[1][0], w[1][1]]));
                    }
                    let segments = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(layer.name()),
                        contents: bytemuck::cast_slice(&segs),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let bytes = (segs.len() * 16) as u64;
                    (
                        Geometry::Lines {
                            segments,
                            count: segs.len() as u32,
                        },
                        bytes,
                    )
                }
            };
            let uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("map-u"),
                size: uniform_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("map"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            GpuLayer {
                name: layer.name().to_string(),
                geometry,
                uniform,
                bind,
                bytes,
            }
        })
        .collect()
}
