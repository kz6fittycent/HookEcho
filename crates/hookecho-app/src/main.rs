//! Hook Echo: a fast, clean weather radar viewer.
//!
//!   hookecho                      open the app
//!   hookecho --smoke-test         load the bundled sample, render one frame offscreen, exit 0
//!   hookecho --screenshots DIR    numbered PNGs from bundled data, both themes
//!   hookecho --bench [--bench-out FILE] [--bench-baseline FILE]

mod app;
mod assets;
mod bench;
mod commands;
// The design system defines the full DESIGN.md token and component set;
// some pieces are first used by Phase 2 screens (side sheet, palette, timeline).
#[allow(dead_code)]
mod design;
mod feeds;
mod headless;
mod model;
mod overlays;
mod palettes;
mod pickers;
mod screen;
mod sheet;
mod shots;
mod smoke;
mod space;
mod spc;
mod stations;
mod warnings;

use eframe::egui;
use std::path::PathBuf;
use std::process::ExitCode;

pub const APP_ID: &str = "io.github.kz6fittycent.HookEcho";

fn arg_value(args: &[String], name: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

fn main() -> ExitCode {
    let start = std::time::Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let result = if has("--help") || has("-h") {
        println!(
            "{}",
            include_str!("main.rs")
                .lines()
                .skip(2)
                .take(4)
                .map(|l| l.trim_start_matches("//!"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        Ok(())
    } else if has("--version") {
        println!("hookecho {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    } else if has("--smoke-test") {
        smoke::run()
    } else if has("--live-screenshot") {
        let i = args.iter().position(|a| a == "--live-screenshot").unwrap();
        let file = args
            .get(i + 1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("live.png"));
        shots::live(&file, args.get(i + 2).map(String::as_str))
    } else if has("--screenshots") {
        shots::run(
            &arg_value(&args, "--screenshots").unwrap_or_else(|| PathBuf::from("screenshots")),
        )
    } else if has("--bench") {
        bench::run(
            start,
            arg_value(&args, "--bench-out").as_deref(),
            arg_value(&args, "--bench-baseline").as_deref(),
        )
    } else {
        windowed()
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hookecho: {e}");
            ExitCode::FAILURE
        }
    }
}

fn windowed() -> Result<(), String> {
    let icon = image::load_from_memory(assets::ICON_PNG)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (w, h) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hook Echo")
            .with_app_id(APP_ID)
            .with_icon(egui::IconData {
                rgba: icon.into_raw(),
                width: w,
                height: h,
            })
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "hookecho",
        options,
        Box::new(|cc| {
            design::install_fonts(&cc.egui_ctx);
            Ok(Box::new(app::Windowed { app: None }))
        }),
    )
    .map_err(|e| e.to_string())
}
