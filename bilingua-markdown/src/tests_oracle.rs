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

fn locos_extract(s: &[u8]) -> Document {
    MarkdownFilter.extract(s, &ExtractOptions::new().with_profile(Some(profile::LOCOS))).unwrap()
}

#[test]
fn a_multi_line_plain_front_matter_value_is_not_offered_under_the_profile() {
    // LocOS offers a key only when the whole block reads it as the one line: the folded value differs
    let src = b"---\ntitle: A plain title that\n  goes on over two lines\nsubtitle: A plain value that\n  continues here\ndescription: A one line summary\n---\n\nBody text.\n";
    let d = locos_extract(src);
    let texts: Vec<String> = d.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, vec!["A one line summary".to_string(), "Body text.".to_string()]);
    let out = MarkdownFilter.merge_with(&d, &MergeOptions::new().with_profile(Some(profile::LOCOS))).unwrap();
    assert_eq!(out, src);
    // a quoted value, a value with a trailing comment and a value followed by a key stay offered
    let src = b"---\ntitle: \"Quoted\"\nsubtitle: Plain # note\ndescription: Next\nlayout: post\n---\nBody.\n";
    let texts: Vec<String> = locos_extract(src).units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, vec!["Quoted".to_string(), "Plain".to_string(), "Next".to_string(), "Body.".to_string()]);
}

#[test]
fn a_schema_1_profile_document_still_merges() {
    // before schema 2 the profile skeleton was the decoded file alone; its first line must not be read as a tag
    let src = b"Intro\n\nBody text.\n";
    let mut d = locos_extract(src);
    d.skeleton = Skeleton::binary(src.to_vec(), bilingua_model::parser_info!(1));
    let o = MergeOptions::new().with_profile(Some(profile::LOCOS));
    assert_eq!(MarkdownFilter.merge_with(&d, &o).unwrap(), src);
    let id = d.units[1].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Textkoerper.")]).unwrap();
    assert_eq!(MarkdownFilter.merge_with(&d, &o).unwrap(), b"Intro\n\nTextkoerper.\n");
    // the current layout keeps its tag line out of the output
    let d = locos_extract(src);
    assert_eq!(MarkdownFilter.merge_with(&d, &o).unwrap(), src);
}

#[test]
fn a_one_line_value_before_a_nested_key_or_an_indented_comment_stays_offered_under_the_profile() {
    let src = b"---\ntitle: Kept title\nauthor:\n  name: x\ndescription: Kept summary\n  # an indented comment\nnotes: a plain value that\n  goes on\n---\nBody text here.\n";
    let d = locos_extract(src);
    let texts: Vec<String> = d.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, vec!["Kept title".to_string(), "Kept summary".to_string(), "Body text here.".to_string()]);
    assert_eq!(MarkdownFilter.merge_with(&d, &MergeOptions::new().with_profile(Some(profile::LOCOS))).unwrap(), src);
}
