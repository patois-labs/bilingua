//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-ftl --release --test speed -- --nocapture
//! Set BILINGUA_FTL_SPEED_OUT to a path to keep the generated file (tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_ftl::{FtlFilter, Limits};
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("### Lumen Bakery generated\n\n-brand-name = Lumen Bakery\n    .gender = feminine\n\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        match i % 5 {
            0 => s.push_str(&format!("# Note for entry {i}\nhello-{i} = Welcome back, {{ $name }}, to {{ -brand-name }} number {i}\n    .title = Click to log in {i}\n    .accesskey = L\n\n")),
            1 => s.push_str(&format!("loaves-{i} = {{ $count ->\n    [one] {{ $count }} loaf was baked ({i}).\n   *[other] {{ $count }} loaves were baked ({i}).\n}}\n\n")),
            2 => s.push_str(&format!("multi-{i} =\n    First line of the text {i}\n      indented line with {{ \"[\" }} bracket\n    last line {{ NUMBER($n, minimumFractionDigits: 2) }}\n\n")),
            3 => s.push_str(&format!("jp-{i} = \u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {{ $v }} {i}\n\n")),
            _ => s.push_str(&format!("plain-{i} = A plain sentence of ordinary words for entry number {i}.\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_FTL_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let limits = Limits { max_units: 10_000_000, ..Limits::default() };
    let t0 = Instant::now();
    let mut doc = FtlFilter.extract_with(text.as_bytes(), &ExtractOptions::default(), &limits).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} "))];
        t.extend(u.segments[0].source.iter().filter(|x| matches!(x, Inline::Tag { .. })).cloned());
        u.segments[0].target = t;
    }
    let out = FtlFilter.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    // an untouched merge gives the exact bytes
    let untouched = FtlFilter.merge(&FtlFilter.extract_with(text.as_bytes(), &ExtractOptions::default(), &limits).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "ftl speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}
