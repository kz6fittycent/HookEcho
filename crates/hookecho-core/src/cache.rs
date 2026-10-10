//! On-disk chunk cache. Chunk objects are immutable (their key includes the
//! volume start time), so a cached copy never needs revalidating. The cache
//! makes cold starts instant and keeps the last data visible offline.

use std::path::{Path, PathBuf};

pub struct ChunkCache {
    dir: PathBuf,
    max_bytes: u64,
}

/// Default size cap: enough for a few hours of one site.
pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;

impl ChunkCache {
    pub fn new(dir: &Path, max_bytes: u64) -> ChunkCache {
        ChunkCache {
            dir: dir.join("chunks"),
            max_bytes,
        }
    }

    /// Path for a chunk key (`SITE/vol/name`). Keys come from parsing, so
    /// they never contain `..`; still, reject anything that isn't a plain
    /// relative path.
    fn path(&self, key: &str) -> Option<PathBuf> {
        let ok = !key.is_empty()
            && key
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains('\\'));
        ok.then(|| self.dir.join(key))
    }

    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(key)?).ok()
    }

    /// Writes atomically (temp file + rename).
    pub fn put(&self, key: &str, data: &[u8]) -> std::io::Result<()> {
        let p = self
            .path(key)
            .ok_or_else(|| std::io::Error::other("bad cache key"))?;
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("part");
        std::fs::write(&tmp, data)?;
        std::fs::rename(tmp, p)
    }

    /// Remembers the newest volume seen for a site, for offline cold starts.
    pub fn set_latest(&self, site: &str, prefix: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(self.dir.join(site))?;
        std::fs::write(self.dir.join(site).join("latest"), prefix)
    }

    pub fn latest(&self, site: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.join(site).join("latest"))
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Cached chunk keys of one volume prefix (`SITE/vol/STAMP`), in sequence order.
    pub fn volume_keys(&self, prefix: &str) -> Vec<String> {
        let Some((site_vol, stamp)) = prefix.rsplit_once('/') else {
            return Vec::new();
        };
        let Ok(rd) = std::fs::read_dir(self.dir.join(site_vol)) else {
            return Vec::new();
        };
        let mut keys: Vec<String> = rd
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with(stamp) && !n.ends_with(".part"))
            .map(|n| format!("{site_vol}/{n}"))
            .collect();
        keys.sort();
        keys
    }

    /// Deletes the oldest files until the cache is under its cap.
    pub fn trim(&self) -> std::io::Result<u64> {
        let mut files = Vec::new();
        let mut total = 0u64;
        let mut stack = vec![self.dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let Ok(m) = e.metadata() else { continue };
                if m.is_dir() {
                    stack.push(e.path());
                } else if e.file_name() != "latest" {
                    total += m.len();
                    files.push((m.modified().ok(), m.len(), e.path()));
                }
            }
        }
        files.sort_by_key(|f| f.0);
        let mut freed = 0;
        for (_, len, path) in files {
            if total <= self.max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
                freed += len;
            }
        }
        Ok(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_list_trim() {
        let dir = std::env::temp_dir().join(format!("hookecho-cache-{}", std::process::id()));
        let c = ChunkCache::new(&dir, 10);
        c.put("KTLX/276/20261008-140202-001-S", b"abcdef").unwrap();
        c.put("KTLX/276/20261008-140202-002-I", b"ghijkl").unwrap();
        assert_eq!(c.get("KTLX/276/20261008-140202-001-S").unwrap(), b"abcdef");
        assert_eq!(c.volume_keys("KTLX/276/20261008-140202").len(), 2);
        assert!(c.get("../../etc/passwd").is_none());
        assert!(c.put("KTLX/../x", b"no").is_err());
        c.set_latest("KTLX", "KTLX/276/20261008-140202").unwrap();
        assert_eq!(
            c.latest("KTLX").as_deref(),
            Some("KTLX/276/20261008-140202")
        );
        assert!(
            c.trim().unwrap() >= 2,
            "12 bytes > 10 byte cap: oldest goes"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
