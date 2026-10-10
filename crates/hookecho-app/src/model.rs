//! Application state that isn't drawing: what's loaded, what's selected,
//! where the camera is. No egui widgets and no GPU calls here.

use hookecho_core::Moment;
use hookecho_core::config::{Config, Units};
use hookecho_core::geo::{self, Camera};
use hookecho_core::level2::{Sweep, Volume};
use hookecho_core::sites::{self, Site, SiteKind};

/// Products offered in the product picker, in display order.
pub const PRODUCTS: [Moment; 6] = [
    Moment::Reflectivity,
    Moment::Velocity,
    Moment::SpectrumWidth,
    Moment::DifferentialReflectivity,
    Moment::CorrelationCoefficient,
    Moment::DifferentialPhase,
];

// Arriving/Pending are produced by Phase 2's real-time chunk feed; the UI
// already renders them (sweep strip, picker).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TiltState {
    /// Data is in.
    In,
    /// Being received (Phase 2: real-time chunks).
    Arriving,
    /// In the VCP but not received.
    Pending,
}

#[derive(Clone, Debug)]
pub struct Tilt {
    /// The VCP's commanded angle, e.g. 0.5.
    pub deg: f32,
    pub state: TiltState,
}

pub struct Loaded {
    pub site: &'static Site,
    pub volume: Volume,
    /// Per sweep index: azimuth bins → radial row (for the inspector).
    az_index: Vec<Vec<u16>>,
    pub archived: bool,
}

impl Loaded {
    pub fn new(site: &'static Site, volume: Volume, archived: bool) -> Loaded {
        let az_index = volume
            .sweeps
            .iter()
            .map(|s| {
                let c: Vec<f32> = s.radials.iter().map(|r| r.azimuth_deg).collect();
                let w: Vec<f32> = s.radials.iter().map(|r| r.azimuth_spacing_deg).collect();
                geo::azimuth_index(&c, &w)
            })
            .collect();
        Loaded {
            site,
            volume,
            az_index,
            archived,
        }
    }

    /// Tilt angle used for display (nominal VCP angle, else measured).
    pub fn sweep_deg(s: &Sweep) -> f32 {
        let d = s.nominal_elevation_deg.unwrap_or(s.elevation_deg);
        (d * 10.0).round() / 10.0
    }

    /// Distinct tilts of the VCP in ascending order, with their state.
    pub fn tilts(&self) -> Vec<Tilt> {
        let mut degs: Vec<f32> = self.volume.sweeps.iter().map(Self::sweep_deg).collect();
        degs.sort_by(f32::total_cmp);
        degs.dedup();
        degs.into_iter()
            .map(|deg| Tilt {
                deg,
                state: TiltState::In,
            })
            .collect()
    }

    /// Best sweep for (moment, tilt): the widest-range sweep at that angle
    /// (the surveillance cut for reflectivity), newest on ties.
    pub fn sweep_for(&self, m: Moment, deg: f32) -> Option<usize> {
        self.volume
            .sweeps
            .iter()
            .enumerate()
            .filter(|(_, s)| (Self::sweep_deg(s) - deg).abs() < 0.05)
            .filter_map(|(i, s)| s.field(m).map(|f| (i, f.gates)))
            .max_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(i, _)| i)
    }

    pub fn has(&self, m: Moment, deg: f32) -> bool {
        self.sweep_for(m, deg).is_some()
    }

    /// Value probe for the inspector at (lat, lon).
    pub fn probe(&self, sweep: usize, m: Moment, at: (f64, f64)) -> Probe {
        let s = &self.volume.sweeps[sweep];
        let site = (self.site.lat, self.site.lon);
        let (ground_km, bearing) = geo::range_bearing(site, at);
        let slant = geo::slant_range_km(ground_km, s.elevation_deg as f64);
        let height_km = geo::beam_height_km(slant, s.elevation_deg as f64);
        let mut p = Probe {
            raw: None,
            value: None,
            ground_km,
            bearing_deg: bearing,
            height_km,
        };
        let Some(f) = s.field(m) else { return p };
        let gate = ((slant as f32 - f.first_gate_km) / f.gate_km + 0.5).floor();
        if gate < 0.0 || gate as usize >= f.gates {
            return p;
        }
        let bin = ((bearing as f32 * 10.0) as usize).min(geo::AZIMUTH_BINS - 1);
        let row = self.az_index[sweep][bin];
        if row == u16::MAX {
            return p;
        }
        let raw = f.raw_at(row as usize, gate as usize);
        p.raw = Some(raw);
        p.value = f.value(raw);
        p
    }

    /// Scan start, ms since the epoch.
    pub fn time_ms(&self) -> Option<i64> {
        self.volume.sweeps.first()?.time_range_ms().map(|t| t.0)
    }
}

