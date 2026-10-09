// Hook Echo prototype scene shaders: radar polar→screen, basemap lines, blur.

// ---------------------------------------------------------------- shared
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

// ---------------------------------------------------------------- radar
struct RadarU {
    viewport: vec2<f32>,      // physical px
    radar_px: vec2<f32>,      // radar position on screen, physical px
    radar_merc: vec2<f32>,    // radar position, normalized mercator
    px_per_merc: f32,         // physical px per mercator unit
    radar_lat: f32,           // radians
    radar_lon: f32,           // radians
    first_gate_km: f32,
    gate_km: f32,
    gates: f32,
    inv_cos_elev: f32,
    alpha: f32,
    _pad: vec2<f32>,
};

const PI: f32 = 3.14159265358979;
const EARTH_KM: f32 = 6371.0;

@group(0) @binding(0) var<uniform> ru: RadarU;
@group(0) @binding(1) var gates_tex: texture_2d<u32>;   // R8Uint, gates × radials
@group(0) @binding(2) var az_tex: texture_2d<u32>;      // R16Uint, 3600 × 1
@group(0) @binding(3) var lut_tex: texture_2d<f32>;     // Rgba8UnormSrgb, 256 × 1

@fragment
fn fs_radar(in: VsOut) -> @location(0) vec4<f32> {
    // Pixel -> mercator (as an offset from the radar, for f32 precision) -> lat/lon.
    let dm = (in.pos.xy - ru.radar_px) / ru.px_per_merc;
    let my = ru.radar_merc.y + dm.y;
    let lat = atan(sinh(PI * (1.0 - 2.0 * my)));
    let lon = ru.radar_lon + dm.x * 2.0 * PI;
    // Local equirectangular about the midpoint latitude: < 0.1% error at 460 km.
    let d = vec2<f32>((lon - ru.radar_lon) * cos(0.5 * (lat + ru.radar_lat)), -(lat - ru.radar_lat)) * EARTH_KM;
    let ground_km = length(d);
    // Screen y grows south; azimuth is clockwise from north.
    var az = degrees(atan2(d.x, -d.y));
    if (az < 0.0) { az = az + 360.0; }
    let slant = ground_km * ru.inv_cos_elev;
    let gate = floor((slant - ru.first_gate_km) / ru.gate_km + 0.5);
    if (gate < 0.0 || gate >= ru.gates) { discard; }
    let row = textureLoad(az_tex, vec2<i32>(i32(min(az * 10.0, 3599.0)), 0), 0).r;
    if (row == 65535u) { discard; }
    let raw = textureLoad(gates_tex, vec2<i32>(i32(gate), i32(row)), 0).r;
    let c = textureLoad(lut_tex, vec2<i32>(i32(raw), 0), 0);
    if (c.a <= 0.0) { discard; }
    return vec4<f32>(c.rgb, c.a * ru.alpha);
}

// ---------------------------------------------------------------- lines
struct LineU {
    viewport: vec2<f32>,
    offset_px: vec2<f32>,     // (layer origin - camera center) * scale + viewport/2
    px_per_unit: f32,         // physical px per stored unit
    width_px: f32,
    _pad: vec2<f32>,
    color: vec4<f32>,         // linear, unpremultiplied
};

@group(0) @binding(0) var<uniform> lu: LineU;

struct LineIn {
    @location(0) pos: vec2<f32>,
    @location(1) normal: vec2<f32>,
    @location(2) side: f32,
};

struct LineOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) side: f32,
};

@vertex
fn vs_line(v: LineIn) -> LineOut {
    // Extrude half the width plus 1 px of feather on each side.
    let half = lu.width_px * 0.5 + 1.0;
    let p = v.pos * lu.px_per_unit + lu.offset_px + v.normal * half;
    var o: LineOut;
    o.pos = vec4<f32>(p.x / lu.viewport.x * 2.0 - 1.0, 1.0 - p.y / lu.viewport.y * 2.0, 0.0, 1.0);
    o.side = v.side;
    return o;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    let half = lu.width_px * 0.5 + 1.0;
    let dist = abs(in.side) * half;
    // Coverage: full inside width/2, linear feather over the last pixel. Thin
    // (sub-pixel) lines fade instead of shimmering.
    let cov = clamp(lu.width_px * 0.5 + 0.5 - dist, 0.0, 1.0) * min(lu.width_px, 1.0);
    return vec4<f32>(lu.color.rgb, lu.color.a * cov);
}

// ---------------------------------------------------------------- blur
struct BlurU {
    texel: vec2<f32>,         // 1 / source size
    dir: vec2<f32>,           // (1,0) or (0,1); (0,0) = downsample
    sigma: f32,
    saturation: f32,
    tone: f32,                // 0 = no compression (intermediate passes)
    tone_light: f32,          // 1 = compress toward white, 0 = toward black
};

@group(0) @binding(0) var<uniform> bu: BlurU;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    // Four bilinear taps cover the 4×4 source block under each target pixel.
    let o = bu.texel;
    var c = textureSample(src, samp, in.uv + vec2<f32>(-o.x, -o.y));
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x, -o.y));
    c += textureSample(src, samp, in.uv + vec2<f32>(-o.x,  o.y));
    c += textureSample(src, samp, in.uv + vec2<f32>( o.x,  o.y));
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
    var c = acc / wsum;
    // Saturation boost so radar color still glows through the glass.
    let l = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    var rgb = clamp(mix(vec3<f32>(l), c.rgb, bu.saturation), vec3<f32>(0.0), vec3<f32>(1.0));
    // Tone compression (DESIGN.md §2.6) guarantees text contrast on glass.
    if (bu.tone > 0.0) {
        if (bu.tone_light > 0.5) {
            rgb = vec3<f32>(1.0) - (vec3<f32>(1.0) - rgb) * bu.tone;
        } else {
            rgb = rgb * bu.tone;
        }
    }
    return vec4<f32>(rgb, c.a);
}
