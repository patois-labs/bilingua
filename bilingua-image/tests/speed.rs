//! Speed: one generated SVG of at least 5 MB (about 40 000 text elements with tspans, entities, a few languages),
//! extracted and merged with every unit translated. Run in release for the real number:
//!   cargo test -p bilingua-image --release --test speed -- --nocapture
//! `tests/tools/speed_locos.py` times the LocOS filter on the same file (set BILINGUA_IMG_SPEED_OUT to keep it).
mod common;
use bilingua_image::SvgFilter;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let mut s = String::with_capacity(5_600_000);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4000\" height=\"4000\" viewBox=\"0 0 4000 4000\">\n<defs><linearGradient id=\"g\"><stop offset=\"0\" stop-color=\"#347\"/></linearGradient></defs>\n<style>.t { font-family: sans-serif; font-size: 12px; }</style>\n");
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        s.push_str(&format!(
            "  <g transform=\"translate({} {})\"><rect x=\"0\" y=\"0\" width=\"80\" height=\"20\" fill=\"url(#g)\"/><text class=\"t\" x=\"4\" y=\"14\" text-anchor=\"start\">Label {i} &amp; caf\u{e9} \u{65e5}\u{672c}\u{8a9e} <tspan font-weight=\"bold\">bold {i}</tspan> tail</text><!-- note {i} --></g>\n",
            i % 50 * 80,
            i / 50 % 190 * 20
        ));
        i += 1;
    }
    s.push_str("<title>Big chart</title><desc>Generated for the speed test</desc>\n</svg>\n");
    s.into_bytes()
}

#[test]
fn large_file() {
    let data = generate();
    assert!(data.len() >= 5_000_000, "{}", data.len());
    let keep = std::env::var("BILINGUA_IMG_SPEED_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("bilingua-image-speed.svg"));
    std::fs::write(keep, &data).unwrap();
    let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000).with_max_xml_elements(10_000_000);
    let o = ExtractOptions::new().with_limits(limits).with_target_lang("ar-SA");
    let t0 = Instant::now();
    let mut doc = SvgFilter.extract(&data, &o).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 60_000, "{n}");
    let t1 = Instant::now();
    assert_eq!(SvgFilter.merge(&doc).unwrap(), data);
    let t_identity = t1.elapsed();
    let t2 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("\u{645}\u{631}\u{62d}\u{628}\u{627} {i} & x"))];
    }
    let out = SvgFilter.merge_with(&doc, &locos_opts()).unwrap();
    let t_merge = t2.elapsed();
    assert!(out.len() > 4_000_000);
    let back = SvgFilter.extract(&out, &o).unwrap();
    assert_eq!(back.units.len(), n);
    let line = format!(
        "svg speed: {:.1} MB, {n} units, extract {:.0} ms, untouched merge {:.0} ms, translate+merge (Arabic, rtl on every text) {:.0} ms, extract+merge {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_identity.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    println!("{line}");
    // `cargo test` hides the output of a passing test: keep the line where a script can read it
    if let Ok(p) = std::env::var("BILINGUA_IMG_SPEED_LOG") {
        std::fs::write(p, &line).unwrap();
    } else {
        let _ = std::fs::write(std::env::temp_dir().join("bilingua-image-speed.txt"), &line);
    }
}
