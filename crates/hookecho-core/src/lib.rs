//! Hook Echo core: decoding, sites, geometry, config. No UI, no GPU.
#![forbid(unsafe_code)]

pub mod basemap;
pub mod config;
pub mod geo;
pub mod level2;
pub mod moment;
pub mod paths;
pub mod realtime;
pub mod sites;

pub use moment::{Moment, RawGates};
