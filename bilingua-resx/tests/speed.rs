//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/tool.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_resx::{Limits, Resx};
use common::*;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        match i % 4 {
            0 => s.push_str(&format!("  <data name=\"Item{i}\" xml:space=\"preserve\">\n    <value>Our {} {} is ready, {{0}} items for {{1}}</value>\n    <comment>note {i}</comment>\n  </data>\n", w(1), w(2))),
            1 => s.push_str(&format!("  <data name=\"Item{i}\"><value>Caf\u{e9} {} \u{65e5}\u{672c} {} don&apos;t &amp; {{0}}</value></data>\n", w(3), w(4))),
            2 => s.push_str(&format!("  <data name=\"Size{i}\" type=\"System.Drawing.Size\"><value>{}, {}</value></data>\n", i % 700, i % 400)),
            _ => s.push_str(&format!("  <data name=\"Item{i}\"><value><![CDATA[A <b>{}</b> & {}]]></value></data>\n", w(1), w(2))),
        }
        i += 1;
    }
    s.push_str("</root>\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let limits = Limits::new().with_max_units(10_000_000).with_max_xml_elements(50_000_000);
    let d = Resx.extract_with(&data, &opts(), &limits).unwrap();
    assert!(d.units.len() > 30_000);
    assert_eq!(Resx.merge(&d).unwrap(), data);
    let mut e = d.clone();
    for u in &mut e.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = Resx.merge(&e).unwrap();
    let back = Resx.extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    let tags = |d: &Document| d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum::<usize>();
    assert_eq!(tags(&back), tags(&d));
}
