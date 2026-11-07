//! Hostile input through the DITA and DocBook filters: typed errors and no panics over random bytes, DITA / DocBook
//! soup, mutated fixtures and every prefix of a fixture. Whatever extracts must merge back to the same bytes
//! untouched, and a pseudo-translated merge must read back with the same units and tags. DTDs are never fetched.

mod common;

use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn seeds() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = ["task.dita", "guide.ditamap", "book5.dbk", "book45.xml"].iter().map(|n| data(n)).collect();
    v.extend(corpus().into_iter().map(|(_, b)| b));
    v
}

fn check_with(f: &dyn Filter, data: &[u8]) {
    let _ = f.sniff(data);
    let Ok(d) = f.extract(data, &opts(None)) else { return };
    let m = f.merge(&d).expect("merge");
    let exact = std::str::from_utf8(data).is_ok() || data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]);
    assert!(!exact || m == data, "untouched merge differs\n in {:?}\nout {:?}", String::from_utf8_lossy(data), String::from_utf8_lossy(&m));
    for target in [None, Some("de-DE"), Some("ar")] {
        let mut e = d.clone();
        e.target_lang = target.map(String::from);
        pseudo(&mut e);
        let out = f.merge(&e).unwrap_or_else(|er| panic!("pseudo merge failed: {er}"));
        let back = f.extract(&out, &opts(None)).unwrap_or_else(|er| panic!("merged file does not read back: {er}\n{}", String::from_utf8_lossy(&out)));
        assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
        assert_eq!(tag_count(&back), tag_count(&d), "tag count after translation");
    }
}

