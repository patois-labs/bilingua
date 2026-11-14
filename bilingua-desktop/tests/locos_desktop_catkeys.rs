//! The Localization OS suite `tests/verify_desktop_catkeys_filter.py`, ported.
//!
//! The suite has 14 test functions. Ported here (12 of 14, plus the two `.desktop` and `.catkeys` rows of
//! `tests/verify_keys_bridge.py`): everything that runs on the filters. Not portable: `test_registration` (the
//! LocOS upload form, Microsoft Graph and MIME tables; the facade contract test covers registration, and the
//! ids and extensions are checked below) and `test_api_and_cli_round_trip` (the LocOS web API and CLI).
//!
//! Fixtures: `tests/data/locos_example.desktop` and `.catkeys` are the suite's `DESKTOP` and `CATKEYS` strings
//! byte for byte. Where the Python passes a `{index: text}` map to `build`, the Rust sets the target of that unit
//! and merges; `build_localized(data, map, "fr-FR")` is the same with `Document::target_lang` set.

mod common;
use bilingua_desktop::*;
use bilingua_model::*;
use common::*;

const DESKTOP: &str = include_str!("data/locos_example.desktop");
const CATKEYS: &str = include_str!("data/locos_example.catkeys");

fn refused_msg(r: Result<Document, Error>) -> String {
    r.expect_err("expected a refusal").to_string()
}

#[test]
fn test_registration() {
    assert_eq!(DesktopFilter.id(), "desktop");
    assert_eq!(DesktopFilter.extensions(), ["desktop"]);
    assert_eq!(CatkeysFilter.id(), "catkeys");
    assert_eq!(CatkeysFilter.extensions(), ["catkeys"]);
}

// ---------------------------------------------------------------- .desktop

#[test]
fn test_desktop_units_and_escapes() {
    let data = DESKTOP.as_bytes();
    let t = dtexts(data);
    let want = ["Text Editor", "Editor", "Edit text files;\nquickly\\safely", "text", "editor", "plain;simple", "New Window"];
    assert_eq!(t, want);
    assert_eq!(merge(&desktop(data).unwrap()), data);
    // every unit given back as it is: identical
    let mut d = desktop(data).unwrap();
    for (i, x) in t.iter().enumerate() {
        set(&mut d, i, x);
    }
    assert_eq!(merge(&d), data);
}

#[test]
fn test_desktop_plain_build() {
    let data = DESKTOP.as_bytes();
    let out = s(&build_d(data, &[(2, " Texte;\tmodifier "), (5, "brut;simple")], None));
    assert!(out.contains("Comment=\\sTexte;\\tmodifier\\s\n"), "{out}");
    assert!(out.contains("Keywords=text;editor;brut\\;simple;\n"), "{out}");
    assert!(out.contains("Name[de]=Texteditor\n") && out.contains("Exec=example-editor %F\n"));
    assert_eq!(dtexts(out.as_bytes())[2], " Texte;\tmodifier ");
}

#[test]
fn test_desktop_localized_build() {
    let data = DESKTOP.as_bytes();
    let tr = [(0, "Éditeur de texte"), (1, "Éditeur"), (2, "Modifier;\nvite"), (3, "texte"), (4, "éditeur"), (6, "Nouvelle fenêtre")];
    let out = s(&build_d(data, &tr, Some("fr-FR")));
    let exp = DESKTOP
        .replace("Name=Text Editor\n", "Name=Text Editor\nName[fr_FR]=Éditeur de texte\n")
        .replace("GenericName = Editor\n", "GenericName = Editor\nGenericName[fr_FR]=Éditeur\n")
        .replace("quickly\\\\safely\n", "quickly\\\\safely\nComment[fr_FR]=Modifier;\\nvite\n")
        .replace(
            "Keywords=text;editor;plain\\;simple;\n",
            "Keywords=text;editor;plain\\;simple;\nKeywords[fr_FR]=texte;éditeur;plain\\;simple;\n",
        )
        .replace("Name=New Window\n", "Name=New Window\nName[fr_FR]=Nouvelle fenêtre\n");
    assert_eq!(out, exp);
    // the vendor group is untouched; source lines kept
    assert!(out.contains("Name=Not a launcher string\n") && !out.contains("Name[fr_FR]=Not"));
    // an existing locale line is replaced in place, other locales stay
    let out_de = s(&build_d(data, &[(0, "Text-Editor")], Some("de")));
    assert_eq!(out_de, DESKTOP.replace("Name[de]=Texteditor", "Name[de]=Text-Editor"));
    // re-export over an already-localized file replaces, never duplicates
    let again = s(&build_d(out.as_bytes(), &[(0, "Éditeur")], Some("fr-FR")));
    assert_eq!(again.matches("\nName[fr_FR]=").count(), 2, "{again}");
    assert!(again.contains("Name=Text Editor\nName[fr_FR]=Éditeur\n"), "{again}");
    // localized lines are never units
    assert_eq!(dtexts(out.as_bytes()), dtexts(data));
    // no translations -> identical bytes
    assert_eq!(build_d(data, &[], Some("fr-FR")), data);
}

