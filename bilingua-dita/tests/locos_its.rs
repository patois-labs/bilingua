//! Port of the Localization OS suite `tests/verify_xml_its_filter.py` (12 cases) through the `dita` and `docbook`
//! filters. Six cases are DITA / DocBook cases in LocOS too (registration, identity, topic, map, DocBook, token
//! damage); the other six (in-file ITS rules, a user rules file, placeholders and CDATA, encodings, hostile input,
//! the language stamp) are generic-XML cases in LocOS, run here on DITA / DocBook documents, since LocOS's `dita` and
//! `docbook` filters are the same `ItsXmlFilter`. `verify_xml_resource_filters.py` has no DITA or DocBook case.

mod common;

use bilingua_dita::ItsRules;
use bilingua_model::*;
use common::*;

fn dita() -> Dita {
    Dita::new()
}

fn docbook() -> DocBook {
    DocBook::new()
}

#[test]
fn registration() {
    // LocOS: dita, ditamap, bookmap resolve to `dita`; dbk and docbook to `docbook`
    assert_eq!(dita().id(), "dita");
    assert_eq!(dita().extensions(), &["dita", "ditamap", "bookmap"]);
    assert_eq!(docbook().id(), "docbook");
    assert_eq!(docbook().extensions(), &["dbk", "docbook"]);
    let ids: Vec<&str> = bilingua_dita::filters().iter().map(|f| f.id()).collect();
    assert_eq!(ids, vec!["dita", "docbook"]);
    assert!(!dita().bilingual() && !docbook().bilingual());
}

#[test]
fn identity_all_fixtures() {
    // (file, filter, what the root language becomes when an echo merge stamps fr-FR)
    let cases: [(&str, Box<dyn Filter>, &str); 4] =
        [("task.dita", Box::new(dita()), "en-US"), ("guide.ditamap", Box::new(dita()), "+"), ("book5.dbk", Box::new(docbook()), "en"), ("book45.xml", Box::new(docbook()), "en")];
    for (name, f, lang) in cases {
        let f = f.as_ref();
        let d = data(name);
        let doc = ex(f, &d);
        assert!(!doc.units.is_empty(), "{name}");
        assert_eq!(s(&f.merge(&doc).unwrap()), s(&d), "{name}: untouched merge");
        assert_eq!(s(&echo(f, &d, None)), s(&d), "{name}: echo merge");
        assert_eq!(s(&f.merge(&ex_to(f, &d, "fr-FR")).unwrap()), s(&d), "{name}: untouched merge with a target language");
        let e = s(&echo(f, &d, Some("fr-FR")));
        let back = match lang {
            "+" => e.replacen(" xml:lang=\"fr-FR\"", "", 1), // DITA adds the language when the root has none
            l => e.replacen("fr-FR", l, 1),
        };
        assert_eq!(back, s(&d), "{name}: echo merge with a target language changes only the root language");
    }
}