pub struct Probe {
    pub raw: Option<u16>,
    pub value: Option<f32>,
    pub ground_km: f64,
    pub bearing_deg: f64,
    pub height_km: f64,
}

#[derive(Clone, Debug, Default)]
pub enum Picker {
    #[default]
    None,
    Site {
        query: String,
        selected: usize,
        scroll: f32,
    },
    Product {
        row: usize,
        col: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Inspect {
    Off,
    /// Follows the pointer.
    Hover((f64, f64)),
    /// Pinned to a location (key I, and screenshots).
    Pinned((f64, f64)),
}

/// Smooth map motion: zoom eases toward a target; drags fling with decay.
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub zoom_target: f64,
    pub zoom_anchor: [f64; 2],
    pub fling: [f64; 2],
}

pub struct Model {
    pub config: Config,
    pub site: &'static Site,
    pub loaded: Option<Loaded>,
    pub moment: Moment,
    pub tilt_deg: f32,
    pub camera: Camera,
    pub motion: Motion,
    pub picker: Picker,
    pub inspect: Inspect,
    pub reduced_motion: bool,
    /// Uploaded GPU sweeps keyed by (sweep index, moment), most recent last.
    pub uploads: Vec<((usize, Moment), hookecho_render::SweepId)>,
}

/// GPU sweeps kept resident (Phase 2 replaces this with the frame LRU).
pub const UPLOAD_CACHE: usize = 24;

impl Model {
    pub fn new(config: Config, initial: &'static Site, size: [f64; 2]) -> Model {
        let camera = Camera::looking_at(initial.lat, initial.lon, 260.0, size[0].max(1.0));
        Model {
            reduced_motion: config.reduced_motion,
            config,
            site: initial,
            loaded: None,
            moment: Moment::Reflectivity,
            tilt_deg: 0.5,
            camera,
            motion: Motion {
                zoom_target: camera.scale,
                zoom_anchor: [size[0] / 2.0, size[1] / 2.0],
                fling: [0.0; 2],
            },
            picker: Picker::None,
            inspect: Inspect::Off,
            uploads: Vec::new(),
        }
    }

    /// The sweep index currently shown, if any.
    pub fn current_sweep(&self) -> Option<usize> {
        let l = self.loaded.as_ref().filter(|l| l.site.id == self.site.id)?;
        l.sweep_for(self.moment, self.tilt_deg).or_else(|| {
            // Fall back to the lowest tilt that has this moment.
            l.tilts()
                .iter()
                .find_map(|t| l.sweep_for(self.moment, t.deg))
        })
    }

    pub fn loaded_here(&self) -> Option<&Loaded> {
        self.loaded.as_ref().filter(|l| l.site.id == self.site.id)
    }

    pub fn select_site(&mut self, site: &'static Site, size: [f64; 2]) {
        self.site = site;
        self.config.site = Some(site.id.clone());
        let km = self.camera.km_per_px() * size[0];
        // Fly to the site at a radar-sized zoom unless already closer in.
        let target = Camera::looking_at(site.lat, site.lon, km.clamp(120.0, 500.0), size[0]);
        self.camera.center = target.center;
        self.camera.scale = target.scale;
        self.motion.zoom_target = target.scale;
        self.motion.fling = [0.0; 2];
    }

