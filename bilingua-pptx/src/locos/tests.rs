//! The LocOS-profile PPTX port: walk, tokens, writing, hostile input. The parity with LocOS itself is checked by
//! the goldens (`bilingua/tests/locos/pptx`, hostile decks and targets included); these tests pin the rules and
//! make sure nothing panics or writes a broken package.
use super::*;
use bilingua_model::Filter;
use bilingua_zip::hostile::stored_zip;
use proptest::prelude::*;

const O: char = '\u{27e6}';
const Z: char = '\u{27e7}';

fn opts(lang: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_target_lang(lang.map(str::to_string))
}

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bilingua/tests/locos/pptx").join(name);
    std::fs::read(p).unwrap()
}

/// A one-slide deck whose slide body (inside `p:spTree`) is `body`.
fn deck(body: &str) -> Vec<u8> {
    deck_with(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"{A}\" xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:sld>"
    ))
}

fn deck_with(slide: &str) -> Vec<u8> {
    let rels = |items: &str| format!("<Relationships xmlns=\"{PKG_REL}\">{items}</Relationships>");
    let root = rels(&format!("<Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"ppt/presentation.xml\"/>"));
    let pres = format!("<p:presentation xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>");
    let prels = rels(&format!("<Relationship Id=\"rId1\" Type=\"{R}/slide\" Target=\"slides/slide1.xml\"/>"));
    stored_zip(&[
        ("[Content_Types].xml", b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"),
        ("_rels/.rels", root.as_bytes()),
        ("ppt/presentation.xml", pres.as_bytes()),
        ("ppt/_rels/presentation.xml.rels", prels.as_bytes()),
        ("ppt/slides/slide1.xml", slide.as_bytes()),
    ])
}

fn sp(paras: &str) -> String {
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"S\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/>{paras}</p:txBody></p:sp>")
}

/// (p_index, LocOS text, style) of every unit, as the adapter numbers them.
fn units(doc: &Document) -> Vec<(String, String, String)> {
    doc.units
        .iter()
        .map(|u| {
            let (t, _) = profile::tokens::to_locos_text_for("pptx", &u.segments[0].source);
            (u.context.extra[profile::P_INDEX].clone(), t, u.context.extra[profile::STYLE].clone())
        })
        .collect()
}

/// Extracts, sets each `(p_index, LocOS text)` as plain text (so LocOS's token rules decide) and merges.
fn write(data: &[u8], lang: Option<&str>, set: &[(&str, &str)]) -> Result<Vec<u8>, Error> {
    let mut doc = crate::Pptx.extract(data, &opts(lang))?;
    for (p, t) in set {
        let id = doc.units.iter().find(|u| u.context.extra[profile::P_INDEX] == *p).unwrap().segments[0].id.clone();
        let u = doc.units.iter().find(|u| u.segments[0].id == id).unwrap();
        let (_, map) = profile::tokens::to_locos_text_for("pptx", &u.segments[0].source);
        profile::locos_set_target(&mut doc, &id, profile::tokens::from_locos_text(t, &map))?;
    }
    crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx"))
}

fn part(zip: &[u8], name: &str) -> String {
    let e = bilingua_zip::read_zip("pptx", zip, &ZipLimits::default()).unwrap();
    String::from_utf8(e.into_iter().find(|e| e.name == name).unwrap().data).unwrap()
}

/// Every XML entry of the package parses.
fn well_formed(zip: &[u8]) {
    for e in bilingua_zip::read_zip("pptx", zip, &ZipLimits::default()).unwrap() {
        if is_xml_name(&e.name) && !e.meta.dir {
            let t = decode(&e.data).unwrap();
            assert!(dom::parse(&t).is_ok(), "{} is not well-formed:\n{t}", e.name);
        }
    }
}

