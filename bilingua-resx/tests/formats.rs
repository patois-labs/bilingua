//! Corpus files against the LocOS filter's units (golden list made with the LocOS venv), Okapi test resources,
//! the 11 languages, encodings, escapes and merge behaviour.

mod common;

use bilingua_model::*;
use bilingua_resx::Resx;
use common::*;
use std::collections::BTreeMap;

fn corpus(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/resx").join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn golden() -> BTreeMap<String, Vec<String>> {
    serde_json::from_str(include_str!("data/locos_golden.json")).unwrap()
}

fn all_files() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/resx"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| !["shiftjis", "gb2312", "euckr"].iter().any(|x| n.contains(x))) // need the B6 encodings
        .map(|n| (n.clone(), corpus(&n)))
        .collect();
    v.push(("okapi_Test01.resx".into(), include_bytes!("data/okapi_Test01.resx").to_vec()));
    v.push(("okapi_test_with_placeholders.resx".into(), include_bytes!("data/okapi_test_with_placeholders.resx").to_vec()));
    v.sort();
    v
}

#[test]
fn units_equal_the_locos_filter_on_every_file() {
    let g = golden();
    for (name, data) in all_files() {
        let want = g.get(&name).unwrap_or_else(|| panic!("no golden for {name}"));
        assert_eq!(&units(&data), want, "{name}");
    }
}

#[test]
fn every_file_round_trips_untouched_and_pseudo_translated() {
    for (name, data) in all_files() {
        let d = extract(&data);
        assert_eq!(Resx.merge(&d).unwrap(), data, "{name}: untouched merge");
        let echo: Vec<(usize, String)> = d.units.iter().enumerate().map(|(i, u)| (i, show(&u.source()))).collect();
        let m: Vec<(usize, &str)> = echo.iter().map(|(i, s)| (*i, s.as_str())).collect();
        assert_eq!(translate(&data, &m), data, "{name}: echo merge");
        let out = Resx.merge(&pseudo(&d)).unwrap();
        let back = extract(&out);
        assert_eq!(back.units.len(), d.units.len(), "{name}: unit count");
        for (a, b) in d.units.iter().zip(&back.units) {
            assert_eq!(show(&b.source()), format!("[{}]", show(&a.source())).trim(), "{name}");
            let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            assert_eq!(tags(a), tags(b), "{name}: tags");
        }
    }
}

#[test]
fn the_eleven_languages() {
    let data = corpus("languages_utf8.resx");
    let u = units(&data);
    assert_eq!(u.len(), 11);
    for (i, code) in ["en", "de", "fr", "es", "ru", "ar", "ja", "zh", "ko", "hi", "tr"].iter().enumerate() {
        let d = extract(&data);
        assert_eq!(d.units[i].context.key.as_deref(), Some(format!("Welcome.{code}").as_str()));
        assert_eq!(d.units[i].context.notes, vec![format!("Greeting, {code}")]);
        assert!(u[i].contains("{0}"));
    }
    // translate every language into every other one and read it back
    let mapping: Vec<(usize, String)> = (0..11).map(|i| (i, u[(i + 5) % 11].clone())).collect();
    let m: Vec<(usize, &str)> = mapping.iter().map(|(i, s)| (*i, s.as_str())).collect();
    let out = translate(&data, &m);
    let back = units(&out);
    for i in 0..11 {
        assert_eq!(back[i], u[(i + 5) % 11]);
    }
    // the same inside the other envelopes
    for n in ["languages_utf8_bom_crlf.resw", "languages_utf16le.resx", "languages_utf16be.resx"] {
        let data = corpus(n);
        let out = translate(&data, &m);
        assert_eq!(units(&out), back, "{n}");
        assert_eq!(out[..2] == data[..2], true, "{n}: same BOM");
    }
}

