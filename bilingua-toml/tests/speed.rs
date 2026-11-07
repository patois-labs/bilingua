//! Speed: a generated file of at least 5 MB. The LocOS comparison is `bilingua-toml/tools/bench_locos.py` (run it
//! on the file `examples/speed.rs` writes). Release numbers on the dev PC: extract 0.8 s, untouched merge 0.07 s,
//! translated merge with verification 0.55 s, against 8.3 s / 7.6 s / 25.7 s for the LocOS filter.

mod common;
use bilingua_model::*;
use bilingua_toml::TomlFilter;
use std::time::Instant;

fn generate() -> String {
    let mut s = String::from("# generated\n");
    let mut i = 0;
    while s.len() < 5_100_000 {
        let n = i / 3;
        match i % 3 {
            0 => s.push_str(&format!("\n[s{n}]\ntitle = \"Welcome to bakery number {n}, fresh bread at six\"   # note\n")),
            1 => s.push_str(&format!("body = 'Literal {n} with {{{{ .Name }}}} and more words to translate'\n")),
            _ => s.push_str(&format!("long = \"\"\"\nMulti-line {n}.\nSecond line with \\\"x\\\".\"\"\"\nn = {n}\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn five_megabytes_extract_and_merge() {
    let s = generate();
    assert!(s.len() >= 5_000_000);
    let t = Instant::now();
    // more units than LocOS's default MAX_DOCUMENT_SEGMENTS (50000): raise it, as a desktop caller would
    let mut o = ExtractOptions::default();
    o.limits = Some(Limits::new().with_max_units(200_000).with_max_lines(20 * 200_000));
    let mut d = TomlFilter.extract(s.as_bytes(), &o).unwrap();
    let extract = t.elapsed();
    assert!(d.units.len() > 50_000);
    assert_eq!(TomlFilter.merge(&d).unwrap(), s.as_bytes());
    for u in d.units.iter_mut() {
        u.segments[0].target = vec![Inline::text("Zielsatz")];
    }
    let t = Instant::now();
    let out = TomlFilter.merge(&d).unwrap();
    let merge = t.elapsed();
    assert!(out.len() < s.len());
    eprintln!("5 MB: extract {extract:?}, translated merge {merge:?}");
    // loose bound that also holds in a debug build; linear behaviour is what is being checked
    assert!(extract.as_secs() < 60 && merge.as_secs() < 60);
}
