//! TDWR (FAA Terminal Doppler Weather Radar) scans from the Level III
//! bucket: finding them, fetching and decoding them (level3.rs) into the
//! same one-sweep `Volume` the app draws for Level II.
//!
//! Objects are named `DAL_TZ0_2026_10_28_21_08_23`: the site without its
//! leading "T" (TDAL → DAL), the product, and the volume time (UTC). A scan
//! is the short-range reflectivity (TZ0) and the velocity (TV0) of the same
//! volume time. Sites publish one about every 6 minutes.

use crate::cache::ChunkCache;
use crate::level2::Volume;
use crate::level3;
use crate::s3::{self, Bucket};
use crate::sites::{self, SiteKind};

pub const BUCKET: &str = "https://unidata-nexrad-level3.s3.amazonaws.com";

/// Products that make up a scan: short-range reflectivity, then velocity.
const PRODUCTS: [&str; 2] = ["TZ0", "TV0"];

/// Whether `site` (e.g. "TDAL") is a TDWR in the bundled site list.
pub fn is_tdwr(site: &str) -> bool {
    sites::by_id(site).is_some_and(|s| s.kind == SiteKind::Tdwr)
}

/// The bucket's three-letter name for a TDWR ("TDAL" → "DAL").
fn short(site: &str) -> &str {
    site.get(1..).unwrap_or(site)
}

/// A scan's volume time as it appears in object names,
/// `YYYY_MM_DD_HH_MM_SS`, with its Unix time.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub text: String,
    pub unix: i64,
}

