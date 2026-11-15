//! Regression tests for the 0.9.1 security audit.
use bilingua_model::*;
use bilingua_serverpages::Dialect;
use std::time::{Duration, Instant};

fn aspx() -> Box<dyn Filter> {
    Dialect::from_extension("aspx").unwrap().filter()
}

/// Extracts, checks the untouched merge is exact, translates the first unit and merges again; returns the time.
fn open_and_merge(page: &str) -> Duration {
    let f = aspx();
    let t = Instant::now();
    let mut doc = f.extract(page.as_bytes(), &ExtractOptions::default()).expect("the page opens");
    assert_eq!(f.merge(&doc).unwrap(), page.as_bytes(), "untouched merge must be exact");
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Bonjour")]).unwrap();
    let out = String::from_utf8(f.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("Bonjour"));
    t.elapsed()
}

/// Finding check_tag:tag_is_clean-cubic-scan: a tag of n attribute values and n code blocks cost n * n * n steps
/// (4 seconds at n = 1200 in a release build). It is linear now, at extract and at merge.
#[test]
fn many_codes_between_many_attributes_is_linear() {
    let n = 20_000;
    let page = format!("<html><body><p>Hello world</p><a {}{}>x</a></body></html>", "a=1 ".repeat(n), "<% %> ".repeat(n));
    let t = open_and_merge(&page);
    assert!(t < Duration::from_secs(10), "took {t:?}");
}

/// The same finding, the `bare_code_ok` half: every code directly follows a quoted value.
#[test]
fn a_code_after_every_quoted_value_is_linear() {
    let n = 20_000;
    let page = format!("<html><body><p>Hello world</p><a {}>x</a></body></html>", "a=\"1\"<% %> ".repeat(n));
    let t = open_and_merge(&page);
    assert!(t < Duration::from_secs(10), "took {t:?}");
}

/// The answers did not change: code between attributes of a tag that reads cleanly is accepted, code inside a value
/// is accepted, and a tag with a stray quote outside its values is still refused.
#[test]
fn tag_check_answers_are_unchanged() {
    let f = aspx();
    let o = ExtractOptions::default();
    for ok in [
        "<p>Hello</p><a href=\"x\" <% %> title='t' <% %>>x</a>",
        "<p>Hello</p><a href=\"<% %>\" class=a<% %>b>x</a>",
        "<p>Hello</p><a href=\"x\"<% %> id=y>x</a>",
    ] {
        let doc = f.extract(ok.as_bytes(), &o).unwrap_or_else(|e| panic!("{ok}: {e:?}"));
        assert_eq!(f.merge(&doc).unwrap(), ok.as_bytes());
    }
    for bad in ["<p>Hello</p><a <% %>=\"x\">x</a>", "<p>Hello</p><a href=\"x\" <% %> b\"c d=\"e\">x</a>", "<p>Hello</p><zz<% %> href=\"x\">x</zz>"] {
        assert!(matches!(f.extract(bad.as_bytes(), &o), Err(Error::Parse { .. })), "{bad}");
    }
}

/// Run 2 G3-4: many tag-library script elements with no end tag, each with a new prefix. Each one used to scan the
/// rest of the page for its end tag (and parse every tag on the way). The end tags are now indexed once.
#[test]
fn serverpages_many_prefixed_scripts_linear() {
    let n = 30_000;
    let jsp = Dialect::from_extension("jsp").unwrap().filter();
    let secs = |page: &str| {
        let t = Instant::now();
        let _ = jsp.extract(page.as_bytes(), &ExtractOptions::default());
        t.elapsed().as_secs_f64()
    };
    let attack: String = format!("<html><body><p>Hello world</p>{}</body></html>", (0..n).map(|i| format!("<p{i}:script>")).collect::<String>());
    let control: String = format!("<html><body><p>Hello world</p>{}</body></html>", (0..n).map(|i| format!("<p{i}:b>")).collect::<String>());
    let c = secs(&control);
    let a = secs(&attack);
    assert!(a < c * 5.0 + 3.0, "scripts {a:.2} s, control {c:.2} s");
    // a closed one is still one opaque run, and an unclosed one is read as HTML
    let f = Dialect::from_extension("jsp").unwrap().filter();
    let doc = f.extract(b"<p>Hello world</p><s:script>var a = 'Not text';</s:script><t:script><p>Kept text</p>", &ExtractOptions::default()).unwrap();
    let texts: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert!(texts.iter().any(|t| t == "Hello world") && texts.iter().any(|t| t == "Kept text"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("Not text")), "{texts:?}");
}
