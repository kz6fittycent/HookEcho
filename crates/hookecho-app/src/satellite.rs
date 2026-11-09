//! GOES satellite imagery under the radar (hookecho-core::goes; drawn by
//! hookecho-render's satellite pass): which image is shown, and its line in
//! the Layers sheet.

use crate::model::Model;
use hookecho_core::goes::{Band, Image};

/// The image to draw: the newest of the chosen band, if it has arrived.
pub fn shown(model: &Model) -> Option<&Image> {
    let band = Band::from_key(model.config.satellite.as_deref()?)?;
    model
        .layers
        .goes
        .as_ref()
        .as_ref()
        .filter(|i| i.band == band)
}

/// "Infrared · GOES-East · 10:26 AM", or what's happening instead.
pub fn status(model: &Model) -> String {
    let Some(band) = model.config.satellite.as_deref().and_then(Band::from_key) else {
        return "GOES-East imagery under the radar".into();
    };
    match shown(model) {
        Some(i) => format!(
            "{} · GOES-East · {}",
            band.name(),
            crate::model::clock_time(i.time_ms, model.config.utc)
        ),
        None if model.layers.goes_trouble.is_some() => {
            "Can't reach the GOES bucket right now; retrying".into()
        }
        None => format!("Loading {}…", band.name().to_lowercase()),
    }
}
