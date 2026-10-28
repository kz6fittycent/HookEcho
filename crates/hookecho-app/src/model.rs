//! Application state that isn't drawing: what's loaded, what's selected,
//! where the camera is. No egui widgets and no GPU calls here.

use hookecho_core::Moment;
use hookecho_core::config::{Config, Units};
use hookecho_core::geo::{self, Camera};
use hookecho_core::level2::{Sweep, Volume};
use hookecho_core::sites::{self, Site, SiteKind};

/// Plain-language name and one-line "what it shows" for the product
/// picker and chip. Written for someone who has never read a radar manual;
/// the abbreviations chasers know stay in parentheses.
pub fn product_label(m: Moment) -> (&'static str, &'static str) {
    match m {
        Moment::Reflectivity => ("Reflectivity", "Rain, hail and snow, and how heavy"),
        Moment::Velocity => (
            "Velocity",
            "Wind toward (blue) or away from (orange) the radar",
        ),
        Moment::SpectrumWidth => ("Spectrum width", "How turbulent the wind is"),
        Moment::DifferentialReflectivity => (
            "Differential reflectivity (ZDR)",
            "Shape of raindrops and hail",
        ),
        Moment::CorrelationCoefficient => (
            "Correlation coefficient (CC)",
            "Mixed precipitation and tornado debris",
        ),
        Moment::DifferentialPhase => ("Differential phase (PHI)", "Heavy rain (advanced)"),
        Moment::ClutterFilterPower => (
            "Clutter filter power",
            "Ground clutter removed (diagnostic)",
        ),
    }
}

/// Products offered in the product picker, in display order.
pub const PRODUCTS: [Moment; 6] = [
    Moment::Reflectivity,
    Moment::Velocity,
    Moment::SpectrumWidth,
    Moment::DifferentialReflectivity,
    Moment::CorrelationCoefficient,
    Moment::DifferentialPhase,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TiltState {
    /// A complete sweep is in.
    In,
    /// Being received right now (real-time chunks).
    Arriving,
    /// In the VCP but not scanned yet.
    Pending,
}

#[derive(Clone, Debug)]
pub struct Tilt {
    /// The VCP's commanded angle, e.g. 0.5.
    pub deg: f32,
    pub state: TiltState,
}

/// A live volume's identity and progress.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveInfo {
    /// `SITE/vol/STAMP`, stable across chunks of one volume.
    pub prefix: String,
    pub chunks: u16,
    pub complete: bool,
}

pub struct Loaded {
    pub site: &'static Site,
    pub volume: Volume,
    /// Per sweep index: azimuth bins → radial row (for the inspector).
    az_index: Vec<Vec<u16>>,
    pub archived: bool,
    /// `Some` for real-time data.
    pub live: Option<LiveInfo>,
    /// Changes whenever the data changes (GPU upload cache key).
    pub generation: u64,
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
            live: None,
            generation: 0,
        }
    }

    /// Newest radial collection time, ms since the epoch (the data's age).
    pub fn newest_ms(&self) -> Option<i64> {
        self.volume
            .sweeps
            .iter()
            .filter_map(|s| s.time_range_ms().map(|t| t.1))
            .max()
    }

    /// Tilt angle used for display (nominal VCP angle, else measured).
    pub fn sweep_deg(s: &Sweep) -> f32 {
        let d = s.nominal_elevation_deg.unwrap_or(s.elevation_deg);
        (d * 10.0).round() / 10.0
    }

    /// Distinct tilts of the VCP in ascending order, with their state:
    /// in (a complete sweep), arriving (partial), or pending (not scanned).
    pub fn tilts(&self) -> Vec<Tilt> {
        let round = |d: f32| (d * 10.0).round() / 10.0;
        let mut degs: Vec<f32> = if self.volume.vcp_cuts_deg.is_empty() {
            self.volume.sweeps.iter().map(Self::sweep_deg).collect()
        } else {
            self.volume.vcp_cuts_deg.iter().map(|&d| round(d)).collect()
        };
        degs.sort_by(f32::total_cmp);
        degs.dedup();
        degs.into_iter()
            .map(|deg| {
                let at: Vec<&Sweep> = self
                    .volume
                    .sweeps
                    .iter()
                    .filter(|s| (Self::sweep_deg(s) - deg).abs() < 0.05)
                    .collect();
                let state = if at.iter().any(|s| s.complete) {
                    TiltState::In
                } else if !at.is_empty() {
                    TiltState::Arriving
                } else {
                    TiltState::Pending
                };
                Tilt { deg, state }
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

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Picker {
    #[default]
    None,
    Site {
        query: String,
        selected: usize,
        scroll: f32,
    },
    Product,
    /// Color palettes for the product on screen (opened from the legend).
    Palette,
    /// Command palette (Ctrl+K).
    Command {
        query: String,
        selected: usize,
        scroll: f32,
    },
    /// Keyboard shortcuts sheet (`?`).
    Shortcuts,
}

/// Side sheet sections (DESIGN.md §3.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetTab {
    Layers,
    Colors,
    Settings,
}

impl SheetTab {
    pub const ALL: [SheetTab; 3] = [SheetTab::Layers, SheetTab::Colors, SheetTab::Settings];

    pub fn label(self) -> &'static str {
        match self {
            SheetTab::Layers => "Layers",
            SheetTab::Colors => "Colors",
            SheetTab::Settings => "Settings",
        }
    }
}

/// Side sheet state. `cache_bytes` is measured when Settings opens.
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub tab: SheetTab,
    pub scroll: f32,
    pub cache_bytes: Option<u64>,
    /// Set by the Clear button; the app clears the cache and resets it.
    pub clear_cache: bool,
}

