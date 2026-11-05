//! Port of the Localization OS suite `tests/verify_ttx_filter.py` (10 cases). Eight are ported one to one with the
//! suite's own fixtures and expected bytes. `test_registration` (the LocOS registry) has its Bilingua counterpart in
//! `registration_counterpart`; `test_export_end_to_end` goes through the LocOS ingest and export services, so only its
//! filter part is ported (`export_partial_translation_keeps_the_ttx`); the service layer is the adapter's gate.
//! Cases run with `Ttx::locos()` (a finished translation is not a unit) unless the name says otherwise.

mod common;

use bilingua_model::*;
use bilingua_ttx::Ttx;
use common::*;

fn fx(b: &[u8]) -> Vec<u8> {
    String::from_utf8(b.to_vec()).unwrap().replace("\r\n", "\n").into_bytes()
}

fn ttx() -> Vec<u8> {
    fx(include_bytes!("data/locos_suite_sample.ttx"))
}

fn unseg() -> Vec<u8> {
    fx(include_bytes!("data/locos_suite_unsegmented.ttx"))
}

fn mapping() -> Vec<(usize, &'static str)> {
    vec![
        (0, "Klicken Sie auf ⟦g1⟧Speichern⟦/g1⟧, um F&E-Notizen zu behalten."),
        (1, "Zeile eins⟦x1⟧⟦g2⟧Zeile zwei⟦/g2⟧ hier."),
        (3, "Leeres Ziel hier."),
        (4, "⟦x1⟧ 42"),
    ]
}

#[test]
fn registration_counterpart() {
    assert_eq!(LOCOS.id(), "ttx");
    assert_eq!(LOCOS.extensions(), ["ttx"]);
    assert!(LOCOS.sniff(&ttx()) && LOCOS.sniff(&utf16(&ttx(), false, true)) && LOCOS.sniff(&utf16(&ttx(), true, false)));
    assert!(!LOCOS.sniff(b"<?xml version='1.0'?><xliff version='1.2'/>"));
    assert!(LOCOS.bilingual());
}

#[test]
fn identity() {
    for raw in [ttx(), utf16(&ttx(), false, true)] {
        assert_identity(&LOCOS, &raw);
        assert_identity(&DEFAULT, &raw);
        // the target language is already set: a target language in the options changes nothing
        let d = LOCOS.extract(&raw, &opts().with_target_lang("de-DE")).unwrap();
        assert_eq!(LOCOS.merge(&d).unwrap(), raw);
    }
}

#[test]
fn units() {
    let want = vec![
        (0, "Click ⟦g1⟧Save⟦/g1⟧ to keep R&D notes.".to_string()),
        (1, "Line one⟦x1⟧⟦g2⟧line two⟦/g2⟧ here.".to_string()),
        (3, "Empty target here.".to_string()),
        (4, "⟦x1⟧ 42".to_string()),
    ];
    assert_eq!(units_with(&LOCOS, &ttx()), want);
    // the finished translation (key 2) is one more unit by default, with its target loaded
    let d = extract_with(&DEFAULT, &ttx());
    let keys: Vec<_> = d.units.iter().map(|u| u.context.key.clone().unwrap()).collect();
    assert_eq!(keys, ["0", "1", "2", "3", "4"]);
    let done = &d.units[2].segments[0];
    assert_eq!((tok(&done.source), tok(&done.target)), ("Already done.".into(), "Schon erledigt.".into()));
    assert_eq!(done.state, SegState::Translated);
    assert_eq!(done.origin, Some(Origin::new(OriginKind::Human).with_match(100)));
    // one run of text outside any segment is counted
    let d = LOCOS.extract(&ttx(), &opts()).unwrap();
    assert!(d.warnings.iter().any(|w| w.kind == "ttx_unsegmented_text" && w.message.starts_with("1 ")), "{:?}", d.warnings);
    // the markup of the first unit's codes is the file's own
    let first = &d.units[0];
    assert!(first.codes["1"].open.as_deref().unwrap().starts_with("<ut Type=\"start\""));
    assert!(first.codes["1"].close.as_deref().unwrap().starts_with("<ut Type=\"end\""));
    // the same file as UTF-16 gives the same units
    assert_eq!(units_with(&LOCOS, &utf16(&ttx(), false, true)), want);
    assert_eq!(units_with(&LOCOS, &utf16(&ttx(), true, true)), want);
}

