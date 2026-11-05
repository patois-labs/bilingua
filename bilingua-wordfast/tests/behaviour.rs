//! Behaviour beyond the LocOS suite: loaded targets, seg-source segments, inline edge cases, escaping and the
//! Okapi `okf_txml` test resources.

mod common;

use bilingua_model::*;
use common::*;

const SAMPLE_TXML: &[u8] = include_bytes!("data/locos_sample.txml");
const SAMPLE_TXLF: &[u8] = include_bytes!("data/locos_sample.txlf");
const SEGSRC: &[u8] = include_bytes!("data/edge_segsource.txlf");
const INLINE: &[u8] = include_bytes!("data/edge_inline.txml");

fn ids(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| u.id.clone()).collect()
}

fn unit<'a>(d: &'a Document, id: &str) -> &'a Unit {
    d.units.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}"))
}

fn flat_target(u: &Unit) -> String {
    locos::to_locos_text(&u.output()).0
}

#[test]
fn txml_existing_translations_load_as_targets() {
    let d = extract(&TXML_T, SAMPLE_TXML);
    assert_eq!(ids(&d), ["0", "1", "4", "5", "6", "7", "8"]);
    // a fuzzy match: loaded as a draft with its score
    let u = unit(&d, "1");
    assert_eq!(flat_target(u), "Druecken Sie Speichern.");
    assert_eq!(u.segments[0].state, SegState::Draft);
    assert_eq!(u.segments[0].origin, Some(Origin::new(OriginKind::Tm).with_match(85)));
    assert!(u.translate);
    // raw machine translation
    assert_eq!(unit(&d, "5").segments[0].origin.as_ref().map(|o| o.kind), Some(OriginKind::Mt));
    assert_eq!(unit(&d, "5").segments[0].state, SegState::Draft);
    // confirmed work: read-only, finished
    for (id, text) in [("4", "Bestaetigte Zeile."), ("8", "Vorbefuellte Zeile.")] {
        let u = unit(&d, id);
        assert_eq!(flat_target(u), text);
        assert!(!u.translate);
        assert_eq!(u.lock_reason.as_deref(), Some("confirmed"));
        assert_eq!(u.segments[0].state, SegState::Translated);
    }
    // not translated yet
    assert!(unit(&d, "6").segments[0].target.is_empty());
    assert_eq!(unit(&d, "6").segments[0].state, SegState::New);
    // the LocOS list is the same without the confirmed ones
    let l = extract(&TXML, SAMPLE_TXML);
    assert_eq!(ids(&l), ["0", "1", "5", "6", "7"]);
    // include_locked lists everything, protected ones read-only with their targets
    let all = TXML.extract(SAMPLE_TXML, &locked_opts()).unwrap();
    assert_eq!(ids(&all), ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    assert_eq!(unit(&all, "3").lock_reason.as_deref(), Some("locked"));
    assert_eq!(unit(&all, "3").segments[0].state, SegState::Locked);
    assert_eq!(flat_target(unit(&all, "3")), "Gesperrte Zeile.");
    assert_eq!(unit(&all, "9").lock_reason.as_deref(), Some("localizable"));
    // notes
    assert_eq!(unit(&d, "7").notes[0].text, "Keep it short & plain");
    assert_eq!(unit(&d, "7").notes[0].author.as_deref(), Some("pm"));
    // languages
    assert_eq!(d.source_lang.as_deref(), Some("en_US"));
    assert_eq!(d.target_lang.as_deref(), Some("de_DE"));
}

#[test]
fn a_loaded_target_that_is_not_touched_is_not_rewritten() {
    // the fuzzy and machine targets keep their bytes unless the translator changes them
    let mut d = extract(&TXML_T, SAMPLE_TXML);
    assert_eq!(merged(&TXML_T, &d), SAMPLE_TXML);
    set(&mut d, 1, "Druecken Sie \u{27e6}x1\u{27e7}Speichern.");
    let out = merged(&TXML_T, &d);
    assert_eq!(seg(&out, "2"), b"<segment segmentId=\"2\" gtmt=\"false\">\n<source>Press <ut x=\"2\">&lt;br/&gt;</ut>Save &amp; close.</source>\n<target score=\"85\">Druecken Sie <ut x=\"2\">&lt;br/&gt;</ut>Speichern.</target>\n</segment>");
    // re-reading gives the new text
    let back = extract(&TXML_T, &out);
    assert_eq!(flat_target(unit(&back, "1")), "Druecken Sie \u{27e6}x1\u{27e7}Speichern.");
}

#[test]
fn txlf_existing_translations_load_as_targets() {
    let d = extract(&TXLF_T, SAMPLE_TXLF);
    assert_eq!(ids(&d), ["0", "1", "3", "4", "5"]);
    let u = unit(&d, "1");
    assert_eq!(flat_target(u), "Dann klicken.");
    assert_eq!(u.segments[0].state, SegState::Draft);
    assert_eq!(u.segments[0].origin, Some(Origin::new(OriginKind::Tm).with_match(75)));
    assert_eq!(unit(&d, "0").segments[0].state, SegState::New); // <target state="needs-translation"></target>
    assert_eq!(unit(&d, "3").segments[0].state, SegState::Translated);
    assert_eq!(unit(&d, "5").segments[0].state, SegState::SignedOff);
    assert_eq!(unit(&d, "5").lock_reason.as_deref(), Some("final"));
    let all = TXLF.extract(SAMPLE_TXLF, &locked_opts()).unwrap();
    assert_eq!(ids(&all), ["0", "1", "2", "3", "4", "5"]);
    assert_eq!(unit(&all, "2").lock_reason.as_deref(), Some("locked"));
    assert_eq!(unit(&all, "2").segments[0].state, SegState::Locked);
    assert_eq!(d.source_lang.as_deref(), Some("en-US"));
    assert_eq!(d.target_lang.as_deref(), Some("de-DE"));
    assert_eq!(merged(&TXLF_T, &d), SAMPLE_TXLF);
}

#[test]
fn txlf_codes_and_free_placeholders() {
    let d = extract(&TXLF, SAMPLE_TXLF);
    // bpt/ept are points whose order matters (LocOS x tokens); ph with {0} may move (p)
    let u0 = &d.units[0].segments[0].source;
    assert!(matches!(&u0[1], Inline::Tag { kind: TagKind::Standalone, free: false, .. }));
    let u1 = &d.units[1].segments[0].source;
    assert!(u1.iter().any(|i| matches!(i, Inline::Tag { free: true, display, .. } if display == "{0}")));
    // the original markup is kept in the code table
    assert_eq!(d.units[0].codes["1"].standalone.as_deref(), Some("<bpt id=\"1\">&lt;b&gt;</bpt>"));
}

#[test]
fn seg_source_segments_are_units() {
    let d = extract(&TXLF, SEGSRC);
    assert_eq!(ids(&d), ["1000000000", "1000000001", "2", "3"]);
    assert_eq!(locos(&d.units[1]), "Two \u{27e6}p1\u{27e7}three.");
    assert_eq!(merged(&TXLF, &d), SEGSRC);
    // no target yet: a target mirroring seg-source is created, only the translated segment filled
    let out = translate(&TXLF, SEGSRC, &[(1_000_000_000, "Uno.")]);
    let s = String::from_utf8_lossy(&out).to_string();
    assert!(s.contains("</seg-source><target><mrk mtype=\"seg\" mid=\"1\">Uno.</mrk> <mrk mtype=\"seg\" mid=\"2\"></mrk></target></trans-unit>"), "{s}");
    let out = translate(&TXLF, SEGSRC, &[(1_000_000_000, "Uno."), (1_000_000_001, "Dos \u{27e6}p1\u{27e7}tres.")]);
    let s = String::from_utf8_lossy(&out).to_string();
    assert!(s.contains("<target><mrk mtype=\"seg\" mid=\"1\">Uno.</mrk> <mrk mtype=\"seg\" mid=\"2\">Dos <x id=\"1\"/>tres.</mrk></target>"), "{s}");
    // translating only the second one creates the mirror as well
    let out = translate(&TXLF, SEGSRC, &[(1_000_000_001, "Dos")]);
    assert!(String::from_utf8_lossy(&out).contains("<mrk mtype=\"seg\" mid=\"1\"></mrk> <mrk mtype=\"seg\" mid=\"2\">Dos</mrk></target>"));
    // an existing target that mirrors the segments: each mrk is filled in place (state needs-translation: open)
    let raw = String::from_utf8_lossy(SEGSRC).replace("<target><mrk", "<target state=\"needs-translation\"><mrk");
    let d = extract(&TXLF, raw.as_bytes());
    assert_eq!(ids(&d), ["1000000000", "1000000001", "1000000002", "1000000003", "2", "3"]);
    let out = translate(&TXLF, raw.as_bytes(), &[(1_000_000_003, "Beta-es")]);
    let s = String::from_utf8_lossy(&out).to_string();
    assert!(s.contains("<target state=\"needs-translation\"><mrk mtype=\"seg\" mid=\"1\"/> <mrk mtype=\"seg\" mid=\"2\">Beta-es</mrk></target>"), "{s}");
    // the same, an empty `<mrk/>` is opened
    let out = translate(&TXLF, raw.as_bytes(), &[(1_000_000_002, "Alfa")]);
    assert!(String::from_utf8_lossy(&out).contains("<mrk mtype=\"seg\" mid=\"1\">Alfa</mrk> <mrk mtype=\"seg\" mid=\"2\"></mrk></target>"));
}

#[test]
fn whole_unit_target_is_created_or_filled() {
    // created after the source (no seg-source), with the source's own prefix; an empty target element is opened
    let out = translate(&TXLF, SEGSRC, &[(2, "Einfach & klar.")]);
    assert!(String::from_utf8_lossy(&out).contains("<source>Plain &amp; simple.</source><target>Einfach &amp; klar.</target></trans-unit>"));
    let out = translate(&TXLF, SEGSRC, &[(3, "\u{27e6}g1\u{27e7}\u{27e6}g2\u{27e7}Verschachtelt\u{27e6}/g2\u{27e7} Text\u{27e6}/g1\u{27e7}")]);
    assert!(String::from_utf8_lossy(&out).contains("<target><g id=\"1\"><g id=\"2\">Verschachtelt</g> Text</g></target>"), "{}", String::from_utf8_lossy(&out));
}

#[test]
fn inline_edge_cases() {
    let d = extract(&TXML, INLINE);
    assert_eq!(ids(&d), ["0", "1", "4"]);
    assert_eq!(locos(&d.units[0]), "A \u{27e6}x1\u{27e7}unpaired start\u{27e6}x2\u{27e7} end\u{27e6}x3\u{27e7}");
    // white space around a source is kept around the translation
    let out = translate(&TXML, INLINE, &[(1, "\u{27e6}x1\u{27e7}")]);
    assert!(contains(&out, "<source>  <ut x=\"1\">&lt;br/&gt;</ut>  </source><target>  <ut x=\"1\">&lt;br/&gt;</ut>  </target>"), "{}", String::from_utf8_lossy(&out));
    // a foreign element is a point carrying its own bytes and may be reordered
    let out = translate(&TXML, INLINE, &[(4, "Text \u{27e6}x1\u{27e7}")]);
    assert!(contains(&out, "<target>Text <foo a=\"1\">kept as a point</foo></target>"));
    // confirmed target with entities and a non-BMP character
    let d = extract(&TXML_T, INLINE);
    assert_eq!(locos(unit(&d, "3")), "<b> \u{e9}\u{1f600} ok");
    assert_eq!(flat_target(unit(&d, "3")), "old <b>");
}

#[test]
fn escaping_and_illegal_characters() {
    let out = translate(&TXML, SAMPLE_TXML, &[(6, "a < b & c ]]> d \u{b}e\u{1}f \u{1f600} \"q\" 'a'")]);
    let s7 = String::from_utf8_lossy(seg(&out, "7")).to_string();
    // a vertical tab becomes a newline (and is trimmed or kept inside), control characters are dropped
    assert!(s7.contains("<target>a &lt; b &amp; c ]]&gt; d \nef \u{1f600} \"q\" 'a'</target>"), "{s7}");
    TXML_T.extract(&out, &opts()).expect("well-formed");
    let out = translate(&TXML, SAMPLE_TXML, &[(6, "   ")]);
    assert_eq!(out, SAMPLE_TXML, "a blank translation is no translation");
}

#[test]
fn cdata_comments_and_references_in_a_source() {
    let data = FILES_LANG_EN;
    let d = extract(&TXML_T, data);
    // CDATA keeps `<raw>` as text; a comment inside a source is dropped; references resolve
    assert!(d.units.iter().any(|u| locos(u).contains("<raw>")));
    assert!(d.units.iter().any(|u| locos(u).contains('\u{a9}')));
    let c = d.units.iter().find(|u| locos(u).starts_with("menu to continue.")).expect("comment unit");
    assert_eq!(locos(c), "menu to continue. Settings");
    assert_eq!(merged(&TXML_T, &d), data);
}

const FILES_LANG_EN: &[u8] = include_bytes!("data/lang_en.txml");

// ---- Okapi okf_txml test resources: Okapi's unit is the block (all segments, <ws> codes included); ours is the
// segment, as LocOS makes it. Compared per block with codes and white space removed they are the same text.

fn squash(s: &str) -> String {
    let mut out = String::new();
    let mut in_token = false;
    for c in s.chars() {
        match c {
            '\u{27e6}' => in_token = true,
            '\u{27e7}' => in_token = false,
            c if !in_token && !c.is_whitespace() => out.push(c),
            _ => {}
        }
    }
    out
}

fn blocks(data: &[u8]) -> Vec<String> {
    let d = extract(&TXML_T, data);
    let mut order: Vec<String> = Vec::new();
    let mut m = std::collections::BTreeMap::<String, String>::new();
    for u in &d.units {
        let b = u.context.key.clone().unwrap().split('/').next().unwrap().to_string();
        if !m.contains_key(&b) {
            order.push(b.clone());
        }
        m.entry(b).or_default().push_str(&squash(&locos(u)));
    }
    order.into_iter().map(|b| m.remove(&b).unwrap()).filter(|s| !s.is_empty()).collect()
}

#[test]
fn okapi_txml_resources() {
    let files: [(&str, &[u8], usize, usize, usize); 3] = [
        ("Test01.docx.txml", include_bytes!("data/Test01.docx.txml"), 8, 10, 9),
        ("Test02.html.txml", include_bytes!("data/Test02.html.txml"), 8, 10, 8),
        ("Test03.mif.txml", include_bytes!("data/Test03.mif.txml"), 235, 235, 233),
    ];
    for (name, data, locos_units, with_targets, okapi_units) in files {
        // LocOS granularity: segments LocOS translates
        assert_eq!(extract(&TXML, data).units.len(), locos_units, "{name}");
        // existing translations loaded: the confirmed segments too
        assert_eq!(extract(&TXML_T, data).units.len(), with_targets, "{name}");
        assert_eq!(merged(&TXML_T, &extract(&TXML_T, data)), data, "{name}: untouched merge");
        // Okapi's units are blocks
        let b = blocks(data);
        assert_eq!(b.len(), okapi_units, "{name}: blocks");
    }
    // Test02: Okapi shows the two sentences of block 3 as one unit
    let d = extract(&TXML, include_bytes!("data/Test02.html.txml"));
    let t: Vec<String> = d.units.iter().map(locos).collect();
    assert!(t.contains(&"First sentence.".to_string()) && t.contains(&"Second sentence.".to_string()));
    // Test02: `<ut>` points inside a segment (type="unknown") are one point each
    assert!(t.iter().any(|x| x == "Text in \u{27e6}x1\u{27e7}bold\u{27e6}x2\u{27e7} and \u{27e6}x3\u{27e7}italics\u{27e6}x4\u{27e7}."), "{t:?}");
}
