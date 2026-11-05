//! The Qt Linguist cases of the Localization OS suite `tests/verify_xml_resource_filters.py` (`test_qt_ts`, the
//! `.ts` lines of `test_registration` and `test_hostile_inputs`, the shared hostile and encoding cases run on a
//! `.ts` file) and of `verify_js_module_filter.py` (the XML `.ts` refusal), on the same fixture
//! (`tests/data/locos_app_de.ts`), written fresh for Bilingua's model.

mod common;
use bilingua_model::*;
use bilingua_qtts::QtTs;
use common::*;

fn app() -> Vec<u8> {
    lf("locos_app_de.ts")
}

// -- test_registration: the filter is the `ts` format and claims the extension --------------------------------

#[test]
fn registration() {
    assert_eq!(QtTs.id(), "ts");
    assert_eq!(QtTs.extensions(), &["ts"]);
    assert!(!QtTs.bilingual());
}

// -- test_qt_ts ---------------------------------------------------------------------------------------------

#[test]
fn units_skip_vanished_and_protect_byte() {
    let d = ex(&app());
    let t = texts(&d);
    assert!(!t.contains(&"Old caption".to_string()), "vanished is skipped");
    assert!(t.contains(&"&Open file...".to_string()));
    assert!(t.contains(&"Tab\u{27e6}x1\u{27e7}separated".to_string()), "<byte> is protected: {t:?}");
    assert_eq!(t.len(), 4);
}

#[test]
fn untouched_merge_is_the_original() {
    let data = app();
    assert_eq!(merged(&ex(&data)), data);
}

#[test]
fn translated_merge_changes_only_translations() {
    let data = app();
    let mut d = ex(&data);
    let map = |s: &str| match s {
        "&Open file..." => "Datei &öffnen...",
        "Saved %1 of %2 pages" => "%1 von %2 Seiten gespeichert",
        "%n file(s) deleted" => "%n Datei(en) gelöscht",
        "Tab" => "Tab",
        "separated" => "getrennt",
        other => panic!("unexpected {other}"),
    };
    translate_all(&mut d, |s| map(s).to_string());
    let out = s(&merged(&d));
    assert!(out.contains("<translation>Datei &amp;öffnen...</translation>"), "{out}");
    assert!(out.contains("<translation>%1 von %2 Seiten gespeichert</translation>"), "{out}"); // was <translation type="unfinished"/>
    assert_eq!(out.matches("<numerusform>%n Datei(en) gelöscht</numerusform>").count(), 2, "{out}");
    assert!(out.contains("<translation type=\"vanished\">Alte Beschriftung</translation>"));
    assert!(out.contains("<translation>Tab<byte value=\"x9\"/>getrennt</translation>"), "{out}");
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n"));
    assert!(!out.contains("type=\"unfinished\""));
    // every byte outside the translated values is the original: put the old values back and compare
    let mut back = out.clone();
    for (new, old) in [
        ("<translation>Datei &amp;öffnen...</translation>", "<translation type=\"unfinished\"></translation>"),
        ("<translation>%1 von %2 Seiten gespeichert</translation>", "<translation type=\"unfinished\"/>"),
        (
            "<translation>\n            <numerusform>%n Datei(en) gelöscht</numerusform>\n            <numerusform>%n Datei(en) gelöscht</numerusform>\n        </translation>",
            "<translation type=\"unfinished\">\n            <numerusform></numerusform>\n            <numerusform></numerusform>\n        </translation>",
        ),
        ("<translation>Tab<byte value=\"x9\"/>getrennt</translation>", "<translation>Tab<byte value=\"x9\"/>getrennt</translation>"),
    ] {
        back = back.replacen(new, old, 1);
    }
    assert_eq!(back, s(&data));
}

#[test]
fn numerus_message_with_no_forms_gets_one_per_target_form() {
    let bare = s(&app()).replace(
        "<translation type=\"unfinished\">\n            <numerusform></numerusform>\n            <numerusform></numerusform>\n        </translation>",
        "<translation type=\"unfinished\"/>",
    );
    let mut d = ex_to(bare.as_bytes(), "pl");
    let k = d.units.iter().position(|u| token_text(u).starts_with("%n")).unwrap();
    set(&mut d, k, "%n plik(ów)");
    let pl = s(&merged(&d));
    assert_eq!(pl.matches("<numerusform>%n plik(ów)</numerusform>").count(), 3, "{pl}");
    assert!(pl.contains("language=\"pl\""), "{pl}");
}