#[test]
fn translated_round_trip() {
    let out = translate_with(&LOCOS, &ttx(), &mapping());
    LOCOS.extract(&out, &opts()).expect("still a TTX");
    assert!(!contains(&out, b"HACK"));
    let t = tus(&out);
    assert_eq!(
        t[0],
        b"<Tu MatchPercent=\"0\" Origin=\"mt\"><Tuv Lang=\"EN-US\">Click <ut Type=\"start\" RightEdge=\"angle\" DisplayText=\"b\">&lt;b&gt;</ut>Save<ut Type=\"end\" LeftEdge=\"angle\" DisplayText=\"b\">&lt;/b&gt;</ut> to keep R&amp;D notes.</Tuv><Tuv Lang=\"DE-DE\">Klicken Sie auf <ut Type=\"start\" RightEdge=\"angle\" DisplayText=\"b\">&lt;b&gt;</ut>Speichern<ut Type=\"end\" LeftEdge=\"angle\" DisplayText=\"b\">&lt;/b&gt;</ut>, um F&amp;E-Notizen zu behalten.</Tuv></Tu>"
            .to_vec(),
        "{}",
        String::from_utf8_lossy(&t[0])
    );
    // missing target Tuv created after the source; source trailing space carried
    assert!(
        t[1].ends_with(b" here. </Tuv><Tuv Lang=\"DE-DE\">Zeile eins<ut DisplayText=\"br\">&lt;br&gt;</ut><df Bold=\"on\">Zeile zwei</df> hier. </Tuv></Tu>"),
        "{}",
        String::from_utf8_lossy(&t[1])
    );
    // the protected Tu is byte-identical
    assert_eq!(t[2], tus(&ttx())[2]);
    // self-closing target expanded; single-quoted MatchPercent keeps its quoting
    assert_eq!(
        t[3],
        b"<Tu MatchPercent='0' Origin=\"mt\"><Tuv Lang=\"EN-US\">Empty target here.</Tuv><Tuv Lang=\"DE-DE\">Leeres Ziel hier.</Tuv></Tu>".to_vec()
    );
    assert_eq!(outside(&out), outside(&ttx()), "bytes outside the written Tu elements changed");
    // partial: only key 3 written, everything else identical
    let part = translate_with(&LOCOS, &ttx(), &[(3, "Leeres Ziel hier.")]);
    assert!(tus(&part)[0] == tus(&ttx())[0] && tus(&part)[1] == tus(&ttx())[1]);
    assert_eq!(part.windows(11).filter(|w| *w == b"Origin=\"mt\"").count(), 1);
    // read back: every written unit is now a translated unit with its text, in the default mode
    let back = extract_with(&DEFAULT, &out);
    assert_eq!(back.units.len(), 5);
    let t0 = &back.units[0].segments[0];
    assert_eq!(tok(&t0.target), "Klicken Sie auf ⟦g1⟧Speichern⟦/g1⟧, um F&E-Notizen zu behalten.");
    assert_eq!(t0.origin, Some(Origin::new(OriginKind::Mt).with_match(0)));
    assert_eq!(t0.state, SegState::Draft);
    assert_eq!(tok(&back.units[1].segments[0].target), "Zeile eins⟦x1⟧⟦g2⟧Zeile zwei⟦/g2⟧ hier.");
    assert_eq!(tok(&back.units[4].segments[0].target), "⟦x1⟧ 42");
}