#[test]
fn test_desktop_line_endings_and_bom() {
    let crlf = DESKTOP.replace('\n', "\r\n").into_bytes();
    assert_eq!(dtexts(&crlf)[0], "Text Editor");
    assert_eq!(merge(&desktop(&crlf).unwrap()), crlf);
    let out = build_d(&crlf, &[(0, "Editor de texto")], Some("es-419"));
    let hay = String::from_utf8_lossy(&out).into_owned();
    assert!(hay.contains("Name=Text Editor\r\nName[es_419]=Editor de texto\r\n"), "{hay}");
    // last line without a line break
    let tail = b"[Desktop Entry]\nType=Application\nName=Clock";
    let out = build_d(tail, &[(0, "Uhr")], Some("de-DE"));
    assert_eq!(out, b"[Desktop Entry]\nType=Application\nName=Clock\nName[de_DE]=Uhr");
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(DESKTOP.as_bytes());
    assert_eq!(merge(&desktop(&bom).unwrap()), bom);
    assert!(build_d(&bom, &[(0, "X")], None).starts_with(b"\xef\xbb\xbf# Launcher"));
}

#[test]
fn test_posix_locale() {
    let cases = [
        ("fr-FR", "fr_FR"),
        ("de", "de"),
        ("sr-Latn", "sr@latin"),
        ("sr-Latn-RS", "sr_RS@latin"),
        ("sr-Cyrl", "sr@cyrillic"),
        ("zh-Hans", "zh_CN"),
        ("zh-Hant", "zh_TW"),
        ("zh-Hant-HK", "zh_HK"),
        ("pt_BR", "pt_BR"),
        ("ca-ES-valencia", "ca_ES@valencia"),
        ("es-419", "es_419"),
    ];
    for (src, want) in cases {
        assert_eq!(posix_locale(src), want, "{src}");
    }
}

#[test]
fn test_desktop_records() {
    let recs = desktop_records(DESKTOP.as_bytes()).unwrap();
    let keys: Vec<&str> = recs.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "Desktop Entry/Name",
            "Desktop Entry/GenericName",
            "Desktop Entry/Comment",
            "Desktop Entry/Keywords[0]",
            "Desktop Entry/Keywords[1]",
            "Desktop Entry/Keywords[2]",
            "Desktop Action new-window/Name"
        ]
    );
    assert_eq!(recs[0].comment, "Shown in menus");
    assert_eq!(recs[0].context, "Desktop Entry");
    assert_eq!(recs.iter().map(|r| r.unit_index).collect::<Vec<_>>(), (0..7).map(Some).collect::<Vec<_>>());
    assert_eq!(recs[5].value, "plain;simple");
}

