//! Live feed: follows one site's real-time Level II volumes on a background
//! thread and reports each new partial or complete volume to the UI. Never
//! blocks the UI thread (KICKOFF §4); on network trouble it keeps the last
//! data, retries with backoff and says so.

use crate::cache::ChunkCache;
use crate::level2::Volume;
use crate::realtime::{ChunkKey, LiveVolume, next_volume};
use crate::s3::{self, Bucket};
use crate::tdwr;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

/// How often to look for the next chunk. New chunks arrive every ~5–12 s.
const POLL: Duration = Duration::from_secs(4);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct LiveUpdate {
    pub site: String,
    /// `SITE/vol/STAMP`: identifies the volume (stable across chunks).
    pub prefix: String,
    pub volume: Arc<Volume>,
    pub chunks: u16,
    pub complete: bool,
    /// Built from the disk cache before any network answer.
    pub from_cache: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FeedState {
    Connecting,
    Live,
    /// The last attempt failed; the next one is in `wait_s`.
    Retrying {
        attempt: u32,
        wait_s: u64,
        error: String,
    },
    /// The bucket has no volumes for this site.
    NoData,
}

#[derive(Clone, Debug)]
pub enum FeedEvent {
    Update(LiveUpdate),
    State { site: String, state: FeedState },
}

pub struct Feed {
    sites: tokio::sync::watch::Sender<Option<String>>,
    pub events: mpsc::Receiver<FeedEvent>,
}

impl Feed {
    /// Starts the feed thread. `wake` is called after every event so the UI
    /// can repaint (the app stays idle otherwise).
    pub fn start(cache_dir: std::path::PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Feed {
        let (site_tx, site_rx) = tokio::sync::watch::channel(None::<String>);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("live-feed".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(run(
                    site_rx,
                    tx,
                    wake,
                    ChunkCache::new(&cache_dir, crate::cache::DEFAULT_MAX_BYTES),
                ));
            })
            .expect("spawn live feed");
        Feed {
            sites: site_tx,
            events: rx,
        }
    }

    /// Follows `site` from now on (stops following the previous one).
    pub fn watch(&self, site: &str) {
        let _ = self.sites.send(Some(site.to_string()));
    }
}

struct Ctx {
    tx: mpsc::Sender<FeedEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
    cache: Arc<ChunkCache>,
}

impl Ctx {
    fn send(&self, e: FeedEvent) {
        let _ = self.tx.send(e);
        (self.wake)();
    }

    fn state(&self, site: &str, state: FeedState) {
        self.send(FeedEvent::State {
            site: site.to_string(),
            state,
        });
    }
}

async fn run(
    mut sites: tokio::sync::watch::Receiver<Option<String>>,
    tx: mpsc::Sender<FeedEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
    cache: ChunkCache,
) {
    let ctx = Ctx {
        tx,
        wake,
        cache: Arc::new(cache),
    };
    let _ = ctx.cache.trim();
    let (Ok(bucket), Ok(level3)) = (Bucket::new(s3::CHUNKS_BUCKET), Bucket::new(tdwr::BUCKET))
    else {
        return;
    };
    loop {
        let site = sites.borrow_and_update().clone();
        match site {
            Some(site) if tdwr::is_tdwr(&site) => {
                tokio::select! {
                    _ = follow_tdwr(&level3, &ctx, &site) => {}
                    r = sites.changed() => if r.is_err() { return },
                }
            }
            Some(site) => {
                tokio::select! {
                    _ = follow(&bucket, &ctx, &site) => {}
                    r = sites.changed() => if r.is_err() { return },
                }
            }
            None => {
                if sites.changed().await.is_err() {
                    return;
                }
            }
        }
    }
}

/// Follows one site forever (until the caller drops the future).
async fn follow(bucket: &Bucket, ctx: &Ctx, site: &str) {
    ctx.state(site, FeedState::Connecting);
    // Cold start from cache: show the last known volume immediately.
    if let Some(prefix) = ctx.cache.latest(site)
        && let Some(lv) = assemble_cached(ctx, &prefix)
    {
        emit(ctx, site, &prefix, &lv, true).await;
    }
    let mut backoff = Duration::from_secs(2);
    let mut attempt = 0;
    let mut current: Option<(String, LiveVolume, String)> = None; // (prefix, volume, last key)
    // Only state *changes* are sent: an idle app must not be woken every poll.
    let mut last_state = FeedState::Connecting;
    let mut set = |state: FeedState| {
        if state != last_state {
            ctx.state(site, state.clone());
            last_state = state;
        }
    };
    loop {
        let step = step(bucket, ctx, site, &mut current).await;
        match step {
            Ok(Some(())) => {
                set(FeedState::Live);
                attempt = 0;
                backoff = Duration::from_secs(2);
                tokio::time::sleep(POLL).await;
            }
            Ok(None) => {
                set(FeedState::NoData);
                tokio::time::sleep(MAX_BACKOFF).await;
            }
            Err(e) => {
                attempt += 1;
                set(FeedState::Retrying {
                    attempt,
                    wait_s: backoff.as_secs(),
                    error: e.to_string(),
                });
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// How often to look for a new TDWR scan; sites publish one about every
/// 6 minutes.
const TDWR_POLL: Duration = Duration::from_secs(60);

/// Follows one TDWR site (tdwr.rs): each new scan is one complete volume.
async fn follow_tdwr(bucket: &Bucket, ctx: &Ctx, site: &str) {
    ctx.state(site, FeedState::Connecting);
    let mut shown: Option<tdwr::Stamp> = None;
    if let Some(stamp) = ctx
        .cache
        .latest(site)
        .and_then(|p| tdwr::Stamp::from_prefix(&p))
        && let Some(v) = tdwr::load_cached(&ctx.cache, site, &stamp)
    {
        send_scan(ctx, site, &stamp, v, true);
        shown = Some(stamp);
    }
    let mut backoff = Duration::from_secs(2);
    let mut attempt = 0;
    let mut last_state = FeedState::Connecting;
    let mut set = |state: FeedState| {
        if state != last_state {
            ctx.state(site, state.clone());
            last_state = state;
        }
    };
    loop {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let step = async {
            let newest = tdwr::scans(bucket, site, i64::MAX, 1, now, 2).await?;
            let Some(stamp) = newest.into_iter().next() else {
                return Ok(None);
            };
            if shown.as_ref() != Some(&stamp) {
                let v = tdwr::load(bucket, &ctx.cache, site, &stamp).await?;
                let _ = ctx.cache.set_latest(site, &stamp.prefix(site));
                send_scan(ctx, site, &stamp, v, false);
                shown = Some(stamp);
            }
            Ok::<_, s3::Error>(Some(()))
        };
        match step.await {
            Ok(Some(())) => {
                set(FeedState::Live);
                attempt = 0;
                backoff = Duration::from_secs(2);
                tokio::time::sleep(TDWR_POLL).await;
            }
            Ok(None) => {
                set(FeedState::NoData);
                tokio::time::sleep(MAX_BACKOFF).await;
            }
            Err(e) => {
                attempt += 1;
                set(FeedState::Retrying {
                    attempt,
                    wait_s: backoff.as_secs(),
                    error: e.to_string(),
                });
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

fn send_scan(ctx: &Ctx, site: &str, stamp: &tdwr::Stamp, volume: Volume, from_cache: bool) {
    ctx.send(FeedEvent::Update(LiveUpdate {
        site: site.to_string(),
        prefix: stamp.prefix(site),
        volume: Arc::new(volume),
        chunks: 1,
        complete: true,
        from_cache,
    }));
}

/// One polling step. `Ok(None)`: the site has no data at all.
async fn step(
    bucket: &Bucket,
    ctx: &Ctx,
    site: &str,
    current: &mut Option<(String, LiveVolume, String)>,
) -> Result<Option<()>, s3::Error> {
    // Find a volume to follow.
    if current.is_none() {
        let Some(prefix) = latest_volume(bucket, site).await? else {
            return Ok(None);
        };
        let k = ChunkKey::parse(&format!("{prefix}-001-S"))
            .ok_or_else(|| s3::Error::Parse(prefix.clone()))?;
        *current = Some((prefix, LiveVolume::for_key(&k), String::new()));
    }
    let (prefix, lv, last) = current.as_mut().unwrap();
    // Next volume once this one is complete.
    if lv.is_complete() {
        let next = format!("{site}/{}/", next_volume(lv.volume));
        let l = bucket.list_page(&next, None, None, 1, None).await?;
        if let Some(k) = l.keys.first().and_then(|(k, _)| ChunkKey::parse(k))
            && k.stamp > lv.stamp
        {
            let p = format!("{site}/{}/{}", k.volume, k.stamp);
            *current = Some((p, LiveVolume::for_key(&k), String::new()));
            return Box::pin(step(bucket, ctx, site, current)).await;
        }
        return Ok(Some(()));
    }
    // New chunks in this volume.
    let start_after = if last.is_empty() {
        None
    } else {
        Some(last.as_str())
    };
    let vol_prefix = format!(
        "{}/",
        prefix.rsplit_once('/').map_or(prefix.as_str(), |p| p.0)
    );
    let listing = bucket.list(&vol_prefix, None, start_after, 200).await?;
    let mut keys: Vec<ChunkKey> = listing
        .keys
        .iter()
        .filter_map(|(k, _)| ChunkKey::parse(k))
        .filter(|k| k.stamp == lv.stamp)
        .collect();
    keys.sort_by_key(|k| k.seq);
    let mut grew = false;
    for k in keys {
        let key = k.key();
        let data = match ctx.cache.get(&key) {
            Some(d) => d,
            None => {
                let d = bucket.get(&key).await?;
                let _ = ctx.cache.put(&key, &d);
                d
            }
        };
        // A malformed chunk is skipped, not fatal: the next one may be fine.
        if let Ok(g) = lv.push(&k, data) {
            grew |= g;
        }
        *last = key;
    }
    if grew {
        let _ = ctx.cache.set_latest(site, prefix);
        let (p, lv_ref) = (prefix.clone(), &*lv);
        emit(ctx, site, &p, lv_ref, false).await;
    }
    Ok(Some(()))
}

/// The newest volume prefix (`SITE/vol/STAMP`) in the bucket for `site`.
pub async fn latest_volume(bucket: &Bucket, site: &str) -> Result<Option<String>, s3::Error> {
    let listing = bucket
        .list(&format!("{site}/"), Some("/"), None, 1000)
        .await?;
    let mut vols: Vec<u16> = listing
        .prefixes
        .iter()
        .filter_map(|p| p.trim_end_matches('/').rsplit('/').next()?.parse().ok())
        .collect();
    vols.sort_unstable();
    let first = |v: u16| async move {
        let l = bucket
            .list_page(&format!("{site}/{v}/"), None, None, 1, None)
            .await?;
        Ok::<_, s3::Error>(l.keys.first().and_then(|(k, _)| ChunkKey::parse(k)))
    };
    let newest = s3::newest_in_ring(vols.len(), |i| {
        let v = vols[i];
        async move {
            Ok(first(v)
                .await?
                .and_then(|k| k.start_time())
                .unwrap_or(i64::MIN))
        }
    })
    .await?;
    let Some(i) = newest else { return Ok(None) };
    Ok(first(vols[i])
        .await?
        .map(|k| format!("{site}/{}/{}", k.volume, k.stamp)))
}

fn assemble_cached(ctx: &Ctx, prefix: &str) -> Option<LiveVolume> {
    let keys = ctx.cache.volume_keys(prefix);
    let first = ChunkKey::parse(keys.first()?)?;
    let mut lv = LiveVolume::for_key(&first);
    for k in keys {
        let key = ChunkKey::parse(&k)?;
        let _ = lv.push(&key, ctx.cache.get(&k)?);
    }
    (lv.chunks() > 0).then_some(lv)
}

async fn emit(ctx: &Ctx, site: &str, prefix: &str, lv: &LiveVolume, from_cache: bool) {
    // Inline on purpose: this thread runs only the feed, and a decode of the
    // uncompressed stream takes ~20-100 ms. (block_in_place would panic on a
    // current-thread runtime; spawn_blocking would need a copy of the stream.)
    let (chunks, complete) = (lv.chunks(), lv.is_complete());
    let decoded = lv.decode();
    if let Ok(v) = decoded {
        ctx.send(FeedEvent::Update(LiveUpdate {
            site: site.to_string(),
            prefix: prefix.to_string(),
            volume: Arc::new(v),
            chunks,
            complete,
            from_cache,
        }));
    }
}

#[cfg(test)]
mod tests {
    /// Live network check against the real bucket; run with
    /// `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_latest_volume_is_recent() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let b = super::Bucket::new(super::s3::CHUNKS_BUCKET).unwrap();
        let p = rt
            .block_on(super::latest_volume(&b, "KTLX"))
            .unwrap()
            .expect("KTLX has volumes");
        let k = super::ChunkKey::parse(&format!("{p}-001-S")).unwrap();
        let age = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - k.start_time().unwrap();
        assert!(age < 3600, "newest KTLX volume {p} is {age} s old");
    }
}

#[cfg(test)]
mod feed_tests {
    /// End-to-end against the real bucket: the feed must deliver a decoded,
    /// recent volume within 60 s. `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_feed_delivers_a_volume() {
        let dir = std::env::temp_dir().join(format!("hookecho-feed-{}", std::process::id()));
        let feed = super::Feed::start(dir.clone(), std::sync::Arc::new(|| {}));
        feed.watch("KTLX");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut states = Vec::new();
        while std::time::Instant::now() < deadline {
            match feed.events.recv_timeout(std::time::Duration::from_secs(5)) {
                Ok(super::FeedEvent::Update(u)) if !u.from_cache => {
                    assert_eq!(u.site, "KTLX");
                    assert!(u.chunks >= 1);
                    eprintln!(
                        "{}: {} chunks, {} sweeps, states {states:?}",
                        u.prefix,
                        u.chunks,
                        u.volume.sweeps.len()
                    );
                    let _ = std::fs::remove_dir_all(&dir);
                    return;
                }
                Ok(super::FeedEvent::State { state, .. }) => states.push(state),
                _ => {}
            }
        }
        panic!("no live update within 60 s; states {states:?}");
    }

    /// The TDWR path (tdwr.rs): a recent Love Field scan, with reflectivity
    /// and velocity. `cargo test -p hookecho-core -- --ignored live_`.
    #[test]
    #[ignore]
    fn live_feed_delivers_a_tdwr_scan() {
        let dir = std::env::temp_dir().join(format!("hookecho-feed-tdwr-{}", std::process::id()));
        let feed = super::Feed::start(dir.clone(), std::sync::Arc::new(|| {}));
        feed.watch("TDAL");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            if let Ok(super::FeedEvent::Update(u)) =
                feed.events.recv_timeout(std::time::Duration::from_secs(5))
            {
                assert_eq!(u.site, "TDAL");
                assert!(u.complete && u.prefix.starts_with("TDAL/"));
                let s = &u.volume.sweeps[0];
                assert!(s.field(crate::Moment::Reflectivity).is_some());
                assert!(s.field(crate::Moment::Velocity).is_some());
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        }
        panic!("no TDAL scan within 60 s");
    }
}
