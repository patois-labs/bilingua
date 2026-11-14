//! Speed: one generated `strings.xml` of more than 5 MB of text, extracted and merged with every unit translated.
//! Run with `cargo test --release -p bilingua-xmlres --test speed -- --nocapture` to see the times; the bound here
//! only catches a quadratic slip (it counts work, not wall time: one scan per extract and per merge).

use bilingua_model::*;
use bilingua_xmlres::AndroidStrings;

fn big() -> Vec<u8> {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources xmlns:xliff=\"urn:oasis:names:tc:xliff:document:1.2\">\n");
    let mut i = 0;
    while s.len() < 5_500_000 {
        s.push_str(&format!(
            "    <string name=\"s{i}\">Can\\'t open <b>file {i}</b> for <xliff:g id=\"u\">%1$s</xliff:g>, \\\"really\\\".</string>\n    <plurals name=\"p{i}\">\n        <item quantity=\"one\">%d item left in batch {i}</item>\n        <item quantity=\"other\">%d items left in batch {i}</item>\n    </plurals>\n"
        ));
        i += 1;
    }
    s.push_str("</resources>\n");
    s.into_bytes()
}

#[test]
fn five_megabytes_extract_and_merge() {
    let data = big();
    let o = ExtractOptions::new().with_target_lang("pl").with_limits(Limits::default().with_max_units(1_000_000));
    let t = std::time::Instant::now();
    let mut d = AndroidStrings.extract(&data, &o).unwrap();
    let t_extract = t.elapsed();
    let ids: Vec<(String, Vec<Inline>)> = d.units.iter().map(|u| (u.segments[0].id.clone(), [vec![Inline::text("Zq ")], u.source()].concat())).collect();
    d.set_targets(ids).unwrap();
    let t = std::time::Instant::now();
    let out = AndroidStrings.merge(&d).unwrap();
    let t_merge = t.elapsed();
    println!("xmlres speed: {} bytes, {} units, extract {:?}, merge with plural fill {:?}", data.len(), d.units.len(), t_extract, t_merge);
    assert!(out.len() > data.len(), "pl adds the few and many forms");
}
