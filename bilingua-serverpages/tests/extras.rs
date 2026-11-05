//! Behaviour that is Bilingua's own (documented deviations from the LocOS filters) and the small public API.
mod common;
use bilingua_model::*;
use bilingua_serverpages::*;
use common::*;

const X1: &str = "\u{27E6}x1\u{27E7}";

#[test]
fn a_backslash_before_an_el_island_cannot_turn_it_into_text() {
    // JSP: `\${x}` prints the characters; a translation ending in a backslash must not do that to our island
    let src = "<p>Back ${x}</p>";
    let out = build_str("jsp", src, &[(0, &format!("Retour \\{X1}"))]);
    assert!(out.contains("&#92;${x}"), "{out}");
    assert_eq!(Dialect::Jsp.code_islands(&out).unwrap(), vec!["${x}".to_string()]);
    // not JSP: nothing special about a backslash
    let out = build_str("phtml", "<p>Back <?= $x ?></p>", &[(0, &format!("Retour \\{X1}"))]);
    assert!(out.contains("Retour \\<?= $x ?>"), "{out}");
}

#[test]
fn a_translation_equal_to_its_source_leaves_the_bytes_alone() {
    // LocOS would write `'` as `&#x27;` and `<` as `&lt;` when a unit is "translated" to itself
    for fmt in FORMATS {
        let src = "<p>It's \"fine\" a < b > c & d</p><img alt=\"Say &quot;hi&quot; it's\">";
        assert_identity(fmt, src.as_bytes());
    }
}

#[test]
fn unit_order_and_kinds() {
    let src = "<html><head><title>T title</title><meta name=\"description\" content=\"A page\"></head><body><img alt=\"A cat\" src=\"c.png\"><p>Body <b>text</b> here</p><a title=\"Go\" href=\"#\">Link</a><img alt=\"12345\"><img alt=\"\"></body></html>";
    let d = extract_str("aspx", src).unwrap();
    let kinds: Vec<(Option<&str>, Option<&str>)> = d.units.iter().map(|u| (u.context.kind.as_deref(), u.context.style.as_deref())).collect();
    assert_eq!(
        kinds,
        vec![
            (Some("paragraph"), None),
            (Some("paragraph"), None),
            (Some("paragraph"), None),
            (Some("alt"), Some("a11y:img-alt")),
            (Some("title"), Some("a11y:link-title")),
            (Some("meta"), Some("meta:description")),
        ]
    );
    assert_eq!(d.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "u2", "u3", "u4", "u5", "u6"]);
    assert_eq!(locos_text(&d.units[1]), "Body \u{27E6}g1\u{27E7}text\u{27E6}/g1\u{27E7} here");
    assert_eq!(d.format, "aspx");
}

#[test]
fn sniffers_are_tight() {
    let plain = b"<!DOCTYPE html><html><body><p>Plain page</p></body></html>";
    for fmt in FORMATS {
        assert!(!filter(fmt).sniff(plain), "{fmt} sniffed a plain page");
        assert!(!filter(fmt).sniff(b"{\"a\": 1}"), "{fmt}");
        assert!(!filter(fmt).sniff(b""), "{fmt}");
    }
    assert!(filter("phtml").sniff(b"<p>Hi <?php echo 1; ?></p>"));
    assert!(filter("phtml").sniff(b"<p>Hi <?= $a ?></p>"));
    assert!(filter("jsp").sniff(b"<%@ page language=\"java\" %><p>x</p>"));
    assert!(filter("jsp").sniff(b"<jsp:include page=\"a.jsp\"/>"));
    assert!(filter("aspx").sniff(b"<%@ Page Language=\"C#\" %><p>x</p>"));
    assert!(!filter("aspx").sniff(b"<%@ page language=\"java\" %><p>x</p>"));
    assert!(filter("shtml").sniff(b"<!--#include virtual=\"/a\" -->"));
    assert!(filter("erb").sniff(b"<p><%= @a %></p>"));
}

#[test]
fn html_dialect_choice() {
    assert_eq!(server_page_for_html("<%@ taglib uri=\"x\" prefix=\"c\" %><p>a</p>").unwrap(), Some(Dialect::Jsp));
    assert_eq!(server_page_for_html("<%@ page language='java' %><p>a</p>").unwrap(), Some(Dialect::Jsp));
    assert_eq!(server_page_for_html("<%@ page language=\"C#\" %><p>a</p>").unwrap(), Some(Dialect::Aspx));
    assert_eq!(server_page_for_html("<jsp:include page=\"a\"/><% x %>").unwrap(), Some(Dialect::Jsp));
    assert_eq!(server_page_for_html("<p>plain</p>").unwrap(), None);
    assert_eq!(server_page_for_html("<?xml version=\"1.0\"?><p>a</p>").unwrap(), None);
    assert_eq!(server_page_for_html("<? short ?>").unwrap(), None); // `<? ` alone is not a PHP opener for .html
    assert_eq!(server_page_for_html("<?php echo 1; ?>").unwrap(), Some(Dialect::Phtml));
    assert_eq!(Dialect::from_extension("JSPF"), Some(Dialect::Jsp));
    assert_eq!(Dialect::from_extension("html"), None);
}

#[test]
fn islands_are_listed_in_order() {
    let src = "<p>A <?= $a ?> <?php if (x): ?> B <?php endif; ?></p>";
    assert_eq!(Dialect::Phtml.code_islands(src).unwrap(), vec!["<?= $a ?>", "<?php if (x): ?>", "<?php endif; ?>"]);
    assert!(Dialect::Phtml.code_islands("<?php 'never").is_err());
    assert_eq!(Dialect::from_extension("stm").unwrap().id(), "shtml");
}

#[test]
fn units_that_no_longer_match_the_page_are_refused_at_merge() {
    let f = filter("jsp");
    let mut doc = f.extract(b"<p>Hello</p><p>World</p>", &opts()).unwrap();
    doc.units.pop();
    doc.units[0].segments[0].target = vec![Inline::text("Bonjour")];
    assert!(matches!(f.merge(&doc), Err(Error::Write(_))));
}

#[test]
fn another_crates_skeleton_is_refused() {
    let f = filter("erb");
    let mut doc = f.extract(b"<p>Hello</p>", &opts()).unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        parser.as_mut().unwrap().name = "bilingua-other".into();
    }
    assert!(matches!(f.merge(&doc), Err(Error::ParserVersion { .. })));
}
