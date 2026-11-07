//! A file link whose caption runs over several lines is one link (wild round 2, bilingua#20): the options and the
//! closing brackets stay codes, and a caption line that looks like a rule or a list item is caption text.
mod common;
use bilingua_wiki::MediaWiki;
use common::*;

const PAGE: &str = "Intro words.\n[[File:a.ogg|thumb|Opening of the speech\n----\n\"A quoted line from the speech.\"]]\nNext line of text.\n\n* after\n";

#[test]
fn caption_over_several_lines_is_one_unit() {
    let f = MediaWiki::default();
    let t = texts(&f, PAGE.as_bytes());
    assert_eq!(t.len(), 2, "{t:?}");
    for s in &t {
        assert!(!s.contains("thumb") && !s.contains("]]") && !s.contains("File:"), "{s}");
    }
    assert!(t[0].contains("Opening of the speech") && t[0].contains("A quoted line"), "{}", t[0]);
    assert_identity(&f, PAGE.as_bytes());
}

#[test]
fn translation_keeps_the_link_whole() {
    let f = MediaWiki::default();
    let doc = extract(&f, PAGE.as_bytes()).unwrap();
    let u = &doc.units[0];
    let got = build_str(&f, PAGE.as_bytes(), &[(0, &format!("FR {}", token_text(u)))]);
    assert!(got.contains("[[File:a.ogg|thumb|"), "{got}");
    assert!(got.contains("speech.\"]]"), "{got}");
    assert!(got.ends_with("* after\n"), "{got}");
}

#[test]
fn caption_with_a_list_marker_line_and_crlf() {
    let f = MediaWiki::default();
    let page = crlf("[[Image:b.png|frame|First line\n* second line\nthird line]]\n\nEnd.\n");
    let t = texts(&f, page.as_bytes());
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[0].contains("second line") && !t[0].contains("]]"), "{}", t[0]);
    assert_identity(&f, page.as_bytes());
}

#[test]
fn an_unclosed_file_link_does_not_swallow_the_page() {
    let f = MediaWiki::default();
    let page = "[[File:c.png|thumb|never closed\n\n== Heading ==\nBody text.\n";
    let t = texts(&f, page.as_bytes());
    assert!(t.iter().any(|s| s == "Heading"), "{t:?}");
    assert_identity(&f, page.as_bytes());
}

#[test]
fn a_close_more_than_200_lines_away_is_not_a_caption() {
    let f = MediaWiki::default();
    let mut page = String::from("[[File:d.png|thumb|caption start\n");
    for k in 0..210 {
        page.push_str(&format!("\nParagraph number {k}.\n"));
        if k == 205 {
            page.push_str("\nA stray close ]] here.\n");
        }
    }
    let t = texts(&f, page.as_bytes());
    assert!(t.iter().any(|s| s == "Paragraph number 200."), "{t:?}");
    assert!(t.len() > 200, "{}", t.len());
    assert_identity(&f, page.as_bytes());
}

#[test]
fn a_heading_between_does_not_join_the_caption() {
    let f = MediaWiki::default();
    let page = "[[File:d.png|thumb|caption start\n\n== Heading ==\nBody text with a stray ]] close.\n";
    let t = texts(&f, page.as_bytes());
    assert!(t.iter().any(|s| s == "Heading"), "{t:?}");
    assert_identity(&f, page.as_bytes());
}

#[test]
fn a_caption_with_a_nested_link_over_two_lines() {
    let f = MediaWiki::default();
    let page = "[[File:e.png|thumb|A caption with [[Some page|a link]]\nand a second line]]\n\nNext paragraph.\n";
    let t = texts(&f, page.as_bytes());
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[0].contains("second line") && !t[0].contains("]]"), "{}", t[0]);
    assert_eq!(t[1], "Next paragraph.");
    assert_identity(&f, page.as_bytes());
    let out = prefixed(&f, page.as_bytes());
    assert!(out.contains("[[File:e.png|thumb|") && out.contains("[[Some page|"), "{out}");
}

#[test]
fn a_close_inside_nowiki_does_not_end_the_caption() {
    let f = MediaWiki::default();
    let page = "[[File:f.png|thumb|Caption with <nowiki>]]</nowiki> in it\nand a second line]]\n\nEnd.\n";
    let t = texts(&f, page.as_bytes());
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[0].contains("second line"), "{}", t[0]);
    assert_identity(&f, page.as_bytes());
}

#[test]
fn the_closing_line_opens_the_next_caption() {
    let f = MediaWiki::default();
    let page = "[[File:g.png|thumb|First caption\nends here]] [[File:h.png|thumb|Second caption\n----\nends too]]\n\nEnd.\n";
    let t = texts(&f, page.as_bytes());
    assert_eq!(t.len(), 2, "{t:?}");
    assert!(t[0].contains("Second caption") && t[0].contains("ends too"), "{}", t[0]);
    assert_identity(&f, page.as_bytes());
}
