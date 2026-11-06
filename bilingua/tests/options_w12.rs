//! Wave 12 item D through the facade: `xaml.view_attributes`, `xml.its_rules`, `xml.multilingual`.

use bilingua::*;

const VIEW: &[u8] = include_bytes!("../../bilingua-xaml/tests/data/locos_suite_view.xaml");
const RULES: &str = r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//code" translate="no"/></its:rules>"#;

#[test]
fn the_keys_are_listed() {
    let keys: Vec<&str> = format_option_keys().iter().map(|k| k.key).collect();
    for k in ["xaml.view_attributes", "xml.its_rules", "xml.multilingual"] {
        assert!(keys.contains(&k), "{k}");
    }
}

#[test]
fn xaml_view_attributes_through_the_facade() {
    let off = extract("a.xaml", VIEW, &ExtractOptions::new()).unwrap();
    let on = extract("a.xaml", VIEW, &ExtractOptions::new().with_format_option("xaml.view_attributes", true)).unwrap();
    assert_eq!(off.units.len(), 1);
    assert_eq!(on.units.len(), 6);
    assert_eq!(merge(&on).unwrap(), VIEW);
}

#[test]
fn xml_options_through_the_facade() {
    let doc = b"<doc><p>Hello</p><code>Run this</code></doc>";
    assert_eq!(extract("a.xml", doc, &ExtractOptions::new()).unwrap().units.len(), 2);
    let o = ExtractOptions::new().with_format_option("xml.its_rules", RULES);
    assert_eq!(extract("a.xml", doc, &o).unwrap().units.len(), 1);
    // a key for another format goes with any file
    assert!(extract("a.xml", doc, &ExtractOptions::new().with_format_option("xaml.view_attributes", true)).is_ok());
    let bad = ExtractOptions::new().with_format_option("xml.its_rules", "<x/>");
    assert!(extract("a.xml", doc, &bad).is_err());
}

const DITA: &[u8] = b"<?xml version=\"1.0\"?>\n<!DOCTYPE topic PUBLIC \"-//OASIS//DTD DITA Topic//EN\" \"topic.dtd\">\n<topic id=\"t\"><title>T</title><body><p>Run <codeph>ls</codeph> now</p><note>Careful</note></body></topic>";
const DBK: &[u8] = b"<?xml version=\"1.0\"?>\n<article xmlns=\"http://docbook.org/ns/docbook\" version=\"5.0\"><title>T</title><para>Hi <code>x</code></para><note><para>Careful</para></note></article>";
const DITA_RULES: &str = r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//codeph" translate="no"/><its:translateRule selector="//note" translate="no"/></its:rules>"#;

fn shown(name: &str, bytes: &[u8], o: &ExtractOptions) -> Result<Vec<String>, Error> {
    let doc = extract(name, bytes, &o.clone().with_granularity(Granularity::Paragraph))?;
    Ok(doc.units.iter().map(|u| profile::tokens::to_locos_text(&u.source()).0).collect())
}

/// DITA and DocBook run on bilingua-xml's engine, as LocOS's `DitaFilter` / `DocBookFilter` subclass `ItsXmlFilter`:
/// `xml.its_rules` applies to them (LocOS `DitaFilter(rules=...)`), `xml.multilingual` is checked and ignored
/// (LocOS's constructor validates it, its multilingual reading is for generic `.xml` only). Expected texts are
/// LocOS's.
#[test]
fn dita_and_docbook_take_the_xml_keys_as_locos_does() {
    let none = ExtractOptions::new();
    let rules = ExtractOptions::new().with_format_option("xml.its_rules", DITA_RULES);
    assert_eq!(shown("t.dita", DITA, &none).unwrap(), ["T", "Run \u{27E6}x1\u{27E7} now", "Careful"]);
    assert_eq!(shown("t.dita", DITA, &rules).unwrap(), ["T", "Run \u{27E6}x1\u{27E7} now"]);
    // DocBook 5 is namespaced: `//note` does not match, as in LocOS
    assert_eq!(shown("t.dbk", DBK, &none).unwrap(), ["T", "Hi \u{27E6}x1\u{27E7}", "Careful"]);
    assert_eq!(shown("t.dbk", DBK, &rules).unwrap(), ["T", "Hi \u{27E6}x1\u{27E7}", "Careful"]);
    let good = ExtractOptions::new().with_format_option("xml.multilingual", r#"{"mode": "element", "source": "en"}"#);
    assert_eq!(shown("t.dita", DITA, &good).unwrap(), ["T", "Run \u{27E6}x1\u{27E7} now", "Careful"]);
    let bad = ExtractOptions::new().with_format_option("xml.multilingual", r#"{"mode": "sideways"}"#);
    assert!(shown("t.dita", DITA, &bad).unwrap_err().to_string().contains("mode must be"));
    let bad = ExtractOptions::new().with_format_option("xml.multilingual", "{nope");
    assert!(shown("t.dbk", DBK, &bad).unwrap_err().to_string().contains("not valid JSON"));
    let bad = ExtractOptions::new().with_format_option("xml.its_rules", "<x/>");
    assert!(shown("t.dita", DITA, &bad).is_err());
    // a merge after the option gives the bytes back
    let doc = extract("t.dita", DITA, &rules).unwrap();
    assert_eq!(merge(&doc).unwrap(), DITA);
}
