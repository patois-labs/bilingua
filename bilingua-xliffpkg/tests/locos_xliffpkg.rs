//! Ports of LocOS `tests/verify_xliff_package_filter.py` (7 cases: registration, identity, mqxlz, worldserver rules,
//! multi member wsxz, hostile inputs, export) plus the LocOS oracle golden.
//!
//! Differences from LocOS that the cases below assert (Bilingua loads finished translations as targets, the
//! adapter protects them): a finished or confirmed segment is extracted with its target and state, and is not
//! locked; only a real lock (`mq:locked`, `iws lock_status`, `sdl locked`, `translate="no"`) refuses a write.

mod common;
use bilingua_model::{Error, ExtractOptions, Inline, LimitKind, Limits, SegState};
use common::*;

#[test]
fn registration() {
    let f = bilingua_xliffpkg::filters();
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].id(), "xlz");
    assert_eq!(f[0].extensions(), &["xlz", "wsxz"]);
    assert!(f[0].bilingual());
    assert!(!f[0].sniff(&xlz()), "a zip is recognised by extension only");
    let m = bilingua_xliffpkg::mqxlz();
    assert_eq!((m.id(), m.extensions()), ("mqxlz", &["mqxlz"][..]));
}

#[test]
fn identity() {
    for (f, pkg) in [(bilingua_xliffpkg::mqxlz(), mqxlz()), (filter(), xlz()), (filter(), wsxz())] {
        let d = extract(&*f, &pkg);
        assert!(d.segment_count() > 0);
        assert_eq!(f.merge(&d).unwrap(), pkg, "{}", f.id());
        // a document that names no segment of the package writes nothing either
        let mut other = d.clone();
        for u in &mut other.units {
            u.id = format!("zz{}", u.id);
            for s in &mut u.segments {
                s.id = format!("zz{}", s.id);
            }
        }
        assert_eq!(f.merge(&other).unwrap(), pkg);
    }
}

#[test]
fn mqxlz_inner_mqxliff_goes_through_the_engine() {
    let f = bilingua_xliffpkg::mqxlz();
    let pkg = mqxlz();
    let mut d = extract(&*f, &pkg);
    assert_eq!(ids(&d), ["document.mqxliff|1", "document.mqxliff|2", "document.mqxliff|3", "document.mqxliff|4", "document.mqxliff|5"]);
    // row 3 is locked, 2 and 4 are confirmed (loaded with their targets), 5 is a machine draft
    let locked: Vec<bool> = d.units.iter().map(|u| !u.translate).collect();
    assert_eq!(locked, [false, false, true, false, false]);
    assert_eq!(locos(&d.find("document.mqxliff|2").unwrap().1.target), "Bestaetigte Zeile.");
    assert_eq!(d.find("document.mqxliff|4").unwrap().1.state, SegState::Confirmed);
    assert_eq!(d.units[0].context.location.as_deref(), Some("document.mqxliff"));
    assert_eq!(d.source_lang.as_deref(), Some("en-US"));
    assert_eq!(d.target_lang.as_deref(), Some("de-DE"));
    translate(&mut d, "document.mqxliff|1", |t| t.replace("Press ", "Druecke ").replace(" to begin.", " zum Beginnen."));
    // a write to the locked row is refused by the engine
    let _ = d.set_target("document.mqxliff|3", vec![Inline::text("NO")]);
    let out = f.merge(&d).unwrap();
    let (before, after) = (members(&pkg), members(&out));
    let head = |m: &[Member]| m.iter().map(|x| (x.0.clone(), x.1, x.2)).collect::<Vec<_>>();
    assert_eq!(head(&before), head(&after), "names, dates, compression");
    assert_eq!(before[1..], after[1..], "skeleton and preview identical");
    assert_eq!(comment(&out), b"memoQ export");
    let inner = text(&after[0].3);
    let tu1 = tu(&inner, "1");
    assert!(tu1.contains("mq:status=\"ManuallyConfirmed\""), "{tu1}");
    assert!(tu1.contains("Druecke "), "{tu1}");
    assert!(tu1.contains("<bpt id=\"1\" ctype=\"bold\">{}</bpt>Start<ept id=\"1\">{}</ept>"), "{tu1}");
    assert!(tu1.contains(" zum Beginnen.</target>"), "{tu1}");
    for id in ["2", "3", "4", "5"] {
        assert_eq!(tu(&inner, id), tu(&text(MQ), id), "{id}");
    }
}

