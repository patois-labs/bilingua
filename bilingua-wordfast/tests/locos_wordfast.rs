//! The LocOS Wordfast suite (`tests/verify_wordfast_filter.py`, 10 cases) ported: same fixtures, same
//! expectations, in Rust. The fixtures are the LocOS ones (`tests/data/locos_sample.*`).

mod common;

use bilingua_model::*;
use common::*;

const SAMPLE_TXML: &[u8] = include_bytes!("data/locos_sample.txml");
const SAMPLE_TXLF: &[u8] = include_bytes!("data/locos_sample.txlf");

fn map(pairs: &[(usize, &str)]) -> std::collections::BTreeMap<usize, String> {
    pairs.iter().map(|(k, v)| (*k, v.to_string())).collect()
}

// 1. test_registration
#[test]
fn registration() {
    assert_eq!(TXML.id(), "txml");
    assert_eq!(TXLF.id(), "txlf");
    assert_eq!(TXML.extensions(), ["txml"]);
    assert_eq!(TXLF.extensions(), ["txlf"]);
    assert!(TXML.sniff(SAMPLE_TXML) && !TXML.sniff(SAMPLE_TXLF));
    assert!(TXLF.sniff(SAMPLE_TXLF) && !TXLF.sniff(SAMPLE_TXML));
    assert!(TXML.bilingual() && TXLF.bilingual());
    assert_eq!(bilingua_wordfast::filters().len(), 2);
}

// 2. test_identity
#[test]
fn identity() {
    for (f, raw) in [(&TXML as &dyn Filter, SAMPLE_TXML), (&TXLF, SAMPLE_TXLF), (&TXML_T, SAMPLE_TXML), (&TXLF_T, SAMPLE_TXLF)] {
        let d = extract(f, raw);
        assert_eq!(merged(f, &d), raw);
        // a target language is only stamped when something was translated (and this file has one already)
        let mut o = opts();
        o.target_lang = Some("fr-FR".into());
        assert_eq!(f.merge(&f.extract(raw, &o).unwrap()).unwrap(), raw);
    }
}

// 3. test_txml_units_and_protection
#[test]
fn txml_units_and_protection() {
    let u = unit_map(&TXML, SAMPLE_TXML);
    assert_eq!(
        u,
        map(&[
            (0, "Open the \u{27e6}g1\u{27e7}Settings\u{27e6}/g1\u{27e7} menu."),
            (1, "Press \u{27e6}x1\u{27e7}Save & close."),
            (5, "Machine line."),
            (6, "Left untranslated."),
            (7, "With a comment."),
        ])
    );
    let r = lock_reasons(&TXML, SAMPLE_TXML);
    assert_eq!(r.get("locked"), Some(&2), "{r:?}");
    assert_eq!(r.get("confirmed"), None, "{r:?}"); // confirmed work is an editable unit with a target, see behaviour.rs
    assert_eq!(r.get("localizable"), Some(&1), "{r:?}");

    let mut d = extract(&TXML, SAMPLE_TXML);
    set(&mut d, 0, "Oeffnen Sie das \u{27e6}g1\u{27e7}Einstellungen\u{27e6}/g1\u{27e7}-Menue.");
    set(&mut d, 1, "Druecken Sie \u{27e6}x1\u{27e7}Speichern & schliessen.");
    for k in [5, 6, 7] {
        set(&mut d, k, &format!("T{k}"));
    }
    let out = merged(&TXML, &d);
    let s1 = seg(&out, "1");
    assert!(
        contains(s1, "<ws> </ws><target>Oeffnen Sie das <ut type=\"start\" x=\"1\">&lt;b class='ui'&gt;</ut>Einstellungen<ut type=\"end\" x=\"1\">&lt;/b&gt;</ut>-Menue.</target>"),
        "{}",
        String::from_utf8_lossy(s1)
    );
    let s2 = seg(&out, "2");
    assert!(contains(s2, "<target score=\"85\">Druecken Sie <ut x=\"2\">&lt;br/&gt;</ut>Speichern &amp; schliessen.</target>"), "{}", String::from_utf8_lossy(s2));
    assert!(contains(seg(&out, "6"), "<target>T5</target>"));
    assert!(contains(seg(&out, "7"), "<target>T6</target>")); // <target/> filled
    let s8 = seg(&out, "8");
    assert!(contains(s8, "<ws>  </ws><source>With a comment.</source><ws> </ws><target>T7</target>"), "{}", String::from_utf8_lossy(s8));
    assert!(contains(s8, "Keep it short &amp; plain</comment>"));
    for sid in ["3", "4", "5", "9", "10"] {
        assert_eq!(seg(&out, sid), seg(SAMPLE_TXML, sid), "{sid}");
    }
    only_changed(SAMPLE_TXML, &out, seg_owned, &["1", "2", "6", "7", "8"]);

    // protected segments are never written, even when a caller puts a target on them
    let mut d = extract(&TXML_T, SAMPLE_TXML);
    for u in d.units.iter_mut() {
        if !u.translate {
            u.segments[0].target = vec![Inline::text("HACKED")];
        }
    }
    let out = merged(&TXML_T, &d);
    for sid in ["3", "4", "5", "9", "10"] {
        assert_eq!(seg(&out, sid), seg(SAMPLE_TXML, sid), "{sid}");
    }
    // set_target refuses a locked unit
    let mut d = TXML.extract(SAMPLE_TXML, &locked_opts()).unwrap();
    let locked = d.units.iter().find(|u| u.id == "3").unwrap().segments[0].id.clone();
    assert!(matches!(d.set_target(&locked, vec![Inline::text("x")]), Err(Error::Locked(_))));
}

