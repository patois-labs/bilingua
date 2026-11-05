//! The LocOS suite `tests/verify_idml_filter.py`, ported case by case (hand-built packages, no InDesign needed).
//! The three LocOS cases about web, API, CLI, MCP and registration surfaces test LocOS itself and have no
//! counterpart here; every other case is below, with its assertions.

mod common;
use bilingua_idml::xml::{Id, Tree};
use bilingua_idml::Idml;
use bilingua_model::*;
use common::*;

fn tags_in(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = s;
    while let Some(p) = rest.find('\u{27E6}') {
        let after = &rest[p + 3..];
        let e = after.find('\u{27E7}').unwrap();
        out.push(format!("\u{27E6}{}\u{27E7}", &after[..e]));
        rest = &after[e + 3..];
    }
    out
}

#[test]
fn multi_story_order_and_byte_identity() {
    let data = Pkg::new(&[("a", psr(&csr("<Content>First story.</Content>"))), ("b", psr(&csr("<Content>Second story.</Content>")))])
        .order(&["b", "a"])
        .build();
    let mut doc = extract(&data);
    assert_eq!(texts(&doc), ["Second story.", "First story."]);
    let id = doc.units[0].id.clone();
    translate(&mut doc, &[(&id, "Deuxieme histoire.".into())]);
    let out = merge(&doc);
    let src = members_of(&data);
    let dst = members_of(&out);
    assert_eq!(src.iter().map(|e| &e.name).collect::<Vec<_>>(), dst.iter().map(|e| &e.name).collect::<Vec<_>>());
    assert_eq!(dst[0].name, "mimetype");
    assert!(dst[0].meta.stored);
    assert_eq!(dst[0].data, MIMETYPE.as_bytes());
    for (a, b) in src.iter().zip(&dst) {
        assert_eq!(a.meta.modified, b.meta.modified, "{}", a.name);
        assert_eq!(a.meta.stored, b.meta.stored, "{}", a.name);
        if a.name != "Stories/Story_b.xml" {
            assert_eq!(a.data, b.data, "{} must be byte-identical", a.name);
        }
    }
    assert!(visible_text(&story_tree(&out, "b")).contains("Deuxieme histoire."));
    // an untranslated merge gives back the original file, byte for byte
    assert_eq!(merge(&extract(&data)), data);
}

#[test]
fn nested_styles_one_segment_and_growth() {
    let body = psr(&format!("{}\n{}\n{}", csr("<Content>Hello </Content>"), csr_b("<Content>bold</Content>", BOLD), csr("<Content> world</Content>")));
    let data = package(body);
    let mut doc = extract(&data);
    assert_eq!(doc.units.len(), 1);
    assert_eq!(tok(&doc.units[0]), "Hello \u{27e6}r1\u{27e7}bold\u{27e6}/r1\u{27e7} world");
    let new = "\u{27e6}r1\u{27e7}Fettgedruckt\u{27e6}/r1\u{27e7} ist hier, hallo liebe Welt";
    translate(&mut doc, &[("p0", new.into())]);
    let out = merge(&doc);
    let t = story_tree(&out, "a");
    let got: Vec<(String, String)> = named(&t, "CharacterStyleRange")
        .into_iter()
        .map(|r| (t.attr(r, "AppliedCharacterStyle").unwrap().to_string(), t.itertext(r).trim().to_string()))
        .collect();
    assert_eq!(got, [(BOLD.to_string(), "Fettgedruckt".to_string()), (NOSTYLE.to_string(), "ist hier, hallo liebe Welt".to_string())]);
    assert_eq!(units_of(&out)[0], new);
}

#[test]
fn paragraph_breaks_inside_one_range() {
    let body = psr(&csr("<Content>Para one.</Content><Br/><Content>Para two.</Content><Br/><Content>Para three.</Content>"));
    let data = package(body);
    let mut doc = extract(&data);
    assert_eq!(texts(&doc), ["Para one.", "Para two.", "Para three."]);
    let id = doc.units[1].id.clone();
    translate(&mut doc, &[(&id, "Absatz zwei, deutlich laenger als vorher.".into())]);
    let out = merge(&doc);
    assert_eq!(named(&story_tree(&out, "a"), "Br").len(), 2);
    assert_eq!(units_of(&out), ["Para one.", "Absatz zwei, deutlich laenger als vorher.", "Para three."]);
}

