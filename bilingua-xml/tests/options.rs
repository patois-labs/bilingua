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

// --- LocOS's suites through the options (verify_xml_its_filter test_user_rules_file, verify_xml_multilingual_filter
// test_config) and what LocOS's `ItsXmlFilter(rules=..., multilingual=...)` gives on a few more inputs.

const ITS_OPEN: &str = r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0">"#;

fn via(key: &str, v: &str, d: &[u8]) -> Result<Vec<String>, Error> {
    XmlFilter::new().extract(d, &with(key, v)).map(|doc| doc.units.iter().map(show).collect())
}

fn refused_via(key: &str, v: &str, d: &[u8], needle: &str) {
    match via(key, v, d) {
        Err(e) => assert!(e.to_string().contains(needle), "{needle} not in {e}"),
        Ok(t) => panic!("accepted ({needle}): {t:?}"),
    }
}

#[test]
fn locos_user_rules_file_through_the_option() {
    let rules = format!(
        r#"{ITS_OPEN}<its:translateRule selector="//note" translate="no"/><its:translateRule selector="//item/@title" translate="yes"/><its:withinTextRule selector="//em" withinText="yes"/></its:rules>"#
    );
    let d = br#"<doc><item title="Hello"><p>A <em>big</em> day</p><note>skip me</note></item></doc>"#;
    assert_eq!(via("xml.its_rules", &rules, d).unwrap(), ["Hello", "A \u{27E6}g1\u{27E7}big\u{27E6}/g1\u{27E7} day"]);
    let mut bad: Vec<(String, &str)> = vec![
        ("<rules/>".into(), "root"),
        (format!(r#"{ITS_OPEN}<its:translateRule selector="//a[" translate="no"/></its:rules>"#), "invalid XPath"),
        (format!(r#"{ITS_OPEN}<its:translateRule selector="//a[//b]" translate="no"/></its:rules>"#), "not allowed inside a predicate"),
        (format!(r#"{ITS_OPEN}<its:translateRule selector="//a[b[c[d]]]" translate="no"/></its:rules>"#), "two deep"),
        (format!(r#"{ITS_OPEN}<its:translateRule selector="//a" translate="maybe"/></its:rules>"#), "yes or no"),
        (format!("{ITS_OPEN}{}</its:rules>", r#"<its:translateRule selector="//a" translate="no"/>"#.repeat(101)), "more than 100"),
        (format!(r#"<!DOCTYPE x [<!ENTITY a "b">]>{ITS_OPEN}</its:rules>"#), "DTD"), // refused, in the crate's own words
    ];
    for (sel, needle) in [
        ("//*/following::*", "following and preceding"),
        ("//p/preceding-sibling::*", "following and preceding"),
        ("//p[../q]", "not allowed inside a predicate"),
        ("//p[parent::*/q]", "not allowed inside a predicate"),
        ("//p[/doc/q = 'x']", "not allowed inside a predicate"),
        ("//p[count(/doc/*) &gt; 1]", "not allowed inside a predicate"),
    ] {
        bad.push((format!(r#"{ITS_OPEN}<its:translateRule selector="{sel}" translate="no"/></its:rules>"#), needle));
    }
    for ptr in ["//p", "/doc/p", "descendant::p", "../following-sibling::p"] {
        bad.push((format!(r#"{ITS_OPEN}<its:locNoteRule selector="//p" locNotePointer="{ptr}"/></its:rules>"#), "pointer"));
    }
    for (raw, needle) in &bad {
        refused_via("xml.its_rules", raw, d, needle);
    }
    // a relative pointer works, and its budget holds
    let ptr = format!(r#"{ITS_OPEN}<its:locNoteRule selector="//p" locNotePointer="../n"/></its:rules>"#);
    let doc = XmlFilter::new().extract(b"<d><g><p>Save</p><n>Button label</n></g></d>", &with("xml.its_rules", &ptr)).unwrap();
    assert_eq!(doc.units[0].notes[0].text, "Button label");
    let many = format!("<d>{}</d>", "<g><p>t</p><n>x</n></g>".repeat(40));
    let o = with("xml.its_rules", &ptr).with_limits(Limits::new().with_max_units(20));
    match XmlFilter::new().extract(many.as_bytes(), &o) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("locNotePointer"), "{}", e.msg),
        other => panic!("pointer budget not enforced: {:?}", other.map(|d| d.units.len())),
    }
}

#[test]
fn its_rules_option_matches_locos_on_more_inputs() {
    let plain = b"<d><t>a</t><u>b</u></d>";
    // LocOS takes an ITS 1.0 rules file and skips rule elements it does not know
    let v1 = r#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="1.0"><its:translateRule selector="//t" translate="no"/></its:rules>"#;
    assert_eq!(via("xml.its_rules", v1, plain).unwrap(), ["b"]);
    let unknown = format!(r#"{ITS_OPEN}<its:fooRule selector="//t"/></its:rules>"#);
    assert_eq!(via("xml.its_rules", &unknown, plain).unwrap(), ["a", "b"]);
    // rules inside the document come later and win
    let user = format!(r#"{ITS_OPEN}<its:translateRule selector="//p" translate="no"/></its:rules>"#);
    let d = br#"<doc xmlns:its="http://www.w3.org/2005/11/its"><its:rules version="2.0"><its:translateRule selector="//p" translate="yes"/></its:rules><p>Hello</p><q>Q</q></doc>"#;
    assert_eq!(via("xml.its_rules", &user, d).unwrap(), ["Hello", "Q"]);
    // a multilingual file is read by its layout; ITS rules do not apply to it
    let ml = br#"<r><g><t xml:lang="en">Hi</t><t xml:lang="de">Hallo</t></g><g><t xml:lang="en">Bye</t><t xml:lang="de">Tschuess</t></g><g><t xml:lang="en">Yes</t></g></r>"#;
    let no_t = format!(r#"{ITS_OPEN}<its:translateRule selector="//t" translate="no"/></its:rules>"#);
    assert_eq!(via("xml.its_rules", &no_t, ml).unwrap(), ["Hi", "Bye", "Yes"]);
    assert_eq!(via("xml.multilingual", r#"{"enabled": "no"}"#, ml).unwrap(), ["Hi", "Hallo", "Bye", "Tschuess", "Yes"]);
    assert_eq!(via("xml.multilingual", r#"{"source": "de"}"#, ml).unwrap(), ["Hallo", "Tschuess"]);
}

#[test]
fn locos_multilingual_config_through_the_option() {
    let d = data("catalog-attr.xml");
    assert_eq!(
        via("xml.multilingual", r#"{"source": "de"}"#, &d).unwrap(),
        ["Willkommen bei \u{27E6}g1\u{27E7}Northwind\u{27E6}/g1\u{27E7}, {0}!", "Speichern & schliessen"]
    );
    let only = b"<r><i><en>Yes</en></i></r>";
    let fill = |json: &str| {
        let mut doc = XmlFilter::new().extract(only, &with("xml.multilingual", json).with_target_lang(Some("fr".to_string()))).unwrap();
        set(&mut doc, 0, "Oui");
        XmlFilter::new().merge(&doc).unwrap()
    };
    assert_eq!(fill(r#"{"mode": "element", "source": "en"}"#), b"<r><i><en>Yes</en><fr>Oui</fr></i></r>");
    // create = no travels with the document: a merge with a plain filter still adds nothing
    assert_eq!(fill(r#"{"mode": "element", "source": "en", "create": "no"}"#), only);
    refused_via("xml.multilingual", r#"{"source": "pt"}"#, &d, "no source-language");
    let long = format!(r#"{{"source": "{}"}}"#, "x".repeat(30_000));
    for (cfg, needle) in [
        (r#"{"colour": "red"}"#.to_string(), "unknown key"),
        (r#"{"mode": "rows"}"#.into(), "mode must be"),
        (r#"{"source": "en'] | //*"}"#.into(), "not a language code"),
        (r#"{"lang_attr": "a b"}"#.into(), "not an attribute name"),
        (r#"{"create": "maybe"}"#.into(), "create must be yes or no"),
        (r#"{"mode": "xpath", "source": "en", "source_xpath": "//v"}"#.into(), "{lang} placeholder"),
        (r#"{"mode": "xpath", "source_xpath": "//v[@l='{lang}']", "target_xpath": "../v[@l='{lang}']"}"#.into(), "needs a source"),
        (r#"{"source": "en", "source_xpath": "//v[@l='{lang}']", "target_xpath": "//v[@l='{lang}']"}"#.into(), "pointer must be relative"),
        ("{not json".into(), "not valid JSON"),
        ("[1]".into(), "must be an object"),
        (long, "larger than 20 KB"),
        (r#"{"source": 5}"#.into(), "must be text"),
    ] {
        refused_via("xml.multilingual", &cfg, &d, needle);
    }
}

#[test]
fn multilingual_option_is_checked_but_not_applied_without_layouts() {
    // LocOS reads multilingual layouts for generic .xml only; a filter with them off (the DITA and DocBook engines,
    // `no_multilingual()`) still refuses a bad config, as LocOS's constructor does
    let ml = br#"<r><g><t xml:lang="en">Hi</t><t xml:lang="de">Hallo</t></g><g><t xml:lang="en">Bye</t><t xml:lang="de">Tschuess</t></g></r>"#;
    let off = XmlFilter::new().no_multilingual();
    assert_eq!(off.extract(ml, &with("xml.multilingual", r#"{"source": "de"}"#)).unwrap().units.len(), 4);
    assert!(off.extract(ml, &with("xml.multilingual", r#"{"mode": "sideways"}"#)).is_err());
}
