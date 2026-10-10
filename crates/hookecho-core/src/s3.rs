//! Anonymous, read-only access to NOAA's public S3 buckets (KICKOFF §3):
//! ListObjectsV2 and GET over HTTPS, with retries and backoff. No AWS SDK,
//! no credentials.

use std::time::Duration;

pub const CHUNKS_BUCKET: &str = "https://unidata-nexrad-level2-chunks.s3.amazonaws.com";

pub const USER_AGENT: &str = concat!(
    "HookEcho/",
    env!("CARGO_PKG_VERSION"),
    " (weather radar viewer; +https://github.com/kz6fittycent/HookEcho)"
);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network: {0}")]
    Network(String),
    #[error("HTTP {0}")]
    Status(u16),
    #[error("not found")]
    NotFound,
    #[error("bad listing: {0}")]
    Parse(String),
}

impl Error {
    /// Worth retrying (timeouts, connection failures, 5xx, throttling).
    pub fn transient(&self) -> bool {
        match self {
            Error::Network(_) => true,
            Error::Status(s) => *s >= 500 || *s == 429,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listing {
    /// (key, LastModified as returned).
    pub keys: Vec<(String, String)>,
    pub prefixes: Vec<String>,
    pub next_token: Option<String>,
}

#[derive(Clone)]
pub struct Bucket {
    client: reqwest::Client,
    base: String,
}

impl Bucket {
    pub fn new(base: &str) -> Result<Bucket, Error> {
        // rustls with the ring provider only (deny.toml bans aws-lc-sys).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok(Bucket {
            client,
            base: base.trim_end_matches('/').to_string(),
        })
    }

    /// One ListObjectsV2 page.
    pub async fn list_page(
        &self,
        prefix: &str,
        delimiter: Option<&str>,
        start_after: Option<&str>,
        max_keys: u32,
        token: Option<&str>,
    ) -> Result<Listing, Error> {
        let mut q: Vec<(&str, String)> = vec![
            ("list-type", "2".into()),
            ("prefix", prefix.into()),
            ("max-keys", max_keys.to_string()),
        ];
        if let Some(d) = delimiter {
            q.push(("delimiter", d.into()));
        }
        if let Some(s) = start_after {
            q.push(("start-after", s.into()));
        }
        if let Some(t) = token {
            q.push(("continuation-token", t.into()));
        }
        let body = self
            .retry(|| async { self.text(&format!("{}/", self.base), &q).await })
            .await?;
        parse_listing(&body)
    }

    /// All pages of a listing (capped at `limit` entries).
    pub async fn list(
        &self,
        prefix: &str,
        delimiter: Option<&str>,
        start_after: Option<&str>,
        limit: usize,
    ) -> Result<Listing, Error> {
        let mut out = Listing::default();
        let mut token: Option<String> = None;
        loop {
            let page = self
                .list_page(prefix, delimiter, start_after, 1000, token.as_deref())
                .await?;
            out.keys.extend(page.keys);
            out.prefixes.extend(page.prefixes);
            token = page.next_token;
            if token.is_none() || out.keys.len() + out.prefixes.len() >= limit {
                return Ok(out);
            }
        }
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, Error> {
        let url = format!("{}/{}", self.base, key);
        self.retry(|| async {
            let resp = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| Error::Network(e.to_string()))?;
            match resp.status().as_u16() {
                200 => resp
                    .bytes()
                    .await
                    .map(|b| b.to_vec())
                    .map_err(|e| Error::Network(e.to_string())),
                404 | 403 => Err(Error::NotFound),
                s => Err(Error::Status(s)),
            }
        })
        .await
    }

    async fn text(&self, url: &str, q: &[(&str, String)]) -> Result<String, Error> {
        let query: Vec<String> = q
            .iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect();
        let full = format!("{url}?{}", query.join("&"));
        let resp = self
            .client
            .get(full)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp.text().await.map_err(|e| Error::Network(e.to_string())),
            404 => Err(Error::NotFound),
            s => Err(Error::Status(s)),
        }
    }

    /// Up to 3 attempts on transient errors, 0.5 s then 1.5 s apart.
    async fn retry<T, F, Fut>(&self, f: F) -> Result<T, Error>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>>,
    {
        let mut delay = Duration::from_millis(500);
        for attempt in 1..=3 {
            match f().await {
                Err(e) if e.transient() && attempt < 3 => {
                    tokio::time::sleep(delay).await;
                    delay *= 3;
                }
                r => return r,
            }
        }
        unreachable!()
    }
}