#[test]
fn tabs_special_characters_and_pis() {
    let data = package(psr(&csr("<Content>Page\t<?ACE 18?> of the\u{2028}book</Content>")));
    let mut doc = extract(&data);
    assert_eq!(tok(&doc.units[0]), "Page\u{27e6}x1\u{27e7}\u{27e6}x2\u{27e7} of the\u{27e6}x3\u{27e7}book");
    translate(&mut doc, &[("p0", "Seite\u{27e6}x1\u{27e7}\u{27e6}x2\u{27e7} des\u{27e6}x3\u{27e7}Buches".into())]);
    let raw = raw_story(&merge(&doc), "a");
    assert!(raw.contains("<Content>Seite\t<?ACE 18?> des\u{2028}Buches</Content>"), "{raw}");
}

#[test]
fn footnote_table_hyperlink_index_xml() {
    let footnote = format!("<Footnote>{}</Footnote>", psr(&csr("<Content><?ACE 4?> Footnote text.</Content>")));
    let table = format!(
        "<Table Self=\"t1\" BodyRowCount=\"1\" ColumnCount=\"2\"><Row Self=\"t1r0\" Name=\"0\"/><Column Self=\"t1c0\" Name=\"0\"/><Column Self=\"t1c1\" Name=\"1\"/><Cell Self=\"t1i0\" Name=\"0:0\">{}</Cell><Cell Self=\"t1i1\" Name=\"1:0\">{}</Cell></Table>",
        psr(&csr("<Content>Cell A</Content>")),
        psr(&csr("<Content>Cell B</Content>"))
    );
    let mut body = psr(&csr(&format!(
        "<Content>See </Content><HyperlinkTextSource Self=\"h1\" Name=\"link\"><Content>our site</Content></HyperlinkTextSource><Content> now</Content><PageReference Self=\"p1\" PageReferenceType=\"CurrentPage\"/><Content>.</Content>{footnote}"
    )));
    body += "\n";
    body += &psr(&csr("<XMLElement Self=\"x1\" MarkupTag=\"XMLTag/name\"><Content>Ada</Content></XMLElement><Content> wrote it.</Content>"));
    body += "\n";
    body += &psr(&csr(&table));
    let data = package(body);
    let mut doc = extract(&data);
    assert_eq!(
        texts(&doc),
        [
            "See \u{27e6}g1\u{27e7}our site\u{27e6}/g1\u{27e7} now\u{27e6}x2\u{27e7}.\u{27e6}x3\u{27e7}",
            "\u{27e6}g1\u{27e7}Ada\u{27e6}/g1\u{27e7} wrote it.",
            "\u{27e6}x1\u{27e7} Footnote text.",
            "Cell A",
            "Cell B"
        ]
    );
    let ids: Vec<String> = doc.units.iter().map(|u| u.id.clone()).collect();
    let mapping = [
        "Besuchen Sie \u{27e6}g1\u{27e7}unsere Seite\u{27e6}/g1\u{27e7} jetzt\u{27e6}x2\u{27e7}.\u{27e6}x3\u{27e7}",
        "\u{27e6}g1\u{27e7}Ada\u{27e6}/g1\u{27e7} schrieb es.",
        "\u{27e6}x1\u{27e7} Fussnotentext.",
        "Zelle A",
        "Zelle B",
    ];
    let pairs: Vec<(&str, String)> = ids.iter().zip(mapping).map(|(i, m)| (i.as_str(), m.to_string())).collect();
    translate(&mut doc, &pairs);
    let out = merge(&doc);
    let t = story_tree(&out, "a");
    assert_eq!(named_attr(&t, "HyperlinkTextSource", "Self", "h1").len(), 1);
    assert_eq!(t.itertext(named(&t, "HyperlinkTextSource")[0]), "unsere Seite");
    assert_eq!(named(&t, "PageReference").len(), 1);
    assert_eq!(named(&t, "Footnote").len(), 1);
    let cells: Vec<Id> = named(&t, "Table").iter().flat_map(|tb| t.children(*tb).iter().copied().filter(|c| t.local(*c) == "Cell")).collect();
    assert_eq!(cells.len(), 2);
    assert_eq!(t.attr(named(&t, "Table")[0], "Self"), Some("t1"));
    assert_eq!(units_of(&out), mapping);
}

