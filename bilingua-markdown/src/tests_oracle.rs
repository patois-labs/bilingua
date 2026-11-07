//! Regression tests for patterns found by comparing Bilingua with other filters.

use super::*;

fn extract(s: &str) -> Document {
    MarkdownFilter.extract(s.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn merged(d: &Document) -> String {
    String::from_utf8(MarkdownFilter.merge(d).unwrap()).unwrap()
}

#[test]
fn alt_text_with_brackets_is_escaped_and_the_image_survives() {
    let d0 = extract("See ![a loaf](loaf.png \"Loaf\") now.\n");
    let mut d = d0.clone();
    let alt = d.units.iter().find(|u| u.context.kind.as_deref() == Some("alt")).unwrap().segments[0].id.clone();
    d.set_target(&alt, vec![Inline::text("[a loaf]")]).unwrap();
    let out = merged(&d);
    println!("{out}");
    assert!(out.contains("![\\[a loaf\\]](loaf.png \"Loaf\")"), "{out}");
    // the image is still one image with the same URL
    let again = extract(&out);
    assert_eq!(again.units.iter().filter(|u| u.context.kind.as_deref() == Some("alt")).count(), 1);
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_first_heading_under_the_profile() {
    let src = "\u{feff}# Title\n\nText\n";
    let o = ExtractOptions::new().with_profile(Some(profile::LOCOS));
    let mut d = MarkdownFilter.extract(src.as_bytes(), &o).unwrap();
    let texts: Vec<String> = d.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, vec!["Title".to_string(), "Text".to_string()]);
    let id0 = d.units[0].segments[0].id.clone();
    d.set_target(&id0, vec![Inline::text("Titel")]).unwrap();
    let out = MarkdownFilter.merge_with(&d, &MergeOptions::new().with_profile(Some(profile::LOCOS))).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "\u{feff}# Titel\n\nText\n");
}
