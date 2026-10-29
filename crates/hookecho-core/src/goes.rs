//! GOES-East satellite imagery: the ABI Level 2 Cloud and Moisture Imagery
//! (CMI) product for the CONUS sector, from NOAA's open data bucket
//! `noaa-goes19` on AWS (NOAA Open Data Dissemination; no account).
//!
//! Files are NetCDF-4 (HDF5) with `CMI` as 12-bit counts in an int16
//! (`_Unsigned`, `scale_factor`, `add_offset`, `_FillValue` -1), the scan
//! angles `x`/`y` as scaled int16, and the `goes_imager_projection`
//! attributes (GOES-R Product User Guide, PUG L2+ Vol. 5, §4.2.8 and
//! §5.1). HDF5 is read with `hdf5-pure` (pure Rust, MIT OR Apache-2.0).
//!
//! Bands offered: 1 (blue visible, 1 km; band 2 at 0.5 km is 65 MB a scan
//! and too heavy to stream), 9 (mid-level water vapor, 2 km) and 13
//! (clean infrared, 2 km). Each becomes an 8-bit image at most
//! [`MAX_WIDTH`] wide, 0 meaning no data, for the renderer to color.

use crate::poll::{Http, Source};
use crate::s3::Bucket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const BUCKET: &str = "https://noaa-goes19.s3.amazonaws.com";
pub const PRODUCT: &str = "ABI-L2-CMIPC";

/// Images wider than this are averaged down (the 1 km visible band is
/// 5000 px across; 2 km is plenty under a radar).
pub const MAX_WIDTH: usize = 2500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Band {
    Visible,
    WaterVapor,
    Infrared,
}

impl Band {
    pub const ALL: [Band; 3] = [Band::Visible, Band::Infrared, Band::WaterVapor];

    pub fn number(self) -> u8 {
        match self {
            Band::Visible => 1,
            Band::WaterVapor => 9,
            Band::Infrared => 13,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Band::Visible => "Visible",
            Band::WaterVapor => "Water vapor",
            Band::Infrared => "Infrared",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Band::Visible => "visible",
            Band::WaterVapor => "water-vapor",
            Band::Infrared => "infrared",
        }
    }

    pub fn from_key(k: &str) -> Option<Band> {
        Band::ALL.into_iter().find(|b| b.key() == k)
    }

    /// The physical range mapped onto image values 1..=255: reflectance
    /// (0-1, square-rooted for contrast) for visible, brightness
    /// temperature in K for the infrared bands, coldest at 255.
    pub fn to_index(self, v: f32) -> u8 {
        let t = match self {
            Band::Visible => v.clamp(0.0, 1.0).sqrt(),
            Band::Infrared | Band::WaterVapor => (IR_WARM_K - v) / (IR_WARM_K - IR_COLD_K),
        };
        1 + (t.clamp(0.0, 1.0) * 254.0).round() as u8
    }
}

/// Brightness temperatures mapped to image value 1 (warm) and 255 (cold).
pub const IR_WARM_K: f32 = 330.0;
pub const IR_COLD_K: f32 = 163.0;

/// The ABI fixed grid: scan angles of pixel centers, and the satellite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    /// Scan angle of column 0 / row 0 centers, and the step, radians.
    pub x0: f64,
    pub dx: f64,
    pub y0: f64,
    pub dy: f64,
    /// Sub-satellite longitude, degrees.
    pub lon0: f64,
    /// Satellite distance from Earth's center, m (perspective point height
    /// plus the semi-major axis).
    pub h: f64,
    pub req: f64,
    pub rpol: f64,
}