#[test]
fn hidden_conditions_notes_and_hidden_text() {
    let conds = "\t<Condition Self=\"Condition/Hide\" Name=\"Hide\" Visible=\"false\"/>\n\t<Condition Self=\"Condition/Show\" Name=\"Show\" Visible=\"true\"/>\n";
    let body = psr(&[
        csr("<Content>Visible </Content>"),
        csr_x("<Content>SECRET</Content>", NOSTYLE, " AppliedConditions=\"Condition/Hide\""),
        csr_x("<Content> shown</Content>", NOSTYLE, " AppliedConditions=\"Condition/Show\""),
        csr("<Content> tail</Content><Note><ParagraphStyleRange><CharacterStyleRange><Content>editor note</Content></CharacterStyleRange></ParagraphStyleRange></Note><HiddenText><Content>hidden words</Content></HiddenText>"),
    ]
    .concat());
    let data = Pkg::one(body).conditions(conds).build();
    let mut doc = extract(&data);
    assert_eq!(doc.units.len(), 1);
    let text = tok(&doc.units[0]);
    assert!(!text.contains("SECRET") && !text.contains("editor note") && !text.contains("hidden words"), "{text}");
    let n = tags_in(&text).len();
    translate(&mut doc, &[("p0", text.replace("Visible", "Sichtbar"))]);
    let out = merge(&doc);
    let t = story_tree(&out, "a");
    let vis = visible_text(&t);
    assert!(vis.contains("SECRET") && vis.contains("editor note") && vis.contains("hidden words") && vis.contains("Sichtbar"), "{vis}");
    assert_eq!(named_attr(&t, "CharacterStyleRange", "AppliedConditions", "Condition/Hide").len(), 1);
    assert_eq!(tags_in(&units_of(&out)[0]).len(), n);
}

#[test]
fn nonprinting_story_skipped() {
    let layers = "\t<Layer Self=\"L2\" Name=\"Notes\" Visible=\"true\" Printable=\"false\"/>\n";
    let frames = "<TextFrame Self=\"f1\" ParentStory=\"a\"/><TextFrame Self=\"f2\" ParentStory=\"b\" ItemLayer=\"L2\"/><TextFrame Self=\"f3\" ParentStory=\"c\" Nonprinting=\"true\"/>";
    let data = Pkg::new(&[
        ("a", psr(&csr("<Content>Printed.</Content>"))),
        ("b", psr(&csr("<Content>Layer note.</Content>"))),
        ("c", psr(&csr("<Content>Slug.</Content>"))),
    ])
    .layers(layers)
    .frames(frames)
    .build();
    assert_eq!(units_of(&data), ["Printed."]);
}

#[test]
fn cjk_and_rtl() {
    let data = Pkg::new(&[
        ("a", psr(&csr("<Content>\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6587}\u{7ae0}\u{3002}</Content>"))),
        ("b", psr(&csr("<Content>\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}</Content>"))),
    ])
    .build();
    let mut doc = extract(&data);
    assert_eq!(texts(&doc), ["\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{6587}\u{7ae0}\u{3002}", "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}"]);
    translate(&mut doc, &[("p0", "\u{4e2d}\u{6587}\u{53e5}\u{5b50}\u{3002}".into()), ("p1", "\u{05e9}\u{05dc}\u{05d5}\u{05dd} \u{05e2}\u{05d5}\u{05dc}\u{05dd}".into())]);
    let out = merge(&doc);
    assert_eq!(units_of(&out), ["\u{4e2d}\u{6587}\u{53e5}\u{5b50}\u{3002}", "\u{05e9}\u{05dc}\u{05d5}\u{05dd} \u{05e2}\u{05d5}\u{05dc}\u{05dd}"]);
}

