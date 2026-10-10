//! `--bench` (KICKOFF §4): measures the performance budgets on bundled data
//! and compares against a committed baseline. A metric that regresses more
//! than 10% against the baseline entry for the same GPU adapter fails the
//! run. Budgets are reported; they're judged on target hardware (an
//! integrated GPU), not on CI's software renderer.

use crate::headless::Offscreen;
use crate::model;
use hookecho_core::Moment;
use hookecho_core::geo::Camera;
use hookecho_render::capture;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::time::Instant;

struct Metric {
    key: &'static str,
    label: &'static str,
    value: f64,
    unit: &'static str,
    budget: Option<f64>,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

/// `HOOKECHO_MEM_TRACE=1` prints RSS at startup stages (memory budget work).
pub fn trace(stage: &str) {
    if std::env::var_os("HOOKECHO_MEM_TRACE").is_some() {
        eprintln!("rss {:>7.1} MB  {stage}", rss_mb().unwrap_or(0.0));
    }
}

/// Resident set size in MB (Linux /proc; elsewhere unavailable).
fn rss_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: f64 = s
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb / 1024.0)
}

pub fn run(
    process_start: Instant,
    out: Option<&Path>,
    baseline: Option<&Path>,
) -> Result<(), String> {
    let mut metrics = Vec::new();
    let size = [1280.0f32, 800.0];

    // Cold start: process start → first radar frame on screen (offscreen),
    // with the bundled (cached) sample.
    let mut o = Offscreen::new(None, size)?;
    o.app.wait_for_data();
    o.app.model.reduced_motion = true;
    let _ = o.capture(size, 1.0);
    metrics.push(Metric {
        key: "cold_start_ms",
        label: "Cold start to radar on screen",
        value: ms(process_start),
        unit: "ms",
        budget: Some(1000.0),
    });
    let adapter = o.adapter_name();
    let rss_start = rss_mb();

    // Decode of the full bundled volume, and the per-sweep share of it.
    let t = Instant::now();
    let vol = hookecho_core::level2::decode_volume(crate::assets::SAMPLE_VOLUME)
        .map_err(|e| e.to_string())?;
    let decode = ms(t);
    let per_sweep = decode / vol.sweeps.len().max(1) as f64;
    metrics.push(Metric {
        key: "decode_volume_ms",
        label: "Decode full Level II volume",
        value: decode,
        unit: "ms",
        budget: None,
    });

    // Tilt/product switch to a sweep that isn't uploaded yet: upload + draw.
    let tilts: Vec<f32> = o
        .app
        .model
        .loaded_here()
        .map(|l| l.tilts().iter().map(|t| t.deg).collect())
        .unwrap_or_default();
    let mut switches = Vec::new();
    for (k, &deg) in tilts.iter().enumerate().take(8) {
        let m = if k % 2 == 0 {
            Moment::Reflectivity
        } else {
            Moment::Velocity
        };
        o.app.model.moment = m;
        o.app.model.tilt_deg = deg;
        let t = Instant::now();
        o.frame(size, 1.0, Vec::new());
        capture::wait_idle(&o.device);
        switches.push(ms(t));
    }
    let switch_p50 = percentile(&mut switches.clone(), 0.5);
    metrics.push(Metric {
        key: "switch_ms",
        label: "Tilt or product switch (upload + draw), p50",
        value: switch_p50,
        unit: "ms",
        budget: Some(50.0),
    });
    metrics.push(Metric {
        key: "new_sweep_ms",
        label: "New sweep → drawn (proxy: decode share + upload + draw)",
        value: per_sweep + switch_p50,
        unit: "ms",
        budget: Some(1000.0),
    });

    // Pan/zoom frame time at 4K (3840×2160 px).
    let k4 = [2560.0f32, 1440.0];
    let ppp = 1.5;
    o.app.model.moment = Moment::Reflectivity;
    o.app.model.tilt_deg = 0.5;
    o.app.model.camera = Camera::looking_at(35.33, -97.4, 120.0, k4[0] as f64);
    let _ = o.capture(k4, ppp);
    let mut frames = Vec::new();
    for i in 0..120 {
        let a = i as f64 / 120.0 * std::f64::consts::TAU;
        o.app.model.camera.pan(a.cos() * 6.0, a.sin() * 4.0);
        let c = o.app.model.camera;
        o.app.model.camera.zoom_at(
            1.0 + 0.004 * a.sin(),
            [k4[0] as f64 / 2.0, k4[1] as f64 / 2.0],
            [k4[0] as f64, k4[1] as f64],
        );
        o.app.model.motion.zoom_target = o.app.model.camera.scale;
        let _ = c;
        let t = Instant::now();
        o.frame(k4, ppp, Vec::new());
        capture::wait_idle(&o.device);
        frames.push(ms(t));
    }
    metrics.push(Metric {
        key: "frame_p50_ms",
        label: "Pan/zoom frame at 4K, p50",
        value: percentile(&mut frames.clone(), 0.5),
        unit: "ms",
        budget: None,
    });
    metrics.push(Metric {
        key: "frame_p99_ms",
        label: "Pan/zoom frame at 4K, p99",
        value: percentile(&mut frames, 0.99),
        unit: "ms",
        budget: Some(16.0),
    });

    // Memory with a 20-frame loop loaded. A loop keeps, per frame, only the
    // sweep of the product being shown (for drawing and the inspector); the
    // rest of each volume is dropped after decoding. Here: 20 frames of 0.5°
    // reflectivity and velocity.
    let mut held: Vec<hookecho_core::level2::Sweep> = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..20 {
        let v = hookecho_core::level2::decode_volume(crate::assets::SAMPLE_VOLUME)
            .map_err(|e| e.to_string())?;
        let l = model::Loaded::new(o.app.model.site, v, true);
        for m in [Moment::Reflectivity, Moment::Velocity] {
            if let Some(i) = l.sweep_for(m, 0.5) {
                let mut s = l.volume.sweeps[i].clone();
                s.moments.retain(|(k, _)| *k == m);
                if let Some(id) = o.app.renderer.upload_sweep(&o.device, &o.queue, &s, m) {
                    ids.push(id);
                }
                held.push(s);
            }
        }
        // `l` (the full volume) drops here.
    }
    capture::wait_idle(&o.device);
    if std::env::var_os("HOOKECHO_BENCH_DEBUG").is_some() {
        eprintln!(
            "rss: after startup {rss_start:?} MB, after loop {:?} MB, held sweeps {}",
            rss_mb(),
            held.len()
        );
    }
    let (gpu_radar, gpu_map) = o.app.renderer.gpu_bytes();
    let gpu_mb = (gpu_radar + gpu_map) as f64 / 1_048_576.0;
    if let Some(rss) = rss_mb() {
        metrics.push(Metric {
            key: "memory_20_frames_mb",
            label: "Memory with a 20-frame loop (RSS)",
            value: rss,
            unit: "MB",
            budget: Some(300.0),
        });
    }
    metrics.push(Metric {
        key: "gpu_20_frames_mb",
        label: "GPU buffers with a 20-frame loop",
        value: gpu_mb,
        unit: "MB",
        budget: None,
    });
    for id in ids {
        o.app.renderer.remove_sweep(id);
    }
    drop(held);

    // Report.
    println!("Hook Echo bench on {adapter}");
    println!("{:<58} {:>10}  {:>8}  ", "metric", "value", "budget");
    for m in &metrics {
        let (b, ok) = match m.budget {
            Some(b) => (
                format!("{b:.0} {}", m.unit),
                if m.value <= b {
                    "within budget"
                } else {
                    "OVER BUDGET"
                },
            ),
            None => ("—".into(), ""),
        };
        println!(
            "{:<58} {:>7.1} {:<2}  {:>8}  {ok}",
            m.label, m.value, m.unit, b
        );
    }
    println!(
        "idle CPU: not measurable offscreen; the app redraws only on input, animation or new data."
    );
    let results: Map<String, Value> = metrics
        .iter()
        .map(|m| (m.key.to_string(), json!((m.value * 10.0).round() / 10.0)))
        .collect();
    if let Some(p) = out {
        let doc =
            json!({ "adapter": adapter, "version": env!("CARGO_PKG_VERSION"), "metrics": results });
        std::fs::write(p, serde_json::to_string_pretty(&doc).unwrap())
            .map_err(|e| e.to_string())?;
        println!("wrote {}", p.display());
    }
    if let Some(b) = baseline {
        compare(&adapter, &results, b)?;
    }
    Ok(())
}

/// Fails if any metric is more than 10% worse than the baseline for this
/// adapter. A missing adapter entry is a warning, not a failure, so a new CI
/// image can record its first baseline.
fn compare(adapter: &str, results: &Map<String, Value>, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(base) = doc["adapters"].get(adapter).and_then(Value::as_object) else {
        println!(
            "::warning::no bench baseline for adapter \"{adapter}\" in {}; record one from this run's --bench-out",
            path.display()
        );
        return Ok(());
    };
    let mut failures = Vec::new();
    for (k, b) in base {
        let (Some(b), Some(v)) = (b.as_f64(), results.get(k).and_then(Value::as_f64)) else {
            continue;
        };
        // Tiny absolute values are noise-dominated; allow 1 ms / 1 MB of slack.
        if v > b * 1.10 && v - b > 1.0 {
            failures.push(format!(
                "{k}: {v:.1} vs baseline {b:.1} (+{:.0}%)",
                (v / b - 1.0) * 100.0
            ));
        }
    }
    if failures.is_empty() {
        println!("no regressions vs baseline ({} metrics)", base.len());
        Ok(())
    } else {
        Err(format!(
            "bench regressed more than 10%:\n  {}",
            failures.join("\n  ")
        ))
    }
}
