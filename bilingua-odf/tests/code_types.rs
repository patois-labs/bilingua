//! BILI-118 (unit model step 5): ODF inline codes carry their type: a link its `href` and `title`, a span its
//! character style, a field its element name; the LocOS profile has them cleared (`profile::annotate`).
mod common;
use bilingua_model::*;
use common::*;

fn doc() -> Vec<u8> {
    let p = "<text:p>See <text:a xlink:type=\"simple\" xlink:href=\"https://x.com/\" office:title=\"X\">the site</text:a>, \
<text:span text:style-name=\"T1\">styled</text:span><text:tab/>page <text:page-number>3</text:page-number> and more.</text:p>";
    let content = odt_content().replace("<text:p>12345</text:p>", &format!("{p}<text:p>12345</text:p>"));
    Pkg::new(content).styles(odt_styles()).build()
}

#[test]
fn extract_fills_types_and_attrs() {
    let d = extract("odt", &doc(), None);
    let u = d.units.iter().find(|u| u.codes().values().any(|c| c.label() == Some("link"))).expect("the link paragraph");
    let by = |l: &str| u.codes().values().find(|c| c.label() == Some(l)).unwrap_or_else(|| panic!("no {l} in {:?}", u.codes()));
    let link = by("link");
    assert_eq!((link.code_type(), link.attr("href"), link.attr("title")), (Some(CodeType::Link), Some("https://x.com/"), Some("X")));
    let span = by("span");
    assert_eq!((span.code_type(), span.attr("style")), (Some(CodeType::Style), Some("T1")));
    assert_eq!(by("tab").code_type(), Some(CodeType::Tab));
    let f = by("page-number");
    assert_eq!((f.code_type(), f.attr("field")), (Some(CodeType::Field), Some("page-number")));
}

#[test]
fn locos_profile_leaves_types_unset() {
    let mut d = filter_for("odt").extract(&doc(), &profile::locos().extract_options()).unwrap();
    profile::annotate(&mut d, Some(profile::LOCOS));
    assert!(d.units.iter().all(|u| u.codes().values().all(|c| c.code_type().is_none() && c.attrs().is_empty())));
}