#[test]
fn damaged_tokens_never_lose_elements() {
    let body = psr(&[
        csr("<Content>A </Content><HyperlinkTextSource Self=\"h9\"><Content>link</Content></HyperlinkTextSource><Content> and</Content><PageReference Self=\"p9\"/><Content> more</Content>"),
        csr_b("<Content> bold</Content>", BOLD),
    ]
    .concat());
    let data = package(body);
    let doc0 = extract(&data);
    for bad in [
        "no tokens at all",
        "\u{27e6}x99\u{27e7} junk \u{27e6}/g7\u{27e7} \u{27e6}g1\u{27e7}a\u{27e6}/g1\u{27e7}\u{27e6}g1\u{27e7}b\u{27e6}/g1\u{27e7}",
    ] {
        let mut doc = doc0.clone();
        translate(&mut doc, &[("p0", bad.into())]);
        let out = merge(&doc);
        let t = story_tree(&out, "a");
        assert_eq!(named_attr(&t, "HyperlinkTextSource", "Self", "h9").len(), 1, "{bad}");
        assert_eq!(named_attr(&t, "PageReference", "Self", "p9").len(), 1, "{bad}");
        assert!(!visible_text(&t).contains('\u{27e6}'), "{bad}");
    }
}

fn ranges_ok(t: &Tree) {
    let selfs: Vec<&str> = t.iter(t.root()).into_iter().filter_map(|i| t.attr(i, "Self")).collect();
    let set: std::collections::HashSet<&&str> = selfs.iter().collect();
    assert_eq!(selfs.len(), set.len(), "{selfs:?}");
    for el in named(t, "CharacterStyleRange") {
        assert_ne!(t.local(t.parent(el).unwrap()), "CharacterStyleRange", "range directly inside a range");
    }
    for c in named(t, "Content") {
        let mut up = t.parent(c);
        let mut ok = false;
        while let Some(u) = up {
            if t.local(u) == "CharacterStyleRange" {
                ok = true;
            }
            up = t.parent(u);
        }
        assert!(ok, "bare Content");
    }
}

#[test]
fn hyperlink_spanning_ranges() {
    let block = format!(
        "\t\t<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/$ID/NormalParagraphStyle\">{}<HyperlinkTextSource Self=\"hb\" Name=\"Hyperlink 1\" Hidden=\"false\">{}{}</HyperlinkTextSource></ParagraphStyleRange>",
        csr_x("", NOSTYLE, " KerningMethod=\"$ID/Optical\""),
        csr("<Content>read the </Content>"),
        csr_b("<Content>manual</Content>", BOLD)
    );
    let inline = psr(&format!(
        "{}{}",
        csr("<Content>Go to </Content>"),
        csr_b(&format!("<HyperlinkTextSource Self=\"hi\" Name=\"Hyperlink 2\">{}</HyperlinkTextSource><Content> today</Content>", csr_b("<Content>our site</Content>", BOLD).trim()), "CharacterStyle/Hyperlink")
    ));
    let data = package(format!("{block}\n{inline}"));
    let doc0 = extract(&data);
    assert_eq!(doc0.units.len(), 2);
    let t0 = texts(&doc0);
    assert!(t0[0].contains("manual") && t0[1].contains("our site"), "{t0:?}");
    let mut doc = doc0.clone();
    let mapping: Vec<(String, String)> = doc.units.iter().map(|u| (u.id.clone(), tok(u).replace("manual", "Handbuch").replace("our site", "unsere Seite"))).collect();
    let pairs: Vec<(&str, String)> = mapping.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
    translate(&mut doc, &pairs);
    let out = merge(&doc);
    let t = story_tree(&out, "a");
    ranges_ok(&t);
    let hb = named_attr(&t, "HyperlinkTextSource", "Self", "hb")[0];
    let hi = named_attr(&t, "HyperlinkTextSource", "Self", "hi")[0];
    assert!(t.itertext(hb).contains("Handbuch"));
    assert_eq!(t.itertext(hi), "unsere Seite");
    let inner = t.children(hi).iter().find(|c| t.local(**c) == "CharacterStyleRange").unwrap();
    assert_eq!(t.attr(*inner, "AppliedCharacterStyle"), Some(BOLD));
    assert_eq!(units_of(&out), pairs.iter().map(|p| p.1.clone()).collect::<Vec<_>>());
    // Misplaced, dropped and repeated tokens: the link survives once, no range in a range.
    let toks = tags_in(&t0[1]);
    let rev: Vec<String> = toks.iter().rev().cloned().collect();
    for bad in ["unsere Seite".to_string(), format!("{} x", rev.join(" ")), format!("{} a {}", toks.last().unwrap(), toks[0])] {
        let mut d = doc0.clone();
        translate(&mut d, &[("p1", bad.clone())]);
        let t = story_tree(&merge(&d), "a");
        ranges_ok(&t);
        assert_eq!(named_attr(&t, "HyperlinkTextSource", "Self", "hi").len(), 1, "{bad}");
    }
}

