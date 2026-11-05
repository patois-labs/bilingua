//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-ass --release --test speed -- --nocapture
//! Set BILINGUA_ASS_SPEED_OUT to a path to keep the generated file (tools/speed_locos.py reads it).
mod common;
use bilingua_ass::AssFilter;
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("[Script Info]\nTitle: Lumen Bakery generated\nScriptType: v4.00+\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize\nStyle: Default,Arial,48\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        let t = format!("0:{:02}:{:02}.00", (i / 60) % 60, i % 60);
        match i % 5 {
            0 => s.push_str(&format!("Dialogue: 0,{t},{t},Default,Anna,0,0,0,,Hello, world, how are you number {i}?\n")),
            1 => s.push_str(&format!("Dialogue: 0,{t},{t},Default,,0,0,0,,{{\\an8}}I said {{\\i1}}no{{\\i0}} to {i}.\\NNever again.\n")),
            2 => s.push_str(&format!("Comment: 0,{t},{t},Default,,0,0,0,,A note for the editor {i}\n")),
            3 => s.push_str(&format!("Dialogue: 0,{t},{t},Default,,0,0,0,,\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{5b57}\u{5e55} {{\\b1}}{i}{{\\b0}}\\h\u{3067}\u{3059}\n")),
            _ => s.push_str(&format!("Dialogue: 0,{t},{t},Default,,0,0,0,,{{\\p1}}m 0 0 l {i} 0 {i} {i}{{\\p0}}\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_ASS_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let opts = ExtractOptions { limits: Some(Limits { max_units: 10_000_000, max_lines: 10_000_000, ..Limits::default() }), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = AssFilter.extract(text.as_bytes(), &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    let items: Vec<(String, Vec<Inline>)> = doc
        .units
        .iter()
        .map(|u| {
            let t = u
                .source()
                .into_iter()
                .map(|i| match i {
                    Inline::Text(t) => Inline::Text(format!("T {t}")),
                    x => x,
                })
                .collect();
            (u.segments[0].id.clone(), t)
        })
        .collect();
    doc.set_targets(items).unwrap();
    let out = AssFilter.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    // an untouched merge gives the exact bytes
    let untouched = AssFilter.merge(&AssFilter.extract(text.as_bytes(), &opts).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "ass speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}
