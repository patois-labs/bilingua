//! Hostile input: typed errors and no panics, over deep nesting, entity bombs, odd encodings, random bytes, random
//! structured XML and mutated fixtures. Whatever extracts must merge back unchanged, and a pseudo-translated merge
//! must read back with the same units.

mod common;

use bilingua_icml::Icml;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn seeds() -> Vec<Vec<u8>> {
    let footnote = format!("<Footnote>{}</Footnote>", psr(&csr("<Content><?ACE 4?> Footnote text.</Content>")));
    let table = format!("<Table Self=\"t1\"><Cell Self=\"c1\">{}</Cell><Cell Self=\"c2\">{}</Cell></Table>", psr(&csr("<Content>Cell A</Content>")), psr(&csr("<Content>Cell B</Content>")));
    let body = format!(
        "{}\n{}",
        psr(&format!(
            "{}{}",
            csr(&format!("<Content>Page\t<?ACE 18?> see </Content><HyperlinkTextSource Self=\"h1\"><Content>our site</Content></HyperlinkTextSource><PageReference Self=\"p1\"/>{footnote}")),
            csr_with("<Content>bold</Content><Br/><Content>next</Content>", BOLD, " AppliedLanguage=\"$ID/English: USA\"")
        )),
        psr(&csr(&table))
    );
    let a = icml(&body, "\t<Language Self=\"L\" Name=\"$ID/English: USA\"/>\n\t<Condition Self=\"Condition/X\" Visible=\"false\"/>\n", "");
    let wc = include_bytes!("data/okapi_Test01.wcml").to_vec();
    let small = icml(&psr(&csr("<Content>Hello &amp; goodbye</Content>")), "", "");
    vec![a, wc, small]
}

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the
/// same unit count, whatever the target language.
fn check(data: &[u8]) {
    let _ = Icml.sniff(data);
    for lang in [None, Some("ar"), Some("de-DE")] {
        let o = ExtractOptions { target_lang: lang.map(str::to_string), granularity: Granularity::Paragraph, ..Default::default() };
        let Ok(d) = Icml.extract(data, &o) else { return };
        assert!(Icml.merge(&d).expect("merge") == data, "untouched merge");
        let mut e = d.clone();
        for u in &mut e.units {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            u.segments[0].target = t;
        }
        let out = Icml.merge(&e).expect("pseudo merge");
        let back = Icml.extract(&out, &o).expect("re-extract");
        assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
    }
}

#[test]
fn seeds_pass() {
    for s in seeds() {
        check(&s);
    }
}

#[test]
fn deep_nesting_is_a_typed_error() {
    for n in [300usize, 5_000, 200_000] {
        let doc = format!("<Document><Story>{}x{}</Story></Document>", "<a>".repeat(n), "</a>".repeat(n));
        match Icml.extract(doc.as_bytes(), &opts()) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlDepth),
            other => panic!("depth {n}: {other:?}"),
        }
    }
    // a caller who raises the limit still gets a refusal, never a stack overflow
    let n = 100_000;
    let doc = format!("<Document><Story>{}x{}</Story></Document>", "<a>".repeat(n), "</a>".repeat(n));
    let lim = Limits { max_xml_depth: 10_000_000, ..Limits::default() };
    assert!(Icml.extract_with(doc.as_bytes(), &opts(), &lim).is_err());
    // nesting that is allowed works
    let ok = icml(&psr(&csr(&format!("<Content>deep</Content>{}", "<Note>".repeat(50) + &"</Note>".repeat(50)))), "", "");
    assert!(Icml.extract(&ok, &opts()).is_ok());
}