#[test]
fn real_export_shapes() {
    let conds = "\t<Condition Self=\"Condition/Print Only\" Name=\"Print Only\" Visible=\"false\"/>\n";
    let hidden = psr(&[csr("<Content>Shown </Content>"), csr_x("<Content>PRINTONLY</Content>", NOSTYLE, " AppliedConditions=\"Condition/Print%20Only\"")].concat());
    assert_eq!(units_of(&Pkg::one(hidden).conditions(conds).build()), ["Shown"]);

    let bare = format!(
        "\t\t<Content>Direct one.</Content>\n{}\n{}\n\t\t<Content>Four.</Content>",
        csr("<Content> Two.</Content>"),
        psr(&csr("<Content>Three.</Content>"))
    );
    let data = package(bare);
    let mut doc = extract(&data);
    let joined = texts(&doc).join(" ");
    assert!(joined.contains("Direct one.") && joined.contains("Four."), "{joined}");
    let pairs: Vec<(String, String)> = doc.units.iter().map(|u| (u.id.clone(), tok(u).to_uppercase())).collect();
    let p: Vec<(&str, String)> = pairs.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
    translate(&mut doc, &p);
    let t = story_tree(&merge(&doc), "a");
    ranges_ok(&t);
    let v = visible_text(&t);
    assert!(v.contains("DIRECT ONE.") && v.contains("FOUR."), "{v}");

    let notes = psr(&csr(&format!(
        "<Content>Body</Content><EndnoteRange Self=\"e1\">{}</EndnoteRange><Change ChangeType=\"InsertedText\">{}</Change>",
        psr(&csr("<Content>Endnote words.</Content>")),
        psr(&csr("<Content>Inserted paragraph.</Content>"))
    )));
    let ts = units_of(&package(notes));
    assert!(ts.iter().any(|t| t == "Endnote words.") && ts.iter().any(|t| t == "Inserted paragraph."), "{ts:?}");

    let keep = psr(&[csr("<Content xml:space=\"preserve\">Kept </Content>"), csr_b("<Content>bold</Content>", BOLD)].concat());
    let data = package(keep);
    let mut doc = extract(&data);
    let new = tok(&doc.units[0]).replace("Kept", "Behalten\nzwei");
    translate(&mut doc, &[("p0", new)]);
    let raw = raw_story(&merge(&doc), "a");
    assert!(raw.contains("xml:space=\"preserve\"") && raw.contains("Behalten\u{2028}zwei") || raw.contains("Behalten zwei"), "{raw}");
    assert!(raw.contains("xml:space=\"preserve\""), "{raw}");
}

