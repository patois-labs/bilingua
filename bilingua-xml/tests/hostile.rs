//! Hostile input: typed errors and no panics, over random bytes, structured soup, mutated fixtures and every prefix
//! of a fixture. Whatever extracts must merge back to the same bytes untouched, and a pseudo-translated merge must
//! read back with the same unit count and tags.

mod common;

use bilingua_model::*;
use bilingua_xml::XmlFilter;
use common::*;
use proptest::prelude::*;

fn seeds() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = ["task.dita", "guide.ditamap", "book5.dbk", "book45.xml", "catalog.xml", "catalog-attr.xml", "items-element.xml", "xpath-pi.xml"]
        .iter()
        .map(|n| data(n))
        .collect();
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/xml");
    if let Ok(rd) = std::fs::read_dir(corpus) {
        let mut names: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        names.sort();
        for p in names {
            v.push(std::fs::read(p).unwrap());
        }
    }
    v
}

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the same
/// unit count and tags.
fn check(data: &[u8]) {
    let f = XmlFilter::new();
    let _ = f.sniff(data);
    let _ = f.inspect(data);
    let Ok(d) = f.extract(data, &opts(None)) else { return };
    let m = f.merge(&d).expect("merge");
    // Bytes that are valid in no encoding but decode anyway (a damaged Shift_JIS file) need not come back byte for
    // byte: the model's legacy decoder is lossy for them. Everything UTF-8 or UTF-16 must.
    let exact = std::str::from_utf8(data).is_ok() || data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]);
    assert!(!exact || m == data, "untouched merge differs\n in {:?}\nout {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&m));
    for target in [None, Some("de-DE")] {
        let mut e = d.clone();
        e.target_lang = target.map(String::from);
        for u in &mut e.units {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            u.segments[0].target = t;
        }
        let out = f.merge(&e).unwrap_or_else(|er| panic!("pseudo merge failed: {er}"));
        let back = f.extract(&out, &opts(None)).unwrap_or_else(|er| panic!("merged file does not read back: {er}\n{}", String::from_utf8_lossy(&out)));
        if target.is_none() || !is_multilingual(&f, data) {
            assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
            assert_eq!(tag_count(&back), tag_count(&d), "tag count after translation");
        }
    }
}

