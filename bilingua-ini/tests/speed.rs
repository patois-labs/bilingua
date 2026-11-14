//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-ini --release --test speed -- --nocapture
//! Set BILINGUA_INI_SPEED_OUT to a path to keep the generated file (the LocOS timing script reads it).
mod common;
use bilingua_ini::{IniFilter, Limits};
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("; Lumen Bakery generated\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        if i % 200 == 0 {
            s.push_str(&format!("\n[Section{}]\n", i / 200));
        }
        match i % 4 {
            0 => s.push_str(&format!("; Note for entry {i}\nKEY_{i}=\"Click \\\"Save\\\" to continue number {i}\"\n")),
            1 => s.push_str(&format!("key_{i} = Welcome back, friend {i}   \n")),
            2 => s.push_str(&format!("COUNT_{i}=\"%d items in %s ({i})\"\n")),
            _ => s.push_str(&format!("JP_{i}=\"\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {i}\"\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_INI_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let limits = { let mut built = Limits::default(); built.max_units = 10_000_000; built };
    let t0 = Instant::now();
    let mut doc = IniFilter.extract_with(text.as_bytes(), &ExtractOptions::default(), &limits).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("T{i} \"q\" translated"))];
    }
    let out = IniFilter.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    // an untouched merge gives the exact bytes
    let untouched = IniFilter.merge(&extract_plain(&text, &limits)).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "ini speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}

fn extract_plain(text: &str, limits: &Limits) -> Document {
    IniFilter.extract_with(text.as_bytes(), &ExtractOptions::default(), limits).unwrap()
}