#[test]
fn target_language_stamped() {
    let langs = "\t<Language Self=\"Language/$ID/English%3a USA\" Name=\"$ID/English: USA\" Id=\"269\"/>\n\
\t<Language Self=\"Language/$ID/de_DE_2006\" Name=\"$ID/de_DE_2006\" Id=\"275\"/>\n\
\t<Language Self=\"Language/$ID/hi_IN\" Name=\"$ID/hi_IN\" Id=\"348\"/>\n";
    let body = format!(
        "{}\n{}",
        psr(&format!(
            "{}{}",
            csr_x("<Content>Hello </Content>", NOSTYLE, " AppliedLanguage=\"$ID/English: USA\""),
            csr_b(&format!("<Content>world</Content><Footnote>{}</Footnote>", psr(&csr_x("<Content>Fn.</Content>", NOSTYLE, " AppliedLanguage=\"$ID/English: USA\""))), BOLD)
        )),
        psr(&csr("<Content>Untouched.</Content>"))
    );
    let data = Pkg::one(body).conditions(langs).build();
    let src = extract(&data);
    let mut base = src.clone();
    let first = tok(&src.units[0]).replace("Hello", "Hallo");
    translate(&mut base, &[("p0", first)]);
    assert!(!raw_story(&merge(&base), "a").contains("de_DE_2006"));
    for (locale, want) in [("de-DE", Some("$ID/de_DE_2006")), ("hi-IN", Some("$ID/hi_IN")), ("fr-FR", None)] {
        let mut d = base.clone();
        d.target_lang = Some(locale.into());
        let t = story_tree(&merge(&d), "a");
        let story = t.children(t.root()).iter().copied().find(|c| t.local(*c) == "Story").unwrap();
        let paras: Vec<Id> = t.children(story).iter().copied().filter(|c| t.local(*c) == "ParagraphStyleRange").collect();
        let got: std::collections::BTreeSet<Option<String>> = t
            .children(paras[0])
            .iter()
            .filter(|c| t.local(**c) == "CharacterStyleRange")
            .map(|r| t.attr(*r, "AppliedLanguage").map(String::from))
            .collect();
        let fn_range = named(&t, "Footnote").into_iter().flat_map(|f| t.iter(f)).find(|i| t.local(*i) == "CharacterStyleRange").unwrap();
        assert_eq!(t.attr(fn_range, "AppliedLanguage"), Some("$ID/English: USA"));
        match want {
            Some(w) => assert_eq!(got, [Some(w.to_string())].into_iter().collect(), "{locale}"),
            None => assert!(!got.contains(&Some("$ID/French".to_string())), "{locale}"),
        }
        let r2 = t.children(paras[1]).iter().find(|c| t.local(**c) == "CharacterStyleRange").unwrap();
        assert_eq!(t.attr(*r2, "AppliedLanguage"), None);
    }
}

