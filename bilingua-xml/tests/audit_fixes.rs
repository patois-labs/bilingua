//! Regression tests for the 0.9.1 security audit: multilingual detection holds each sibling group once, and an
//! XPath step keeps its node set deduplicated and pays for the upward axes.

use bilingua_model::*;
use bilingua_xml::XmlFilter;
use std::time::{Duration, Instant};

fn extract(body: &str, limits: Limits) -> Result<Document, Error> {
    XmlFilter::new().extract(body.as_bytes(), &ExtractOptions::new().with_limits(limits))
}

fn translate_all(doc: &mut Document, target: &str) {
    doc.target_lang = Some(target.to_string());
    for u in &mut doc.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
}

/// `m` siblings of one name alternating en / fr under one parent, plus a second small group so the layout is
/// detected: the big group has m/2 source members.
fn one_big_group(m: usize) -> String {
    let mut s = String::from("<r>");
    for i in 0..m {
        s.push_str(if i % 2 == 0 { "<a xml:lang=\"en\">x</a>" } else { "<a xml:lang=\"fr\">x</a>" });
    }
    s.push_str("<q><b xml:lang=\"en\">x</b><b xml:lang=\"fr\">x</b></q></r>");
    s
}

/// One group with thousands of source members used to be copied once per member (88 KB of input, 276 MB of heap;
/// four times that for twice the input). It is now held once, and the merge walks it once.
#[test]
fn a_big_multilingual_group_is_held_once() {
    let body = one_big_group(40_000);
    let start = Instant::now();
    let mut doc = extract(&body, Limits::new()).expect("extract");
    assert_eq!(doc.units.len(), 20_001);
    let f = XmlFilter::new();
    assert_eq!(f.merge(&doc).expect("untouched merge"), body.as_bytes());
    translate_all(&mut doc, "fr");
    let out = String::from_utf8(f.merge(&doc).expect("merge into fr")).unwrap();
    assert!(start.elapsed() < Duration::from_secs(20), "took {:?}", start.elapsed());
    // every source keeps its text, and the k-th source fills the k-th fr sibling
    assert_eq!(out.matches("<a xml:lang=\"en\">x</a>").count(), 20_000);
    assert_eq!(out.matches("<a xml:lang=\"fr\">[x]</a>").count(), 20_000);
    assert!(out.ends_with("<q><b xml:lang=\"en\">x</b><b xml:lang=\"fr\">[x]</b></q></r>"));
}

/// The merge of a small file writes what it wrote before: an existing target sibling is filled (one per source,
/// in order), a missing one is created after the last member of the group.
#[test]
fn multilingual_merge_targets_are_unchanged() {
    let body = "<r>\n  <g>\n    <a xml:lang=\"en\">One</a>\n    <a xml:lang=\"FR\"/>\n    <a xml:lang=\"en\">Two</a>\n    <a xml:lang=\"de\">Zwei</a>\n  </g>\n  <g>\n    <a xml:lang=\"en\">Three</a>\n    <a xml:lang=\"fr\">Trois</a>\n    <a xml:lang=\"fr\">Encore</a>\n  </g>\n</r>";
    let f = XmlFilter::new();
    let mut doc = extract(body, Limits::new()).expect("extract");
    assert_eq!(doc.units.len(), 3);
    translate_all(&mut doc, "fr");
    let out = String::from_utf8(f.merge(&doc).expect("merge")).unwrap();
    let want = "<r>\n  <g>\n    <a xml:lang=\"en\">One</a>\n    <a xml:lang=\"FR\">[One]</a>\n    <a xml:lang=\"en\">Two</a>\n    <a xml:lang=\"de\">Zwei</a>\n    <a xml:lang=\"FR\">[Two]</a>\n  </g>\n  <g>\n    <a xml:lang=\"en\">Three</a>\n    <a xml:lang=\"fr\">[Three]</a>\n    <a xml:lang=\"fr\">Encore</a>\n  </g>\n</r>";
    assert_eq!(out, want);
}

/// More source-language nodes than the unit limit is refused while the layout is read, with the unit limit's
/// error.
#[test]
fn an_oversized_multilingual_layout_is_a_limit_error() {
    let body = one_big_group(400);
    match extract(&body, Limits::new().with_max_units(50)) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(d) => panic!("expected a limit error, got {} units", d.units.len()),
    }
    assert_eq!(extract(&body, Limits::new().with_max_units(201)).expect("within the limit").units.len(), 201);
}

