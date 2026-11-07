//! Port of LocOS `tests/verify_ini_filter.py` (five cases) plus the shared helpers it uses.
mod common;
use bilingua_ini::{records, IniFilter, Limits};
use bilingua_model::*;
use common::*;

const JOOMLA: &[u8] = b"; Joomla! Project\n\
; Note : All ini files need to be saved as UTF-8\n\
\n\
; Shown on the login button\n\
COM_DEMO_LOGIN=\"Log in\"\n\
COM_DEMO_QUOTE=\"Click \\\"Save\\\" to continue\"\n\
COM_DEMO_LEGACY=\"Press \"_QQ_\"Go\"_QQ_\" now\"\n\
COM_DEMO_COUNT=\"%d items in %s\"\n\
COM_DEMO_EMPTY=\"\"\n\
COM_DEMO_HTML=\"<strong>Warning!</strong> Check it\"\n";

const GENERIC: &[u8] = b"[General]\r\n\
# Window caption\r\n\
title = My Application\r\n\
version=1.2.3\r\n\
just a stray line\r\n\
\r\n\
[Messages]\r\n\
title = Messages window\r\n\
welcome =   Welcome back, friend   \r\n";

fn replace(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(hay).replace(&*String::from_utf8_lossy(from), &String::from_utf8_lossy(to));
    s.into_bytes()
}

#[test]
fn joomla_dialects() {
    assert_eq!(
        units(JOOMLA),
        vec!["Log in", "Click \"Save\" to continue", "Press \"Go\" now", "%d items in %s", "<strong>Warning!</strong> Check it"]
    );
    assert_identity(JOOMLA);
    let mapping: Vec<(usize, &str)> = vec![
        (0, "Anmelden"),
        (1, "Klicken Sie auf \"Speichern\""),
        (2, "Drücken Sie \"Los\""),
        (3, "%d Elemente in %s"),
        (4, "Zeile eins\nZeile zwei"),
    ];
    let mut expected = replace(JOOMLA, b"\"Log in\"", b"\"Anmelden\"");
    expected = replace(&expected, b"Click \\\"Save\\\" to continue", b"Klicken Sie auf \\\"Speichern\\\"");
    expected = replace(&expected, b"Press \"_QQ_\"Go\"_QQ_\" now", "Drücken Sie \"_QQ_\"Los\"_QQ_\"".as_bytes());
    expected = replace(&expected, b"%d items in %s", b"%d Elemente in %s");
    expected = replace(&expected, b"<strong>Warning!</strong> Check it", b"Zeile eins Zeile zwei");
    let out = build(JOOMLA, &mapping);
    assert_eq!(String::from_utf8_lossy(&out), String::from_utf8_lossy(&expected));
    let back = units(&out);
    assert_eq!(back[1], "Klicken Sie auf \"Speichern\"");
    assert_eq!(back[2], "Drücken Sie \"Los\"");
    assert_eq!(back[4], "Zeile eins Zeile zwei");
}

#[test]
fn generic_sections_and_layout() {
    assert_eq!(units(GENERIC), vec!["My Application", "1.2.3", "Messages window", "Welcome back, friend"]);
    assert_identity(GENERIC);
    let mut expected = replace(GENERIC, b"= My Application", b"= Meine Anwendung");
    expected = replace(&expected, b"Welcome back, friend", "Willkommen zurück".as_bytes());
    let out = build(GENERIC, &[(0, "Meine Anwendung"), (3, "Willkommen zurück")]);
    assert_eq!(out, expected);
    let back = units(&out);
    assert_eq!(back.len(), 4);
    assert_eq!(back[0], "Meine Anwendung");
    assert_eq!(back[3], "Willkommen zurück");
}

fn skeleton_encoding(d: &Document) -> &str {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => encoding,
        _ => panic!("text skeleton"),
    }
}

#[test]
fn encodings() {
    let mut bom8 = vec![0xEF, 0xBB, 0xBF];
    bom8.extend_from_slice(JOOMLA);
    assert_eq!(units(&bom8)[0], "Log in");
    assert_identity(&bom8);
    assert!(build(&bom8, &[(0, "Anmelden")]).starts_with(b"\xef\xbb\xbf; Joomla"));

    let utf16 = |s: &str| -> Vec<u8> {
        let mut v = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    };
    let g = String::from_utf8(GENERIC.to_vec()).unwrap();
    let u16 = utf16(&g);
    let doc = extract(&u16).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "My Application");
    assert_eq!(skeleton_encoding(&doc), "utf-16le-bom");
    assert_identity(&u16);
    assert_eq!(build(&u16, &[(0, "日本")]), utf16(&g.replace("My Application", "日本")));

    // Windows-1252 "[a]\nname = Café\n"
    let cp = b"[a]\nname = Caf\xe9\n".to_vec();
    let doc = extract(&cp).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "Café");
    assert_eq!(skeleton_encoding(&doc), "cp1252");
    // Fits Windows-1252: written back in it. Does not fit: UTF-8.
    assert_eq!(build(&cp, &[(0, "Caf\u{e8}")]), b"[a]\nname = Caf\xe8\n");
    assert_eq!(build(&cp, &[(0, "Кафе")]), "[a]\nname = Кафе\n".as_bytes());
}

#[test]
fn hostile_inputs() {
    // binary bytes are refused
    let bin: Vec<u8> = b"k=\x81\x00\x00\x00\x01\x02".iter().copied().cycle().take(6 * 40).collect();
    assert!(matches!(extract(&bin), Err(Error::Encoding(m)) if m.contains("binary")));
    // the segment cap (lowered, like the LocOS test lowers MAX_DOCUMENT_SEGMENTS)
    let mut data = Vec::new();
    for i in 0..30 {
        data.extend_from_slice(format!("KEY_{i}=\"Value number {i}\"\n").as_bytes());
    }
    let tight = { let mut built = Limits::default(); built.max_units = 10; built };
    assert!(matches!(IniFilter.extract_with(&data, &opts(), &tight), Err(Error::Limit(_))));
    assert!(IniFilter.extract(&data, &opts()).is_ok());
    // degenerate lines are tolerated
    assert!(units(b"=\n= =\n[]\n[unclosed\n;\n").is_empty());
}

#[test]
fn records_and_placeholders() {
    let recs = records(GENERIC).unwrap();
    let got: Vec<(&str, &str)> = recs.iter().map(|r| (r.section.as_str(), r.key.as_str())).collect();
    assert_eq!(got, vec![("General", "title"), ("General", "version"), ("Messages", "title"), ("Messages", "welcome")]);
    assert_eq!(recs[0].comment, "Window caption");
    assert!(!recs[2].duplicate);
    let j = records(JOOMLA).unwrap();
    assert_eq!(j[0].comment, "Shown on the login button");
    assert_eq!(j[4].key, "COM_DEMO_EMPTY");
    assert_eq!(j[4].unit_index, None);
    // The placeholder gate (`placeholder_violation`) lives in the LocOS quality layer, not in the filter: what the
    // filter owes it is that `%d items in %s` arrives as one unit with its placeholders as text.
    assert_eq!(j[3].value, "%d items in %s");
    assert_eq!(j[3].unit_index, Some(3));
}
