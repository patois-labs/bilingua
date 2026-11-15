//! Resource limits (security review run 2): the ITS rules of one document share one XPath step budget. Each rule
//! used to get its own budget, so a file with many rules whose predicates stay just under a single rule's budget
//! could spend the rule count times that budget.

use bilingua_model::*;
use bilingua_xml::XmlFilter;

const ITS: &str = "http://www.w3.org/2005/11/its";

/// `rules` translate rules with `selector` in the document, over `n` empty children.
fn doc(selector: &str, rules: usize, n: usize) -> String {
    let mut s = format!("<r xmlns:its=\"{ITS}\"><its:rules version=\"2.0\">");
    for _ in 0..rules {
        s.push_str(&format!("<its:translateRule translate=\"yes\" selector=\"{selector}\"/>"));
    }
    s.push_str("</its:rules>");
    s.push_str(&"<a/>".repeat(n));
    s.push_str("<p>Hello</p></r>");
    s
}

fn extract(body: &str) -> Result<Document, Error> {
    XmlFilter::new().extract(body.as_bytes(), &ExtractOptions::new())
}

/// Forty rules whose predicate evaluates every term of a false `or` chain on every element: each one fits a single
/// rule's budget, together they spend forty times that. The document's shared budget refuses them.
#[test]
fn many_costly_rules_share_one_budget() {
    let terms = vec!["b"; 80].join(" or ");
    let selector = format!("//*[{terms}]");
    let r = extract(&doc(&selector, 40, 5000));
    assert!(matches!(&r, Err(Error::Parse { msg, .. }) if msg.contains("too expensive")), "{:?}", r.map(|d| d.units.len()));
}

/// The same number of ordinary rules over the same document still extracts.
#[test]
fn many_ordinary_rules_still_extract() {
    let d = extract(&doc("//p", 40, 5000)).expect("ordinary rules");
    assert_eq!(d.units.len(), 1);
    let d = extract(&doc("//*[@id = 'x' or self::p]", 40, 5000)).expect("rules with a small predicate");
    assert_eq!(d.units.len(), 1);
}
