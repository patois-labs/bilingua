//! `.ts` is shared by Qt Linguist (XML) and TypeScript: the facade decides by sniffing the bytes.

use bilingua::*;

const QT: &[u8] = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"de_DE\">\n<context>\n    <name>Main</name>\n    <message>\n        <source>Open file</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n</context>\n</TS>\n";
const TYPESCRIPT: &[u8] = b"export const messages = {\n  greeting: 'Hello there, friend',\n  farewell: 'See you soon',\n};\n";

#[test]
fn a_qt_ts_file_goes_to_the_qt_filter() {
    assert_eq!(detect("app_de.ts", QT).unwrap().id(), "ts");
    assert_eq!(detect("APP.TS", QT).unwrap().id(), "ts");
    let opts = ExtractOptions::new().with_target_lang("fr-FR");
    let mut doc = extract("app_de.ts", QT, &opts).unwrap();
    assert_eq!(doc.format, "ts");
    assert_eq!(doc.units.len(), 1);
    assert_eq!(merge(&doc).unwrap(), QT);
    doc.set_target("u1", vec![Inline::text("Ouvrir")]).unwrap();
    let out = String::from_utf8(merge(&doc).unwrap()).unwrap();
    assert!(out.contains("<translation>Ouvrir</translation>") && out.contains("language=\"fr_FR\""), "{out}");
}

#[test]
fn a_typescript_module_goes_to_the_js_module_filter() {
    assert_eq!(detect("messages.ts", TYPESCRIPT).unwrap().id(), "js");
    let doc = extract("messages.ts", TYPESCRIPT, &ExtractOptions::default()).unwrap();
    assert_eq!(doc.format, "js");
    assert_eq!(doc.units.len(), 2);
    assert_eq!(merge(&doc).unwrap(), TYPESCRIPT);
}

#[test]
fn other_xml_with_a_ts_extension_is_not_read_as_typescript() {
    let other = b"<?xml version=\"1.0\"?><catalog><item>Hello</item></catalog>";
    let e = extract("data.ts", other, &ExtractOptions::default()).unwrap_err();
    assert!(e.to_string().contains("XML"), "{e}");
}

#[test]
fn mts_and_cts_stay_javascript_and_extensionless_qt_is_sniffed() {
    assert_eq!(detect("a.mts", TYPESCRIPT).unwrap().id(), "js");
    assert_eq!(detect("a.cts", TYPESCRIPT).unwrap().id(), "js");
    assert_eq!(detect("noextension", QT).unwrap().id(), "ts");
}
