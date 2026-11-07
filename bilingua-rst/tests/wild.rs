//! Regressions found by running real files from the Internet through the pseudo round trip.
mod common;
/// The profile the shared helpers extract with.
const PROFILE: Option<&str> = None;
use common::*;

#[test]
fn literal_block_under_a_bullet_stays_literal() {
    let src = b"* Run it::\n\n      $ tool\n      <doc a>\n          <item>\n\n* Next item.\n";
    assert_eq!(units(src), vec!["Run it".to_string(), "Next item.".to_string()]);
    assert_identity(src);
}

#[test]
fn literal_ending_in_a_backtick_is_one_literal() {
    let src = b"They are written as ``:role:`content```.\n";
    let u = units(src);
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].matches('\u{27E6}').count(), 1, "one code for the whole literal: {u:?}");
    assert_identity(src);
}

#[test]
fn table_cell_does_not_wrap_inside_an_escaped_blank() {
    // the closing `*` is followed by a letter, so the writer adds `\ `; the wrap must not cut it in two
    let src = b"+-------------------------------+-------+\n| when processing match *m* now | other |\n+-------------------------------+-------+\n";
    let out = build_str(src, &[(0, "when processing match \u{27E6}g1\u{27E7}m\u{27E6}/g1\u{27E7}qZ")]);
    assert!(!out.contains("\\\n") && !out.contains("\\ \n") && !out.contains("\\ |"), "{out}");
    assert!(out.contains("qZ"), "{out}");
}

#[test]
fn indented_simple_table_can_be_written() {
    let src = b"Intro:\n\n  =========  ==========\n  name       value\n  =========  ==========\n  Abstract   one\n  Address    two\n  =========  ==========\n\nAfter.\n";
    let u = units(src);
    let k = u.iter().position(|t| t == "Abstract").expect("cell unit");
    let out = build_str(src, &[(k, "Abs")]);
    assert!(out.contains("  Abs        one"), "{out}");
    assert!(out.starts_with("Intro:\n\n  ========="), "{out}");
}

#[test]
fn deeper_continuation_lines_of_an_item_stay_one_paragraph() {
    // LocOS parity: lines that continue the item's first paragraph are not cut into a definition list
    let src = b"- First line of the item\n    goes on here\n";
    assert_eq!(units(src), vec!["First line of the item goes on here".to_string()]);
    assert_identity(src);
}

#[test]
fn other_backtick_runs_close_at_the_first_pair() {
    // LocOS parity: only a run of exactly three backticks ends the literal at its end
    let u = units(b"See ``lit```x`:py:class: here.\n");
    assert_eq!(u, vec!["See \u{27E6}x1\u{27E7}`x`:py:class: here.".to_string()]);
    let u = units(b"See ``a`b````lit`` here.\n");
    assert_eq!(u, vec!["See \u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7} here.".to_string()]);
}

#[test]
fn backslash_that_ends_a_line_is_an_escape() {
    use bilingua_model::{Filter, Inline, SegState};
    // `\ ` after a bold run at the end of a line prints nothing: an escape code, not text (native profile)
    let src = b"Some text here. **Alpha, Beta**\\ \n\nNext one.\n";
    let u = units(src);
    assert!(u[0].ends_with('\u{27E7}') && !u[0].contains('\\'), "the unit ends in the escape code: {u:?}");
    assert_eq!(u[0].matches('\u{27E6}').count(), 3, "the strong pair and the escape: {u:?}");
    assert_identity(src);
    let mut d = extract(src).unwrap();
    let mut t = d.units[0].source();
    t.insert(0, Inline::text("Zq"));
    d.units[0].segments[0].target = t;
    d.units[0].segments[0].state = SegState::Draft;
    let out = String::from_utf8(bilingua_rst::Rst.merge(&d).unwrap()).unwrap();
    assert!(out.contains("**Alpha, Beta**\\ "), "{out:?}");
    assert!(!out.contains("\\\\"), "no visible backslash: {out:?}");
    let back = extract(out.as_bytes()).unwrap();
    assert!(back.units[0].source().iter().all(|i| !matches!(i, Inline::Text(t) if t.contains('\\'))), "{out:?}");
}

#[test]
fn backslash_that_ends_a_line_stays_text_under_the_locos_profile() {
    use bilingua_model::{ExtractOptions, Filter, Granularity};
    let o = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_profile("locos");
    let d = bilingua_rst::Rst.extract(b"a\\ \n", &o).unwrap();
    assert_eq!(d.units[0].source(), vec![bilingua_model::Inline::text("a\\")]);
}
