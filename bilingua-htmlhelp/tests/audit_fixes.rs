//! Regression tests for the 0.9.1 security audit: the export check's browser view finds the element an end tag
//! closes without walking the open-element stack, and the merge looks edited spans up in a set.
mod common;
use bilingua_htmlhelp::*;
use bilingua_model::*;
use common::*;
use std::time::{Duration, Instant};

const HEAD: &str = "<HTML><BODY><UL><LI><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"Alpha\"></OBJECT></UL>";

/// Extracts, translates the one entry and merges; the time of all three and the built file.
fn translated(data: &str, mo: MergeOptions) -> (Duration, String) {
    let t = Instant::now();
    let out = merge_with(&HhcFilter, data.as_bytes(), &[(0, "Beta")], None, mo).expect("merge");
    (t.elapsed(), String::from_utf8(out).expect("ASCII output"))
}

/// Many open elements and then as many end tags that close none of them: each end tag used to walk the whole
/// stack (17 s for 16 000 in a debug build). The same with the open elements in SVG, below an SVG integration
/// point, and with a `<select>` that is open by name only (the legacy select rule looked for it on the stack).
#[test]
fn end_tags_do_not_walk_the_open_elements() {
    let n = 40_000;
    let cases = [
        ("stray end tags", format!("{HEAD}{}{}", "<b>".repeat(n), "</zz>".repeat(n))),
        ("stray end tags in svg", format!("{HEAD}<svg>{}{}", "<g>".repeat(n), "</zz>".repeat(n))),
        ("stray end tags below foreignObject", format!("{HEAD}<svg><foreignObject>{}{}", "<b>".repeat(n), "</zz>".repeat(n))),
        ("html end tags in svg", format!("{HEAD}{}<svg>{}{}", "<b>".repeat(n / 2), "<g>".repeat(n / 2), "</zz>".repeat(n))),
        ("select closed by another end tag", format!("{HEAD}{}{}", "<i>".repeat(n), "<b><select></b><input>".repeat(n / 4))),
    ];
    for (what, data) in cases {
        let (took, out) = translated(&data, MergeOptions::default());
        assert!(out.contains("value=\"Beta\""), "{what}: the translation is written");
        assert!(took < Duration::from_secs(5), "{what}: {took:?}");
    }
}

/// A contents file leaves every `<LI>` open, so the stack is as deep as the list is long: that is an ordinary file
/// and is not refused for its depth.
#[test]
fn a_long_flat_list_of_open_items_still_merges() {
    let mut data = String::from("<HTML><BODY><UL>\n");
    for i in 0..20_000 {
        data.push_str(&format!("<LI><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"Topic {i}\"></OBJECT>\n"));
    }
    data.push_str("</UL></BODY></HTML>\n");
    let doc = extract(&HhcFilter, data.as_bytes()).expect("extract");
    assert_eq!(doc.units.len(), 20_000);
    assert_eq!(HhcFilter.merge(&doc).expect("untouched merge"), data.as_bytes());
    let (took, out) = translated(&data, MergeOptions::default());
    assert!(out.contains("value=\"Beta\"") && out.contains("value=\"Topic 19999\""));
    assert!(took < Duration::from_secs(5), "{took:?}");
}

/// The end-tag rules themselves are unchanged: an end tag closes the nearest open element of its name, not through
/// an integration point, and a file a browser would read differently after translation is still refused.
#[test]
fn the_export_check_reads_end_tags_as_before() {
    // </b> inside <desc> must not close the <b> outside the SVG
    let inside = format!("{HEAD}<b><svg><desc></b><i></i></desc></svg></b>");
    assert!(translated(&inside, MergeOptions::default()).1.contains("value=\"Beta\""));
    // a foreign end tag closes its element through other foreign elements
    let foreign = format!("{HEAD}<svg><g><path><circle></g><g></g></svg><p>x</p>");
    assert!(translated(&foreign, MergeOptions::default()).1.contains("value=\"Beta\""));
    // the guard still refuses a translation that a browser would read as new markup
    let hidden = "<HTML><BODY><UL><LI><OBJECT type=\"text/sitemap\"><textarea><param name=\"Name\" value=\"Alpha\"></OBJECT></UL>";
    let r = merge_with(&HhcFilter, hidden.as_bytes(), &[(0, "</textarea><img src=x onerror=alert(1)>")], None, MergeOptions::default());
    match r {
        Ok(out) => assert!(!String::from_utf8_lossy(&out).contains("<img"), "markup from a translation was written"),
        Err(Error::Unwritable { .. }) => {}
        Err(e) => panic!("unexpected error: {e}"),
    }
}

/// One See Also span per tag, each of them edited: the pass that fixes untranslated values for the code page used
/// to scan the edit list once per span (over 5 s for 160 000 in a release build). It now costs about what the
/// UTF-8 merge, which never ran that pass, costs.
#[test]
fn edited_spans_are_looked_up_in_a_set() {
    let m = 120_000;
    let data = format!(
        "<HTML><BODY><UL><LI><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"Alpha\">{}</OBJECT></UL>",
        "<param name=\"See Also\" value=\"Alpha\">".repeat(m)
    );
    let (control, out_utf8) = translated(&data, MergeOptions::new().with_output_encoding(OutputEncoding::Utf8));
    let (keep, out) = translated(&data, MergeOptions::default());
    assert_eq!(out.matches("value=\"Beta\"").count(), m + 1);
    assert_eq!(out_utf8.matches("value=\"Beta\"").count(), m + 1);
    assert!(keep < control * 3 + Duration::from_secs(2), "keep {keep:?}, UTF-8 control {control:?}");
}

/// Resource limit: SVG lets `<style>` nest, and each level's body holds the text of every level inside it, so the
/// export check's browser view held the file about n/2 times over (n levels). The bodies now stop at twice the file
/// plus 1 MiB, and a file over that is refused at export; a few levels still merge.
#[test]
fn nested_svg_style_bodies_are_refused_not_copied() {
    let n = 20_000;
    let data = format!("{HEAD}<svg>{}x{}</svg>", "<style>".repeat(n), "</style>".repeat(n));
    let t = Instant::now();
    let r = merge_with(&HhcFilter, data.as_bytes(), &[(0, "Beta")], None, MergeOptions::default());
    assert!(matches!(&r, Err(Error::Unwritable { reason, .. }) if reason.contains("nested inside each other")), "{:?}", r.map(|b| b.len()));
    assert!(t.elapsed() < Duration::from_secs(20), "took {:?}", t.elapsed());
    let (_, out) = translated(&format!("{HEAD}<svg><style><style>a{{}}</style></style></svg>"), MergeOptions::default());
    assert!(out.contains("value=\"Beta\""));
}
