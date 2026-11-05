//! Adversarial checks added at the Opus gate: depth 1000 on a 1 MB stack, hostile targets, tampered paragraph
//! numbers, undeclared languages with byte-exact untouched stories, odd XML.
use bilingua_icml::Icml;
use bilingua_model::*;

fn doc(body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<?aid SnippetType=\"InCopyInterchange\"?>\n<Document DOMVersion=\"8.0\"><Language Name=\"$ID/German: Reformed\"/>{body}</Document>\n"
    )
    .into_bytes()
}

fn opts(t: Option<&str>) -> ExtractOptions {
    ExtractOptions::new().with_granularity(Granularity::Paragraph).with_target_lang(t.map(str::to_string))
}

fn reread(out: &[u8]) -> usize {
    Icml.extract(out, &opts(None)).expect("merge output must re-read").units.len()
}

#[test]
fn deep_wrappers_at_depth_1000_on_a_small_stack() {
    let h = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let n = 990;
            let mut inner = String::from("<Content>deep</Content>");
            for _ in 0..n {
                inner = format!("<XMLElement MarkupTag=\"XMLTag/b\">{inner}</XMLElement>");
            }
            let data = doc(&format!("<Story Self=\"s\"><ParagraphStyleRange><CharacterStyleRange>{inner}</CharacterStyleRange></ParagraphStyleRange></Story>"));
            let lim = Limits::new().with_max_xml_depth(1000);
            let r = Icml.extract_with(&data, &opts(Some("de-DE")), &lim);
            if let Ok(mut d) = r {
                let src = d.units[0].source();
                d.units[0].segments[0].target = src;
                let _ = Icml.merge(&d);
            }
        })
        .unwrap();
    h.join().expect("no stack overflow");
}

#[test]
fn hostile_targets_never_break_the_xml() {
    let data = doc("<!-- <Story Self=\"fake\"> --><Story Self=\"s\"><ParagraphStyleRange><CharacterStyleRange><Content>A </Content></CharacterStyleRange><CharacterStyleRange FontStyle=\"Bold\"><Content>bold</Content><HyperlinkTextSource Self=\"h\"><Content>link</Content></HyperlinkTextSource><Content>\u{27E6}x1\u{27E7} tail<?ACE 7?></Content><Br/></CharacterStyleRange></ParagraphStyleRange><ParagraphStyleRange><CharacterStyleRange><Content>Two</Content></CharacterStyleRange></ParagraphStyleRange></Story>");
    let d0 = Icml.extract(&data, &opts(Some("de-DE"))).unwrap();
    assert_eq!(d0.units.len(), 2);
    assert!(d0.units[0].source().iter().any(|i| matches!(i, Inline::Text(t) if t.contains('\u{27E6}'))));
    let ids: Vec<String> = d0.units[0].codes.keys().filter(|k| !k.starts_with('~')).cloned().collect();
    let variants: Vec<Vec<Inline>> = vec![
        vec![Inline::text("]]> <&> \r\n \u{0}\u{FFFE} \u{1F600}")],
        vec![Inline::tag("99", TagKind::Open, ""), Inline::text("x"), Inline::tag("1", TagKind::Close, "")],
        ids.iter().rev().flat_map(|i| vec![Inline::tag(i.clone(), TagKind::Close, ""), Inline::tag(i.clone(), TagKind::Open, "")]).collect(),
        ids.iter().flat_map(|i| vec![Inline::tag(i.clone(), TagKind::Standalone, ""), Inline::tag(i.clone(), TagKind::Open, ""), Inline::tag(i.clone(), TagKind::Open, "")]).collect(),
        vec![Inline::tag("01", TagKind::Open, ""), Inline::tag("~p", TagKind::Standalone, ""), Inline::text(" ")],
        vec![Inline::tag("2", TagKind::Close, "")],
    ];
    for v in variants {
        let mut d = d0.clone();
        let mut v = v;
        v.push(Inline::text("t"));
        d.units[0].segments[0].target = v.clone();
        d.units[1].segments[0].target = vec![Inline::text("Zwei")];
        let out = Icml.merge(&d).unwrap_or_else(|e| panic!("merge failed for {v:?}: {e}"));
        assert_eq!(reread(&out), 2, "{v:?}");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("<!-- <Story Self=\"fake\"> -->"));
        assert!(s.contains("Self=\"h\""), "hyperlink source kept for {v:?}");
        assert!(s.contains("<?ACE 7?>"), "special char kept for {v:?}");
        assert!(s.contains("AppliedLanguage=\"$ID/German: Reformed\""));
    }
}

#[test]
fn tampered_paragraph_index_is_a_typed_error() {
    let data = doc("<Story Self=\"s\"><ParagraphStyleRange><CharacterStyleRange><Content>One</Content></CharacterStyleRange></ParagraphStyleRange></Story>");
    let mut d = Icml.extract(&data, &opts(None)).unwrap();
    d.units[0].codes.insert("~p".into(), CodeData { standalone: Some("18446744073709551615".into()), ..Default::default() });
    d.units[0].segments[0].target = vec![Inline::text("x")];
    assert!(Icml.merge(&d).is_err());
    d.units[0].codes.insert("~p".into(), CodeData { standalone: Some("-1".into()), ..Default::default() });
    let out = Icml.merge(&d).unwrap();
    assert_eq!(out, data);
}

#[test]
fn undeclared_language_is_not_stamped_and_untouched_story_is_byte_exact() {
    let data = String::from(
        "<?xml version='1.0' encoding='utf-8'?>\r\n<ea:Stories xmlns:ea='urn:SmartConnection_v3'><ea:Story><Document><Story Self='a'><ParagraphStyleRange><CharacterStyleRange><Content>Keep &amp; me</Content><Br /></CharacterStyleRange></ParagraphStyleRange></Story></Document></ea:Story><ea:Story><Document><Story Self='b'><ParagraphStyleRange><CharacterStyleRange><Content>Change</Content></CharacterStyleRange></ParagraphStyleRange></Story></Document></ea:Story></ea:Stories>"
    )
    .into_bytes();
    let mut d = Icml.extract(&data, &opts(Some("ja-JP"))).unwrap();
    assert_eq!(d.units.len(), 2);
    d.units[1].segments[0].target = vec![Inline::text("変更")];
    let out = String::from_utf8(Icml.merge(&d).unwrap()).unwrap();
    assert!(!out.contains("AppliedLanguage"));
    assert!(out.contains("<Story Self='a'><ParagraphStyleRange><CharacterStyleRange><Content>Keep &amp; me</Content><Br /></CharacterStyleRange></ParagraphStyleRange></Story>"));
    assert!(out.contains("変更"));
}

#[test]
fn odd_xml_is_refused() {
    for bad in [
        "<?xml version=\"1.0\"?><Document><Story><x:a/></Story></Document>",
        "<Document xmlns:x=\"\"><Story/></Document>",
        "<Document><Story Self=\"a\" Self=\"b\"/></Document>",
        "<Document><!-- <!DOCTYPE x> --><Story/></Document>",
        "<Document><Story>&ent;</Story></Document>",
        "<Document><Story>&#0;</Story></Document>",
        "<Document><Story></Document>",
        "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><Document><Story/></Document>",
        "\u{FEFF}\u{FEFF}<Document><Story/></Document>",
    ] {
        let r = Icml.extract(bad.as_bytes(), &opts(None));
        assert!(r.is_err(), "accepted: {bad}");
    }
}
