//! hookecho-data: regenerates the bundled site list and basemap from public
//! sources (docs/PROVENANCE.md). Run by `refresh-radar-data.yml` weekly and
//! by hand. Every command refuses to write output that lost entries compared
//! to `--baseline`, so a broken upstream never ships a regression.
//!
//!   hookecho-data sites   --out crates/hookecho-core/data/sites.tsv [--baseline FILE]
//!   hookecho-data basemap --out assets/basemap/basemap.bin --cache DIR [--baseline FILE]

mod basemap;
mod fetch;
mod sites;

use std::path::PathBuf;
use std::process::ExitCode;

fn arg(args: &[String], name: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = arg(&args, "--out");
    let baseline = arg(&args, "--baseline");
    let result = match (args.first().map(String::as_str), out) {
        (Some("sites"), Some(out)) => sites::run(&out, baseline.as_deref()),
        (Some("basemap"), Some(out)) => {
            let cache = arg(&args, "--cache")
                .unwrap_or_else(|| PathBuf::from("target/hookecho-data-cache"));
            basemap::run(&out, &cache, baseline.as_deref())
        }
        _ => Err(
            "usage: hookecho-data (sites|basemap) --out PATH [--cache DIR] [--baseline PATH]"
                .into(),
        ),
    };
    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hookecho-data: {e}");
            ExitCode::FAILURE
        }
    }
}
