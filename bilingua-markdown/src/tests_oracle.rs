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