impl Projection {
    /// (lat, lon) degrees to scan angles (x, y), radians; `None` where the
    /// satellite can't see the point (PUG Vol. 5 §4.2.8.1).
    pub fn scan_angles(&self, lat: f64, lon: f64) -> Option<(f64, f64)> {
        let (phi, lam) = (lat.to_radians(), (lon - self.lon0).to_radians());
        let e2 = (self.req * self.req - self.rpol * self.rpol) / (self.req * self.req);
        let phic = ((self.rpol * self.rpol) / (self.req * self.req) * phi.tan()).atan();
        let rc = self.rpol / (1.0 - e2 * phic.cos().powi(2)).sqrt();
        let sx = self.h - rc * phic.cos() * lam.cos();
        let sy = -rc * phic.cos() * lam.sin();
        let sz = rc * phic.sin();
        let visible = self.h * (self.h - sx)
            >= sy * sy + (self.req * self.req) / (self.rpol * self.rpol) * sz * sz;
        if !visible {
            return None;
        }
        let y = (sz / sx).atan();
        let x = (-sy / (sx * sx + sy * sy + sz * sz).sqrt()).asin();
        Some((x, y))
    }

    /// Pixel (column, row), fractional, for (lat, lon).
    pub fn pixel(&self, lat: f64, lon: f64) -> Option<(f64, f64)> {
        let (x, y) = self.scan_angles(lat, lon)?;
        Some(((x - self.x0) / self.dx, (y - self.y0) / self.dy))
    }
}

/// One band's image, ready to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub band: Band,
    /// Scan start.
    pub time_ms: i64,
    pub width: usize,
    pub height: usize,
    /// Row-major, 0 = no data.
    pub data: Vec<u8>,
    pub proj: Projection,
    /// The S3 key it came from.
    pub key: String,
}

impl Image {
    pub fn at(&self, lat: f64, lon: f64) -> u8 {
        let Some((c, r)) = self.proj.pixel(lat, lon) else {
            return 0;
        };
        let (c, r) = (c.round(), r.round());
        if c < 0.0 || r < 0.0 || c >= self.width as f64 || r >= self.height as f64 {
            return 0;
        }
        self.data[r as usize * self.width + c as usize]
    }
}

// ----------------------------------------------------------------- decoding

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn attr_f64(a: &std::collections::HashMap<String, hdf5_pure::AttrValue>, k: &str) -> Option<f64> {
    use hdf5_pure::AttrValue::*;
    match a.get(k)? {
        F64(v) => Some(*v),
        F32(v) => Some(*v as f64),
        F64Array(v) => v.first().copied(),
        F32Array(v) => v.first().map(|x| *x as f64),
        I64(v) => Some(*v as f64),
        _ => None,
    }
}

/// A scaled int16 coordinate variable's first value and step.
fn axis(file: &hdf5_pure::File, name: &str) -> Result<(f64, f64, usize), String> {
    let d = file.dataset(name).map_err(err)?;
    let a = d.attrs().map_err(err)?;
    let scale = attr_f64(&a, "scale_factor").ok_or(format!("{name}: no scale_factor"))?;
    let offset = attr_f64(&a, "add_offset").ok_or(format!("{name}: no add_offset"))?;
    let v = d.read_i16().map_err(err)?;
    let first = *v.first().ok_or(format!("{name}: empty"))?;
    Ok((first as f64 * scale + offset, scale, v.len()))
}

/// "2026-10-29T15:26:17.3Z".
fn coverage_start(file: &hdf5_pure::File) -> Option<i64> {
    let a = file.root().attrs().ok()?;
    match a.get("time_coverage_start")? {
        hdf5_pure::AttrValue::AsciiString(s) | hdf5_pure::AttrValue::String(s) => {
            crate::alerts::parse_time(s)
        }
        _ => None,
    }
}