#[test]
fn entity_bombs_and_doctypes_are_refused() {
    let bomb = "<?xml version=\"1.0\"?><!DOCTYPE l [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\"><!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\">]><Document><Story><ParagraphStyleRange><CharacterStyleRange><Content>&c;</Content></CharacterStyleRange></ParagraphStyleRange></Story></Document>";
    assert!(matches!(Icml.extract(bomb.as_bytes(), &opts()), Err(Error::Limit(_))));
    let plain = "<?xml version=\"1.0\"?><!DOCTYPE Document SYSTEM \"x.dtd\"><Document><Story/></Document>";
    assert!(matches!(Icml.extract(plain.as_bytes(), &opts()), Err(Error::Parse { .. })));
    let ext = "<?xml version=\"1.0\"?><!DOCTYPE d [<!ENTITY x SYSTEM \"http://example.com/x\">]><Document><Story>&x;</Story></Document>";
    assert!(Icml.extract(ext.as_bytes(), &opts()).is_err());
    // an undefined entity, a lone ampersand and a character reference to NUL
    for bad in ["&nbsp;", "&", "&#0;", "&#xD800;", "&#99999999;"] {
        let doc = icml(&psr(&csr(&format!("<Content>{bad}</Content>"))), "", "");
        assert!(matches!(Icml.extract(&doc, &opts()), Err(Error::Parse { .. })), "{bad}");
    }
}

#[test]
fn oddities_are_typed_errors() {
    let good = icml(&psr(&csr("<Content>Hello</Content>")), "", "");
    let s = String::from_utf8(good.clone()).unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("only a BOM", vec![0xEF, 0xBB, 0xBF]),
        ("NUL bytes", vec![0; 64]),
        ("UTF-32", vec![0, 0, 0xFE, 0xFF, 0, 0, 0, b'<']),
        ("xml decl twice", s.replacen("<Document", "<?xml version=\"1.0\"?><Document", 1).into_bytes()),
        ("mismatched tag", s.replace("</Content>", "</Contents>").into_bytes()),
        ("attribute without quotes", s.replace("Self=\"u1\"", "Self=u1").into_bytes()),
        ("duplicate attribute", s.replace("Self=\"u1\"", "Self=\"u1\" Self=\"u2\"").into_bytes()),
        ("undeclared prefix", s.replace("<Content>", "<x:Content>").replace("</Content>", "</x:Content>").into_bytes()),
        ("lt in attribute", s.replace("Self=\"u1\"", "Self=\"u<1\"").into_bytes()),
        ("text after root", [good.clone(), b"junk".to_vec()].concat()),
        ("two roots", [good.clone(), good.clone()].concat()),
        ("control char", s.replace("Hello", "Hel\u{1}lo").into_bytes()),
        ("fffe char", s.replace("Hello", "Hel\u{fffe}lo").into_bytes()),
        ("cdata end in text", s.replace("Hello", "a]]>b").into_bytes()),
        ("comment dashes", s.replace("<Document", "<!-- a -- b --><Document").into_bytes()),
        ("pi named xml", s.replace("<Document", "<?xml-stylesheet x?><?XML x?><Document").into_bytes()),
        ("bad name", s.replace("<Content>", "<1Content>").into_bytes()),
        ("invalid utf-8", [&good[..40], &[0xC3, 0x28], &good[40..]].concat()),
        ("lone surrogate bytes", [&good[..40], &[0xED, 0xA0, 0x80], &good[40..]].concat()),
    ];
    for (name, bytes) in cases {
        match Icml.extract(&bytes, &opts()) {
            Err(_) => {}
            Ok(_) => panic!("{name}: must be refused"),
        }
    }
    // a huge attribute list and a long name are fine or refused, never a panic
    let attrs: String = (0..20_000).map(|i| format!(" a{i}=\"v\"")).collect();
    let big = s.replace("<Color ", &format!("<Color{attrs} "));
    let _ = Icml.extract(big.as_bytes(), &opts());
    let long = format!("<Document><Story><{n}/></Story></Document>", n = "n".repeat(1_000_000));
    let _ = Icml.extract(long.as_bytes(), &opts());
    // oversized input
    let lim = Limits { max_input_bytes: 100, ..Limits::default() };
    assert!(matches!(Icml.extract_with(&good, &opts(), &lim), Err(Error::Limit(_))));
}