#[test]
fn test_desktop_hostile() {
    assert!(refused_msg(desktop(b"Name=No group\n")).contains("Desktop Entry"));
    assert!(matches!(desktop(b"[Desktop Entry]\nName=Caf\xe9\n"), Err(Error::Encoding(m)) if m.contains("UTF-8")));
    assert!(matches!(desktop(b"[Desktop Entry]\nName=a\x00b\n"), Err(Error::Encoding(_))));
    // LocOS refuses a file without a translatable value; Bilingua returns a document with no units
    assert!(desktop(b"[Desktop Entry]\nType=Application\nExec=x\n").unwrap().units.is_empty());
    let low = |l: Limits| ExtractOptions::new().with_limits(l);
    let many = format!("[Desktop Entry]\nKeywords={}\n", (0..10).map(|i| format!("k{i}")).collect::<Vec<_>>().join(";"));
    let e = DesktopFilter.extract(many.as_bytes(), &low(Limits::new().with_max_units(3))).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Units), "{e}");
    let mut lines = b"[Desktop Entry]\n".to_vec();
    lines.extend(b"\n".repeat(100));
    let e = DesktopFilter.extract(&lines, &low(Limits::new().with_max_lines(60))).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Lines), "{e}");
    let mut long = b"[Desktop Entry]\nComment=".to_vec();
    long.extend(b"w ".repeat(200));
    long.push(b'\n');
    let e = DesktopFilter.extract(&long, &low(Limits::new().with_max_unit_chars(100))).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::UnitChars), "{e}");
    // a stray line without '=' and odd keys are kept byte for byte
    let odd = b"[Desktop Entry]\nName=Ok\ngarbage line\nName[x y]=z\n";
    assert_eq!(build_d(odd, &[(0, "Gut")], None), b"[Desktop Entry]\nName=Gut\ngarbage line\nName[x y]=z\n");
}

// ---------------------------------------------------------------- .catkeys

#[test]
fn test_catkeys_units_and_build() {
    let data = CATKEYS.as_bytes();
    let t = ctexts(data);
    assert_eq!(t, ["Open", "Save %s?", "Line one\nLine \"two\"", "Quit"]);
    assert_eq!(merge(&catkeys(data).unwrap()), data);
    // current translations given back -> identical
    let same = [(0, "Open"), (1, "Save %s?"), (2, "Line one\nLine \"two\""), (3, "Beenden")];
    assert_eq!(build_c(data, &same, None), data);
    let tr = [(0, "Öffnen"), (1, "%s speichern?"), (2, "Zeile eins\nZeile \"zwei\"\tTab"), (3, "Beenden")];
    let out = s(&build_c(data, &tr, None));
    assert!(out.contains("Open\tMainWindow\t\tÖffnen\n"));
    assert!(out.contains("Save %s?\tMainWindow\tfile name\t%s speichern?\n"));
    assert!(out.contains("Line one\\nLine \"two\"\tAbout\t\tZeile eins\\nZeile \\\"zwei\\\"\\tTab\n"), "{out}");
    assert!(out.starts_with("1\tenglish\tx-vnd.Example-App\t123456789\n")); // build(): header kept
    assert!(out.contains("(\tCalcView\t\t(\n") && out.contains("Quit\tMainWindow\t\tBeenden\n"));
    assert_eq!(ctexts(out.as_bytes()), t); // source column never changes
}

#[test]
fn test_catkeys_localized_header() {
    let data = CATKEYS.as_bytes();
    let out = s(&build_c(data, &[(0, "Öffnen")], Some("de-DE")));
    assert!(out.starts_with("1\tgerman (germany)\tx-vnd.Example-App\t123456789\n"), "{}", &out[..80]);
    // Localization OS also stamps the language when nothing is translated; Bilingua returns the original bytes
    // (lead decision 11-04, the "stamp anyway" merge option is coming in the model)
    assert_eq!(build_c(data, &[], Some("pt-BR")), data);
    let out = s(&build_c(data, &[(0, "Abrir")], Some("pt-BR")));
    assert!(out.starts_with("1\tportuguese (brazil)\t"), "{}", &out[..60]);
    assert_eq!(language_name("de"), "german");
    assert_eq!(language_name("xx-!!"), "xx-!!");
    // already english: nothing to change
    assert_eq!(build_c(data, &[(0, "Open")], Some("en")), data);
}

