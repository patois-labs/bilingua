//! Hostile input: random bytes, MIF-shaped noise, mutated and truncated fixtures, deep and huge files. Nothing
//! panics; a file that opens merges back to its exact bytes and survives translation.
mod common;
use bilingua_mif::Mif;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn check(data: &[u8]) {
    let _ = Mif.sniff(data);
    match extract(data) {
        Ok(mut doc) => {
            assert!(Mif.merge(&doc).unwrap() == data, "untouched merge must be exact");
            let n = doc.units.len();
            for u in doc.units.iter_mut() {
                let src = u.source();
                // the translation keeps every tag and wraps the text in characters that need escapes
                let tr: Vec<Inline> = src
                    .iter()
                    .map(|i| match i {
                        Inline::Text(t) => Inline::Text(format!("~{t}~\"'`\\>")),
                        tag => tag.clone(),
                    })
                    .collect();
                u.segments[0].target = tr;
            }
            doc.target_lang = Some("de-DE".into());
            let out = Mif.merge(&doc).unwrap_or_else(|e| {
                panic!("merge failed ({e}) for {:?}", String::from_utf8_lossy(data))
            });
            let again = extract(&out).unwrap_or_else(|e| {
                panic!(
                    "reopen failed ({e}) for {:?} -> {:?}",
                    String::from_utf8_lossy(data),
                    String::from_utf8_lossy(&out)
                )
            });
            assert_eq!(again.units.len(), n, "unit count after translation");
        }
        Err(Error::Parse { .. } | Error::Unsupported(_) | Error::Encoding(_) | Error::Limit(_)) => {}
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    v.push(SUITE.to_vec());
    v
}

#[test]
fn fixtures_survive_hostility() {
    for f in fixtures() {
        check(&f);
        // every truncation of the first 3 KB
        for cut in (0..f.len().min(3000)).step_by(7) {
            check(&f[..cut]);
        }
        // every single byte flipped in the first 600 bytes
        for i in 0..f.len().min(600) {
            let mut m = f.clone();
            m[i] = m[i].wrapping_add(1);
            check(&m);
            m[i] = b'>';
            check(&m);
            m[i] = b'`';
            check(&m);
        }
    }
}

#[test]
fn deep_nesting_is_refused_not_a_stack_overflow() {
    for depth in [257usize, 1000, 200_000] {
        let mut d = b"<MIFFile 2019>\n".to_vec();
        d.extend(b"<A ".repeat(depth));
        d.extend(b">".repeat(depth));
        match extract(&d) {
            Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::XmlDepth),
            other => panic!("depth {depth}: {other:?}"),
        }
    }
    // exactly at the limit still opens
    let mut d = b"<MIFFile 2019>\n".to_vec();
    d.extend(b"<A ".repeat(253));
    d.extend(b"<Para <ParaLine <String `Deep text'>>>");
    d.extend(b">".repeat(253));
    assert_eq!(extract(&d).unwrap().units.len(), 1);
    // a caller's own depth limit is honoured
    let mut o = opts();
    o.limits = Some(Limits { max_xml_depth: 8, ..Limits::default() });
    let mut d = b"<MIFFile 2019>\n".to_vec();
    d.extend(b"<A ".repeat(9));
    d.extend(b">".repeat(9));
    assert!(matches!(Mif.extract(&d, &o), Err(Error::Limit(_))));
}

