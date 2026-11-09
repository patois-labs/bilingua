//! Port of the Localization OS suite `tests/verify_xml_its_filter.py` (12 cases), plus the generic-XML cases of
//! `tests/verify_xml_resource_filters.py` that go through the ITS filter (see `locos_resource_generic.rs`).

mod common;

use bilingua_model::*;
use bilingua_xml::{Dialect, ItsRules, XmlFilter};
use common::*;

fn generic() -> XmlFilter {
    XmlFilter::new()
}

fn dita() -> XmlFilter {
    XmlFilter::with_id("dita", &["dita", "ditamap", "bookmap"]).dialect(Dialect::dita())
}

fn docbook() -> XmlFilter {
    XmlFilter::with_id("docbook", &["dbk"]).dialect(Dialect::docbook())
}

#[test]
fn registration() {
    // ids and extensions the facade will list (the LocOS registry names dita, ditamap, bookmap, dbk and xml)
    assert_eq!(generic().id(), "xml");
    assert_eq!(generic().extensions(), &["xml"]);
    assert_eq!(dita().id(), "dita");
    assert!(dita().extensions().contains(&"ditamap") && dita().extensions().contains(&"bookmap"));
    assert_eq!(docbook().extensions(), &["dbk"]);
}

#[test]
fn identity_all_fixtures() {
    for (name, f, lang) in [("task.dita", dita(), "en-US"), ("guide.ditamap", dita(), "+"), ("book5.dbk", docbook(), "en"), ("book45.xml", generic(), "en"), ("catalog.xml", generic(), "")] {
        let d = data(name);
        let doc = ex(&f, &d);
        assert!(!doc.units.is_empty(), "{name}");
        assert_eq!(s(&f.merge(&doc).unwrap()), s(&d), "{name}: untouched merge");
        assert_eq!(s(&echo(&f, &d, None)), s(&d), "{name}: echo merge");
        let doc = ex_to(&f, &d, "fr-FR");
        assert_eq!(s(&f.merge(&doc).unwrap()), s(&d), "{name}: untouched merge with a target language");
        // echoed targets count as translations: the only change is the root language
        let e = s(&echo(&f, &d, Some("fr-FR")));
        let back = match lang {
            "" => e.clone(),
            "+" => e.replacen(" xml:lang=\"fr-FR\"", "", 1), // DITA adds the language when the root has none
            l => e.replacen("fr-FR", l, 1),
        };
        assert_eq!(back, s(&d), "{name}: echo merge with a target language");
    }
}

