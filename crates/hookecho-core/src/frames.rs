//! Loop frames: the volumes before the live one, fetched from the real-time
//! chunk bucket but only as far as the selected tilt. At 0.5° that's about
//! six chunks (~2 MB) per frame instead of a whole volume (~20 MB), and the
//! disk cache makes repeats free. Each frame keeps only the sweeps at that
//! tilt (every user-facing moment, so product switches need no refetch).

use crate::Moment;
use crate::cache::ChunkCache;
use crate::level2::{Sweep, Volume};
use crate::realtime::{ChunkKey, LiveVolume};
use crate::s3::{self, Bucket};
use std::sync::{Arc, mpsc};

#[derive(Clone, Debug, PartialEq)]
pub struct LoopRequest {
    pub site: String,
    /// Commanded tilt, degrees (e.g. 0.5).
    pub tilt_deg: f32,
    /// Frames wanted, not counting the live volume.
    pub frames: usize,
    /// Volume prefix of the live volume (`SITE/vol/STAMP`); frames are the
    /// volumes before it.
    pub before: String,
}

#[derive(Clone, Debug)]
pub struct LoopFrame {
    pub request: LoopRequest,
    pub prefix: String,
    /// Volume start, Unix seconds.
    pub start: i64,
    /// Only the sweeps at the requested tilt.
    pub volume: Arc<Volume>,
}

pub enum LoopEvent {
    Frame(LoopFrame),
    /// Every frame the bucket had for this request has been sent.
    Done(LoopRequest),
    Failed(LoopRequest, String),
}

pub struct LoopLoader {
    requests: tokio::sync::watch::Sender<Option<LoopRequest>>,
    pub events: mpsc::Receiver<LoopEvent>,
}

impl LoopLoader {
    pub fn start(cache_dir: std::path::PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> LoopLoader {
        let (req_tx, mut req_rx) = tokio::sync::watch::channel(None::<LoopRequest>);
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("loop-frames".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(async move {
                    let cache = ChunkCache::new(&cache_dir, crate::cache::DEFAULT_MAX_BYTES);
                    let Ok(bucket) = Bucket::new(s3::CHUNKS_BUCKET) else {
                        return;
                    };
                    let send = |e: LoopEvent| {
                        let _ = tx.send(e);
                        wake();
                    };
                    loop {
                        let req = req_rx.borrow_and_update().clone();
                        if let Some(req) = req {
                            // A newer request cancels this one.
                            tokio::select! {
                                r = load(&bucket, &cache, &req, &send) => match r {
                                    Ok(()) => send(LoopEvent::Done(req.clone())),
                                    Err(e) => send(LoopEvent::Failed(req.clone(), e.to_string())),
                                },
                                c = req_rx.changed() => if c.is_err() { return } else { continue },
                            }
                        }
                        if req_rx.changed().await.is_err() {
                            return;
                        }
                    }
                });
            })
            .expect("spawn loop loader");
        LoopLoader {
            requests: req_tx,
            events: rx,
        }
    }

    /// Replaces any request in progress.
    pub fn request(&self, r: LoopRequest) {
        if self.requests.borrow().as_ref() != Some(&r) {
            let _ = self.requests.send(Some(r));
        }
    }
}

/// Frames newest first, so the loop fills in backward from the live frame.
async fn load(
    bucket: &Bucket,
    cache: &ChunkCache,
    req: &LoopRequest,
    send: &impl Fn(LoopEvent),
) -> Result<(), s3::Error> {
    let before = ChunkKey::parse(&format!("{}-001-S", req.before))
        .ok_or_else(|| s3::Error::Parse(req.before.clone()))?;
    // All volumes of the site with their start stamps, newest first, older
    // than the live one.
    let listing = bucket
        .list(&format!("{}/", req.site), Some("/"), None, 1000)
        .await?;
    let mut vols: Vec<u16> = listing
        .prefixes
        .iter()
        .filter_map(|p| p.trim_end_matches('/').rsplit('/').next()?.parse().ok())
        .collect();
    vols.sort_unstable();
    // Walk backward around the ring from the live volume.
    let start_i = vols
        .iter()
        .position(|&v| v == before.volume)
        .unwrap_or(vols.len());
    let order: Vec<u16> = (1..=vols.len())
        .map(|k| vols[(start_i + vols.len() - k) % vols.len()])
        .collect();
    let mut sent = 0;
    let mut newest_stamp = before.stamp.clone();
    for v in order {
        if sent >= req.frames {
            break;
        }
        let keys = bucket
            .list(&format!("{}/{v}/", req.site), None, None, 200)
            .await?;
        let mut chunks: Vec<ChunkKey> = keys
            .keys
            .iter()
            .filter_map(|(k, _)| ChunkKey::parse(k))
            .collect();
        chunks.sort_by_key(|k| k.seq);
        let Some(first) = chunks.first().cloned() else {
            continue;
        };
        // Older than the previous frame, or the ring wrapped into stale data.
        if first.stamp >= newest_stamp {
            continue;
        }
        newest_stamp = first.stamp.clone();
        if let Some(volume) = fetch_tilt(bucket, cache, &chunks, req.tilt_deg).await? {
            send(LoopEvent::Frame(LoopFrame {
                request: req.clone(),
                prefix: format!("{}/{v}/{}", req.site, first.stamp),
                start: first.start_time().unwrap_or(0),
                volume: Arc::new(volume),
            }));
            sent += 1;
        }
    }
    Ok(())
}

