//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-stf --release --test speed -- --nocapture
//! Set BILINGUA_STF_SPEED_OUT to a path to keep the generated file (the LocOS timing script reads it).
use bilingua_model::*;
use bilingua_stf::StfFilter;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("# Lumen Bakery generated\nLanguage code: de\nType: Bilingual\n\n------------------TRANSLATED-------------------\n\n# KEY\tLABEL\tTRANSLATION\tOUT OF DATE\n");
    let mut i = 0;
    while s.len() < 2_700_000 {
        match i % 4 {
            0 => s.push_str(&format!("CustomField.Account.F{i}.FieldLabel\tClick the button number {i}\tKlicken Sie die Taste {i}\t-\n")),
            1 => s.push_str(&format!("CustomField.Account.F{i}.HelpText\tPick the region.\\nOne per account {i}\tRegion w\u{e4}hlen\t*\n")),
            2 => s.push_str(&format!("CustomLabel.L{i}\t{{0}} items in {{!$User.Name}} ({i})\t\u{65e5}\u{672c}\u{8a9e}\t-\n")),
            _ => s.push_str(&format!("CustomLabel.M{i}\t\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {i}\t\t-\n")),
        }
        i += 1;
    }
    s.push_str("\n------------------UNTRANSLATED-----------------\n\n# KEY\tLABEL\n");
    while s.len() < 5_400_000 {
        match i % 3 {
            0 => s.push_str(&format!("CustomLabel.U{i}\tWelcome back, {{0}}! You have {{1,number}} messages ({i})\n")),
            1 => s.push_str(&format!("PicklistValue.Account.Industry.V{i}\tAgriculture and farming {i}\n")),
            _ => s.push_str(&format!("CustomLabel.N{i}\tLine one\\nLine two\\tTabbed {i}\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_STF_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let limits = Limits { max_units: 10_000_000, max_lines: 10_000_000, ..Limits::default() };
    let t0 = Instant::now();
    let mut doc = StfFilter.extract_with(text.as_bytes(), &ExtractOptions { target_lang: Some("de-DE".into()), ..Default::default() }, &limits).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} \"q\" "))];
        t.extend(u.segments[0].source.iter().filter(|x| matches!(x, Inline::Tag { .. })).cloned());
        u.segments[0].target = t;
    }
    let out = StfFilter.merge_with(&doc, &MergeOptions::locos()).unwrap();
    let t_merge = t1.elapsed();
    let untouched = StfFilter.extract_with(text.as_bytes(), &ExtractOptions::default(), &limits).unwrap();
    assert_eq!(StfFilter.merge(&untouched).unwrap(), text.as_bytes());
    println!(
        "stf speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}
