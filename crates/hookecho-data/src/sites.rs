//! `sites`: NOAA NCEI's public NEXRAD/TDWR station list → sites.tsv.

use crate::fetch;
use std::collections::BTreeSet;
use std::path::Path;

pub const SOURCE: &str = "https://www.ncei.noaa.gov/access/homr/file/nexrad-stations.txt";

/// WSR-88Ds in the NCEI list that are research or test radars, not part of
/// the operational network: KOUN (NOAA NSSL, Norman) and KCRI (NWS Radar
/// Operations Center redundant test RDA, Norman). Shown only when "Show
/// research radars" is on (docs/FEATURE_PARITY.md).
const RESEARCH: [&str; 2] = ["KOUN", "KCRI"];

pub fn run(out: &Path, baseline: Option<&Path>) -> Result<String, String> {
    let client = fetch::client()?;
    let cache = std::env::temp_dir().join("hookecho-data");
    let text = std::fs::read_to_string(fetch::cached(&client, SOURCE, &cache)?)
        .map_err(|e| e.to_string())?;
    let rows = parse_ncei(&text)?;
    let tsv = render(&rows);
    let parsed = hookecho_core::sites::parse(&tsv);
    if parsed.len() != rows.len() {
        return Err("generated TSV doesn't round-trip through hookecho-core".into());
    }
    if let Some(b) = baseline {
        let old = std::fs::read_to_string(b).map_err(|e| format!("{}: {e}", b.display()))?;
        let old_ids: BTreeSet<String> = hookecho_core::sites::parse(&old)
            .into_iter()
            .map(|s| s.id)
            .collect();
        let new_ids: BTreeSet<String> = parsed.iter().map(|s| s.id.clone()).collect();
        let lost: Vec<_> = old_ids.difference(&new_ids).collect();
        if !lost.is_empty() {
            return Err(format!(
                "refusing to write: {} site(s) lost vs baseline: {lost:?}",
                lost.len()
            ));
        }
    }
    std::fs::write(out, tsv).map_err(|e| e.to_string())?;
    Ok(format!("wrote {} sites to {}", rows.len(), out.display()))
}

pub struct Row {
    id: String,
    name: String,
    state: String,
    lat: f64,
    lon: f64,
    elevation_m: f64,
    kind: &'static str,
}

/// The file is fixed-width; column spans come from the dashed ruler line
/// under the header, so a column change upstream is detected, not misread.
fn parse_ncei(text: &str) -> Result<Vec<Row>, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or("empty station list")?;
    let ruler = lines.next().ok_or("missing ruler line")?;
    let mut spans = Vec::new();
    let mut start = None;
    for (i, c) in ruler
        .char_indices()
        .chain(std::iter::once((ruler.len(), ' ')))
    {
        match (c == '-', start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                spans.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    let names: Vec<String> = spans
        .iter()
        .map(|&(s, e)| {
            header
                .get(s..e.min(header.len()))
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .collect();
    let col = |n: &str| {
        names
            .iter()
            .position(|c| c == n)
            .ok_or(format!("column {n} missing from NCEI list"))
    };
    let (icao, name, st, lat, lon, elev, ty) = (
        col("ICAO")?,
        col("NAME")?,
        col("ST")?,
        col("LAT")?,
        col("LON")?,
        col("ELEV")?,
        col("STNTYPE")?,
    );
    let mut rows = Vec::new();
    for l in lines.filter(|l| !l.trim().is_empty()) {
        let f = |i: usize| {
            let (s, e) = spans[i];
            // The last column runs to end of line.
            let e = if i == spans.len() - 1 { l.len() } else { e };
            l.get(s.min(l.len())..e.min(l.len()))
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let kind = match f(ty).as_str() {
            "NEXRAD" if RESEARCH.contains(&f(icao).as_str()) => "RESEARCH",
            "NEXRAD" => "NEXRAD",
            "TDWR" => "TDWR",
            _ => continue,
        };
        let num = |i: usize| {
            f(i).parse::<f64>()
                .map_err(|e| format!("{}: {e} in {l:?}", names[i]))
        };
        rows.push(Row {
            id: f(icao),
            name: title_case(&f(name)),
            state: f(st),
            lat: num(lat)?,
            lon: num(lon)?,
            elevation_m: (num(elev)? * 0.3048 * 10.0).round() / 10.0, // NCEI ELEV is feet
            kind,
        });
    }
    if rows.len() < 150 {
        return Err(format!("only {} stations parsed; refusing", rows.len()));
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

fn title_case(s: &str) -> String {
    const KEEP: [&str; 10] = [
        "AFB", "AAF", "NAS", "NWS", "USAF", "US", "NSSL", "ROC", "FAA", "RDA",
    ];
    s.split_whitespace()
        .map(|w| {
            if KEEP.contains(&w) {
                w.to_string()
            } else {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_string() + &c.as_str().to_lowercase())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render(rows: &[Row]) -> String {
    let mut s = format!(
        "# Generated by `hookecho-data sites` from {SOURCE}\n# id\tname\tstate\tlat\tlon\televation_m\tkind\n"
    );
    for r in rows {
        s += &format!(
            "{}\t{}\t{}\t{:.6}\t{:.6}\t{:.1}\t{}\n",
            r.id, r.name, r.state, r.lat, r.lon, r.elevation_m, r.kind
        );
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn title_case_keeps_abbreviations() {
        assert_eq!(
            super::title_case("ANDERSEN AFB AGANA"),
            "Andersen AFB Agana"
        );
        assert_eq!(super::title_case("OKLAHOMA CITY"), "Oklahoma City");
    }
}
