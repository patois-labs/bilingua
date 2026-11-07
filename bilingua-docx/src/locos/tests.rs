//! The LocOS-profile DOCX port: shapes the goldens do not cover, hostile input, and a property test of the
//! extract / merge round trip.

use super::*;
use bilingua_model::profile::tokens;
use bilingua_model::{Filter, MergeOptions};
use proptest::prelude::*;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
    let entries: Vec<(bilingua_zip::EntryMeta, Vec<u8>)> =
        members.iter().map(|(n, b)| (bilingua_zip::EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, b.to_vec())).collect();
    bilingua_zip::write_zip(&entries).unwrap()
}

/// A small package: `body` inside `<w:body>`, a styles part with `styles` inside `<w:styles>`, optional extra parts
/// related from the document (`(name, reltype tail, content type, xml)`).
fn docx(body: &str, styles: &str, extra: &[(&str, &str, &str, &str)]) -> Vec<u8> {
    let mut ct = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
    );
    let mut rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>");
    for (i, (name, kind, ctype, _)) in extra.iter().enumerate() {
        ct.push_str(&format!("<Override PartName=\"/word/{name}\" ContentType=\"{ctype}\"/>"));
        rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{name}\"/>", i + 2));
    }
    ct.push_str("</Types>");
    rels.push_str("</Relationships>");
    let doc = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W_NS}\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}</w:body></w:document>");
    let sty = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W_NS}\">{styles}</w:styles>");
    let pkg_rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>";
    let core = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>T</dc:title></cp:coreProperties>";
    let extra_xml: Vec<(String, String)> = extra.iter().map(|(n, _, _, x)| (format!("word/{n}"), x.to_string())).collect();
    let mut members: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", pkg_rels.as_bytes()),
        ("word/document.xml", doc.as_bytes()),
        ("word/_rels/document.xml.rels", rels.as_bytes()),
        ("word/styles.xml", sty.as_bytes()),
        ("docProps/core.xml", core.as_bytes()),
    ];
    for (n, x) in &extra_xml {
        members.push((n.as_str(), x.as_bytes()));
    }
    zip_of(&members)
}

const NORMAL: &str = "<w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val=\"en-US\"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/></w:style>";

fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_profile("locos").with_granularity(Granularity::Paragraph).with_target_lang(target.map(str::to_string))
}

fn units(data: &[u8]) -> Vec<(String, String, String)> {
    let doc = crate::Docx.extract(data, &opts(None)).unwrap();
    doc.units
        .iter()
        .map(|u| {
            let (t, _) = tokens::to_locos_text_for("docx", &u.segments[0].source);
            (u.context.extra[profile::P_INDEX].clone(), t, u.context.style.clone().unwrap_or_default())
        })
        .collect()
}

/// Sets LocOS text on every unit (by p_index) and merges under the profile.
fn build(data: &[u8], target: Option<&str>, texts: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = crate::Docx.extract(data, &opts(target)).unwrap();
    for u in doc.units.clone() {
        let p: usize = u.context.extra[profile::P_INDEX].parse().unwrap();
        if let Some((_, t)) = texts.iter().find(|(q, _)| *q == p) {
            let (_, map) = tokens::to_locos_text_for("docx", &u.segments[0].source);
            profile::locos_set_target(&mut doc, &u.segments[0].id, tokens::from_locos_text(t, &map)).unwrap();
        }
    }
    crate::Docx.merge_with(&doc, &profile::locos().merge_options("docx")).unwrap()
}

fn entry(zip: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut f = z.by_name(name).unwrap();
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).unwrap();
    s
}

#[test]
fn styles_runs_and_numbering() {
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p><w:p/><w:p><w:r><w:t xml:space=\"preserve\">One </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>two</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val=\"Missing\"/></w:pPr><w:r><w:t>Gone</w:t></w:r></w:p>";
    let d = docx(body, NORMAL, &[]);
    assert_eq!(
        units(&d),
        vec![
            ("0".into(), "Title".into(), "Heading 2".into()),
            ("2".into(), "\u{27e6}r1\u{27e7}One \u{27e6}/r1\u{27e7}\u{27e6}r2\u{27e7}two\u{27e6}/r2\u{27e7}".into(), "Normal".into()),
            ("3".into(), "Gone".into(), "Normal".into())
        ]
    );
}