/// A zip with stored entries written by hand, so tests can build what a safe writer refuses (duplicate names,
/// backslashes).
fn raw_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = !0u32;
        for b in data {
            c ^= *b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
        }
        !c
    }
    let mut out = vec![];
    let mut dir = vec![];
    for (name, data) in members {
        let off = out.len() as u32;
        let (c, n) = (crc(data), name.len() as u16);
        let mut h = vec![];
        h.extend_from_slice(&[0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        h.extend_from_slice(&c.to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&(data.len() as u32).to_le_bytes());
        h.extend_from_slice(&n.to_le_bytes());
        h.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&h);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        let mut d = vec![];
        d.extend_from_slice(&[0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        d.extend_from_slice(&c.to_le_bytes());
        d.extend_from_slice(&(data.len() as u32).to_le_bytes());
        d.extend_from_slice(&(data.len() as u32).to_le_bytes());
        d.extend_from_slice(&n.to_le_bytes());
        d.extend_from_slice(&[0; 12]);
        d.extend_from_slice(&off.to_le_bytes());
        d.extend_from_slice(name.as_bytes());
        dir.extend_from_slice(&d);
    }
    let start = out.len() as u32;
    out.extend_from_slice(&dir);
    out.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

type Member = (String, Vec<u8>);

fn refused(data: &[u8], why: &str) -> Error {
    match Idml.extract(data, &ExtractOptions::default()) {
        Err(e) => e,
        Ok(_) => panic!("{why}: extract must refuse"),
    }
}

#[test]
fn malformed_and_hostile() {
    refused(b"not a zip at all", "not a zip");
    let good = Pkg::one(psr(&csr("<Content>x</Content>")));
    let rezip = |f: &dyn Fn(&str, Vec<u8>) -> Option<Member>| -> Vec<u8> {
        let m: Vec<(String, Vec<u8>)> = good.members().into_iter().filter_map(|(n, d)| f(&n, d)).collect();
        zip_of(&m)
    };
    refused(&rezip(&|n, d| if n == "designmap.xml" { None } else { Some((n.into(), d)) }), "no designmap");
    refused(&rezip(&|n, d| Some((n.into(), if n == "mimetype" { b"application/zip".to_vec() } else { d }))), "wrong mimetype");
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><idPkg:Story xmlns:idPkg=\"x\"><Story><ParagraphStyleRange><CharacterStyleRange><Content>&e;</Content></CharacterStyleRange></ParagraphStyleRange></Story></idPkg:Story>".to_vec();
    refused(&rezip(&|n, d| Some((n.into(), if n.starts_with("Stories/") { xxe.clone() } else { d }))), "XXE story");
    refused(&rezip(&|n, d| Some((n.into(), if n.starts_with("Stories/") { b"<Story><unclosed>".to_vec() } else { d }))), "malformed XML");
    refused(&rezip(&|n, d| Some((if n.starts_with("Resources/") { "../../evil.xml".into() } else { n.into() }, d))), "path traversal");
    refused(&rezip(&|n, d| Some((if n.starts_with("Resources/") { "C:/evil.xml".into() } else { n.into() }, d))), "drive path");
    // duplicate member names (zip smuggling)
    let dup = raw_zip(&[("mimetype", MIMETYPE.as_bytes()), ("designmap.xml", &designmap(&["a"], "", "")), ("Stories/Story_a.xml", b"<x/>"), ("Stories/Story_a.xml", b"<x/>")]);
    refused(&dup, "duplicate member");
    // compression bomb: one part inflates far past the ratio limit
    let mut m = good.members();
    m.push(("Resources/Bomb.xml".into(), vec![0u8; 64 * 1024 * 1024]));
    let e = refused(&zip_of(&m), "bomb");
    assert!(matches!(e, Error::Limit(_)), "{e}");
}

#[test]
fn token_shuffle_fuzz() {
    let body = psr(&[
        csr("<Content>Start\t</Content><HyperlinkTextSource Self=\"hz\"><Content>link</Content></HyperlinkTextSource><PageReference Self=\"pz\"/><Content>mid<?ACE 7?>dle</Content>"),
        csr_b("<Content> bold </Content>", BOLD),
        csr_b("<Content>italic</Content>", "CharacterStyle/Italic"),
        csr("<Content> end.</Content><Br/><Content>Next para.</Content>"),
    ]
    .concat());
    let data = package(body);
    let doc0 = extract(&data);
    let toks = tags_in(&tok(&doc0.units[0]));
    let mut state = 1010u64;
    let mut rnd = move |n: usize| {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % n.max(1)
    };
    for _ in 0..200 {
        let mut picked: Vec<String> = toks.iter().filter(|_| rnd(10) >= 2).cloned().collect();
        for _ in 0..rnd(3) {
            picked.push(toks[rnd(toks.len())].clone());
        }
        for i in (1..picked.len()).rev() {
            picked.swap(i, rnd(i + 1));
        }
        let words = ["Wort", "zwei", " ", "drei"];
        let text: String = picked.iter().map(|t| format!("{t}{}", words[rnd(4)])).collect();
        let mut doc = doc0.clone();
        translate(&mut doc, &[("p0", text.clone())]);
        let out = merge(&doc);
        let t = story_tree(&out, "a");
        assert_eq!(named_attr(&t, "HyperlinkTextSource", "Self", "hz").len(), 1, "{text}");
        assert_eq!(named_attr(&t, "PageReference", "Self", "pz").len(), 1, "{text}");
        assert_eq!(named(&t, "Br").len(), 1, "{text}");
        assert!(!visible_text(&t).contains('\u{27e6}'), "{text}");
        assert_eq!(units_of(&out).last().unwrap(), "Next para.", "{text}");
    }
}
