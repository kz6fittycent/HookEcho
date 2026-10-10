//! Place labels via glyphon (cosmic-text shaping), decluttered by rank with a
//! greedy collision pass. Shaped buffers are cached per place, so panning and
//! zooming only move text; they never reshape it.

use glyphon::{
    Attrs, Buffer, Cache, Color, ColorMode, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use hookecho_core::basemap::Place;
use hookecho_core::geo::{Camera, to_mercator};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct LabelStyle {
    /// sRGB colors.
    pub color: [u8; 4],
    pub halo: [u8; 4],
    /// Logical px; major places get `size + 1`.
    pub size: f32,
    pub family: &'static str,
}

struct Entry {
    buffer: Buffer,
    /// Logical width/height.
    size: [f32; 2],
}

pub struct Labels {
    fonts: FontSystem,
    swash: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderer: TextRenderer,
    places: Vec<(Place, [f64; 2])>,
    cache: HashMap<usize, Entry>,
    cache_ppp: f32,
}

/// The most labels drawn at once; more than this reads as clutter.
const MAX_LABELS: usize = 160;
/// Visible candidates examined per frame (places are sorted by rank, best first).
const MAX_CANDIDATES: usize = 4000;

impl Labels {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        fonts: Vec<Vec<u8>>,
        places: &[Place],
    ) -> Labels {
        // Only our bundled fonts: never scan the system font database (slow,
        // and different on every machine).
        let mut db = glyphon::fontdb::Database::new();
        for f in fonts {
            db.load_font_data(f);
        }
        let fonts = FontSystem::new_with_locale_and_db("en-US".into(), db);
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas =
            TextAtlas::with_color_mode(device, queue, &cache, format, ColorMode::Accurate);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        let mut places: Vec<(Place, [f64; 2])> = places
            .iter()
            .map(|p| (p.clone(), to_mercator(p.lat, p.lon)))
            .collect();
        places.sort_by_key(|p| std::cmp::Reverse(p.0.rank));
        Labels {
            fonts,
            swash: SwashCache::new(),
            atlas,
            viewport,
            renderer,
            places,
            cache: HashMap::new(),
            cache_ppp: 0.0,
        }
    }

    /// Chooses and lays out the visible labels for this frame.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size_px: [u32; 2],
        ppp: f32,
        camera: &Camera,
        style: &LabelStyle,
        exclude: &[[f32; 4]],
    ) {
        if (ppp - self.cache_ppp).abs() > f32::EPSILON {
            self.cache.clear();
            self.cache_ppp = ppp;
        }
        let logical = [
            size_px[0] as f64 / ppp as f64,
            size_px[1] as f64 / ppp as f64,
        ];
        // Hide small towns as you zoom out (land area as a size proxy).
        let km = camera.km_per_px();
        let min_rank = (km * km * 20.0).max(400.0) as u64;
        let mut taken: Vec<[f32; 4]> = exclude.to_vec();
        let mut chosen: Vec<(usize, [f32; 2])> = Vec::new();
        let mut examined = 0;
        for (i, (p, m)) in self.places.iter().enumerate() {
            if p.rank < min_rank || chosen.len() >= MAX_LABELS || examined >= MAX_CANDIDATES {
                break;
            }
            // Visibility first (cheap), so zoomed-in views still reach the
            // small towns far down the global ranking.
            let s = camera.to_screen(*m, logical);
            if s[0] < 0.0 || s[1] < 0.0 || s[0] > logical[0] || s[1] > logical[1] {
                continue;
            }
            examined += 1;
            let major = p.rank > 30_000;
            let entry = self.cache.entry(i).or_insert_with(|| {
                let font_px = (style.size + if major { 1.0 } else { 0.0 }) * ppp;
                let mut buffer =
                    Buffer::new(&mut self.fonts, Metrics::new(font_px, font_px * 1.25));
                let weight = if major {
                    Weight::SEMIBOLD
                } else {
                    Weight::MEDIUM
                };
                buffer.set_text(
                    &p.name,
                    &Attrs::new()
                        .family(Family::Name(style.family))
                        .weight(weight),
                    Shaping::Advanced,
                    None,
                );
                buffer.set_size(None, None);
                buffer.shape_until_scroll(&mut self.fonts, false);
                let w = buffer.layout_runs().map(|r| r.line_w).fold(0.0, f32::max) / ppp;
                Entry {
                    buffer,
                    size: [w, font_px * 1.25 / ppp],
                }
            });
            let [w, h] = entry.size;
            let r = [
                s[0] as f32 - w / 2.0 - 6.0,
                s[1] as f32 - h / 2.0 - 3.0,
                s[0] as f32 + w / 2.0 + 6.0,
                s[1] as f32 + h / 2.0 + 3.0,
            ];
            // Whole label on screen, or not at all.
            if r[0] < 0.0 || r[1] < 0.0 || r[2] > logical[0] as f32 || r[3] > logical[1] as f32 {
                continue;
            }
            if taken
                .iter()
                .any(|t| r[0] < t[2] && r[2] > t[0] && r[1] < t[3] && r[3] > t[1])
            {
                continue;
            }
            taken.push(r);
            chosen.push((i, [s[0] as f32 - w / 2.0, s[1] as f32 - h / 2.0]));
        }

        let color = |c: [u8; 4]| Color::rgba(c[0], c[1], c[2], c[3]);
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: size_px[0] as i32,
            bottom: size_px[1] as i32,
        };
        let halo = 1.2 * ppp.min(2.0);
        let mut areas = Vec::with_capacity(chosen.len() * 9);
        // Halo first (8 offsets), then the label itself on top.
        for &(i, at) in &chosen {
            let buffer = &self.cache[&i].buffer;
            for (dx, dy) in [
                (-1.0, 0.0),
                (1.0, 0.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (-0.7, -0.7),
                (0.7, 0.7),
                (-0.7, 0.7),
                (0.7, -0.7),
            ] {
                areas.push(TextArea {
                    buffer,
                    left: at[0] * ppp + dx * halo,
                    top: at[1] * ppp + dy * halo,
                    scale: 1.0,
                    bounds,
                    default_color: color(style.halo),
                    custom_glyphs: &[],
                });
            }
        }
        for &(i, at) in &chosen {
            areas.push(TextArea {
                buffer: &self.cache[&i].buffer,
                left: at[0] * ppp,
                top: at[1] * ppp,
                scale: 1.0,
                bounds,
                default_color: color(style.color),
                custom_glyphs: &[],
            });
        }
        self.viewport.update(
            queue,
            Resolution {
                width: size_px[0],
                height: size_px[1],
            },
        );
        if let Err(e) = self.renderer.prepare(
            device,
            queue,
            &mut self.fonts,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash,
        ) {
            eprintln!("labels: {e}");
        }
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Err(e) = self.renderer.render(&self.atlas, &self.viewport, pass) {
            eprintln!("labels: {e}");
        }
    }

    /// Frees atlas space for glyphs not used this frame.
    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}
