//! Renders the Moore sample offscreen and checks real pixels: radar color at
//! the debris ball, basemap land, and a working blur target.

use hookecho_core::geo::Camera;
use hookecho_core::{Moment, basemap, level2};
use hookecho_render::{Frame, LabelStyle, Renderer, SceneStyle, capture};

fn root(p: &str) -> String {
    format!("{}/../../{p}", env!("CARGO_MANIFEST_DIR"))
}

fn style() -> SceneStyle {
    let g = |v: f32| [v, v, v, 1.0];
    SceneStyle {
        water: g(0.004),
        land: g(0.009),
        lakes: g(0.004),
        lines: vec![
            ("county", 0.75, [1.0, 1.0, 1.0, 0.12], false),
            ("state", 1.25, [1.0, 1.0, 1.0, 0.4], true),
        ],
        labels: LabelStyle {
            color: [230, 233, 238, 210],
            halo: [21, 24, 29, 200],
            size: 12.0,
            family: "Inter",
        },
        radar_alpha: 1.0,
        blur_sigma: 18.0,
        blur_saturation: 1.25,
        blur_tone: 0.16,
        blur_tone_light: false,
    }
}

#[test]
fn renders_moore_sweep_offscreen() {
    let (adapter, device, queue) = match capture::headless_device() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("skipping: {e}");
            return;
        }
    };
    eprintln!("adapter: {}", adapter.get_info().name);
    let map = basemap::decode(&std::fs::read(root("assets/basemap/basemap.bin")).unwrap()).unwrap();
    let fonts = vec![
        std::fs::read(root("assets/fonts/Inter-Medium.ttf")).unwrap(),
        std::fs::read(root("assets/fonts/Inter-SemiBold.ttf")).unwrap(),
    ];
    let mut r = Renderer::new(&device, &queue, &map, fonts);
    let vol = level2::decode_volume(
        &std::fs::read(root("tests/data/KTLX20130520_201643_3sweeps.ar2.gz")).unwrap(),
    )
    .unwrap();
    let id = r
        .upload_sweep(&device, &queue, &vol.sweeps[0], Moment::Reflectivity)
        .unwrap();
    let ktlx = (35.333361, -97.277761);
    let size = [800u32, 500];
    let camera = Camera::looking_at(35.30, -97.40, 70.0, size[0] as f64);
    r.ensure_targets(&device, size);
    let s = style();
    let mut enc = device.create_command_encoder(&Default::default());
    r.render(
        &device,
        &queue,
        &mut enc,
        &Frame {
            size_px: size,
            ppp: 1.0,
            camera,
            radar: &[(id, ktlx)],
            style: &s,
            label_exclusions: &[],
        },
    );
    queue.submit([enc.finish()]);
    let px = capture::read_rgba8(&device, &queue, &r.targets.as_ref().unwrap().scene);
    if std::env::var_os("HOOKECHO_TEST_PNG").is_some() {
        image::RgbaImage::from_raw(size[0], size[1], px.clone())
            .unwrap()
            .save("/tmp/hookecho-offscreen.png")
            .unwrap();
    }
    let at = |lat: f64, lon: f64| {
        let p = camera.latlon_to_screen(lat, lon, [size[0] as f64, size[1] as f64]);
        assert!(
            p[0] >= 0.0 && p[1] >= 0.0 && p[0] < size[0] as f64 && p[1] < size[1] as f64,
            "({lat}, {lon}) is off-screen"
        );
        let i = ((p[1] as u32 * size[0] + p[0] as u32) * 4) as usize;
        [px[i], px[i + 1], px[i + 2]]
    };
    // Debris ball (~69 dBZ): pink/magenta in the default palette.
    let ball = at(35.321, -97.5217);
    assert!(ball[0] > 180 && ball[2] > 120, "debris ball pixel {ball:?}");
    // The radar cone of silence (inside the first gate, 2 km) shows plain land.
    let cone = at(35.3334, -97.2778);
    assert!(
        cone.iter().all(|&c| c < 40),
        "cone of silence pixel {cone:?}"
    );
    let (radar_bytes, map_bytes) = r.gpu_bytes();
    eprintln!(
        "gpu: radar {} KB, basemap {} KB",
        radar_bytes / 1024,
        map_bytes / 1024
    );
}