#[test]
fn object_mode_and_alt_text_numbering() {
    let body = "<w:p><w:r><w:t>See</w:t></w:r><w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"P\" descr=\"A red square\" title=\"!!\"/></wp:inline></w:drawing></w:r><w:r><w:t> here</w:t></w:r></w:p><w:p><w:hyperlink r:id=\"x\"><w:r><w:t>link</w:t></w:r></w:hyperlink></w:p>";
    let d = docx(body, NORMAL, &[]);
    let u = units(&d);
    // LocOS numbers the object x3, after the two runs
    assert_eq!(u[0].1, "\u{27e6}r1\u{27e7}See\u{27e6}/r1\u{27e7}\u{27e6}x3\u{27e7}\u{27e6}r2\u{27e7} here\u{27e6}/r2\u{27e7}");
    assert_eq!(u[1].1, "link");
    // alt text after every paragraph; a title without a letter or digit is not a unit
    assert_eq!(u[2], ("2".into(), "A red square".into(), "a11y:img-alt".into()));
    assert_eq!(u.len(), 3);
    // the object goes back where the translation puts it, the link is re-created around its text
    let out = build(&d, None, &[(0, "\u{27e6}r1\u{27e7}Voir\u{27e6}/r1\u{27e7}\u{27e6}x3\u{27e7}\u{27e6}r2\u{27e7} ici\u{27e6}/r2\u{27e7}"), (1, "lien"), (2, "Un carre")]);
    let xml = entry(&out, "word/document.xml");
    assert!(xml.contains("<w:t>Voir</w:t></w:r><w:r><w:drawing>"), "{xml}");
    assert!(xml.contains("<w:hyperlink r:id=\"x\"><w:r><w:t>lien</w:t></w:r></w:hyperlink>"), "{xml}");
    assert!(xml.contains("descr=\"Un carre\""), "{xml}");
}

#[test]
fn declares_the_target_language() {
    let body = "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r></w:p><w:p><w:r><w:t>Kept</w:t></w:r></w:p>";
    let d = docx(body, NORMAL, &[]);
    let out = build(&d, Some("ar-SA"), &[(0, "Marhaba")]);
    let xml = entry(&out, "word/document.xml");
    // RTL paragraph direction and the complex-script twin on the translated run; the untranslated run keeps the
    // source language the old default gave it
    assert!(xml.contains("<w:p><w:pPr><w:bidi/></w:pPr><w:r><w:rPr><w:b/><w:bCs/></w:rPr><w:t>Marhaba</w:t>"), "{xml}");
    // the old default named no complex-script language: nothing to keep
    assert!(xml.contains("<w:p><w:r><w:t>Kept</w:t>"), "{xml}");
    let core = entry(&out, "docProps/core.xml");
    assert!(core.contains("<dc:language>ar-SA</dc:language>"), "{core}");
    let sty = entry(&out, "word/styles.xml");
    assert!(sty.contains("w:bidi=\"ar-SA\""), "{sty}");
    // a German build: the untranslated run is stamped with the source language
    let out = build(&d, Some("de_DE"), &[(0, "Hallo")]);
    let xml = entry(&out, "word/document.xml");
    assert!(xml.contains("<w:r><w:rPr><w:lang w:val=\"en-US\"/></w:rPr><w:t>Kept</w:t>"), "{xml}");
    assert!(entry(&out, "word/styles.xml").contains("w:val=\"de-DE\""));
}

#[test]
fn a_new_rpr_goes_after_the_text_that_leads_a_run() {
    // a run holding bare text (no w:t, found by the random differential): lxml's insert(0, rPr) keeps the run's
    // text before the new rPr, so LocOS writes `<w:r>text<w:rPr>..`
    let body = "<w:p><w:r>bare</w:r></w:p><w:p><w:r><w:t>Kept</w:t></w:r></w:p>";
    let d = docx(body, NORMAL, &[]);
    let out = build(&d, Some("de-DE"), &[(1, "Behalten")]);
    let xml = entry(&out, "word/document.xml");
    assert!(xml.contains("<w:r>bare<w:rPr><w:lang w:val=\"en-US\"/></w:rPr></w:r>"), "{xml}");
}

