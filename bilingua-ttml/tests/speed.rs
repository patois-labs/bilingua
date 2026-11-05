//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/ttml_dump.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_ttml::Limits;
use common::*;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xml:lang=\"en\">\r\n<body><div>\r\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        match i % 4 {
            0 => s.push_str(&format!("<p begin=\"{i}s\" end=\"{i}s\">Our {} {} is ready,<br/>{{0}} items for {{1}}</p>\r\n", w(1), w(2))),
            1 => s.push_str(&format!("<p begin=\"{i}s\" end=\"{i}s\">Caf\u{e9} {} \u{65e5}\u{672c} <span tts:fontStyle=\"italic\">{}</span> &amp; {{0}}</p>\r\n", w(3), w(4))),
            2 => s.push_str(&format!("<!-- c{i} -->\r\n<p begin=\"{i}s\" end=\"{i}s\">{}</p>\r\n", w(5))),
            _ => s.push_str(&format!("<p begin=\"{i}s\" end=\"{i}s\">\r\n  A long text about {}\r\n</p>\r\n", w(1))),
        }
        i += 1;
    }
    s.push_str("</div></body></tt>\r\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let limits = Limits { max_units: 10_000_000, ..Limits::default() };
    let mut d = F.extract_with(&data, &opts_for(Some("ar")), &limits).unwrap();
    assert!(d.units.len() > 30_000);
    assert_eq!(F.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = F.merge(&d).unwrap();
    let back = F.extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), d.units.len());
}
