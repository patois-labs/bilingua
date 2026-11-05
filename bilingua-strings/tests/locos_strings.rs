//! The Localization OS suite `tests/verify_strings_filter.py`, ported case by case (6 test functions, 6 here),
//! plus the helper checks it uses from `_kvres_common.py`.
mod common;
use bilingua_model::*;
use bilingua_strings::{records, Limits, Strings};
use common::*;

const SRC: &str = "/* Title of the main window */\r\n\
\"Welcome\" = \"Welcome\";\r\n\
// Button that saves\r\n\
\"save.button\" = \"Save \\\"all\\\" files\";\r\n\
\"multi\" = \"Line one\\nLine two\";\r\n\
\"greeting\" = \"Hello %@, you have %1$@ items\";\r\n\
\"pct\" = \"%@\";\r\n\
\"unicode\" = \"Caf\\U00e9\";\r\n\
plainKey = plainValue;\r\n\
\"backslash\" = \"C:\\\\Temp\";\r\n";

fn src_units() -> Vec<String> {
    ["Welcome", "Save \"all\" files", "Line one\nLine two", "Hello %@, you have %1$@ items", "Caf\u{e9}", "plainValue", "C:\\Temp"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

const UTF8: &[u8] = include_bytes!("data/locos_src_utf8.strings");
const LE_BOM: &[u8] = include_bytes!("data/locos_src_utf16le_bom.strings");
const LE_NOBOM: &[u8] = include_bytes!("data/locos_src_utf16le_nobom.strings");
const BE_BOM: &[u8] = include_bytes!("data/locos_src_utf16be_bom.strings");
const BRACES: &[u8] = include_bytes!("data/locos_braces.strings");
const CP1252: &[u8] = include_bytes!("data/locos_cp1252.strings");
const BPLIST: &[u8] = include_bytes!("data/locos_bplist.strings");

fn utf16le(s: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        v.extend_from_slice(&u.to_le_bytes());
    }
    v
}

#[test]
fn fixtures_match_the_suite_source() {
    assert_eq!(UTF8, SRC.as_bytes());
    assert_eq!(LE_BOM, utf16le(SRC).as_slice());
}

#[test]
fn test_grammar_utf8() {
    let data = SRC.as_bytes();
    assert_eq!(units_of(data), src_units());
    assert_identity(data);
    let mut d = ex(data);
    set_nth(&mut d, 0, "Willkommen");
    set_nth(&mut d, 1, "Speichere \"alle\" Dateien");
    set_nth(&mut d, 2, "Zeile eins\nZeile zwei");
    set_nth(&mut d, 5, "Klartext");
    set_nth(&mut d, 6, "D:\\Temp");
    let expected = SRC
        .replace("\"Welcome\" = \"Welcome\"", "\"Welcome\" = \"Willkommen\"")
        .replace("Save \\\"all\\\" files", "Speichere \\\"alle\\\" Dateien")
        .replace("Line one\\nLine two", "Zeile eins\\nZeile zwei")
        .replace("plainKey = plainValue;", "plainKey = \"Klartext\";")
        .replace("C:\\\\Temp", "D:\\\\Temp");
    let out = merged(&d);
    assert_eq!(String::from_utf8(out.clone()).unwrap(), expected);
    // Re-read: same unit count, translations read back.
    let back = units_of(&out);
    assert_eq!(back.len(), 7);
    assert_eq!(back[0], "Willkommen");
    assert_eq!(back[1], "Speichere \"alle\" Dateien");
    assert_eq!(back[2], "Zeile eins\nZeile zwei");
    assert_eq!(back[5], "Klartext");
    assert_eq!(back[6], "D:\\Temp");
    assert_eq!(back[3], src_units()[3]);
}

#[test]
fn test_utf16_with_bom_round_trip() {
    let doc = ex(LE_BOM);
    assert_eq!(texts(&doc), src_units());
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "utf-16le-bom"));
    assert_identity(LE_BOM);
    let mut d = doc;
    let jp = "\u{3088}\u{3046}\u{3053}\u{305d}";
    set_nth(&mut d, 0, jp);
    assert_eq!(merged(&d), utf16le(&SRC.replace("= \"Welcome\"", &format!("= \"{jp}\""))));
    // BOM-less UTF-16 LE (genstrings without a BOM) is sniffed and kept BOM-less.
    assert_eq!(units_of(LE_NOBOM), src_units());
    assert_identity(LE_NOBOM);
    assert!(matches!(&ex(LE_NOBOM).skeleton, Skeleton::Text { encoding, .. } if encoding == "utf-16le"));
    // UTF-16 BE with BOM.
    assert_eq!(units_of(BE_BOM), src_units());
    assert_identity(BE_BOM);
    assert!(matches!(&ex(BE_BOM).skeleton, Skeleton::Text { encoding, .. } if encoding == "utf-16be-bom"));
    // Also UTF-8 with BOM and bare UTF-8.
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(SRC.as_bytes());
    assert_identity(&bom);
    assert_eq!(units_of(&bom), src_units());
}