#[test]
fn test_catkeys_fingerprint() {
    // Worked example of Haiku HashFun (h = 5h + signed byte, then 5h + 1), chained over source, context, comment:
    // "A" -> 5*0+65=65 -> 326; context "" -> 5*326+1 = 1631; comment "" -> 5*1631+1 = 8156.
    assert_eq!(catkeys_fingerprint(b"1\tenglish\tx-vnd.T\t8156\nA\t\t\tA\n").unwrap(), (8156, 8156));
    // a non-ASCII byte counts as a SIGNED char: e acute is 0xC3 0xA9 -> 5*0-61, 5*(-61)-87, then 5h+1 and two
    // more empty fields
    let mut h: u32 = (-1959i32) as u32;
    h = h.wrapping_mul(5).wrapping_add(1);
    h = h.wrapping_mul(5).wrapping_add(1);
    let two = "1\tenglish\tx\t0\n\u{e9}\t\t\tx\nA\t\t\tA\n".as_bytes();
    assert_eq!(catkeys_fingerprint(two).unwrap().1, h.wrapping_add(8156));
    // escapes are hashed unescaped (as Haiku's catalog does): "\n" in the file is a newline
    let raw_nl = (((65u32 * 5 + 10) * 5 + 1) * 5 + 1) * 5 + 1;
    assert_eq!(catkeys_fingerprint(b"1\tenglish\tx\t0\nA\\n\t\t\t\n").unwrap().1, raw_nl);
}

#[test]
fn test_catkeys_records() {
    let recs = catkeys_records(CATKEYS.as_bytes()).unwrap();
    assert_eq!(recs.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["Open", "Save %s?", "(", "Line one\nLine \"two\"", "Quit"]);
    assert_eq!(recs[1].comment, "file name");
    assert_eq!(recs[1].context, "MainWindow");
    assert_eq!(recs[4].value, "Beenden");
    assert_eq!(recs.iter().map(|r| r.unit_index).collect::<Vec<_>>(), [Some(0), Some(1), None, Some(2), Some(3)]);
    let dup = format!("{CATKEYS}Open\tMainWindow\t\tAuf\nOpen\tOther\t\tAuf\n");
    let flags: Vec<bool> = catkeys_records(dup.as_bytes()).unwrap().iter().map(|r| r.duplicate).collect();
    assert_eq!(flags[flags.len() - 2..], [true, false]);
}

#[test]
fn test_catkeys_crlf_bom_and_hostile() {
    let crlf = CATKEYS.replace('\n', "\r\n").into_bytes();
    assert_eq!(ctexts(&crlf)[0], "Open");
    assert_eq!(merge(&catkeys(&crlf).unwrap()), crlf);
    let out = build_c(&crlf, &[(0, "Ouvrir")], None);
    assert!(String::from_utf8_lossy(&out).contains("Open\tMainWindow\t\tOuvrir\r\n"));
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend_from_slice(CATKEYS.as_bytes());
    assert!(build_c(&bom, &[(0, "Ouvrir")], None).starts_with(b"\xef\xbb\xbf1\t"));
    assert!(refused_msg(catkeys(b"2\tenglish\tx\t1\nA\t\t\tA\n")).contains("first line"));
    assert!(refused_msg(catkeys(b"1\tenglish\tx\tabc\nA\t\t\tA\n")).contains("fingerprint"));
    assert!(refused_msg(catkeys(b"1\tenglish\tx\t1\nA\tonly two\n")).contains("four"));
    assert!(matches!(catkeys(b"1\tenglish\tx\t1\nCaf\xe9\t\t\t\n"), Err(Error::Encoding(m)) if m.contains("UTF-8")));
    assert!(refused_msg(catkeys(b"")).contains("empty"));
    // a catalog whose only entry has no letter: LocOS refuses, Bilingua returns a document with no units
    assert!(catkeys(b"1\tenglish\tx\t1\n(\t\t\t(\n").unwrap().units.is_empty());
    let many: String = std::iter::once("1\tenglish\tx\t1\n".to_string()).chain((0..10).map(|i| format!("S{i}\t\t\t\n"))).collect();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(3));
    assert!(matches!(CatkeysFilter.extract(many.as_bytes(), &o), Err(Error::Limit(l)) if l.kind == LimitKind::Units));
}

/// The two rows of `tests/verify_keys_bridge.py`: a source export is byte identical.
#[test]
fn test_keys_bridge_rows() {
    let d = include_bytes!("data/bridge_app.desktop");
    let c = include_bytes!("data/bridge_en.catkeys");
    assert_eq!(dtexts(d), ["Notes", "Take notes"]);
    assert_eq!(merge(&desktop(d).unwrap()), d);
    assert_eq!(desktop_records(d).unwrap().len(), 2);
    assert_eq!(ctexts(c), ["Open"]);
    assert_eq!(merge(&catkeys(c).unwrap()), c);
    assert_eq!(catkeys_records(c).unwrap().len(), 1);
}