impl Sheet {
    pub fn new(tab: SheetTab) -> Sheet {
        Sheet {
            tab,
            scroll: 0.0,
            cache_bytes: None,
            clear_cache: false,
        }
    }
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
    /// Uploaded GPU sweeps keyed by (data generation, sweep index, moment),
    /// most recently used last.
    pub uploads: Vec<((u64, usize, Moment), hookecho_render::SweepId)>,
    /// The last complete volume, drawn under a sweep that's still arriving.
    pub previous: Option<Loaded>,
    /// Live feed state for the current site (None for archived/sample data).
    pub feed: Option<hookecho_core::live::FeedState>,
    pub looping: Loop,
    pub palettes: crate::palettes::Library,
    /// A short message and the UI time (s) it disappears.
    pub notice: Option<(String, f64)>,
    /// Cached `no_precipitation` answer, keyed by (generation, sweep, palette version).
    quiet: Option<((u64, usize, u64), bool)>,
    pub sheet: Option<Sheet>,
    /// Last frame's glass panels (logical px, [x0, y0, x1, y1]).
    pub chrome: Vec<[f32; 4]>,
    generation: u64,
}

/// The animation loop (DESIGN.md §3.3): earlier volumes at the current tilt,
/// oldest first; the live volume is always the last position.
#[derive(Default)]
pub struct Loop {
    pub frames: Vec<Loaded>,
    /// Shown frame; `None` = live.
    pub index: Option<usize>,
    pub playing: bool,
    /// UI clock time (s) of the next frame change while playing.
    pub next_at: f64,
    /// The request the frames belong to.
    pub request: Option<hookecho_core::frames::LoopRequest>,
}

/// Seconds per frame at 1× speed; the newest frame holds three times longer
/// so the loop reads as "...and this is now".
pub const FRAME_DWELL_S: f64 = 0.45;
pub const LIVE_DWELL_FACTOR: f64 = 3.0;

/// How long a notice stays up, seconds.
pub const NOTICE_S: f64 = 4.0;

/// Dual-pol gates show only where the reflectivity at the same range is
/// at least this (dBZ) while `mask_dualpol` is on.
pub const DUALPOL_MIN_DBZ: f32 = 5.0;

/// Moments the dual-pol mask applies to.
pub fn is_dualpol(m: Moment) -> bool {
    matches!(
        m,
        Moment::DifferentialReflectivity
            | Moment::DifferentialPhase
            | Moment::CorrelationCoefficient
    )
}

/// Less echo than this (km², about a 5 km shower) reads as "little or no
/// rain or snow".
pub const QUIET_KM2: f64 = 20.0;

