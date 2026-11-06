//! The `xml.its_rules` and `xml.multilingual` format options.

mod common;

use bilingua_model::*;
use bilingua_xml::{ItsRules, MlConfig, XmlFilter};
use common::*;

const RULES: &str = r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//code" translate="no"/><its:locNoteRule selector="//p"><its:locNote>Keep it short.</its:locNote></its:locNoteRule></its:rules>"#;
const DOC: &[u8] = b"<doc><p>Hello</p><code>Run this</code></doc>";

fn with(key: &str, v: &str) -> ExtractOptions {
    opts(None).with_format_option(key, v)
}

#[test]
fn its_rules_option_matches_the_builder() {
    let plain = XmlFilter::new().extract(DOC, &opts(None)).unwrap();
    assert_eq!(plain.units.len(), 2);
    let built = XmlFilter::new().its_rules(ItsRules::parse(RULES.as_bytes()).unwrap()).extract(DOC, &opts(None)).unwrap();
    let via = XmlFilter::new().extract(DOC, &with("xml.its_rules", RULES)).unwrap();
    assert_eq!(built.units.len(), 1);
    assert_eq!(via.units.len(), 1);
    assert_eq!(via.units[0].context.notes, built.units[0].context.notes);
    assert_eq!(via.units[0].context.notes, ["Keep it short."]);
    assert_eq!(XmlFilter::new().merge(&via).unwrap(), DOC);
}

#[test]
fn rules_in_the_document_still_apply_after_the_option() {
    let doc = br#"<doc xmlns:its="http://www.w3.org/2005/11/its" its:version="2.0"><its:rules version="2.0"><its:translateRule selector="//p" translate="no"/></its:rules><p>Hello</p><code>Run this</code><q>Quote</q></doc>"#;
    let d = XmlFilter::new().extract(doc, &with("xml.its_rules", RULES)).unwrap();
    let t: Vec<String> = d.units.iter().map(show).collect();
    assert_eq!(t, ["Quote"]);
}

#[test]
fn bad_rules_are_refused_not_ignored() {
    assert!(XmlFilter::new().extract(DOC, &with("xml.its_rules", "<not-rules/>")).is_err());
    assert!(XmlFilter::new().extract(DOC, &with("xml.its_rules", "<its:rules")).is_err());
    // an empty value is no rules
    assert_eq!(XmlFilter::new().extract(DOC, &with("xml.its_rules", "  ")).unwrap().units.len(), 2);
}

#[test]
fn multilingual_option_matches_the_builder() {
    let d = data("catalog-attr.xml");
    let json = r#"{"mode": "attr", "source": "en"}"#;
    let built = XmlFilter::new().multilingual(MlConfig::from_json(json).unwrap());
    let a = texts(&built, &d);
    let b: Vec<String> = XmlFilter::new().extract(&d, &with("xml.multilingual", json)).unwrap().units.iter().map(show).collect();
    assert_eq!(a, b);
    assert!(!a.is_empty());
    // switched off, the file is read as plain XML: more units than the source side only
    let off = XmlFilter::new().extract(&d, &with("xml.multilingual", r#"{"enabled": false}"#)).unwrap();
    assert!(off.units.len() > a.len(), "{} vs {}", off.units.len(), a.len());
    // a merge needs no option: the layout travels in the document
    let doc = XmlFilter::new().extract(&d, &with("xml.multilingual", json)).unwrap();
    assert_eq!(XmlFilter::new().merge(&doc).unwrap(), d);
}

#[test]
fn bad_multilingual_config_is_refused() {
    let d = data("catalog-attr.xml");
    assert!(XmlFilter::new().extract(&d, &with("xml.multilingual", "{nope")).is_err());
    assert!(XmlFilter::new().extract(&d, &with("xml.multilingual", r#"{"mode": "sideways"}"#)).is_err());
}

#[test]
fn keys_are_declared_and_checked() {
    let keys: Vec<&str> = XmlFilter::new().format_option_keys().iter().map(|k| k.key).collect();
    assert_eq!(keys, ["xml.its_rules", "xml.multilingual"]);
    assert!(matches!(XmlFilter::new().extract(DOC, &with("xml.its_rule", "x")), Err(Error::FormatOption { .. })));
    let doc = XmlFilter::new().extract(DOC, &opts(None)).unwrap();
    assert!(matches!(XmlFilter::new().merge_with(&doc, &MergeOptions::new().with_format_option("xml.nope", 1)), Err(Error::FormatOption { .. })));
}
