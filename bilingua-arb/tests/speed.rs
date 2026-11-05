//! Speed on one large generated ARB file (about 6 MB, never committed). Run with
//! `cargo test -p bilingua-arb --release --test speed -- --nocapture` and compare with
//! `tests/tools/locos_speed.py`, which times the LocOS filter on the identical file.

use bilingua_arb::Arb;
use bilingua_model::*;
use std::time::Instant;

const N: usize = 30000;

/// Must match `generate()` in locos_speed.py.
fn generate(n: usize) -> String {
    let mut out = String::from("{\n  \"@@locale\": \"en\"");
    for i in 0..n {
        let m = match i % 4 {
            0 => format!("Welcome to Larchmoor {{name}}, your order number {i} is ready for pickup at the harbour bakery counter today."),
            1 => format!("{{count, plural, =0{{No loaves in basket {i}}} one{{# loaf in basket {i}}} other{{# loaves in basket {i}}}}} baked fresh this morning"),
            2 => format!("{{gender, select, female{{She ordered item {i}}} male{{He ordered item {i}}} other{{They ordered item {i}}}}}"),
            _ => format!("Opening hours for branch {i}: every day from 7 AM to 6 PM, closed on public holidays and Sundays in winter."),
        };
        out.push_str(&format!(",\n  \"message{i}\": \"{m}\",\n  \"@message{i}\": {{\"description\": \"Shown on screen number {i}\"}}"));
    }
    out.push_str("\n}\n");
    out
}

#[test]
fn large_file_extract_and_merge() {
    let text = generate(N);
    let data = text.as_bytes();
    assert!(data.len() > 5_000_000, "{}", data.len());
    let t = Instant::now();
    let mut doc = Arb.extract(data, &ExtractOptions::default()).unwrap();
    let t_extract = t.elapsed();
    let t = Instant::now();
    let same = Arb.merge(&doc).unwrap();
    let t_untouched = t.elapsed();
    assert_eq!(same, data);
    doc.target_lang = Some("ru".into());
    for u in doc.units.iter_mut() {
        for s in u.segments.iter_mut() {
            let mut t = vec![Inline::text("T:")];
            t.extend(s.source.clone());
            s.target = merge_text(t);
        }
    }
    let t = Instant::now();
    let out = Arb.merge(&doc).unwrap();
    let t_localized = t.elapsed();
    println!("file: {} bytes, {} messages, {} units", data.len(), N, doc.units.len());
    println!("rust extract: {t_extract:?}");
    println!("rust merge (nothing translated): {t_untouched:?}");
    println!("rust merge (all translated, ru): {t_localized:?}");
    println!("rust total: {:?}", t_extract + t_untouched + t_localized);
    assert!(out.len() > data.len());
    if !cfg!(debug_assertions) {
        assert!((t_extract + t_untouched + t_localized).as_secs_f64() < 3.0);
    }
}
