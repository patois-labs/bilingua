//! Hostile input: bombs, entity declarations, deep nesting, path escapes, duplicate members, damaged and truncated
//! files, encodings that are not UTF-8, markers that clash with the text. Every case is a typed error or a clean
//! read; none panics.
mod common;
use bilingua_flare::Flare;
use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn kind(e: &Error) -> Option<LimitKind> {
    match e {
        Error::Limit(l) => Some(l.kind),
        _ => None,
    }
}

fn raised() -> ExtractOptions {
    // deep nesting compresses far past the zip ratio limit; raise it so the XML guard is what answers
    ExtractOptions { limits: Some(Limits { max_zip_ratio: 1_000_000, ..Limits::default() }), ..Default::default() }
}

#[test]
fn entity_declarations_in_every_part_are_refused() {
    let dtd = "<!DOCTYPE x [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">]>";
    let with = |s: &str| s.replacen("?>", &format!("?>{dtd}"), 1);
    for (name, body) in [
        ("Project/VariableSets/General.flvar", with(FLVAR)),
        ("Project/TOCs/Master.fltoc", with(FLTOC)),
        ("Project/Glossaries/Glossary.flglo", with(FLGLO)),
        ("Content/Topic.htm", with(TOPIC)),
        ("Content/Resources/Snippets/Brand.flsnp", with(BRAND)),
    ] {
        let e = extract_err(&project_with(&[(name, body.as_bytes())], &[]));
        assert_eq!(kind(&e), Some(LimitKind::XmlEntities), "{name}: {e}");
    }
    // a DTD with no entity is still refused in a structured part
    let e = extract_err(&project_with(&[("Project/TOCs/Master.fltoc", b"<!DOCTYPE t><CatapultToc/>")], &[]));
    assert_eq!(kind(&e), Some(LimitKind::XmlEntities), "{e}");
}

#[test]
fn deep_nesting_is_a_typed_error() {
    let n = 100_000;
    let deep = |open: &str, close: &str| format!("{}x{}", open.repeat(n), close.repeat(n));
    let topic = TOPIC.replace("<p>Hidden body text.</p>", &deep("<div>", "</div>"));
    let e = Flare.extract(&project_with(&[("Content/Topic.htm", topic.as_bytes())], &[]), &raised()).expect_err("deep topic");
    assert_eq!(kind(&e), Some(LimitKind::XmlDepth), "{e}");
    let inline = TOPIC.replace("<p>Hidden body text.</p>", &format!("<p>{}x{}</p>", "<MadCap:xref>".repeat(n), "</MadCap:xref>".repeat(n)));
    let e = Flare.extract(&project_with(&[("Content/Topic.htm", inline.as_bytes())], &[]), &raised()).expect_err("deep inline");
    assert!(kind(&e).is_some(), "{e}");
    let toc = FLTOC.replace("<TocEntry Title=\"Getting started\"", &format!("{}<TocEntry Title=\"Getting started\"", "<TocEntry>".repeat(n)));
    let e = Flare.extract(&project_with(&[("Project/TOCs/Master.fltoc", toc.as_bytes())], &[]), &raised()).expect_err("deep toc");
    assert_eq!(kind(&e), Some(LimitKind::XmlDepth), "{e}");
    let o = ExtractOptions { limits: Some(Limits { max_xml_depth: 3, ..Limits::default() }), ..Default::default() };
    assert_eq!(kind(&Flare.extract(&project(), &o).expect_err("depth 3")), Some(LimitKind::XmlDepth));
}