#[test]
fn dita_topic() {
    let d = data("task.dita");
    let f = dita();
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
    assert!(!out.contains("FR:Press <uicontrol>FR:Print</uicontrol>"));
    assert!(out.contains("<uicontrol>Print</uicontrol> &amp; w\u{e0}it &lt;5&gt; seconds for &product;."), "{out}");
    assert!(out.contains("<indexterm>FR:printing<indexterm>FR:reports</indexterm></indexterm>"), "{out}");
    assert!(out.contains(r#"alt="FR:A l\u{e0}ser printer""#.replace("\\u{e0}", "\u{e0}").as_str()));
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

    let d45 = data("book45.xml");
    let g = generic();
    let insp = g.inspect(&d45).unwrap();
    assert_eq!(insp.dialect.as_deref(), Some("docbook"), "DocBook 4.5 sniffed from the DOCTYPE");
    assert_eq!(texts(&g, &d45).last().unwrap(), "Back up \u{27E6}x1\u{27E7} every \u{27E6}g2\u{27E7}day\u{27E6}/g2\u{27E7}.");
    let out45 = s(&translate_all(&g, &d45, Some("ja-JP")));
    assert!(out45.contains(r#"<book lang="ja-JP">"#), "DocBook 4.x uses lang: {out45}");
    assert!(out45.contains("<screen>$ app backup --all</screen>"));
    assert!(out45.contains("<filename>/etc/app.conf</filename>"));
}

#[test]
fn its_rules_in_document() {
    let d = data("catalog.xml");
    let f = generic();
    let doc = ex(&f, &d);
    let t: Vec<String> = doc.units.iter().map(show).collect();
    assert_eq!(t, vec!["Desk lamp", "A \u{27E6}g1\u{27E7}bright\u{27E6}/g1\u{27E7} lamp\u{27E6}x2\u{27E7}for your desk.", "Line one\n    indented   line"]);
    assert_eq!(doc.units[1].notes[0].text, "Max 80 characters.");
    assert_eq!(doc.units[0].context.style.as_deref(), Some("attr:label"));
    let out = s(&translate_all(&f, &d, None));
    well_formed(&f, out.as_bytes());
    assert!(out.contains("<sku>L-100-BLK</sku>") && out.contains(r#"<legal its:translate="no">Made in EU</legal>"#));
    assert!(out.contains(r#"code="L-1""#) && out.contains("label=\"FR:Desk l\u{e0}mp\""), "{out}");
    assert!(out.contains("FR:Line one\n    indented   line"), "preserved white space untouched");
    assert!(out.contains("<its:locNote>Max 80 characters.</its:locNote>"));
}

const ITS_OPEN: &str = r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0">"#;

fn refused(raw: &str, needle: &str) {
    match ItsRules::parse(raw.as_bytes()) {
        Err(e) => assert!(e.to_string().contains(needle), "{needle} not in {e}"),
        Ok(_) => panic!("rules accepted: {raw:.80}"),
    }
}

#[test]
fn user_rules_file() {
    let rules = ItsRules::parse(
        br#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//note" translate="no"/><its:translateRule selector="//item/@title" translate="yes"/><its:withinTextRule selector="//em" withinText="yes"/></its:rules>"#,
    )
    .unwrap();
    let f = XmlFilter::new().its_rules(rules);
    let d = br#"<doc><item title="Hello"><p>A <em>big</em> day</p><note>skip me</note></item></doc>"#;
    assert_eq!(texts(&f, d), vec!["Hello", "A \u{27E6}g1\u{27E7}big\u{27E6}/g1\u{27E7} day"]);
    assert_eq!(texts(&XmlFilter::new(), d), vec!["A", "big", "day", "skip me"]);

    refused("<rules/>", "root");
    refused(&format!(r#"{ITS_OPEN}<its:translateRule selector="//a[" translate="no"/></its:rules>"#), "invalid XPath");
    refused(&format!(r#"{ITS_OPEN}<its:translateRule selector="//a[//b]" translate="no"/></its:rules>"#), "not allowed inside a predicate");
    refused(&format!(r#"{ITS_OPEN}<its:translateRule selector="//a[b[c[d]]]" translate="no"/></its:rules>"#), "two deep");
    refused(&format!(r#"{ITS_OPEN}<its:translateRule selector="//a" translate="maybe"/></its:rules>"#), "yes or no");
    let many = format!("{ITS_OPEN}{}</its:rules>", r#"<its:translateRule selector="//a" translate="no"/>"#.repeat(101));
    refused(&many, "more than 100");
    // quadratic selector and pointer shapes
    for (sel, needle) in [
        ("//*/following::*", "following and preceding"),
        ("//p/preceding-sibling::*", "following and preceding"),
        ("//p[../q]", "not allowed inside a predicate"),
        ("//p[parent::*/q]", "not allowed inside a predicate"),
        ("//p[/doc/q = 'x']", "not allowed inside a predicate"),
        ("//p[count(/doc/*) &gt; 1]", "not allowed inside a predicate"),
    ] {
        refused(&format!(r#"{ITS_OPEN}<its:translateRule selector="{sel}" translate="no"/></its:rules>"#), needle);
    }
    for ptr in ["//p", "/doc/p", "descendant::p", "../following-sibling::p"] {
        refused(&format!(r#"{ITS_OPEN}<its:locNoteRule selector="//p" locNotePointer="{ptr}"/></its:rules>"#), "pointer");
    }
    // a literal that merely contains an axis name, and a relative pointer, are fine
    let ok = ItsRules::parse(
        format!(r#"{ITS_OPEN}<its:translateRule selector="//p[@kind='following']" translate="no"/><its:locNoteRule selector="//p" locNotePointer="../n"/></its:rules>"#).as_bytes(),
    )
    .unwrap();
    assert_eq!(ok.len(), 2);
    let ptr_rules = ItsRules::parse(format!(r#"{ITS_OPEN}<its:locNoteRule selector="//p" locNotePointer="../n"/></its:rules>"#).as_bytes()).unwrap();
    let f = XmlFilter::new().its_rules(ptr_rules);
    let doc = ex(&f, b"<d><g><p>Save</p><n>Button label</n></g></d>");
    assert_eq!(doc.units[0].notes[0].text, "Button label");
    // a pointer runs once per hit: hits x elements is bounded
    let many = format!("<d>{}</d>", "<g><p>t</p><n>x</n></g>".repeat(40));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(20));
    match f.extract(many.as_bytes(), &o) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("locNotePointer"), "{}", e.msg),
        other => panic!("pointer budget not enforced: {:?}", other.map(|d| d.units.len())),
    }
}

#[test]
fn placeholders_and_cdata() {
    let f = generic();
    let d = b"<r><m>Hello {0}, you have %1$s new {{count}} items</m><c><![CDATA[Use <b> here]]></c></r>";
    assert_eq!(texts(&f, d), vec!["Hello {0}, you have %1$s new {{count}} items", "Use <b> here"]);
    let out = s(&merge_with(&f, d, None, &[(0, "Hallo {0}, du hast %1$s neue {{count}}"), (1, "Nimm <b> ]]> hier")]));
    assert!(out.contains("<m>Hallo {0}, du hast %1$s neue {{count}}</m>"));
    assert!(out.contains("<c><![CDATA[Nimm <b> ]]]]><![CDATA[> hier]]></c>"), "{out}");
    well_formed(&f, out.as_bytes());
    // reads back as the original text
    assert_eq!(texts(&f, out.as_bytes())[1], "Nimm <b> ]]> hier");
}

#[test]
fn token_damage_stays_wellformed() {
    let d = data("book5.dbk");
    let f = docbook();
    let k = 2; // "Run the <g1>installer</g1>, then read <x2>.<x3>"
    let doc0 = ex(&f, &d);
    let tags: Vec<Inline> = doc0.units[k].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let (open1, close1, x2, x3) = (tags[0].clone(), tags[1].clone(), tags[2].clone(), tags[3].clone());
    let cases: Vec<Vec<Inline>> = vec![
        // unclosed g1, dropped x3 (the footnote)
        vec![Inline::text("Lancez "), open1.clone(), Inline::text("l'installateur, puis lisez "), x2.clone(), Inline::text(".")],
        // stray close, duplicate, unknown, doubled point
        vec![close1.clone(), Inline::text("Lancez "), open1.clone(), Inline::text(" "), open1.clone(), Inline::text("x"), close1.clone(), Inline::text(" "), Inline::tag("9", TagKind::Standalone, "⟦x9⟧"), x2.clone(), x2.clone(), Inline::text(" fin")],
        vec![Inline::text("Plain text with < & > only")],
    ];
    for bad in cases {
        let mut doc = ex(&f, &d);
        set_raw(&mut doc, k, bad);
        let out = f.merge(&doc).unwrap();
        well_formed(&f, &out);
        let st = s(&out);
        assert!(!st.contains('\u{27E6}') && !st.contains('\u{27E7}'), "token leaked: {st}");
        assert_eq!(st.matches("<footnote>").count(), 1, "the footnote is kept exactly once");
    }
    let _ = x3;
}

#[test]
fn encodings() {
    let f = generic();
    let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<r>\r\n  <p>caf\u{e9}</p>\r\n  <q>two</q>\r\n</r>\r\n";
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(body.as_bytes());
    assert_eq!(f.merge(&ex(&f, &bom)).unwrap(), bom, "BOM and CRLF kept");
    let out = merge_with(&f, &bom, None, &[(1, "deux")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(s(&out[3..]).contains("<q>deux</q>\r\n</r>\r\n"));

    // UTF-16: read as text; written back in UTF-16 (the envelope), not UTF-8
    let u16s = body.replace("UTF-8", "UTF-16");
    let mut u16b = vec![0xFF, 0xFE];
    for c in u16s.encode_utf16() {
        u16b.extend_from_slice(&c.to_le_bytes());
    }
    assert_eq!(texts(&f, &u16b), vec!["caf\u{e9}", "two"]);
    assert_eq!(f.merge(&ex(&f, &u16b)).unwrap(), u16b);
    let out16 = merge_with(&f, &u16b, None, &[(0, "Kaffee")]);
    assert!(out16.starts_with(&[0xFF, 0xFE]));
    let (dec, _) = bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(&out16[2..]);
    assert!(dec.contains("<p>Kaffee</p>") && dec.contains("encoding=\"UTF-16\""), "{dec}");

    // Windows-1252 declared: as uploaded when untouched; the envelope is kept when the text fits
    let (latin, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode("<?xml version=\"1.0\" encoding=\"windows-1252\"?><r><p>na\u{ef}ve</p></r>");
    let latin = latin.into_owned();
    assert_eq!(texts(&f, &latin), vec!["na\u{ef}ve"]);
    assert_eq!(f.merge(&ex(&f, &latin)).unwrap(), latin);
    let out = merge_with(&f, &latin, None, &[(0, "na\u{ef}ve!")]);
    let (back, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.decode(&out);
    assert_eq!(back, "<?xml version=\"1.0\" encoding=\"windows-1252\"?><r><p>na\u{ef}ve!</p></r>");
    // text that does not fit the code page is written as UTF-8 under a UTF-8 declaration
    let out = merge_with(&f, &latin, None, &[(0, "\u{65e5}\u{672c}")]);
    assert_eq!(s(&out), "<?xml version=\"1.0\" encoding=\"UTF-8\"?><r><p>\u{65e5}\u{672c}</p></r>");
}

#[test]
fn hostile_inputs() {
    let f = generic();
    let laughs = br#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">]><lolz>&lol2;</lolz>"#;
    let xxe = br#"<!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/passwd">]><r><p>&x;</p></r>"#;
    // Bilingua keeps an internal subset (LocOS refuses it): Okapi opens such files, and real DocBook 4 and DITA
    // files have them. Nothing is expanded or fetched; every entity stays a code and the DOCTYPE is written back.
    for raw in [&laughs[..], &xxe[..]] {
        let d = f.extract(raw, &opts(None)).unwrap();
        assert_eq!(f.merge(&d).unwrap(), raw);
        assert!(d.units.iter().all(|u| !show(u).contains("root:")));
    }
    let subset = br#"<!DOCTYPE r [<!ELEMENT r (#PCDATA)>]><r>x</r>"#;
    assert_eq!(texts(&f, subset), vec!["x"]);

    let ext = br#"<!DOCTYPE r SYSTEM "http://127.0.0.1:9/evil.dtd"><r><p>Hi &ent; there</p></r>"#;
    assert_eq!(texts(&f, ext), vec!["Hi \u{27E6}x1\u{27E7} there"], "entity kept as a token");
    let out = s(&merge_with(&f, ext, None, &[(0, "Salut \u{27E6}x1\u{27E7} toi")]));
    assert_eq!(out, r#"<!DOCTYPE r SYSTEM "http://127.0.0.1:9/evil.dtd"><r><p>Salut &ent; toi</p></r>"#);

    for (raw, needle) in [(&b"<r><p>unclosed</r>"[..], "xml"), (b"<r><p>&nbsp;</p></r>", "undefined entity")] {
        match f.extract(raw, &opts(None)) {
            Err(Error::Parse { msg, .. }) => assert!(msg.contains(needle) || needle == "xml", "{msg}"),
            other => panic!("accepted {:?}: {:?}", String::from_utf8_lossy(raw), other.map(|d| d.units.len())),
        }
    }
    // deep nesting: refused by the XML depth limit
    let deep = format!("{}x{}", "<a>".repeat(300), "</a>".repeat(300));
    match f.extract(deep.as_bytes(), &opts(None)) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlDepth),
        other => panic!("deep nesting accepted: {:?}", other.map(|d| d.units.len())),
    }
    // segment cap
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let many = format!("<r>{}</r>", "<p>x</p>".repeat(6));
    match f.extract(many.as_bytes(), &o) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units),
        other => panic!("segment cap not enforced: {:?}", other.map(|d| d.units.len())),
    }
    // element cap
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_xml_elements(100));
    let huge = format!("<r>{}</r>", "<b/>".repeat(300));
    match f.extract(huge.as_bytes(), &o) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlElements),
        other => panic!("element budget not enforced: {:?}", other.map(|d| d.units.len())),
    }
    // rules x elements budget
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let costly = format!(
        r#"<r xmlns:its="http://www.w3.org/2005/11/its"><its:rules version="2.0">{}</its:rules>{}</r>"#,
        r#"<its:translateRule selector="//b" translate="no"/>"#.repeat(6),
        "<b/>".repeat(185)
    );
    match f.extract(costly.as_bytes(), &o) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("number of ITS rules"), "{}", e.msg),
        other => panic!("rules x elements budget not enforced: {:?}", other.map(|d| d.units.len())),
    }
}

#[test]
fn generic_localized_lang_only_when_present() {
    let f = generic();
    let d = b"<r><p>Hello</p></r>";
    assert_eq!(merge_with(&f, d, Some("fr-FR"), &[(0, "Bonjour")]), b"<r><p>Bonjour</p></r>");
    let d2 = br#"<r xml:lang="en"><p>Hello</p></r>"#;
    // untouched: nothing changes; translated: the language is updated
    assert_eq!(f.merge(&ex_to(&f, d2, "fr-FR")).unwrap(), d2);
    assert_eq!(merge_with(&f, d2, Some("fr-FR"), &[(0, "Bonjour")]), br#"<r xml:lang="fr-FR"><p>Bonjour</p></r>"#);
    // a locale is written in BCP 47 form
    assert_eq!(merge_with(&f, d2, Some("pt_br"), &[(0, "Ola")]), br#"<r xml:lang="pt-BR"><p>Ola</p></r>"#);
}

/// Generic XML with no language on the root: a nested element that declares the source language and holds a changed
/// unit is restamped, the root is left without one (LocOS), a foreign passage stays.
#[test]
fn generic_nested_source_language_restamped_without_root_language() {
    let f = generic();
    let d = br#"<r><ex xml:lang="en"><p>Hello</p></ex><ex xml:lang="fr"><p>Salut</p></ex><ex xml:lang="en"><p>Bye</p></ex></r>"#;
    let out = merge_with(&f, d, Some("de-DE"), &[(0, "Hallo"), (1, "Tschuess"), (2, "Tschuess")]);
    assert_eq!(s(&out), r#"<r><ex xml:lang="de-DE"><p>Hallo</p></ex><ex xml:lang="fr"><p>Tschuess</p></ex><ex xml:lang="de-DE"><p>Tschuess</p></ex></r>"#);
    // nothing translated: untouched
    assert_eq!(f.merge(&ex_to(&f, d, "de-DE")).unwrap(), d);
}

/// No language on the root, and the project says `en`: LocOS takes the first language the file declares as the source
/// (`ta` here), not the project's. Under the profile the `ta` passage is restamped and the `en` one stays; without the
/// profile the project's language decides.
#[test]
fn locos_nested_source_is_the_first_declared_language() {
    let f = generic();
    let d = br#"<r><h><title xml:lang="ta">Kalam</title></h><d xml:lang="en"><p>Hello</p></d></r>"#;
    let build = |locos: bool| {
        let o = ExtractOptions::new().with_source_lang("en").with_target_lang("de-DE");
        let mut doc = f.extract(d, &o).expect("extract");
        for i in 0..doc.units.len() {
            set(&mut doc, i, "Neu");
        }
        let m = MergeOptions::new().with_profile(if locos { Some("locos".to_string()) } else { None });
        s(&f.merge_with(&doc, &m).expect("merge"))
    };
    assert_eq!(
        build(true),
        r#"<r><h><title xml:lang="de-DE">Neu</title></h><d xml:lang="en"><p>Neu</p></d></r>"#
    );
    assert_eq!(
        build(false),
        r#"<r><h><title xml:lang="ta">Neu</title></h><d xml:lang="de-DE"><p>Neu</p></d></r>"#
    );
}

/// A skeleton written before the root marker carried the first declared language (no third field) merges under the
/// profile as it did then: the project's language decides.
#[test]
fn locos_skeleton_without_the_first_language_merges_as_before() {
    let f = generic();
    let d = br#"<r><h><title xml:lang="ta">Kalam</title></h><d xml:lang="en"><p>Hello</p></d></r>"#;
    let o = ExtractOptions::new().with_source_lang("en").with_target_lang("de-DE");
    let mut doc = f.extract(d, &o).expect("extract");
    for i in 0..doc.units.len() {
        set(&mut doc, i, "Neu");
    }
    let Skeleton::Text { parts, .. } = &mut doc.skeleton else { panic!("text skeleton") };
    let mut stripped = 0;
    for p in parts.iter_mut() {
        if let SkelPart::Raw(r) = p {
            if let Some(at) = r.find("\0L") {
                let end = at + 2 + r[at + 2..].find('\0').expect("marker end");
                let arg: Vec<&str> = r[at + 2..end].split('|').collect();
                if arg.len() == 3 {
                    *r = format!("{}\0L{}|{}{}", &r[..at], arg[0], arg[1], &r[end..]);
                    stripped += 1;
                }
            }
        }
    }
    assert_eq!(stripped, 1, "the new skeleton carries the first language");
    let m = MergeOptions::new().with_profile(Some("locos".to_string()));
    assert_eq!(
        s(&f.merge_with(&doc, &m).expect("merge")),
        r#"<r><h><title xml:lang="ta">Neu</title></h><d xml:lang="de-DE"><p>Neu</p></d></r>"#
    );
}
