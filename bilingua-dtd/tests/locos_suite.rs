//! Port of `tests/verify_dtd_filter.py` (6 cases): units and identity, translated round trip, expansion in an XML
//! document, BOM and CRLF, hostile inputs and refusals, other declarations kept.
mod common;
use bilingua_dtd::{DtdFilter, Limits};
use bilingua_model::*;
use common::*;

const SRC: &str = "<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. -->

<!ENTITY % brandDTD SYSTEM \"chrome://branding/locale/brand.dtd\">
%brandDTD;

<!-- LOCALIZATION NOTE (window.title): &brandShortName; is the product name. -->
<!ENTITY window.title \"&brandShortName; Settings\">
<!ENTITY save.label 'Save &amp; close'>
<!ENTITY save.accesskey \"S\">
<!ENTITY quote.text \"Say &quot;hi&quot; to &#169; 100&#037;\">
<!ENTITY markup.text \"Click <html:b>here</html:b> &lt;now&gt;\">
<!ENTITY only.ref \"&brandShortName;\">
<!ENTITY   spaced.label
     \"Spaced out\"  >
<!ENTITY ext.file SYSTEM \"http://example.invalid/evil.dtd\">
";

#[test]
fn units_and_identity() {
    assert_eq!(DtdFilter.extensions(), &["dtd"]);
    let doc = extract(SRC.as_bytes()).unwrap();
    let got: Vec<(String, String)> = doc.units.iter().map(|u| (u.context.key.clone().unwrap(), flat(&u.source()))).collect();
    let want = [
        ("window.title", "&brandShortName; Settings"),
        ("save.label", "Save & close"),
        ("save.accesskey", "S"),
        ("quote.text", "Say \"hi\" to \u{a9} 100%"),
        ("markup.text", "Click <html:b>here</html:b> &lt;now&gt;"),
        ("spaced.label", "Spaced out"),
    ];
    assert_eq!(got, want.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Vec<_>>());
    assert_eq!(DtdFilter.merge(&doc).unwrap(), SRC.as_bytes());
    assert_identity(SRC.as_bytes());
    // the inline codes: references and markup are tags, the rest is text
    let tags = |i: usize| -> Vec<(String, TagKind)> {
        doc.units[i]
            .source()
            .iter()
            .filter_map(|x| match x {
                Inline::Tag { display, kind, .. } => Some((display.clone(), *kind)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(tags(0), vec![("&brandShortName;".to_string(), TagKind::Standalone)]);
    assert_eq!(
        tags(4),
        vec![
            ("<html:b>".to_string(), TagKind::Open),
            ("</html:b>".to_string(), TagKind::Close),
            ("&lt;".to_string(), TagKind::Standalone),
            ("&gt;".to_string(), TagKind::Standalone)
        ]
    );
    assert!(doc.units[0].codes.values().all(|c| c.label.as_deref() == Some("placeholder") || c.label.as_deref() == Some("\"")));
    // the LOCALIZATION NOTE comment is the note of window.title; the licence comment is detached by the blank line
    assert_eq!(doc.units[0].context.notes.len(), 1);
    assert!(doc.units[0].context.notes[0].starts_with("LOCALIZATION NOTE (window.title)"));
    assert!(doc.units[1].context.notes.is_empty());
}

#[test]
fn translated_round_trip() {
    let de = [
        "&brandShortName;-Einstellungen",
        "Speichern & schlie\u{df}en, 'sofort'",
        "P",
        "Sag \"hallo\" zu \u{a9} 100%",
        "Klicke <html:b>hier</html:b> &lt;jetzt&gt;",
        "Mit Abstand",
    ];
    let mapping: Vec<(usize, &str)> = de.iter().enumerate().map(|(i, s)| (i, *s)).collect();
    let out = String::from_utf8(build(SRC.as_bytes(), &mapping)).unwrap();
    assert!(out.contains("<!ENTITY window.title \"&brandShortName;-Einstellungen\">"), "{out}");
    assert!(out.contains("<!ENTITY save.label 'Speichern &amp; schlie\u{df}en, &apos;sofort&apos;'>"), "{out}");
    assert!(out.contains("<!ENTITY quote.text \"Sag &quot;hallo&quot; zu \u{a9} 100&#037;\">"), "{out}");
    assert!(out.contains("\"Klicke <html:b>hier</html:b> &lt;jetzt&gt;\""), "{out}");
    assert!(out.contains("<!ENTITY   spaced.label\n     \"Mit Abstand\"  >"), "{out}");
    assert!(out.contains("<!ENTITY % brandDTD SYSTEM \"chrome://branding/locale/brand.dtd\">\n%brandDTD;"));
    assert!(out.contains("<!ENTITY ext.file SYSTEM \"http://example.invalid/evil.dtd\">"));
    assert_eq!(units(out.as_bytes()), de);
}

/// The Python test expands the rebuilt entities with lxml; here the same check is a small expander that knows
/// only the predefined entities and character references (what an XML parser does for these values).
#[test]
fn rebuilt_entities_expand_to_the_translated_text() {
    fn expand(raw: &str) -> String {
        let mut s = raw.to_string();
        for (from, to) in [("&quot;", "\""), ("&apos;", "'"), ("&#037;", "%"), ("&#169;", "\u{a9}"), ("&amp;", "&")] {
            s = s.replace(from, to);
        }
        s
    }
    let out = String::from_utf8(build(SRC.as_bytes(), &[(1, "A & B \"c\" 'd' 50%")])).unwrap();
    let get = |name: &str| -> String {
        let line = out.lines().find(|l| l.starts_with(&format!("<!ENTITY {name} "))).unwrap();
        let q = line.chars().find(|c| *c == '"' || *c == '\'').unwrap();
        let body = &line[line.find(q).unwrap() + 1..line.rfind(q).unwrap()];
        expand(body)
    };
    assert_eq!(format!("{}|{}", get("save.label"), get("quote.text")), "A & B \"c\" 'd' 50%|Say \"hi\" to \u{a9} 100%");
}

#[test]
fn bom_crlf() {
    let src = b"\xef\xbb\xbf<!-- c -->\r\n<!ENTITY a \"One\">\r\n<!ENTITY b \"Two\">\r\n";
    assert_eq!(units(src), vec!["One", "Two"]);
    assert_eq!(DtdFilter.merge(&extract(src).unwrap()).unwrap(), src);
    assert_eq!(build(src, &[(1, "Zwei")]), b"\xef\xbb\xbf<!-- c -->\r\n<!ENTITY a \"One\">\r\n<!ENTITY b \"Zwei\">\r\n");
}

#[test]
fn hostile_and_refusals() {
    // billion laughs: nothing is ever expanded, the values are just text
    let lol = b"<!ENTITY lol \"lol\">\n<!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;\">\n";
    assert_eq!(units(lol), vec!["lol"]);
    let bad: [&[u8]; 6] = [
        b"<!ENTITY a \"caf\xe9\">",
        b"\x00\x01<!ENTITY a \"x\">",
        b"<!ENTITY a \"unterminated>\n",
        b"<!-- never closed",
        b"<!ENTITY a \"x\" junk>",
        b"stray text",
    ];
    for b in bad {
        assert!(extract(b).is_err(), "must refuse {:?}", String::from_utf8_lossy(b));
    }
    // the segment cap and the declaration budget (LocOS sets MAX_DOCUMENT_SEGMENTS = 3 for this test)
    let tight = Limits { max_units: 3, ..Limits::default() };
    let many: String = (0..10).map(|i| format!("<!ENTITY e{i} \"Value {i}\">\n")).collect();
    assert!(matches!(DtdFilter.extract_with(many.as_bytes(), &opts(), &tight), Err(Error::Unsupported(_))));
    let decls = "<!-- x -->".repeat(20);
    assert!(matches!(DtdFilter.extract_with(decls.as_bytes(), &opts(), &tight), Err(Error::Unsupported(_))));
}

#[test]
fn other_declarations_kept() {
    let src = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!ELEMENT r (#PCDATA)>\n<!ATTLIST r a CDATA \"x>y\">\n<!ENTITY a \"Alpha\">\n<!ENTITY pub PUBLIC \"-//X//EN\" \"x.dtd\">\n";
    assert_eq!(units(src), vec!["Alpha"]);
    let out = build(src, &[(0, "Alfa")]);
    assert_eq!(out, String::from_utf8_lossy(src).replace("\"Alpha\"", "\"Alfa\"").as_bytes());
}