#[test]
fn encodings_other_than_utf8_are_refused_not_guessed() {
    // Latin-1 without a declaration, with a lying UTF-8 declaration, and UTF-16 with and without a mark
    for body in [b"<html><body><p>caf\xe9</p></body></html>".to_vec(), b"<?xml version=\"1.0\" encoding=\"utf-8\"?><html><body><p>caf\xe9</p></body></html>".to_vec()] {
        let e = extract_err(&project_with(&[("Content/Topic.htm", &body)], &[]));
        assert!(matches!(e, Error::Encoding(_)), "{e}");
    }
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    for c in "<html><body><p>Hi</p></body></html>".encode_utf16() {
        u16.extend_from_slice(&c.to_le_bytes());
    }
    assert!(matches!(extract_err(&project_with(&[("Content/Topic.htm", &u16)], &[])), Error::Encoding(_)));
    assert!(matches!(extract_err(&project_with(&[("Project/TOCs/Master.fltoc", &u16)], &[])), Error::Encoding(_)));
    assert!(matches!(extract_err(&project_with(&[("Content/Topic.htm", &u16[2..])], &[])), Error::Encoding(_)));
    // UTF-8 that declares another charset is read as UTF-8, as LocOS does, and written back with its declaration
    let declared = "<?xml version=\"1.0\" encoding=\"iso-8859-1\"?>\n<html xmlns:MadCap=\"http://www.madcapsoftware.com/Schemas/MadCap.xsd\"><body><p>caf\u{e9} cr\u{e8}me</p></body></html>\n";
    let src = project_with(&[("Content/Topic.htm", declared.as_bytes())], &[]);
    let mut d = extract(&src);
    assert!(loc_texts(&d).contains(&"caf\u{e9} cr\u{e8}me".to_string()));
    assert_eq!(merge(&d), src);
    let i = index_of(&d, "caf\u{e9} cr\u{e8}me");
    set(&mut d, i, "th\u{e9} \u{65e5}\u{672c}");
    let (m, _) = members(&merge(&d));
    let t = text_of(&m, "Content/Topic.htm");
    assert!(t.contains("encoding=\"iso-8859-1\"") && t.contains("th\u{e9} \u{65e5}\u{672c}"), "{t}");
}

#[test]
fn damaged_structured_parts_are_typed_errors() {
    for (name, body) in [
        ("Project/VariableSets/General.flvar", FLVAR.replace("</CatapultVariableSet>", "")),
        ("Project/VariableSets/General.flvar", FLVAR.replace("&amp;", "&nbsp;")),
        ("Project/VariableSets/General.flvar", FLVAR.replace("<Variable Name=\"Year\"", "<Variable Name=\"Year\" Name=\"b\"")),
        ("Project/TOCs/Master.fltoc", FLTOC.replace("</CatapultToc>", "")),
        ("Project/TOCs/Master.fltoc", format!("{FLTOC}<Extra/>")),
        ("Project/TOCs/Master.fltoc", String::new()),
        ("Project/Glossaries/Glossary.flglo", FLGLO.replace("<Term>spooler</Term>", "<Term>spooler</Trem>")),
        ("Project/Glossaries/Glossary.flglo", "\u{0}\u{0}\u{0}".to_string()),
    ] {
        let e = extract_err(&project_with(&[(name, body.as_bytes())], &[]));
        assert!(matches!(e, Error::Parse { .. } | Error::Encoding(_) | Error::Limit(_)), "{name}: {e}");
    }
}

#[test]
fn hostile_topics_are_read_without_panic() {
    for t in [
        "<html><body><p>unclosed <MadCap:xref href=\"a\">text",
        "<html><body></MadCap:xref></MadCap:conditionalText><p>orphan closes</p></body></html>",
        "<html><body><p><MadCap:variable name=\"a\">no end",
        "<html><body><p><MadCap:keyword term=\"unterminated",
        "<html><body><p><MadCap:keyword term=\"\" /><MadCap:keyword term=\";;::\" /><MadCap:keyword term=\"\\\" /></p></body></html>",
        "<html><body><title><MadCap:variable name=\"x\"/> Name</title><p>x</p></body></html>",
        "<html><body><!-- <MadCap:xref> --><script>var a = '<MadCap:xref>';</script><p>ok</p></body></html>",
        "<html><body><p mcf=\"1\" data-mcf=\"2\">mcf mcfx mcfxx mcfxxx mcfxxxx mcfxxxxx <MadCap:xref>a</MadCap:xref></p></body></html>",
        "<html><body><p>a <span mcf=\"0\">b</span> <wbr mcf=\"0\"/> <MadCap:xref>c</MadCap:xref></p></body></html>",
        "<<<<<>>>>>&&&&;;;;<MadCap:<MadCap:xref",
        "",
    ] {
        let src = project_with(&[("Content/Topic.htm", t.as_bytes())], &[]);
        if let Ok(mut d) = Flare.extract(&src, &ExtractOptions::default()) {
            assert_eq!(merge(&d), src, "{t}");
            translate_all(&mut d);
            if let Ok(out) = Flare.merge(&d) {
                assert!(Flare.extract(&out, &ExtractOptions::default()).is_ok(), "{t}");
            }
        }
    }
}

