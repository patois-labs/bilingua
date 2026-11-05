//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/icml_dump.rs` (`time`); this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_icml::Icml;
use bilingua_model::*;
use common::*;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from(HEAD);
    s.push_str("\t<Story Self=\"u1\">\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        let body = match i % 3 {
            0 => psr(&format!("{}{}", csr(&format!("<Content>Our {} {} is ready, order #{i} </Content>", w(1), w(2))), csr_with(&format!("<Content>{}</Content><Br/>", w(3)), BOLD, ""))),
            1 => psr(&csr(&format!("<Content>Caf\u{e9} {}\t\u{65e5}\u{672c} </Content><HyperlinkTextSource Self=\"h{i}\"><Content>{}</Content></HyperlinkTextSource><Br/>", w(4), w(5)))),
            _ => psr(&csr(&format!("<Content>Head {}</Content><Br/><Content>Line<?ACE 7?> of {}</Content><Br/>", w(6), w(7)))),
        };
        s.push_str(&body);
        s.push('\n');
        i += 1;
    }
    s.push_str("\t</Story>\n</Document>\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let limits = Limits::new().with_max_units(10_000_000);
    let mut d = Icml.extract_with(&data, &opts(), &limits).unwrap();
    assert!(d.units.len() > 15_000, "{}", d.units.len());
    assert_eq!(Icml.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = Icml.merge(&d).unwrap();
    let back = Icml.extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), d.units.len());
}