#[test]
fn legacy_encodings_are_kept() {
    for n in ["languages_cp1252.resx", "languages_latin1.resx"] {
        let data = corpus(n);
        assert!(std::str::from_utf8(&data).is_err());
        let d = extract(&data);
        assert_eq!(d.units.len(), 4);
        assert!(show(&d.units[2].source()).starts_with("Bienvenue à "));
        let out = translate(&data, &[(1, "Grüße aus Köln, {0}")]);
        assert!(std::str::from_utf8(&out).is_err(), "{n}: stays single byte");
        assert!(out.windows(11).any(|w| w == b"Gr\xfc\xdfe aus K"), "{n}");
        assert_eq!(units(&out)[1], "Grüße aus Köln, {0}");
        // a character the code page cannot hold: UTF-8 with a truthful declaration
        let out = translate(&data, &[(1, "Greek: αβγ")]);
        let t = std::str::from_utf8(&out).unwrap();
        assert!(t.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{n}: {}", &t[..60]);
        assert_eq!(units(&out)[1], "Greek: αβγ");
    }
}

#[test]
fn utf8_file_declaring_a_legacy_charset_is_declared_utf8_once_text_needs_it() {
    let data = b"<?xml version=\"1.0\" encoding=\"iso-8859-1\"?>\n<root><data name=\"a\"><value>Hello there</value></data></root>\n";
    assert_identity(data);
    let out = translate(data, &[(0, "Gr\u{fc}\u{df}e")]);
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    let out = translate(data, &[(0, "Hallo dort")]);
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"iso-8859-1\"?>"), "ASCII keeps its declaration");
}

#[test]
fn okapi_resources() {
    let t1 = include_bytes!("data/okapi_Test01.resx");
    let d = extract(t1);
    assert_eq!(d.units.len(), 22);
    assert_eq!(d.units[0].context.key.as_deref(), Some("toolStripMenuItem1.Text"));
    assert_eq!(show(&d.units[0].source()), "&File");
    assert_eq!(show(&d.units[21].source()), "Gandalf Monitoring System");
    // placeholders: Okapi codes {1} and {dbName}; LocOS keeps them as text. Bilingua tags them (flattening gives
    // the LocOS text); the escaped HTML in the value stays text (an Okapi difference).
    let d = extract(include_bytes!("data/okapi_test_with_placeholders.resx"));
    assert_eq!(d.units.len(), 1);
    let tags: Vec<String> = d.units[0].source().iter().filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None }).collect();
    assert_eq!(tags, ["{1}", "{dbName}"]);
    assert_eq!(show(&d.units[0].source()), "<b>File</b> was in the {1} <a href=\"test\">database</a> {dbName} but not on the disk.");
}

#[test]
fn placeholder_forms() {
    let data = corpus("lumen_ui.resx");
    let d = extract(&data);
    let tags = |i: usize| -> Vec<(String, bool)> {
        d.units[i].source().iter().filter_map(|t| if let Inline::Tag { display, free, .. } = t { Some((display.clone(), *free)) } else { None }).collect()
    };
    assert_eq!(tags(1), [("{0}".into(), true), ("{1:HH:mm}".into(), true), ("{2:D}".into(), true)]);
    assert_eq!(tags(2), [("{0:C2}".into(), true), ("{1}".into(), true)], "{{ }} are text");
    // a translation may move free tags
    let src = show(&d.units[1].source());
    assert!(src.contains("#{0}"));
    let out = translate(&data, &[(1, "Am {2:D} um {1:HH:mm} ist Ihre Bestellung #{0} bereit.")]);
    assert_eq!(units(&out)[1], "Am {2:D} um {1:HH:mm} ist Ihre Bestellung #{0} bereit.");
}

