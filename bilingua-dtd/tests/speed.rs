//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-dtd --release --test speed -- --nocapture
//! Set BILINGUA_DTD_SPEED_OUT to a path to keep the generated file (tools/speed_locos.py reads it).
mod common;
use bilingua_dtd::{DtdFilter, Limits};
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("<!-- This Source Code Form is subject to the terms of the Mozilla Public\n   - License, v. 2.0. -->\n\n");
    s.push_str("<!ENTITY % brandDTD SYSTEM \"chrome://branding/locale/brand.dtd\">\n%brandDTD;\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        match i % 5 {
            0 => s.push_str(&format!("<!-- LOCALIZATION NOTE (k{i}.label): shown in the toolbar -->\n<!ENTITY k{i}.label \"Click &quot;Save&quot; to continue number {i}\">\n")),
            1 => s.push_str(&format!("<!ENTITY k{i}.title \"&brandShortName; Settings {i}\">\n")),
            2 => s.push_str(&format!("<!ENTITY k{i}.text 'Save &amp; close <html:b>now</html:b> 100&#037; ({i})'>\n")),
            3 => s.push_str(&format!("<!ENTITY k{i}.jp \"\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {i}\">\n")),
            _ => s.push_str(&format!("<!ENTITY k{i}.key \"Welcome back, friend {i}\"  >\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_DTD_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let limits = Limits::new().with_max_units(10_000_000);
    let opts = ExtractOptions::default();
    let t0 = Instant::now();
    let mut doc = DtdFilter.extract_with(text.as_bytes(), &opts, &limits).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("T{i} \"q\" translated"))];
    }
    let out = DtdFilter.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    let untouched = DtdFilter.merge(&DtdFilter.extract_with(text.as_bytes(), &opts, &limits).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "dtd speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}
