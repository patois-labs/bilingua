//! Behaviour beyond the LocOS suite: 11 languages, encodings, comments, right-to-left sheets, byte-for-byte parts,
//! segments, the skeleton in JSON, and the point codes.
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;

const LINES: [(&str, &str); 11] = [
    ("en", "Welcome to the harbor lantern show"),
    ("es", "Bienvenidos al espect\u{e1}culo del faro"),
    ("fr", "Bienvenue au spectacle de la lanterne"),
    ("it", "Benvenuti allo spettacolo del faro"),
    ("de", "Willkommen zur Laternenshow am Hafen"),
    ("nl", "Welkom bij de lantaarnshow in de haven"),
    ("pt", "Bem-vindos ao espet\u{e1}culo da lanterna"),
    ("ja", "\u{6e2f}\u{306e}\u{30e9}\u{30f3}\u{30bf}\u{30f3}\u{30b7}\u{30e7}\u{30fc}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}"),
    ("zh", "\u{6b22}\u{8fce}\u{89c2}\u{770b}\u{6e2f}\u{53e3}\u{706f}\u{7b3c}\u{8868}\u{6f14}"),
    ("ko", "\u{d56d}\u{ad6c} \u{b4f1}\u{bd88} \u{c1fc}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{639}\u{631}\u{636} \u{641}\u{627}\u{646}\u{648}\u{633} \u{627}\u{644}\u{645}\u{64a}\u{646}\u{627}\u{621}"),
];

fn lang_doc() -> Vec<u8> {
    let mut c = String::new();
    for (i, (_, t)) in LINES.iter().enumerate() {
        c.push_str(&format!("<text:p>{t} <text:span text:style-name=\"T1\">{i}</text:span> &amp; &lt;done&gt;</text:p>"));
    }
    let content = odt_content().replace("<text:p>12345</text:p>", &format!("{c}<text:p>12345</text:p>"));
    Pkg::new(content).styles(odt_styles()).build()
}

