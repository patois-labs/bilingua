//! The Localization OS profile (Track D group B): LocOS's units, package numbering and splice.

mod common;
use bilingua_model::{profile, Inline};
use common::*;
use proptest::prelude::*;

fn member(units: &[&str]) -> Vec<u8> {
    let tus: String = units.iter().enumerate().map(|(i, u)| format!("<trans-unit id=\"{i}\"{u}</trans-unit>")).collect();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file source-language=\"en\"><body>{tus}</body></file></xliff>").into_bytes()
}

fn p_indexes(d: &bilingua_model::Document) -> Vec<String> {
    d.units.iter().map(|u| u.context.extra[profile::P_INDEX].clone()).collect()
}

#[test]
fn units_are_numbered_by_member_offset_and_walk_slot() {
    let a = member(&["><source>One</source>", " approved=\"yes\"><source>Two</source>", "><source>Three</source>"]);
    let b = member(&["><source>Four</source>"]);
    let pkg = zip_of(&[("a.xlf", &a, true), ("skeleton.skl", b"opaque", true), ("b.xlf", &b, true)], b"");
    let f = filter();
    let mut d = f.extract(&pkg, &profile::locos().extract_options()).unwrap();
    assert_eq!(p_indexes(&d), ["0", "2", "3"]);
    let seg = d.units[2].segments[0].id.clone();
    d.set_target(&seg, vec![Inline::text("Vier")]).unwrap();
    let out = f.merge_with(&d, &profile::locos().merge_options("xlz")).unwrap();
    assert_eq!(member_of(&out, "a.xlf"), a, "an unwritten member stays as it was");
    assert!(text(&member_of(&out, "b.xlf")).contains("<source>Four</source><target>Vier</target>"));
    // nothing written and no language: the original package
    let d = f.extract(&pkg, &profile::locos().extract_options()).unwrap();
    assert_eq!(f.merge_with(&d, &profile::locos().merge_options("xlz")).unwrap(), pkg);
}

fn member_of(pkg: &[u8], name: &str) -> Vec<u8> {
    common::member(pkg, name)
}

#[test]
fn hostile_members_are_refused() {
    let f = filter();
    let o = profile::locos().extract_options();
    let ok = member(&["><source>One</source>"]);
    for bad in [
        zip_of(&[("a.xlf", b"<!DOCTYPE x [<!ENTITY e 'x'>]><xliff version='1.2'/>", true)], b""),
        zip_of(&[("a.xlf", b"<?xml version='1.0' encoding='iso-8859-1'?><xliff version='1.2'/>", true)], b""),
        zip_of(&[("a.xlf", b"<xliff version='2.0'/>", true)], b""),
        zip_of(&[("a.xlf", b"<xliff><unclosed></xliff>", true)], b""),
        zip_of(&[("a.txt", &ok, true)], b""),
        b"not a zip".to_vec(),
    ] {
        assert!(f.extract(&bad, &o).is_err());
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn any_targets_give_a_readable_package(words in proptest::collection::vec("[a-z &<>\u{27e6}\u{27e7}0-9/gx]{0,12}", 1..4)) {
        let srcs: Vec<String> = words.iter().map(|w| format!("><source>S <g id=\"1\">{}</g></source>", w.replace('&', "&amp;").replace('<', "&lt;"))).collect();
        let refs: Vec<&str> = srcs.iter().map(String::as_str).collect();
        let pkg = zip_of(&[("m.xlf", &member(&refs), true)], b"");
        let f = filter();
        let mut d = f.extract(&pkg, &profile::locos().extract_options().with_target_lang(Some("de-DE".to_string()))).unwrap();
        for (u, w) in d.units.clone().iter().zip(&words) {
            let (_, map) = profile::tokens::to_locos_text(&u.segments[0].source);
            let _ = profile::locos_set_target(&mut d, &u.segments[0].id, profile::tokens::from_locos_text(w, &map));
        }
        let out = f.merge_with(&d, &profile::locos().merge_options("xlz")).unwrap();
        prop_assert!(f.extract(&out, &profile::locos().extract_options()).is_ok());
    }
}

/// Issue 814 item 1: text in a member that looks like a token (here the very token its code gets) is written back
/// as text, and the private-use characters of the escape come back byte for byte.
#[test]
fn token_look_alikes_in_a_member_stay_text() {
    let body = "A \u{27e6}g1\u{27e7} <g id=\"1\">b</g> \u{27e6}/g1\u{27e7}\u{e000}\u{e001}";
    let a = member(&[&format!("><source>{body}</source>")]);
    let pkg = zip_of(&[("a.xlf", &a, true)], b"");
    let f = filter();
    let mut d = f.extract(&pkg, &profile::locos().extract_options()).unwrap();
    let seg = d.units[0].segments[0].clone();
    profile::locos_set_target(&mut d, &seg.id, seg.source.clone()).unwrap();
    let out = f.merge_with(&d, &profile::locos().merge_options("xlz")).unwrap();
    assert!(text(&member_of(&out, "a.xlf")).contains(&format!("<source>{body}</source><target>{body}</target>")), "{}", text(&member_of(&out, "a.xlf")));
    // the code moved, the look-alikes stay where the translation put them
    let mut d = f.extract(&pkg, &profile::locos().extract_options()).unwrap();
    let mut t: Vec<Inline> = seg.source.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned().collect();
    t.insert(1, Inline::text("B"));
    t.push(Inline::text(" \u{27e6}g1\u{27e7}x\u{27e6}/g1\u{27e7}"));
    profile::locos_set_target(&mut d, &seg.id, t).unwrap();
    let out = f.merge_with(&d, &profile::locos().merge_options("xlz")).unwrap();
    assert!(text(&member_of(&out, "a.xlf")).contains("<target><g id=\"1\">B</g> \u{27e6}g1\u{27e7}x\u{27e6}/g1\u{27e7}</target>"), "{}", text(&member_of(&out, "a.xlf")));
}

/// B-SIDE 14: under the profile each XLIFF member's slot count (protected slots included) comes as a
/// `locos_member_slots` warning in package order, so LocOS's listing (`{path, offset, units}`) can be written.
#[test]
fn members_are_listed_with_their_slots() {
    let a = member(&["><source>One</source>", " approved=\"yes\"><source>Two</source>", "><source>Three</source>"]);
    let b = member(&["><source>Four</source>"]);
    let pkg = zip_of(&[("a.xlf", &a, true), ("skeleton.skl", b"opaque", true), ("b.xlf", &b, true)], b"");
    let d = filter().extract(&pkg, &profile::locos().extract_options()).unwrap();
    let listed: Vec<(String, String)> =
        d.warnings.iter().filter(|w| w.kind == bilingua_xliffpkg::MEMBER_SLOTS).map(|w| (w.part.clone(), w.message.clone())).collect();
    assert_eq!(listed, [("a.xlf".to_string(), "3".to_string()), ("b.xlf".to_string(), "1".to_string())]);
    // Bilingua's own reading lists nothing
    let native = filter().extract(&pkg, &bilingua_model::ExtractOptions::new()).unwrap();
    assert!(native.warnings.iter().all(|w| w.kind != bilingua_xliffpkg::MEMBER_SLOTS));
}

/// B-SIDE 15: a memoQ package's refusal names `mqxlz`, not `xlz`.
#[test]
fn an_mqxlz_refusal_names_mqxlz() {
    let f = bilingua_xliffpkg::mqxlz();
    match f.extract(b"not a zip", &profile::locos().extract_options()) {
        Err(bilingua_model::Error::Parse { format, .. }) => assert_eq!(format, "mqxlz"),
        other => panic!("{other:?}"),
    }
}
