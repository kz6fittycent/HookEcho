//! Prototype basemap: polylines from `prototype/data/basemap.bin`, stroked with
//! lyon into screen-width-independent geometry (position + extrusion normal),
//! so line width stays constant in pixels at every zoom.

use crate::view::to_merc;
use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, LineJoin, Side, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    /// Mercator position relative to the layer origin (keeps f32 precision).
    pub pos: [f32; 2],
    pub normal: [f32; 2],
    /// -1 / +1 across the stroke, for edge anti-aliasing.
    pub side: f32,
}

pub struct LineLayer {
    pub name: String,
    pub origin: [f64; 2],
    pub geometry: VertexBuffers<LineVertex, u32>,
}

pub struct Place {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub land_area: u64,
}

pub fn load_lines(bytes: &[u8], origin_latlon: (f64, f64)) -> Vec<LineLayer> {
    let mut rd = Reader { b: bytes, i: 0 };
    assert_eq!(rd.take(4), b"HEB0", "basemap magic");
    let origin = to_merc(origin_latlon.0, origin_latlon.1);
    let layers = rd.u32();
    let mut out = Vec::new();
    for _ in 0..layers {
        let n = rd.take(1)[0] as usize;
        let name = String::from_utf8_lossy(rd.take(n)).into_owned();
        let count = rd.u32();
        let mut builder = Path::builder();
        for _ in 0..count {
            let pts = rd.u32() as usize;
            for k in 0..pts {
                let lon = rd.f32() as f64;
                let lat = rd.f32() as f64;
                let m = to_merc(lat, lon);
                let p = point(
                    ((m[0] - origin[0]) * 1.0e4) as f32,
                    ((m[1] - origin[1]) * 1.0e4) as f32,
                );
                if k == 0 {
                    builder.begin(p);
                } else {
                    builder.line_to(p);
                }
            }
            builder.end(false);
        }
        let path = builder.build();
        let mut geometry: VertexBuffers<LineVertex, u32> = VertexBuffers::new();
        StrokeTessellator::new()
            .tessellate_path(
                &path,
                // Width is irrelevant: we keep the path position and extrude by
                // `normal` in the shader, so lines stay constant in pixels.
                &StrokeOptions::default()
                    .with_line_width(1.0)
                    .with_line_join(LineJoin::Round),
                &mut BuffersBuilder::new(&mut geometry, |v: StrokeVertex| {
                    let p = v.position_on_path();
                    LineVertex {
                        pos: [p.x, p.y],
                        normal: v.normal().to_array(),
                        side: if v.side() == Side::Positive {
                            1.0
                        } else {
                            -1.0
                        },
                    }
                }),
            )
            .expect("tessellate basemap");
        out.push(LineLayer {
            name,
            origin,
            geometry,
        });
    }
    out
}

pub fn load_places(tsv: &str) -> Vec<Place> {
    tsv.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let name = it.next()?.to_string();
            if name.contains("government") {
                return None;
            }
            Some(Place {
                name,
                lat: it.next()?.parse().ok()?,
                lon: it.next()?.parse().ok()?,
                land_area: it.next()?.parse().ok()?,
            })
        })
        .collect()
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        s
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take(4).try_into().unwrap())
    }
}
