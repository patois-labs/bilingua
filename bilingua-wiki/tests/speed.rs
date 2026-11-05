//! Speed: one generated file of at least 5 MB per dialect, extracted and merged with every unit translated. Run in
//! release for the real number:
//!   cargo test -p bilingua-wiki --release --test speed -- --nocapture
//! Set BILINGUA_WIKI_SPEED_OUT to a folder to keep the generated files (tests/tools/speed_locos.py times LocOS on
//! them).
mod common;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn generate_mw() -> String {
    let mut s = String::with_capacity(5_300_000);
    s.push_str("{{Infobox bakery\n| name = Lumen\n}}\n__NOTOC__\n");
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        s.push_str(&format!(
            "== Section {i} ==\n'''Lumen''' bakes [[sourdough bread|sourdough]] number {i} every morning.<ref>Hours change on holidays {i}.</ref> See https://example.com/menu/{i} or [https://example.com/order/{i} order online].\nSecond line of the paragraph with ''italic words'' and a {{{{tpl|a={i}}}}} template, \u{65e5}\u{672c}\u{8a9e} text and \u{645}\u{631}\u{62d}\u{628}\u{627}.\n\n* First item {i} with &amp; entity\n* Second item [[File:Loaf{i}.jpg|thumb|200px|A fresh loaf {i}]]\n# Numbered {i}\n; Term {i} : The definition of term {i}\n\n{{| class=\"wikitable\"\n|+ Menu {i}\n! Day !! Bread\n|-\n| style=\"x\" | Monday {i} || Rye loaf\n|}}\n\n<!-- editor note {i} -->\n<pre>\nraw {i}\n</pre>\n[[Category:Bakery {i}]]\n\n"
        ));
        i += 1;
    }
    s
}

pub fn generate_dw() -> String {
    let mut s = String::with_capacity(5_300_000);
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        s.push_str(&format!(
            "====== Section {i} ======\nOur **sourdough** bread number {i} is //baked// every morning in the [[bakery:menu|old bakery]]. See https://example.com/menu/{i} ((Hours change {i})).\nSecond line with ''mono'' and {{{{logo.png?100|A fresh loaf {i}}}}} and \u{65e5}\u{672c}\u{8a9e} text.\n\n  * First item {i}\n  - Second item {i}\n\n^ Day ^ Bread ^\n| Monday {i} | Rye loaf |\n\n> Quote number {i} about fresh bread.\n<code>\necho \"keep {i}\";\n</code>\n\n"
        ));
        i += 1;
    }
    s
}

fn run(label: &str, f: &dyn Filter, text: String, ext: &str) {
    assert!(text.len() >= 5_000_000, "{}", text.len());
    if let Ok(dir) = std::env::var("BILINGUA_WIKI_SPEED_OUT") {
        std::fs::write(std::path::Path::new(&dir).join(format!("speed.{ext}")), &text).unwrap();
    }
    let data = text.into_bytes();
    let limits = Limits { max_units: 10_000_000, max_lines: 100_000_000, max_unit_chars: 1_000_000, ..Limits::default() };
    let opts = ExtractOptions { limits: Some(limits), target_lang: Some("de-DE".into()), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = f.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000, "{n}");
    assert_eq!(f.merge(&doc).unwrap(), data);
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = f.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 1_000_000);
    let back = f.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "wiki speed ({label}): {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
}

#[test]
fn large_mediawiki_file() {
    run("mediawiki", &mw(), generate_mw(), "wiki");
}

#[test]
fn large_dokuwiki_file() {
    run("dokuwiki", &dw(), generate_dw(), "dokuwiki");
}