    /// Steps the tilt up/down among tilts that have the current moment.
    pub fn step_tilt(&mut self, dir: i32) {
        let Some(l) = self.loaded_here() else { return };
        let tilts: Vec<f32> = l
            .tilts()
            .into_iter()
            .map(|t| t.deg)
            .filter(|&d| l.has(self.moment, d))
            .collect();
        let i = tilts
            .iter()
            .position(|&d| (d - self.tilt_deg).abs() < 0.05)
            .unwrap_or(0) as i32;
        if let Some(&d) = tilts.get((i + dir).clamp(0, tilts.len() as i32 - 1) as usize) {
            self.tilt_deg = d;
        }
    }

    pub fn toggle_favorite(&mut self, id: &str) {
        if let Some(p) = self.config.favorites.iter().position(|f| f == id) {
            self.config.favorites.remove(p);
        } else {
            self.config.favorites.push(id.to_string());
        }
    }
}

/// How well a site matches the query: lower is better, `None` = no match.
/// State code, then ID prefix, then a name word prefix, then a substring.
fn match_rank(s: &Site, q: &str) -> Option<u8> {
    if q.is_empty() {
        return Some(0);
    }
    let name = s.name.to_ascii_lowercase();
    if s.id.eq_ignore_ascii_case(q) || s.state.eq_ignore_ascii_case(q) {
        Some(0)
    } else if s.id.to_ascii_lowercase().starts_with(q)
        || s.id[1..].to_ascii_lowercase().starts_with(q)
    {
        Some(1)
    } else if name.split_whitespace().any(|w| w.starts_with(q)) {
        Some(2)
    } else if name.contains(q) {
        Some(3)
    } else {
        None
    }
}

/// Sites the picker offers, filtered by `query` (state, ID, or name).
/// Favorites first, then operational NEXRADs by match quality, state, ID.
pub fn site_list(query: &str, favorites: &[String]) -> Vec<(&'static Site, bool)> {
    let q = query.trim().to_ascii_lowercase();
    let matches = |s: &Site| match_rank(s, &q).is_some();
    let mut fav: Vec<(&'static Site, bool)> = favorites
        .iter()
        .filter_map(|id| sites::by_id(id))
        .filter(|s| matches(s))
        .map(|s| (s, true))
        .collect();
    let mut rest: Vec<&'static Site> = sites::all()
        .iter()
        .filter(|s| s.kind == SiteKind::Nexrad && matches(s) && !favorites.contains(&s.id))
        .collect();
    rest.sort_by(|a, b| {
        match_rank(a, &q)
            .cmp(&match_rank(b, &q))
            .then(a.state.cmp(&b.state))
            .then(a.id.cmp(&b.id))
    });
    fav.extend(rest.into_iter().map(|s| (s, false)));
    fav
}

/// Display formatting by unit system.
pub struct Fmt(pub Units);

impl Fmt {
    pub fn distance(&self, km: f64) -> String {
        match self.0 {
            Units::Imperial => format!("{:.1} mi", km * 0.621_371),
            Units::Metric => format!("{km:.1} km"),
        }
    }

    pub fn height(&self, km: f64) -> String {
        match self.0 {
            Units::Imperial => format!("{:.0} ft ARL", (km * 3280.84 / 10.0).round() * 10.0),
            Units::Metric => format!("{:.0} m ARL", (km * 1000.0 / 10.0).round() * 10.0),
        }
    }

    /// (value text, unit text) for a decoded moment value.
    pub fn value(&self, m: Moment, v: f32) -> (String, &'static str) {
        let speed = |v: f32| match self.0 {
            Units::Imperial => (v * 1.943_844, "kt"),
            Units::Metric => (v, "m/s"),
        };
        match m {
            Moment::Reflectivity => (format!("{v:.1}"), "dBZ"),
            Moment::Velocity => {
                let (s, u) = speed(v);
                (format!("{s:+.0}"), u)
            }
            Moment::SpectrumWidth => {
                let (s, u) = speed(v);
                (format!("{s:.0}"), u)
            }
            Moment::DifferentialReflectivity => (format!("{v:+.1}"), "dB"),
            Moment::CorrelationCoefficient => (format!("{v:.2}"), ""),
            Moment::DifferentialPhase => (format!("{v:.0}"), "°"),
            Moment::ClutterFilterPower => (format!("{v:.1}"), "dB"),
        }
    }

    /// Legend unit label and the factor from native units.
    pub fn legend_unit(&self, m: Moment) -> (&'static str, f32) {
        match (m, self.0) {
            (Moment::Velocity | Moment::SpectrumWidth, Units::Imperial) => ("kt", 1.943_844),
            (Moment::Velocity | Moment::SpectrumWidth, Units::Metric) => ("m/s", 1.0),
            (m, _) => (m.unit(), 1.0),
        }
    }
}

/// "May 20, 2013 · 20:16 UTC" for archived scans (deterministic: UTC).
pub fn archive_time(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_millis_opt(ms)
        .single()
        .map_or_else(String::new, |t| {
            t.format("%b %-d, %Y · %H:%M UTC").to_string()
        })
}

/// Cache of GPU uploads: returns the id for (sweep, moment), uploading on
/// first use and evicting the oldest beyond `UPLOAD_CACHE`.
pub fn ensure_upload(
    model: &mut Model,
    renderer: &mut hookecho_render::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sweep: usize,
    m: Moment,
) -> Option<hookecho_render::SweepId> {
    if let Some(i) = model.uploads.iter().position(|(k, _)| *k == (sweep, m)) {
        let e = model.uploads.remove(i);
        let id = e.1;
        model.uploads.push(e);
        return Some(id);
    }
    let l = model.loaded.as_ref()?;
    let id = renderer.upload_sweep(device, queue, &l.volume.sweeps[sweep], m)?;
    model.uploads.push(((sweep, m), id));
    while model.uploads.len() > UPLOAD_CACHE {
        let (_, old) = model.uploads.remove(0);
        renderer.remove_sweep(old);
    }
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_search() {
        let all = site_list("", &[]);
        assert!(all.len() >= 155);
        let ok = site_list("ok", &[]);
        assert!(
            ok.iter().take(4).all(|(s, _)| s.state == "OK"),
            "state matches rank first"
        );
        assert!(ok.iter().any(|(s, _)| s.id == "KTLX"));
        let tul = site_list("tul", &[]);
        assert_eq!(tul[0].0.id, "KINX", "name word prefix (Tulsa)");
        let t = site_list("ktlx", &[]);
        assert_eq!(t[0].0.id, "KTLX", "exact ID first");
        assert!(
            site_list("tlx", &[]).iter().any(|(s, _)| s.id == "KTLX"),
            "ID without the K"
        );
        let fav = site_list("", &["KFWS".into()]);
        assert!(fav[0].1 && fav[0].0.id == "KFWS", "favorites first");
        assert!(site_list("koun", &[]).is_empty(), "research radars hidden");
    }

    #[test]
    fn archive_time_format() {
        assert_eq!(archive_time(1_369_080_003_000), "May 20, 2013 · 20:00 UTC");
    }

    #[test]
    fn tilts_and_best_sweep_for_moore() {
        let v = hookecho_core::level2::decode_volume(crate::assets::SAMPLE_VOLUME).unwrap();
        let l = Loaded::new(sites::by_id("KTLX").unwrap(), v, true);
        let tilts: Vec<f32> = l.tilts().iter().map(|t| t.deg).collect();
        assert_eq!(tilts.first(), Some(&0.5));
        assert!(
            tilts.len() >= 12,
            "VCP 12 has 14 cuts at ~12+ distinct angles: {tilts:?}"
        );
        // Reflectivity at 0.5° comes from the long-range surveillance cut.
        let i = l.sweep_for(Moment::Reflectivity, 0.5).unwrap();
        assert_eq!(
            l.volume.sweeps[i]
                .field(Moment::Reflectivity)
                .unwrap()
                .gates,
            1832
        );
        // Velocity at 0.5° comes from the Doppler cut.
        assert!(l.sweep_for(Moment::Velocity, 0.5).is_some());
    }
}
