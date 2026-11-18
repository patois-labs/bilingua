//! BILI-118 (unit model step 5): the type of every code pptx labels, and the link target from the slide's
//! relationships.

use crate::fixtures::*;
use crate::rich::fmt_type;
use crate::*;

/// Every formatting label `label_of` gives, alone and in the combinations that matter, with its type.
#[test]
fn fmt_label_type_pairs() {
    let cases: [(&[&str], CodeType); 13] = [
        (&["b"], CodeType::Bold),
        (&["i"], CodeType::Italic),
        (&["u"], CodeType::Underline),
        (&["sup"], CodeType::Superscript),
        (&["sub"], CodeType::Subscript),
        (&["strike"], CodeType::Strike),
        (&["link"], CodeType::Link),
        (&["b", "link"], CodeType::Link),
        (&["b", "caps"], CodeType::Bold),
        (&["caps"], CodeType::Other),
        (&["highlight"], CodeType::Other),
        (&["b", "i"], CodeType::Other),
        (&[], CodeType::Other),
    ];
    for (props, ty) in cases {
        assert_eq!(fmt_type(props), ty, "{props:?}");
    }
}

/// The rich fixture paragraph: bold, a link (rId9 is https://example.com/ in the slide's rels), a line break and a
/// slide number field. The standalone labels pair with their types; the writer-private codes stay untyped.
#[test]
fn extract_fills_types() {
    let doc = Pptx.extract(&presentation_zip(), &ExtractOptions::new()).unwrap();
    let u = doc.units.iter().find(|u| u.id == "slide1-p14").expect("rich paragraph");
    let by = |l: &str| u.codes().values().find(|c| c.label() == Some(l)).unwrap_or_else(|| panic!("no {l} in {:?}", u.codes()));
    assert_eq!(by("bold").code_type(), Some(CodeType::Bold));
    let link = by("link");
    assert_eq!((link.code_type(), link.attr("href")), (Some(CodeType::Link), Some("https://example.com/")));
    assert_eq!(by("line break").code_type(), Some(CodeType::Linebreak));
    let f = by("field slidenum");
    assert_eq!((f.code_type(), f.attr("field")), (Some(CodeType::Field), Some("slidenum")));
    for (k, c) in u.codes() {
        if k.starts_with('_') {
            assert_eq!((c.code_type(), c.attrs().len()), (None, 0), "{k}");
        }
    }
}

/// Under the LocOS profile no code is typed.
#[test]
fn locos_profile_leaves_types_unset() {
    let doc = Pptx.extract(&presentation_zip(), &ExtractOptions::new().with_profile(bilingua_model::profile::LOCOS)).unwrap();
    assert!(doc.units.iter().all(|u| u.codes().values().all(|c| c.code_type().is_none() && c.attrs().is_empty())));
}
