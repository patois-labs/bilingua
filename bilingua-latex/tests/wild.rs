//! Regressions found by running real files from the Internet through the pseudo round trip.
mod common;
use common::*;

#[test]
fn begin_document_inside_an_example_is_not_the_document() {
    // a chapter file of a manual: no preamble, and an example that shows a whole document
    let src = b"First paragraph of the chapter.\n\n\\begin{example}\n\\begin{document}\nHello\n\\end{document}\n\\end{example}\n\nLast paragraph.\n";
    let u = units(src);
    assert!(u.iter().any(|t| t == "First paragraph of the chapter."), "{u:?}");
    assert!(u.iter().any(|t| t == "Last paragraph."), "{u:?}");
}

#[test]
fn begin_document_in_a_verbatim_environment_is_skipped() {
    let src = b"Intro text.\n\n\\begin{minted}{latex}\n\\begin{document}\n\\end{minted}\n\nMore text.\n";
    let u = units(src);
    assert!(u.iter().any(|t| t == "Intro text."), "{u:?}");
    assert!(u.iter().any(|t| t == "More text."), "{u:?}");
}

#[test]
fn a_real_preamble_still_ends_at_begin_document() {
    let src = b"\\documentclass{article}\n\\title{Hello}\n\\begin{document}\nBody.\n\\end{document}\n";
    let u = units(src);
    assert_eq!(u, vec!["Hello".to_string(), "Body.".to_string()]);
}

#[test]
fn text_is_not_glued_to_a_control_word_before_it() {
    let src = b"\\medskip\\noindent\nTo see this.\n";
    let out = String::from_utf8(build(src, &[(0, "ZqTo see this.qZ")])).unwrap();
    assert!(!out.contains("\\medskipZq"), "{out}");
    assert!(out.contains("Zq"), "{out}");
}

#[test]
fn text_is_not_glued_to_a_trailing_control_space() {
    let src = b"Compare vs.\\\nnext line.\n";
    let doc = extract(src).unwrap();
    let n = doc.units.len();
    assert!(n >= 1);
    let out = String::from_utf8(build(src, &[(0, "Zqvs.\u{27E6}x1\u{27E7}qZ")])).unwrap_or_default();
    assert!(!out.contains("\\qZ"), "{out}");
}