#[test]
fn worldserver_rules() {
    let f = filter();
    let pkg = xlz();
    let mut d = extract(&*f, &pkg);
    let l: Vec<(String, bool, SegState)> = d.segments().map(|(u, s)| (s.id.clone(), !u.translate, s.state)).collect();
    // tu2 finished (loaded as a translation), tu3 locked, tu1 tu4 tu5 open
    assert_eq!(l[0], ("content.xlf|1".to_string(), false, SegState::New));
    assert_eq!(l[1], ("content.xlf|2".to_string(), false, SegState::Translated));
    assert!(l[2].1);
    assert_eq!(locos(&d.find("content.xlf|4").unwrap().1.source), "Use {0} items & \u{27E6}p1\u{27E7}more.", "#693: x is free");
    translate(&mut d, "content.xlf|1", |t| t.replace("Open the ", "Ouvrez la page ").replace(" page.", "."));
    translate(&mut d, "content.xlf|4", |t| t.replace("Use", "Utilisez").replace("items", "elements"));
    let _ = d.set_target("content.xlf|3", vec![Inline::text("OVERWRITE")]);
    let out = f.merge(&d).unwrap();
    let inner = text(&member(&out, "content.xlf"));
    let tu1 = tu(&inner, "1");
    assert!(tu1.contains(">Ouvrez la page <g id=\"1\">Settings</g>.</target>"), "{tu1}");
    assert!(tu1.contains("<iws:status translation_status=\"finished\" translation_type=\"manual_translation\"/>"), "{tu1}");
    let tu4 = tu(&inner, "4");
    assert!(tu4.contains(">Utilisez {0} elements &amp; <x id=\"2\"/>more.</target>"), "{tu4}");
    assert!(tu4.contains("<iws:status translation_status='finished'/>"), "quoting kept: {tu4}");
    for id in ["2", "3", "5"] {
        assert_eq!(tu(&inner, id), tu(&text(WS), id), "{id}");
    }
    assert!(!inner.contains("OVERWRITE"));
    assert_eq!(member(&out, "content.skl"), b"<skl>{1}</skl>");
    // the same rules hold for a loose WorldServer .xlf
    let loose = bilingua_xliff::filters().into_iter().find(|x| x.id() == "xliff").unwrap();
    let ld = loose.extract(WS, &ExtractOptions::default()).unwrap();
    let open: Vec<&str> = ld.segments().filter(|(u, s)| u.translate && s.state == SegState::New).map(|(_, s)| s.id.as_str()).collect();
    assert_eq!(open, ["1", "4", "5"]);
}

