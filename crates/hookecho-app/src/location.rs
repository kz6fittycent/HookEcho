//! "Use my location" (docs/LOCATION.md, option A): one request at street
//! accuracy to the XDP Location portal (org.freedesktop.portal.Location),
//! which asks the desktop's GeoClue and shows the desktop's own permission
//! prompt. No background tracking; nothing is asked at startup.

use std::sync::mpsc;
use std::time::Duration;

/// Gives up on a portal that never answers (no location service running,
/// or the prompt left open).
const TIMEOUT: Duration = Duration::from_secs(45);

/// Starts one location request; the answer arrives on the channel as
/// (lat, lon) or a sentence saying why not.
pub fn locate(wake: crate::feeds::Wake) -> mpsc::Receiver<Result<(f64, f64), String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("location".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let r = match rt {
                Ok(rt) => rt.block_on(async {
                    tokio::time::timeout(TIMEOUT, ask())
                        .await
                        .unwrap_or_else(|_| Err("No answer from the location service".into()))
                }),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(r);
            wake();
        })
        .expect("spawn location");
    rx
}

async fn ask() -> Result<(f64, f64), String> {
    use ashpd::desktop::location::{Accuracy, CreateSessionOptions, LocationProxy};
    use futures_util::{FutureExt, StreamExt};
    let proxy = LocationProxy::new().await.map_err(why)?;
    let session = proxy
        .create_session(CreateSessionOptions::default().set_accuracy(Accuracy::Street))
        .await
        .map_err(why)?;
    let mut updates = proxy.receive_location_updated().await.map_err(why)?;
    let (started, first) = futures_util::join!(
        proxy.start(&session, None, Default::default()),
        updates
            .next()
            .map(|l| l.ok_or("The location service stopped")),
    );
    let _ = session.close().await;
    started.map_err(why)?;
    let l = first?;
    Ok((l.latitude(), l.longitude()))
}

/// A portal error in words a person can act on.
fn why(e: ashpd::Error) -> String {
    use ashpd::desktop::ResponseError;
    match &e {
        ashpd::Error::Response(ResponseError::Cancelled) => {
            "Location access wasn't allowed".to_string()
        }
        ashpd::Error::PortalNotFound(_) => "No location service on this computer".to_string(),
        _ => {
            let s = e.to_string();
            if s.contains("disabled") || s.contains("Location services") {
                "Location is turned off in Settings".to_string()
            } else if s.contains("ServiceUnknown") || s.contains("UnknownMethod") {
                "No location service on this computer".to_string()
            } else {
                format!("Couldn't get your location ({s})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Against this desktop's portal (may show its permission prompt):
    /// `cargo test -p hookecho-app -- --ignored live_locate --nocapture`.
    #[test]
    #[ignore]
    fn live_locate() {
        let rx = super::locate(std::sync::Arc::new(|| {}));
        let r = rx.recv().unwrap();
        println!("{r:?}");
        if let Err(why) = r {
            assert!(!why.is_empty());
        }
    }
}
