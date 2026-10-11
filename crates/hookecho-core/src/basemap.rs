//! Bundled basemap format ("HEBM" v1), written by `hookecho-data basemap`.
//!
//! Coordinates are lat/lon quantized to 1e-5° (about 1 m), stored as
//! zigzag-varint deltas from the previous point. Line layers hold polylines;
//! fill layers hold triangles already tessellated by lyon at build time, so
//! nothing is tessellated at startup (KICKOFF §4: cold start < 1 s).
//!
//! ```text
//! "HEBM" u32le version(=1)
//! u32le layer count
//!   u8 kind (0 lines, 1 fill) · u8 name length · name
//!   lines: varint polyline count, then per polyline: varint n, n × (dlat, dlon)
//!   fill:  varint vertex count, vertices as (dlat, dlon); varint index count, indices as zigzag deltas
//! varint place count
//!   varint name length · name · u8 state length · state · lat · lon (zigzag, absolute) · varint rank
//! ```

pub const MAGIC: &[u8; 4] = b"HEBM";
pub const VERSION: u32 = 1;
const Q: f64 = 1e5;

#[derive(Clone, Debug, PartialEq)]
pub enum Layer {
    /// Polylines of (lat, lon).
    Lines {
        name: String,
        lines: Vec<Vec<(f64, f64)>>,
    },
    /// Triangles: vertices (lat, lon) and indices into them.
    Fill {
        name: String,
        vertices: Vec<(f64, f64)>,
        indices: Vec<u32>,
    },
}

impl Layer {
    pub fn name(&self) -> &str {
        match self {
            Layer::Lines { name, .. } | Layer::Fill { name, .. } => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: String,
    pub state: String,
    pub lat: f64,
    pub lon: f64,
    /// Larger = more important: the Census population estimate, or for
    /// census-designated places a conservative guess from land area.
    pub rank: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Basemap {
    pub layers: Vec<Layer>,
    pub places: Vec<Place>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a Hook Echo basemap")]
    Magic,
    #[error("unsupported basemap version {0}")]
    Version(u32),
    #[error("truncated basemap")]
    Truncated,
    #[error("bad layer kind {0}")]
    Kind(u8),
    #[error("coordinate out of range")]
    Range,
    #[error("text is not UTF-8")]
    Utf8,
}

fn q(v: f64) -> i64 {
    (v * Q).round() as i64
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_zigzag(out: &mut Vec<u8>, v: i64) {
    put_varint(out, ((v << 1) ^ (v >> 63)) as u64);
}

fn put_points(out: &mut Vec<u8>, pts: &[(f64, f64)]) {
    let (mut pl, mut po) = (0i64, 0i64);
    for &(lat, lon) in pts {
        let (a, b) = (q(lat), q(lon));
        put_zigzag(out, a - pl);
        put_zigzag(out, b - po);
        (pl, po) = (a, b);
    }
}

pub fn encode(map: &Basemap) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(map.layers.len() as u32).to_le_bytes());
    for layer in &map.layers {
        let name = layer.name().as_bytes();
        match layer {
            Layer::Lines { lines, .. } => {
                out.push(0);
                out.push(name.len() as u8);
                out.extend_from_slice(name);
                put_varint(&mut out, lines.len() as u64);
                for l in lines {
                    put_varint(&mut out, l.len() as u64);
                    put_points(&mut out, l);
                }
            }
            Layer::Fill {
                vertices, indices, ..
            } => {
                out.push(1);
                out.push(name.len() as u8);
                out.extend_from_slice(name);
                put_varint(&mut out, vertices.len() as u64);
                put_points(&mut out, vertices);
                put_varint(&mut out, indices.len() as u64);
                // lyon emits indices with strong locality: deltas are ~1 byte.
                let mut prev = 0i64;
                for &i in indices {
                    put_zigzag(&mut out, i as i64 - prev);
                    prev = i as i64;
                }
            }
        }
    }
    put_varint(&mut out, map.places.len() as u64);
    for p in &map.places {
        put_varint(&mut out, p.name.len() as u64);
        out.extend_from_slice(p.name.as_bytes());
        out.push(p.state.len() as u8);
        out.extend_from_slice(p.state.as_bytes());
        put_zigzag(&mut out, q(p.lat));
        put_zigzag(&mut out, q(p.lon));
        put_varint(&mut out, p.rank);
    }
    out
}

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let s = self.b.get(self.i..self.i + n).ok_or(Error::Truncated)?;
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn varint(&mut self) -> Result<u64, Error> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= ((b & 0x7f) as u64) << shift;
            if b < 0x80 {
                return Ok(v);
            }
        }
        Err(Error::Truncated)
    }
    fn zigzag(&mut self) -> Result<i64, Error> {
        let v = self.varint()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    fn len(&mut self) -> Result<usize, Error> {
        let n = self.varint()? as usize;
        // Every element takes at least one byte; reject impossible counts early.
        if n > self.b.len() - self.i.min(self.b.len()) {
            return Err(Error::Truncated);
        }
        Ok(n)
    }
    fn points(&mut self, n: usize) -> Result<Vec<(f64, f64)>, Error> {
        let (mut a, mut b) = (0i64, 0i64);
        (0..n)
            .map(|_| {
                // Checked: hostile input must not overflow, and only real
                // lat/lon is accepted, which also keeps the f64 conversion
                // exact so decode/encode round-trips (both found by fuzzing).
                a = a.checked_add(self.zigzag()?).ok_or(Error::Truncated)?;
                b = b.checked_add(self.zigzag()?).ok_or(Error::Truncated)?;
                if a.unsigned_abs() > 90 * Q as u64 || b.unsigned_abs() > 180 * Q as u64 {
                    return Err(Error::Range);
                }
                Ok((a as f64 / Q, b as f64 / Q))
            })
            .collect()
    }
    fn string(&mut self, n: usize) -> Result<String, Error> {
        // Strict: lossy replacement would change lengths and break the
        // decode/encode round trip (found by fuzzing).
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| Error::Utf8)
    }
}