#[test]
fn text_that_looks_like_the_marker_still_round_trips() {
    let t = r#"<html xmlns:MadCap="x"><body><p>mcf0 data-mcf="1" <MadCap:xref href="a">link mcf</MadCap:xref> and <MadCap:variable name="v" /> end</p></body></html>"#;
    let src = project_with(&[("Content/Topic.htm", t.as_bytes())], &[]);
    let mut d = extract(&src);
    assert_eq!(merge(&d), src);
    translate_all(&mut d);
    let (m, _) = members(&merge(&d));
    let out = text_of(&m, "Content/Topic.htm");
    assert!(out.contains(r#"<MadCap:xref href="a">"#) && out.contains(r#"<MadCap:variable name="v" />"#), "{out}");
    assert!(out.contains("T(mcf0 data-mcf=\"1\" "), "{out}");
}

#[test]
fn path_escapes_and_odd_names_are_refused() {
    for name in ["../evil.htm", "/abs/evil.htm", "Content/../../evil", "C:/evil", "a/b:stream"] {
        let e = extract_err(&project_with(&[(name, b"x")], &[]));
        assert!(matches!(e, Error::Parse { .. } | Error::Limit(_)), "{name}: {e}");
    }
}

#[test]
fn hidden_duplicate_members_are_refused() {
    let mut bytes = make_zip(&[("Help.flprj".into(), FLPRJ.as_bytes().to_vec()), ("Content/aaaa.htm".into(), b"1".to_vec()), ("Content/bbbb.htm".into(), b"2".to_vec())]);
    let a = b"Content/bbbb.htm";
    let mut i = 0;
    while let Some(p) = bytes[i..].windows(a.len()).position(|w| w == a) {
        bytes[i + p..i + p + 12].copy_from_slice(b"Content/aaaa");
        i += p + a.len();
    }
    let e = extract_err(&bytes);
    assert!(e.to_string().contains("twice") || e.to_string().contains("duplicate"), "{e}");
    assert!(Flare.extract(&make_zip(&[("Help.flprj".into(), b"x".to_vec()), ("Content/A.htm".into(), b"1".to_vec()), ("Content/a.htm".into(), b"2".to_vec())]), &ExtractOptions::default()).is_err());
}

#[test]
fn truncated_files_never_panic() {
    let full = project();
    for cut in (0..full.len()).step_by(37) {
        let _ = Flare.extract(&full[..cut], &ExtractOptions::default());
    }
    let _ = Flare.extract(&[], &ExtractOptions::default());
}

#[test]
fn many_unclosed_elements_stay_fast() {
    // an unclosed element makes a naive reader scan to the end of the topic for each one
    for body in ["<MadCap:variable name=\"x\">", "<MadCap:xref href=\"x\">", "<script>", "<code>x ", "<MadCap:keyword term=\"a;b\"/>"] {
        let t = format!("<html><body><p>{}</p><p>after</p></body></html>", body.repeat(20_000));
        let src = project_with(&[("Content/Topic.htm", t.as_bytes())], &[]);
        let start = std::time::Instant::now();
        let r = Flare.extract(&src, &ExtractOptions::default());
        let el = start.elapsed();
        assert!(r.is_ok() || r.is_err());
        assert!(el.as_secs() < 20, "{body}: took {el:?}");
    }
}

#[test]
fn an_oversized_input_and_unit_text_are_limits() {
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
    assert_eq!(kind(&Flare.extract(&project(), &o).expect_err("big")), Some(LimitKind::InputBytes));
    let o = ExtractOptions { limits: Some(Limits { max_unit_chars: 4, ..Limits::default() }), ..Default::default() };
    assert_eq!(kind(&Flare.extract(&project(), &o).expect_err("long")), Some(LimitKind::UnitChars));
    let o = ExtractOptions { limits: Some(Limits { max_xml_elements: 3, ..Limits::default() }), ..Default::default() };
    assert!(kind(&Flare.extract(&project(), &o).expect_err("elements")).is_some());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = Flare.extract(&bytes, &ExtractOptions::default());
        let mut zipish = b"PK\x03\x04".to_vec();
        zipish.extend(bytes);
        let _ = Flare.extract(&zipish, &ExtractOptions::default());
    }

    #[test]
    fn mutated_fixture_never_panics(edits in proptest::collection::vec((any::<proptest::sample::Index>(), any::<u8>()), 1..12)) {
        let mut b = project();
        for (i, v) in &edits {
            let at = i.index(b.len());
            b[at] = *v;
        }
        if let Ok(d) = Flare.extract(&b, &ExtractOptions::default()) {
            let _ = Flare.merge(&d);
        }
    }

    #[test]
    fn mutated_members_never_panic(
        which in 0usize..7,
        edits in proptest::collection::vec((any::<proptest::sample::Index>(), any::<u8>()), 1..8),
        cut in proptest::option::of(any::<proptest::sample::Index>()),
    ) {
        let names = [
            "Content/Topic.htm", "Content/Intl.htm", "Content/Resources/Snippets/Inner.flsnp", "Project/VariableSets/General.flvar",
            "Project/TOCs/Master.fltoc", "Project/Glossaries/Glossary.flglo", "Help.flprj",
        ];
        let (m, _) = members(&project());
        let mut part = get(&m, names[which]).to_vec();
        for (i, v) in &edits {
            let at = i.index(part.len());
            part[at] = *v;
        }
        if let Some(c) = cut {
            part.truncate(c.index(part.len() + 1));
        }
        let bytes = project_with(&[(names[which], &part)], &[]);
        if let Ok(mut d) = Flare.extract(&bytes, &ExtractOptions::default()) {
            d.target_lang = Some("ar".into());
            for i in 0..d.units.len() {
                let t = format!("T({})", loc_text(&d.units[i]));
                let u = &d.units[i];
                let (_, map) = to_locos_text(&u.source());
                let id = u.segments[0].id.clone();
                let _ = d.set_target(&id, from_locos_text(&t, &map));
            }
            if let Ok(out) = Flare.merge(&d) {
                // what we write we can read again
                prop_assert!(Flare.extract(&out, &ExtractOptions::default()).is_ok());
            }
        }
    }

    #[test]
    fn random_topic_text_never_panics(s in "[<>/a-zA-Z:=\"' &;#0-9\\\\\\n]{0,400}") {
        let t = format!("<html xmlns:MadCap=\"x\"><body><p>{s}</p></body></html>");
        let bytes = project_with(&[("Content/Topic.htm", t.as_bytes())], &[]);
        if let Ok(mut d) = Flare.extract(&bytes, &ExtractOptions::default()) {
            prop_assert_eq!(Flare.merge(&d).unwrap(), bytes.clone());
            for i in 0..d.units.len() {
                let t = format!("T({})", loc_text(&d.units[i]));
                let u = &d.units[i];
                let (_, map) = to_locos_text(&u.source());
                let id = u.segments[0].id.clone();
                let _ = d.set_target(&id, from_locos_text(&t, &map));
            }
            let _ = Flare.merge(&d);
        }
    }
}
