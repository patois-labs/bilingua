//! Wave 3 B-side (11-07): the CAT formats under the LocOS profile, each item with a small hand-written file.
//! The goldens in `tests/locos/{ttx,txml,txlf,xliff,...}` compare whole builds with LocOS; these pin each behaviour.
use bilingua::profile::tokens;
use bilingua::*;

fn opts() -> ExtractOptions {
    profile::locos().extract_options().with_granularity(Granularity::Paragraph).with_target_lang("de-DE")
}

fn units(name: &str, data: &[u8]) -> Vec<(i64, String, String)> {
    let doc = extract(name, data, &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
    profile::locos_units(&doc).into_iter().map(|u| (u.p_index, u.text, u.style)).collect()
}

/// Sets LocOS texts by p_index the way the adapter does (`locos_set_targets`) and merges with the profile.
fn build(name: &str, data: &[u8], texts: &[(i64, &str)]) -> Result<(Vec<u8>, Vec<tokens::TagRepair>), Error> {
    let mut doc = extract(name, data, &opts())?;
    let mut repairs = Vec::new();
    for u in profile::locos_units(&doc) {
        if let Some((_, t)) = texts.iter().find(|(p, _)| *p == u.p_index) {
            let (inl, _, r) = tokens::repair_locos_text_for(&doc.format, t, &u.map);
            repairs.extend(r);
            profile::locos_set_target(&mut doc, &u.segment_id, inl)?;
        }
    }
    Ok((merge_with(&doc, &profile::locos().merge_options(&doc.format))?, repairs))
}

const G1: &str = "\u{27E6}g1\u{27E7}";
const C1: &str = "\u{27E6}/g1\u{27E7}";

const TXML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<txml locale="en_US" version="1.0" targetlocale="de_DE">
<translatable blockId="1">
<segment segmentId="1"><source>Open the <ut type="start" x="1">&lt;b&gt;</ut>box<ut type="end" x="1">&lt;/b&gt;</ut>.</source></segment>
<segment segmentId="2"><source>Done line.</source><target score="100">Fertig.</target></segment>
<segment segmentId="3" modified="true"><source>Kept line.</source><target>Behalten.</target></segment>
<segment segmentId="4"><source>Fuzzy line.</source><target score="70">Unscharf.</target></segment>
</translatable>
<localizable blockId="2"><segment segmentId="1"><source>Menu</source></segment></localizable>
<translatable blockId="3"><segment segmentId="1"><source>Last line.</source></segment></translatable>
</txml>
"#;

/// B-SIDE 1, 2, 3: a confirmed TXML segment is not offered (LocOS `wordfast._skip`), the units keep LocOS's
/// position among every segment (protected ones counted), and they carry no style.
#[test]
fn txml_confirmed_segments_are_protected_and_numbered_like_locos() {
    let want = vec![(0, format!("Open the {G1}box{C1}."), String::new()), (3, "Fuzzy line.".into(), String::new()), (5, "Last line.".into(), String::new())];
    assert_eq!(units("a.txml", TXML.as_bytes()), want);
    // a build never writes the confirmed segments
    let (out, _) = build("a.txml", TXML.as_bytes(), &[(0, "Q"), (3, "R"), (5, "S")]).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("<target score=\"100\">Fertig.</target>") && out.contains("<target>Behalten.</target>"), "{out}");
    assert!(out.contains("<target score=\"70\">R</target>"), "{out}");
    // Bilingua's own reading still lists the confirmed segments as editable, with their translation
    let native = extract("a.txml", TXML.as_bytes(), &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    assert!(native.units.iter().filter(|u| u.translate).count() > 3);
}

const TXLF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2" xmlns:gs4tr="http://www.gs4tr.org/schema/xliff-ext">
  <file original="a.html" source-language="en-US" target-language="de-DE">
    <body>
      <trans-unit id="1"><source>First line.</source></trans-unit>
      <trans-unit id="2"><source>Kept line.</source><target state="translated">Behalten.</target></trans-unit>
      <trans-unit id="3"><source>Third line.</source></trans-unit>
    </body>
  </file>
</xliff>
"#;

/// B-SIDE 1, 2, 3 for TXLF: a translated target is protected (LocOS `cat_xliff._unit_skip`), the slot key numbers
/// the units, no style.
#[test]
fn txlf_translated_units_are_protected_and_keep_their_slot() {
    assert_eq!(units("a.txlf", TXLF.as_bytes()), vec![(0, "First line.".into(), String::new()), (2, "Third line.".into(), String::new())]);
}

fn ttx(enc: &str) -> String {
    format!(
        "<?xml version='1.0' encoding='{enc}'?>\r\n<TRADOStag Version=\"2.0\"><FrontMatter>\r\n<UserSettings DataType=\"HTML\" SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"></UserSettings>\r\n</FrontMatter><Body><Raw>\r\n\
<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">One.</Tuv></Tu>\r\n\
<Tu MatchPercent=\"100\"><Tuv Lang=\"EN-US\">Two.</Tuv><Tuv Lang=\"DE-DE\">Zwei.</Tuv></Tu>\r\n\
<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">Three.</Tuv></Tu>\r\n\
</Raw></Body></TRADOStag>\r\n"
    )
}

fn utf16le(s: &str) -> Vec<u8> {
    let mut b = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        b.extend_from_slice(&u.to_le_bytes());
    }
    b
}

