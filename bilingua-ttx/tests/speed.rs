//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/ttx_dump.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_ttx::Limits;
use common::*;

fn generate(min_chars: usize) -> Vec<u8> {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("<?xml version='1.0'?>\r\n<TRADOStag Version=\"2.0\"><FrontMatter>\r\n<UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"></UserSettings>\r\n</FrontMatter><Body><Raw>\r\n");
    let mut i = 0usize;
    while s.len() < min_chars {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        s.push_str("<ut Type=\"start\" Style=\"external\" RightEdge=\"angle\" DisplayText=\"p\">&lt;p&gt;</ut>");
        match i % 4 {
            0 => s.push_str(&format!("<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">Our {} {} is <ut Type=\"start\" DisplayText=\"b\">&lt;b&gt;</ut>{}<ut Type=\"end\" DisplayText=\"b\">&lt;/b&gt;</ut> now.</Tuv><Tuv Lang=\"DE-DE\"></Tuv></Tu>", w(1), w(2), w(3))),
            1 => s.push_str(&format!("<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">Caf\u{e9} {} \u{65e5}\u{672c} &amp; {}<ut DisplayText=\"br\">&lt;br&gt;</ut><df Bold=\"on\">{}</df> </Tuv></Tu>", w(3), w(4), w(5))),
            2 => s.push_str(&format!("<Tu MatchPercent=\"100\" Origin=\"manual\"><Tuv Lang=\"EN-US\">Done {}.</Tuv><Tuv Lang=\"DE-DE\">Fertig {}.</Tuv></Tu>", w(6), w(7))),
            _ => s.push_str(&format!("<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">Text about {}\r\n and {}</Tuv><Tuv Lang=\"DE-DE\"/></Tu>", w(1), w(2))),
        }
        s.push_str("<ut Type=\"end\" Style=\"external\" LeftEdge=\"angle\" DisplayText=\"p\">&lt;/p&gt;</ut>\r\n");
        i += 1;
    }
    s.push_str("</Raw></Body></TRADOStag>\r\n");
    let mut v = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        v.extend_from_slice(&u.to_le_bytes());
    }
    v
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(2_800_000);
    assert!(data.len() > 5_000_000, "{}", data.len());
    let limits = Limits::new().with_max_units(10_000_000);
    let mut d = DEFAULT.extract_with(&data, &opts(), &limits).unwrap();
    assert!(d.units.len() > 5_000);
    assert_eq!(DEFAULT.merge(&d).unwrap(), data);
    let n = d.units.len();
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = DEFAULT.merge(&d).unwrap();
    let back = DEFAULT.extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), n);
}
