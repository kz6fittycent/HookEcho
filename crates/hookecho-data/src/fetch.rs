//! Polite HTTP fetching: descriptive User-Agent, on-disk cache, conditional
//! requests (If-Modified-Since), no retries hammering a failing endpoint.

use std::path::{Path, PathBuf};
use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "hookecho-data/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/kz6fittycent/HookEcho)"
);

pub fn client() -> Result<reqwest::blocking::Client, String> {
    // Ignore the error: another call may already have installed it.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())
}

/// Downloads `url` into `cache/`, reusing the cached copy when the server
/// says it is unchanged. Returns the local path.
pub fn cached(
    client: &reqwest::blocking::Client,
    url: &str,
    cache: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let name = url.rsplit('/').next().unwrap_or("download");
    let path = cache.join(name);
    let mut req = client.get(url);
    if let Ok(meta) = std::fs::metadata(&path)
        && let Ok(modified) = meta.modified()
    {
        req = req.header(reqwest::header::IF_MODIFIED_SINCE, httpdate(modified));
    }
    let resp = req.send().map_err(|e| format!("{url}: {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(path);
    }
    if !resp.status().is_success() {
        return Err(format!("{url}: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().map_err(|e| format!("{url}: {e}"))?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    eprintln!("fetched {url} ({} KB)", bytes.len() / 1024);
    Ok(path)
}

/// RFC 7231 IMF-fixdate, without pulling in a date crate.
fn httpdate(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant's algorithm, public domain).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    const WD: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MO: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        WD[days.rem_euclid(7) as usize],
        d,
        MO[(m - 1) as usize],
        y,
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn httpdate_known_value() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(784_111_777);
        assert_eq!(super::httpdate(t), "Sun, 06 Nov 1994 08:49:37 GMT");
    }
}
