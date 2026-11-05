//! One large generated XMB (over 5 MB): extract, untouched merge, XTB conversion. Run
//! `cargo test --release -p bilingua-xmb --test speed -- --nocapture` for the timings. Set `XMB_SPEED_OUT` to a path
//! to also save the file, so the LocOS filter can be timed on the same bytes.

mod common;
use bilingua_model::Filter;
use bilingua_xmb::Xmb;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let mut s = String::with_capacity(6_500_000);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<messagebundle handler=\"angular\">\n");
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    for n in 0..40_000usize {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        s.push_str(&format!(
            "  <msg id=\"{}\" desc=\"Entry {n}\"><source>src/app/page{n}.html:{}</source>Open the {} <ph name=\"START_BOLD_TEXT\"><ex>&lt;b&gt;</ex>&lt;b&gt;</ph>{}<ph name=\"CLOSE_BOLD_TEXT\"><ex>&lt;/b&gt;</ex>&lt;/b&gt;</ph> near the {} at <ph name=\"INTERPOLATION\"><ex>{{{{ x }}}}</ex>{{{{ x }}}}</ph> &amp; bring {} for caf\u{e9} number {n} of the guide.</msg>\n",
            1_000_000_000_000_000_000u64 + n as u64 * 7919,
            n % 90 + 1,
            w(1),
            w(2),
            w(3),
            w(4)
        ));
    }
    s.push_str("</messagebundle>\n");
    s.into_bytes()
}

#[test]
fn large_file_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("XMB_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let t0 = Instant::now();
    let mut d = Xmb.extract(&bytes, &opts()).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(d.units.len(), 40_000);
    let t1 = Instant::now();
    let same = Xmb.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    for u in d.units.iter_mut() {
        let mut t = vec![bilingua_model::Inline::text("FR ")];
        t.extend(u.segments[0].source.clone());
        u.segments[0].target = t;
    }
    let t2 = Instant::now();
    let out = Xmb.merge(&d).unwrap();
    let t_xtb = t2.elapsed();
    let back = bilingua_xmb::Xtb.extract(&out, &opts()).unwrap();
    assert_eq!(back.units.len(), 40_000);
    println!(
        "xmb speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, XTB merge {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_xtb,
        t_extract + t_merge
    );
}