/// B-SIDE 5 and 6: a UTF-16 TTX is written back as UTF-16 with its BOM (LocOS `build_ttx`), and a `<Tu>` with an
/// existing translation keeps its place in the numbering (LocOS key = position among every `<Tu>`).
#[test]
fn ttx_keeps_utf16_and_numbers_every_tu() {
    let data = utf16le(&ttx("utf-16"));
    assert_eq!(units("a.ttx", &data).into_iter().map(|u| u.0).collect::<Vec<_>>(), vec![0, 2]);
    let (out, _) = build("a.ttx", &data, &[(2, "Drei.")]).unwrap();
    assert!(out.starts_with(&[0xFF, 0xFE]), "BOM kept");
    let text = String::from_utf16(&out[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()).unwrap();
    assert!(text.starts_with("<?xml version='1.0' encoding='utf-16'?>") && text.contains("Drei."), "{text}");
    // an untouched build with the target language already set gives the file back
    let (same, _) = build("a.ttx", &data, &[]).unwrap();
    assert_eq!(same, data);
}

const SDLXLIFF: &str = r#"<?xml version="1.0" encoding="utf-8"?><xliff xmlns:sdl="http://sdl.com/FileTypes/SdlXliff/1.0" xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2" sdl:version="1.0">
<file original="a.docx" datatype="x-sdlfilterframework2" source-language="en-US" target-language="de-DE"><body>
<trans-unit id="u1"><source>Click <g id="5">here</g> now.</source><seg-source><mrk mtype="seg" mid="1">Click <g id="5">here</g> now.</mrk></seg-source><target><mrk mtype="seg" mid="1"></mrk></target><sdl:seg-defs><sdl:seg id="1"/></sdl:seg-defs></trans-unit>
</body></file></xliff>"#;

/// B-SIDE 8: a repeated, stray or unclosed pair token (common in MT output) is repaired as LocOS's CAT writers do
/// (`cat_xliff._render`), not refused; each repair is listed. A token for a code the unit lacks is still text.
#[test]
fn repeated_and_unclosed_pair_tokens_are_repaired_like_locos() {
    // a seg-source segment's p_index (LocOS SEG_BASE + n); the strings are LocOS's own builds of the same texts
    const SEG: i64 = 1_000_000_000;
    let written = |out: Vec<u8>, want: &str| {
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(&format!(">{want}</")), "{want}\n{s}");
    };
    let (out, repairs) = build("a.sdlxliff", SDLXLIFF.as_bytes(), &[(SEG, &format!("Bitte {G1}hier{C1} und {G1}da{C1} jetzt."))]).unwrap();
    written(out, "Bitte <g id=\"5\">hier</g> und da jetzt.");
    assert!(matches!(repairs.as_slice(), [tokens::TagRepair::Repeated { .. }]), "{repairs:?}");
    let (out, repairs) = build("a.sdlxliff", SDLXLIFF.as_bytes(), &[(SEG, &format!("{C1}Bitte {G1}hier jetzt."))]).unwrap();
    written(out, "Bitte <g id=\"5\">hier jetzt.</g>");
    assert!(matches!(repairs.as_slice(), [tokens::TagRepair::StrayClose { .. }, tokens::TagRepair::Unclosed { .. }]), "{repairs:?}");
    // a pair written as a point token is written empty; text is never invented
    let (out, repairs) = build("a.sdlxliff", SDLXLIFF.as_bytes(), &[(SEG, "Bitte \u{27E6}x1\u{27E7} jetzt.")]).unwrap();
    written(out, "Bitte <g id=\"5\"></g> jetzt.");
    assert!(matches!(repairs.as_slice(), [tokens::TagRepair::PairAsPoint { .. }]), "{repairs:?}");
    // a tag the source lacks is still refused when given as a tag
    let mut doc = extract("a.sdlxliff", SDLXLIFF.as_bytes(), &opts()).unwrap();
    let seg = doc.units[0].segments[0].id.clone();
    assert!(profile::locos_set_target(&mut doc, &seg, vec![Inline::tag("99", TagKind::Standalone, "<x/>")]).is_err());
    // without the profile the strict rule stands
    let mut native = extract("a.sdlxliff", SDLXLIFF.as_bytes(), &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    let s = native.units[0].segments[0].clone();
    let mut twice = s.source.clone();
    twice.extend(s.source.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned());
    assert!(native.set_target(&s.id, twice).is_err());
}

const XLF_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file original=\"a\" source-language=\"en\" datatype=\"plaintext\"><body><trans-unit id=\"1\"><source>";
const XLF_TAIL: &str = "</source></trans-unit></body></file></xliff>";

/// B-SIDE 10 (#571): nested inline tags in a plain XLIFF are budgeted as LocOS budgets its element copies (ten per
/// allowed segment), refused as a limit with LocOS's sentence.
#[test]
fn nested_inline_tags_are_budgeted_like_locos() {
    let cap = 20usize;
    let o = opts().with_limits(Limits::default().with_max_units(cap));
    let nested = format!("{XLF_HEAD}{}{}{}{XLF_TAIL}", "<g id=\"g\">".repeat(12), "<x id=\"x\"/>".repeat(cap), "</g>".repeat(12));
    match extract("a.xlf", nested.as_bytes(), &o) {
        Err(Error::Limit(e)) => {
            assert_eq!(e.kind, LimitKind::XmlElements);
            assert_eq!(e.msg, format!("This XLIFF file has far more inline tags than MAX_DOCUMENT_SEGMENTS allows ({cap} segments). Split it into smaller files."));
        }
        other => panic!("{other:?}"),
    }
    // the same tags without the nesting fit
    let flat = format!("{XLF_HEAD}{}{XLF_TAIL}", "<x id=\"x\"/>".repeat(cap));
    assert!(extract("a.xlf", flat.as_bytes(), &o).is_ok());
}

/// B-SIDE 13: a character reference XML 1.0 does not allow (`&#30;`) makes the file not well-formed, as lxml says;
/// under the profile (the LocOS port). Bilingua's own reader keeps such a reference as text. Allowed references
/// still read.
#[test]
fn invalid_character_references_are_refused() {
    for bad in ["&#30;", "&#x1E;", "&#0;", "&#xFFFE;"] {
        let f = format!("{XLF_HEAD}Hello {bad} world{XLF_TAIL}");
        assert!(matches!(extract("a.xlf", f.as_bytes(), &opts()), Err(Error::Parse { .. })), "{bad} under the profile");
        // Bilingua's own reader shows malformed inline markup as the text it is: the character never enters the
        // document, so no writer can put it in a file
        let native = extract("a.xlf", f.as_bytes(), &ExtractOptions::new()).unwrap();
        let text = native.units[0].segments[0].source.iter().map(|i| if let Inline::Text(t) = i { t.clone() } else { String::new() }).collect::<String>();
        assert!(!text.chars().any(|c| (c as u32) < 0x20 || c == '\u{FFFE}'), "{bad} native: {text:?}");
        let sdl = SDLXLIFF.replace("Click <g", &format!("Click {bad} <g"));
        assert!(matches!(extract("a.sdlxliff", sdl.as_bytes(), &opts()), Err(Error::Parse { .. })), "{bad} sdlxliff");
    }
    let ok = format!("{XLF_HEAD}Tab&#9;and &#xE9; &#x10000;{XLF_TAIL}");
    assert!(extract("a.xlf", ok.as_bytes(), &opts()).is_ok());
    assert!(extract("a.xlf", ok.as_bytes(), &ExtractOptions::new()).is_ok());
    let e = extract("a.xlf", format!("{XLF_HEAD}a&#30;b{XLF_TAIL}").as_bytes(), &opts()).unwrap_err();
    assert!(e.to_string().contains("xmlParseCharRef: invalid xmlChar value 30"), "{e}");
}

/// B-SIDE 16: JSON nested past its cap is a limit (`LimitKind::Depth`), like every other crate's nesting cap.
#[test]
fn json_nesting_cap_is_a_limit() {
    let deep = "[".repeat(1000) + &"]".repeat(1000);
    for o in [ExtractOptions::new(), opts()] {
        match extract("a.json", deep.as_bytes(), &o) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Depth, "{}", e.msg),
            other => panic!("{other:?}"),
        }
    }
}
