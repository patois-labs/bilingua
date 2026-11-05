//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/dump.rs` (`gen` and `time`) and `tests/tools/speed_locos.py`; this test keeps the size, the round trip
//! and a generous time budget honest in every build.

use bilingua_model::*;
use bilingua_xml::XmlFilter;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<catalog xmlns:its=\"http://www.w3.org/2005/11/its\" its:version=\"2.0\" xml:lang=\"en\">\n  <its:rules version=\"2.0\">\n    <its:translateRule selector=\"//price\" translate=\"no\"/>\n    <its:translateRule selector=\"//item/@label\" translate=\"yes\"/>\n    <its:withinTextRule selector=\"//b | //i | //br\" withinText=\"yes\"/>\n  </its:rules>\n",
    );
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        s.push_str(&format!(
            "  <item sku=\"B-{i}\" label=\"Our {} {}\">\n    <name>{} {}</name>\n    <desc>A <b>{}</b> loaf, baked <i>{}</i> every day &amp; sold {{0}} times.<br/>Caf\u{e9} \u{65e5}\u{672c} order before noon, {} please.</desc>\n    <price currency=\"EUR\">{}.50</price>\n  </item>\n",
            w(1), w(2), w(3), w(4), w(5), w(6), w(7), i % 90
        ));
        i += 1;
    }
    s.push_str("</catalog>\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let f = XmlFilter::new();
    let o = ExtractOptions {
        limits: Some(Limits { max_units: 10_000_000, max_xml_elements: 100_000_000, ..Limits::default() }),
        target_lang: Some("fr".into()),
        ..Default::default()
    };
    let t0 = std::time::Instant::now();
    let mut d = f.extract(&data, &o).unwrap();
    assert!(d.units.len() > 50_000);
    assert_eq!(f.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = f.merge(&d).unwrap();
    let took = t0.elapsed().as_secs_f64();
    let back = f.extract(&out, &o).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    assert!(String::from_utf8_lossy(&out).contains("xml:lang=\"fr\""));
    eprintln!("5 MB: {} units, extract + merge + translated merge in {took:.2} s (this build)", d.units.len());
    assert!(took < 120.0, "far too slow: {took:.1} s");
}
