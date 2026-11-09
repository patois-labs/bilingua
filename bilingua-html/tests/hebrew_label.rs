//! A page labelled ISO-8859-8 that holds Windows-1255 vowel points and a sign only Windows-1255 has (curly quotes):
//! it opens, an untouched merge gives its bytes back, and a changed page is written as UTF-8 (LocOS #810 gate).
use bilingua_html::Html;
use bilingua_model::*;

fn page() -> Vec<u8> {
    let mut b = b"<html><head><meta charset=\"iso-8859-8\"><title>t</title></head><body><p>".to_vec();
    // shin with a qamats vowel point, lamed, vav, final mem, then a quoted alef in curly quotes
    b.extend_from_slice(&[0xF9, 0xC8, 0xEC, 0xE5, 0xED, b' ', 0x93, 0xE0, 0x94]);
    b.extend_from_slice(b"</p><p>");
    b.extend_from_slice(&[0xF9, 0xEC, 0xE5, 0xED]);
    b.extend_from_slice(b"</p></body></html>");
    b
}

#[test]
fn iso_8859_8_page_with_vowel_points_and_curly_quotes_opens_and_round_trips() {
    let b = page();
    let d = Html.extract(&b, &ExtractOptions::default()).unwrap();
    assert!(d.units.iter().any(|u| plain_text(&u.source()).contains('\u{201c}')));
    assert_eq!(Html.merge(&d).unwrap(), b);
    let locos = MergeOptions::new().with_profile(profile::LOCOS);
    assert_eq!(Html.merge_with(&d, &locos).unwrap(), b);
}

#[test]
fn a_changed_page_that_keeps_a_curly_quote_is_written_as_utf8() {
    let b = page();
    let mut d = Html.extract(&b, &ExtractOptions::default()).unwrap();
    let i = d.units.iter().position(|u| plain_text(&u.source()) == "\u{5e9}\u{5dc}\u{5d5}\u{5dd}").unwrap();
    let id = d.units[i].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("\u{5d0}")]).unwrap();
    let out = String::from_utf8(Html.merge(&d).unwrap()).expect("written as UTF-8");
    assert!(out.contains("<meta charset=\"UTF-8\">") && out.contains("\u{201c}\u{5d0}\u{201d}"), "{out}");
}
