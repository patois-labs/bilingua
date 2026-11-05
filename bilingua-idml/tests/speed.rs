//! Speed: one large generated package (at least 5 MB of text). Run in release:
//!
//! `cargo test --release -p bilingua-idml --test speed -- --ignored --nocapture`
//!
//! `IDML_SPEED_OUT=path` also writes the package, so the LocOS filter can be timed on the same file.

mod common;
use bilingua_idml::Idml;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn big_package(paragraphs: usize) -> (Vec<u8>, usize) {
    let words = ["translation", "localization", "layout", "paragraph", "InDesign", "brochure", "catalogue", "headline"];
    let mut body = String::new();
    let mut text_bytes = 0usize;
    for i in 0..paragraphs {
        let mut a = String::new();
        for k in 0..12 {
            a.push_str(words[(i + k) % words.len()]);
            a.push(' ');
        }
        let b = format!("{} bold {}", words[i % 8], i);
        let c = format!(" and more text after the bold part of paragraph number {i}, to make the paragraph longer.");
        text_bytes += a.len() + b.len() + c.len();
        let link = if i % 7 == 0 { "<HyperlinkTextSource Self=\"hl\"><Content>a link</Content></HyperlinkTextSource>" } else { "" };
        body.push_str(&psr(&format!(
            "{}{}{}",
            csr(&format!("<Content>{a}</Content>{link}")),
            csr_b(&format!("<Content>{b}</Content>"), BOLD),
            csr(&format!("<Content>{c}</Content>"))
        )));
        body.push('\n');
    }
    (Pkg::one(body).build(), text_bytes)
}

#[test]
#[ignore = "timing run: release build, prints the numbers"]
fn large_package_speed() {
    let (data, text_bytes) = big_package(26_000);
    assert!(text_bytes >= 5 * 1024 * 1024, "{text_bytes}");
    if let Ok(p) = std::env::var("IDML_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_units(100_000)).with_target_lang("de-DE");
    let t0 = Instant::now();
    let mut doc = Idml.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let t1 = Instant::now();
    let same = Idml.merge(&doc).unwrap();
    let t_untouched = t1.elapsed();
    assert_eq!(same, data);
    for u in &mut doc.units {
        let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(t) = i { Inline::text(t.to_uppercase()) } else { i.clone() }).collect();
        u.segments[0].target = t;
    }
    let t2 = Instant::now();
    let out = Idml.merge(&doc).unwrap();
    let t_merge = t2.elapsed();
    let again = Idml.extract(&out, &opts).unwrap();
    assert_eq!(again.units.len(), doc.units.len());
    println!(
        "IDML SPEED: {} bytes zip, {} bytes text, {} units | extract {:?} | untouched merge {:?} | translated merge {:?}",
        data.len(),
        text_bytes,
        doc.units.len(),
        t_extract,
        t_untouched,
        t_merge
    );
}