#[test]
fn multi_member_wsxz() {
    let f = filter();
    let pkg = wsxz();
    let mut d = extract(&*f, &pkg);
    let paths: Vec<_> = d.units.iter().filter_map(|u| u.context.location.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    assert_eq!(paths, ["fr-FR/guide.docx.sdlxliff", "fr-FR/help.html.xlf"]);
    assert_eq!(locos(&d.find("fr-FR/help.html.xlf|1").unwrap().1.source), "Open the \u{27E6}g1\u{27E7}Settings\u{27E6}/g1\u{27E7} page.");
    translate(&mut d, "fr-FR/help.html.xlf|1", |_| "Ouvrez ".into());
    translate(&mut d, "fr-FR/guide.docx.sdlxliff|1", |_| "DE eins".into());
    let out = f.merge(&d).unwrap();
    let (before, after) = (members(&pkg), members(&out));
    assert_eq!(before[0], after[0], "TMX identical");
    assert_eq!(before[2], after[2], "TBX identical");
    let sdl = text(&after[1].3);
    assert!(sdl.contains("DE eins") && sdl.contains("conf=\"Translated\""), "{sdl}");
    assert!(text(&after[3].3).contains(">Ouvrez </target>"));
    // writing only the second member leaves the first byte-identical
    let mut d2 = extract(&*f, &pkg);
    translate(&mut d2, "fr-FR/help.html.xlf|1", |_| "Ouvrez.".into());
    let out2 = members(&f.merge(&d2).unwrap());
    assert_eq!(out2[1], before[1]);
    assert_ne!(out2[3], before[3]);
}

fn refused(pkg: &[u8], why: &str) -> Error {
    match filter().extract(pkg, &ExtractOptions::default()) {
        Err(e) => e,
        Ok(_) => panic!("{why} must be refused"),
    }
}

/// Replaces every occurrence of `from` with `to` (same length): local and central headers both.
fn rename_all(mut z: Vec<u8>, from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut i = 0;
    while i + from.len() <= z.len() {
        if &z[i..i + from.len()] == from {
            z[i..i + from.len()].copy_from_slice(to);
        }
        i += 1;
    }
    z
}

#[test]
fn hostile_inputs() {
    refused(MQ, "a bare xliff named .xlz");
    refused(b"PK\x03\x04garbage", "a broken zip");
    refused(&zip_of(&[("readme.txt", b"hi", true)], b""), "a zip with no XLIFF");
    // duplicate names, found in the central directory
    let dup = rename_all(zip_of(&[("a.xlf", WS, true), ("a.xlg", WS, true)], b""), b"a.xlg", b"a.xlf");
    let e = refused(&dup, "duplicate member names");
    assert!(e.to_string().contains("duplicate"), "{e}");
    // case-only duplicate and backslash twin
    refused(&rename_all(zip_of(&[("a.xlf", WS, true), ("a.xlg", WS, true)], b""), b"a.xlg", b"A.XLF"), "case duplicate");
    refused(&rename_all(zip_of(&[("d/a.xlf", WS, true), ("d_a.xlf", WS, true)], b""), b"d_a.xlf", b"d\\a.xlf"), "backslash twin");
    // compression bomb
    let zeros = vec![0u8; 8 * 1024 * 1024];
    let bomb = zip_of(&[("content.xlf", WS, true), ("pad.bin", &zeros, true)], b"");
    match refused(&bomb, "a compression bomb") {
        Error::Limit(l) => assert_eq!(l.kind, LimitKind::ZipRatio),
        e => panic!("{e}"),
    }
    // encrypted flag (central directory and local header)
    let mut enc = xlz();
    for i in 0..enc.len() - 8 {
        if enc[i..i + 4] == [0x50, 0x4b, 3, 4] {
            enc[i + 6] |= 1;
        }
        if enc[i..i + 4] == [0x50, 0x4b, 1, 2] {
            enc[i + 8] |= 1;
        }
    }
    refused(&enc, "an encrypted member");
    // entity declaration inside (the XML guard)
    let doctype = text(WS).replacen("<xliff ", "<!DOCTYPE x [<!ENTITY a \"b\">]><xliff ", 1);
    match refused(&zip_of(&[("content.xlf", doctype.as_bytes(), true)], b""), "an entity declaration") {
        Error::Limit(l) => assert_eq!(l.kind, LimitKind::XmlEntities),
        e => panic!("{e}"),
    }
    // legacy encoding declared
    let legacy = text(WS).replacen("encoding=\"UTF-8\"", "encoding=\"ISO-8859-1\"", 1);
    refused(&zip_of(&[("content.xlf", legacy.as_bytes(), true)], b""), "a legacy encoding");
    // segment cap
    let o = ExtractOptions { limits: Some(Limits { max_units: 2, ..Default::default() }), ..Default::default() };
    match filter().extract(&xlz(), &o) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::Units),
        r => panic!("{r:?}"),
    }
    // a traversal-style member name is refused (LocOS carried it; nothing is ever extracted either way)
    refused(&zip_of(&[("../../evil.xlf", WS, true)], b""), "a path escape");
    // XLIFF 2 inside is refused, as LocOS
    let x2 = "<?xml version=\"1.0\"?><xliff version=\"2.0\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" srcLang=\"en\" trgLang=\"fr\"><file id=\"f\"><unit id=\"1\"><segment><source>Hi</source></segment></unit></file></xliff>";
    refused(&zip_of(&[("a.xlf", x2.as_bytes(), true)], b""), "xliff 2");
}

