//! Speed: one generated contents file of at least 5 MB (Windows-1252, CRLF), extracted and merged with every unit
//! translated into Russian (so the whole file is re-encoded to Windows-1251). Run in release for the real number:
//!   cargo test -p bilingua-htmlhelp --release --test speed -- --nocapture
//! Set BILINGUA_HH_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_htmlhelp::*;
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let mut s = String::with_capacity(5_400_000);
    s.push_str("<!DOCTYPE HTML PUBLIC \"-//IETF//DTD HTML//EN\">\r\n<HTML><HEAD>\r\n<meta name=\"GENERATOR\" content=\"Microsoft&reg; HTML Help Workshop 4.1\">\r\n<!-- Sitemap 1.0 -->\r\n</HEAD><BODY>\r\n<OBJECT type=\"text/site properties\">\r\n\t<param name=\"Font\" value=\"Tahoma,8,0\">\r\n</OBJECT>\r\n<UL>\r\n");
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        s.push_str(&format!(
            "\t<LI> <OBJECT type=\"text/sitemap\">\r\n\t\t<param name=\"Name\" value=\"Chapter {i}: Caf\u{e9} &amp; &quot;tips&quot; for the {i}th bakery\">\r\n\t\t<param name=\"Local\" value=\"html\\chapter{i}.htm#top\">\r\n\t\t<param name=\"ImageNumber\" value=\"{}\">\r\n\t\t</OBJECT>\r\n",
            i % 9
        ));
        i += 1;
    }
    s.push_str("</UL>\r\n</BODY></HTML>\r\n");
    s.chars().map(|c| c as u32 as u8).collect()
}

#[test]
fn large_file() {
    let data = generate();
    assert!(data.len() >= 5_000_000, "{}", data.len());
    if let Ok(p) = std::env::var("BILINGUA_HH_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let limits = Limits { max_units: 10_000_000, max_lines: 100_000_000, ..Limits::default() };
    let opts = ExtractOptions { limits: Some(limits), target_lang: Some("ru-RU".into()), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = HhcFilter.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000);
    let t_id = Instant::now();
    assert_eq!(HhcFilter.merge(&doc).unwrap(), data);
    let t_identity = t_id.elapsed();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let t = format!("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} {i}");
        u.segments[0].target = vec![Inline::text(t)];
    }
    let out = HhcFilter.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 1_000_000);
    let back = HhcFilter.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "htmlhelp speed: {:.1} MB, {n} units, extract {:.0} ms, untouched merge {:.0} ms, translate+merge (re-encode to Windows-1251) {:.0} ms, extract+merge {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_identity.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
}