fn is_multilingual(f: &XmlFilter, data: &[u8]) -> bool {
    f.inspect(data).map(|i| i.multilingual.is_some()).unwrap_or(false)
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<?xml version=\"1.0\"?>".to_string()),
        Just("<r>".to_string()),
        Just("</r>".to_string()),
        Just("<p>".to_string()),
        Just("</p>".to_string()),
        Just("<b>".to_string()),
        Just("</b>".to_string()),
        Just("<br/>".to_string()),
        Just("<p title=\"t\">".to_string()),
        Just("<p xml:space=\"preserve\">".to_string()),
        Just("<its:rules xmlns:its=\"http://www.w3.org/2005/11/its\" version=\"2.0\"><its:translateRule selector=\"//b\" translate=\"no\"/></its:rules>".to_string()),
        Just("<p its:translate=\"no\" xmlns:its=\"http://www.w3.org/2005/11/its\">".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("<!-- c -->".to_string()),
        Just("<!--".to_string()),
        Just("<?pi x?>".to_string()),
        Just("<!DOCTYPE r>".to_string()),
        Just("<!DOCTYPE r SYSTEM \"r.dtd\">".to_string()),
        Just("&amp;".to_string()),
        Just("&lt;".to_string()),
        Just("&#233;".to_string()),
        Just("&#x65E5;".to_string()),
        Just("&ent;".to_string()),
        Just("&".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just("\"".to_string()),
        Just("'".to_string()),
        Just("=".to_string()),
        Just(" ".to_string()),
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\t".to_string()),
        Just("text".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
        Just("<g><t xml:lang=\"en\">Hi</t><t xml:lang=\"de\">Hallo</t></g>".to_string()),
        Just("<g><en>Hi</en><fr>Salut</fr></g>".to_string()),
        Just("<topic id=\"x\">".to_string()),
        Just("<codeph>".to_string()),
        Just("<article xmlns=\"http://docbook.org/ns/docbook\">".to_string()),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn structured_soup_round_trips(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..64, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8), cut in prop::option::of(any::<prop::sample::Index>())) {
        let all = seeds();
        let mut data = all[seed % all.len()].clone();
        for (i, b) in flips {
            if !data.is_empty() {
                let at = i.index(data.len());
                data[at] = b;
            }
        }
        if let Some(c) = cut {
            data.truncate(c.index(data.len() + 1));
        }
        check(&data);
    }

    #[test]
    fn every_prefix_of_a_fixture_is_handled(seed in 0usize..64, cut in any::<prop::sample::Index>()) {
        let all = seeds();
        let d = &all[seed % all.len()];
        check(&d[..cut.index(d.len() + 1)]);
    }
}

#[test]
fn malformed_files_are_typed_errors() {
    let f = XmlFilter::new();
    for bad in [
        &b""[..],
        b"   ",
        b"not xml",
        b"<r>",
        b"<r></s>",
        b"<r><p></r></p>",
        b"<r/><r/>",
        b"<r a=1/>",
        b"<r a=\"1\" a=\"2\"/>",
        b"<r a=\"<\"/>",
        b"<r>&unknown;</r>",
        b"<r>&#0;</r>",
        b"<r>&#xD800;</r>",
        b"<r>a & b</r>",
        b"<r>]]></r>",
        b"<r><![CDATA[x</r>",
        b"<r><!-- x</r>",
        b"<r><!-- a -- b --></r>",
        b"<x:r/>",
        b"<r x:a=\"1\"/>",
        b"<r>\x01</r>",
        b"<1r/>",
        b"<r></r>text",
        b"text<r/>",
        b"<?xml version=\"1.0\"?>",
        b"<!DOCTYPE r><!DOCTYPE r><r/>",
    ] {
        match f.extract(bad, &opts(None)) {
            Err(Error::Parse { .. } | Error::Encoding(_) | Error::Limit(_)) => {}
            other => panic!("{:?} should be refused, got {:?}", String::from_utf8_lossy(bad), other.map(|d| d.units.len())),
        }
    }
}

#[test]
fn entity_declarations_are_refused() {
    let f = XmlFilter::new();
    let bomb = br#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;"><!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">]><lolz>&lol3;</lolz>"#;
    let xxe = br#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><r>&xxe;</r>"#;
    let param = br#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY % p SYSTEM "http://127.0.0.1:9/x.dtd"> %p;]><r/>"#;
    let in_attr = br#"<!DOCTYPE r SYSTEM "a>b" [<!ENTITY a "x">]><r y="&a;">hi</r>"#;
    for raw in [&bomb[..], &xxe[..], &param[..], &in_attr[..]] {
        assert!(matches!(f.extract(raw, &opts(None)), Err(Error::Limit(_) | Error::Parse { .. })), "{}", String::from_utf8_lossy(raw));
    }
    // entities in a UTF-16 file are found too
    let mut u16 = vec![0xFF, 0xFE];
    for c in String::from_utf8_lossy(&bomb[..]).encode_utf16() {
        u16.extend_from_slice(&c.to_le_bytes());
    }
    assert!(matches!(f.extract(&u16, &opts(None)), Err(Error::Limit(_))));
    // the five predefined entities and character references are all a document needs
    let ok = br#"<r>&amp;&lt;&gt;&quot;&apos;&#65;&#x42;</r>"#;
    assert_eq!(texts(&f, ok), vec![r#"&<>"'AB"#]);
}

#[test]
fn deep_wide_and_huge_files_hit_the_limits() {
    let f = XmlFilter::new();
    let deep = format!("{}x{}", "<a>".repeat(5000), "</a>".repeat(5000));
    assert!(matches!(f.extract(deep.as_bytes(), &opts(None)), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth));
    // a caller who raises the depth limit still gets a typed error, never a stack overflow
    let o = ExtractOptions { limits: Some(Limits { max_xml_depth: 1_000_000, ..Limits::default() }), ..Default::default() };
    let deeper = format!("{}x{}", "<a>".repeat(60_000), "</a>".repeat(60_000));
    assert!(matches!(f.extract(deeper.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth));
    let attrs = format!("<r {}/>", (0..20_000).map(|i| format!("a{i}=\"v\"")).collect::<Vec<_>>().join(" "));
    assert!(f.extract(attrs.as_bytes(), &opts(None)).is_ok());
    let dup = format!("<r {}/>", (0..20_000).map(|_| "a=\"v\"".to_string()).collect::<Vec<_>>().join(" "));
    assert!(matches!(f.extract(dup.as_bytes(), &opts(None)), Err(Error::Parse { .. })));
    let small = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(f.extract(&[b'<'; 200], &small), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let long = format!("<r>{}</r>", "word ".repeat(30_000));
    assert!(matches!(f.extract(long.as_bytes(), &opts(None)), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
    // a long run of siblings and a long single text
    let wide = format!("<r>{}</r>", "<b/>".repeat(200_000));
    assert!(f.extract(wide.as_bytes(), &opts(None)).is_ok());
}

#[test]
fn hostile_xpath_rules_are_bounded() {
    let f = XmlFilter::new();
    let doc = |sel: &str| {
        format!(
            r#"<r xmlns:its="http://www.w3.org/2005/11/its"><its:rules version="2.0"><its:translateRule selector="{sel}" translate="no"/></its:rules>{}</r>"#,
            "<p>x</p>".repeat(50)
        )
    };
    // each of these is refused or runs within budget, never loops or panics
    for sel in ["//p[//p[//p]]", "//*/following::*", "//p[position() = last()]", "//*[count(*) &gt; 0]", "//p[contains(., 'x')][1]", "//p | //*", "(//p)[2]", "//p[1 div 0 = 1]", "//@*", "//text()", "id('a')", "//*[$v]"] {
        let _ = f.extract(doc(sel).as_bytes(), &opts(None));
    }
    let nested = format!("//p{}", "[x".repeat(40) + &"]".repeat(40));
    let _ = f.extract(doc(&nested).as_bytes(), &opts(None));
    let parens = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
    let _ = f.extract(doc(&parens).as_bytes(), &opts(None));
}

#[test]
fn undeclared_legacy_bytes_are_refused() {
    let f = XmlFilter::new();
    // 0xE9 alone is not UTF-8 and nothing says what it is: refused, not guessed
    assert!(matches!(f.extract(b"<r><p>caf\xe9</p></r>", &opts(None)), Err(Error::Encoding(_))));
    assert!(matches!(f.extract(b"<?xml version=\"1.0\"?><r>caf\xe9</r>", &opts(None)), Err(Error::Encoding(_))));
    assert!(f.extract(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><r>caf\xe9</r>", &opts(None)).is_ok());
}
