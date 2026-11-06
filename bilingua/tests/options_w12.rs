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
