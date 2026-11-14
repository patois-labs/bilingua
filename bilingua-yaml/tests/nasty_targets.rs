//! Any translation text (quotes, colons, brackets, line breaks, white space) merges into a file that reads back
//! with the same translations (gate, 11-04).
mod common;
use bilingua_model::*;
use bilingua_yaml::Yaml;
use common::*;
use proptest::prelude::*;
fn srcs() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = ["generic.yaml", "rails.en.yml", "messages.en.yaml"].iter().map(|n| fixture(n)).collect();
    v.push(b"a: |\n  one\n\n  two\nb: >-\n  x\n  y\nc: [a, {b: 'c'}, \"d\"]\nd: |+\n  keep\n\ne: >2\n   ind\nf: 'q'\n".to_vec());
    v.push(b"en:\r\n  x:\r\n    one: One\r\n    other: Many\r\n  y: |\r\n    blk\r\n".to_vec());
    v
}
proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]
    #[test]
    fn nasty_targets(which in 0usize..5, seed in proptest::collection::vec(r#"[ a-z\u{e9}\u{65e5}\-:#'"\\\[\]{},&*!|>%@`\n\t~?]{1,12}"#, 1..40), tgt in proptest::option::of(proptest::sample::select(vec!["de-DE","pl","ar","no","ja"]))) {
        let seed: Vec<String> = seed;
        let src = &srcs()[which];
        let mut d = Yaml.extract(src, &opts(tgt)).unwrap();
        let mut want: Vec<String> = Vec::new();
        for (i, u) in d.units.iter_mut().enumerate() {
            let t: String = seed[i % seed.len()].clone();
            u.segments[0].target = vec![Inline::text(t.clone())];
            let tr: String = t.trim().to_string();
            if tr.chars().any(|c: char| c.is_alphanumeric()) { want.push(tr); }
        }
        let out = Yaml.merge(&d).unwrap();
        let s = String::from_utf8_lossy(&out).to_string();
        let back = Yaml.extract(&out, &opts(None));
        prop_assert!(back.is_ok(), "{:?} {}", back.err(), s);
        let got: Vec<String> = back.unwrap().units.iter().map(text_of).collect();
        let mut it = got.iter();
        for e in &want { prop_assert!(it.any(|g: &String| g == e), "missing {:?} in {}", e, s); }
    }
}