#[test]
fn many_paragraphs_in_one_range_are_fast_enough() {
    // 20 000 breaks in a single range: the normalisation step must not be quadratic
    let content: String = (0..20_000).map(|i| format!("<Content>p{i}</Content><Br/>")).collect();
    let data = icml(&psr(&csr(&content)), "", "");
    let t = std::time::Instant::now();
    let mut d = Icml.extract(&data, &opts()).unwrap();
    assert_eq!(d.units.len(), 20_000);
    for u in d.units.iter_mut().step_by(1000) {
        u.segments[0].target = vec![Inline::text("x")];
    }
    let out = Icml.merge(&d).unwrap();
    assert!(well_formed(&out));
    assert!(t.elapsed().as_secs() < 120, "{:?}", t.elapsed());
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<Document DOMVersion=\"20.0\" Self=\"d\">".to_string()),
        Just("</Document>".to_string()),
        Just("<Story Self=\"s\">".to_string()),
        Just("</Story>".to_string()),
        Just("<ParagraphStyleRange AppliedParagraphStyle=\"P\">".to_string()),
        Just("</ParagraphStyleRange>".to_string()),
        Just("<CharacterStyleRange AppliedCharacterStyle=\"C\">".to_string()),
        Just("<CharacterStyleRange AppliedConditions=\"Condition/X\">".to_string()),
        Just("</CharacterStyleRange>".to_string()),
        Just("<Content>".to_string()),
        Just("</Content>".to_string()),
        Just("<Br/>".to_string()),
        Just("<Properties><X/></Properties>".to_string()),
        Just("<HyperlinkTextSource Self=\"h\">".to_string()),
        Just("</HyperlinkTextSource>".to_string()),
        Just("<Footnote>".to_string()),
        Just("</Footnote>".to_string()),
        Just("<Table><Cell>".to_string()),
        Just("</Cell></Table>".to_string()),
        Just("<Change ChangeType=\"InsertedText\">".to_string()),
        Just("</Change>".to_string()),
        Just("<PageReference/>".to_string()),
        Just("<Condition Self=\"Condition/X\" Visible=\"false\"/>".to_string()),
        Just("<Language Name=\"$ID/de_DE_2006\"/>".to_string()),
        Just("<ea:Stories xmlns:ea=\"urn:SmartConnection_v3\">".to_string()),
        Just("</ea:Stories>".to_string()),
        Just("<ea:Story>".to_string()),
        Just("</ea:Story>".to_string()),
        Just("<!--".to_string()),
        Just("-->".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("<?ACE 7?>".to_string()),
        Just("<?xml version=\"1.0\"?>".to_string()),
        Just("<!DOCTYPE a [<!ENTITY e \"x\">]>".to_string()),
        Just("&amp;".to_string()),
        Just("&#x41;".to_string()),
        Just("&e;".to_string()),
        Just("&".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just("\t".to_string()),
        Just("\u{2028}".to_string()),
        Just(" ".to_string()),
        Just("\r\n".to_string()),
        Just("Hello".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| v.concat().into_bytes())
}

fn mutated() -> impl Strategy<Value = Vec<u8>> {
    let n = seeds().len();
    (0..n, prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..4), 0..8)).prop_map(move |(k, edits)| {
        let mut d = seeds()[k].clone();
        for (idx, byte, op) in edits {
            if d.is_empty() {
                break;
            }
            let i = idx.index(d.len());
            match op {
                0 => d[i] = byte,
                1 => {
                    d.remove(i);
                }
                2 => d.insert(i, byte),
                _ => d.truncate(i),
            }
        }
        d
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..2000)) {
        check(&data);
    }

    #[test]
    fn random_xml_soup_never_panics(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(data in mutated()) {
        check(&data);
    }
}
