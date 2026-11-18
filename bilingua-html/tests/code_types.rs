//! BILI-118 (unit model step 5): html inline codes carry their type, a link its `href` and `title`, an image its
//! `src` and `alt`; the LocOS profile leaves them unset (the facade clears them, `profile::annotate`).

use bilingua_html::Html;
use bilingua_model::*;

const SRC: &str = "<html><body><p>Some <b>bold</b>, a <a href=\"https://x.com/?a=1&amp;b=2\" title=\"X\">link</a>, \
<code>code</code>, <img src=\"a.png\" alt=\"A cat\"> and a<br>break.</p></body></html>";

#[test]
fn extract_fills_types_and_attrs() {
    let doc = Html.extract(SRC.as_bytes(), &ExtractOptions::default()).unwrap();
    let u = doc.units.iter().find(|u| u.codes().len() >= 5).expect("the paragraph");
    let by = |l: &str| u.codes().values().find(|c| c.label() == Some(l)).unwrap_or_else(|| panic!("no {l} in {:?}", u.codes()));
    assert_eq!(by("bold").code_type(), Some(CodeType::Bold));
    assert_eq!(by("code").code_type(), Some(CodeType::Code));
    assert_eq!(by("line break").code_type(), Some(CodeType::Linebreak));
    let link = by("link");
    assert_eq!((link.code_type(), link.attr("href"), link.attr("title")), (Some(CodeType::Link), Some("https://x.com/?a=1&b=2"), Some("X")));
    let img = by("image");
    assert_eq!((img.code_type(), img.attr("src"), img.attr("alt")), (Some(CodeType::Image), Some("a.png"), Some("A cat")));
}

#[test]
fn merge_ignores_the_types() {
    let doc = Html.extract(SRC.as_bytes(), &ExtractOptions::default()).unwrap();
    let mut untyped = doc.clone();
    for u in &mut untyped.units {
        for c in u.codes_mut().values_mut() {
            c.clear_typing();
        }
    }
    assert_eq!(Html.merge(&doc).unwrap(), Html.merge(&untyped).unwrap());
}

#[test]
fn locos_profile_leaves_types_unset() {
    let mut doc = Html.extract(SRC.as_bytes(), &profile::locos().extract_options()).unwrap();
    profile::annotate(&mut doc, Some(profile::LOCOS));
    assert!(doc.units.iter().all(|u| u.codes().values().all(|c| c.code_type().is_none() && c.attrs().is_empty())));
}