#[test]
fn runs_breaks_and_fields_are_numbered_like_locos() {
    let d = deck(&sp("<a:p><a:r><a:rPr b=\"1\"/><a:t>A</a:t></a:r><a:br/><a:r><a:t>B</a:t></a:r><a:fld id=\"{1}\" type=\"slidenum\"><a:t>1</a:t></a:fld></a:p><a:p/><a:p><a:r><a:t>Only</a:t></a:r></a:p>"));
    let doc = crate::Pptx.extract(&d, &opts(None)).unwrap();
    let want = format!("{O}r1{Z}A{O}/r1{Z}{O}x3{Z}{O}r2{Z}B{O}/r2{Z}{O}x4{Z}");
    assert_eq!(units(&doc), vec![("0".into(), want, "slide 1".into()), ("2".into(), "Only".into(), "slide 1".into())]);
    assert!(matches!(doc.skeleton, Skeleton::Binary { .. }));
    // without the profile, the 0.9.0 extraction is unchanged (a zip skeleton, its own ids)
    let plain = crate::Pptx.extract(&d, &ExtractOptions::new()).unwrap();
    assert!(matches!(plain.skeleton, Skeleton::Zip { .. }));
    assert!(plain.units.iter().all(|u| !u.context.extra.contains_key(profile::P_INDEX)));
}

#[test]
fn spans_follow_locos_token_rules() {
    use Span::*;
    let s = |t: &str| spans(t, 2);
    assert_eq!(s(&format!("a{O}r2{Z}b{O}/r1{Z}c")), vec![T(None, "a".into()), T(Some(1), "b".into()), T(None, "c".into())]);
    // out of range run, closing point, unknown letter: literal text; any point number is a point (maybe out of range)
    assert_eq!(s(&format!("{O}r3{Z}{O}/x3{Z}{O}g1{Z}{O}x1{Z}")), vec![T(None, format!("{O}r3{Z}")), T(None, format!("{O}/x3{Z}")), T(None, format!("{O}g1{Z}")), X(-2)]);
    // Unicode decimal digits count (Python's \d and int()), a number past u64 is a point that writes nothing
    assert_eq!(s(&format!("{O}r\u{0662}{Z}x")), vec![T(Some(1), "x".into())]);
    assert_eq!(s(&format!("{O}x99999999999999999999999{Z}")), vec![X(i128::MAX)]);
    // malformed tokens are text
    assert_eq!(s(&format!("{O}{Z}{O}r{Z}{O}R1{Z}")), vec![T(None, format!("{O}{Z}{O}r{Z}{O}R1{Z}"))]);
}

#[test]
fn writing_keeps_points_and_copies_run_properties() {
    let d = deck(&sp("<a:p><a:r><a:rPr b=\"1\"/><a:t>A</a:t></a:r><a:br/><a:r><a:rPr i=\"1\"/><a:t>B</a:t></a:r><a:endParaRPr/></a:p>"));
    // the break is dropped and a new line added: the break comes back at the end, the new line is a new a:br
    let out = write(&d, Some("he-IL"), &[("0", &format!("{O}r2{Z}x\ny{O}/r2{Z}{O}r1{Z}z{O}/r1{Z}"))]).unwrap();
    well_formed(&out);
    let s = part(&out, "ppt/slides/slide1.xml");
    let want = "<a:p><a:pPr rtl=\"1\"/><a:r><a:rPr i=\"1\" lang=\"he-IL\"/><a:t>x</a:t></a:r><a:br><a:rPr i=\"1\" lang=\"he-IL\"/></a:br><a:r><a:rPr i=\"1\" lang=\"he-IL\"/><a:t>y</a:t></a:r><a:r><a:rPr b=\"1\" lang=\"he-IL\"/><a:t>z</a:t></a:r><a:br><a:rPr lang=\"he-IL\"/></a:br><a:endParaRPr lang=\"he-IL\"/></a:p>";
    assert!(s.contains(want), "{s}");
    // nothing written: the input comes back as it was
    assert_eq!(write(&d, Some("de-DE"), &[]).unwrap(), d);
}

