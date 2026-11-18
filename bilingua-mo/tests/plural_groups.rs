//! Unit model step 6: the singular and plural of a message are one plural group, with their msgstr index.

mod common;
use bilingua_model::*;
use bilingua_mo::Mo;
use common::*;

#[test]
fn the_two_forms_share_a_group_with_their_index() {
    let d = ex(&mo_le());
    let forms: Vec<_> = d.units.iter().filter_map(|u| u.plural().map(|p| (u.id.as_str(), p.group(), p.index(), p.category(), p.kind()))).collect();
    assert_eq!(forms, [("e2.0", "e2", Some(0), None, PluralKind::Cardinal), ("e2.1", "e2", Some(1), None, PluralKind::Cardinal)]);
    assert!(d.units[2..].iter().all(|u| u.plural().is_none()));
    assert_eq!(d.units[1].context.extra().get("plural"), Some("1"));
}

#[test]
fn the_locos_profile_fills_no_slot() {
    let d = Mo.extract(&mo_le(), &ExtractOptions::new().with_profile(profile::LOCOS)).unwrap();
    assert!(d.units.len() >= 2 && d.units.iter().all(|u| u.plural().is_none()));
}