/// Decodes one CMI file into an [`Image`].
pub fn decode(band: Band, bytes: Vec<u8>, key: &str) -> Result<Image, String> {
    let file = hdf5_pure::File::from_bytes(bytes).map_err(err)?;
    let cmi = file.dataset("CMI").map_err(err)?;
    let shape = cmi.shape().map_err(err)?;
    let [rows, cols] = shape[..] else {
        return Err(format!("CMI has shape {shape:?}"));
    };
    let (rows, cols) = (rows as usize, cols as usize);
    let a = cmi.attrs().map_err(err)?;
    let scale = attr_f64(&a, "scale_factor").ok_or("CMI: no scale_factor")? as f32;
    let offset = attr_f64(&a, "add_offset").unwrap_or(0.0) as f32;
    let raw = cmi.read_i16().map_err(err)?;
    if raw.len() != rows * cols {
        return Err("CMI is short".into());
    }
    let (x0, dx, nx) = axis(&file, "x")?;
    let (y0, dy, ny) = axis(&file, "y")?;
    if nx != cols || ny != rows {
        return Err("x/y don't match CMI".into());
    }
    let p = file
        .dataset("goes_imager_projection")
        .and_then(|d| d.attrs())
        .map_err(err)?;
    let get = |k: &str| attr_f64(&p, k).ok_or(format!("projection: no {k}"));
    let req = get("semi_major_axis")?;
    // Average `step`×`step` blocks down to at most MAX_WIDTH across; a
    // block with any fill pixels uses only the valid ones.
    let step = cols.div_ceil(MAX_WIDTH).max(1);
    let (w, h) = (cols / step, rows / step);
    let mut data = vec![0u8; w * h];
    for r in 0..h {
        for c in 0..w {
            let (mut sum, mut n) = (0.0f32, 0u32);
            for dr in 0..step {
                let row = &raw[(r * step + dr) * cols + c * step..][..step];
                for &v in row {
                    // 12-bit unsigned counts; -1 (65535) is fill.
                    let u = v as u16;
                    if u <= 4095 {
                        sum += u as f32 * scale + offset;
                        n += 1;
                    }
                }
            }
            if n > 0 {
                data[r * w + c] = band.to_index(sum / n as f32);
            }
        }
    }
    let half = (step as f64 - 1.0) / 2.0;
    Ok(Image {
        band,
        time_ms: coverage_start(&file).unwrap_or(0),
        width: w,
        height: h,
        data,
        proj: Projection {
            x0: x0 + half * dx,
            dx: dx * step as f64,
            y0: y0 + half * dy,
            dy: dy * step as f64,
            lon0: get("longitude_of_projection_origin")?,
            h: get("perspective_point_height")? + req,
            req,
            rpol: get("semi_minor_axis")?,
        },
        key: key.to_string(),
    })
}

// ---------------------------------------------------------------- the feed

/// The S3 prefix of one UTC hour's files: `ABI-L2-CMIPC/2026/302/15/`.
pub fn hour_prefix(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let (y, _, _) = crate::tdwr::civil_ymd(days);
    let doy = days - crate::tdwr::days_from_civil(y, 1, 1) + 1;
    let hour = ms.div_euclid(3_600_000).rem_euclid(24);
    format!("{PRODUCT}/{y:04}/{doy:03}/{hour:02}/")
}

/// Whether `key` is a file of `band` (any scan mode).
pub fn is_band(key: &str, band: Band) -> bool {
    key.contains(&format!("{PRODUCT}-M")) && key.contains(&format!("C{:02}_G19_", band.number()))
}

/// The band the UI wants; shared with it.
pub type Wanted = Arc<Mutex<Option<Band>>>;

/// Polls the bucket every two minutes (CONUS scans come every five) for the
/// wanted band's newest file, downloading only when it's new.
pub struct GoesSource {
    wanted: Wanted,
    bucket: Option<Bucket>,
    current: Option<Image>,
}

impl GoesSource {
    pub fn new(wanted: Wanted) -> GoesSource {
        GoesSource {
            wanted,
            bucket: Bucket::new(BUCKET).ok(),
            current: None,
        }
    }
}

impl Source for GoesSource {
    type Data = Option<Image>;
    const NAME: &'static str = "goes";

    fn every(&self) -> Duration {
        Duration::from_secs(120)
    }