#[test]
fn unterminated_and_huge_inputs() {
    for bad in [
        &b"<MIFFile 2019>\n<Para <ParaLine <String `never ends>>>"[..],
        b"<MIFFile 2019>\n<String `ends with backslash\\",
        b"<MIFFile 2019>\n`",
        b"<MIFFile 2019>\n'",
        b"<MIFFile 2019>\n<",
        b"<MIFFile 2019>\n< 9>",
        b"<MIFFile 2019>\n<A <B <C",
        b"<MIFFile 2019>\n# comment without newline",
        b"<MIFFile ",
        b"<MIFFile 2019",
        b"",
        b"\xef\xbb\xbf",
        b"<MIFFile 2019>\n\xff\xfe",
    ] {
        let r = extract(bad);
        if let Ok(d) = &r {
            // a lone comment is fine; it must then merge back exactly
            assert!(Mif.merge(d).unwrap() == bad);
        }
    }
    // over the input limit
    let mut o = opts();
    o.limits = Some(Limits { max_input_bytes: 100, ..Limits::default() });
    let e = Mif.extract(SUITE, &o).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::InputBytes), "{e}");
    // a statement flood and a unit flood
    let mut flood = b"<MIFFile 2019>\n".to_vec();
    flood.extend(b"<X>".repeat(50_000 * 200 + 1));
    let e = extract(&flood).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlElements), "{e}");
    // a million tiny paragraphs under raised limits: no quadratic work
    let mut o = opts();
    o.limits = Some(Limits { max_units: 2_000_000, ..Limits::default() });
    let mut big = b"<MIFFile 2019>\n<TextFlow\n".to_vec();
    for i in 0..30_000 {
        big.extend(format!(" <Para <PgfTag `B'><ParaLine <String `Word {i}'>>>\n").as_bytes());
    }
    big.extend(b">\n");
    let t0 = std::time::Instant::now();
    let d = Mif.extract(&big, &o).unwrap();
    assert_eq!(d.units.len(), 30_000);
    assert!(Mif.merge(&d).unwrap() == big);
    assert!(t0.elapsed().as_secs() < 20);
}

#[test]
fn lines_of_equals_signs_stay_linear() {
    // facet detection must not rescan the line for every token
    let mut d = b"<MIFFile 2019>\n<Para <ParaLine <String `Hello there'>>>\n".to_vec();
    for _ in 0..200_000 {
        d.extend(b"=a &b ");
    }
    d.push(b'\n');
    let t0 = std::time::Instant::now();
    let doc = extract(&d).unwrap();
    assert_eq!(doc.units.len(), 1);
    assert!(t0.elapsed().as_secs() < 5);
}

fn noise() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just(b"<Para ".to_vec()),
        Just(b"<ParaLine ".to_vec()),
        Just(b"<String `".to_vec()),
        Just(b"<Char Tab>".to_vec()),
        Just(b"<Char HardReturn>".to_vec()),
        Just(b"<Font <FTag `Em'>>".to_vec()),
        Just(b"<Font <FTag `'>>".to_vec()),
        Just(b"<XRef <XRefName `x'>>".to_vec()),
        Just(b"<XRefEnd >".to_vec()),
        Just(b"<PgfTag `Body'>".to_vec()),
        Just(b"<TextRectID 3>".to_vec()),
        Just(b"<Variable <VariableName `v'>>".to_vec()),
        Just(b"'>".to_vec()),
        Just(b">".to_vec()),
        Just(b"\\>".to_vec()),
        Just(b"\\x8e ".to_vec()),
        Just(b"\\u00e9".to_vec()),
        Just(b"# c\n".to_vec()),
        Just(b"\n=facet\n".to_vec()),
        Just(b" ".to_vec()),
        Just("é日本\u{27e6}".as_bytes().to_vec()),
        "[ -~]{0,6}".prop_map(|s| s.into_bytes()),
    ];
    prop::collection::vec(piece, 0..40).prop_map(|v| {
        let mut d = b"<MIFFile 2019>\n".to_vec();
        for p in v {
            d.extend(p);
        }
        d
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(600))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        check(&data);
    }

    #[test]
    fn random_bytes_after_a_header_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        let mut d = b"<MIFFile 2019>\n".to_vec();
        d.extend(data);
        check(&d);
    }

    #[test]
    fn mif_shaped_noise_never_panics(d in noise()) {
        check(&d);
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..6, edits in prop::collection::vec((any::<usize>(), any::<u8>()), 1..6)) {
        let fx = fixtures();
        let mut d = fx[which % fx.len()].clone();
        for (i, b) in edits {
            if !d.is_empty() {
                let n = d.len();
                d[i % n] = b;
            }
        }
        check(&d);
    }

    #[test]
    fn string_escapes_round_trip(s in "\\PC{0,60}") {
        let enc = bilingua_mif::encode_string(&s);
        let back = bilingua_mif::decode_string(&enc);
        let want: String = s.chars().filter(|c| (*c as u32) >= 0x20 && *c != '\u{7f}' || *c == '\t').collect();
        prop_assert_eq!(back, want);
        // an encoded string never holds a bare terminator
        prop_assert!(!enc.contains('\'') && !enc.contains('`') && !enc.replace("\\>", "").contains('>'));
    }
}
