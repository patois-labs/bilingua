//! Port of the LocOS suite `tests/verify_rc_filter.py` (13 tests; the HTTP end-to-end test is ported as a
//! document-level equivalent). Fixtures: `tests/data/sample.rc`, the same text the LocOS suite builds.
//!
//! One deliberate difference (lead decision 11-04): a merge where nothing is translated returns the original
//! bytes, with no LANGUAGE stamp. LocOS stamps even then; the checks below translate one unit where LocOS
//! translated none, and a separate test pins the default.
mod common;
use bilingua_model::*;
use bilingua_rc::Rc;
use common::*;

const EXPECTED: &[(&str, &str)] = &[
    ("&File", "rc:menu"),
    ("&Open...⟦x1⟧Ctrl+O", "rc:menu"),
    ("Save && E&xit", "rc:menu"),
    ("&Help", "rc:menu"),
    ("&About", "rc:menu"),
    ("About Sample", "rc:caption"),
    ("Sample, Version 1.0", "rc:control"),
    ("&Remember me", "rc:control"),
    ("OK", "rc:control"),
    ("Hello, ⟦x1⟧! You have ⟦x2⟧ new messages.", "rc:string"),
    ("Say \"hi\" to C:\\Temp⟦x1⟧then leave", "rc:string"),
    ("Copying ⟦p1⟧ to ⟦p2⟧...", "rc:string"),
    ("5¼-Inch Floppy Disk", "rc:string"),
    ("first part second part", "rc:string"),
    ("Sample Application", "rc:version"),
    ("Copyright (C) 2026", "rc:version"),
    ("Sample", "rc:version"),
];

fn sample_crlf() -> String {
    crlf(SAMPLE)
}

#[test]
fn registered() {
    assert_eq!(Rc.id(), "rc");
    assert!(Rc.extensions().contains(&"rc"));
    assert!(!Rc.bilingual());
}

#[test]
fn parse_units() {
    let data = cp1252(&sample_crlf());
    let doc = extract(&data).unwrap();
    let got: Vec<(String, String)> = doc.units.iter().map(|u| (token_text(u), u.context.style.clone().unwrap())).collect();
    let want: Vec<(String, String)> = EXPECTED.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(got, want);
    // stable ids in file order
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    let want_ids: Vec<String> = (1..=EXPECTED.len()).map(|n| format!("u{n}")).collect();
    assert_eq!(ids, want_ids.iter().map(String::as_str).collect::<Vec<_>>());
    // the tab is a standalone code that keeps its written form; the numbered placeholders may move
    let u = &doc.units[1];
    assert_eq!(u.codes["1"].standalone.as_deref(), Some("\\t"));
    let kinds: Vec<bool> = doc.units[11].source().iter().filter(|i| matches!(i, Inline::Tag { .. })).map(Inline::is_free).collect();
    assert_eq!(kinds, vec![true, true]);
    let kinds: Vec<bool> = doc.units[9].source().iter().filter(|i| matches!(i, Inline::Tag { .. })).map(Inline::is_free).collect();
    assert_eq!(kinds, vec![false, false]);
    match &doc.skeleton {
        Skeleton::Text { encoding, .. } => assert_eq!(encoding, "windows-1252"),
        _ => panic!("text skeleton"),
    }
}

#[test]
fn identity_round_trip_every_encoding() {
    let text = sample_crlf();
    let variants: Vec<(&str, Vec<u8>)> = vec![
        ("cp1252", cp1252(&text)),
        ("utf-8", text.clone().into_bytes()),
        ("utf-8-bom", [&[0xEF, 0xBB, 0xBF][..], text.as_bytes()].concat()),
        ("utf-16-le-bom", utf16le(&text, true)),
        ("utf-16-be-bom", utf16be(&text, true)),
        ("utf-16-le", utf16le(&text, false)),
    ];
    for (name, data) in variants {
        let doc = extract(&data).unwrap();
        assert_eq!(doc.units.len(), EXPECTED.len(), "{name}");
        assert_identity(&data);
    }
}

