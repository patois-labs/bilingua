//! The rule-driven API later XML formats (DITA, DocBook, others) configure: a made-up vocabulary built from a
//! `Dialect`, ITS rules for notes, the locale attribute and auto-detection among several dialects.

mod common;

use bilingua_model::*;
use bilingua_xml::{Detect, Dialect, ItsRules, LangStamp, XmlFilter};
use common::*;

fn recipe() -> Dialect {
    let its = ItsRules::parse(
        br#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:locNoteRule selector="//warning"><its:locNote>Safety text: keep the tone.</its:locNote></its:locNoteRule></its:rules>"#,
    )
    .unwrap();
    Dialect::new("recipe")
        .no_translate(["sku code"])
        .inline(["b i br code"])
        .nested(["footnote"])
        .preserve(["pre"])
        .translatable_attr("title", ["step"])
        .translatable_attr("alt", Vec::<&str>::new())
        .alt_attrs(["alt"])
        .translate_attr("tr")
        .no_translate_if_attr(["ref"])
        .lang(LangStamp::new().with_update(["locale"]).with_add("locale"))
        .detect(Detect::new().with_root_names(["recipe"]))
        .its_rules(its)
}

const DOC: &str = r#"<recipe locale="en"><title>Soup</title>
<sku>S-1</sku>
<step title="Boil">Boil the <b>water</b> for <code>10</code> minutes.<br/>Stir.<footnote>Use a lid.</footnote></step>
<pre>a   b
c</pre>
<warning>Hot!</warning>
<p tr="no">Keep</p><p ref="x">Linked</p>
<img alt="A pot"/>
</recipe>"#;

#[test]
fn a_dialect_decides_what_is_a_unit() {
    let f = XmlFilter::with_id("recipe", &["rcp"]).dialect(recipe());
    let doc = ex(&f, DOC.as_bytes());
    let t: Vec<String> = doc.units.iter().map(show).collect();
    assert_eq!(
        t,
        vec![
            "Soup",
            "Boil",
            "Boil the \u{27E6}g1\u{27E7}water\u{27E6}/g1\u{27E7} for \u{27E6}x2\u{27E7} minutes.\u{27E6}x3\u{27E7}Stir.\u{27E6}x4\u{27E7}",
            "Use a lid.",
            "a   b\nc",
            "Hot!",
            "A pot",
        ]
    );
    assert_eq!(doc.units[1].context.style.as_deref(), Some("attr:title"));
    assert_eq!(doc.units[6].context.style.as_deref(), Some("a11y:img-alt"));
    assert_eq!(doc.units[5].notes[0].text, "Safety text: keep the tone.");
    // the locale attribute is updated on a target-language merge
    let mut d = ex_to(&f, DOC.as_bytes(), "fr-CA");
    set(&mut d, 5, "Chaud!");
    let out = s(&f.merge(&d).unwrap());
    assert!(out.starts_with(r#"<recipe locale="fr-CA">"#) && out.contains("<warning>Chaud!</warning>"), "{out}");
    assert_eq!(s(&f.merge(&ex(&f, DOC.as_bytes())).unwrap()), DOC);
}

#[test]
fn dialects_are_recognised_in_order() {
    let f = XmlFilter::with_id("any", &["xml"]).auto_dialects(vec![Dialect::dita(), recipe(), Dialect::docbook()]);
    assert_eq!(f.inspect(DOC.as_bytes()).unwrap().dialect.as_deref(), Some("recipe"));
    assert_eq!(f.inspect(&data("task.dita")).unwrap().dialect.as_deref(), Some("dita"));
    assert_eq!(f.inspect(&data("book5.dbk")).unwrap().dialect.as_deref(), Some("docbook"));
    assert_eq!(f.inspect(b"<other>x</other>").unwrap().dialect, None);
    // the generic filter knows DITA and DocBook and nothing else
    assert_eq!(XmlFilter::new().inspect(DOC.as_bytes()).unwrap().dialect, None);
    assert_eq!(XmlFilter::new().no_dialect().inspect(&data("task.dita")).unwrap().dialect, None);
}

#[test]
fn rules_given_to_the_filter_apply_after_the_dialect() {
    let user = ItsRules::parse(br#"<its:rules xmlns:its="http://www.w3.org/2005/11/its" version="2.0"><its:translateRule selector="//title" translate="no"/></its:rules>"#).unwrap();
    let f = XmlFilter::with_id("recipe", &["rcp"]).dialect(recipe()).its_rules(user);
    let t = texts(&f, DOC.as_bytes());
    assert!(!t.contains(&"Soup".to_string()) && t.contains(&"Boil".to_string()), "{t:?}");
    // sniffing: well-formed XML only
    assert!(f.sniff(DOC.as_bytes()) && !f.sniff(b"key = value") && !f.sniff(b"<a>"));
}
