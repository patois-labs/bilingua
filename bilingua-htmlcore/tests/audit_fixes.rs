//! Regression tests for run 2 of the security review: the browser view behind every HTML, server-page and email
//! export check reads each comment once and closes foreign elements through an index.

use bilingua_htmlcore::browser::{browser_view, browser_views};
use std::time::Instant;

/// Seconds `f` takes.
fn secs(f: impl FnOnce()) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64()
}

/// G3-2: comments that each end with `--!>` in a file with no `-->`: each comment used to search to the end of the
/// file for `-->`. The view now costs about what a page of as many ordinary tags costs.
#[test]
fn browser_view_comment_bang_endings_linear() {
    let n = 150_000;
    let attack = format!("<p>Hello world</p>{}", "<!--a--!>".repeat(n));
    let control = format!("<p>Hello world</p>{}", "<b>a</b>x".repeat(n));
    let c = secs(|| {
        browser_views(&control);
    });
    let a = secs(|| {
        let v = browser_views(&attack);
        assert!(v.iter().all(|v| v.starts.iter().any(|s| s.name == "p")));
    });
    assert!(a < c * 5.0 + 2.0, "comments {a:.2} s, control {c:.2} s");
    // a comment still ends at whichever of `-->` and `--!>` comes first
    let v = browser_view("<!-- a --!><b>x</b><!-- c --><i>y</i><!-- d -- --><u>z</u>", true, false);
    let names: Vec<&str> = v.starts.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["b", "i", "u"]);
}

/// G3-3: in SVG, many start tags of names that are end-tag-optional in HTML, then many end tags of a block that is
/// not open: each end tag used to walk every open foreign element.
#[test]
fn browser_view_foreign_end_tags_linear() {
    let n = 60_000;
    let attack = format!("<p>Hello world</p><svg>{}{}", "<caption>".repeat(n), "</div>".repeat(n));
    let control = format!("<p>Hello world</p><svg>{}{}", "<caption>".repeat(n), "<g/>".repeat(n));
    let c = secs(|| {
        browser_views(&control);
    });
    let a = secs(|| {
        browser_views(&attack);
    });
    assert!(a < c * 5.0 + 2.0, "end tags {a:.2} s, control {c:.2} s");
    // the end tags close as before: a foreign end tag closes its element through other foreign elements, and an
    // HTML element above them is closed the HTML way
    let v = browser_view("<svg><g><path><circle></g><text>t</text></svg><p>x</p>", true, false);
    let found: Vec<(&str, &str)> = v.starts.iter().map(|s| (s.name.as_str(), s.ns)).collect();
    assert_eq!(found, [("svg", "svg"), ("g", "svg"), ("path", "svg"), ("circle", "svg"), ("text", "svg"), ("p", "html")]);
    let v = browser_view("<b><svg><g></b><i>after</i>", true, false);
    assert_eq!(v.starts.last().map(|s| (s.name.as_str(), s.ns)), Some(("i", "html")));
}