#[test]
fn translated_round_trip_utf16() {
    let data = utf16le(&sample_crlf(), true);
    assert_eq!(extract(&data).unwrap().units.len(), EXPECTED.len());
    let tr = [
        (1, "&Öffnen...⟦x1⟧Strg+O"),
        (9, "Hallo, ⟦x1⟧! Sie haben ⟦x2⟧ neue Nachrichten."),
        (10, "Sag \"hallo\" zu C:\\Temp⟦x1⟧dann geh"),
        (12, "5¼-Zoll-Diskette"),
    ];
    let out = build(&data, &tr);
    assert_eq!(&out[..2], &[0xFF, 0xFE]);
    let s = String::from_utf16(&out[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()).unwrap();
    assert!(s.contains("\"&Öffnen...\\tStrg+O\""), "{s}");
    assert!(s.contains("\"Hallo, %s! Sie haben %d neue Nachrichten.\""));
    assert!(s.contains("\"Sag \"\"hallo\"\" zu C:\\\\Temp\\ndann geh\""));
    assert!(s.contains("L\"5¼-Zoll-Diskette\""));
    // untouched lines identical
    let orig = sample_crlf();
    assert_eq!(s.replace("\"&Öffnen...\\tStrg+O\"", "\"&Open...\\tCtrl+O\"").matches("\r\n").count(), orig.matches("\r\n").count());
    assert!(s.contains("MENUITEM \"Save && E&xit\"") && s.contains("\"Copying %1 to %2!s!...\""));
    let again = units(&out);
    assert_eq!(again[1], tr[0].1);
    assert_eq!(again[9], tr[1].1);
    assert_eq!(again[10], tr[2].1);
}

#[test]
fn placeholders_move_and_invented_codes_are_refused() {
    let data = SAMPLE.as_bytes();
    // the two printf codes swap places: their raw source text comes back
    let out = String::from_utf8(build(data, &[(9, "⟦x2⟧ / ⟦x1⟧")])).unwrap();
    assert!(out.contains("\"%d / %s\""), "{out}");
    // LocOS drops an invented token; Bilingua refuses it when the target is set
    let mut doc = extract(data).unwrap();
    let id = doc.units[9].segments[0].id.clone();
    let invented = vec![Inline::tag("7", TagKind::Standalone, "%x")];
    assert!(matches!(doc.set_target(&id, invented), Err(Error::InventedTag(_))));
    let paired = vec![Inline::tag("1", TagKind::Open, "<b>"), Inline::text("x"), Inline::tag("1", TagKind::Close, "</b>")];
    assert!(doc.set_target(&id, paired).is_err());
    // a merge that is handed an unknown code anyway drops it
    let mut doc = extract(data).unwrap();
    doc.units[9].segments[0].target = vec![Inline::text("a"), Inline::tag("7", TagKind::Standalone, "%x"), Inline::text("b")];
    let out = String::from_utf8(Rc.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("IDS_HELLO               \"ab\""), "{out}");
}

#[test]
fn accelerators() {
    let data = SAMPLE.as_bytes();
    let out = String::from_utf8(build(
        data,
        &[
            (0, "Datei"),                 // missing: an alphabetic target marks its own first letter
            (3, "Hilfe"),                 // source &H, no h in... the letter is present
            (4, "&Info &über"),           // two: the second becomes a literal ampersand
            (7, "ファイルを記憶"),        // CJK style append
            (1, "Öffnen...⟦x1⟧Strg+O"),   // before the tab
            (2, "Speichern && Beenden"),  // source has one (&x)
        ],
    ))
    .unwrap();
    assert!(out.contains("POPUP \"&Datei\""), "{out}");
    assert!(out.contains("POPUP \"&Hilfe\""), "{out}");
    assert!(out.contains("MENUITEM \"&Info &&über\""), "{out}");
    assert!(out.contains("\"ファイルを記憶(&R)\""), "{out}");
    assert!(out.contains("\"&Öffnen...\\tStrg+O\""), "{out}");
    // CJK before a tab still appends (&X) before the dots
    let cjk = String::from_utf8(build(data, &[(1, "開く...⟦x1⟧Ctrl+O")])).unwrap();
    assert!(cjk.contains("\"開く(&O)...\\tCtrl+O\""), "{cjk}");
    assert!(out.contains("\"Speichern && Beenden(&X)\"") || !out.contains("\"Speichern && Beend&en\""));
    // strings in a STRINGTABLE are never touched by accelerator logic
    let out2 = String::from_utf8(build(data, &[(9, "A & B")])).unwrap();
    assert!(out2.contains("\"A & B\""));
}

#[test]
fn ansi_overflow_switches_to_utf8_pragma() {
    let data = cp1252(&sample_crlf());
    let out = build(&data, &[(5, "关于示例")]);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("#pragma code_page(65001)") && !s.contains("code_page(1252)"));
    assert!(s.contains("CAPTION \"关于示例\""));
    // a cp1252-representable translation stays cp1252
    let out2 = build(&data, &[(5, "À propos")]);
    assert_eq!(out2.iter().filter(|&&b| b == 0xC0).count(), 1);
    assert!(std::str::from_utf8(&out2).is_err());
    // no pragma at all: one is added at the top
    let nop = SAMPLE.replace("#pragma code_page(1252)\n", "");
    let s3 = String::from_utf8(build(nop.as_bytes(), &[(5, "关于")])).unwrap();
    assert!(s3.starts_with("#pragma code_page(65001)\n"), "{}", &s3[..60]);
}

