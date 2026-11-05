//! The Localization OS suite `tests/verify_xml_resource_filters.py`, the parts that run on a `.wxl` file, ported.
//!
//! The suite has 12 test functions. Ported here (5 of 12, every one that can run on a `.wxl`): `test_wxl` (all of
//! it), `test_hostile_inputs` and `test_encoding_and_line_endings` (both run on the shared XML resource base in
//! LocOS, here on a `.wxl`), plus the `.wxl` row of `tests/verify_keys_bridge.py` (`test_j_pending_formats`) and
//! the `records()` view. Not portable: `test_registration` (LocOS upload form and extension table; the facade
//! contract test covers registration), `test_upload_translate_download_end_to_end` (LocOS web API and CLI), and
//! the Android (4 functions), resx, Java properties and Qt functions (other formats).
//!
//! Fixture: `tests/data/locos_package.wxl` is `tests/fixtures/formats/wxl/Package.en-us.wxl` byte for byte.

mod common;
use bilingua_model::*;
use bilingua_wxl::Wxl;
use common::*;

fn unit_with<'a>(d: &'a Document, text: &str) -> &'a Unit {
    d.units.iter().find(|u| flat(&u.source()) == text).unwrap_or_else(|| panic!("no unit {text:?}"))
}

fn set(d: &mut Document, text: &str, to: &str) {
    let id = unit_with(d, text).segments[0].id.clone();
    d.set_target(&id, vec![Inline::text(to)]).unwrap();
}

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

