//! Speed: one large generated package (at least 5 MB of text). Run in release:
//!
//! `cargo test --release -p bilingua-xliffpkg --test speed -- --ignored --nocapture`
//!
//! `XLIFFPKG_SPEED_OUT=path` also writes the package, so the LocOS filter can be timed on the same file.

mod common;
use bilingua_model::{ExtractOptions, Inline, Limits};
use common::*;
use std::time::Instant;

fn big_package(units: usize) -> (Vec<u8>, usize) {
    let words = ["translation", "localization", "package", "segment", "workflow", "glossary", "terminology", "review"];
    let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file original=\"big.html\" source-language=\"en-US\" target-language=\"de-DE\" datatype=\"html\"><body>\n");
    let mut text_bytes = 0;
    for i in 0..units {
        let mut s = String::new();
        for k in 0..22 {
            s.push_str(words[(i + k) % words.len()]);
            s.push(' ');
        }
        let t = format!("Press <g id=\"1\">{}</g> number {i} and see {}.", words[i % 8], s.trim_end());
        text_bytes += t.len();
        x.push_str(&format!("<trans-unit id=\"{i}\"><source>{t}</source><target></target></trans-unit>\n"));
    }
    x.push_str("</body></file></xliff>\n");
    let pad = vec![b'x'; 200_000];
    (zip_of(&[("tm/pad.tmx", &pad, true), ("big.xlf", x.as_bytes(), true)], b""), text_bytes)
}

#[test]
#[ignore = "timing run: release build, prints the numbers"]
fn large_package_speed() {
    let (data, text_bytes) = big_package(30_000);
    assert!(text_bytes >= 5 * 1024 * 1024, "{text_bytes}");
    if let Ok(p) = std::env::var("XLIFFPKG_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let f = filter();
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_units(100_000)).with_target_lang("de-DE");
    let t0 = Instant::now();
    let mut doc = f.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(doc.segment_count(), 30_000);
    let t1 = Instant::now();
    assert_eq!(f.merge(&doc).unwrap(), data);
    let t_untouched = t1.elapsed();
    for u in &mut doc.units {
        let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(t) = i { Inline::text(t.to_uppercase()) } else { i.clone() }).collect();
        u.segments[0].target = t;
        u.segments[0].state = bilingua_model::SegState::Translated;
    }
    let t2 = Instant::now();
    let out = f.merge(&doc).unwrap();
    let t_merge = t2.elapsed();
    let back = f.extract(&out, &opts).unwrap();
    assert_eq!(back.segment_count(), 30_000);
    assert!(!back.units[7].segments[0].target.is_empty());
    println!("package {} bytes, text {} bytes: extract {:?}, untouched merge {:?}, translated merge {:?}", data.len(), text_bytes, t_extract, t_untouched, t_merge);
}