pub fn decode(bytes: &[u8]) -> Result<Basemap, Error> {
    let mut r = Rd { b: bytes, i: 0 };
    if r.take(4)? != MAGIC {
        return Err(Error::Magic);
    }
    let version = u32::from_le_bytes(r.take(4)?.try_into().unwrap());
    if version != VERSION {
        return Err(Error::Version(version));
    }
    let count = u32::from_le_bytes(r.take(4)?.try_into().unwrap());
    let mut layers = Vec::new();
    for _ in 0..count {
        let kind = r.u8()?;
        let n = r.u8()? as usize;
        let name = r.string(n)?;
        layers.push(match kind {
            0 => {
                let lines = (0..r.len()?)
                    .map(|_| {
                        let n = r.len()?;
                        r.points(n)
                    })
                    .collect::<Result<_, _>>()?;
                Layer::Lines { name, lines }
            }
            1 => {
                let n = r.len()?;
                let vertices = r.points(n)?;
                let mut prev = 0i64;
                let indices = (0..r.len()?)
                    .map(|_| {
                        prev = prev.checked_add(r.zigzag()?).ok_or(Error::Truncated)?;
                        u32::try_from(prev).map_err(|_| Error::Truncated)
                    })
                    .collect::<Result<Vec<u32>, _>>()?;
                if indices.iter().any(|&i| i as usize >= vertices.len()) {
                    return Err(Error::Truncated);
                }
                Layer::Fill {
                    name,
                    vertices,
                    indices,
                }
            }
            k => return Err(Error::Kind(k)),
        });
    }
    let places = (0..r.len()?)
        .map(|_| {
            let n = r.len()?;
            let name = r.string(n)?;
            let n = r.u8()? as usize;
            let state = r.string(n)?;
            let (lat, lon) = (r.zigzag()?, r.zigzag()?);
            if lat.unsigned_abs() > 90 * Q as u64 || lon.unsigned_abs() > 180 * Q as u64 {
                return Err(Error::Range);
            }
            let (lat, lon) = (lat as f64 / Q, lon as f64 / Q);
            Ok(Place {
                name,
                state,
                lat,
                lon,
                rank: r.varint()?,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Basemap { layers, places })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let map = Basemap {
            layers: vec![
                Layer::Lines {
                    name: "state".into(),
                    lines: vec![vec![(35.0, -97.5), (35.00001, -97.49999), (36.5, -100.0)]],
                },
                Layer::Fill {
                    name: "land".into(),
                    vertices: vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
                    indices: vec![0, 1, 2],
                },
            ],
            places: vec![Place {
                name: "Moore".into(),
                state: "OK".into(),
                lat: 35.3285,
                lon: -97.47476,
                rank: 5722,
            }],
        };
        let bytes = encode(&map);
        assert_eq!(decode(&bytes).unwrap(), map);
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        assert!(decode(b"nope").is_err());
        let mut bytes = encode(&Basemap::default());
        bytes.truncate(10);
        assert!(decode(&bytes).is_err());
        let mut bytes = encode(&Basemap {
            layers: vec![],
            places: vec![],
        });
        bytes.extend_from_slice(&[0xff; 12]);
        let _ = decode(&bytes);
    }
}