#[test]
fn empty_target_keeps_one_empty_run() {
    let d = deck(&sp("<a:p><a:fld id=\"{1}\"><a:t>1</a:t></a:fld><a:r><a:t>Text</a:t></a:r></a:p>"));
    let mut doc = crate::Pptx.extract(&d, &opts(None)).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    profile::locos_set_target(&mut doc, &id, vec![]).unwrap();
    let out = crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx")).unwrap();
    let s = part(&out, "ppt/slides/slide1.xml");
    assert!(s.contains("<a:p><a:r><a:t/></a:r><a:fld id=\"{1}\"><a:t>1</a:t></a:fld></a:p>"), "{s}");
}

#[test]
fn copied_properties_keep_their_namespace() {
    let d = deck(&sp("<a:p><a:r xmlns:q=\"urn:q\"><a:rPr q:x=\"1\"/><a:t>A</a:t></a:r><a:r><a:t>B</a:t></a:r></a:p>"));
    let out = write(&d, None, &[("0", &format!("{O}r1{Z}new{O}/r1{Z}"))]).unwrap();
    well_formed(&out);
    assert!(part(&out, "ppt/slides/slide1.xml").contains("<a:rPr xmlns:q=\"urn:q\" q:x=\"1\"/>"));
}

#[test]
fn strict_namespaces_swap_like_locos() {
    let t = part_to_transitional(b"<a xmlns:p=\"http://purl.oclc.org/ooxml/presentationml/main\" t='http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties'>\"http://purl.oclc.org/ooxml/drawingml/main\"</a>").unwrap();
    assert_eq!(
        String::from_utf8(t.clone()).unwrap(),
        "<a xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" t='http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties'>\"http://schemas.openxmlformats.org/drawingml/2006/main\"</a>"
    );
    assert_eq!(
        String::from_utf8(part_to_strict(&t)).unwrap(),
        "<a xmlns:p=\"http://purl.oclc.org/ooxml/presentationml/main\" t='http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties'>\"http://purl.oclc.org/ooxml/drawingml/main\"</a>"
    );
    // a Strict name LocOS does not know refuses the part; the package and markup-compatibility names stay
    assert!(part_to_transitional(b"<a x=\"http://purl.oclc.org/ooxml/unknown\"/>").is_none());
    let keep = b"<a x=\"http://schemas.openxmlformats.org/package/2006/relationships\" y='http://schemas.openxmlformats.org/markup-compatibility/2006'/>";
    assert_eq!(part_to_strict(keep), keep.to_vec());
}

#[test]
fn paths_resolve_like_posixpath() {
    assert_eq!(resolve("ppt/slides/slide1.xml", "../media/a.png").as_deref(), Some("ppt/media/a.png"));
    assert_eq!(resolve("ppt/slides/slide1.xml", "/ppt/./charts//c.xml").as_deref(), Some("ppt/charts/c.xml"));
    assert_eq!(resolve("", "ppt/presentation.xml").as_deref(), Some("ppt/presentation.xml"));
    assert_eq!(resolve("ppt/slides/slide1.xml", "../../../x.xml"), None);
    assert_eq!(resolve("ppt/slides/slide1.xml", "..").as_deref(), Some("ppt"));
    assert_eq!(resolve("ppt/slides/slide1.xml", "").as_deref(), Some("ppt/slides"));
    assert_eq!(resolve("ppt/slides/slide1.xml", "/"), None);
    assert_eq!(resolve("ppt/a.xml", "../../x"), None);
    assert_eq!(resolve("a.xml", "..x/y"), None); // LocOS refuses any path starting with "..", even a name
    assert_eq!(rels_name(""), "_rels/.rels");
    assert_eq!(rels_name("ppt/presentation.xml"), "ppt/_rels/presentation.xml.rels");
}

