// Hook Echo scene shaders: basemap fills/lines, radar polar->screen, blur.

const PI: f32 = 3.14159265358979;
const EARTH_KM: f32 = 6371.0;
// Effective earth radius for standard refraction, km (4/3 · 6371).
const RE_KM: f32 = 8494.6667;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

// ------------------------------------------------------------- map geometry
// Positions are stored as (mercator - origin) * 1e4 to keep f32 precision.
struct MapU {
    viewport: vec2<f32>,      // physical px
    offset_px: vec2<f32>,     // (origin - camera center) * px_per_merc + viewport / 2
    px_per_unit: f32,         // physical px per stored unit (= px_per_merc / 1e4)
    width_px: f32,            // lines only
    _pad: vec2<f32>,
    color: vec4<f32>,         // linear, unpremultiplied
};

@group(0) @binding(0) var<uniform> mu: MapU;

fn to_ndc(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / mu.viewport.x * 2.0 - 1.0, 1.0 - p.y / mu.viewport.y * 2.0, 0.0, 1.0);
}

@vertex
fn vs_fill(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return to_ndc(pos * mu.px_per_unit + mu.offset_px);
}

@fragment
fn fs_fill() -> @location(0) vec4<f32> {
    return mu.color;
}

struct LineOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) across: f32,   // -1..1 across the quad
};

// One instance per segment; 6 vertices make a quad with butt ends. At the
// line widths we use (≤ 2 px) the missing join wedges are invisible, and
// butt ends avoid double-blending translucent lines at every vertex.
@vertex
fn vs_line(@builtin(vertex_index) vi: u32, @location(0) a: vec2<f32>, @location(1) b: vec2<f32>) -> LineOut {
    let pa = a * mu.px_per_unit + mu.offset_px;
    let pb = b * mu.px_per_unit + mu.offset_px;
    var dir = pb - pa;
    let len = length(dir);
    if (len < 1e-4) { dir = vec2<f32>(1.0, 0.0); } else { dir = dir / len; }
    let n = vec2<f32>(-dir.y, dir.x);
    let half = mu.width_px * 0.5 + 1.0;
    // Corners: (end, side) for the two triangles.
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    let p = mix(pa, pb, c.x) + n * half * c.y;
    var o: LineOut;
    o.pos = to_ndc(p);
    o.across = c.y;
    return o;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    let half = mu.width_px * 0.5 + 1.0;
    let dist = abs(in.across) * half;
    // Full coverage inside width/2, a 1 px feather outside; sub-pixel widths
    // fade instead of shimmering.
    let cov = clamp(mu.width_px * 0.5 + 0.5 - dist, 0.0, 1.0) * min(mu.width_px, 1.0);
    return vec4<f32>(mu.color.rgb, mu.color.a * cov);
}

// -------------------------------------------------------------------- radar
struct RadarU {
    viewport: vec2<f32>,
    radar_px: vec2<f32>,      // radar position on screen, physical px
    radar_merc: vec2<f32>,    // radar position, normalized mercator
    px_per_merc: f32,
    radar_lat: f32,           // radians
    radar_lon: f32,           // radians
    elev: f32,                // radians
    first_gate_km: f32,
    gate_km: f32,
    gates: f32,
    scale: f32,               // value = (raw - offset) / scale
    offset: f32,
    lut_min: f32,             // palette covers [lut_min, lut_max]
    lut_max: f32,
    alpha: f32,
    min_value: f32,           // values below are not drawn (low-dBZ filter)
    _pad: f32,
    rf_color: vec4<f32>,      // range-folded color (linear, alpha 0 = hidden)
};

@group(0) @binding(0) var<uniform> ru: RadarU;
@group(0) @binding(1) var gates_tex: texture_2d<u32>;   // R16Uint, gates × radials
@group(0) @binding(2) var az_tex: texture_2d<u32>;      // R16Uint, 3600 × 1
@group(0) @binding(3) var lut_tex: texture_2d<f32>;     // Rgba8UnormSrgb, 1024 × 1