fn check(data: &[u8]) {
    check_with(&Dita::new(), data);
    check_with(&DocBook::new(), data);
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
        Just("<!DOCTYPE task PUBLIC \"-//OASIS//DTD DITA Task//EN\" \"task.dtd\">"),
        Just("<!DOCTYPE book PUBLIC \"-//OASIS//DTD DocBook XML V4.5//EN\" \"http://127.0.0.1:9/docbookx.dtd\">"),
        Just("<!DOCTYPE t [<!ENTITY e SYSTEM \"file:///etc/passwd\">]>"),
        Just("<topic id=\"t\">"),
        Just("</topic>"),
        Just("<map title=\"M\">"),
        Just("<topicref href=\"a.dita\" navtitle=\"Nav\"/>"),
        Just("<article xmlns=\"http://docbook.org/ns/docbook\" version=\"5.0\">"),
        Just("<book lang=\"en\">"),
        Just("<title>"),
        Just("</title>"),
        Just("<p>"),
        Just("</p>"),
        Just("<para>"),
        Just("</para>"),
        Just("<p conref=\"a.dita#a/b\"/>"),
        Just("<p conkeyref=\"k/p\">x</p>"),
        Just("<keyword keyref=\"k\"/>"),
        Just("<p translate=\"no\">"),
        Just("<ph translate=\"yes\">"),
        Just("</ph>"),
        Just("<ph>"),
        Just("<b>"),
        Just("</b>"),
        Just("<codeph>x {0}</codeph>"),
        Just("<codeblock xml:space=\"preserve\">a\n b</codeblock>"),
        Just("<fn>"),
        Just("</fn>"),
        Just("<indexterm>t</indexterm>"),
        Just("<footnote><para>f</para></footnote>"),
        Just("<emphasis>"),
        Just("</emphasis>"),
        Just("<programlisting>p</programlisting>"),
        Just("<xref href=\"x.dita\"/>"),
        Just("<image href=\"i.png\" alt=\"Alt\"/>"),
        Just("<draft-comment>d</draft-comment>"),
        Just("<![CDATA[a < b]]>"),
        Just("<!-- c -->"),
        Just("<?pi x?>"),
        Just("&product;"),
        Just("&amp;"),
        Just("&#x65E5;"),
        Just("&"),
        Just("<"),
        Just("\""),
        Just(" "),
        Just("\r\n"),
        Just("text"),
        Just("caf\u{e9}"),
        Just("\u{645}\u{631}"),
        Just("\u{1f600}"),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..600)) {
        check(&data);
    }

    #[test]
    fn dita_and_docbook_soup_round_trips(data in soup()) {
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
fn dtds_are_never_fetched_and_internal_subsets_are_refused() {
    // DITA and DocBook files name a DTD by public id and URL; the filter reads them without fetching anything (the
    // URLs below do not answer, and this crate has no network or file access at all: it builds for wasm32)
    for f in [&Dita::new() as &dyn Filter, &DocBook::new()] {
        for dt in [
            "<!DOCTYPE topic PUBLIC \"-//OASIS//DTD DITA Topic//EN\" \"topic.dtd\">",
            "<!DOCTYPE topic PUBLIC \"-//OASIS//DTD DITA Topic//EN\" \"http://127.0.0.1:9/topic.dtd\">",
            "<!DOCTYPE topic SYSTEM \"file:///etc/passwd\">",
            "<!DOCTYPE book PUBLIC \"-//OASIS//DTD DocBook XML V4.5//EN\" \"http://www.oasis-open.org/docbook/xml/4.5/docbookx.dtd\">",
        ] {
            let doc = format!("{dt}<topic id=\"t\"><title>A &ent; and &amp; B</title></topic>");
            let d = ex(f, doc.as_bytes());
            assert_eq!(show(&d.units[0]), "A \u{27E6}x1\u{27E7} and & B", "an entity of the DTD stays a code");
            assert_eq!(f.merge(&d).unwrap(), doc.as_bytes());
        }
        // an internal subset (entity bombs, external entities, parameter entities) is kept as it is: nothing is
        // expanded or fetched, an entity stays a code and the file is written back unchanged
        for raw in [
            &br#"<!DOCTYPE topic [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">]><topic><title>&b;</title></topic>"#[..],
            br#"<!DOCTYPE topic PUBLIC "-//OASIS//DTD DITA Topic//EN" "topic.dtd" [<!ENTITY x SYSTEM "http://127.0.0.1:9/x">]><topic><title>&x;</title></topic>"#,
            br#"<!DOCTYPE topic [<!ENTITY % p SYSTEM "http://127.0.0.1:9/p.dtd"> %p;]><topic/>"#,
            br#"<!DOCTYPE book [<!ELEMENT book ANY>]><book><para>x</para></book>"#,
        ] {
            let d = f.extract(raw, &opts(None)).unwrap_or_else(|e| panic!("{}: {e}", String::from_utf8_lossy(raw)));
            assert_eq!(f.merge(&d).unwrap(), raw, "untouched = as uploaded");
        }
        // XInclude is not followed: the element stays markup
        let xi = br#"<topic id="t" xmlns:xi="http://www.w3.org/2001/XInclude"><title>T <xi:include href="file:///etc/passwd"/></title></topic>"#;
        let d = ex(f, xi);
        assert!(!show(&d.units[0]).contains("root:"));
        assert_eq!(f.merge(&d).unwrap(), xi);
    }
}

#[test]
fn malformed_truncated_deep_and_huge_files_are_typed_errors() {
    let f = Dita::new();
    let task = data("task.dita");
    for cut in [10, 100, 300, task.len() - 3] {
        assert!(matches!(f.extract(&task[..cut], &opts(None)), Err(Error::Parse { .. })), "cut at {cut}");
    }
    for bad in [&b""[..], b"DITA", b"<topic>", b"<topic></map>", b"<topic a=1/>", b"<topic>&unknown;</topic>", b"<topic>\x01</topic>", b"<topic/><topic/>"] {
        assert!(matches!(f.extract(bad, &opts(None)), Err(Error::Parse { .. } | Error::Encoding(_) | Error::Limit(_))), "{:?}", String::from_utf8_lossy(bad));
    }
    let deep = format!("<topic>{}x{}</topic>", "<ph>".repeat(5000), "</ph>".repeat(5000));
    assert!(matches!(f.extract(deep.as_bytes(), &opts(None)), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth));
    // the default depth limit (256) holds for nested inline elements on a 1 MB stack (wasm's default), debug build
    let at_limit = |n: usize| format!("<topic><title>{}x{}</title></topic>", "<ph>".repeat(n), "</ph>".repeat(n));
    let ok = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            let f = Dita::new();
            let mut d = f.extract(at_limit(250).as_bytes(), &opts(None)).expect("250 levels");
            pseudo(&mut d);
            f.merge(&d).is_ok() && matches!(f.extract(at_limit(300).as_bytes(), &opts(None)), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth)
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(ok);
    // a caller who raises the limit: 1000 nested inline elements and 1000 nested footnotes extract and merge on the
    // same 1 MB stack (bilingua-xml walks the tree and merges with explicit stacks), and 60 000 levels are a typed
    // error at bilingua-xml's 1024-level cap
    let raised = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let f = Dita::new();
            let o = ExtractOptions::new().with_limits(Limits::new().with_max_xml_depth(1_000_000));
            for (open, close) in [("<ph>", "</ph>"), ("a <fn>", "</fn>")] {
                let src = format!("<topic><title>{}x{}</title></topic>", open.repeat(1000), close.repeat(1000));
                let mut d = f.extract(src.as_bytes(), &o).expect("1000 levels");
                assert_eq!(f.merge(&d).unwrap(), src.as_bytes());
                pseudo(&mut d);
                let n = d.units.len();
                assert_eq!(f.extract(&f.merge(&d).unwrap(), &o).unwrap().units.len(), n);
            }
            let deeper = format!("<topic>{}x{}</topic>", "<ph>".repeat(60_000), "</ph>".repeat(60_000));
            matches!(f.extract(deeper.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth)
        })
        .unwrap()
        .join()
        .unwrap();
    assert!(raised, "typed error when the caller raises the limit");
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(100));
    assert!(matches!(f.extract(&task, &small), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    let long = format!("<topic><title>{}</title></topic>", "word ".repeat(30_000));
    assert!(matches!(f.extract(long.as_bytes(), &opts(None)), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
    let wide = format!("<topic><body>{}</body></topic>", "<p/>".repeat(200_000));
    assert!(f.extract(wide.as_bytes(), &opts(None)).is_ok());
}

#[test]
fn unknown_format_options_are_refused() {
    let o = ExtractOptions::new().with_format_option("dita.nope", "1");
    assert!(matches!(Dita::new().extract(&data("task.dita"), &o), Err(Error::FormatOption { .. })));
    let o = ExtractOptions::new().with_format_option("docbook.nope", "1");
    assert!(matches!(DocBook::new().extract(&data("book5.dbk"), &o), Err(Error::FormatOption { .. })));
    // an option for another format is not ours to judge
    let o = ExtractOptions::new().with_format_option("po.something", "1");
    assert!(Dita::new().extract(&data("task.dita"), &o).is_ok());
}

#[test]
fn docbook4_file_with_an_entity_subset_and_latin1_opens() {
    // the shape of a DocBook 4 manual: public DOCTYPE with an internal subset (parameter entity, text entity,
    // include entity), ISO-8859-1 text. Nothing is fetched; entities stay codes; the bytes come back unchanged.
    let mut raw = b"<?xml version='1.0' encoding='ISO-8859-1'?>\n<!DOCTYPE refentry PUBLIC \"-//OASIS//DTD DocBook XML V4.2//EN\"\n  \"http://www.oasis-open.org/docbook/xml/4.2/docbookx.dtd\" [\n<!-- it's a comment with > and quotes -->\n<!ENTITY % defs SYSTEM \"defs.ent\"> %defs;\n<!ENTITY prog \"tool\">\n<!ENTITY part SYSTEM \"part.xml\">\n]>\n<refentry><refnamediv><refpurpose>Caf".to_vec();
    raw.push(0xE9);
    raw.extend_from_slice(b" for &prog; users</refpurpose></refnamediv>&part;</refentry>\n");
    for f in [&docbook() as &dyn Filter, &dita()] {
        let d = ex(f, &raw);
        assert_eq!(show(&d.units[0]), "Caf\u{e9} for \u{27E6}x1\u{27E7} users");
        assert_eq!(f.merge(&d).unwrap(), raw);
    }
}
