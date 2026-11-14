//! One large generated file (over 5 MB: 40 000 subtitles), extracted and merged. Run
//! `cargo test --release -p bilingua-stl --test speed -- --nocapture` for the timings. Set `STL_SPEED_OUT` to a path
//! to also save the file, so the LocOS filter can be timed on the same bytes (`tools/speed_locos.py`).

mod common;
use bilingua_model::*;
use bilingua_stl::Stl;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let n = 40_000usize;
    let mut v = gsi_default(n);
    for i in 0..n {
        let w = |k: usize| words[(i * 7 + k) % words.len()];
        let mut tf = Vec::new();
        tf.extend_from_slice(DH);
        tf.extend_from_slice(format!("Open the {} {} by {}", w(1), w(2), w(3)).as_bytes());
        tf.extend_from_slice(END);
        tf.extend_from_slice(b"\x8a\x8a");
        tf.extend_from_slice(DH);
        tf.extend_from_slice(format!("bring {} to {} \x01{i}\x07", w(4), w(5)).as_bytes());
        if i % 5 == 0 {
            tf.extend_from_slice(b" caf\xc2e");
        }
        tf.extend_from_slice(END);
        v.extend(tti(&Tti { sn: (i % 65535) as u16 + 1, ..Default::default() }, &tf));
    }
    v
}

#[test]
fn large_file_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("STL_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let t0 = Instant::now();
    let mut d = Stl.extract(&bytes, &opts(Some("fr"))).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(d.units.len(), 40_000);
    let t1 = Instant::now();
    let same = Stl.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    // Translate everything (the Latin accent and codes included) and merge again.
    pseudo(&mut d);
    let t2 = Instant::now();
    let out = Stl.merge(&d).unwrap();
    let t_merge_tr = t2.elapsed();
    let back = Stl.extract(&out, &opts(None)).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    println!(
        "stl speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, translated merge {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_merge_tr,
        t_extract + t_merge_tr
    );
}
