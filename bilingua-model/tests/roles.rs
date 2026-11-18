//! Structure roles (unit model step 4): `UnitContext::role` and `level` in JSON, the catch-all for a newer
//! release's value, and the LocOS profile leaving them unset.

use bilingua_model::*;

const ALL: [(Role, &str); 13] = [
    (Role::Heading, "heading"),
    (Role::Title, "title"),
    (Role::Subtitle, "subtitle"),
    (Role::SlideTitle, "slidetitle"),
    (Role::Header, "header"),
    (Role::Footer, "footer"),
    (Role::Footnote, "footnote"),
    (Role::Endnote, "endnote"),
    (Role::Notes, "notes"),
    (Role::Comment, "comment"),
    (Role::Caption, "caption"),
    (Role::SheetName, "sheetname"),
    (Role::Other, "other"),
];

#[test]
fn every_role_has_its_lowercase_json_name() {
    for (r, name) in ALL {
        assert_eq!(serde_json::to_string(&r).unwrap(), format!("\"{name}\""));
        assert_eq!(serde_json::from_str::<Role>(&format!("\"{name}\"")).unwrap(), r);
        assert_eq!(r.as_str(), name);
        assert_eq!(r.to_string(), name);
    }
}

#[test]
fn an_unknown_role_from_a_newer_release_reads_as_other() {
    assert_eq!(serde_json::from_str::<Role>("\"sidebar\"").unwrap(), Role::Other);
    let c: UnitContext = serde_json::from_str(r#"{"kind":"x","role":"pullquote","level":2}"#).unwrap();
    assert_eq!(c.role(), Some(Role::Other));
    assert_eq!(c.level(), Some(2));
    // a whole unit with a newer role still loads
    let u: Unit = serde_json::from_str(r#"{"id":"u1","segments":[{"id":"u1","source":[{"text":"Hi"}]}],"context":{"role":"marginnote"}}"#).unwrap();
    assert_eq!(u.context.role(), Some(Role::Other));
}

#[test]
fn unset_role_and_level_are_left_out_of_json() {
    let c = UnitContext::new().with_kind("paragraph");
    let j = serde_json::to_string(&c).unwrap();
    assert!(!j.contains("role") && !j.contains("level"), "{j}");
    // JSON without the keys reads back as unset
    let back: UnitContext = serde_json::from_str(&j).unwrap();
    assert_eq!(back, c);
    assert_eq!((back.role(), back.level()), (None, None));
}

#[test]
fn role_and_level_round_trip_after_the_old_fields() {
    let c = UnitContext::new().with_kind("heading").with_role(Role::Heading).with_level(3);
    let j = serde_json::to_string(&c).unwrap();
    assert!(j.ends_with(r#""extra":{},"role":"heading","level":3}"#), "{j}");
    assert_eq!(serde_json::from_str::<UnitContext>(&j).unwrap(), c);
}

#[test]
fn setters_and_the_heading_shortcut() {
    let mut c = UnitContext::new();
    c.set_heading_role(2);
    assert_eq!((c.role(), c.level()), (Some(Role::Heading), Some(2)));
    c.set_heading_role(0);
    assert_eq!((c.role(), c.level()), (Some(Role::Heading), None));
    c.set_role(Role::Footer);
    c.set_level(None);
    assert_eq!((c.role(), c.level()), (Some(Role::Footer), None));
    c.set_role(None);
    assert_eq!(c.role(), None);
    let c = UnitContext::new().with_role(Role::Notes).with_role(None).with_level(None);
    assert_eq!(c, UnitContext::new());
}

#[test]
fn the_locos_profile_annotate_clears_roles() {
    let mut u = Unit::new("u1", vec![Inline::text("Title")]);
    u.context.set_heading_role(1);
    let mut doc = Document::new("markdown", vec![u], Skeleton::text(vec![], "utf-8", parser_info!(1)));
    profile::annotate(&mut doc, None);
    assert_eq!(doc.units[0].context.role(), Some(Role::Heading));
    profile::annotate(&mut doc, Some(profile::LOCOS));
    assert_eq!((doc.units[0].context.role(), doc.units[0].context.level()), (None, None));
}