/// `depth` nested `<n>` around `leaves` text leaves, with one in-file ITS rule set.
fn deep(rules: &str, depth: usize, leaves: usize) -> String {
    let mut s = format!("<r><its:rules xmlns:its=\"http://www.w3.org/2005/11/its\" version=\"2.0\">{rules}</its:rules>");
    s.push_str(&"<n>".repeat(depth));
    for i in 0..leaves {
        s.push_str(&format!("<a>t{i}</a>"));
    }
    s.push_str(&"</n>".repeat(depth));
    s.push_str("</r>");
    s
}

fn rule(selector: &str, translate: &str) -> String {
    format!("<its:translateRule selector=\"{selector}\" translate=\"{translate}\"/>")
}

fn unit_texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| u.source().iter().map(|i| if let Inline::Text(t) = i { t.clone() } else { "#".into() }).collect()).collect()
}

/// A step whose context nodes reach the same nodes over and over (every leaf has the same 100 ancestors, every
/// `<n>` the same descendants) builds its result through the deduplicating path. It must select exactly what a
/// selector does that names the same nodes from a single context node.
#[test]
fn a_deduplicated_step_selects_the_same_nodes() {
    let (depth, leaves) = (100, 3000);
    // every leaf's ancestors: 3000 context nodes, 101 results each, 101 distinct
    let via_ancestors = extract(&deep(&(rule("//a/ancestor::*", "no") + &rule("//a", "yes")), depth, leaves), Limits::new()).expect("ancestor");
    let control = extract(&deep(&(rule("/* | //n", "no") + &rule("//a", "yes")), depth, leaves), Limits::new()).expect("control");
    assert_eq!(via_ancestors.units.len(), leaves);
    assert_eq!(unit_texts(&via_ancestors), unit_texts(&control));
    // every element under another: 3100 context nodes, the leaves found 100 times over
    let via_descendants = extract(&deep(&(rule("//*//*", "no") + &rule("//a[position() mod 2 = 0]", "yes")), depth, leaves), Limits::new()).expect("descendant");
    let control = extract(&deep(&(rule("/*//*", "no") + &rule("//a[position() mod 2 = 0]", "yes")), depth, leaves), Limits::new()).expect("control");
    assert_eq!(via_descendants.units.len(), leaves / 2);
    assert_eq!(unit_texts(&via_descendants), unit_texts(&control));
}

/// The ancestor axes used to cost nothing, so a chain of them could walk depth times elements nodes per step
/// without limit. They now draw on the rule's budget: an ordinary use passes, a chain over a deep file is refused
/// with an error (not a panic, and not an unbounded walk).
#[test]
fn the_ancestor_axes_are_charged() {
    let ordinary = deep(&rule("//a/ancestor::n/parent::*", "no"), 20, 2000);
    assert!(extract(&ordinary, Limits::new()).is_ok());
    let chain = "//*/ancestor-or-self::*/ancestor-or-self::*/ancestor-or-self::*/ancestor-or-self::*";
    let start = Instant::now();
    let r = extract(&deep(&rule(chain, "no"), 250, 20_000), Limits::new());
    assert!(matches!(&r, Err(e) if e.to_string().contains("too expensive")), "{:?}", r.map(|d| d.units.len()));
    assert!(start.elapsed() < Duration::from_secs(20), "took {:?}", start.elapsed());
}

/// Resource limit: a descendant step from every element of a deep document walked the tree once per level, so
/// `//*//*//*` over 250 levels ran out of the rule's step budget. Elements inside another context element are now
/// skipped on that step (no predicate looks at a position), so the walk is linear and selects the same nodes.
#[test]
fn chained_descendant_steps_walk_the_tree_once() {
    let (depth, leaves) = (250, 20_000);
    let start = Instant::now();
    let chained = extract(&deep(&(rule("//*//*//*", "no") + &rule("//a[position() mod 2 = 0]", "yes")), depth, leaves), Limits::new()).expect("chained descendants");
    let control = extract(&deep(&(rule("/*//*", "no") + &rule("//a[position() mod 2 = 0]", "yes")), depth, leaves), Limits::new()).expect("control");
    assert_eq!(chained.units.len(), leaves / 2);
    assert_eq!(unit_texts(&chained), unit_texts(&control));
    // a positional predicate still sees each context node's own axis: from every <n>, the first descendant is its
    // first child, which a single context node (the outer <n>) does not give for the inner ones
    let sel = |s: &str| unit_texts(&extract(&deep(&(rule("//*", "yes") + &rule(s, "no")), 3, 4), Limits::new()).expect(s));
    assert_eq!(sel("//n/descendant::*[1]"), sel("//n/*[1]"));
    assert_ne!(sel("//n/descendant::*[1]"), sel("/r/n/descendant::*[1]"));
    assert!(start.elapsed() < Duration::from_secs(20), "took {:?}", start.elapsed());
}