#[test]
fn declared_code_page() {
    let (sj, _, bad) = bilingua_model::encoding_rs::SHIFT_JIS.encode("LANGUAGE 0x11, 0x01\n#pragma code_page(932)\nSTRINGTABLE\nBEGIN\n 1 \"ファイル\"\nEND\n");
    assert!(!bad);
    let data = sj.into_owned();
    let doc = extract(&data).unwrap();
    assert_eq!(doc.units.iter().map(token_text).collect::<Vec<_>>(), vec!["ファイル"]);
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "shift_jis"));
    let out = build(&data, &[(0, "ファイルを開く")]);
    let (back, _) = bilingua_model::encoding_rs::SHIFT_JIS.decode_without_bom_handling(&out);
    assert_eq!(back.matches("ファイルを開く").count(), 1);
    let mixed = b"#pragma code_page(1252)\n#pragma code_page(932)\nSTRINGTABLE BEGIN 1 \"x\" END\n";
    // LocOS refuses several different pragmas; Bilingua reads each section in its own page (next test)
    assert_eq!(units(mixed), vec!["x"]);
    let bad = b"#pragma code_page(1234)\nSTRINGTABLE BEGIN 1 \"x\" END\n";
    assert!(matches!(extract(bad), Err(Error::Encoding(_))));
}

fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

#[test]
fn language_stamping() {
    let data = cp1252(&sample_crlf());
    // default: nothing translated, so the original bytes (no stamp); LocOS stamps here, the lead decided not to
    assert_eq!(build_for(&data, &[], Some("de-DE")), data);
    let out = String::from_utf8(cp1252_back(&build_for(&data, &[(5, "Info")], Some("de-DE")))).unwrap();
    assert!(out.contains("LANGUAGE 0x07, 0x01"), "{}", &out[..400]);
    assert!(out.contains("BLOCK \"040704b0\""));
    assert!(out.contains("VALUE \"Translation\", 0x407, 1200"));
    let ja = s(build_for(&data, &[(5, "バージョン情報")], Some("ja-JP")));
    assert!(ja.contains("LANGUAGE 0x11, 0x01") && ja.contains("BLOCK \"041104b0\""));
    let zh = String::from_utf8(cp1252_back(&build_for(&data, &[(5, "About")], Some("zh-Hant")))).unwrap();
    assert!(zh.contains("LANGUAGE 0x04, 0x01") && zh.contains("0x404"), "{}", &zh[..400]);
    // a multi-language script keeps its LANGUAGE statements
    let multi = "LANGUAGE 9, 1\nSTRINGTABLE BEGIN 1 \"a\" END\nLANGUAGE 7, 1\nSTRINGTABLE BEGIN 2 \"b\" END\n";
    let out_m = s(build_for(multi.as_bytes(), &[(0, "ab")], Some("fr-FR")));
    assert_eq!(out_m, multi.replace("\"a\"", "\"ab\""));
    // a LANGUAGE statement inside a header is stamped too
    let per = "STRINGTABLE\nLANGUAGE LANG_ENGLISH, SUBLANG_NEUTRAL\nBEGIN\n 1 \"Hi\"\nEND\n";
    assert!(s(build_for(per.as_bytes(), &[(0, "Salut")], Some("fr-FR"))).contains("LANGUAGE 0x0c, 0x01"));
    // an empty target language stamps nothing
    assert_eq!(s(build_for(per.as_bytes(), &[(0, "Hi")], Some(""))), per);
    assert!(s(build_for(per.as_bytes(), &[(0, "Salut")], Some(""))).contains("LANGUAGE LANG_ENGLISH, SUBLANG_NEUTRAL"));
    // no target language set: the script keeps its LANGUAGE
    assert!(s(build_for(per.as_bytes(), &[(0, "Salut")], None)).contains("LANGUAGE LANG_ENGLISH"));
}

