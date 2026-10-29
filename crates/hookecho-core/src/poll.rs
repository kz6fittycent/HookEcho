//! The polling feeds behind the map's data layers (warnings, SPC, surface
//! observations, space weather): one background thread per feed that asks
//! its [`Source`] for fresh data on the source's own schedule, only while
//! the layer is switched on, and never blocks the UI.
//!
//! [`Http`] does the conditional GETs: it remembers each URL's `ETag` and
//! `Last-Modified` and the body that came with them, so an unchanged answer
//! costs a 304 and the source still gets the bytes back to work with.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// A feed's data source.
pub trait Source: Send + 'static {
    type Data: Send + Sync + 'static;

    /// Thread name, and the stem of the disk cache file.
    const NAME: &'static str;

    /// Time between polls after a success.
    fn every(&self) -> Duration;

    /// One poll. `Ok(None)` when nothing changed since the last one. The
    /// future runs on the feed's own single-threaded runtime, so it needn't
    /// be `Send`.
    #[allow(async_fn_in_trait)]
    async fn fetch(&mut self, http: &mut Http) -> Result<Option<Self::Data>, String>;

    /// Bytes to keep on disk for a cold start, written after each `Ok(Some)`.
    fn save(&self, _data: &Self::Data) -> Option<Vec<u8>> {
        None
    }

    /// Reads back what [`Source::save`] wrote.
    fn load(&self, _bytes: &[u8]) -> Option<Self::Data> {
        None
    }
}

#[derive(Clone, Debug)]
pub enum Event<T> {
    /// The current data (from the network, or the disk cache at start).
    Data(Arc<T>),
    /// The last poll failed; the previous data still stands.
    Trouble(String),
}

pub struct Feed<T> {
    enabled: tokio::sync::watch::Sender<bool>,
    /// Asks for a poll now, ahead of schedule.
    kick: tokio::sync::watch::Sender<()>,
    pub events: mpsc::Receiver<Event<T>>,
}

/// Longest wait between retries after failures.
const MAX_BACKOFF: Duration = Duration::from_secs(600);

impl<T: Send + Sync + 'static> Feed<T> {
    /// Starts the polling thread, paused until [`Feed::enable`]. `wake` is
    /// called after every event so the UI can repaint.
    pub fn start<S: Source<Data = T>>(
        source: S,
        cache_dir: PathBuf,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Feed<T> {
        let (on_tx, on_rx) = tokio::sync::watch::channel(false);
        let (kick_tx, kick_rx) = tokio::sync::watch::channel(());
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name(S::NAME.into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(run(source, on_rx, kick_rx, tx, wake, cache_dir));
            })
            .expect("spawn feed");
        Feed {
            enabled: on_tx,
            kick: kick_tx,
            events: rx,
        }
    }

    /// Polls while `on`; when off, makes no requests at all.
    pub fn enable(&self, on: bool) {
        self.enabled
            .send_if_modified(|v| std::mem::replace(v, on) != on);
    }

    /// Polls as soon as the feed is enabled, without waiting for the timer.
    pub fn poll_now(&self) {
        let _ = self.kick.send(());
    }

    /// Everything that arrived since the last call: the newest data, if any,
    /// and whether the last poll failed (`Some(why)`) or succeeded (`None`).
    #[allow(clippy::type_complexity)]
    pub fn drain(&self) -> (Option<Arc<T>>, Option<Option<String>>) {
        let (mut data, mut trouble) = (None, None);
        while let Ok(e) = self.events.try_recv() {
            match e {
                Event::Data(d) => {
                    data = Some(d);
                    trouble = Some(None);
                }
                Event::Trouble(why) => trouble = Some(Some(why)),
            }
        }
        (data, trouble)
    }
}

fn cache_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.cache"))
}

async fn run<S: Source>(
    mut source: S,
    mut enabled: tokio::sync::watch::Receiver<bool>,
    mut kick: tokio::sync::watch::Receiver<()>,
    tx: mpsc::Sender<Event<S::Data>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    cache_dir: PathBuf,
) {
    let send = |e: Event<S::Data>| {
        let _ = tx.send(e);
        wake();
    };
    let path = cache_path(&cache_dir, S::NAME);
    if let Some(d) = std::fs::read(&path).ok().and_then(|b| source.load(&b)) {
        send(Event::Data(Arc::new(d)));
    }
    let Some(mut http) = Http::new() else {
        return;
    };
    let mut backoff = source.every();
    loop {
        while !*enabled.borrow_and_update() {
            if enabled.changed().await.is_err() {
                return;
            }
        }
        kick.borrow_and_update();
        let wait = match source.fetch(&mut http).await {
            Ok(Some(d)) => {
                if let Some(bytes) = source.save(&d) {
                    let _ = std::fs::create_dir_all(&cache_dir);
                    let _ = std::fs::write(&path, bytes);
                }
                send(Event::Data(Arc::new(d)));
                backoff = source.every();
                backoff
            }
            Ok(None) => {
                backoff = source.every();
                backoff
            }
            Err(e) => {
                send(Event::Trouble(e));
                backoff = (backoff * 2).clamp(Duration::from_secs(30), MAX_BACKOFF);
                backoff
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            r = enabled.changed() => if r.is_err() { return },
            r = kick.changed() => if r.is_err() { return },
        }
    }
}

/// A body and the validators it came with.
struct Cached {
    etag: Option<String>,
    modified: Option<String>,
    body: Arc<Vec<u8>>,
}

/// An HTTP client that revalidates what it fetched before.
pub struct Http {
    client: reqwest::Client,
    cache: HashMap<String, Cached>,
}

/// One answer: the body, and whether it changed since the last fetch of
/// the same URL (false on a 304).
pub struct Got {
    pub body: Arc<Vec<u8>>,
    pub changed: bool,
}

impl Http {
    /// The client every feed uses: our User-Agent (the NWS asks for a
    /// descriptive one), rustls, and timeouts so a stalled server can't
    /// hang a feed.
    pub fn new() -> Option<Http> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent(crate::s3::USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .ok()?;
        Some(Http {
            client,
            cache: HashMap::new(),
        })
    }

    /// A conditional GET (`If-None-Match` / `If-Modified-Since`).
    pub async fn get(&mut self, url: &str, accept: &str) -> Result<Got, String> {
        let mut req = self.client.get(url).header("Accept", accept);
        if let Some(c) = self.cache.get(url) {
            if let Some(t) = &c.etag {
                req = req.header("If-None-Match", t);
            }
            if let Some(m) = &c.modified {
                req = req.header("If-Modified-Since", m);
            }
        }
        let r = req.send().await.map_err(|e| e.to_string())?;
        if r.status() == reqwest::StatusCode::NOT_MODIFIED
            && let Some(c) = self.cache.get(url)
        {
            return Ok(Got {
                body: c.body.clone(),
                changed: false,
            });
        }
        if !r.status().is_success() {
            return Err(format!("HTTP {}", r.status().as_u16()));
        }
        let header = |k: &str| {
            r.headers()
                .get(k)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let (etag, modified) = (header("etag"), header("last-modified"));
        let body = Arc::new(r.bytes().await.map_err(|e| e.to_string())?.to_vec());
        let changed = self.cache.get(url).is_none_or(|c| c.body != body);
        self.cache.insert(
            url.to_string(),
            Cached {
                etag,
                modified,
                body: body.clone(),
            },
        );
        Ok(Got { body, changed })
    }

    /// Forgets URLs not in `keep`, so per-item URLs (one per watch, say)
    /// don't pile up over a long session.
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.cache.retain(|k, _| keep(k));
    }
}