#[test]
fn unsegmented_and_all_translated() {
    // a TTX with no <Tu> is not refused: it has no units and says why (LocOS refuses with a plain message)
    let d = LOCOS.extract(&unseg(), &opts()).unwrap();
    assert!(d.units.is_empty());
    let w = d.warnings.iter().find(|w| w.kind == "ttx_not_segmented").expect("warning");
    assert!(w.message.contains("not segmented") && w.message.contains("Pre-translate"), "{}", w.message);
    // its text outside any segment is listed, locked, with include_locked
    let d = LOCOS.extract(&unseg(), &opts().with_include_locked(true)).unwrap();
    assert_eq!(d.units.len(), 1);
    assert!(!d.units[0].translate);
    assert_eq!(d.units[0].segments[0].source, vec![Inline::text("Nothing here is segmented yet.")]);
    assert_eq!(LOCOS.merge(&d).unwrap(), unseg());
    // every segment already translated: nothing to translate in LocOS mode, all targets loaded by default
    let mut done = ttx();
    for (from, to) in [
        (&b"<Tuv Lang=\"DE-DE\"></Tuv>"[..], &b"<Tuv Lang=\"DE-DE\">x</Tuv>"[..]),
        (b"<Tuv Lang=\"DE-DE\"/>", b"<Tuv Lang=\"DE-DE\">x</Tuv>"),
        (b"here. </Tuv></Tu>", b"here. </Tuv><Tuv Lang=\"DE-DE\">y</Tuv></Tu>"),
        (b" 42</Tuv></Tu>", b" 42</Tuv><Tuv Lang=\"DE-DE\">42</Tuv></Tu>"),
    ] {
        let s = String::from_utf8(done).unwrap().replace(std::str::from_utf8(from).unwrap(), std::str::from_utf8(to).unwrap());
        done = s.into_bytes();
    }
    assert!(LOCOS.extract(&done, &opts()).unwrap().units.is_empty());
    let d = extract_with(&DEFAULT, &done);
    assert_eq!(d.units.len(), 5);
    assert!(d.units.iter().all(|u| !u.segments[0].target.is_empty() && u.segments[0].state.is_accepted() || u.segments[0].state == SegState::Draft));
    assert_eq!(DEFAULT.merge(&d).unwrap(), done);
}

#[test]
fn encodings() {
    for (be, bom) in [(false, true), (true, true), (false, false)] {
        let raw = utf16(&ttx(), be, bom);
        let out = translate_with(&LOCOS, &raw, &mapping());
        let head = utf16(b"<?xml", be, bom);
        let head = &head[..head.len().min(if bom { 12 } else { 10 })];
        assert!(out.starts_with(head), "{be} {bom}: {:?}", &out[..12]);
        let text = decode16(&out, be, bom);
        assert!(text.contains("\r\n") && text.contains("Origin=\"mt\"") && text.contains("Leeres Ziel hier."));
        assert_eq!(text.matches("\r\n").count(), decode16(&raw, be, bom).matches("\r\n").count());
    }
    let mut bom8 = b"\xef\xbb\xbf".to_vec();
    bom8.extend(ttx());
    let out = translate_with(&LOCOS, &bom8, &mapping());
    assert!(out.starts_with(b"\xef\xbb\xbf<?xml") && contains(&out, b"Leeres Ziel"));
    let legacy = String::from_utf8(ttx()).unwrap().replace("encoding='utf-8'", "encoding='windows-1252'").into_bytes();
    match LOCOS.extract(&legacy, &opts()) {
        Err(Error::Encoding(m)) => assert!(m.contains("windows-1252"), "{m}"),
        other => panic!("a declared legacy encoding must be refused: {other:?}"),
    }
}

#[test]
fn inline_damage_is_safe() {
    let mut d = extract_with(&LOCOS, &ttx());
    let tag = |id: &str, kind| Inline::tag(id, kind, "x");
    // repeated, unknown, stray close
    d.units[0].segments[0].target = vec![
        Inline::text("Klicken "),
        tag("1", TagKind::Open),
        tag("1", TagKind::Open),
        Inline::text("Speichern "),
        tag("9", TagKind::Standalone),
        Inline::text(" "),
        tag("7", TagKind::Close),
        Inline::text(" und"),
    ];
    // close before open, open never closed
    d.units[1].segments[0].target = vec![tag("2", TagKind::Close), Inline::text("Zeile"), tag("2", TagKind::Open), Inline::text(" offen")];
    // text with markup characters
    d.units[2].segments[0].target = vec![Inline::text("Text mit "), tag("1", TagKind::Standalone), Inline::text(" unbekanntem Token & <b>")];
    let out = LOCOS.merge(&d).unwrap();
    LOCOS.extract(&out, &opts()).expect("well-formed after damage");
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("&amp; &lt;b>"), "{s}");
    assert!(!s.contains('\u{27E6}'));
    // the open of a pair that is never closed is closed by the writer
    assert!(s.contains("<df Bold=\"on\">Zeile offen</df>") || s.contains("Zeile<df Bold=\"on\"> offen</df>"), "{s}");
}

