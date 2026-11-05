//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated into the target
//! column. Run in release for the real number:
//!   cargo test -p bilingua-csv --release --test speed -- --nocapture
//! Set BILINGUA_CSV_SPEED_OUT to a path to keep the generated file (tools/speed_locos.py reads it).
mod common;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("key,source,target,context\r\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        match i % 5 {
            0 => s.push_str(&format!("btn.{i},\"Click \"\"Save\"\" to continue, number {i}\",,button\r\n")),
            1 => s.push_str(&format!("msg.{i},Welcome back, friend {i}.,,home\r\n")),
            2 => s.push_str(&format!("qty.{i},\"%d items in %s ({i})\nsecond line\",,basket\r\n")),
            3 => s.push_str(&format!("jp.{i},\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {i},,\r\n")),
            _ => s.push_str(&format!("num.{i},12.50,,\r\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_CSV_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let limits = Limits::new().with_max_units(10_000_000);
    let opts = ExtractOptions::new().with_source_lang("en").with_target_lang("de").with_limits(limits);
    let c = Call::csv().langs("en", &["de"], Some("de"));
    let t0 = Instant::now();
    let mut doc = c.extract_with(text.as_bytes(), &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("T{i}, \"q\" translated"))];
    }
    let out = c.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    // an untouched merge gives the exact bytes
    let untouched = c.merge(&c.extract_with(text.as_bytes(), &opts).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "csv speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}