impl Stamp {
    /// From an object name like `DAL_TZ0_2026_10_28_21_08_23`.
    pub fn from_key(key: &str) -> Option<Stamp> {
        let text = key.splitn(3, '_').nth(2)?;
        let n: Vec<i64> = text
            .split('_')
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [y, mo, d, h, mi, s] = n[..] else {
            return None;
        };
        if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
            return None;
        }
        Some(Stamp {
            text: text.to_string(),
            unix: days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s,
        })
    }

    /// The app's identifier for a scan, `TDAL/2026_10_28_21_08_23`.
    pub fn prefix(&self, site: &str) -> String {
        format!("{site}/{}", self.text)
    }

    /// Back from [`Stamp::prefix`].
    pub fn from_prefix(prefix: &str) -> Option<Stamp> {
        let (site, text) = prefix.split_once('/')?;
        Stamp::from_key(&format!("{}_TZ0_{text}", short(site)))
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`, public domain).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY_MM_DD` of the UTC day `days` after the epoch.
fn day_text(days: i64) -> String {
    // Inverse of days_from_civil (same source).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}_{m:02}_{d:02}")
}

/// Up to `n` scans of `site` older than `before` (Unix seconds), newest
/// first, looking back at most `days` UTC days from `now`.
pub async fn scans(
    bucket: &Bucket,
    site: &str,
    before: i64,
    n: usize,
    now: i64,
    days: i64,
) -> Result<Vec<Stamp>, s3::Error> {
    let mut out: Vec<Stamp> = Vec::new();
    let today = now.div_euclid(86_400);
    for day in (today - days + 1..=today).rev() {
        let prefix = format!("{}_TZ0_{}", short(site), day_text(day));
        let listing = bucket.list(&prefix, None, None, 2000).await?;
        let mut found: Vec<Stamp> = listing
            .keys
            .iter()
            .filter_map(|(k, _)| Stamp::from_key(k))
            .filter(|s| s.unix < before)
            .collect();
        found.sort_by(|a, b| b.cmp(a));
        out.extend(found);
        if out.len() >= n {
            break;
        }
    }
    out.truncate(n);
    Ok(out)
}

/// One product file, from the disk cache or the bucket (then cached).
async fn product(
    bucket: &Bucket,
    cache: &ChunkCache,
    key: &str,
) -> Result<Option<Vec<u8>>, s3::Error> {
    if let Some(d) = cache.get(key) {
        return Ok(Some(d));
    }
    match bucket.get(key).await {
        Ok(d) => {
            let _ = cache.put(key, &d);
            Ok(Some(d))
        }
        // A scan may lack a product (velocity is sometimes late or absent).
        Err(s3::Error::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

/// The scan at `stamp` as a one-sweep volume. Reflectivity is required;
/// velocity joins when present.
pub async fn load(
    bucket: &Bucket,
    cache: &ChunkCache,
    site: &str,
    stamp: &Stamp,
) -> Result<Volume, s3::Error> {
    let mut products = Vec::new();
    for code in PRODUCTS {
        let key = format!("{}_{code}_{}", short(site), stamp.text);
        if let Some(bytes) = product(bucket, cache, &key).await? {
            match level3::decode(&bytes) {
                Ok(p) => products.push(p),
                Err(e) if code == PRODUCTS[0] => return Err(s3::Error::Parse(e.to_string())),
                Err(_) => {}
            }
        } else if code == PRODUCTS[0] {
            return Err(s3::Error::NotFound);
        }
    }
    level3::sweep_volume(&products, site).ok_or_else(|| s3::Error::Parse(stamp.text.clone()))
}

/// The cached scan at `stamp`, without touching the network (cold start).
pub fn load_cached(cache: &ChunkCache, site: &str, stamp: &Stamp) -> Option<Volume> {
    let products: Vec<level3::Product> = PRODUCTS
        .iter()
        .filter_map(|code| cache.get(&format!("{}_{code}_{}", short(site), stamp.text)))
        .filter_map(|b| level3::decode(&b).ok())
        .collect();
    level3::sweep_volume(&products, site)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_and_days() {
        let s = Stamp::from_key("DAL_TZ0_2026_10_28_21_08_23").unwrap();
        assert_eq!(s.unix, 1_793_221_703);
        assert_eq!(s.prefix("TDAL"), "TDAL/2026_10_28_21_08_23");
        assert_eq!(Stamp::from_prefix("TDAL/2026_10_28_21_08_23"), Some(s));
        assert_eq!(day_text(1_793_221_703 / 86_400), "2026_10_28");
        assert_eq!(day_text(0), "1970_01_01");
        assert_eq!(day_text(days_from_civil(2024, 2, 29)), "2024_02_29");
        assert!(Stamp::from_key("DAL_TZ0_2026_13_28_21_08_23").is_none());
        assert!(Stamp::from_key("DAL_TZ0_garbage").is_none());
        assert!(Stamp::from_key("").is_none());
    }

    #[test]
    fn tdwr_sites() {
        assert!(is_tdwr("TDAL") && is_tdwr("TDFW"));
        assert!(!is_tdwr("KFWS") && !is_tdwr("NOPE"));
        assert_eq!(short("TDAL"), "DAL");
    }

    /// The disk cache alone rebuilds a scan (cold start, offline).
    #[test]
    fn cached_scan() {
        let dir = std::env::temp_dir().join(format!("hookecho-tdwr-{}", std::process::id()));
        let cache = ChunkCache::new(&dir, crate::cache::DEFAULT_MAX_BYTES);
        let data = format!("{}/../../tests/data/level3", env!("CARGO_MANIFEST_DIR"));
        for code in PRODUCTS {
            let key = format!("DAL_{code}_2026_10_28_21_08_23");
            cache
                .put(&key, &std::fs::read(format!("{data}/{key}")).unwrap())
                .unwrap();
        }
        let stamp = Stamp::from_key("DAL_TZ0_2026_10_28_21_08_23").unwrap();
        let v = load_cached(&cache, "TDAL", &stamp).unwrap();
        let s = &v.sweeps[0];
        assert!(s.field(crate::Moment::Reflectivity).is_some());
        assert!(s.field(crate::Moment::Velocity).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against the real bucket: `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_tdal_scans() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let b = Bucket::new(BUCKET).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let found = rt.block_on(scans(&b, "TDAL", i64::MAX, 3, now, 2)).unwrap();
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(
            now - found[0].unix < 3600,
            "newest TDAL scan is old: {found:?}"
        );
        let dir = std::env::temp_dir().join(format!("hookecho-tdwr-live-{}", std::process::id()));
        let cache = ChunkCache::new(&dir, crate::cache::DEFAULT_MAX_BYTES);
        let v = rt.block_on(load(&b, &cache, "TDAL", &found[0])).unwrap();
        assert_eq!(v.sweeps[0].radials.len(), 360);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