    async fn fetch(&mut self, _http: &mut Http) -> Result<Option<Option<Image>>, String> {
        let Some(band) = self.wanted.lock().ok().and_then(|w| *w) else {
            return Ok(None);
        };
        let bucket = self.bucket.as_ref().ok_or("no HTTP client")?;
        let now = crate::time::now_ms();
        // This hour, or the last one just after the hour turns.
        let mut newest = None;
        for ms in [now, now - 3_600_000] {
            let l = bucket
                .list(&hour_prefix(ms), None, None, 1000)
                .await
                .map_err(err)?;
            newest = l
                .keys
                .into_iter()
                .map(|(k, _)| k)
                .filter(|k| is_band(k, band))
                .max();
            if newest.is_some() {
                break;
            }
        }
        let key = newest.ok_or("no recent files")?;
        if self
            .current
            .as_ref()
            .is_some_and(|i| i.key == key && i.band == band)
        {
            return Ok(None);
        }
        let bytes = bucket.get(&key).await.map_err(err)?;
        let img = tokio::task::spawn_blocking(move || decode(band, bytes, &key))
            .await
            .map_err(err)??;
        self.current = Some(img.clone());
        Ok(Some(Some(img)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GOES-16's PUG example (Vol. 5 §4.2.8.1): 33.846162° N, 84.690932° W
    /// is at x = -0.024052, y = 0.095340 radians from 75.0° W.
    #[test]
    fn pug_example() {
        let p = Projection {
            x0: 0.0,
            dx: 1.0,
            y0: 0.0,
            dy: 1.0,
            lon0: -75.0,
            h: 35_786_023.0 + 6_378_137.0,
            req: 6_378_137.0,
            rpol: 6_356_752.314_14,
        };
        let (x, y) = p.scan_angles(33.846162, -84.690932).unwrap();
        assert!((x - -0.024052).abs() < 1e-6, "{x}");
        assert!((y - 0.095340).abs() < 1e-6, "{y}");
        // The far side of the Earth isn't visible.
        assert!(p.scan_angles(0.0, 105.0).is_none());
    }

    #[test]
    fn indices() {
        assert_eq!(Band::Visible.to_index(0.0), 1);
        assert_eq!(Band::Visible.to_index(1.0), 255);
        assert_eq!(Band::Infrared.to_index(IR_WARM_K), 1);
        assert_eq!(Band::Infrared.to_index(IR_COLD_K), 255);
        assert_eq!(Band::Infrared.to_index(400.0), 1);
        for b in Band::ALL {
            assert_eq!(Band::from_key(b.key()), Some(b));
        }
    }

    #[test]
    fn prefixes_and_keys() {
        let t = crate::alerts::parse_time("2026-10-29T15:26:17Z").unwrap();
        assert_eq!(hour_prefix(t), "ABI-L2-CMIPC/2026/302/15/");
        let jan1 = crate::alerts::parse_time("2027-01-01T00:10:00Z").unwrap();
        assert_eq!(hour_prefix(jan1), "ABI-L2-CMIPC/2027/001/00/");
        let k = "ABI-L2-CMIPC/2026/302/15/OR_ABI-L2-CMIPC-M6C13_G19_s20263021526173_e2026_c2026.nc";
        assert!(is_band(k, Band::Infrared));
        assert!(!is_band(k, Band::Visible));
    }

    /// A small file shaped like a real CMI file (the variables and
    /// attributes read from OR_ABI-L2-CMIPC-M6C13_G19, 2026-10-29), `cols` ×
    /// `rows` pixels with counts 0, 100, 200, ... (mod 4096) and one fill.
    fn synthetic(cols: usize, rows: usize) -> Vec<u8> {
        use hdf5_pure::{AttrValue, FileBuilder};
        let mut b = FileBuilder::new();
        let mut cmi: Vec<i16> = (0..rows * cols).map(|i| (i * 100 % 4096) as i16).collect();
        cmi[0] = -1;
        b.create_dataset("CMI")
            .with_i16_data(&cmi)
            .with_shape(&[rows as u64, cols as u64])
            .set_attr("scale_factor", AttrValue::F32Array(vec![0.06145332]))
            .set_attr("add_offset", AttrValue::F32Array(vec![89.62]))
            .set_attr("_FillValue", AttrValue::I16Array(vec![-1]));
        let xs: Vec<i16> = (0..cols as i16).collect();
        let ys: Vec<i16> = (0..rows as i16).collect();
        b.create_dataset("x")
            .with_i16_data(&xs)
            .set_attr("scale_factor", AttrValue::F32Array(vec![5.6e-5]))
            .set_attr("add_offset", AttrValue::F32Array(vec![-0.101332]));
        b.create_dataset("y")
            .with_i16_data(&ys)
            .set_attr("scale_factor", AttrValue::F32Array(vec![-5.6e-5]))
            .set_attr("add_offset", AttrValue::F32Array(vec![0.128212]));
        b.create_dataset("goes_imager_projection")
            .with_i32_data(&[-2147483647])
            .set_attr(
                "longitude_of_projection_origin",
                AttrValue::F64Array(vec![-75.0]),
            )
            .set_attr(
                "perspective_point_height",
                AttrValue::F64Array(vec![35786023.0]),
            )
            .set_attr("semi_major_axis", AttrValue::F64Array(vec![6378137.0]))
            .set_attr("semi_minor_axis", AttrValue::F64Array(vec![6356752.31414]));
        b.set_attr(
            "time_coverage_start",
            AttrValue::AsciiString("2026-10-29T15:26:17.3Z".into()),
        );
        b.finish().unwrap()
    }

    #[test]
    fn decodes_a_cmi_file() {
        let img = decode(Band::Infrared, synthetic(6, 4), "k").unwrap();
        assert_eq!((img.width, img.height), (6, 4));
        assert_eq!(img.data[0], 0, "fill");
        // Count 100 → 95.8 K: colder than IR_COLD_K, so the top index.
        assert_eq!(img.data[1], 255);
        // Count 2300 → 230.9 K.
        assert_eq!(
            img.data[23],
            Band::Infrared.to_index(2300.0 * 0.06145332 + 89.62)
        );
        assert!((img.proj.x0 - -0.101332).abs() < 1e-7, "attributes are f32");
        assert!((img.proj.dy - -5.6e-5).abs() < 1e-10);
        assert_eq!(img.proj.h, 35786023.0 + 6378137.0);
        assert_eq!(
            img.time_ms,
            crate::alerts::parse_time("2026-10-29T15:26:17Z").unwrap(),
            "to the second"
        );
        assert!(decode(Band::Infrared, b"not hdf5".to_vec(), "k").is_err());
    }

    #[test]
    fn wide_images_are_averaged_down() {
        let img = decode(Band::Infrared, synthetic(MAX_WIDTH * 2, 2), "k").unwrap();
        assert_eq!((img.width, img.height), (MAX_WIDTH, 1));
        // The first block has a fill pixel: the three valid ones still
        // make a value.
        assert_ne!(img.data[0], 0);
        assert!((img.proj.dx - 2.0 * 5.6e-5).abs() < 1e-10);
        assert!((img.proj.x0 - (-0.101332 + 0.5 * 5.6e-5)).abs() < 1e-7);
    }

    /// A real file: `HOOKECHO_GOES_FILE=path cargo test -p hookecho-core goes -- --ignored`.
    #[test]
    #[ignore]
    fn real_file() {
        let Ok(p) = std::env::var("HOOKECHO_GOES_FILE") else {
            return;
        };
        let img = decode(Band::Infrared, std::fs::read(p).unwrap(), "k").unwrap();
        assert_eq!((img.width, img.height), (2500, 1500));
        // Oklahoma City is in the CONUS sector.
        assert!(
            img.proj
                .pixel(35.47, -97.52)
                .is_some_and(|(c, r)| c > 0.0 && r > 0.0 && c < 2500.0 && r < 1500.0)
        );
        assert_ne!(img.at(35.47, -97.52), 0);
    }

    /// Against the real bucket: `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_goes() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut http = Http::new().unwrap();
        let mut s = GoesSource::new(Arc::new(Mutex::new(Some(Band::Infrared))));
        let img = rt.block_on(s.fetch(&mut http)).unwrap().unwrap().unwrap();
        assert_eq!(img.width, 2500);
        assert!(rt.block_on(s.fetch(&mut http)).unwrap().is_none());
    }
}
