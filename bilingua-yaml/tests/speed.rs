//! Speed: one large generated Rails-style file, timed in release mode.
//!
//!     cargo test -p bilingua-yaml --release --test speed -- --ignored --nocapture
//!
//! Set `SPEED_OUT=path.yml` to keep the generated file (the LocOS timing in the README uses the same bytes).

mod common;
use bilingua_model::*;
use bilingua_yaml::{Limits, Yaml};
use common::*;
use std::time::Instant;

pub fn generate(entries: usize) -> String {
    let mut s = String::from("# generated\nen:\n");
    for i in 0..entries / 8 {
        s.push_str(&format!("  # section {i}\n  section_{i}:\n"));
        s.push_str(&format!("    title: Welcome to section number {i}, %{{name}}!\n"));
        s.push_str(&format!("    save: 'Save the changes you made in section {i}'\n"));
        s.push_str(&format!("    quoted: \"Say \\\"hello\\\" to <b>section {i}</b> now\"\n"));
        s.push_str("    count: 42\n    enabled: yes\n");
        s.push_str(&format!("    body: |\n      First line of the long text for section {i}.\n      Second line with {{count}} items and more words to fill the line.\n"));
        s.push_str(&format!("    messages:\n      one: \"%{{count}} message in section {i}\"\n      other: \"%{{count}} messages in section {i}\"\n"));
        s.push_str(&format!("    items:\n      - First item of section {i}\n      - Second item of section {i} with some more text\n      - [flow a {i}, flow b {i}]\n"));
        s.push_str(&format!("    folded: >-\n      A folded paragraph for section {i}\n      that spans two lines.\n"));
    }
    s
}

#[test]
#[ignore]
fn large_file_timing() {
    let text = generate(60_000);
    assert!(text.len() > 5_000_000, "{} bytes", text.len());
    if let Ok(p) = std::env::var("SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let bytes = text.as_bytes();
    let lim = Limits::unlimited();
    let t = Instant::now();
    let d = Yaml.extract_with(bytes, &ExtractOptions::default(), &lim).unwrap();
    let t_extract = t.elapsed();
    let t = Instant::now();
    let out = Yaml.merge(&d).unwrap();
    let t_merge = t.elapsed();
    assert_eq!(out, bytes);
    let mut d2 = d.clone();
    pseudo(&mut d2, "[xx] ");
    let t = Instant::now();
    let out2 = Yaml.merge(&d2).unwrap();
    let t_pseudo = t.elapsed();
    assert!(out2.len() > bytes.len());
    println!("SPEED bytes={} units={} extract_ms={} merge_untouched_ms={} merge_pseudo_ms={}", bytes.len(), d.units.len(), t_extract.as_millis(), t_merge.as_millis(), t_pseudo.as_millis());
}