#[test]
fn escapes_and_whitespace_on_merge() {
    let data = corpus("lumen_ui.resx");
    let d = extract(&data);
    let idx = |k: &str| d.units.iter().position(|u| u.context.key.as_deref() == Some(k)).unwrap();
    // untouched values keep entities, CDATA and multi-line text
    assert_eq!(Resx.merge(&d).unwrap(), data);
    let out = translate(&data, &[(idx("Quote"), "Sie sagte \"frisch\" & <fertig>\nZeile 2"), (idx("Cdata.Block"), "Benutze <b>fett</b> & \"Zitate\""), (idx("Padding"), "Eingerückter Absatz")]);
    let t = std::str::from_utf8(&out).unwrap();
    assert!(t.contains("<value>Sie sagte \"frisch\" &amp; &lt;fertig&gt;\nZeile 2</value>"));
    assert!(t.contains("<value><![CDATA[Benutze <b>fett</b> & \"Zitate\"]]></value>"));
    assert!(t.contains("<value>\n      Eingerückter Absatz\n    </value>"), "lead and trail white space kept");
    // skipped entries are byte for byte
    for keep in ["Icon.Logo", "Readme", "Timeout", ">>cancelButton.Name"] {
        let name = keep.replace(">>", "&gt;&gt;");
        let at = t.find(&format!("<data name=\"{name}\"")).unwrap();
        let s = std::str::from_utf8(&data).unwrap();
        let at0 = s.find(&format!("<data name=\"{name}\"")).unwrap();
        let end = s[at0..].find("</data>").unwrap();
        assert_eq!(&t[at..at + end], &s[at0..at0 + end], "{keep}");
    }
    // a real CR in a translation is a character reference, other control characters are dropped, Word breaks become LF
    let out = translate(&data, &[(idx("Menu.Title"), "A\rB\u{1}C\u{b}D")]);
    let t = std::str::from_utf8(&out).unwrap();
    assert!(t.contains("<value>A&#13;BC\nD</value>"), "{t}");
    assert_eq!(units(&out)[idx("Menu.Title")], "A\rBC\nD".trim());
}

#[test]
fn non_string_entries_comments_and_odd_values_are_left_alone() {
    let data = corpus("lumen_ui.resx");
    let recs = Resx.records(&data).unwrap();
    let by = |k: &str| recs.iter().find(|r| r.key == k).unwrap();
    assert!(!by("Icon.Logo").translatable && by("Icon.Logo").comment == "The application icon");
    assert!(!by("Timeout").translatable && !by("Dashes").translatable);
    assert!(by("Only.Number").translatable && by("Only.Placeholder").translatable, "LocOS: any letter or digit");
    assert!(!by("Child.Element").translatable && !by("Commented.Value").translatable);
    assert!(!by("Empty.Value").translatable && !by("Empty.Pair").translatable);
    assert_eq!(recs.iter().filter(|r| r.key == "Duplicate.Key").count(), 2);
    assert!(recs.iter().all(|r| r.key != "No.Value"), "a data with no value is not an entry");
    assert_eq!(recs.iter().filter(|r| r.translatable).count(), 15);
}

#[test]
fn include_locked_lists_text_values_left_alone() {
    let data = corpus("lumen_ui.resx");
    let o = ExtractOptions { include_locked: true, ..opts() };
    let d = Resx.extract(&data, &o).unwrap();
    let locked: Vec<String> = d.units.iter().filter(|u| !u.translate).map(|u| show(&u.source())).collect();
    assert_eq!(locked, ["---", "cancelButton"]);
    assert!(d.units.iter().all(|u| u.translate || u.lock_reason.is_some()));
    // locked units are never written, even when given a target
    assert_eq!(Resx.merge(&d).unwrap(), data);
}

#[test]
fn target_locale_changes_nothing_in_the_file() {
    // the culture lives in the file name (Strings.de-DE.resx); nothing is stamped
    let data = corpus("lumen_ui.resx");
    let mut d = Resx.extract(&data, &ExtractOptions { target_lang: Some("ar".into()), ..opts() }).unwrap();
    d.target_lang = Some("ja".into());
    assert_eq!(Resx.merge(&d).unwrap(), data);
}

#[test]
fn ids_are_stable_and_locations_are_lines() {
    let data = corpus("lumen_ui.resx");
    let a = extract(&data);
    let b = extract(&data);
    assert_eq!(a.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), b.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>());
    assert_eq!(a.units[0].id, "u1");
    let line = a.units[0].context.location.clone().unwrap();
    let n: usize = line.trim_start_matches("line ").parse().unwrap();
    let text = String::from_utf8(data).unwrap();
    assert!(text.lines().nth(n - 1).unwrap().contains("Menu.Title"));
}

#[test]
fn skeleton_from_an_older_parser_is_refused() {
    let mut d = extract(&corpus("lumen_ui.resx"));
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(Resx.merge(&d).is_err());
}