#[test]
fn nothing_written_returns_the_upload() {
    let d = docx("<w:p><w:r><w:t>Same</w:t></w:r></w:p>", NORMAL, &[]);
    assert_eq!(build(&d, None, &[]), d);
    assert_eq!(build(&d, None, &[(0, "Same")]), d);
}

#[test]
fn headers_comments_and_the_comments_option() {
    let hdr = format!("<w:hdr xmlns:w=\"{W_NS}\"><w:p><w:r><w:t>Head</w:t></w:r></w:p></w:hdr>");
    let com = format!("<w:comments xmlns:w=\"{W_NS}\"><w:comment w:id=\"0\"><w:p><w:r><w:annotationRef/></w:r><w:r><w:t>Note</w:t></w:r></w:p></w:comment></w:comments>");
    let body = "<w:p><w:r><w:t>Body</w:t></w:r><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>";
    let d = docx(
        body,
        NORMAL,
        &[
            ("comments.xml", "comments", "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml", &com),
            ("header1.xml", "header", "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml", &hdr),
        ],
    );
    let u = units(&d);
    assert_eq!(u.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), vec!["Body\u{27e6}x1\u{27e7}", "Head", "\u{27e6}x1\u{27e7}Note"]);
    let off = crate::Docx.extract(&d, &opts(None).with_format_option("docx.translate_comments", "false")).unwrap();
    assert_eq!(off.units.len(), 2);
    let out = build(&d, None, &[(1, "Kopf"), (2, "\u{27e6}x1\u{27e7}Notiz")]);
    assert!(entry(&out, "word/header1.xml").contains("<w:t>Kopf</w:t>"));
    assert!(entry(&out, "word/comments.xml").contains("<w:annotationRef/></w:r><w:r><w:t>Notiz</w:t>"));
}

#[test]
fn hostile_packages_are_refused_or_kept() {
    let good = docx("<w:p><w:r><w:t>x</w:t></w:r></w:p>", NORMAL, &[]);
    // not a zip, an empty zip, no main part, a main part of another type, a broken main part
    for bad in [b"not a zip".to_vec(), zip_of(&[]), zip_of(&[("[Content_Types].xml", b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>")])] {
        assert!(crate::Docx.extract(&bad, &opts(None)).is_err());
    }
    let broken = String::from_utf8_lossy(&entry(&good, "word/document.xml").into_bytes()).replace("</w:body>", "");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&good[..])).unwrap();
    let mut members: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut b).unwrap();
        members.push((f.name().unwrap().to_string(), b));
    }
    let with = |name: &str, data: &[u8]| -> Vec<u8> {
        let m: Vec<(&str, &[u8])> = members.iter().map(|(n, b)| (n.as_str(), if n == name { data } else { b.as_slice() })).collect();
        zip_of(&m)
    };
    assert!(crate::Docx.extract(&with("word/document.xml", broken.as_bytes()), &opts(None)).is_err());
    assert!(crate::Docx.extract(&with("word/document.xml", b"<!DOCTYPE x [<!ENTITY a \"b\">]><x/>"), &opts(None)).is_err());
    // a dangling relationship is refused (python-docx cannot open it either)
    let rels = entry(&good, "word/_rels/document.xml.rels").replace("</Relationships>", "<Relationship Id=\"rId9\" Type=\"urn:x\" Target=\"missing.xml\"/></Relationships>");
    assert!(crate::Docx.extract(&with("word/_rels/document.xml.rels", rels.as_bytes()), &opts(None)).is_err());
    // a footnotes part that is not XML is left out of the walk and kept as it was
    let ct = entry(&good, "[Content_Types].xml").replace("</Types>", "<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/></Types>");
    let rels = entry(&good, "word/_rels/document.xml.rels").replace("</Relationships>", "<Relationship Id=\"rId9\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes\" Target=\"footnotes.xml\"/></Relationships>");
    let mut m: Vec<(&str, &[u8])> = members.iter().map(|(n, b)| (n.as_str(), b.as_slice())).filter(|(n, _)| *n != "[Content_Types].xml" && *n != "word/_rels/document.xml.rels").collect();
    m.push(("[Content_Types].xml", ct.as_bytes()));
    m.push(("word/_rels/document.xml.rels", rels.as_bytes()));
    m.push(("word/footnotes.xml", b"\xff\xfe<not xml"));
    let notes = zip_of(&m);
    let out = build(&notes, Some("fr"), &[(0, "y")]);
    assert_eq!(entry_bytes(&out, "word/footnotes.xml"), b"\xff\xfe<not xml");
    // deep wrapper chains and hostile token text do not crash and lose no text
    // past MAX_INLINE_DEPTH wrappers the content is one object: no text, no unit
    let deep = |n: usize| docx(&format!("<w:p>{}<w:r><w:t>deep</w:t></w:r>{}</w:p>", "<w:smartTag>".repeat(n), "</w:smartTag>".repeat(n)), NORMAL, &[]);
    assert!(units(&deep(40)).is_empty());
    let d = deep(20);
    assert_eq!(units(&d).len(), 1);
    for t in ["\u{27e6}x99\u{27e7}", "\u{27e6}/r1\u{27e7}\u{27e6}r7\u{27e7}a\u{27e6}", "\u{27e6}x1\u{27e7}\u{27e6}x1\u{27e7}", "\u{0}\u{b}tab\there"] {
        let out = build(&d, Some("de"), &[(0, t)]);
        assert!(bilingua_xmltree::parse(&entry(&out, "word/document.xml")).is_ok());
    }
}