@fragment
fn fs_radar(in: VsOut) -> @location(0) vec4<f32> {
    // Pixel -> mercator (as an offset from the radar, for f32 precision) -> lat/lon.
    let dm = (in.pos.xy - ru.radar_px) / ru.px_per_merc;
    let lat = atan(sinh(PI * (1.0 - 2.0 * (ru.radar_merc.y + dm.y))));
    let dlon = dm.x * 2.0 * PI;
    // Local equirectangular about the midpoint latitude (< 0.1% error at 460 km).
    let d = vec2<f32>(dlon * cos(0.5 * (lat + ru.radar_lat)), lat - ru.radar_lat) * EARTH_KM;
    let ground = length(d);
    var az = degrees(atan2(d.x, d.y));
    if (az < 0.0) { az += 360.0; }
    // Ground distance -> slant range (4/3 earth, closed form; see
    // hookecho_core::geo::slant_range_km).
    let phi = ground / RE_KM;
    let slant = RE_KM * sin(phi) / cos(ru.elev + phi);
    let gate = floor((slant - ru.first_gate_km) / ru.gate_km + 0.5);
    if (gate < 0.0 || gate >= ru.gates) { discard; }
    let row = textureLoad(az_tex, vec2<i32>(i32(min(az * 10.0, 3599.0)), 0), 0).r;
    if (row == 65535u) { discard; }
    let raw = textureLoad(gates_tex, vec2<i32>(i32(gate), i32(row)), 0).r;
    if (raw == 0u) { discard; }
    if (raw == 1u) {
        if (ru.rf_color.a <= 0.0) { discard; }
        return vec4<f32>(ru.rf_color.rgb, ru.rf_color.a * ru.alpha);
    }
    let v = (f32(raw) - ru.offset) / ru.scale;
    if (v < ru.min_value) { discard; }
    let t = clamp((v - ru.lut_min) / (ru.lut_max - ru.lut_min), 0.0, 1.0);
    let c = textureLoad(lut_tex, vec2<i32>(i32(t * 1023.0 + 0.5), 0), 0);
    if (c.a <= 0.0) { discard; }
    return vec4<f32>(c.rgb, c.a * ru.alpha);
}

// --------------------------------------------------------------------- blur
struct BlurU {
    texel: vec2<f32>,
    dir: vec2<f32>,           // (1,0)/(0,1); (0,0) = downsample
    sigma: f32,
    saturation: f32,
    tone: f32,                // 0 = no compression
    tone_light: f32,          // 1 = compress toward white
};

@group(0) @binding(0) var<uniform> bu: BlurU;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    // Four bilinear taps cover the 4×4 source block under each target pixel.
    let o = bu.texel;
    var c = textureSample(src, samp, in.uv + vec2<f32>(-o.x, -o.y));
    c += textureSample(src, samp, in.uv + vec2<f32>(o.x, -o.y));
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x, o.y));
    c += textureSample(src, samp, in.uv + vec2<f32>(o.x, o.y));
    return c * 0.25;
}

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let radius = i32(ceil(bu.sigma * 3.0));
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -radius; i <= radius; i++) {
        let w = exp(-f32(i * i) / (2.0 * bu.sigma * bu.sigma));
        acc += textureSampleLevel(src, samp, in.uv + bu.dir * bu.texel * f32(i), 0.0) * w;
        wsum += w;
    }
    let c = acc / wsum;
    let l = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    var rgb = clamp(mix(vec3<f32>(l), c.rgb, bu.saturation), vec3<f32>(0.0), vec3<f32>(1.0));
    // Tone compression (docs/DESIGN.md §2.6) guarantees glass text contrast.
    if (bu.tone > 0.0) {
        if (bu.tone_light > 0.5) {
            rgb = vec3<f32>(1.0) - (vec3<f32>(1.0) - rgb) * bu.tone;
        } else {
            rgb = rgb * bu.tone;
        }
    }
    return vec4<f32>(rgb, 1.0);
}