#[test]
fn test_wxl() {
    let d = ex(LOCOS_PACKAGE);
    let units = texts(&d);
    assert!(!units.contains(&"{6F2C1E7A-0000-4000-8000-000000000001}".to_string())); // Localizable="no"
    assert!(units.contains(&"A newer version of [ProductName] is already installed.".to_string()));
    assert_eq!(
        units,
        vec!["Northwind Desktop", "A newer version of [ProductName] is already installed.", "Click Next to continue.", "Welcome to the setup wizard"]
    );
    assert_eq!(merged(&d), LOCOS_PACKAGE); // build(data, {}) == data

    let mut t = ex(LOCOS_PACKAGE);
    set(&mut t, "Northwind Desktop", "Northwind \"Bureau\" & co");
    set(&mut t, "Click Next to continue.", "Klicken Sie auf Weiter.");
    set(&mut t, "Welcome to the setup wizard", "Willkommen");
    t.target_lang = Some("de-DE".into());
    let out = s(&merged(&t));
    assert!(out.contains(r#"<String Id="ProductName" Value="Northwind &quot;Bureau&quot; &amp; co" />"#), "{out}");
    assert!(out.contains(r#"<String Id="LegacyText">Klicken Sie auf Weiter.</String>"#)); // WiX v3 form
    assert!(out.contains(r#"Text="Willkommen""#));
    assert!(out.contains(r#"Culture="de-de" Codepage="1252""#), "{out}"); // Codepage left as uploaded
    let again = ex(out.as_bytes());
    assert!(texts(&again).contains(&"Northwind \"Bureau\" & co".to_string()), "{:?}", texts(&again));

    // Gate: an attribute look-alike inside another attribute's value is left alone.
    let tricky = b"<WixLocalization Culture=\"en-us\" xmlns=\"http://wixtoolset.org/schemas/v4/wxl\"><String Id=\"a Value='x'\" Value=\"Hello\" /></WixLocalization>";
    let mut d = ex(tricky);
    set(&mut d, "Hello", "Hallo");
    let out = s(&merged(&d));
    assert!(out.contains(r#"<String Id="a Value='x'" Value="Hallo" />"#), "{out}");
}

#[test]
fn test_hostile_inputs_on_a_wxl() {
    let wrap = |inner: &str, doctype: &str| format!("<?xml version=\"1.0\"?>{doctype}<WixLocalization Culture=\"en-us\">{inner}</WixLocalization>");
    let refused = |data: &str, kind: Option<LimitKind>| {
        let e = try_ex(data.as_bytes()).expect_err(data);
        match (&e, kind) {
            (Error::Limit(l), Some(k)) => assert_eq!(l.kind, k, "{e}"),
            (Error::Parse { .. } | Error::Limit(_), None) => {}
            _ => panic!("unexpected {e:?} for {data}"),
        }
    };
    // Billion laughs: an internal subset with entity declarations.
    refused(
        &wrap("<String Id=\"x\">&b;</String>", "<!DOCTYPE WixLocalization [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">]>"),
        Some(LimitKind::XmlEntities),
    );
    // Gate: a ">" inside a quoted system id must not hide the internal subset (entity used only in an attribute).
    refused(&wrap("<String Id=\"x\" y=\"&a;\" Value=\"hi\"/>", "<!DOCTYPE WixLocalization SYSTEM \"a>b\" [<!ENTITY a \"x\">]>"), Some(LimitKind::XmlEntities));
    // External entity (XXE).
    refused(&wrap("<String Id=\"x\">&xxe;</String>", "<!DOCTYPE WixLocalization [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]>"), Some(LimitKind::XmlEntities));
    // XXE without a declaration is an undefined entity.
    refused(&wrap("<String Id=\"x\">&xxe;</String>", "<!DOCTYPE WixLocalization SYSTEM \"file:///etc/passwd\">"), None);
    // Not XML at all.
    let e = try_ex(b"export const x: number = 1;\n").unwrap_err();
    assert!(e.to_string().contains("not well-formed"), "{e}");
    // Deep nesting.
    let deep = format!("<WixLocalization><String Id='x'>{}t{}</String></WixLocalization>", "<b>".repeat(3000), "</b>".repeat(3000));
    refused(&deep, Some(LimitKind::XmlDepth));
    // The unit cap (LocOS lowers MAX_DOCUMENT_SEGMENTS to 5 and feeds 20 strings).
    let many: String = (0..20).map(|i| format!("<String Id=\"s{i}\" Value=\"Text {i}\"/>")).collect();
    let small = ExtractOptions { limits: Some(Limits { max_units: 5, ..Limits::default() }), ..opts(false) };
    match Wxl.extract(wrap(&many, "").as_bytes(), &small) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::Units),
        other => panic!("{other:?}"),
    }
    // ... and the element budget (LocOS: 300 empty elements against a cap of 5 units).
    let big = format!("<WixLocalization>{}</WixLocalization>", "<x/>".repeat(300));
    let small = ExtractOptions { limits: Some(Limits { max_xml_elements: 100, ..Limits::default() }), ..opts(false) };
    match Wxl.extract(big.as_bytes(), &small) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::XmlElements),
        other => panic!("{other:?}"),
    }
    // A stray ]]> in a translation for a CDATA value falls back to escaped text, still valid.
    let data = b"<WixLocalization><String Id=\"r\"><![CDATA[Read the <b>docs</b>]]></String></WixLocalization>";
    let mut d = ex(data);
    set(&mut d, "Read the <b>docs</b>", "a ]]> b");
    let out = merged(&d);
    assert!(s(&out).contains("a ]]&gt; b"), "{}", s(&out));
    ex(&out);
    // ... and a CDATA value without it stays CDATA.
    let mut d = ex(data);
    set(&mut d, "Read the <b>docs</b>", "Lies <i>das</i>");
    // (the target language de is stamped as the root's Culture, which this file did not have)
    assert_eq!(s(&merged(&d)), "<WixLocalization Culture=\"de\"><String Id=\"r\"><![CDATA[Lies <i>das</i>]]></String></WixLocalization>");
}

#[test]
fn test_encoding_and_line_endings() {
    // LocOS reads a UTF-16 upload and exports UTF-8 with a truthful declaration. Bilingua keeps the encoding the
    // file came in (an untouched merge must give the file back), so the check here is: read as UTF-16, same
    // units as the UTF-8 original, and the translated file is UTF-16 again with the new text.
    let d = ex(LANGUAGES_UTF16LE);
    let reference = ex(LANGUAGES);
    assert_eq!(texts(&d), texts(&reference));
    assert!(matches!(&d.skeleton, Skeleton::Text { encoding, .. } if enc::base(encoding) == "utf-16le"));
    let mut t = d.clone();
    set(&mut t, "Welcome to [ProductName]", "Hallo [ProductName]");
    let out = merged(&t);
    assert!(out.starts_with(&[0xFF, 0xFE]));
    let back = ex(&out);
    assert!(texts(&back).contains(&"Hallo [ProductName]".to_string()));
    assert_eq!(back.units.len(), d.units.len());
    // Line endings: CRLF stays CRLF in an untouched file and in a translated one.
    let crlf = ex(LANGUAGES_CRLF_BOM);
    assert_eq!(merged(&crlf), LANGUAGES_CRLF_BOM);
    let mut t = crlf.clone();
    set(&mut t, "Welcome to Larchmoor", "Willkommen in Larchmoor");
    let out = merged(&t);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(!s(&out).replace("\r\n", "").contains('\n'), "a bare line feed appeared");
}

#[test]
fn keys_bridge_sample() {
    // verify_keys_bridge.py PENDING row "wxl": the key list and a byte-identical export.
    let data = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<WixLocalization Culture=\"en-us\" xmlns=\"http://wixtoolset.org/schemas/v4/wxl\">\n  <String Id=\"Welcome\" Value=\"Welcome to setup\" />\n</WixLocalization>\n";
    let d = ex(data);
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].context.key.as_deref(), Some("Welcome"));
    assert_eq!(flat(&d.units[0].source()), "Welcome to setup");
    assert_eq!(merged(&d), data);
}

#[test]
fn records_of_the_locos_package() {
    // LocOS `records()`: every entry, translatable or not. With include_locked the Localizable="no" entry comes
    // as a locked unit; the default extract leaves it out.
    let d = ex_locked(LOCOS_PACKAGE);
    let keys: Vec<_> = d.units.iter().map(|u| u.context.key.clone().unwrap()).collect();
    assert_eq!(keys, vec!["ProductName", "Downgrade", "UpgradeCode", "LegacyText", "WelcomeDlg.Title"]);
    let locked: Vec<_> = d.units.iter().filter(|u| !u.translate).map(|u| u.context.key.clone().unwrap()).collect();
    assert_eq!(locked, vec!["UpgradeCode"]);
    assert_eq!(d.units[4].context.kind.as_deref(), Some("ui"));
    assert_eq!(merged(&d), LOCOS_PACKAGE);
    assert_eq!(ex(LOCOS_PACKAGE).units.len(), 4);
}
