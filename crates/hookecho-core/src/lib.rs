//! Hook Echo core: decoding, sites, geometry, config. No UI, no GPU.
#![forbid(unsafe_code)]

pub mod alerts;
pub mod basemap;
pub mod cache;
pub mod config;
pub mod frames;
pub mod geo;
pub mod level2;
pub mod level3;
pub mod live;
pub mod moment;
pub mod obs;
pub mod pal;
pub mod paths;
pub mod poll;
pub mod realtime;
pub mod s3;
pub mod sites;
pub mod spc;
pub mod tdwr;
pub mod time;

pub use moment::{Moment, RawGates};