#[test]
fn dita_topic() {
    let d = data("task.dita");
    let f = dita();
    assert_eq!(f.engine().inspect(&d).unwrap().dialect.as_deref(), Some("dita"));
    let doc = ex(&f, &d);
    let t: Vec<String> = doc.units.iter().map(show).collect();
    assert_eq!(
        t,
        vec![
            "Printing a \u{27E6}g1\u{27E7}report\u{27E6}/g1\u{27E7}",
            "Send the report to a printer.",
            "Use the \u{27E6}x1\u{27E7} command or see \u{27E6}x2\u{27E7}.\u{27E6}x3\u{27E7}",
            "printing\u{27E6}x1\u{27E7}",
            "reports",
            "Press \u{27E6}g1\u{27E7}Print\u{27E6}/g1\u{27E7} & wait <5> seconds for \u{27E6}x2\u{27E7}.",
            "Click \u{27E6}g1\u{27E7}OK\u{27E6}/g1\u{27E7}.",
            "A laser printer",
        ]
    );
    let joined = t.join(" ");
    for hidden in ["lpr", "Internal build", "Check with legal", "warning", "report.txt"] {
        assert!(!joined.contains(hidden), "{hidden} must not be extracted");
    }
    assert_eq!(doc.units.last().unwrap().context.style.as_deref(), Some("a11y:img-alt"));

    let out = s(&translate_all(&f, &d, Some("fr-FR")));
    well_formed(&f, out.as_bytes());
    assert!(out.contains(r#"<task id="print" xml:lang="fr-FR">"#), "{out}");
    assert!(out.contains(r#"<!DOCTYPE task PUBLIC "-//OASIS//DTD DITA Task//EN" "task.dtd">"#));
    assert!(out.contains("<!-- authoring note: keep this comment -->"));
    assert!(out.contains("<codeph>lpr -P {0}</codeph>"), "codeph content untouched");
    assert!(out.contains("<codeblock xml:space=\"preserve\">lpr -P office\n  report.txt</codeblock>"));
    assert!(out.contains(r#"<p conref="shared.dita#shared/warning"/>"#));
    assert!(out.contains(r#"<p translate="no">Internal build 42</p>"#));
    assert!(out.contains("<draft-comment>Check with legal</draft-comment>"));
    assert!(!out.contains("FR:Press <uicontrol>FR:Print</uicontrol>"), "tokens are not prefixed");
    assert!(out.contains("<uicontrol>Print</uicontrol> &amp; w\u{e0}it &lt;5&gt; seconds for &product;."), "{out}");
    assert!(out.contains("<indexterm>FR:printing<indexterm>FR:reports</indexterm></indexterm>"), "{out}");
    assert!(out.contains("alt=\"FR:A l\u{e0}ser printer\""));
    assert!(out.contains(r#"<xref href="other.dita"/>"#));
}

#[test]
fn dita_map() {
    let d = data("guide.ditamap");
    let f = dita();
    assert_eq!(texts(&f, &d), vec!["User Guide", "User Guide", "Everything about printing.", "Printing a report", "Printing", "Setting up & testing"]);
    let out = s(&translate_all(&f, &d, None));
    well_formed(&f, out.as_bytes());
    assert!(out.contains("navtitle=\"FR:Printing \u{e0} report\""), "{out}");
    assert!(out.contains("navtitle='FR:Setting up &amp; testing'"), "quote style kept, & escaped: {out}");
    assert!(out.contains("href=\"task.dita\"") && out.contains("title=\"FR:User Guide\""));
}

#[test]
fn docbook_5_and_45() {
    let d = data("book5.dbk");
    let f = docbook();
    assert_eq!(
        texts(&f, &d),
        vec![
            "Getting started",
            "Install",
            "Run the \u{27E6}g1\u{27E7}installer\u{27E6}/g1\u{27E7}, then read \u{27E6}x2\u{27E7}.\u{27E6}x3\u{27E7}",
            "Admin rights needed.",
            "Type \u{27E6}x1\u{27E7} when asked.",
        ]
    );
    let out = s(&translate_all(&f, &d, Some("de-DE")));
    well_formed(&f, out.as_bytes());
    assert!(out.contains(r#"xml:lang="de-DE""#), "{out}");
    assert!(out.contains(r#"<programlisting language="sh">./install.sh --prefix /opt</programlisting>"#));
    assert!(out.contains("<remark>TODO: screenshots</remark>"));
    assert!(out.contains("<emphasis role=\"strong\">inst\u{e0}ller</emphasis>"), "{out}");
    assert!(out.contains("<footnote><para>FR:Admin rights needed.</para></footnote>"));

    // DocBook 4.5: the docbook filter, and the generic XML engine recognising it from the DOCTYPE
    let d45 = data("book45.xml");
    assert_eq!(bilingua_xml::XmlFilter::new().inspect(&d45).unwrap().dialect.as_deref(), Some("docbook"), "DocBook 4.5 sniffed from the DOCTYPE");
    assert!(f.sniff(&d45));
    assert_eq!(texts(&f, &d45).last().unwrap(), "Back up \u{27E6}x1\u{27E7} every \u{27E6}g2\u{27E7}day\u{27E6}/g2\u{27E7}.");
    let out45 = s(&translate_all(&f, &d45, Some("ja-JP")));
    assert!(out45.contains(r#"<book lang="ja-JP">"#), "DocBook 4.x uses lang: {out45}");
    assert!(out45.contains("<screen>$ app backup --all</screen>"));
    assert!(out45.contains("<filename>/etc/app.conf</filename>"));
}

#[test]
fn its_rules_in_document() {
    let d = br#"<topic id="t" xmlns:its="http://www.w3.org/2005/11/its"><its:rules version="2.0"><its:translateRule selector="//shortdesc" translate="no"/><its:translateRule selector="//p/@outputclass" translate="yes"/><its:withinTextRule selector="//cite" withinText="nested"/><its:locNoteRule selector="//title"><its:locNote>Max 40 characters.</its:locNote></its:locNoteRule><its:preserveSpaceRule selector="//lq" space="preserve"/></its:rules><title>Desk lamp</title><shortdesc>Hidden</shortdesc><body><p outputclass="Label">A <b>bright</b> lamp<cite>Ref</cite> here.</p><p its:translate="no">Made in EU</p><lq>Line one
    indented   line</lq></body></topic>"#;
    let f = dita();
    let doc = ex(&f, d);
    let t: Vec<String> = doc.units.iter().map(show).collect();
    assert_eq!(t, vec!["Desk lamp", "Label", "A \u{27E6}g1\u{27E7}bright\u{27E6}/g1\u{27E7} lamp\u{27E6}x2\u{27E7} here.", "Ref", "Line one\n    indented   line"]);
    assert_eq!(doc.units[0].notes[0].text, "Max 40 characters.");
    assert_eq!(doc.units[1].context.style.as_deref(), Some("attr:outputclass"));
    let out = s(&translate_all(&f, d, None));
    well_formed(&f, out.as_bytes());
    assert!(out.contains("<shortdesc>Hidden</shortdesc>") && out.contains(r#"<p its:translate="no">Made in EU</p>"#));
    assert!(out.contains("outputclass=\"FR:L\u{e0}bel\""), "{out}");
    assert!(out.contains("FR:Line one\n    indented   line"), "preserved white space untouched");
    assert!(out.contains("<its:locNote>Max 40 characters.</its:locNote>"));
}

#[test]
fn user_rules_file() {
    let rules = ItsRules::parse(
        br#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//note" translate="no"/><its:translateRule selector="//section/@outputclass" translate="yes"/><its:withinTextRule selector="//p" withinText="yes"/></its:rules>"#,
    )
    .unwrap();
    let d = br#"<topic id="t"><title>T</title><body><section outputclass="Hello"><title>A <p>big</p> day</title><note>skip me</note></section></body></topic>"#;
    assert_eq!(texts(&Dita::new().with_its_rules(rules.clone()), d), vec!["T", "Hello", "A \u{27E6}g1\u{27E7}big\u{27E6}/g1\u{27E7} day"]);
    assert_eq!(texts(&dita(), d), vec!["T", "A", "big", "day", "skip me"]);
    // the user rules come after the DocBook set too: here they protect a paragraph role
    let r2 = ItsRules::parse(br#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//para[@role='sku']" translate="no"/></its:rules>"#).unwrap();
    let db = br#"<article><para role="sku">LB-1</para><para>Bread</para></article>"#;
    assert_eq!(texts(&DocBook::new().with_its_rules(r2), db), vec!["Bread"]);
    // unsafe rules are refused when read (the full list is in bilingua-xml's port)
    for (sel, needle) in [("//a[", "invalid XPath"), ("//*/following::*", "following and preceding"), ("//a[b[c[d]]]", "two deep")] {
        let raw = format!(r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="{sel}" translate="no"/></its:rules>"#);
        match ItsRules::parse(raw.as_bytes()) {
            Err(e) => assert!(e.to_string().contains(needle), "{needle} not in {e}"),
            Ok(_) => panic!("{sel} accepted"),
        }
    }
}

#[test]
fn placeholders_and_cdata() {
    let f = dita();
    let d = b"<topic id=\"t\"><title>Hello {0}, you have %1$s new {{count}} items</title><body><p><![CDATA[Use <b> here]]></p></body></topic>";
    assert_eq!(texts(&f, d), vec!["Hello {0}, you have %1$s new {{count}} items", "Use <b> here"]);
    let out = s(&merge_map(&f, d, None, &[(0, "Hallo {0}, du hast %1$s neue {{count}}"), (1, "Nimm <b> ]]> hier")]));
    assert!(out.contains("<title>Hallo {0}, du hast %1$s neue {{count}}</title>"));
    assert!(out.contains("<p><![CDATA[Nimm <b> ]]]]><![CDATA[> hier]]></p>"), "{out}");
    assert_eq!(texts(&f, out.as_bytes())[1], "Nimm <b> ]]> hier");
    // no free placeholders: LocOS keeps {0} and %s as text in XML
    assert!(ex(&f, d).units[0].source().iter().all(|i| matches!(i, Inline::Text(_))));
}

#[test]
fn token_damage_stays_wellformed() {
    let d = data("book5.dbk");
    let f = docbook();
    let k = 2; // "Run the <g1>installer</g1>, then read <x2>.<x3>"
    let tags: Vec<Inline> = ex(&f, &d).units[k].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let (open1, close1, x2) = (tags[0].clone(), tags[1].clone(), tags[2].clone());
    let cases: Vec<Vec<Inline>> = vec![
        vec![Inline::text("Lancez "), open1.clone(), Inline::text("l'installateur, puis lisez "), x2.clone(), Inline::text(".")],
        vec![close1.clone(), Inline::text("Lancez "), open1.clone(), Inline::text(" "), open1.clone(), Inline::text("x"), close1.clone(), Inline::text(" "), Inline::tag("9", TagKind::Standalone, "\u{27E6}x9\u{27E7}"), x2.clone(), x2, Inline::text(" fin")],
        vec![Inline::text("Plain text with < & > only")],
    ];
    for bad in cases {
        let mut doc = ex(&f, &d);
        doc.units[k].segments[0].target = bad;
        let out = f.merge(&doc).unwrap();
        well_formed(&f, &out);
        let st = s(&out);
        assert!(!st.contains('\u{27E6}') && !st.contains('\u{27E7}'), "token leaked: {st}");
        assert_eq!(st.matches("<footnote>").count(), 1, "the footnote is kept exactly once");
    }
}

#[test]
fn encodings() {
    let f = dita();
    let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<topic id=\"t\">\r\n  <title>caf\u{e9}</title>\r\n  <shortdesc>two</shortdesc>\r\n</topic>\r\n";
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(body.as_bytes());
    assert_eq!(f.merge(&ex(&f, &bom)).unwrap(), bom, "BOM and CRLF kept");
    let out = merge_map(&f, &bom, None, &[(1, "deux")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(s(&out[3..]).contains("<shortdesc>deux</shortdesc>\r\n</topic>\r\n"));

    // UTF-16 is read; written back in UTF-16 (B-bytes), where LocOS writes UTF-8
    let mut u16b = vec![0xFF, 0xFE];
    for c in body.replace("UTF-8", "UTF-16").encode_utf16() {
        u16b.extend_from_slice(&c.to_le_bytes());
    }
    assert_eq!(texts(&f, &u16b), vec!["caf\u{e9}", "two"]);
    assert!(f.sniff(&u16b) || !f.sniff(&u16b), "sniff never panics on UTF-16");
    assert_eq!(f.merge(&ex(&f, &u16b)).unwrap(), u16b);
    let out16 = merge_map(&f, &u16b, None, &[(0, "Kaffee")]);
    let (dec, _) = bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(&out16[2..]);
    assert!(out16.starts_with(&[0xFF, 0xFE]) && dec.contains("<title>Kaffee</title>"), "{dec}");
    // with the UTF-8 output option (the LocOS profile), UTF-8 under a UTF-8 declaration, as LocOS
    let mut doc = ex(&f, &u16b);
    set(&mut doc, 0, "Kaffee");
    let utf8 = f.merge_with(&doc, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)).unwrap();
    assert!(s(&utf8).starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>") && s(&utf8).contains("<title>Kaffee</title>"));

    // Windows-1252 declared
    let (latin, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode("<?xml version=\"1.0\" encoding=\"windows-1252\"?><topic id=\"t\"><title>na\u{ef}ve</title></topic>");
    let latin = latin.into_owned();
    assert_eq!(texts(&f, &latin), vec!["na\u{ef}ve"]);
    assert_eq!(f.merge(&ex(&f, &latin)).unwrap(), latin, "untouched = as uploaded");
    let out = merge_map(&f, &latin, None, &[(0, "\u{65e5}\u{672c}")]);
    assert_eq!(s(&out), "<?xml version=\"1.0\" encoding=\"UTF-8\"?><topic id=\"t\"><title>\u{65e5}\u{672c}</title></topic>");
}

#[test]
fn hostile_inputs() {
    for f in [&dita() as &dyn Filter, &docbook()] {
        let laughs = br#"<?xml version="1.0"?><!DOCTYPE topic [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">]><topic>&lol2;</topic>"#;
        let xxe = br#"<!DOCTYPE topic PUBLIC "-//OASIS//DTD DITA Topic//EN" "topic.dtd" [<!ENTITY x SYSTEM "file:///etc/passwd">]><topic><title>&x;</title></topic>"#;
        // Bilingua keeps an internal subset (LocOS refuses it); no entity is expanded or fetched
        for raw in [&laughs[..], &xxe[..]] {
            let d = f.extract(raw, &opts(None)).unwrap();
            assert_eq!(f.merge(&d).unwrap(), raw);
            assert!(d.units.iter().all(|u| !show(u).contains("root:")));
        }
        // an external DTD is never fetched (the address does not answer): its entities stay tags
        let ext = br#"<!DOCTYPE topic PUBLIC "-//OASIS//DTD DITA Topic//EN" "http://127.0.0.1:9/evil.dtd"><topic id="t"><title>Hi &ent; there</title></topic>"#;
        assert_eq!(texts(f, ext), vec!["Hi \u{27E6}x1\u{27E7} there"], "entity kept as a token");
        let out = s(&merge_map(f, ext, None, &[(0, "Salut \u{27E6}x1\u{27E7} toi")]));
        assert_eq!(out, r#"<!DOCTYPE topic PUBLIC "-//OASIS//DTD DITA Topic//EN" "http://127.0.0.1:9/evil.dtd"><topic id="t"><title>Salut &ent; toi</title></topic>"#);
        for raw in [&b"<topic><p>unclosed</topic>"[..], b"<topic><p>&nbsp;</p></topic>"] {
            assert!(matches!(f.extract(raw, &opts(None)), Err(Error::Parse { .. })), "{:?}", String::from_utf8_lossy(raw));
        }
        let deep = format!("<topic>{}x{}</topic>", "<ph>".repeat(300), "</ph>".repeat(300));
        assert!(matches!(f.extract(deep.as_bytes(), &opts(None)), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth));
        let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
        let many = format!("<topic>{}</topic>", "<p>x</p>".repeat(6));
        assert!(matches!(f.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
        let o = ExtractOptions::new().with_limits(Limits::new().with_max_xml_elements(100));
        let huge = format!("<topic>{}</topic>", "<b/>".repeat(300));
        assert!(matches!(f.extract(huge.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::XmlElements));
    }
}

#[test]
fn localized_language_stamp() {
    // DITA adds xml:lang when the root has none, and updates it when it has one; only on a translated merge
    let f = dita();
    let d = b"<topic id=\"t\"><title>Hello</title></topic>";
    assert_eq!(f.merge(&ex_to(&f, d, "fr-FR")).unwrap(), d, "nothing translated: original bytes");
    assert_eq!(merge_map(&f, d, Some("fr-FR"), &[(0, "Bonjour")]), b"<topic id=\"t\" xml:lang=\"fr-FR\"><title>Bonjour</title></topic>");
    let d2 = br#"<topic id="t" xml:lang="en"><title>Hello</title></topic>"#;
    assert_eq!(merge_map(&f, d2, Some("pt_br"), &[(0, "Ola")]), br#"<topic id="t" xml:lang="pt-BR"><title>Ola</title></topic>"#);
    // the LocOS profile stamps even when nothing is translated, as LocOS does
    let stamp = MergeOptions::new().with_stamp_target_always(true);
    assert_eq!(f.merge_with(&ex_to(&f, d2, "fr-FR"), &stamp).unwrap(), br#"<topic id="t" xml:lang="fr-FR"><title>Hello</title></topic>"#);
    // DocBook: 4.x adds lang, 5 (namespace) adds xml:lang, an existing lang or xml:lang is updated
    let db = docbook();
    assert_eq!(merge_map(&db, b"<article><para>Hi</para></article>", Some("ar"), &[(0, "x")]), b"<article lang=\"ar\"><para>x</para></article>");
    assert_eq!(
        merge_map(&db, br#"<article xmlns="http://docbook.org/ns/docbook"><para>Hi</para></article>"#, Some("ar"), &[(0, "x")]),
        br#"<article xmlns="http://docbook.org/ns/docbook" xml:lang="ar"><para>x</para></article>"#
    );
    assert_eq!(merge_map(&db, br#"<article lang="en"><para>Hi</para></article>"#, Some("ko-KR"), &[(0, "x")]), br#"<article lang="ko-KR"><para>x</para></article>"#);
    assert_eq!(
        merge_map(&db, br#"<article xml:lang="en" lang="en"><para>Hi</para></article>"#, Some("ko-KR"), &[(0, "x")]),
        br#"<article xml:lang="ko-KR" lang="en"><para>x</para></article>"#,
        "xml:lang wins when both are there"
    );
}

/// LocOS `xml_text.xml_target_text` (used by the XML emitter): vertical tab and form feed become line breaks,
/// characters XML 1.0 forbids are dropped from a translation.
#[test]
fn target_text_is_made_xml_safe() {
    let f = dita();
    let d = b"<topic id=\"t\"><title>T</title></topic>";
    let mut doc = ex(&f, d);
    doc.units[0].segments[0].target = vec![Inline::text("a\u{b}b\u{c}c\u{1}d\u{fffe}")];
    assert_eq!(s(&f.merge(&doc).unwrap()), "<topic id=\"t\"><title>a\nb\ncd</title></topic>");
}
