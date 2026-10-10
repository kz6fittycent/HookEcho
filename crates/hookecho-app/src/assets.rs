//! Everything bundled into the binary, so the app (and its snap) needs no
//! data files at runtime.

macro_rules! asset {
    ($p:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/", $p))
    };
}
macro_rules! asset_str {
    ($p:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/", $p))
    };
}
pub(crate) use asset_str;

pub const BASEMAP: &[u8] = asset!("basemap/basemap.bin");

/// KTLX 2013-05-20 20:16:43 UTC (Moore, OK). Public domain NOAA data.
pub const SAMPLE_VOLUME: &[u8] = asset!("bundled/KTLX20130520_201643_V06.gz");
pub const SAMPLE_SITE: &str = "KTLX";

pub const INTER_REGULAR: &[u8] = asset!("fonts/Inter-Regular.ttf");
pub const INTER_MEDIUM: &[u8] = asset!("fonts/Inter-Medium.ttf");
pub const INTER_SEMIBOLD: &[u8] = asset!("fonts/Inter-SemiBold.ttf");
pub const INTER_BOLD: &[u8] = asset!("fonts/Inter-Bold.ttf");
pub const INTER_TAB_REGULAR: &[u8] = asset!("fonts/InterTabular-Regular.ttf");
pub const INTER_TAB_MEDIUM: &[u8] = asset!("fonts/InterTabular-Medium.ttf");
pub const INTER_TAB_SEMIBOLD: &[u8] = asset!("fonts/InterTabular-SemiBold.ttf");

pub const ICON_PNG: &[u8] = asset!("icon/hookecho-256.png");
