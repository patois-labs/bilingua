//! D4 gate: translations that try to inject markup in every HTML context, the stored-document schema, and the scan
//! paths that must stay linear on hostile pages.
use bilingua_html::{Html, HtmlFilter};
use bilingua_htmlcore::browser::browser_view;
use bilingua_model::*;
use std::time::Instant;

fn open(f: &HtmlFilter, page: &str) -> Document {
    f.extract(page.as_bytes(), &ExtractOptions::default()).unwrap()
}

/// Every segment's target: `t(source text)`, its tags kept in place.
fn translate_all(doc: &mut Document, t: impl Fn(&str) -> String) {
    let ids: Vec<(String, Vec<Inline>)> = doc
        .segments()
        .map(|(_, s)| (s.id.clone(), s.source.iter().map(|i| if let Inline::Text(x) = i { Inline::Text(t(x)) } else { i.clone() }).collect()))
        .collect();
    for (id, v) in ids {
        doc.set_target(&id, v).unwrap();
    }
}

/// The (name, attribute names) of every start tag a browser sees, in all four modes.
fn tags(page: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for (sc, sel) in [(true, false), (false, true), (false, false), (true, true)] {
        for st in browser_view(page, sc, sel).starts {
            out.push((st.name.clone(), st.attrs.iter().map(|a| a.0.clone()).collect()));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A merge either refuses or gives a browser no element or attribute name the source does not have.
fn assert_no_injection(f: &HtmlFilter, page: &str, payload: &str) {
    let mut d = open(f, page);
    translate_all(&mut d, |s| format!("{s}{payload}"));
    let Ok(out) = f.merge(&d) else { return };
    let out = String::from_utf8(out).unwrap();
    let before = tags(page);
    for t in tags(&out) {
        assert!(before.contains(&t), "new tag {t:?} in\n{out}\nfrom page\n{page}\npayload {payload}");
    }
}

const PAYLOADS: &[&str] = &[
    "</title><script>alert(1)</script>",
    "</textarea><img src=x onerror=alert(1)>",
    "\"><img src=x onerror=alert(1)>",
    "' onmouseover='alert(1)",
    "\" onmouseover=\"alert(1)",
    " onmouseover=alert(1) x=",
    "`onmouseover=alert(1)",
    "<!--",
    "-->",
    "]]><script>alert(1)</script>",
    "&lt;script&gt;alert(1)&lt;/script&gt;",
    "<![CDATA[<script>alert(1)</script>]]>",
    "</xmp><img src=x onerror=alert(1)>",
    "</noscript><img src=x onerror=alert(1)>",
    "{{ '<script>alert(1)</script>' }}",
];

const PAGES: &[&str] = &[
    "<!DOCTYPE html><html><head><title>Hello title</title><meta name=\"description\" content=\"About us\"></head><body><p>Para <b>bold</b> text</p><textarea>Area text</textarea><img src=\"a.png\" alt=\"A cat\"><a href=\"/\" title='Home page'>Link</a><option>Choice</option></body></html>",
    // a page whose markup a browser reads differently from the scanner (quotes must be escaped everywhere)
    "<p>x</p><!--><img alt=\"Real one\">--> <p>Text \"here\"</p><a title=\"Go now\">y</a>",
    "<svg><style><a title=\"</style><img alt='Inner'>\"></a></svg><p>Plain text</p>",
    "<xmp><p>not a tag</p></xmp><p>After xmp</p><noscript><p>Script off</p></noscript>",
    "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Xhtml text<br/>more</p><![CDATA[ x ]]></body></html>",
    "<p>Template {{ user.name }} and {% if a %}x{% endif %} end</p><img alt=\"{{ alt }} picture\">",
    "<select><option>One</option><textarea>T</textarea></select><p>Para</p>",
    "<p class=x title=unquoted>Unquoted attrs</p><img alt=bare src=x>",
];

#[test]
fn translations_cannot_add_markup_in_any_context() {
    let template = regex::Regex::new(r"(?s)\{\{.*?\}\}|\{%.*?%\}").unwrap();
    for page in PAGES {
        for payload in PAYLOADS {
            assert_no_injection(&HtmlFilter::new(), page, payload);
            assert_no_injection(&HtmlFilter::new().with_page_meta(false), page, payload);
            assert_no_injection(&HtmlFilter::new().with_template_pattern(template.clone()), page, payload);
        }
    }
}

#[test]
fn attribute_targets_cannot_break_out() {
    let page = "<p>Hi</p><img src=\"a.png\" alt=\"A cat\"><img src='b.png' alt='A dog'><a href=\"/\" title=\"Home\">x</a>";
    let mut d = open(&HtmlFilter::new(), page);
    translate_all(&mut d, |s| format!("{s}\" onerror=\"alert(1)' onload='alert(2)"));
    let out = String::from_utf8(Html.merge(&d).unwrap()).unwrap();
    assert!(!tags(&out).iter().any(|(_, a)| a.iter().any(|n| n.starts_with("on"))), "{out}");
}

#[test]
fn reordered_and_dropped_codes_add_nothing() {
    let page = "<p>One <b>two</b> <a href=\"/x\">three</a> <i>four</i></p>";
    let mut d = open(&HtmlFilter::new(), page);
    let (id, src) = d.segments().next().map(|(_, s)| (s.id.clone(), s.source.clone())).unwrap();
    let mut rev = src.clone();
    rev.reverse();
    if d.set_target(&id, rev).is_ok() {
        if let Ok(out) = Html.merge(&d) {
            let out = String::from_utf8(out).unwrap();
            for t in tags(&out) {
                assert!(tags(page).contains(&t), "{out}");
            }
        }
    }
}

#[test]
fn an_xhtml_page_declaring_markup_entities_is_refused_at_extract() {
    // an XML reader (a browser reading XHTML, an EPUB reader) would expand `&evil;` into an element; the B6 guard
    // refuses such a page before any unit is offered (the export check refuses the reference too, see htmlcore's
    // xml.rs tests)
    let page = "<?xml version=\"1.0\"?>
<!DOCTYPE html [<!ENTITY evil \"<img src='x' onerror='alert(1)'/>\">]>
<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello world</p></body></html>";
    assert!(matches!(Html.extract(page.as_bytes(), &ExtractOptions::default()), Err(Error::Limit(_))));
}

#[test]
fn a_schema_1_document_with_units_is_refused_with_a_typed_error() {
    let page = "<p>Hello <b>there</b></p>";
    let mut d = open(&HtmlFilter::new(), page);
    translate_all(&mut d, |s| format!("{s}!"));
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        parser.as_mut().unwrap().schema = 1;
    }
    assert!(matches!(Html.merge(&d), Err(Error::ParserVersion { .. })));
    // the old layout (unit parts in the skeleton) too, never a panic
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        *parts = vec![SkelPart::Raw("<p>".into()), SkelPart::Unit(0), SkelPart::Raw("</p>".into())];
    }
    assert!(matches!(Html.merge(&d), Err(Error::ParserVersion { .. })));
    // a page without units still merges
    let mut e = open(&HtmlFilter::new(), "<p> </p>");
    assert!(e.units.is_empty());
    if let Skeleton::Text { parser, .. } = &mut e.skeleton {
        parser.as_mut().unwrap().schema = 1;
    }
    assert_eq!(Html.merge(&e).unwrap(), b"<p> </p>");
}

#[test]
fn empty_match_template_patterns_do_not_hang() {
    for re in ["", "x*", r"\b", "(?:)", r"\{\{.*?\}\}|"] {
        let f = HtmlFilter::new().with_template_pattern(regex::Regex::new(re).unwrap());
        let page = "<p>Hello {{ x }} world</p><p>xx yy</p>";
        let mut d = open(&f, page);
        assert_eq!(f.merge(&d).unwrap(), page.as_bytes());
        translate_all(&mut d, |s| format!("{s} <z>"));
        let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
        assert!(!out.contains("<z>"), "{re}: {out}");
    }
}

fn timed(what: &str, secs: u64, f: impl FnOnce()) {
    let t = Instant::now();
    f();
    assert!(t.elapsed().as_secs() < secs, "{what} took {:?}", t.elapsed());
}

#[test]
fn many_template_spans_in_one_unit_merge_in_linear_time() {
    // 10 000 spans in one unit (under the 100 000 character unit limit)
    let spans: String = (0..10_000).map(|i| format!("{{{{{i}}}}}")).collect();
    let page = format!("<p>Start {spans}end</p>");
    let f = HtmlFilter::new().with_template_pattern(regex::Regex::new(r"\{\{.*?\}\}").unwrap());
    let mut d = open(&f, &page);
    translate_all(&mut d, |s| s.to_string() + " Z");
    timed("10 000 template spans", 20, || {
        let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
        assert!(out.contains("{{9999}}end Z"));
    });
}

#[test]
fn many_unclosed_raw_text_elements_merge_in_linear_time() {
    // self-closing, so the depth cap does not refuse it first; a browser still reads each as an RCDATA start
    let page = format!("<p>Hello</p>{}</title>", "<title/>".repeat(40_000));
    let mut d = open(&HtmlFilter::new(), &page);
    translate_all(&mut d, |_| "Bonjour".into());
    timed("40 000 unclosed titles", 20, || {
        let _ = Html.merge(&d);
    });
}

#[test]
fn a_tag_with_thousands_of_attributes_is_linear() {
    let alts: String = (0..20_000).map(|i| format!(" alt=\"Cat {i}\"")).collect();
    let page = format!("<p>Hello</p><img src=x{alts}>");
    timed("20 000 alt attributes", 20, || {
        let mut d = open(&HtmlFilter::new(), &page);
        translate_all(&mut d, |s| format!("{s} Z"));
        let _ = Html.merge(&d);
    });
    let names: String = (0..40_000).map(|i| format!(" a{i}=\"v\"")).collect();
    let page = format!("<p translate=\"yes\"{names}>Hello</p><meta name=\"description\"{names} content=\"About\"><p{names}>Again</p>");
    timed("40 000 attribute names", 20, || {
        let mut d = open(&HtmlFilter::new(), &page);
        translate_all(&mut d, |s| format!("{s} Z"));
        let out = String::from_utf8(Html.merge(&d).unwrap()).unwrap();
        assert!(out.contains("Again Z"));
    });
}
