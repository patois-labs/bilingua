//! A generated file of more than 5 MB per dialect. The real timing (release build, against the LocOS filter) is
//! `examples/wordfast_bench.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_wordfast::{Txlf, Txml};
use common::*;

const WORDS: [&str; 10] = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];

fn generate_txml(min_bytes: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<txml locale=\"en_US\" version=\"1.0\" targetlocale=\"de_DE\">\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| WORDS[(i * 7 + k * 3) % WORDS.len()];
        s.push_str(&format!(
            "<translatable blockId=\"{i}\">\n<segment segmentId=\"1\"><source>Our {} {} is <ut type=\"start\" x=\"1\">&lt;b&gt;</ut>ready<ut type=\"end\" x=\"1\">&lt;/b&gt;</ut>, {{0}} items &amp; more.</source><ws> </ws></segment>\n<segment segmentId=\"2\" gtmt=\"true\"><source>Caf\u{e9} {} \u{65e5}\u{672c} <ut x=\"2\">&lt;br/&gt;</ut>{}</source><target>MT {}</target></segment>\n<segment segmentId=\"3\" locked=\"true\"><source>SKU-{i:06}</source></segment>\n</translatable>\n",
            w(1),
            w(2),
            w(3),
            w(4),
            w(3)
        ));
        i += 1;
    }
    s.push_str("</txml>\n");
    s
}

fn generate_txlf(min_bytes: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\">\n<file original=\"a.docx\" source-language=\"en-US\" target-language=\"de-DE\"><body>\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| WORDS[(i * 7 + k * 3) % WORDS.len()];
        s.push_str(&format!(
            "<trans-unit id=\"{a}\"><source>Our {} {} is <bpt id=\"1\">&lt;b&gt;</bpt>ready<ept id=\"1\">&lt;/b&gt;</ept>, <ph id=\"2\">{{0}}</ph> items &amp; more.</source><target state=\"needs-translation\"/></trans-unit>\n<trans-unit id=\"{b}\"><source>Caf\u{e9} {} \u{65e5}\u{672c} <x id=\"3\"/>{}</source><target state=\"needs-review-translation\" gs4tr:score=\"70\">MT {}</target></trans-unit>\n<trans-unit id=\"{c}\" gs4tr:locked=\"true\"><source>SKU-{i:06}</source></trans-unit>\n",
            w(1),
            w(2),
            w(3),
            w(4),
            w(3),
            a = i * 3 + 1,
            b = i * 3 + 2,
            c = i * 3 + 3,
        ));
        i += 1;
    }
    s.push_str("</body></file></xliff>\n");
    s
}

fn round_trip(f: &dyn Filter, data: &[u8]) {
    assert!(data.len() > 5_000_000);
    let o = opts().with_limits(Limits::new().with_max_units(10_000_000).with_max_xml_elements(100_000_000));
    let mut d = f.extract(data, &o).unwrap();
    assert!(d.units.len() > 20_000, "{}", d.units.len());
    assert_eq!(f.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = f.merge(&d).unwrap();
    let back = f.extract(&out, &o).unwrap();
    assert_eq!(back.units.len(), d.units.len());
}

#[test]
fn txml_five_megabytes_round_trip() {
    round_trip(&Txml { load_targets: true }, generate_txml(5_200_000).as_bytes());
}

#[test]
fn txlf_five_megabytes_round_trip() {
    round_trip(&Txlf { load_targets: true }, generate_txlf(5_200_000).as_bytes());
}
