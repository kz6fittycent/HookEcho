//! Renders assets/icon/hookecho.svg to PNGs (dev tool):
//!   cargo run -p hookecho-app --example render_icon
fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icon");
    let svg = std::fs::read_to_string(format!("{root}/hookecho.svg")).expect("svg");
    let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).expect("parse svg");
    for px in [256u32, 512] {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px).unwrap();
        let s = px as f32 / 512.0;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(s, s),
            &mut pixmap.as_mut(),
        );
        let out = format!("{root}/hookecho-{px}.png");
        pixmap.save_png(&out).expect("write png");
        println!("wrote {out}");
    }
}