#[test]
fn hostile_packages_are_refused_not_panicking() {
    let slide = |s: &str| deck_with(s);
    for bad in [
        b"not a zip at all".to_vec(),
        stored_zip(&[("a.xml", b"<a/>")]),
        slide("<!DOCTYPE p:sld [<!ENTITY x 'y'>]><p:sld/>"),
        slide("<p:sld xmlns:p=\"urn:p\"><unclosed></p:sld>"),
        slide("<p:sld>&undefined;</p:sld>"),
    ] {
        assert!(crate::Pptx.extract(&bad, &opts(None)).is_err());
    }
    // a slide in an unknown Strict namespace refuses the file, as LocOS does
    let strict = {
        let rels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{STRICT_RELS}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>");
        stored_zip(&[("_rels/.rels", rels.as_bytes()), ("ppt/presentation.xml", b"<p:presentation xmlns:p=\"http://purl.oclc.org/ooxml/presentationml/unknown\"/>")])
    };
    assert!(crate::Pptx.extract(&strict, &opts(None)).is_err());
    // a slide outside the package, an external slide, an empty slide list: no units, no error
    let d = deck("");
    assert!(crate::Pptx.extract(&d, &opts(None)).unwrap().units.is_empty());
}

fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z &<>\"']{0,6}",
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("\u{1}\u{b}\u{fffe}".to_string()),
        (0u8..7, any::<bool>()).prop_map(|(n, c)| format!("{O}{}r{n}{Z}", if c { "/" } else { "" })),
        (0u8..9, any::<bool>()).prop_map(|(n, c)| format!("{O}{}x{n}{Z}", if c { "/" } else { "" })),
        Just(format!("{O}g1{Z}{O}{Z}{O}")),
        Just("\u{1F600}\u{0627}".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn any_targets_give_a_well_formed_package(targets in proptest::collection::vec(proptest::collection::vec(piece(), 0..6), 1..20), lang in prop_oneof![Just(None), Just(Some("ar-SA")), Just(Some("de-DE"))]) {
        let data = fixture("pptx_filter_1.pptx");
        let mut doc = crate::Pptx.extract(&data, &opts(lang)).unwrap();
        let ids: Vec<String> = doc.units.iter().map(|u| u.segments[0].id.clone()).collect();
        for (id, t) in ids.iter().zip(&targets) {
            let _ = profile::locos_set_target(&mut doc, id, vec![Inline::text(t.concat())]);
        }
        let out = crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx")).unwrap();
        well_formed(&out);
        // the result is a deck LocOS's walk reads again, with the same paragraph slots
        let again = crate::Pptx.extract(&out, &opts(None)).unwrap();
        prop_assert!(again.units.iter().all(|u| u.context.extra[profile::P_INDEX].parse::<usize>().is_ok()));
    }

    #[test]
    fn plain_targets_read_back(t in "[a-zA-Z0-9][a-zA-Z0-9 &<>\"']{0,20}") {
        let d = deck(&sp("<a:p><a:r><a:t>Hello</a:t></a:r></a:p>"));
        let out = write(&d, Some("fr-FR"), &[("0", &t)]).unwrap();
        let again = crate::Pptx.extract(&out, &opts(None)).unwrap();
        prop_assert_eq!(units(&again), vec![("0".to_string(), t, "slide 1".to_string())]);
    }

    #[test]
    fn damaged_slides_never_panic(cut in 0usize..400, byte in any::<u8>()) {
        let mut s = format!("<p:sld xmlns:a=\"{A}\" xmlns:p=\"{P}\"><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:sld>", sp("<a:p><a:r><a:t>A</a:t></a:r><a:br/></a:p>")).into_bytes();
        let i = cut % s.len();
        s[i] = byte;
        let d = deck_with(&String::from_utf8_lossy(&s));
        if let Ok(doc) = crate::Pptx.extract(&d, &opts(Some("ar-SA"))) {
            let _ = crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx"));
        }
    }
}

fn write_inl(data: &[u8], lang: Option<&str>, t: Vec<Inline>) -> Vec<u8> {
    let mut doc = crate::Pptx.extract(data, &opts(lang)).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    profile::locos_set_target(&mut doc, &id, t).unwrap();
    crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx")).unwrap()
}

