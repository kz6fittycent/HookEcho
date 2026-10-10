//! Real-time chunk assembly on real KTLX chunks (tests/data/realtime/):
//! out-of-order arrival, progressive sweeps, and equality with decoding the
//! raw concatenated chunks directly.

use hookecho_core::Moment;
use hookecho_core::realtime::{ChunkKey, LiveVolume};

fn chunks() -> Vec<(ChunkKey, Vec<u8>)> {
    let dir = format!(
        "{}/../../tests/data/realtime/KTLX-276",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut v: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            let key = ChunkKey::parse(&format!("KTLX/276/{name}")).unwrap();
            let data = std::fs::read(format!("{dir}/{name}")).unwrap();
            (key, data)
        })
        .collect();
    v.sort_by_key(|(k, _)| k.seq);
    v
}

#[test]
fn assembles_progressively_and_matches_direct_decode() {
    let all = chunks();
    assert_eq!(all.len(), 8);
    let mut live = LiveVolume::for_key(&all[0].0);
    // Start chunk only: metadata, no radials yet.
    assert!(live.push(&all[0].0, all[0].1.clone()).unwrap());
    // Chunk 3 before 2: held back until the gap fills.
    assert!(!live.push(&all[2].0, all[2].1.clone()).unwrap());
    assert_eq!(live.chunks(), 1);
    assert!(live.push(&all[1].0, all[1].1.clone()).unwrap());
    assert_eq!(live.chunks(), 3);
    // Duplicates are ignored.
    assert!(!live.push(&all[1].0, all[1].1.clone()).unwrap());
    for (k, d) in &all[3..] {
        live.push(k, d.clone()).unwrap();
    }
    assert_eq!(live.chunks(), 8);
    assert!(!live.is_complete());

    let v = live.decode().unwrap();
    assert_eq!(v.vcp, 35);
    assert_eq!(v.sweeps.len(), 2);
    assert!(
        v.sweeps[0].complete && v.sweeps[0].radials.len() == 720,
        "first sweep done"
    );
    assert!(!v.sweeps[1].complete, "second sweep still arriving");
    assert!(v.sweeps[1].field(Moment::Velocity).is_some());

    // Identical to decoding the raw (bzip2) concatenation the classic way.
    let raw: Vec<u8> = all.iter().flat_map(|(_, d)| d.clone()).collect();
    let direct = hookecho_core::level2::decode_volume(&raw).unwrap();
    assert_eq!(direct.sweeps.len(), v.sweeps.len());
    for (a, b) in direct.sweeps.iter().zip(&v.sweeps) {
        assert_eq!(a.radials.len(), b.radials.len());
        for ((ma, fa), (mb, fb)) in a.moments.iter().zip(&b.moments) {
            assert_eq!(ma, mb);
            assert_eq!(fa.raw, fb.raw, "{ma:?} gates differ");
        }
    }
}

#[test]
fn rejects_chunks_from_another_volume() {
    let all = chunks();
    let mut live = LiveVolume::new("KTLX", 277, "20261008-140902");
    assert!(live.push(&all[0].0, all[0].1.clone()).is_err());
}