// 4. test_txml_tag_damage_is_safe
#[test]
fn txml_tag_damage_is_safe() {
    let mut d = extract(&TXML, SAMPLE_TXML);
    // a stray close, an invented code, an open that never closes: bypass set_target's checks on purpose
    d.units[0].segments[0].target = vec![
        Inline::tag("1", TagKind::Close, "</b>"),
        Inline::text(" Text "),
        Inline::tag("9", TagKind::Standalone, "<x/>"),
        Inline::text(" ohne "),
        Inline::tag("1", TagKind::Open, "<b>"),
        Inline::text("Tags"),
    ];
    d.units[1].segments[0].target = vec![Inline::text("x")];
    let out = merged(&TXML, &d);
    let back = TXML.extract(&out, &opts()).expect("always well-formed");
    // segment 1 now has a finished-looking target, so it reads back as confirmed work
    assert_eq!(back.units.len(), 4);
    let s1 = String::from_utf8_lossy(seg(&out, "1")).to_string();
    assert!(!s1.contains("x=\"9\"") && !s1.contains('\u{27e6}'), "{s1}");
    assert!(s1.contains("Tags<ut type=\"end\" x=\"1\">&lt;/b&gt;</ut></target>"), "{s1}");
    assert!(contains(seg(&out, "2"), "score=\"85\">x</target>"));
}

// 5. test_txlf
#[test]
fn txlf_units_protection_and_writing() {
    let u = unit_map(&TXLF, SAMPLE_TXLF);
    assert_eq!(
        u,
        map(&[(0, "Open the \u{27e6}x1\u{27e7}Settings\u{27e6}x2\u{27e7} menu."), (1, "Then click \u{27e6}p1\u{27e7}."), (4, "Left untranslated.")])
    );
    let r = lock_reasons(&TXLF, SAMPLE_TXLF);
    assert_eq!((r.get("locked"), r.get("confirmed"), r.get("final")), (Some(&1), None, Some(&1)), "{r:?}");

    let mut d = extract(&TXLF, SAMPLE_TXLF);
    set(&mut d, 0, "Oeffnen \u{27e6}x1\u{27e7}Einstellungen\u{27e6}x2\u{27e7}.");
    set(&mut d, 1, "T1");
    set(&mut d, 4, "T4");
    let out = merged(&TXLF, &d);
    assert!(
        contains(tu(&out, "1"), "<target state=\"translated\">Oeffnen <bpt id=\"1\">&lt;b&gt;</bpt>Einstellungen<ept id=\"1\">&lt;/b&gt;</ept>.</target>"),
        "{}",
        String::from_utf8_lossy(tu(&out, "1"))
    );
    assert!(contains(tu(&out, "2"), "<target state=\"translated\" gs4tr:score=\"75\">T1</target>"), "{}", String::from_utf8_lossy(tu(&out, "2")));
    assert!(contains(tu(&out, "5"), "<target>T4</target>"));
    for id in ["3", "4", "6"] {
        assert_eq!(tu(&out, id), tu(SAMPLE_TXLF, id), "{id}");
    }
    only_changed(SAMPLE_TXLF, &out, tu_owned, &["1", "2", "5"]);

    // a gs4tr lock on the target element also protects
    let raw = String::from_utf8(SAMPLE_TXLF.to_vec()).unwrap().replace("<target state=\"needs-translation\">", "<target gs4tr:locked=\"true\">");
    assert!(!unit_map(&TXLF, raw.as_bytes()).contains_key(&0));

    // protected units are never written, even when a caller puts a target on them
    let mut d = extract(&TXLF_T, SAMPLE_TXLF);
    for u in d.units.iter_mut() {
        if !u.translate {
            u.segments[0].target = vec![Inline::text("HACKED")];
        }
    }
    let out = merged(&TXLF_T, &d);
    for id in ["3", "4", "6"] {
        assert_eq!(tu(&out, id), tu(SAMPLE_TXLF, id), "{id}");
    }
}

