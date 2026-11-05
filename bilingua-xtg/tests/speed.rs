//! Speed: one generated file of at least 5 MB (UTF-16 with a BOM, as QuarkXPress for Windows writes it), extracted
//! and merged with every unit translated. Run in release for the real number:
//!   cargo test -p bilingua-xtg --release --test speed -- --nocapture
//! Set BILINGUA_XTG_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_model::*;
use bilingua_xtg::Xtg;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(3_000_000);
    s.push_str("<v19.00><e8>\r\n@Normal=[S\"\",\"\"]<*L*h\"Standard\"f\"Helvetica\"z12>\r\n@Heading=[S\"\",\"Body\"]<*C><B>\r\n");
    let mut i = 0usize;
    while s.len() < 2_700_000 {
        s.push_str(&format!("@Heading:<*C>Getting started with item {i}\r\n"));
        s.push_str(&format!(
            "@Body:Hello <B>bold {i}<P> world<\\n>second line with <I>italic <B>and bold<B$><I$> text, price 5 \u{20ac}, caf\u{e9}.\r\n"
        ));
        s.push_str(&format!("Mail us <\\@> once, 3 <\\<> 4 <\\>> 2, path C:<\\\\>x{i}<\\i>.\r\n"));
        s.push_str(&format!("<f\"Arial\"><z10>Leading tags stay outside {i}<f$><z$>\r\n"));
        s.push_str(&format!(
            "@$:<XI,\"Entry {i}\",\"\">Indexed <A(3,\"HYP\\\"B\",\\#002\\#000)[2]>link <c\"Red\">red {i}<c$> and \u{65e5}\u{672c}\u{8a9e} \u{645}\u{631}\u{62d}\u{628}\u{627}.\r\n\r\n"
        ));
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    let data = common::utf16le(&text, true);
    assert!(data.len() >= 5_000_000, "{}", data.len());
    if let Ok(p) = std::env::var("BILINGUA_XTG_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let limits = Limits { max_units: 10_000_000, max_lines: 100_000_000, ..Limits::default() };
    let opts = ExtractOptions { limits: Some(limits), target_lang: Some("de-DE".into()), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = Xtg.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000);
    assert_eq!(Xtg.merge(&doc).unwrap(), data);
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} <q> "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = Xtg.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 1_000_000);
    let back = Xtg.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "xtg speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
}
