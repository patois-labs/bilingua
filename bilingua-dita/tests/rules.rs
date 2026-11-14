//! The DITA and DocBook rules one by one: conref / conkeyref / keyref, the DITA `translate` attribute, inline elements
//! as tags, the DocBook inline set, the fixed dialect, and the sniffers. Expected units are what LocOS gives.

mod common;

use bilingua_model::*;
use common::*;

fn kinds(u: &Unit) -> Vec<(TagKind, String)> {
    u.source()
        .into_iter()
        .filter_map(|i| match i {
            Inline::Tag { kind, display, free, .. } => {
                assert!(!free, "XML codes are never free placeholders");
                Some((kind, display))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn conref_conkeyref_and_keyref_are_left_untouched() {
    let f = Dita::new();
    let d = br##"<topic id="t"><title>T</title><body><p>Use <keyword keyref="prod"/> and <ph keyref="v">fallback</ph>. <xref keyref="k">link text</xref></p><p conkeyref="k/p"/><p conref="#a/b">old</p><ph conref="c.dita#c/p">Old</ph></body></topic>"##;
    let doc = ex(&f, d);
    assert_eq!(doc.units.iter().map(show).collect::<Vec<_>>(), vec!["T", "Use \u{27E6}x1\u{27E7} and \u{27E6}g2\u{27E7}fallback\u{27E6}/g2\u{27E7}. \u{27E6}g3\u{27E7}link text\u{27E6}/g3\u{27E7}"]);
    // the keyref element is one standalone code holding its markup; keyref attributes ride in the start tags
    let k = kinds(&doc.units[1]);
    assert_eq!(k[0], (TagKind::Standalone, r#"<keyword keyref="prod"/>"#.to_string()));
    assert_eq!(k[1], (TagKind::Open, r#"<ph keyref="v">"#.to_string()));
    let out = s(&translate_all(&f, d, Some("de-DE")));
    for kept in [r#"<keyword keyref="prod"/>"#, r#"<ph keyref="v">"#, r#"<xref keyref="k">"#, r#"<p conkeyref="k/p"/>"#, r##"<p conref="#a/b">old</p>"##, r#"<ph conref="c.dita#c/p">Old</ph>"#] {
        assert!(out.contains(kept), "{kept} changed: {out}");
    }
}

#[test]
fn dita_translate_attribute() {
    let f = Dita::new();
    let d = br#"<topic id="t"><title translate="no">T <ph translate="yes">yes</ph></title><body><p translate="no">No <b translate="yes">yes b</b></p><section translate="no"><p translate="yes">Back on</p><p>still off</p></section><p translate="maybe">other values are ignored</p></body></topic>"#;
    assert_eq!(texts(&f, d), vec!["yes", "yes b", "Back on", "other values are ignored"]);
    let out = s(&translate_all(&f, d, None));
    assert!(out.contains("<p>still off</p>") && out.contains("<title translate=\"no\">T <ph translate=\"yes\">FR:yes</ph></title>"), "{out}");
    // a whole topic marked translate="no": nothing at all, and the file merges back unchanged
    let none = br#"<topic id="t" translate="no"><title>T</title><body><p>x</p></body></topic>"#;
    let doc = ex(&f, none);
    assert!(doc.units.is_empty());
    assert_eq!(f.merge(&doc).unwrap(), none);
    // DocBook has no translate attribute: it is plain markup there
    assert_eq!(texts(&DocBook::new(), br#"<article><para translate="no">kept</para></article>"#), vec!["kept"]);
}

#[test]
fn dita_inline_elements_are_codes() {
    let f = Dita::new();
    let d = br#"<topic id="t"><title>A <b>b</b> <i>i</i> <codeph>cp</codeph> <xref href="x.dita">X</xref> <keyword>K</keyword> <tm tmtype="reg">TM</tm> <term>term</term> <uicontrol>UI</uicontrol> <filepath>/a</filepath> <ph>p</ph> <xref href="y.dita"/></title></topic>"#;
    let doc = ex(&f, d);
    assert_eq!(
        show(&doc.units[0]),
        "A \u{27E6}g1\u{27E7}b\u{27E6}/g1\u{27E7} \u{27E6}g2\u{27E7}i\u{27E6}/g2\u{27E7} \u{27E6}x3\u{27E7} \u{27E6}g4\u{27E7}X\u{27E6}/g4\u{27E7} \u{27E6}g5\u{27E7}K\u{27E6}/g5\u{27E7} \u{27E6}g6\u{27E7}TM\u{27E6}/g6\u{27E7} \u{27E6}g7\u{27E7}term\u{27E6}/g7\u{27E7} \u{27E6}g8\u{27E7}UI\u{27E6}/g8\u{27E7} \u{27E6}x9\u{27E7} \u{27E6}g10\u{27E7}p\u{27E6}/g10\u{27E7} \u{27E6}x11\u{27E7}"
    );
    let k = kinds(&doc.units[0]);
    // protected code is one standalone tag with its whole markup; an empty inline element is standalone too
    assert!(k.contains(&(TagKind::Standalone, "<codeph>cp</codeph>".into())));
    assert!(k.contains(&(TagKind::Standalone, "<filepath>/a</filepath>".into())));
    assert!(k.contains(&(TagKind::Standalone, r#"<xref href="y.dita"/>"#.into())));
    assert!(k.contains(&(TagKind::Open, r#"<xref href="x.dita">"#.into())) && k.contains(&(TagKind::Close, "</xref>".into())));
    // a translation may move the codes; the markup comes back exactly
    let mut doc = doc;
    set(&mut doc, 0, "\u{27E6}g4\u{27E7}X\u{27E6}/g4\u{27E7} \u{27E6}x3\u{27E7} \u{27E6}g1\u{27E7}B\u{27E6}/g1\u{27E7}");
    let out = s(&f.merge(&doc).unwrap());
    assert!(out.starts_with(r#"<topic id="t"><title><xref href="x.dita">X</xref> <codeph>cp</codeph> <b>B</b>"#), "{out}");
    // dropped standalone codes come back at the end of the unit, never lost; dropped pairs go with their text (LocOS)
    assert_eq!(out, r#"<topic id="t"><title><xref href="x.dita">X</xref> <codeph>cp</codeph> <b>B</b><filepath>/a</filepath><xref href="y.dita"/></title></topic>"#);
}

#[test]
fn nested_footnotes_index_terms_and_alt_text() {
    let f = Dita::new();
    let d = br#"<topic id="t"><title>T</title><body><p>Bake it.<fn>Or fry it.</fn><indexterm>bake<indexterm>bread</indexterm></indexterm></p><fig><title>Loaf</title><image href="l.png" alt="A loaf"><alt>A round loaf</alt></image></fig></body></topic>"#;
    let doc = ex(&f, d);
    let t: Vec<String> = doc.units.iter().map(show).collect();
    assert_eq!(t[1], "Bake it.\u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7}");
    assert!(t.contains(&"Or fry it.".to_string()) && t.contains(&"bake\u{27E6}x1\u{27E7}".to_string()) && t.contains(&"bread".to_string()));
    let alt = doc.units.iter().find(|u| show(u) == "A loaf").expect("alt attribute unit");
    assert_eq!(alt.context.style.as_deref(), Some("a11y:img-alt"));
    let out = s(&translate_all(&f, d, None));
    assert!(out.contains("<fn>FR:Or fry it.</fn>") && out.contains("alt=\"FR:A lo\u{e0}f\""), "{out}");
}

#[test]
fn docbook_inline_set() {
    let f = DocBook::new();
    let d = br#"<article><para>Hi <emphasis>e</emphasis> <command>ls</command> <guibutton>OK</guibutton> <xref linkend="a"/><footnote><para>F</para></footnote> <link linkend="b">L</link> <literal>lit</literal> <keycombo><keycap>Ctrl</keycap><keycap>C</keycap></keycombo> <productname>P</productname> <replaceable>file</replaceable> <subscript>2</subscript></para><programlisting>code</programlisting><remark>note to self</remark><screen>$ x</screen></article>"#;
    let doc = ex(&f, d);
    assert_eq!(
        doc.units.iter().map(show).collect::<Vec<_>>(),
        vec![
            "Hi \u{27E6}g1\u{27E7}e\u{27E6}/g1\u{27E7} \u{27E6}x2\u{27E7} \u{27E6}g3\u{27E7}OK\u{27E6}/g3\u{27E7} \u{27E6}x4\u{27E7}\u{27E6}x5\u{27E7} \u{27E6}g6\u{27E7}L\u{27E6}/g6\u{27E7} \u{27E6}x7\u{27E7} \u{27E6}g8\u{27E7}\u{27E6}g9\u{27E7}Ctrl\u{27E6}/g9\u{27E7}\u{27E6}g10\u{27E7}C\u{27E6}/g10\u{27E7}\u{27E6}/g8\u{27E7} \u{27E6}g11\u{27E7}P\u{27E6}/g11\u{27E7} \u{27E6}g12\u{27E7}file\u{27E6}/g12\u{27E7} \u{27E6}g13\u{27E7}2\u{27E6}/g13\u{27E7}",
            "F",
        ]
    );
    let k = kinds(&doc.units[0]);
    assert!(k.contains(&(TagKind::Standalone, "<command>ls</command>".into())));
    assert!(k.contains(&(TagKind::Standalone, "<literal>lit</literal>".into())));
    let out = s(&translate_all(&f, d, None));
    for kept in ["<programlisting>code</programlisting>", "<remark>note to self</remark>", "<screen>$ x</screen>", "<command>ls</command>"] {
        assert!(out.contains(kept), "{kept}: {out}");
    }
    // DocBook 5 with a prefix: matched on local names, the namespace decides the language attribute
    let p = br#"<db:article xmlns:db="http://docbook.org/ns/docbook"><db:para>Hi <db:emphasis>x</db:emphasis> <db:literal>y</db:literal></db:para></db:article>"#;
    assert_eq!(texts(&f, p), vec!["Hi \u{27E6}g1\u{27E7}x\u{27E6}/g1\u{27E7} \u{27E6}x2\u{27E7}"]);
}

#[test]
fn the_dialect_is_fixed_whatever_the_root() {
    // LocOS's dita / docbook filters apply their rule set even when the content is not recognised as one
    let d = b"<doc><p>Hi <ph>x</ph></p><codeph>c</codeph></doc>";
    assert_eq!(texts(&Dita::new(), d), vec!["Hi \u{27E6}g1\u{27E7}x\u{27E6}/g1\u{27E7}"]);
    assert_eq!(texts(&bilingua_xml::XmlFilter::new(), d), vec!["Hi", "x", "c"], "the generic filter does not guess");
    let db = b"<doc><para>Hi <emphasis>x</emphasis></para><programlisting>y</programlisting></doc>";
    assert_eq!(texts(&DocBook::new(), db), vec!["Hi \u{27E6}g1\u{27E7}x\u{27E6}/g1\u{27E7}"]);
    // a DocBook file opened as DITA gets the DITA rules (as in LocOS): emphasis is a block there
    assert_eq!(texts(&Dita::new(), db), vec!["Hi", "x", "y"]);
    // documents are tagged with their filter's format id
    assert_eq!(ex(&Dita::new(), d).format, "dita");
    assert_eq!(ex(&DocBook::new(), db).format, "docbook");
}

#[test]
fn sniffers_are_strict() {
    let (dita, db) = (Dita::new(), DocBook::new());
    let task = data("task.dita");
    let map = data("guide.ditamap");
    let b5 = data("book5.dbk");
    let b45 = data("book45.xml");
    assert!(dita.sniff(&task) && dita.sniff(&map) && !dita.sniff(&b5) && !dita.sniff(&b45));
    assert!(db.sniff(&b5) && db.sniff(&b45) && !db.sniff(&task) && !db.sniff(&map));
    // the architecture attribute alone is enough
    let arch = br#"<concept xmlns:ditaarch="http://dita.oasis-open.org/architecture/2005/" ditaarch:DITAArchVersion="1.3" id="c"><title>x</title></concept>"#;
    assert!(dita.sniff(arch));
    // a bare root name is left to the generic XML filter, which reads it with the DITA rules anyway
    assert!(!dita.sniff(br#"<topic id="t"><title>x</title></topic>"#));
    assert_eq!(bilingua_xml::XmlFilter::new().inspect(br#"<topic id="t"><title>x</title></topic>"#).unwrap().dialect.as_deref(), Some("dita"));
    // the word in a comment or text is not enough, nor is a broken file
    for no in [&b"<r><!-- DITA DocBook --><p>DITA and DocBook</p></r>"[..], b"<!DOCTYPE task PUBLIC \"-//OASIS//DTD DITA Task//EN\" \"t.dtd\"><task>", b"DITA", b"", b"\xff\xfe"] {
        assert!(!dita.sniff(no) && !db.sniff(no), "{:?}", String::from_utf8_lossy(no));
    }
    // UTF-16 files are looked into
    let mut u16 = vec![0xFF, 0xFE];
    for c in String::from_utf8(task.clone()).unwrap().replace("UTF-8", "UTF-16").encode_utf16() {
        u16.extend_from_slice(&c.to_le_bytes());
    }
    assert!(dita.sniff(&u16) && !db.sniff(&u16));
    // XHTML and other XML vocabularies are not ours
    assert!(!dita.sniff(b"<!DOCTYPE html><html><body><p>DITA</p></body></html>"));
    assert!(!db.sniff(br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>DocBook</p></body></html>"#));
}

#[test]
fn every_corpus_file_round_trips() {
    let files = corpus();
    assert!(files.len() >= 8, "corpus has {} files", files.len());
    for (name, bytes) in files {
        let f = filter_for(&name);
        let f = f.as_ref();
        assert!(f.sniff(&bytes), "{name}: every corpus file names its vocabulary (DOCTYPE, namespace or architecture attribute)");
        let d = f.extract(&bytes, &opts(Some("de-DE"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!d.units.is_empty(), "{name}");
        assert_eq!(f.merge(&d).unwrap(), bytes, "{name}: untouched merge");
        assert_eq!(echo(f, &bytes, None), bytes, "{name}: echo merge");
        for target in [None, Some("de-DE"), Some("ar"), Some("ja-JP"), Some("zh-CN"), Some("ko")] {
            let mut e = f.extract(&bytes, &opts(target)).unwrap();
            pseudo(&mut e);
            let out = f.merge(&e).unwrap_or_else(|er| panic!("{name} {target:?}: {er}"));
            let back = f.extract(&out, &opts(None)).unwrap_or_else(|er| panic!("{name} {target:?} does not read back: {er}"));
            assert_eq!(back.units.len(), e.units.len(), "{name} {target:?}: unit count");
            assert_eq!(tag_count(&back), tag_count(&d), "{name} {target:?}: tag count");
            for (b, u) in back.units.iter().zip(&d.units) {
                assert_eq!(show(b), format!("[{}]", show(u)), "{name} {target:?}");
            }
            if let Some(t) = target {
                let st = s(&out);
                let attr = if name.ends_with(".dbk") { format!("lang=\"{t}\"") } else { format!("xml:lang=\"{t}\"") };
                assert!(st.contains(&attr), "{name}: {attr} missing");
            }
        }
    }
}

#[test]
fn unit_ids_are_stable_and_paragraph_mode_holds() {
    for (name, bytes) in corpus() {
        let f = filter_for(&name);
        let a = f.extract(&bytes, &opts(None)).unwrap();
        let b = f.extract(&bytes, &ExtractOptions::new().with_granularity(Granularity::Sentence)).unwrap();
        let ids = |d: &Document| d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&a), ids(&b), "{name}");
        assert_eq!(ids(&a), (1..=a.units.len()).map(|i| format!("u{i}")).collect::<Vec<_>>(), "{name}");
        assert!(a.units.iter().all(|u| u.segments.len() == 1 && u.segments[0].id == u.id), "{name}");
        assert!(a.units.iter().all(|u| u.context.key.as_deref().is_some_and(|k| k.starts_with('/'))), "{name}: element path keys");
    }
}