// 6. test_untranslated_stays_empty
#[test]
fn untranslated_stays_empty() {
    // a unit nobody translated is not written (no copy of the source into the target)
    let out = translate(&TXML, SAMPLE_TXML, &[(0, "A")]);
    assert_eq!(seg(&out, "7"), seg(SAMPLE_TXML, "7"));
    let out = translate(&TXLF, SAMPLE_TXLF, &[(0, "A")]);
    assert_eq!(tu(&out, "5"), tu(SAMPLE_TXLF, "5"));
}

// 7. test_encodings
#[test]
fn encodings() {
    let mut crlf = vec![0xEF, 0xBB, 0xBF];
    crlf.extend_from_slice(String::from_utf8_lossy(SAMPLE_TXML).replace('\n', "\r\n").as_bytes());
    let d = extract(&TXML, &crlf);
    assert_eq!(merged(&TXML, &d), crlf);
    let out = translate(&TXML, &crlf, &[(5, "Maschine.")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    let n = |v: &[u8]| v.windows(2).filter(|w| *w == b"\r\n").count();
    assert_eq!(n(&out), n(&crlf));
    assert!(contains(&out, "<segment segmentId=\"6\" gtmt=\"true\">\r\n<source>Machine line.</source>\r\n<target>Maschine.</target>"));

    let s = String::from_utf8_lossy(SAMPLE_TXML).replace("encoding=\"UTF-8\"", "encoding=\"UTF-16\"");
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    u16.extend(s.encode_utf16().flat_map(|c| c.to_le_bytes()));
    assert_eq!(unit_map(&TXML, &u16)[&5], "Machine line.");
    let out16 = translate(&TXML, &u16, &[(5, "Maschine \u{e4}.")]);
    assert_eq!(&out16[..2], &[0xFF, 0xFE]);
    let back: Vec<u16> = out16[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    assert!(String::from_utf16(&back).unwrap().contains("<target>Maschine \u{e4}.</target>"));
    assert_eq!(merged(&TXML, &extract(&TXML, &u16)), u16);

    // a declared legacy encoding is refused
    let legacy = String::from_utf8_lossy(SAMPLE_TXML).replace("encoding=\"UTF-8\"", "encoding=\"ISO-8859-1\"");
    match TXML.extract(legacy.as_bytes(), &opts()) {
        Err(Error::Encoding(m)) => assert!(m.contains("TXML"), "{m}"),
        other => panic!("{other:?}"),
    }
    // so is undeclared non-UTF-8
    let mut bad = SAMPLE_TXML.to_vec();
    bad.extend_from_slice(b"\xe9");
    assert!(TXML.extract(&bad, &opts()).is_err());
}

// 8. test_target_locale
#[test]
fn target_locale() {
    let raw = String::from_utf8_lossy(SAMPLE_TXML).replace(" targetlocale=\"de_DE\"", "");
    let mut o = opts();
    o.target_lang = Some("fr-FR".into());
    let mut d = TXML.extract(raw.as_bytes(), &o).unwrap();
    assert_eq!(d.target_lang.as_deref(), Some("fr-FR"));
    set(&mut d, 5, "M");
    let out = merged(&TXML, &d);
    assert!(contains(&out, "createdby=\"hand-written fixture\" targetlocale=\"fr_FR\">"), "{}", String::from_utf8_lossy(&out[..300]));
    // nothing translated: the original bytes, no stamp
    let d = TXML.extract(raw.as_bytes(), &o).unwrap();
    assert_eq!(merged(&TXML, &d), raw.as_bytes());
    // an existing targetlocale is never changed
    let mut d = TXML.extract(SAMPLE_TXML, &o).unwrap();
    set(&mut d, 5, "M");
    assert!(contains(&merged(&TXML, &d), "targetlocale=\"de_DE\""));
    // TXLF: target-language on a file that has none
    let raw = String::from_utf8_lossy(SAMPLE_TXLF).replace(" target-language=\"de-DE\"", "");
    let mut d = TXLF.extract(raw.as_bytes(), &o).unwrap();
    set(&mut d, 4, "M");
    assert!(contains(&merged(&TXLF, &d), "target-language=\"fr-FR\""));
}

// 9. test_hostile_inputs
#[test]
fn hostile_inputs() {
    let doctype = String::from_utf8_lossy(SAMPLE_TXML).replacen("<txml ", "<!DOCTYPE x [<!ENTITY a \"b\">]><txml ", 1);
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE t [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><txml><translatable><segment><source>&e;</source></segment></translatable></txml>";
    for (name, raw) in [
        ("doctype", doctype.as_bytes()),
        ("xxe", &xxe[..]),
        ("not txml", SAMPLE_TXLF),
        ("malformed", b"<txml><translatable><segment><source>a</segment>"),
    ] {
        assert!(TXML.extract(raw, &opts()).is_err(), "{name} must be refused");
    }
    assert!(TXLF.extract(SAMPLE_TXML, &opts()).is_err(), "txml is not txlf");
    // segment cap
    let one = ExtractOptions { limits: Some(Limits { max_units: 1, ..Limits::default() }), ..opts() };
    for (f, raw) in [(&TXML as &dyn Filter, SAMPLE_TXML), (&TXLF, SAMPLE_TXLF)] {
        assert!(matches!(f.extract(raw, &one), Err(Error::Limit(_))), "over the segment cap must be refused");
    }
    // the markup budget
    let mut bomb = String::from("<txml><translatable>");
    bomb.push_str(&"<segment><source>a</source></segment>".repeat(50));
    bomb.push_str("</translatable></txml>");
    let three = ExtractOptions { limits: Some(Limits { max_units: 3, ..Limits::default() }), ..opts() };
    match TXML.extract(bomb.as_bytes(), &three) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("TXML"), "{}", e.msg),
        other => panic!("{other:?}"),
    }
}

// 10. test_export_end_to_end: a partly translated document merges, the rest keeps its bytes
#[test]
fn partial_translation_keeps_the_rest() {
    fn pseudo(f: &dyn Filter, raw: &[u8]) -> Vec<u8> {
        let mut d = extract(f, raw);
        let n = d.units.len();
        for i in 0..n - 1 {
            let u = &d.units[i];
            let id = u.segments[0].id.clone();
            let mut t = vec![Inline::text("DE ")];
            t.extend(u.source());
            d.set_target(&id, t).unwrap();
        }
        merged(f, &d)
    }
    let out = pseudo(&TXML, SAMPLE_TXML);
    assert!(contains(seg(&out, "6"), "<target>DE Machine line.</target>"), "{}", String::from_utf8_lossy(seg(&out, "6")));
    assert_eq!(seg(&out, "8"), seg(SAMPLE_TXML, "8")); // untranslated: no target created
    assert_eq!(seg(&out, "5"), seg(SAMPLE_TXML, "5"));
    let out = pseudo(&TXLF, SAMPLE_TXLF);
    assert!(contains(tu(&out, "2"), "DE Then click"), "{}", String::from_utf8_lossy(tu(&out, "2")));
    assert_eq!(tu(&out, "5"), tu(SAMPLE_TXLF, "5"));
    assert_eq!(tu(&out, "3"), tu(SAMPLE_TXLF, "3"));
}