#[test]
fn literal_token_lookalikes_survive_a_merge() {
    // run 1 holds a look-alike of the real break x3; the translation keeps it, then reorders the real codes
    let d = deck(&sp(&format!("<a:p><a:r><a:t>Go {O}x3{Z}</a:t></a:r><a:br/><a:r><a:rPr b=\"1\"/><a:t>B</a:t></a:r></a:p>")));
    let like = format!("{O}x3{Z}");
    let t = |first: bool| {
        let one = vec![Inline::tag("1", TagKind::Open, ""), Inline::text(format!("Va {like}")), Inline::tag("1", TagKind::Close, "")];
        let two = vec![Inline::tag("2", TagKind::Open, ""), Inline::text("B"), Inline::tag("2", TagKind::Close, "")];
        let br = vec![Inline::tag("3", TagKind::Standalone, "")];
        if first { [one, br, two].concat() } else { [two, br, one].concat() }
    };
    for first in [true, false] {
        let out = write_inl(&d, Some("fr"), t(first));
        well_formed(&out);
        let s = part(&out, "ppt/slides/slide1.xml");
        assert!(s.contains(&format!("<a:t>Va {like}</a:t>")), "{s}");
        assert_eq!(s.matches("<a:br").count(), 1, "{s}");
        assert_eq!(s.find("Va ").unwrap() < s.find("<a:br").unwrap(), first, "{s}");
    }
    // a one-run paragraph keeps a look-alike too
    let d1 = deck(&sp("<a:p><a:r><a:t>Plain</a:t></a:r></a:p>"));
    let out = write_inl(&d1, Some("fr"), vec![Inline::text(format!("A {O}r1{Z} {O}x1{Z}"))]);
    assert!(part(&out, "ppt/slides/slide1.xml").contains(&format!("<a:t>A {O}r1{Z} {O}x1{Z}</a:t>")));
}

const PUA: &str = "a\u{e000}\u{e001}b\u{e000}\u{e000}c\u{e000}\u{27e6}x1\u{27e7}\u{e001}d";

#[test]
fn private_use_characters_round_trip_in_runs_and_alt_text() {
    let body = format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"S\" descr=\"Alt {PUA}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>{PUA}</a:t></a:r><a:br/><a:r><a:rPr b=\"1\"/><a:t>B{PUA}</a:t></a:r></a:p><a:p><a:r><a:t>One {PUA}</a:t></a:r></a:p></p:txBody></p:sp>"
    );
    let d = deck(&body);
    let mut doc = crate::Pptx.extract(&d, &opts(Some("fr"))).unwrap();
    assert_eq!(doc.units.len(), 3);
    for u in doc.units.clone() {
        profile::locos_set_target(&mut doc, &u.segments[0].id, u.segments[0].source.clone()).unwrap();
    }
    let out = crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx")).unwrap();
    well_formed(&out);
    let s = part(&out, "ppt/slides/slide1.xml");
    assert!(s.contains(&format!("<a:t>{PUA}</a:t>")), "{s}");
    assert!(s.contains(&format!("<a:t>B{PUA}</a:t>")), "{s}");
    assert!(s.contains(&format!("<a:t>One {PUA}</a:t>")), "{s}");
    assert!(s.contains(&format!("descr=\"Alt {PUA}\"")), "{s}");
}

#[test]
fn public_rebuild_still_reads_plain_locos_text() {
    // `rebuild` takes LocOS text: its tokens are read as tokens and its private-use characters are kept
    let d = deck(&sp("<a:p><a:r><a:t>A</a:t></a:r><a:br/><a:r><a:rPr b=\"1\"/><a:t>B</a:t></a:r></a:p>"));
    let text = format!("{O}r2{Z}Y{O}/r2{Z}{O}x3{Z}{O}r1{Z}X \u{e000}\u{e001}\u{e000}\u{e000}{O}/r1{Z}");
    let out = rebuild(&d, &[(0, text)], None).unwrap();
    let s = part(&out, "ppt/slides/slide1.xml");
    assert!(s.contains("<a:rPr b=\"1\"/><a:t>Y</a:t></a:r><a:br/><a:r><a:t>X \u{e000}\u{e001}\u{e000}\u{e000}</a:t>"), "{s}");
    // and `target_text` is still plain LocOS text
    assert_eq!(target_text(&[Inline::text(format!("{O}x1{Z}\u{e000}"))]), format!("{O}x1{Z}\u{e000}"));
}

