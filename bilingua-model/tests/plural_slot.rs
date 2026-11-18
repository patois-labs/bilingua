//! Unit model step 6: the plural slot on a unit, its JSON, the target categories and the LocOS backstop.

use bilingua_model::*;

#[test]
fn the_slot_is_left_out_of_the_json_when_unset_and_round_trips() {
    let u = Unit::new("u1", vec![Inline::text("x")]);
    assert!(!serde_json::to_string(&u).unwrap().contains("plural"));
    let u = u.with_plural(PluralSlot::new("u1#0", PluralKind::Cardinal).with_category("few").with_arg("n").with_added(true));
    let j = serde_json::to_string(&u).unwrap();
    assert!(j.ends_with(r#""plural":{"group":"u1#0","kind":"cardinal","category":"few","arg":"n","added":true}}"#), "{j}");
    let back: Unit = serde_json::from_str(&j).unwrap();
    assert_eq!(back, u);
    // a kind from a newer release reads as other
    let s: PluralSlot = serde_json::from_str(r#"{"group":"g","kind":"gender"}"#).unwrap();
    assert_eq!((s.kind(), s.is_added(), s.index()), (PluralKind::Other, false, None));
}

#[test]
fn plural_categories_come_from_the_target_language() {
    let doc = Document::new("json", vec![], Skeleton::default()).with_target_lang("pl-PL");
    assert_eq!(doc.plural_categories(PluralKind::Cardinal).unwrap(), ["one", "few", "many", "other"]);
    assert_eq!(doc.plural_categories(PluralKind::Ordinal).unwrap(), ["other"]);
    assert_eq!(doc.plural_categories(PluralKind::Select), None);
    assert_eq!(Document::new("json", vec![], Skeleton::default()).with_target_lang("deu").plural_categories(PluralKind::Cardinal).unwrap(), ["one", "other"]);
    assert_eq!(Document::default().plural_categories(PluralKind::Cardinal), None);
}

#[test]
fn the_locos_profile_annotate_clears_plural_slots() {
    let u = Unit::new("u1", vec![Inline::text("x")]).with_plural(PluralSlot::new("e1", PluralKind::Cardinal).with_index(0));
    let mut doc = Document::new("po", vec![u], Skeleton::text(vec![], "utf-8", parser_info!(1)));
    profile::annotate(&mut doc, None);
    assert!(doc.units[0].plural().is_some());
    profile::annotate(&mut doc, Some(profile::LOCOS));
    assert!(doc.units[0].plural().is_none());
}