#[test]
fn eleven_languages_read_and_write() {
    let data = lang_doc();
    let d = extract("odt", &data, None);
    let t: Vec<String> = d.units.iter().map(ltext).collect();
    for (i, (_, line)) in LINES.iter().enumerate() {
        assert!(t.contains(&format!("{line} \u{27e6}g1\u{27e7}{i}\u{27e6}/g1\u{27e7} & <done>")), "{line}");
    }
    // each language becomes the target of every other unit, with every target locale
    for (lang, target) in LINES {
        let f = filter_for("odt");
        let mut d = f.extract(&data, &opts(Some(lang))).unwrap();
        for u in d.units.iter_mut() {
            let (text, map) = locos::to_locos_text(&u.source());
            u.segments[0].target = locos::from_locos_text(&format!("{target} / {text}"), &map);
        }
        let out = f.merge(&d).unwrap();
        let back: Vec<String> = extract("odt", &out, None).units.iter().map(ltext).collect();
        let want: Vec<String> = d.units.iter().map(|u| format!("{target} / {}", ltext(u))).collect();
        assert_eq!(back, want, "{lang}");
        let styles = member_str(&out, "styles.xml");
        let ds = element(&styles, r#"<style:default-style style:family="paragraph">"#, "</style:default-style>");
        match lang {
            "ja" | "zh" | "ko" => assert!(ds.contains(&format!(r#"style:language-asian="{lang}""#)), "{lang}: {ds}"),
            "ar" => assert!(ds.contains(r#"style:language-complex="ar""#) && ds.contains("rl-tb"), "{lang}: {ds}"),
            _ => assert!(ds.contains(&format!(r#"fo:language="{lang}""#)), "{lang}: {ds}"),
        }
        assert!(member_str(&out, "meta.xml").contains(&format!("<dc:language>{lang}</dc:language>")));
    }
}

#[test]
fn markup_characters_in_text_are_escaped() {
    let data = odt();
    let out = build("odt", &data, &[(0, "Tom & Jerry <b> \"quotes\" ]]> \u{1}\u{b}x")], None);
    let c = member_str(&out, "content.xml");
    assert!(c.contains("Tom &amp; Jerry &lt;b&gt; \"quotes\" ]]&gt; <text:line-break/>x"), "{c}");
    assert_eq!(units("odt", &out)[0].1, "Tom & Jerry <b> \"quotes\" ]]> \u{27e6}x1\u{27e7}x");
}

#[test]
fn point_and_paired_codes_have_display_and_labels() {
    let d = extract("odt", &odt(), None);
    let u = &d.units[3];
    let disp: Vec<String> = u
        .source()
        .iter()
        .filter_map(|i| if let Inline::Tag { display, kind, .. } = i { Some(format!("{kind:?}:{display}")) } else { None })
        .collect();
    assert_eq!(
        disp,
        vec![
            "Standalone:<text:tab/>",
            r#"Open:<text:a xlink:href="https://x.example/">"#,
            "Close:</text:a>",
            r#"Standalone:<text:bookmark text:name="b1"/>"#
        ]
    );
    assert_eq!(u.codes["2"].label.as_deref(), Some("link"));
    assert_eq!(u.codes["1"].label.as_deref(), Some("tab"));
    // the footnote anchor is one point code; the footnote body is its own unit
    let u1 = &d.units[1];
    assert!(u1.codes["2"].standalone.as_ref().unwrap().starts_with("<text:note "));
    assert_eq!(d.units[2].context.kind.as_deref(), Some("paragraph"));
    assert_eq!(d.units[1].context.location.as_deref(), Some("content.xml paragraph 1"));
}

#[test]
fn comments_are_units_only_when_asked() {
    let data = odt();
    let f = Odt::with_options(OdfOptions { translate_comments: true });
    let mut d = f.extract(&data, &opts(None)).unwrap();
    let c: Vec<&Unit> = d.units.iter().filter(|u| u.id.starts_with('c')).collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].id, "c0");
    assert_eq!(p_index(c[0]), 1_000_000);
    assert_eq!(c[0].context.kind.as_deref(), Some("comment"));
    assert_eq!(ltext(c[0]), "Comment text");
    set(&mut d, 1_000_000, "Kommentartext");
    let out = f.merge(&d).unwrap();
    let content = member_str(&out, "content.xml");
    assert!(content.contains("<dc:creator>Bo</dc:creator><text:p>Kommentartext</text:p></office:annotation>"), "{content}");
    // the host paragraph is untouched
    assert_eq!(units("odt", &out), expected_without_comment());
    // off: not units
    assert!(!extract("odt", &data, None).units.iter().any(|u| u.id.starts_with('c')));
}

fn expected_without_comment() -> Vec<(usize, String)> {
    units("odt", &odt())
}

#[test]
fn ods_sheets_turn_right_to_left() {
    let plain = r#"<table:table table:name="A"><table:table-row><table:table-cell office:value-type="string"><text:p>Hello</text:p></table:table-cell></table:table-row></table:table>"#;
    let styled = r#"<table:table table:name="B" table:style-name="ta1"><table:table-row><table:table-cell office:value-type="string"><text:p>World</text:p></table:table-cell></table:table-row></table:table>"#;
    let with_tp = r#"<table:table table:name="C" table:style-name="ta2"><table:table-row><table:table-cell office:value-type="string"><text:p>Again</text:p></table:table-cell></table:table-row></table:table>"#;
    let common_styled = r#"<table:table table:name="D" table:style-name="Common"><table:table-row><table:table-cell office:value-type="string"><text:p>Last</text:p></table:table-cell></table:table-row></table:table>"#;
    let autos = concat!(
        r#"<office:automatic-styles><style:style style:name="ta1" style:family="table"><style:table-properties table:display="true"/></style:style>"#,
        r#"<style:style style:name="ta2" style:family="table"><style:table-properties style:writing-mode="lr-tb"/></style:style></office:automatic-styles>"#
    );
    let doc = |a: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NSDECL}>{a}<office:body><office:spreadsheet>{plain}{styled}{with_tp}{common_styled}</office:spreadsheet></office:body></office:document-content>"
        )
    };
    for a in [autos, ""] {
        let data = Pkg::new(doc(a)).mime(MIME_SHEET).build();
        let out = build("ods", &data, &[(0, "Hallo")], Some("he-IL"));
        let c = member_str(&out, "content.xml");
        // every sheet names a style that says rl-tb
        let styles: Vec<&str> = c.split("<style:style ").skip(1).map(|s| s.split("</style:style>").next().unwrap()).collect();
        let rtl_names: Vec<String> = styles
            .iter()
            .filter(|s| s.contains(r#"style:family="table""#) && s.contains(r#"style:writing-mode="rl-tb""#))
            .map(|s| s.split(r#"style:name=""#).nth(1).unwrap().split('"').next().unwrap().to_string())
            .collect();
        for sheet in c.split("<table:table ").skip(1) {
            let n = sheet.split(r#"table:style-name=""#).nth(1).unwrap_or_else(|| panic!("a sheet without a style: {sheet}")).split('"').next().unwrap();
            assert!(rtl_names.iter().any(|r| r == n), "sheet style {n} is not rtl: {rtl_names:?}\n{c}");
        }
        // the existing ta2 keeps lr-tb nowhere: it was turned
        assert!(!c.contains(r#"style:writing-mode="lr-tb""#));
        assert!(c.contains(r#"style:parent-style-name="Common""#));
        assert_eq!(units("ods", &out).len(), 4);
        // left-to-right target: sheets are not touched
        let ltr = build("ods", &data, &[(0, "Hallo")], Some("de-DE"));
        assert!(!member_str(&ltr, "content.xml").contains("rl-tb"));
    }
}

#[test]
fn untouched_parts_keep_their_bytes_and_compression() {
    let data = odt();
    let out = build("odt", &data, &[(0, "Erste Schritte")], None);
    let (a, b) = (entries(&data), entries(&out));
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.name, y.name);
        if x.name != "content.xml" {
            assert_eq!(x.data, y.data, "{}", x.name);
            assert_eq!(x.meta.stored, y.meta.stored, "{}", x.name);
            assert_eq!(x.meta.modified, y.meta.modified, "{}", x.name);
            assert_eq!(x.packed.as_ref().map(|p| &p.1), y.packed.as_ref().map(|p| &p.1), "{}: compressed bytes", x.name);
        }
    }
    // with a target language only the parts that carry it change
    let out = build("odt", &data, &[(0, "Erste Schritte")], Some("de-DE"));
    for n in ["META-INF/manifest.xml", "Pictures/a.png", "mimetype"] {
        assert_eq!(member(&out, n), member(&data, n), "{n}");
    }
    // every part of the result is well formed (it reads back)
    assert_eq!(units("odt", &out).len(), 11);
}

#[test]
fn only_the_changed_paragraph_changes() {
    let data = odt();
    let out = build("odt", &data, &[(0, "Erste Schritte")], None);
    let (a, b) = (member_str(&data, "content.xml"), member_str(&out, "content.xml"));
    assert_eq!(a.replace("Getting started", "Erste Schritte"), b);
}

#[test]
fn encodings_are_kept() {
    // UTF-8 with a byte-order mark and CRLF line ends
    let c = format!("\u{feff}{}", odt_content().replace("><", ">\r\n<"));
    let data = Pkg::new(c.clone()).styles(odt_styles()).build();
    assert_eq!(units("odt", &data).len(), 11);
    assert_eq!(build("odt", &data, &[], None), data);
    let out = build("odt", &data, &[(0, "Erste Schritte")], None);
    let new = member(&out, "content.xml");
    assert!(new.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM kept");
    let s = String::from_utf8(new).unwrap();
    assert_eq!(s, c.replace("Getting started", "Erste Schritte"), "CRLF and everything else kept");
    // ISO-8859-1 declared: read as Latin-1, written as Latin-1 while the text fits
    let latin = odt_content().replace("encoding=\"UTF-8\"", "encoding=\"ISO-8859-1\"").replace("Unicode caf\u{e9} \u{65e5}\u{672c} \u{1F600}", "Unicode caf\u{e9}");
    let bytes = latin.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
    let zip = latin1_package(&bytes);
    let got = units("odt", &zip);
    assert_eq!(got[8].1, "Unicode caf\u{e9}");
    assert_eq!(build("odt", &zip, &[], None), zip);
    let out = build("odt", &zip, &[(10, "Caf\u{e9} cr\u{e8}me")], None);
    let raw = member(&out, "content.xml");
    assert!(raw.windows(9).any(|w| w == b"Caf\xe9 cr\xe8m"), "written as Latin-1");
    assert!(String::from_utf8_lossy(&raw).contains("ISO-8859-1"));
    // text that does not fit goes out as UTF-8 under a UTF-8 declaration
    let out = build("odt", &zip, &[(10, "\u{65e5}\u{672c}\u{8a9e}")], None);
    let raw = member_str(&out, "content.xml");
    assert!(raw.contains("encoding=\"UTF-8\"") && raw.contains("\u{65e5}\u{672c}\u{8a9e}"));
    assert_eq!(units("odt", &out)[8].1, "\u{65e5}\u{672c}\u{8a9e}");
    // UTF-16 with a byte-order mark
    let mut u16b = vec![0xFF, 0xFE];
    for u in odt_content().replace("UTF-8", "UTF-16").encode_utf16() {
        u16b.extend_from_slice(&u.to_le_bytes());
    }
    let zip16 = replace_content(&odt(), &u16b);
    assert_eq!(units("odt", &zip16).len(), 11);
    let out = build("odt", &zip16, &[(0, "Erste Schritte")], None);
    let raw = member(&out, "content.xml");
    assert!(raw.starts_with(&[0xFF, 0xFE]), "UTF-16 kept");
    assert_eq!(units("odt", &out)[0].1, "Erste Schritte");
}

fn latin1_package(content: &[u8]) -> Vec<u8> {
    replace_content(&Pkg::new(String::new()).styles(odt_styles()).build(), content)
}

/// The package with content.xml replaced by raw bytes.
fn replace_content(pkg: &[u8], content: &[u8]) -> Vec<u8> {
    use bilingua_zip::write_zip;
    let list: Vec<_> = entries(pkg).into_iter().map(|e| (e.meta, if e.name == "content.xml" { content.to_vec() } else { e.data })).collect();
    write_zip(&list).unwrap()
}

#[test]
fn undeclared_non_utf8_is_refused() {
    let mut bytes = b"<?xml version=\"1.0\"?><office:document-content ".to_vec();
    bytes.extend_from_slice(NSDECL.as_bytes());
    bytes.extend_from_slice(b"><office:body><office:text><text:p>caf\xe9</text:p></office:text></office:body></office:document-content>");
    let zip = replace_content(&odt(), &bytes);
    match Odt::default().extract(&zip, &opts(None)) {
        Err(Error::Encoding(m)) => assert!(m.contains("declares no encoding"), "{m}"),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
}

#[test]
fn skeleton_survives_json() {
    let data = odt();
    let f = Odt::default();
    let mut d = f.extract(&data, &opts(Some("de-DE"))).unwrap();
    set(&mut d, 0, "Erste Schritte");
    let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(back, d);
    assert_eq!(f.merge(&back).unwrap(), f.merge(&d).unwrap());
    let untouched: Document = serde_json::from_str(&serde_json::to_string(&f.extract(&data, &opts(None)).unwrap()).unwrap()).unwrap();
    assert_eq!(f.merge(&untouched).unwrap(), data);
}

#[test]
fn several_segments_join_with_their_separators() {
    let data = odt();
    let f = Odt::default();
    let mut d = f.extract(&data, &opts(None)).unwrap();
    let u = d.units.iter_mut().find(|u| p_index(u) == 2).unwrap();
    u.segments = vec![
        Segment { id: "p2#1".into(), source: vec![Inline::text("A footnote")], target: vec![Inline::text("Eine Fu\u{df}note")], state: SegState::Translated, origin: None },
        Segment { id: "p2#2".into(), source: vec![Inline::text("about %1$s.")], target: vec![], state: SegState::New, origin: None },
    ];
    u.separators = vec![" ".into()];
    let out = f.merge(&d).unwrap();
    assert_eq!(units("odt", &out)[2].1, "Eine Fu\u{df}note about %1$s.");
}

#[test]
fn locked_and_foreign_units_are_refused_or_skipped() {
    let data = odt();
    let f = Odt::default();
    let mut d = f.extract(&data, &opts(None)).unwrap();
    d.units[0].segments[0].target = vec![Inline::text("x")];
    d.units[0].id = "zzz".into();
    assert!(matches!(f.merge(&d), Err(Error::Write(_))));
    let mut d = f.extract(&data, &opts(None)).unwrap();
    d.units[0].segments[0].target = vec![Inline::text("x")];
    d.units[0].id = "p9999".into();
    assert!(matches!(f.merge(&d), Err(Error::Write(_))));
    // another parser's skeleton is refused
    let mut d = f.extract(&data, &opts(None)).unwrap();
    if let Skeleton::Zip { parser, .. } = &mut d.skeleton {
        parser.as_mut().unwrap().name = "bilingua-other".into();
    }
    assert!(matches!(f.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn sniff_tells_the_four_families_apart() {
    let ods = Pkg::new(odt_content()).mime(MIME_SHEET).build();
    let odp = Pkg::new(odt_content()).mime(MIME_SLIDES).build();
    let odg = Pkg::new(odt_content()).mime(MIME_DRAW).build();
    let tpl = Pkg::new(odt_content()).mime("application/vnd.oasis.opendocument.text-template").build();
    assert!(Ods::default().sniff(&ods) && !Odt::default().sniff(&ods));
    assert!(Odp::default().sniff(&odp) && !Odg::default().sniff(&odp));
    assert!(Odg::default().sniff(&odg));
    assert!(Odt::default().sniff(&tpl));
    // a template and a document extract the same way
    assert_eq!(units("ott", &tpl), units("odt", &Pkg::new(odt_content()).build()));
}

#[test]
fn a_deflated_mimetype_is_written_first_and_stored() {
    // not what the spec asks for, but such files exist; the merge used to refuse them
    let data = odt();
    let mut es: Vec<(bilingua_zip::EntryMeta, Vec<u8>)> = entries(&data).into_iter().map(|e| (e.meta, e.data)).collect();
    es.rotate_left(1); // mimetype last
    for (m, _) in es.iter_mut() {
        if m.name == "mimetype" {
            m.stored = false;
        }
    }
    let deflated = bilingua_zip::write_zip(&es).unwrap();
    assert!(!entries(&deflated).iter().find(|e| e.name == "mimetype").unwrap().meta.stored);
    let out = build("odt", &deflated, &[(0, "Erste Schritte")], Some("de-DE"));
    assert_eq!(&out[..4], b"PK\x03\x04");
    assert_eq!(u16::from_le_bytes([out[8], out[9]]), 0, "stored");
    assert_eq!(u16::from_le_bytes([out[28], out[29]]), 0, "no extra field");
    assert_eq!(&out[30..38], b"mimetype");
    assert_eq!(member(&out, "mimetype"), MIME_TEXT.as_bytes());
    assert_eq!(units("odt", &out)[0].1, "Erste Schritte");
}
