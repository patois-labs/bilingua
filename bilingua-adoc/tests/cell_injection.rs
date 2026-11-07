//! #788: a translated table cell cannot change the table structure or start a preprocessor directive.
mod common;
use common::*;

fn index_of(doc: &str, text: &str) -> usize {
    units(doc.as_bytes()).iter().position(|t| t == text).unwrap_or_else(|| panic!("no unit {text:?}"))
}

#[test]
fn nested_cell_escapes_both_separators() {
    let doc = "|===\na|\n!===\n!Widget\n!===\n|===\n";
    let out = build_str(doc.as_bytes(), &[(index_of(doc, "Widget"), "a | b ! c")]);
    assert!(out.contains(r"!a \| b \! c"), "{out}");
}

#[test]
fn outer_cell_escapes_only_its_own_separator() {
    let doc = "|===\n|Widget\n|===\n";
    let out = build_str(doc.as_bytes(), &[(index_of(doc, "Widget"), "a | b ! c")]);
    assert!(out.contains(r"|a \| b ! c"), "{out}");
}

#[test]
fn cell_at_line_start_gets_the_empty_guard() {
    let doc = "|===\n|first\nWidget\n|===\n";
    let i = index_of(doc, "Widget");
    for evil in ["ifdef::x[]", "include::other.adoc[]", "// hidden", "endif::[]"] {
        let out = build_str(doc.as_bytes(), &[(i, evil)]);
        assert!(out.contains(&format!("{{empty}}{evil}")), "{evil}: {out}");
        assert!(!out.lines().any(|l| l.starts_with(evil)), "{evil}: {out}");
    }
}

#[test]
fn cell_after_a_separator_is_not_guarded() {
    let doc = "|===\n|Widget\n|===\n";
    let out = build_str(doc.as_bytes(), &[(index_of(doc, "Widget"), "include::x[]")]);
    assert!(out.contains("|include::x[]\n"), "{out}");
}