#[test]
fn test_braces_wrapper_and_dictionary_form() {
    assert_eq!(units_of(BRACES), ["Alpha", "Beta"]);
    assert_identity(BRACES);
}

#[test]
fn test_placeholders() {
    // LocOS flags a dropped placeholder with its shared quality gate. Bilingua makes each placeholder a tag
    // instead, so a translation cannot drop one unseen or invent one.
    let d = ex(SRC.as_bytes());
    let u = d.units.iter().find(|u| u.context.key.as_deref() == Some("greeting")).unwrap();
    let src = u.source();
    let tags: Vec<(&str, bool)> = src
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { display, free, .. } => Some((display.as_str(), *free)),
            _ => None,
        })
        .collect();
    assert_eq!(tags, [("%@", false), ("%1$@", true)]);
    let mut d2 = ex(b"\"k\" = \"Deleted %d files\";\n");
    let id = d2.units[0].segments[0].id.clone();
    assert!(matches!(d2.set_target(&id, vec![Inline::tag("9", TagKind::Standalone, "%d")]), Err(Error::InventedTag(_))));
    let t = target_for(&d2.units[0], "%d Dateien entfernt");
    d2.set_target(&id, t.clone()).unwrap();
    assert_eq!(merged(&d2), b"\"k\" = \"%d Dateien entfernt\";\n");
    let mut dup = t.clone();
    dup.push(t[0].clone());
    assert!(matches!(d2.set_target(&id, dup), Err(Error::DuplicatedTag(_))));
    // A translation that moves the positional tag keeps it.
    let mut d3 = ex(b"\"k\" = \"Hello %@, you have %1$@ items\";\n");
    set_nth(&mut d3, 0, "%1$@ Artikel, hallo %@");
    assert_eq!(merged(&d3), b"\"k\" = \"%1$@ Artikel, hallo %@\";\n");
}

fn assert_refused(b: &[u8], needle: &str) {
    let msg = parse_err(b);
    assert!(msg.to_lowercase().contains(&needle.to_lowercase()), "{msg:?} should contain {needle:?}");
}

#[test]
fn test_hostile_inputs() {
    assert_refused(b"\"a\" = \"never closed;\n", "never closed");
    assert_refused(b"/* open comment\n\"a\" = \"b\";\n", "never closed");
    assert!(matches!(Strings.extract(BPLIST, &opts()), Err(Error::Unsupported(m)) if m.contains("binary property list")));
    assert!(matches!(Strings.extract(CP1252, &opts()), Err(Error::Unsupported(m)) if m.contains("UTF-8")));
    assert_refused(b"\"a\" = \"b\"\n\"c\" = \"d\";\n", "missing ';'");
    assert_refused(b"\"a\" = \"b\"; <script>\n", "unexpected");
    assert_refused(b"\"a\" = \"b\";\n\"c\" =", "");
    // Segment cap: more entries than the limit are refused.
    let data: Vec<u8> = (0..30).flat_map(|i| format!("\"k{i}\" = \"Value number {i}\";\n").into_bytes()).collect();
    let small = { let mut built = Limits::default(); built.max_units = 10; built };
    assert!(matches!(Strings.extract_with(&data, &opts(), &small), Err(Error::Unsupported(_))));
    assert!(Strings.extract_with(&data, &opts(), &Limits::default()).is_ok());
}

#[test]
fn test_records_view() {
    let recs = records(SRC.as_bytes()).unwrap();
    assert_eq!(recs.len(), 8);
    assert!(recs[0].key == "Welcome" && recs[0].comment == "Title of the main window", "{:?}", recs[0]);
    assert!(recs[1].key == "save.button" && recs[1].comment == "Button that saves", "{:?}", recs[1]);
    assert!(recs[4].key == "pct" && recs[4].unit_index.is_none(), "{:?}", recs[4]);
    assert!(recs[6].key == "plainKey" && recs[6].value == "plainValue");
    assert_eq!(recs[7].unit_index, Some(6));
    // The same data through the document: the comment above a pair is the unit's note.
    let d = ex(SRC.as_bytes());
    assert_eq!(d.units[0].context.notes, ["Title of the main window"]);
    assert_eq!(d.units[0].context.key.as_deref(), Some("Welcome"));
    assert_eq!(d.units[1].context.notes, ["Button that saves"]);
}