#[test]
fn existing_forms_are_topped_up_on_a_localised_merge() {
    // #729 (decision 18): 2 forms in the file, Polish 3, Arabic 6, Japanese stays 2, and build() adds none
    let data = app();
    let k = ex(&data).units.iter().position(|u| token_text(u).starts_with("%n")).unwrap();
    let run = |target: Option<&str>, text: &str| {
        let mut d = match target {
            Some(t) => ex_to(&data, t),
            None => ex(&data),
        };
        set(&mut d, k, text);
        s(&merged(&d))
    };
    let pl2 = run(Some("pl-PL"), "%n plik(ów)");
    assert_eq!(pl2.matches("<numerusform>%n plik(ów)</numerusform>").count(), 3, "{pl2}");
    assert!(
        pl2.contains(
            "            <numerusform>%n plik(ów)</numerusform>\n            <numerusform>%n plik(ów)</numerusform>\n            <numerusform>%n plik(ów)</numerusform>\n        </translation>"
        ),
        "{pl2}"
    );
    let ar = run(Some("ar-SA"), "AR %n");
    assert_eq!(ar.matches("<numerusform>AR %n</numerusform>").count(), 6, "{ar}");
    let ja = run(Some("ja-JP"), "JA %n");
    assert_eq!(ja.matches("<numerusform>JA %n</numerusform>").count(), 2, "{ja}");
    let plain = run(None, "X %n");
    assert_eq!(plain.matches("<numerusform>X").count(), 2);
}

#[test]
fn key_view_context_comment_and_plural() {
    // the LocOS records() view: context, comment, plural kind and which entries are translatable
    let d = ex_locked(&app());
    assert_eq!(d.units.len(), 5);
    let u0 = &d.units[0];
    assert_eq!(u0.context.extra.get("context").map(String::as_str), Some("MainWindow"));
    assert_eq!(u0.context.notes, vec!["menu entry".to_string()]);
    assert_eq!(u0.context.key.as_deref(), Some("&Open file..."));
    assert_eq!(u0.context.location.as_deref(), Some("../src/mainwindow.cpp:42"));
    assert_eq!(d.units[2].context.extra.get("plural").map(String::as_str), Some("numerus"));
    assert!(d.units[2].translate);
    assert!(!d.units[3].translate, "the vanished message is listed but locked");
    assert_eq!(d.units[3].lock_reason.as_deref(), Some("vanished message"));
    assert!(d.units[4].translate);
    // slot numbers are LocOS's p_index
    let idx: Vec<&str> = d.units.iter().map(|u| u.context.extra["p_index"].as_str()).collect();
    assert_eq!(idx, ["0", "1", "2", "3", "4"]);
}

// -- test_hostile_inputs, on a .ts file ----------------------------------------------------------------------

#[test]
fn typescript_is_not_a_qt_file() {
    let msg = parse_msg(try_ex(b"export const x: number = 1;\n"));
    assert!(msg.to_lowercase().contains("not well-formed"), "{msg}");
}

#[test]
fn dtd_entity_bomb_is_refused() {
    let lol = b"<?xml version=\"1.0\"?><!DOCTYPE TS [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">]><TS><context><name>x</name><message><source>&b;</source><translation/></message></context></TS>";
    assert!(matches!(try_ex(lol), Err(Error::Limit(_))));
    // an internal subset without an ENTITY is refused by the Qt reader itself, with LocOS's words
    let subset = b"<?xml version=\"1.0\"?><!DOCTYPE TS [<!ELEMENT TS ANY>]><TS><context><name>x</name><message><source>Hi</source><translation/></message></context></TS>";
    assert!(parse_msg(try_ex(subset)).to_lowercase().contains("internal subset"));
}

#[test]
fn a_gt_inside_a_quoted_system_id_does_not_hide_the_subset() {
    let hidden = b"<?xml version=\"1.0\"?><!DOCTYPE TS SYSTEM \"a>b\" [<!ENTITY a \"x\">]><TS><context><name>n</name><message><source y=\"&a;\">hi</source><translation/></message></context></TS>";
    assert!(matches!(try_ex(hidden), Err(Error::Limit(_)) | Err(Error::Parse { .. })));
}