fn deg(s: &Sweep) -> f32 {
    (s.nominal_elevation_deg.unwrap_or(s.elevation_deg) * 10.0).round() / 10.0
}

/// Downloads chunks in order until every sweep at `tilt` is complete and the
/// volume has moved past it, then keeps only those sweeps.
async fn fetch_tilt(
    bucket: &Bucket,
    cache: &ChunkCache,
    chunks: &[ChunkKey],
    tilt: f32,
) -> Result<Option<Volume>, s3::Error> {
    let Some(first) = chunks.first() else {
        return Ok(None);
    };
    let mut lv = LiveVolume::for_key(first);
    for k in chunks {
        let key = k.key();
        let data = match cache.get(&key) {
            Some(d) => d,
            None => {
                let d = bucket.get(&key).await?;
                let _ = cache.put(&key, &d);
                d
            }
        };
        if lv.push(k, data).is_err() {
            continue;
        }
        // Decoding the uncompressed stream is cheap; check after each chunk.
        let Ok(v) = lv.decode() else { continue };
        let at: Vec<&Sweep> = v
            .sweeps
            .iter()
            .filter(|s| (deg(s) - tilt).abs() < 0.05)
            .collect();
        let past = v.sweeps.last().is_some_and(|s| deg(s) > tilt + 0.05);
        if (past || lv.is_complete()) && !at.is_empty() && at.iter().all(|s| s.complete) {
            return Ok(Some(trim(v, tilt)));
        }
        if lv.is_complete() {
            return Ok((!at.is_empty()).then(|| trim(v, tilt)));
        }
    }
    Ok(None)
}

/// Keeps only the sweeps at `tilt`, without the diagnostic CFP moment.
fn trim(mut v: Volume, tilt: f32) -> Volume {
    v.sweeps.retain(|s| (deg(s) - tilt).abs() < 0.05);
    for s in &mut v.sweeps {
        s.moments.retain(|(m, _)| *m != Moment::ClutterFilterPower);
    }
    v
}

#[cfg(test)]
mod tests {
    /// Real bucket: three frames of KTLX 0.5° before the newest volume.
    /// `cargo test -p hookecho-core -- --ignored live_`
    #[test]
    #[ignore]
    fn live_loop_frames_arrive_newest_first() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let bucket = super::Bucket::new(super::s3::CHUNKS_BUCKET).unwrap();
        let before = rt
            .block_on(crate::live::latest_volume(&bucket, "KTLX"))
            .unwrap()
            .unwrap();
        let dir = std::env::temp_dir().join(format!("hookecho-frames-{}", std::process::id()));
        let loader = super::LoopLoader::start(dir.clone(), std::sync::Arc::new(|| {}));
        loader.request(super::LoopRequest {
            site: "KTLX".into(),
            tilt_deg: 0.5,
            frames: 3,
            before: before.clone(),
        });
        let mut starts = Vec::new();
        loop {
            match loader
                .events
                .recv_timeout(std::time::Duration::from_secs(120))
                .expect("loop event")
            {
                super::LoopEvent::Frame(f) => {
                    assert!(
                        f.volume.sweeps.iter().all(|s| s.complete),
                        "{}: incomplete sweep",
                        f.prefix
                    );
                    assert!(!f.volume.sweeps.is_empty());
                    eprintln!("frame {} with {} sweeps", f.prefix, f.volume.sweeps.len());
                    starts.push(f.start);
                }
                super::LoopEvent::Done(_) => break,
                super::LoopEvent::Failed(_, e) => panic!("{e}"),
            }
        }
        assert_eq!(starts.len(), 3);
        assert!(
            starts.windows(2).all(|w| w[0] > w[1]),
            "newest first: {starts:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