/// A one-slide deck with a classic comments part and a modern one (PowerPoint 365), both related to the slide.
fn deck_with_comments() -> Vec<u8> {
    let rels = |items: &str| format!("<Relationships xmlns=\"{PKG_REL}\">{items}</Relationships>");
    let root = rels(&format!("<Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"ppt/presentation.xml\"/>"));
    let pres = format!("<p:presentation xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>");
    let prels = rels(&format!("<Relationship Id=\"rId1\" Type=\"{R}/slide\" Target=\"slides/slide1.xml\"/>"));
    let slide = format!("<p:sld xmlns:a=\"{A}\" xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:sld>", sp("<a:p><a:r><a:t>Body</a:t></a:r></a:p>"));
    let srels = rels(&format!(
        "<Relationship Id=\"rId2\" Type=\"{R}/comments\" Target=\"../comments/comment1.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.microsoft.com/office/2018/10/relationships/comments\" Target=\"../comments/modernComment_1.xml\"/>"
    ));
    let classic = format!("<p:cmLst xmlns:p=\"{P}\"><p:cm authorId=\"0\" idx=\"1\"><p:pos x=\"1\" y=\"1\"/><p:text>  Check this.  </p:text></p:cm><p:cm authorId=\"0\" idx=\"2\"><p:text>--</p:text></p:cm></p:cmLst>");
    let modern = format!(
        "<p188:cmLst xmlns:a=\"{A}\" xmlns:p188=\"{P188}\"><p188:cm id=\"{{1}}\" authorId=\"{{A}}\"><p188:txBody><a:bodyPr/><a:p><a:r><a:t>Modern </a:t></a:r><a:r><a:rPr b=\"1\"/><a:t>note</a:t></a:r></a:p></p188:txBody><p188:replyLst><p188:reply id=\"{{2}}\" authorId=\"{{A}}\"><p188:txBody><a:bodyPr/><a:p><a:r><a:t>A reply</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst></p188:cm></p188:cmLst>"
    );
    stored_zip(&[
        ("[Content_Types].xml", b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"),
        ("_rels/.rels", root.as_bytes()),
        ("ppt/presentation.xml", pres.as_bytes()),
        ("ppt/_rels/presentation.xml.rels", prels.as_bytes()),
        ("ppt/slides/slide1.xml", slide.as_bytes()),
        ("ppt/slides/_rels/slide1.xml.rels", srels.as_bytes()),
        ("ppt/comments/comment1.xml", classic.as_bytes()),
        ("ppt/comments/modernComment_1.xml", modern.as_bytes()),
    ])
}

fn comment_opts() -> ExtractOptions {
    opts(Some("de-DE")).with_format_option("pptx.translate_comments", true)
}

#[test]
fn slide_comments_are_units_like_locos() {
    let d = deck_with_comments();
    // without the option: the body only, as before
    let plain = crate::Pptx.extract(&d, &opts(None)).unwrap();
    assert_eq!(units(&plain), vec![("0".into(), "Body".into(), "slide 1".into())]);
    let doc = crate::Pptx.extract(&d, &comment_opts()).unwrap();
    assert!(doc.warnings.iter().all(|w| w.kind != "format_option"), "{:?}", doc.warnings);
    let c = "slide 1 comment".to_string();
    assert_eq!(
        units(&doc),
        vec![
            ("0".into(), "Body".into(), "slide 1".into()),
            // the classic comment is stripped; the second one ("--") has no letter, so 1000001 is skipped
            ("1000000".into(), "Check this.".into(), c.clone()),
            ("1000002".into(), format!("{O}r1{Z}Modern {O}/r1{Z}{O}r2{Z}note{O}/r2{Z}"), c.clone()),
            ("1000003".into(), "A reply".into(), c),
        ]
    );
    assert_eq!(doc.units[1].context.kind.as_deref(), Some("comment"));
}

#[test]
fn slide_comments_are_written_back() {
    let d = deck_with_comments();
    let mut doc = crate::Pptx.extract(&d, &comment_opts()).unwrap();
    let set = [("1000000", "Pr\u{fc}fen."), ("1000002", &*format!("{O}r2{Z}Neue{O}/r2{Z}{O}r1{Z} Notiz{O}/r1{Z}")), ("1000003", "Eine Antwort")];
    for (p, t) in set {
        let u = doc.units.iter().find(|u| u.context.extra[profile::P_INDEX] == p).unwrap();
        let id = u.segments[0].id.clone();
        let (_, map) = profile::tokens::to_locos_text_for("pptx", &u.segments[0].source);
        profile::locos_set_target(&mut doc, &id, profile::tokens::from_locos_text(t, &map)).unwrap();
    }
    let out = crate::Pptx.merge_with(&doc, &profile::locos().merge_options("pptx")).unwrap();
    well_formed(&out);
    // the classic comment keeps its own white space around the translation
    let classic = part(&out, "ppt/comments/comment1.xml");
    assert!(classic.contains("<p:text>  Pr\u{fc}fen.  </p:text>"), "{classic}");
    assert!(classic.contains("<p:text>--</p:text>"), "{classic}");
    let modern = part(&out, "ppt/comments/modernComment_1.xml");
    assert!(modern.contains("<a:r><a:rPr b=\"1\" lang=\"de-DE\"/><a:t>Neue</a:t></a:r><a:r><a:rPr lang=\"de-DE\"/><a:t> Notiz</a:t></a:r>"), "{modern}");
    assert!(modern.contains("<a:t>Eine Antwort</a:t>"), "{modern}");
    // the slide and every other part keep their bytes
    assert_eq!(part(&out, "ppt/slides/slide1.xml"), part(&d, "ppt/slides/slide1.xml"));
    let back = crate::Pptx.extract(&out, &comment_opts()).unwrap();
    assert_eq!(units(&back)[1].1, "Pr\u{fc}fen.");
}

#[test]
fn a_broken_comments_part_only_matters_when_comments_are_wanted() {
    let d = deck_with_comments();
    let entries = bilingua_zip::read_zip("pptx", &d, &ZipLimits::default()).unwrap();
    let mut items: Vec<(String, Vec<u8>)> = entries.into_iter().map(|e| (e.name, e.data)).collect();
    for (n, b) in &mut items {
        if n == "ppt/comments/comment1.xml" {
            *b = b"<p:cmLst><p:cm>".to_vec();
        }
    }
    let refs: Vec<(&str, &[u8])> = items.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
    let broken = stored_zip(&refs);
    assert!(crate::Pptx.extract(&broken, &opts(None)).is_ok());
    assert!(crate::Pptx.extract(&broken, &comment_opts()).is_err());
}

/// W-D D4 (sec-2): the segment cap counts every walked paragraph, empty ones too, as LocOS's walk does. A deck with
/// 2 units and 5 paragraphs is read under a cap of 5 and refused under a cap of 4; native mode counts units only.
#[test]
fn segment_cap_counts_empty_paragraphs() {
    let empty = "<a:p/>";
    let data = deck(&sp(&format!("<a:p><a:r><a:t>One</a:t></a:r></a:p>{empty}{empty}<a:p><a:r><a:t>Two</a:t></a:r></a:p>{empty}")));
    let capped = |n: usize| profile::locos().extract_options().with_limits(bilingua_model::Limits::new().with_max_units(n));
    let doc = crate::Pptx.extract(&data, &capped(5)).unwrap();
    assert_eq!(doc.units.len(), 2);
    match crate::Pptx.extract(&data, &capped(4)) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, bilingua_model::LimitKind::Units),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // native: 2 units, so a cap of 2 passes
    let native = ExtractOptions::new().with_limits(bilingua_model::Limits::new().with_max_units(2));
    assert_eq!(crate::Pptx.extract(&data, &native).unwrap().units.len(), 2);
}