#[test]
fn external_entity_is_refused() {
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE TS SYSTEM \"file:///etc/passwd\"><TS><context><name>n</name><message><source>&xxe;</source><translation/></message></context></TS>";
    assert!(parse_msg(try_ex(xxe)).to_lowercase().contains("entity"));
}

#[test]
fn deep_nesting_is_refused() {
    let mut deep = b"<TS><context><name>n</name><message><source>".to_vec();
    deep.extend(b"<b>".repeat(3000));
    deep.push(b't');
    deep.extend(b"</b>".repeat(3000));
    deep.extend(b"</source><translation/></message></context></TS>");
    assert!(matches!(try_ex(&deep), Err(Error::Limit(_))));
}

#[test]
fn unit_budget_is_enforced() {
    let mut many = String::from("<TS><context><name>n</name>");
    for i in 0..20 {
        many.push_str(&format!("<message><source>Text {i}</source><translation type=\"unfinished\"/></message>"));
    }
    many.push_str("</context></TS>");
    let small = Limits { max_units: 5, ..Limits::default() };
    let e = QtTs.extract_with(many.as_bytes(), &opts(None), &small).unwrap_err();
    assert!(matches!(e, Error::Limit(_)), "{e}");
    // far more markup than units can ever need is refused before it is parsed
    let big = format!("<TS>{}</TS>", "<x/>".repeat(300));
    let tiny = Limits { max_units: 5, ..Limits::default() };
    assert!(matches!(QtTs.extract_with(big.as_bytes(), &opts(None), &tiny), Err(Error::Limit(_))));
}

#[test]
fn a_file_that_is_not_ts_is_refused_with_a_plain_message() {
    let other = b"<?xml version=\"1.0\"?><catalog><item>Hello</item></catalog>";
    assert!(parse_msg(try_ex(other)).contains("no <TS> root"));
}

// -- test_encoding_and_line_endings, on a .ts file ----------------------------------------------------------------

#[test]
fn utf16_upload_is_read_and_written_back_as_utf16() {
    // LocOS converts to UTF-8 on export; Bilingua keeps the encoding, so an untouched merge is byte identical
    let src = s(&app()).replace("encoding=\"utf-8\"", "encoding=\"utf-16\"");
    let mut utf16 = vec![0xFF, 0xFE];
    for u in src.encode_utf16() {
        utf16.extend(u.to_le_bytes());
    }
    let mut d = ex(&utf16);
    assert_eq!(d.units.len(), 4);
    assert_eq!(merged(&d), utf16);
    set(&mut d, 0, "Hallo");
    let out = merged(&d);
    assert!(out.starts_with(&[0xFF, 0xFE]));
    assert!(decoded(&out).contains("<translation>Hallo</translation>"));
}

// -- verify_js_module_filter: an XML .ts file is the Qt filter's, never JavaScript's -----------------------------

#[test]
fn qt_sniff_recognises_the_file_and_not_typescript() {
    assert!(QtTs.sniff(&app()));
    assert!(!QtTs.sniff(b"export const greeting = 'Hello';\n"));
    assert!(!QtTs.sniff(b"interface A { x: number }\nexport default {};\n"));
    assert!(!QtTs.sniff(b"<?xml version=\"1.0\"?><resources><string name=\"a\">b</string></resources>"));
    assert!(!QtTs.sniff(b""));
}

#[test]
fn undeclared_non_utf8_input_is_refused_not_guessed() {
    let mut b = b"<TS><context><name>n</name><message><source>caf".to_vec();
    b.push(0xE9);
    b.extend(b"</source><translation/></message></context></TS>");
    assert!(matches!(try_ex(&b), Err(Error::Encoding(_))));
    // the same bytes with a declaration are read as declared
    let mut d = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><TS><context><name>n</name><message><source>caf".to_vec();
    d.push(0xE9);
    d.extend(b"</source><translation/></message></context></TS>");
    assert_eq!(texts(&ex(&d)), vec!["caf\u{e9}".to_string()]);
}
