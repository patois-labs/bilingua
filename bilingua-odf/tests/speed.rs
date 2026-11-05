//! A large generated document (over 5 MB of content.xml), extracted and merged. Run
//! `cargo test --release -p bilingua-odf --test speed -- --nocapture` for the timings. Set `ODF_SPEED_OUT` to a folder
//! to also save the file (`big.odt`), so the LocOS filter can be timed on the same bytes (`tools/time_locos.py`).
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;
use std::time::Instant;

pub fn big() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut c = String::with_capacity(6_000_000);
    c.push_str(&odt_content().split("<office:text>").next().unwrap().replace("NSDECL", NSDECL));
    c.push_str("<office:text>");
    let mut n = 0usize;
    while c.len() < 5_600_000 {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        c.push_str(&format!(
            "<text:p text:style-name=\"P{}\">Open the {} <text:span text:style-name=\"T{}\">{}</text:span> near the <text:a xlink:href=\"https://x.example/{n}\">{}</text:a> at gate {n}.<text:tab/>Bring the {} for number {n} of the guide,<text:s text:c=\"2\"/>please &amp; thank you.<text:line-break/>The {} closes early.</text:p>",
            n % 7,
            w(1),
            n % 5,
            w(2),
            w(3),
            w(4),
            w(5)
        ));
        n += 1;
    }
    c.push_str("</office:text></office:body></office:document-content>");
    Pkg::new(c).styles(odt_styles()).build()
}

#[test]
fn large_document_round_trips_and_is_timed() {
    let bytes = big();
    let content = member(&bytes, "content.xml").len();
    assert!(content >= 5_000_000, "only {content} bytes of content.xml");
    if let Ok(dir) = std::env::var("ODF_SPEED_OUT") {
        std::fs::write(format!("{dir}/big.odt"), &bytes).unwrap();
    }
    let f = Odt::default();
    let o = ExtractOptions::new().with_target_lang("de-DE");
    let t = Instant::now();
    let mut d = f.extract(&bytes, &o).unwrap();
    let t_extract = t.elapsed();
    let n = d.units.len();
    assert!(n >= 15_000, "{n}");
    let t = Instant::now();
    let same = f.merge(&d).unwrap();
    let t_same = t.elapsed();
    assert_eq!(same, bytes);
    for u in d.units.iter_mut() {
        let (text, map) = profile::tokens::to_locos_text(&u.source());
        u.segments[0].target = profile::tokens::from_locos_text(&format!("\u{ab}{text}\u{bb}"), &map);
    }
    let t = Instant::now();
    let out = f.merge(&d).unwrap();
    let t_de = t.elapsed();
    let back = f.extract(&out, &ExtractOptions::default()).unwrap();
    assert_eq!(back.units.len(), n);
    println!("odt speed: {} bytes ({} content.xml), {n} units; extract {t_extract:?}, untouched merge {t_same:?}, de merge {t_de:?}; extract+merge {:?}", bytes.len(), content, t_extract + t_de);
}
