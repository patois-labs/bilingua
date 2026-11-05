//! Hostile input: nothing panics, errors are typed, accepted files keep their round trip.

mod common;
use bilingua_model::*;
use bilingua_yaml::Yaml;
use common::*;
use proptest::prelude::*;

fn check(bytes: &[u8]) {
    match Yaml.extract(bytes, &opts(Some("de"))) {
        Ok(mut d) => {
            // a file that parses merges back untouched (the localized rename aside) and reads again after pseudo-translation
            d.target_lang = None;
            assert_eq!(Yaml.merge(&d).unwrap(), bytes);
            pseudo(&mut d, "[x] ");
            let out = Yaml.merge(&d).unwrap();
            let again = Yaml.extract(&out, &opts(None)).expect("a pseudo-translated merge must parse");
            assert_eq!(again.units.len(), d.units.len());
        }
        Err(Error::Parse { .. } | Error::Unsupported(_)) => {}
        Err(e) => panic!("unexpected error kind {e:?}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = ["generic.yaml", "rails.en.yml", "messages.en.yaml"].iter().map(|n| fixture(n)).collect();
    v.push(b"a: |\n  one\n\n  two\nb: >-\n  x\n  y\nc: [a, {b: 'c'}, \"d\"]\n? [k]\n: v\n".to_vec());
    v
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        check(&bytes);
    }

    #[test]
    fn random_yaml_like_text_never_panics(s in "[ a-z0-9:\\-\\[\\]{},#&*!|>'\"?%@\\n\\r\\t.~]{0,400}") {
        check(s.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..4, edits in proptest::collection::vec((any::<u16>(), any::<u8>(), 0u8..3), 1..8), cut in proptest::option::of(any::<u16>())) {
        let mut b = fixtures()[which].clone();
        for (pos, byte, kind) in edits {
            if b.is_empty() { break; }
            let i = pos as usize % b.len();
            match kind {
                0 => b[i] = byte,
                1 => { b.insert(i, byte); }
                _ => { b.remove(i); }
            }
        }
        if let Some(c) = cut { let n = c as usize % (b.len() + 1); b.truncate(n); }
        check(&b);
    }
}

#[test]
fn deep_huge_and_odd_files_return_errors_fast() {
    let t0 = std::time::Instant::now();
    let cases: Vec<Vec<u8>> = vec![
        "[".repeat(1_000_000).into_bytes(),
        "- ".repeat(300_000).into_bytes(),
        format!("a: {}", "x".repeat(2_000_000)).into_bytes(),
        "a:\n".repeat(100_000).into_bytes(),
        "&a ".repeat(200_000).into_bytes(),
        "? ".repeat(100_000).into_bytes(),
        format!("k: '{}", "word ".repeat(300_000)).into_bytes(),
        "{".repeat(300).into_bytes(),
        (0..300).map(|i| format!("{}k:\n", " ".repeat(i))).collect::<String>().into_bytes(),
        b"\xff\xfe\x00\x00".to_vec(),
        b"\xef\xbb\xbf".to_vec(),
        Vec::new(),
    ];
    for c in &cases {
        match Yaml.extract(c, &opts(None)) {
            Ok(_) | Err(Error::Parse { .. }) | Err(Error::Unsupported(_)) => {}
            Err(e) => panic!("{e:?}"),
        }
    }
    assert!(t0.elapsed().as_secs() < 20, "took {:?}", t0.elapsed());
}

#[test]
fn deeply_nested_blocks_are_refused_not_overflowed() {
    let deep: String = (0..250).map(|i| format!("{}k:\n", " ".repeat(i))).collect::<String>() + &" ".repeat(250) + "v: x\n";
    assert!(matches!(Yaml.extract(deep.as_bytes(), &opts(None)), Err(Error::Unsupported(_))));
    let ok: String = (0..150).map(|i| format!("{}k:\n", " ".repeat(i))).collect::<String>() + &" ".repeat(150) + "v: x\n";
    assert_eq!(Yaml.extract(ok.as_bytes(), &opts(None)).unwrap().units.len(), 1);
}

#[test]
fn stale_skeleton_is_refused() {
    let mut d = extract(b"a: Hello\n");
    d.skeleton = Skeleton::text(vec![SkelPart::Unit(5)], "utf-8", bilingua_model::ParserInfo::new("bilingua-yaml", "0", 1));
    assert!(Yaml.merge(&d).is_err());
    let mut d = extract(b"a: Hello\n");
    d.skeleton = Skeleton::text(vec![SkelPart::Unit(0)], "utf-8", bilingua_model::ParserInfo::new("other-crate", "0", 1));
    assert!(Yaml.merge(&d).is_err());
}
