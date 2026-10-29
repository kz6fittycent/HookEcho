//! The data-layer feeds (hookecho-core::poll): started with the live app,
//! switched on and off with their layers, and drained into the model once
//! per frame.

use crate::model::Model;
use hookecho_core::alerts::{Alert, AlertSource};
use hookecho_core::goes::{self, GoesSource, Image};
use hookecho_core::obs::{Obs, ObsSource, Wanted};
use hookecho_core::poll::Feed;
use hookecho_core::space::{SpaceSource, SpaceWeather};
use hookecho_core::spc::{Spc, SpcSource};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Feeds {
    alerts: Feed<Vec<Alert>>,
    spc: Feed<Spc>,
    obs: Feed<Obs>,
    /// The station the panel shows, for its history.
    wanted: Wanted,
    space: Feed<SpaceWeather>,
    /// Whether the space feed fetches the aurora grid.
    aurora: Arc<AtomicBool>,
    goes: Feed<Option<Image>>,
    band: goes::Wanted,
}

impl Feeds {
    pub fn start(cache: PathBuf, wake: Wake) -> Feeds {
        let wanted = Wanted::default();
        let aurora = Arc::new(AtomicBool::new(false));
        let band = goes::Wanted::default();
        Feeds {
            alerts: Feed::start(AlertSource::default(), cache.clone(), wake.clone()),
            spc: Feed::start(SpcSource::new(), cache.clone(), wake.clone()),
            obs: Feed::start(ObsSource::new(wanted.clone()), cache.clone(), wake.clone()),
            wanted,
            space: Feed::start(
                SpaceSource::new(aurora.clone()),
                cache.clone(),
                wake.clone(),
            ),
            aurora,
            goes: Feed::start(GoesSource::new(band.clone()), cache, wake),
            band,
        }
    }

    /// Polls each feed only while a layer that needs it is on.
    pub fn update(&self, model: &mut Model) {
        let c = &model.config;
        self.alerts.enable(c.warnings);
        self.spc
            .enable(c.watches || c.discussions || c.outlook.is_some());
        self.obs.enable(c.station_plots || c.station_panel);
        let want = c
            .station_panel
            .then(|| crate::stations::panel_station(model).map(|o| o.id.clone()))
            .flatten();
        if let Ok(mut w) = self.wanted.lock()
            && *w != want
        {
            *w = want;
            self.obs.poll_now();
        }
        // Space weather while its card is open or the aurora is shown; a
        // poll right away when the aurora is switched on.
        let c = &model.config;
        let card = matches!(model.picker, crate::model::Picker::SpaceWeather { .. })
            || crate::dock::wants_dock(model, crate::dock::Panel::Space);
        self.space.enable(c.aurora || card);
        if self.aurora.swap(c.aurora, Ordering::Relaxed) != c.aurora && c.aurora {
            self.space.poll_now();
        }
        // Satellite: the chosen band, fetched at once when it changes.
        let want = model
            .config
            .satellite
            .as_deref()
            .and_then(hookecho_core::goes::Band::from_key);
        self.goes.enable(want.is_some());
        if let Ok(mut w) = self.band.lock()
            && *w != want
        {
            *w = want;
            self.goes.poll_now();
        }
        let d = &mut model.layers;
        take(&self.alerts, &mut d.alerts, &mut d.alerts_trouble);
        take(&self.spc, &mut d.spc, &mut d.spc_trouble);
        take(&self.obs, &mut d.obs, &mut d.obs_trouble);
        take(&self.space, &mut d.space, &mut d.space_trouble);
        take(&self.goes, &mut d.goes, &mut d.goes_trouble);
    }
}

fn take<T: Send + Sync + 'static>(feed: &Feed<T>, data: &mut Arc<T>, trouble: &mut Option<String>) {
    let (d, t) = feed.drain();
    if let Some(d) = d {
        *data = d;
    }
    if let Some(t) = t {
        *trouble = t;
    }
}