/// Minimal ListObjectsV2 XML reader: `<Contents>` keys with LastModified,
/// `<CommonPrefixes>` prefixes, and the continuation token.
pub fn parse_listing(xml: &str) -> Result<Listing, Error> {
    if !xml.contains("<ListBucketResult") {
        return Err(Error::Parse(xml.chars().take(120).collect()));
    }
    let mut out = Listing::default();
    let mut rest = xml;
    while let Some(start) = rest.find("<Contents>") {
        let end = rest[start..]
            .find("</Contents>")
            .ok_or_else(|| Error::Parse("unterminated Contents".into()))?
            + start;
        let block = &rest[start..end];
        if let Some(k) = tag(block, "Key") {
            out.keys.push((
                unescape(k),
                tag(block, "LastModified").unwrap_or("").to_string(),
            ));
        }
        rest = &rest[end..];
    }
    let mut rest = xml;
    while let Some(start) = rest.find("<CommonPrefixes>") {
        let end = rest[start..]
            .find("</CommonPrefixes>")
            .ok_or_else(|| Error::Parse("unterminated CommonPrefixes".into()))?
            + start;
        if let Some(p) = tag(&rest[start..end], "Prefix") {
            out.prefixes.push(unescape(p));
        }
        rest = &rest[end..];
    }
    out.next_token = tag(xml, "NextContinuationToken").map(unescape);
    Ok(out)
}

/// RFC 3986 percent-encoding of a query value (unreserved chars pass).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn tag<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let a = s.find(&open)? + open.len();
    let b = s[a..].find(&format!("</{name}>"))? + a;
    Some(&s[a..b])
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Index of the newest entry in a ring sorted by number whose times
/// increase around the ring (a rotated sorted array): the element before
/// the single drop. `time(i)` is fetched lazily; O(log n) calls.
pub async fn newest_in_ring<F, Fut>(n: usize, time: F) -> Result<Option<usize>, Error>
where
    F: Fn(usize) -> Fut,
    Fut: std::future::Future<Output = Result<i64, Error>>,
{
    if n == 0 {
        return Ok(None);
    }
    let (mut lo, mut hi) = (0usize, n - 1);
    let t_lo = time(lo).await?;
    if t_lo <= time(hi).await? {
        return Ok(Some(hi)); // not rotated
    }
    // Invariant: the maximum is in [lo, hi] and t(lo) >= t_lo.
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if time(mid).await? >= t_lo {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    Ok(Some(lo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listings() {
        let xml = r#"<?xml version="1.0"?><ListBucketResult><IsTruncated>true</IsTruncated>
            <Contents><Key>KTLX/276/20261008-140202-001-S</Key><LastModified>2026-10-08T14:02:05.000Z</LastModified><Size>2382</Size></Contents>
            <Contents><Key>KTLX/276/20261008-140202-002-I</Key><LastModified>2026-10-08T14:02:17.000Z</LastModified></Contents>
            <CommonPrefixes><Prefix>KTLX/277/</Prefix></CommonPrefixes>
            <NextContinuationToken>abc&amp;def</NextContinuationToken></ListBucketResult>"#;
        let l = parse_listing(xml).unwrap();
        assert_eq!(l.keys.len(), 2);
        assert_eq!(l.keys[1].0, "KTLX/276/20261008-140202-002-I");
        assert_eq!(l.prefixes, vec!["KTLX/277/".to_string()]);
        assert_eq!(l.next_token.as_deref(), Some("abc&def"));
        assert!(parse_listing("<Error><Code>AccessDenied</Code></Error>").is_err());
        assert_eq!(encode("KTLX/276/"), "KTLX%2F276%2F");
        assert_eq!(encode("a+b/c="), "a%2Bb%2Fc%3D");
    }

    fn ring_newest(times: &[i64]) -> Option<usize> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(newest_in_ring(times.len(), |i| {
            let t = times[i];
            async move { Ok(t) }
        }))
        .unwrap()
    }

    #[test]
    fn finds_newest_in_rotated_ring() {
        assert_eq!(ring_newest(&[]), None);
        assert_eq!(ring_newest(&[5]), Some(0));
        assert_eq!(ring_newest(&[1, 2, 3, 4]), Some(3));
        assert_eq!(ring_newest(&[7, 8, 9, 1, 2]), Some(2));
        assert_eq!(ring_newest(&[9, 1, 2, 3]), Some(0));
        assert_eq!(ring_newest(&[2, 3, 4, 5, 1]), Some(3));
        // Exhaustive check over every rotation of a 50-element ring.
        let base: Vec<i64> = (0..50).collect();
        for r in 0..50 {
            let rot: Vec<i64> = base[r..].iter().chain(&base[..r]).copied().collect();
            assert_eq!(rot[ring_newest(&rot).unwrap()], 49, "rotation {r}");
        }
    }
}
