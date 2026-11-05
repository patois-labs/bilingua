//! Opus gate findings (11-05): a JSP expression joined across a dropped or empty code, code repeated through a
//! duplicated tag, and the JSP hint of an `.html` page read with the word boundaries of the LocOS pattern.
mod common;
use bilingua_model::*;
use bilingua_serverpages::{server_page_for_html, Dialect};
use common::*;

fn merge_target(fmt: &str, src: &str, unit: usize, target: Vec<Inline>) -> String {
    let f = filter(fmt);
    let mut doc = f.extract(src.as_bytes(), &opts()).unwrap();
    doc.units[unit].segments[0].target = target;
    String::from_utf8(f.merge(&doc).unwrap()).unwrap()
}

#[test]
fn a_jsp_expression_cannot_be_joined_across_a_dropped_or_empty_code() {
    let t = |s: &str| Inline::text(s);
    let cases: Vec<(&str, Vec<Inline>)> = vec![
        // an unknown tag is dropped
        ("<p>Hello <b>bold</b> world</p>", vec![t("a$"), Inline::tag("99", TagKind::Standalone, "x"), t("{evil}")]),
        ("<p>Hello <b>bold</b> world</p>", vec![t("a#"), Inline::tag("99", TagKind::Open, "x"), t("{evil}")]),
        // the close of an unclosed <b> writes nothing
        ("<p>Hello <b>bold world</p>", vec![t("a$"), Inline::tag("1", TagKind::Close, "x"), t("{evil}")]),
        // a repeated output island is written once
        ("<p>Hello ${a} world</p>", vec![t("a"), Inline::tag("1", TagKind::Standalone, "x"), t("$"), Inline::tag("1", TagKind::Standalone, "x"), t("{evil}")]),
    ];
    for (src, target) in cases {
        let out = merge_target("jsp", src, 0, target.clone());
        let want = Dialect::Jsp.code_islands(src).unwrap();
        let got = Dialect::Jsp.code_islands(&out).unwrap();
        assert_eq!(got, want, "{src} {target:?}: the page's code changed\n{out}");
        assert!(!out.contains("${evil}") || out.contains("\\${evil}"), "{out}");
        assert!(!out.contains("#{evil}") || out.contains("\\#{evil}"), "{out}");
    }
}

#[test]
fn a_tag_that_carries_code_is_written_once() {
    let src = "<p>Hi <a href=\"<?php track(); ?>\">there</a> now <img src=\"<?= $u ?>\"></p>";
    let f = filter("phtml");
    let doc = f.extract(src.as_bytes(), &opts()).unwrap();
    let tags: Vec<Inline> = doc.units[0].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let mut target = Vec::new();
    for _ in 0..3 {
        target.push(Inline::text("x "));
        target.extend(tags.iter().cloned());
    }
    let out = merge_target("phtml", src, 0, target);
    assert_eq!(out.matches("<?php track(); ?>").count(), 1, "{out}");
    assert_eq!(out.matches("<?= $u ?>").count(), 1, "{out}");
}

#[test]
fn the_jsp_hint_of_an_html_page_has_word_boundaries() {
    let d = |s: &str| server_page_for_html(s).unwrap();
    // checked against LocOS server_page_for_html
    assert_eq!(d("<%@ page xlanguage=\"java\" %><% x %>"), Some(Dialect::Aspx));
    assert_eq!(d("<%@ page language=\"java\" %><% x %>"), Some(Dialect::Jsp));
    assert_eq!(d("<%@ pages language=\"java\" %><% x %>"), Some(Dialect::Aspx));
    assert_eq!(d("<%@ page xlanguage=\"c\" language=\"java\" %><% x %>"), Some(Dialect::Jsp));
    assert_eq!(d("<%@ page language=\"\"java\" %><% x %>"), Some(Dialect::Aspx));
    assert_eq!(d("<%@ page language=java %><% x %>"), Some(Dialect::Jsp));
}