fn entry_bytes(zip: &[u8], name: &str) -> Vec<u8> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut f = z.by_name(name).unwrap();
    let mut b = Vec::new();
    std::io::Read::read_to_end(&mut f, &mut b).unwrap();
    b
}

#[test]
fn without_the_profile_nothing_changes() {
    let d = docx("<w:p><w:r><w:t xml:space=\"preserve\">One </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>two</w:t></w:r></w:p>", NORMAL, &[]);
    let doc = crate::Docx.extract(&d, &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    assert!(matches!(doc.skeleton, Skeleton::Zip { .. }));
    let (t, _) = tokens::to_locos_text_as(&doc.units[0].segments[0].source, 'r');
    assert_eq!(t, "One \u{27e6}r1\u{27e7}two\u{27e6}/r1\u{27e7}");
    assert!(crate::Docx.merge_with(&doc, &MergeOptions::new()).is_ok());
}

fn paragraph() -> impl Strategy<Value = String> {
    let run = (proptest::bool::ANY, "[a-zA-Z <&>\"']{0,8}").prop_map(|(bold, t)| {
        let t = t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        format!("<w:r>{}<w:t xml:space=\"preserve\">{t}</w:t></w:r>", if bold { "<w:rPr><w:b/></w:rPr>" } else { "" })
    });
    let piece = prop_oneof![
        4 => run,
        1 => Just("<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>".to_string()),
        1 => Just("<w:bookmarkEnd w:id=\"1\"/>".to_string()),
        1 => Just("<w:r><w:br w:type=\"page\"/></w:r>".to_string()),
        1 => Just("<w:hyperlink r:id=\"h\"><w:r><w:t>link</w:t></w:r></w:hyperlink>".to_string()),
        1 => Just("<w:ins w:id=\"5\" w:author=\"a\"><w:r><w:t>ins</w:t></w:r></w:ins>".to_string()),
    ];
    proptest::collection::vec(piece, 1..6).prop_map(|v| format!("<w:p>{}</w:p>", v.join("")))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Writing each unit's own text back keeps every unit's text, and any token shuffle still gives XML.
    #[test]
    fn round_trip_keeps_units(paras in proptest::collection::vec(paragraph(), 1..4), shuffle in proptest::bool::ANY) {
        let d = docx(&paras.join(""), NORMAL, &[]);
        let before = units(&d);
        let texts: Vec<(usize, String)> = before.iter().map(|(p, t, _)| {
            let t = if shuffle { let mut toks: Vec<&str> = t.split('\u{27e7}').collect(); toks.reverse(); toks.join("\u{27e7}") } else { t.clone() };
            (p.parse().unwrap(), t)
        }).collect();
        let refs: Vec<(usize, &str)> = texts.iter().map(|(p, t)| (*p, t.as_str())).collect();
        let out = build(&d, Some("fr-FR"), &refs);
        let xml = entry(&out, "word/document.xml");
        prop_assert!(bilingua_xmltree::parse(&xml).is_ok());
        if !shuffle {
            let after = units(&out);
            prop_assert_eq!(before.iter().map(|u| (u.0.clone(), u.1.clone())).collect::<Vec<_>>(), after.iter().map(|u| (u.0.clone(), u.1.clone())).collect::<Vec<_>>());
        }
    }
}

/// Sets `target` on unit `i` and merges under the profile.
fn build_inl(data: &[u8], target: Option<&str>, i: usize, t: Vec<Inline>) -> Vec<u8> {
    let mut doc = crate::Docx.extract(data, &opts(target)).unwrap();
    let id = doc.units[i].segments[0].id.clone();
    profile::locos_set_target(&mut doc, &id, t).unwrap();
    crate::Docx.merge_with(&doc, &profile::locos().merge_options("docx")).unwrap()
}

const LIKE: &str = "\u{27e6}x2\u{27e7}";

#[test]
fn literal_token_lookalikes_survive_a_merge() {
    let body = format!("<w:p><w:r><w:t xml:space=\"preserve\">Hi {LIKE} </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>there</w:t></w:r></w:p>");
    let d = docx(&body, NORMAL, &[]);
    let doc = crate::Docx.extract(&d, &opts(None)).unwrap();
    let src = doc.units[0].segments[0].source.clone();
    assert!(src.iter().any(|i| matches!(i, Inline::Text(t) if t.contains(LIKE))));
    // the translation keeps the look-alike where it was
    let kept = vec![
        Inline::tag("1", TagKind::Open, ""),
        Inline::text(format!("Salut {LIKE} ")),
        Inline::tag("1", TagKind::Close, ""),
        Inline::tag("2", TagKind::Open, ""),
        Inline::text("la"),
        Inline::tag("2", TagKind::Close, ""),
    ];
    let xml = entry(&build_inl(&d, Some("fr"), 0, kept), "word/document.xml");
    assert!(xml.contains(&format!("Salut {LIKE} </w:t>")), "{xml}");
    // real codes reordered around it: the look-alike is text of run 1, run 2 (bold) comes first
    let moved = vec![
        Inline::tag("2", TagKind::Open, ""),
        Inline::text("la"),
        Inline::tag("2", TagKind::Close, ""),
        Inline::tag("1", TagKind::Open, ""),
        Inline::text(format!(" Salut {LIKE}")),
        Inline::tag("1", TagKind::Close, ""),
    ];
    let xml = entry(&build_inl(&d, Some("fr"), 0, moved), "word/document.xml");
    let (a, b) = (xml.find("<w:b/>").expect("bold"), xml.find(&format!("Salut {LIKE}")).expect("look-alike kept"));
    assert!(a < b, "{xml}");
    assert!(xml.contains("<w:t>la</w:t>"), "{xml}");
    // a lone literal token in a one-run paragraph is text too
    let d1 = docx("<w:p><w:r><w:t>Plain</w:t></w:r></w:p>", NORMAL, &[]);
    let xml = entry(&build_inl(&d1, Some("fr"), 0, vec![Inline::text(format!("A{LIKE}\u{27e6}r1\u{27e7}"))]), "word/document.xml");
    assert!(xml.contains(&format!("A{LIKE}\u{27e6}r1\u{27e7}")), "{xml}");
}

#[test]
fn literal_lookalikes_survive_object_mode() {
    let body = "<w:p><w:r><w:t>See \u{27e6}x3\u{27e7}</w:t></w:r><w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"P\"/></wp:inline></w:drawing></w:r><w:r><w:t> here</w:t></w:r></w:p>";
    let d = docx(body, NORMAL, &[]);
    // the object is the real x3 and the text holds a look-alike x3; the object moves in front of run 2
    let t = vec![
        Inline::tag("1", TagKind::Open, ""),
        Inline::text("Voir \u{27e6}x3\u{27e7}"),
        Inline::tag("1", TagKind::Close, ""),
        Inline::tag("3", TagKind::Standalone, ""),
        Inline::tag("2", TagKind::Open, ""),
        Inline::text(" ici"),
        Inline::tag("2", TagKind::Close, ""),
    ];
    let xml = entry(&build_inl(&d, Some("fr"), 0, t), "word/document.xml");
    assert!(xml.contains("Voir \u{27e6}x3\u{27e7}</w:t></w:r><w:r><w:drawing>"), "{xml}");
    assert_eq!(xml.matches("<w:drawing>").count(), 1, "{xml}");
}