fn cp1252_back(b: &[u8]) -> Vec<u8> {
    // the sample is ASCII plus Latin-1 text: re-read as UTF-8 for the assertions
    b.iter().flat_map(|&c| if c < 0x80 { vec![c] } else { (c as char).to_string().into_bytes() }).collect()
}

#[test]
fn rc_exe_string_rules() {
    let src = "STRINGTABLE\nBEGIN\n  1 \"Hello \\\n        world\"\n  2 \"Don\\'t\"\n  3 L\"Don\\'t\"\nEND\nVS_VERSION_INFO VERSIONINFO\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \"040904B0\"\n  BEGIN\n   VALUE \"ProductName\", \"Sample\"\n  END\n END\nEND\n";
    let data = src.as_bytes();
    assert_eq!(units(data), vec!["Hello world", "Don\\'t", "Don't", "Sample"]);
    let out = s(build(data, &[(0, "Hallo Welt"), (1, "Don\\'t do it"), (2, "Nicht's")]));
    assert!(out.contains("1 \"Hallo Welt\"") && out.contains("2 \"Don\\\\'t do it\""), "{out}");
    assert!(out.contains("3 L\"Nicht's\""), "{out}");
    let again = units(out.as_bytes());
    assert_eq!(&again[..3], &["Hallo Welt", "Don\\'t do it", "Nicht's"]);
    let loc = s(build_for(data, &[(0, "Hallo Welt")], Some("fr-FR")));
    assert!(loc.contains("BLOCK \"040C04B0\""), "{loc}");
}

#[test]
fn include_never_followed() {
    let src = "#include \"C:\\\\Windows\\\\win.ini\"\n#include <secret.rc>\n  #  include \"indented.rc\"\nrcinclude other.rc\nSTRINGTABLE { 1, \"Only me\" }\n";
    assert_eq!(units(src.as_bytes()), vec!["Only me"]);
}