#[test]
fn too_many_xliff_members() {
    let names: Vec<String> = (0..201).map(|i| format!("m{i}.xlf")).collect();
    let tiny = "<?xml version=\"1.0\"?><xliff version=\"1.2\"><file source-language=\"en\"><body><trans-unit id=\"1\"><source>Hi</source></trans-unit></body></file></xliff>";
    let m: Vec<(&str, &[u8], bool)> = names.iter().map(|n| (n.as_str(), tiny.as_bytes(), true)).collect();
    match refused(&zip_of(&m, b""), "201 xliff members") {
        Error::Limit(l) => assert_eq!(l.kind, LimitKind::ZipEntries),
        e => panic!("{e}"),
    }
    assert!(filter().extract(&zip_of(&m[..200], b""), &ExtractOptions::default()).is_ok());
}

#[test]
fn export_end_to_end_partial_translation_keeps_the_package() {
    let f = filter();
    let pkg = xlz();
    let mut d = extract_to(&*f, &pkg, "fr-FR");
    assert_eq!(locos(&d.units[0].segments[0].source), "Open the \u{27E6}g1\u{27E7}Settings\u{27E6}/g1\u{27E7} page.");
    translate(&mut d, "content.xlf|1", |t| t.replace("Open the ", "Ouvrez la page ").replace(" page.", "."));
    let out = f.merge(&d).unwrap();
    let inner = text(&member(&out, "content.xlf"));
    assert!(tu(&inner, "1").contains(">Ouvrez la page <g id=\"1\">Settings</g>.</target>"));
    for id in ["2", "3", "4", "5"] {
        assert_eq!(tu(&inner, id), tu(&text(WS), id), "untranslated stay empty, never source");
    }
    assert_eq!(member(&out, "content.skl"), b"<skl>{1}</skl>");
}

fn flat(s: &str) -> String {
    // bpt/ept pairs are g pairs here and x x in LocOS: compare with every token as one marker
    let mut out = String::new();
    let mut in_tok = false;
    for c in s.chars() {
        match c {
            '\u{27E6}' => {
                in_tok = true;
                out.push('\u{B7}');
            }
            '\u{27E7}' => in_tok = false,
            _ if !in_tok => out.push(c),
            _ => {}
        }
    }
    out.replace('\u{FFFD}', "")
}

#[test]
fn locos_oracle_golden() {
    let gold: serde_json::Value = serde_json::from_str(include_str!("data/locos_golden.json")).unwrap();
    let cases: Vec<(&str, Box<dyn bilingua_model::Filter>, Vec<u8>)> = vec![
        ("locos_sample.mqxlz", bilingua_xliffpkg::mqxlz(), include_bytes!("data/locos_sample.mqxlz").to_vec()),
        ("locos_content.xlz", filter(), xlz()),
        ("locos_multi.wsxz", filter(), wsxz()),
        ("lumen_languages.wsxz", filter(), include_bytes!("../../oracle/corpus/xliffpkg/lumen_languages.wsxz").to_vec()),
        ("lumen_utf16.xlz", filter(), include_bytes!("../../oracle/corpus/xliffpkg/lumen_utf16.xlz").to_vec()),
        ("lumen_bom.xlz", filter(), include_bytes!("../../oracle/corpus/xliffpkg/lumen_bom.xlz").to_vec()),
        // Okapi's own WorldServer package test resource (okapi/filters/wsxzpackage, Apache-2.0, unmodified)
        ("okapi_test1.wsxz", filter(), include_bytes!("data/okapi_test1.wsxz").to_vec()),
    ];
    for (name, f, pkg) in cases {
        let d = extract(&*f, &pkg);
        // LocOS lists a segment when it is not protected: not locked and not yet a finished translation
        let ours: Vec<String> = d
            .segments()
            .filter(|(u, s)| u.translate && matches!(s.state, SegState::New | SegState::Draft))
            .map(|(_, s)| flat(&locos(&s.source)).trim().to_string())
            .collect();
        // Known difference: LocOS lists a segment that holds only a placeholder (its text is a single token);
        // bilingua-xliff reads no segment there, since there is nothing to translate.
        let theirs: Vec<String> = gold[name].as_array().unwrap().iter().map(|v| flat(v.as_str().unwrap())).filter(|t| t != "\u{B7}").collect();
        assert_eq!(ours, theirs, "{name}");
    }
}