#[test]
fn hostile_inputs() {
    let doctype = String::from_utf8(ttx()).unwrap().replacen("<TRADOStag ", "<!DOCTYPE x [<!ENTITY a \"b\">]><TRADOStag ", 1).into_bytes();
    for (name, raw) in [
        ("doctype", doctype),
        ("not ttx", b"<?xml version='1.0'?><xliff version='1.2'><file/></xliff>".to_vec()),
        ("malformed", b"<TRADOStag><Body><Raw><Tu>".to_vec()),
    ] {
        assert!(LOCOS.extract(&raw, &opts()).is_err(), "{name} must be refused");
    }
    // 4 units, a cap of 2
    let capped = opts().with_limits(Limits::new().with_max_units(2));
    for raw in [ttx(), utf16(&ttx(), false, true)] {
        match DEFAULT.extract(&raw, &capped) {
            Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units),
            other => panic!("over the segment cap must be refused: {other:?}"),
        }
    }
    let flood = String::from_utf8(ttx()).unwrap().replace("</Raw>", &format!("{}</Raw>", "<ut/>".repeat(50))).into_bytes();
    match DEFAULT.extract(&flood, &opts().with_limits(Limits::new().with_max_units(1))) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("segments")),
        other => panic!("the pre-parse markup budget must refuse: {other:?}"),
    }
}

#[test]
fn localized_language() {
    let raw = String::from_utf8(ttx())
        .unwrap()
        .replace("TargetLanguage=\"DE-DE\"", "TargetLanguage=\"\"")
        .replace("<Tuv Lang=\"DE-DE\"></Tuv>", "")
        .replace("<Tuv Lang=\"DE-DE\"/>", "")
        .into_bytes();
    let fr = opts().with_target_lang("fr-FR");
    let mut d = LOCOS.extract(&raw, &fr).unwrap();
    set(&mut d, 0, "Bonjour ⟦g1⟧x⟦/g1⟧.");
    set(&mut d, 3, "Cible.");
    let out = String::from_utf8(LOCOS.merge(&d).unwrap()).unwrap();
    assert!(out.contains("TargetLanguage=\"FR-FR\""), "{}", &out[..400]);
    assert!(out.contains("<Tuv Lang=\"FR-FR\">Cible.</Tuv>"), "{out}");
    // nothing translated: no stamp, the original bytes (lead decision 11-04)
    let d0 = LOCOS.extract(&raw, &fr).unwrap();
    assert_eq!(LOCOS.merge(&d0).unwrap(), raw);
    // no target language anywhere: a target Tuv cannot be named
    let mut d = LOCOS.extract(&raw, &opts().with_target_lang(None::<String>)).unwrap();
    set(&mut d, 3, "Cible.");
    match LOCOS.merge(&d) {
        Err(Error::Write(m)) => assert!(m.contains("target language"), "{m}"),
        other => panic!("a target Tuv with no language must be refused: {other:?}"),
    }
}

#[test]
fn export_partial_translation_keeps_the_ttx() {
    let raw = utf16(&ttx(), false, true);
    let mut d = extract_with(&DEFAULT, &raw);
    assert_eq!(tok(&d.units[0].segments[0].source), "Click ⟦g1⟧Save⟦/g1⟧ to keep R&D notes.");
    set(&mut d, 3, "Leeres Ziel hier.");
    d.set_state(&id_of(&d, 3), SegState::Translated).unwrap();
    let out = DEFAULT.merge(&d).unwrap();
    assert!(out.starts_with(b"\xff\xfe"));
    let text = decode16(&out, false, true);
    assert_eq!(text.matches("Origin=\"mt\"").count(), 1);
    assert!(text.contains("<Tuv Lang=\"DE-DE\">Leeres Ziel hier.</Tuv>"));
    // untranslated units stay without a target (never source-filled)
    assert!(text.contains("<Tuv Lang=\"DE-DE\"></Tuv>"));
    assert!(text.contains("here. </Tuv></Tu>"));
}

#[test]
fn skeleton_is_stamped() {
    let d = extract_with(&DEFAULT, &ttx());
    assert_eq!(d.format, "ttx");
    assert_eq!(d.skeleton.parser().unwrap().name, "bilingua-ttx");
    assert_eq!((d.source_lang.as_deref(), d.target_lang.as_deref()), (Some("EN-US"), Some("DE-DE")));
    let _: &Ttx = &LOCOS;
}
