//! LocOS tracker #788 (found by a round-trip structure fuzz): `x:: w` (also `:::`, `::::`, `;;`) in a translation
//! must not turn a paragraph, admonition or term into a description list item.
mod common;
use common::*;

fn one(doc: &str, text: &str, t: &str) -> String {
    let k = units(doc.as_bytes()).iter().position(|u| u == text).unwrap_or_else(|| panic!("no unit {text:?}"));
    let out = build_str(doc.as_bytes(), &[(k, t)]);
    assert_eq!(units(out.as_bytes()).len(), units(doc.as_bytes()).len(), "the units stay: {out}");
    out
}

#[test]
fn a_paragraph_does_not_become_a_list_item() {
    let doc = "Intro text.\n\nMore text.\n";
    assert_eq!(one(doc, "Intro text.", "x:: w"), "x{two-colons} w\n\nMore text.\n");
    assert_eq!(one(doc, "Intro text.", "x::: w"), "x{two-colons}: w\n\nMore text.\n");
    assert_eq!(one(doc, "Intro text.", "x:::: w"), "x{two-colons}{two-colons} w\n\nMore text.\n");
    assert_eq!(one(doc, "Intro text.", "x;; w"), "x{two-semicolons} w\n\nMore text.\n");
    assert_eq!(one(doc, "Intro text.", "w x::"), "w x{two-colons}\n\nMore text.\n");
}

#[test]
fn colons_that_are_no_delimiter_stay() {
    let doc = "Intro text.\n\nMore text.\n";
    assert_eq!(one(doc, "Intro text.", "a: b"), "a: b\n\nMore text.\n");
    assert_eq!(one(doc, "Intro text.", "w a::b"), "w a::b\n\nMore text.\n");
    assert_eq!(one(doc, "Intro text.", "w ;:: x"), "w ;{two-colons} x\n\nMore text.\n");
}

#[test]
fn a_term_ending_in_a_colon_keeps_its_delimiter() {
    let doc = "Term one:: Its description.\n";
    assert_eq!(one(doc, "Term one", "w:"), "w:{empty}:: Its description.\n");
    assert_eq!(one(doc, "Term one", "w :::"), "w {two-colons}:{empty}:: Its description.\n");
}

#[test]
fn a_translated_mark_next_to_a_code_mark_stays_apart() {
    // structure fuzz: `*strong*` + `* w` read back as `*strong**` + ` w`
    let doc = "A paragraph with *strong* words.\n";
    let t = units(doc.as_bytes())[0].replace(" words.", "* w");
    let out = one(doc, &units(doc.as_bytes())[0], &t);
    assert_eq!(out, "A paragraph with *strong*&#42; w\n");
    // a translated `[x]` right before a mark would become its attribute list
    let t = units(doc.as_bytes())[0].replace("A paragraph with ", "w [x]");
    let out = one(doc, &units(doc.as_bytes())[0], &t);
    assert!(out.starts_with("w [x&#93;*"), "{out}");
    assert!(units(out.as_bytes())[0].contains('\u{27E6}'), "the mark stays a code: {out}");
}

#[test]
fn a_title_keeps_a_translated_closing_marker() {
    // structure fuzz: `== w ==` reads back as the title `w`
    let doc = "== Section heading\n\nText.\n";
    assert_eq!(one(doc, "Section heading", "w =="), "== w =={empty}\n\nText.\n");
    assert_eq!(one(doc, "Section heading", "w=="), "== w==\n\nText.\n");
}

#[test]
fn admonitions_and_terms_keep_their_kind() {
    let doc = "NOTE: Admonition text.\n\nTerm one:: Its description.\n";
    let out = one(doc, "Admonition text.", "w:: x");
    assert!(out.starts_with("NOTE: w{two-colons} x\n"), "{out}");
    let out = one(doc, "Term one", "a:: b");
    assert!(out.contains("a{two-colons} b:: Its description."), "{out}");
}