/// Area the palette clearly draws, in runs of at least 1 km (four
/// quarter-km gates) along a radial, so clear-air speckle doesn't count.
pub fn echo_area_km2(
    sweep: &Sweep,
    f: &hookecho_core::level2::Field,
    p: &hookecho_render::palette::Palette,
) -> f64 {
    let spacing = sweep.radials.first().map_or(1.0, |r| r.azimuth_spacing_deg);
    f.echo_area_km2(|v| p.sample(v)[3] >= 128, 4, spacing)
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
            previous: None,
            feed: None,
            looping: Loop::default(),
            palettes: crate::palettes::Library::load(None),
            notice: None,
            quiet: None,
            sheet: None,
            chrome: Vec::new(),
            generation: 0,
        }
    }

    /// What's on screen: the selected loop frame, or the live volume.
    pub fn shown(&self) -> Option<&Loaded> {
        match self.looping.index {
            Some(i) => self
                .looping
                .frames
                .get(i)
                .filter(|f| f.site.id == self.site.id),
            None => self.loaded_here(),
        }
    }

    /// Loop positions for the timeline: (time ms, is live), oldest first.
    pub fn timeline(&self) -> Vec<(i64, bool)> {
        let mut v: Vec<(i64, bool)> = self
            .looping
            .frames
            .iter()
            .filter_map(|f| f.time_ms().map(|t| (t, false)))
            .collect();
        if let Some(t) = self.loaded_here().and_then(|l| l.time_ms()) {
            v.push((t, true));
        }
        v
    }

    /// Position on the timeline of what's shown.
    pub fn timeline_pos(&self) -> usize {
        self.looping.index.unwrap_or(self.looping.frames.len())
    }

    /// Shows timeline position `pos` (the last position is live).
    pub fn show_pos(&mut self, pos: usize) {
        self.looping.index = (pos < self.looping.frames.len()).then_some(pos);
    }

    /// Steps one frame back (-1) or forward (+1), wrapping when playing.
    pub fn step_frame(&mut self, dir: i32) {
        let n = self.looping.frames.len() + 1;
        let pos = self.timeline_pos() as i32 + dir;
        self.show_pos(pos.clamp(0, n as i32 - 1) as usize);
    }

    pub fn toggle_play(&mut self, now: f64) {
        self.looping.playing = !self.looping.playing && !self.looping.frames.is_empty();
        self.looping.next_at = now;
        if !self.looping.playing {
            self.looping.index = None; // pausing returns to live, like the original
        }
    }

    /// Advances playback; returns the time of the next change, if playing.
    pub fn advance(&mut self, now: f64) -> Option<f64> {
        if !self.looping.playing || self.looping.frames.is_empty() {
            self.looping.playing = false;
            return None;
        }
        let speed = self.config.loop_speed.clamp(0.25, 4.0) as f64;
        while now >= self.looping.next_at {
            let n = self.looping.frames.len() + 1;
            let pos = (self.timeline_pos() + 1) % n;
            self.show_pos(pos);
            let dwell = FRAME_DWELL_S / speed * if pos == n - 1 { LIVE_DWELL_FACTOR } else { 1.0 };
            self.looping.next_at = self.looping.next_at.max(now - dwell) + dwell;
        }
        Some(self.looping.next_at)
    }

    /// The loop request this view needs, if any.
    pub fn loop_request(&self) -> Option<hookecho_core::frames::LoopRequest> {
        let live = self.loaded_here()?.live.as_ref()?;
        Some(hookecho_core::frames::LoopRequest {
            site: self.site.id.clone(),
            tilt_deg: self.tilt_deg,
            frames: self.config.loop_frames.saturating_sub(1),
            before: live.prefix.clone(),
        })
    }

    /// Adopts a new request: keeps frames that still fit (same site and
    /// tilt, older than the live volume), drops the rest.
    pub fn set_loop_request(&mut self, r: hookecho_core::frames::LoopRequest) {
        let tilt = r.tilt_deg;
        let before = r.before.clone();
        let keep = |f: &Loaded| {
            f.site.id == r.site
                && f.live
                    .as_ref()
                    .is_some_and(|i| stamp_of(&i.prefix) < stamp_of(&before))
                && f.volume
                    .sweeps
                    .iter()
                    .any(|s| (Loaded::sweep_deg(s) - tilt).abs() < 0.05)
        };
        let shown = self
            .looping
            .index
            .and_then(|i| self.looping.frames.get(i))
            .and_then(|f| f.live.as_ref().map(|l| l.prefix.clone()));
        self.looping.frames.retain(keep);
        self.looping.frames.truncate(r.frames.max(1) * 2);
        self.looping.index = shown.and_then(|p| {
            self.looping
                .frames
                .iter()
                .position(|f| f.live.as_ref().is_some_and(|l| l.prefix == p))
        });
        self.looping.request = Some(r);
    }

    /// Takes a loop frame for the current request (others are stale).
    pub fn accept_frame(&mut self, f: hookecho_core::frames::LoopFrame) {
        if self.looping.request.as_ref() != Some(&f.request) {
            return;
        }
        if self
            .looping
            .frames
            .iter()
            .any(|x| x.live.as_ref().is_some_and(|i| i.prefix == f.prefix))
        {
            return;
        }
        let Some(site) = sites::by_id(&f.request.site) else {
            return;
        };
        self.generation += 1;
        let volume = std::sync::Arc::try_unwrap(f.volume).unwrap_or_else(|a| (*a).clone());
        let mut l = Loaded::new(site, volume, false);
        l.generation = self.generation;
        l.live = Some(LiveInfo {
            prefix: f.prefix,
            chunks: 0,
            complete: true,
        });
        let shown = self
            .looping
            .index
            .and_then(|i| self.looping.frames.get(i))
            .and_then(|x| x.live.as_ref().map(|l| l.prefix.clone()));
        self.looping.frames.push(l);
        self.looping
            .frames
            .sort_by_key(|x| x.live.as_ref().map(|i| stamp_of(&i.prefix).to_string()));
        // Keep only the newest frames the loop length allows.
        let keep = f.request.frames.max(1);
        if self.looping.frames.len() > keep {
            self.looping
                .frames
                .drain(..self.looping.frames.len() - keep);
        }
        if let Some(p) = shown {
            self.looping.index = self
                .looping
                .frames
                .iter()
                .position(|x| x.live.as_ref().is_some_and(|l| l.prefix == p));
        }
    }

    /// The sweep index currently shown, if any.
    /// True when the shown live reflectivity sweep is complete and has
    /// (almost) nothing the palette draws, as in clear air. A blank map
    /// otherwise looks broken. The bar is low: a single small shower is
    /// a few hundred gates, so it never hides real weather.
    pub fn no_precipitation(&mut self) -> bool {
        if self.moment != Moment::Reflectivity {
            return false;
        }
        let Some(i) = self.current_sweep() else {
            return false;
        };
        let version = self.palettes.version;
        let Some(l) = self.shown() else { return false };
        if l.archived || l.live.is_none() {
            return false;
        }
        let key = (l.generation, i, version);
        if let Some((k, v)) = self.quiet
            && k == key
        {
            return v;
        }
        let sweep = &l.volume.sweeps[i];
        let palette = self.palettes.active(&self.config, Moment::Reflectivity);
        let quiet = sweep.complete
            && sweep
                .field(Moment::Reflectivity)
                .is_some_and(|f| echo_area_km2(sweep, f, palette) < QUIET_KM2);
        self.quiet = Some((key, quiet));
        quiet
    }

    /// Opens the side sheet (closing any picker), or closes it.
    pub fn toggle_sheet(&mut self) {
        self.sheet = match self.sheet {
            Some(_) => None,
            None => Some(Sheet::new(SheetTab::Layers)),
        };
        self.picker = Picker::None;
    }

    pub fn current_sweep(&self) -> Option<usize> {
        let l = self.shown()?;
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

    /// Takes a live update for the current site. When a new volume starts,
    /// the finished one becomes `previous`, so its sweeps keep showing under
    /// the new ones as they arrive.
    pub fn accept_live(&mut self, u: hookecho_core::live::LiveUpdate) {
        if u.site != self.site.id {
            return;
        }
        let Some(site) = sites::by_id(&u.site) else {
            return;
        };
        self.generation += 1;
        let volume = std::sync::Arc::try_unwrap(u.volume).unwrap_or_else(|a| (*a).clone());
        let mut l = Loaded::new(site, volume, false);
        l.generation = self.generation;
        l.live = Some(LiveInfo {
            prefix: u.prefix,
            chunks: u.chunks,
            complete: u.complete,
        });
        if let Some(old) = self.loaded.take() {
            let new_volume =
                old.live.as_ref().map(|i| &i.prefix) != l.live.as_ref().map(|i| &i.prefix);
            if new_volume
                && old.site.id == l.site.id
                && old.live.as_ref().is_some_and(|i| i.complete)
            {
                self.previous = Some(old);
            }
        }
        self.loaded = Some(l);
    }

    /// The previous volume's sweep for the current product and tilt, drawn
    /// under the current sweep while that one is still arriving.
    pub fn underlay_sweep(&self) -> Option<usize> {
        if self.looping.index.is_some() {
            return None;
        }
        let cur = self.loaded_here()?;
        let i = self.current_sweep()?;
        if cur.volume.sweeps[i].complete {
            return None;
        }
        let prev = self
            .previous
            .as_ref()
            .filter(|p| p.site.id == self.site.id)?;
        prev.sweep_for(self.moment, self.tilt_deg)
    }

    pub fn select_site(&mut self, site: &'static Site, size: [f64; 2]) {
        if site.id != self.site.id {
            self.loaded = None;
            self.previous = None;
            self.feed = None;
            self.looping = Loop::default();
        }
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

    /// The (lat, lon) at the middle of the map.
    pub fn map_center(&self) -> (f64, f64) {
        hookecho_core::geo::from_mercator(self.camera.center)
    }

    /// Makes the map center the home point.
    pub fn set_home_here(&mut self) {
        self.config.home = Some(self.map_center());
    }

    /// Switches to the NEXRAD nearest home, or to the map center if no home
    /// is set.
    pub fn go_to_nearest(&mut self, size: [f64; 2]) {
        let (lat, lon) = self.config.home.unwrap_or_else(|| self.map_center());
        if let Some((s, _)) = sites::nearest(lat, lon, SiteKind::Nexrad) {
            self.select_site(s, size);
        }
    }
}

/// The site to open on: the last one viewed; on a first run, the NEXRAD
/// nearest home; otherwise the bundled sample's site.
pub fn initial_site(config: &hookecho_core::config::Config, fallback: &str) -> &'static Site {
    config
        .site
        .as_deref()
        .and_then(sites::by_id)
        .or_else(|| {
            let (lat, lon) = config.home?;
            sites::nearest(lat, lon, SiteKind::Nexrad).map(|(s, _)| s)
        })
        .unwrap_or_else(|| sites::by_id(fallback).expect("fallback site"))
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
/// Favorites first, then operational NEXRADs: nearest to `near` first when
/// there's no query (so the radars around the map come up top), otherwise
/// by match quality, state, ID.
pub fn site_list(
    query: &str,
    favorites: &[String],
    near: Option<(f64, f64)>,
) -> Vec<(&'static Site, bool)> {
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
    match near {
        Some(p) if q.is_empty() => {
            let km = |s: &Site| hookecho_core::geo::range_bearing(p, (s.lat, s.lon)).0;
            rest.sort_by(|a, b| km(a).total_cmp(&km(b)));
        }
        _ => rest.sort_by(|a, b| {
            match_rank(a, &q)
                .cmp(&match_rank(b, &q))
                .then(a.state.cmp(&b.state))
                .then(a.id.cmp(&b.id))
        }),
    }
    fav.extend(rest.into_iter().map(|s| (s, false)));
    fav
}

/// Display formatting by unit system.
pub struct Fmt(pub Units, pub hookecho_core::config::Speed);

impl Fmt {
    pub fn of(c: &Config) -> Fmt {
        Fmt(c.units, c.speed)
    }

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
        let (unit, k) = self.1.resolve(self.0);
        let speed = |v: f32| (v * k, unit);
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
        match m {
            Moment::Velocity | Moment::SpectrumWidth => self.1.resolve(self.0),
            m => (m.unit(), 1.0),
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

/// How the site chip's status dot reads (DESIGN.md §3.1, §4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    Fresh,
    Aging,
    Stale,
    /// Archived data (accent), not a freshness judgment.
    Archive,
    /// Nothing to judge yet (connecting, no data loaded).
    Muted,
}

/// Expected time between volumes for a VCP: about 10 minutes in clear air
/// and 6 in precipitation (NWS VCP documentation; SAILS can be faster).
fn expected_volume_s(vcp: u16) -> i64 {
    if hookecho_core::level2::vcp_is_clear_air(vcp) {
        600
    } else {
        360
    }
}

fn ago(age_s: i64) -> String {
    match age_s {
        s if s < 120 => "Live".to_string(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 172_800 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86_400),
    }
}

impl Model {
    /// The site chip's second line and its dot: freshness for live data,
    /// connection trouble, or archive context.
    pub fn status_line(&self, now_ms: i64) -> (String, StatusKind) {
        use hookecho_core::live::FeedState;
        let mode = |vcp: u16| {
            if hookecho_core::level2::vcp_is_clear_air(vcp) {
                "Clear-air mode"
            } else {
                "Precipitation mode"
            }
        };
        let l = self.loaded_here();
        match (l, &self.feed) {
            (Some(l), _) if l.archived => (
                format!("Archived scan · {}", mode(l.volume.vcp)),
                StatusKind::Archive,
            ),
            (Some(l), feed) => {
                let age = ((now_ms - l.newest_ms().unwrap_or(now_ms)) / 1000).max(0);
                let expected = expected_volume_s(l.volume.vcp);
                let kind = if age <= expected {
                    StatusKind::Fresh
                } else if age <= 2 * expected {
                    StatusKind::Aging
                } else {
                    StatusKind::Stale
                };
                match feed {
                    Some(FeedState::Retrying { .. }) => (
                        format!(
                            "Offline · data {}",
                            ago(age).to_lowercase().replace("live", "current")
                        ),
                        StatusKind::Stale,
                    ),
                    _ => (format!("{} · {}", ago(age), mode(l.volume.vcp)), kind),
                }
            }
            (None, Some(FeedState::Connecting)) => ("Connecting…".to_string(), StatusKind::Muted),
            (None, Some(FeedState::Live)) => {
                ("Waiting for the first scan…".to_string(), StatusKind::Muted)
            }
            (None, Some(FeedState::Retrying { wait_s, .. })) => (
                format!("Can't reach NOAA · retrying in {wait_s} s"),
                StatusKind::Stale,
            ),
            (None, Some(FeedState::NoData)) => {
                (format!("No data from {}", self.site.id), StatusKind::Stale)
            }
            (None, None) => ("No data loaded".to_string(), StatusKind::Muted),
        }
    }
}

/// `STAMP` part of a `SITE/vol/STAMP` prefix (sortable: YYYYMMDD-HHMMSS).
fn stamp_of(prefix: &str) -> &str {
    prefix.rsplit('/').next().unwrap_or("")
}

/// "3:16 PM" in the user's local time for live data, or "20:16 UTC" when
/// the user prefers UTC; archive scans keep the explicit UTC date
/// (`archive_time`).
pub fn clock_time(ms: i64, utc: bool) -> String {
    use chrono::{Local, TimeZone, Utc};
    if utc {
        return Utc
            .timestamp_millis_opt(ms)
            .single()
            .map_or_else(String::new, |t| t.format("%H:%M UTC").to_string());
    }
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map_or_else(String::new, |t| t.format("%-I:%M %p").to_string())
}

/// Which loaded volume to draw from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    Current,
    Previous,
}

/// Cache of GPU uploads: returns the id for (volume, sweep, moment),
/// uploading on first use and evicting the oldest beyond `UPLOAD_CACHE`.
pub fn ensure_upload(
    model: &mut Model,
    renderer: &mut hookecho_render::Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    which: Which,
    sweep: usize,
    m: Moment,
) -> Option<hookecho_render::SweepId> {
    let l = match which {
        Which::Current => model.shown()?,
        Which::Previous => model.previous.as_ref()?,
    };
    let key = (l.generation, sweep, m);
    if let Some(i) = model.uploads.iter().position(|(k, _)| *k == key) {
        let e = model.uploads.remove(i);
        let id = e.1;
        model.uploads.push(e);
        return Some(id);
    }
    let s = &l.volume.sweeps[sweep];
    let id = if model.config.mask_dualpol && is_dualpol(m) {
        // Upload a masked copy of just this moment.
        let masked = Sweep {
            radials: s.radials.clone(),
            moments: vec![(m, s.masked_by_reflectivity(m, DUALPOL_MIN_DBZ)?)],
            ..*s
        };
        renderer.upload_sweep(device, queue, &masked, m)?
    } else {
        renderer.upload_sweep(device, queue, s, m)?
    };
    model.uploads.push((key, id));
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
        let all = site_list("", &[], None);
        assert!(all.len() >= 155);
        let ok = site_list("ok", &[], None);
        assert!(
            ok.iter().take(4).all(|(s, _)| s.state == "OK"),
            "state matches rank first"
        );
        assert!(ok.iter().any(|(s, _)| s.id == "KTLX"));
        let tul = site_list("tul", &[], None);
        assert_eq!(tul[0].0.id, "KINX", "name word prefix (Tulsa)");
        let t = site_list("ktlx", &[], None);
        assert_eq!(t[0].0.id, "KTLX", "exact ID first");
        assert!(
            site_list("tlx", &[], None)
                .iter()
                .any(|(s, _)| s.id == "KTLX"),
            "ID without the K"
        );
        let fav = site_list("", &["KFWS".into()], None);
        assert!(fav[0].1 && fav[0].0.id == "KFWS", "favorites first");
        assert!(
            site_list("koun", &[], None).is_empty(),
            "research radars hidden"
        );
    }

    /// North of Dallas: the picker's unfiltered list starts with the local
    /// radar, not with Alaska; a query still ranks by match.
    #[test]
    fn site_list_nearest_first() {
        let plano = Some((33.1, -96.75));
        let near = site_list("", &[], plano);
        assert_eq!(near[0].0.id, "KFWS");
        let fav = site_list("", &["KTLX".into()], plano);
        assert_eq!(
            (fav[0].0.id.as_str(), fav[1].0.id.as_str()),
            ("KTLX", "KFWS")
        );
        assert_eq!(site_list("ok", &[], plano)[0].0.state, "OK");
    }

    #[test]
    fn home_and_nearest() {
        use hookecho_core::config::Config;
        let first_run = Config::default();
        assert_eq!(initial_site(&first_run, "KTLX").id, "KTLX");
        let with_home = Config {
            home: Some((33.1, -96.75)),
            ..Default::default()
        };
        assert_eq!(initial_site(&with_home, "KTLX").id, "KFWS");
        let returning = Config {
            site: Some("KINX".into()),
            ..with_home.clone()
        };
        assert_eq!(
            initial_site(&returning, "KTLX").id,
            "KINX",
            "last site wins"
        );

        let size = [1280.0, 800.0];
        let mut m = Model::new(Config::default(), sites::by_id("KTLX").unwrap(), size);
        m.camera = hookecho_core::geo::Camera::looking_at(33.1, -96.75, 300.0, size[0]);
        m.go_to_nearest(size);
        assert_eq!(m.site.id, "KFWS", "no home: nearest to the map center");
        m.set_home_here();
        let (lat, lon) = m.config.home.unwrap();
        assert!(
            (lat - 32.57).abs() < 0.1 && (lon + 97.30).abs() < 0.1,
            "home at KFWS"
        );
        m.camera = hookecho_core::geo::Camera::looking_at(40.0, -100.0, 300.0, size[0]);
        m.go_to_nearest(size);
        assert_eq!(m.site.id, "KFWS", "home wins over the map center");
    }

    #[test]
    fn status_line_ages_and_states() {
        use hookecho_core::live::FeedState;
        let v = hookecho_core::level2::decode_volume(crate::assets::SAMPLE_VOLUME).unwrap();
        let site = sites::by_id("KTLX").unwrap();
        let mut m = Model::new(Config::default(), site, [1280.0, 800.0]);
        assert_eq!(m.status_line(0).1, StatusKind::Muted);
        m.feed = Some(FeedState::Connecting);
        assert_eq!(m.status_line(0).0, "Connecting…");
        let mut l = Loaded::new(site, v, false);
        l.live = Some(LiveInfo {
            prefix: "KTLX/1/x".into(),
            chunks: 55,
            complete: true,
        });
        let newest = l.newest_ms().unwrap();
        m.loaded = Some(l);
        m.feed = Some(FeedState::Live);
        let (text, kind) = m.status_line(newest + 30_000);
        assert_eq!(text, "Live · Precipitation mode");
        assert_eq!(kind, StatusKind::Fresh);
        assert_eq!(m.status_line(newest + 9 * 60_000).1, StatusKind::Aging);
        assert_eq!(m.status_line(newest + 30 * 60_000).1, StatusKind::Stale);
        assert!(
            m.status_line(newest + 30 * 60_000)
                .0
                .starts_with("30 min ago")
        );
        m.feed = Some(FeedState::Retrying {
            attempt: 2,
            wait_s: 8,
            error: "x".into(),
        });
        assert_eq!(
            m.status_line(newest + 5 * 60_000),
            ("Offline · data 5 min ago".to_string(), StatusKind::Stale)
        );
    }

    fn frame(
        req: &hookecho_core::frames::LoopRequest,
        stamp: &str,
        v: &hookecho_core::level2::Volume,
    ) -> hookecho_core::frames::LoopFrame {
        hookecho_core::frames::LoopFrame {
            request: req.clone(),
            prefix: format!("KTLX/1/{stamp}"),
            start: 0,
            volume: std::sync::Arc::new(v.clone()),
        }
    }

    #[test]
    fn loop_frames_order_dedupe_and_playback() {
        let v = hookecho_core::level2::decode_volume(crate::assets::SAMPLE_VOLUME).unwrap();
        let site = sites::by_id("KTLX").unwrap();
        let mut m = Model::new(Config::default(), site, [1280.0, 800.0]);
        let mut live = Loaded::new(site, v.clone(), false);
        live.live = Some(LiveInfo {
            prefix: "KTLX/9/20261010-220000".into(),
            chunks: 10,
            complete: false,
        });
        m.loaded = Some(live);
        let req = m.loop_request().unwrap();
        assert_eq!(req.frames, 9, "10-frame loop = 9 earlier volumes + live");
        m.set_loop_request(req.clone());
        // Frames arrive newest first; a duplicate and a stale request are ignored.
        for stamp in ["20261010-215300", "20261010-214600", "20261010-215300"] {
            m.accept_frame(frame(&req, stamp, &v));
        }
        let mut stale = req.clone();
        stale.tilt_deg = 0.9;
        m.accept_frame(frame(&stale, "20261010-213900", &v));
        assert_eq!(m.looping.frames.len(), 2);
        let order: Vec<_> = m
            .looping
            .frames
            .iter()
            .map(|f| f.live.as_ref().unwrap().prefix.clone())
            .collect();
        assert_eq!(
            order,
            vec!["KTLX/1/20261010-214600", "KTLX/1/20261010-215300"],
            "oldest first"
        );
        assert_eq!(m.timeline().len(), 3);
        assert!(m.timeline()[2].1, "live is last");
        // Stepping and playback.
        assert_eq!(m.timeline_pos(), 2);
        m.step_frame(-1);
        assert_eq!(m.looping.index, Some(1));
        m.toggle_play(0.0);
        assert!(
            m.looping.playing && m.looping.index == Some(1),
            "play resumes from where you are"
        );
        m.toggle_play(0.0);
        assert!(
            !m.looping.playing && m.looping.index.is_none(),
            "pausing shows the latest scan"
        );
        m.toggle_play(10.0);
        let next = m.advance(10.0).unwrap();
        assert_eq!(
            m.timeline_pos(),
            0,
            "playback wraps from live to the oldest frame"
        );
        assert!((next - 10.0 - FRAME_DWELL_S).abs() < 1e-9);
        m.advance(next);
        m.advance(next + FRAME_DWELL_S);
        assert_eq!(m.timeline_pos(), 2, "then live");
        let after_live = m.looping.next_at;
        assert!(
            (after_live - (next + FRAME_DWELL_S + FRAME_DWELL_S * LIVE_DWELL_FACTOR)).abs() < 1e-6,
            "live holds longer"
        );
    }

    #[test]
    fn archive_time_format() {
        assert_eq!(archive_time(1_369_080_003_000), "May 20, 2013 · 20:00 UTC");
        assert_eq!(clock_time(1_369_080_003_000, true), "20:00 UTC");
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
