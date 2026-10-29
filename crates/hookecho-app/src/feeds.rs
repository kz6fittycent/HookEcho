//! The data-layer feeds (hookecho-core::poll): started with the live app,
//! switched on and off with their layers, and drained into the model once
//! per frame.

use crate::model::Model;
use hookecho_core::alerts::{Alert, AlertSource};
use hookecho_core::poll::Feed;
use hookecho_core::spc::{Spc, SpcSource};
use std::path::PathBuf;
use std::sync::Arc;

pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub struct Feeds {
    alerts: Feed<Vec<Alert>>,
    spc: Feed<Spc>,
}

impl Feeds {
    pub fn start(cache: PathBuf, wake: Wake) -> Feeds {
        Feeds {
            alerts: Feed::start(AlertSource::default(), cache.clone(), wake.clone()),
            spc: Feed::start(SpcSource::new(), cache, wake),
        }
    }

    /// Polls each feed only while a layer that needs it is on.
    pub fn update(&self, model: &mut Model) {
        let c = &model.config;
        self.alerts.enable(c.warnings);
        self.spc
            .enable(c.watches || c.discussions || c.outlook.is_some());
        let d = &mut model.layers;
        take(&self.alerts, &mut d.alerts, &mut d.alerts_trouble);
        take(&self.spc, &mut d.spc, &mut d.spc_trouble);
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