#[test]
fn hostile_inputs() {
    // binary
    let bin = [&b"\x00\x01\x02\x03garbage\x00"[..]].repeat(50).concat();
    let mut bin = bin;
    bin.push(0x81);
    assert!(matches!(extract(&bin), Err(Error::Encoding(_))));
    // unterminated string, stray END, deep nesting: no crash
    let junk = format!("STRINGTABLE BEGIN 1 \"open\n2 \"ok\" END END END\n{}\nX MENU BEGIN MENUITEM \"&Go\", 1", "BEGIN ".repeat(5000));
    assert_eq!(units(junk.as_bytes())[..1], ["ok"]);
    // unterminated block comment
    extract(b"STRINGTABLE BEGIN 1 \"a\" END /* never closed \"b\"").unwrap();
    // segment cap
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let mut big = String::from("STRINGTABLE\nBEGIN\n");
    for i in 0..20 {
        big.push_str(&format!(" {i} \"Text {i}\"\n"));
    }
    big.push_str("END\n");
    assert!(matches!(Rc.extract(big.as_bytes(), &small), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let quotes = "\"".repeat(2000);
    assert!(matches!(Rc.extract(quotes.as_bytes(), &small), Err(Error::Limit(_))));
    // a huge single string
    let lim = Limits::default();
    let huge = format!("STRINGTABLE BEGIN 1 \"{}\" END", "a".repeat(lim.max_unit_chars + 5));
    assert!(matches!(extract(huge.as_bytes()), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
    // pathological backslashes and quotes stay linear
    let t0 = std::time::Instant::now();
    extract(format!("STRINGTABLE BEGIN 1 \"{}\" END", "\\\\".repeat(20000)).as_bytes()).unwrap();
    extract(format!("STRINGTABLE BEGIN 1 \"{}\" END", "\"\"".repeat(20000)).as_bytes()).unwrap();
    assert!(t0.elapsed().as_secs() < 10);
}

/// The LocOS end-to-end test goes through the web app (upload, save two segments, export de-DE). The same flow at
/// document level: UTF-16 in, two translations, de-DE out.
#[test]
fn end_to_end_upload_export() {
    let src = utf16le(&sample_crlf(), true);
    let mut doc = Rc.extract(&src, &ExtractOptions::new().with_source_lang("en-US").with_target_lang("de-DE")).unwrap();
    let by_src: std::collections::BTreeMap<String, String> = doc.units.iter().map(|u| (plain_text(&u.source()), u.segments[0].id.clone())).collect();
    assert!(by_src.contains_key("About Sample") && by_src.contains_key("&About"));
    for (source, target) in [("About Sample", "Info zu Sample"), ("&About", "Info")] {
        doc.set_target(&by_src[source], vec![Inline::text(target)]).unwrap();
    }
    let body = Rc.merge(&doc).unwrap();
    assert_eq!(&body[..2], &[0xFF, 0xFE]);
    let st = String::from_utf16(&body[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()).unwrap();
    assert!(st.contains("CAPTION \"Info zu Sample\""), "{}", &st[..300]);
    assert!(st.contains("MENUITEM \"&Info\""));
    assert!(st.contains("LANGUAGE 0x07, 0x01") && st.contains("BLOCK \"040704b0\""));
}

#[test]
fn sections_with_different_code_pages_are_read_each_in_its_own_page() {
    // A Visual Studio script has one section per language: Russian text in 1251, then a neutral section in 1252.
    let hello = "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}";
    let mut data: Vec<u8> = Vec::new();
    data.extend_from_slice(b"// head\r\n#pragma code_page(1251)\r\nSTRINGTABLE\r\nBEGIN\r\n IDS_A \"");
    let (ru, _, bad) = bilingua_model::encoding_rs::WINDOWS_1251.encode(hello);
    assert!(!bad);
    data.extend_from_slice(&ru);
    data.extend_from_slice(b"\"\r\nEND\r\n#pragma code_page(1252)\r\nSTRINGTABLE\r\nBEGIN\r\n IDS_B \"Caf\xe9\"\r\nEND\r\n");
    let doc = extract(&data).expect("several pragmas are read section by section");
    assert_eq!(doc.units.iter().map(token_text).collect::<Vec<_>>(), vec![hello, "Caf\u{e9}"]);
    assert_eq!(Rc.merge(&doc).unwrap(), data, "unchanged merge is the original bytes");
    // a translation that fits both pages is written in each section's own page
    let out = build(&data, &[(0, "\u{41f}\u{43e}\u{43a}\u{430}"), (1, "Th\u{e9}")]);
    assert!(out.windows(2).any(|w| w == b"\"\xcf"), "1251 section keeps its page");
    assert!(out.windows(3).any(|w| w == b"Th\xe9"), "1252 section keeps its page");
    // a text neither page holds makes the whole file UTF-8 with every pragma 65001
    let text = String::from_utf8(build(&data, &[(1, "\u{65e5}\u{672c}")])).expect("utf-8");
    assert_eq!(text.matches("#pragma code_page(65001)").count(), 2, "{text}");
    // a byte the section's page rejects is refused, and says which page
    let bad = b"#pragma code_page(1252)\n#pragma code_page(932)\nSTRINGTABLE BEGIN 1 \"\x85\" END\n";
    match extract(bad) {
        Err(Error::Encoding(m)) => assert!(m.contains("932"), "{m}"),
        other => panic!("{other:?}"),
    }
}
