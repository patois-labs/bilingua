//! Unit model step 7: the variants of a Fluent select share a plural group, and outside the LocOS profile a plural
//! select gets a unit for each category the target needs and it lacks. Untranslated, it writes what today's merges
//! write (nothing in `merge`, the copy of `[other]` in `merge_localized`); translated, its text.

use bilingua_ftl::FtlFilter;
use bilingua_model::*;

const SRC: &str = "hi = Hello\nemails =\n    { $count ->\n        [one] You have one email.\n       *[other] You have { $count } emails.\n    }\nwho = { $g ->\n    [female] { $n ->\n        [one] her file\n       *[other] her files\n    }\n   *[other] their files\n}\n";

fn extract(target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_target_lang(target.map(str::to_string));
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    FtlFilter.extract(SRC.as_bytes(), &o).unwrap()
}

fn by_key<'d>(d: &'d Document, key: &str) -> &'d Unit {
    d.units.iter().find(|u| u.context.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}"))
}

/// Sets the target of the unit with this key: its source with every text piece mapped by `f` (tags kept).
fn translate(d: &mut Document, key: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| u.context.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
}

fn plain(d: &Document) -> Result<String, Error> {
    FtlFilter.merge(d).map(|b| String::from_utf8(b).unwrap())
}

fn localized(d: &Document) -> Result<String, Error> {
    FtlFilter.merge_localized(d, None).map(|b| String::from_utf8(b).unwrap())
}

#[test]
fn variants_share_a_group_nested_ones_name_their_parent() {
    let d = extract(None, false);
    let slot = |k: &str| {
        let p = by_key(&d, k).plural().unwrap_or_else(|| panic!("{k}"));
        (p.group().to_string(), p.kind(), p.category().map(str::to_string), p.parent().map(str::to_string))
    };
    assert_eq!(slot("emails[one]"), ("emails#0".into(), PluralKind::Cardinal, Some("one".into()), None));
    assert_eq!(slot("emails[other]"), ("emails#0".into(), PluralKind::Cardinal, Some("other".into()), None));
    assert_eq!(slot("who[female][one]"), ("who#2".into(), PluralKind::Cardinal, Some("one".into()), Some("who#1".into())));
    assert_eq!(slot("who[other]"), ("who#1".into(), PluralKind::Select, Some("other".into()), None));
    assert!(by_key(&d, "hi").plural().is_none());
    assert_eq!(d.units.len(), 6, "no target, nothing added");
}

#[test]
fn polish_adds_few_and_many_and_untouched_merges_are_todays() {
    let d = extract(Some("pl"), false);
    let added: Vec<&str> = d.units.iter().filter(|u| u.plural().is_some_and(|p| p.is_added())).map(|u| u.context.key.as_deref().unwrap()).collect();
    assert_eq!(added, ["emails[few]", "emails[many]", "who[female][few]", "who[female][many]"]);
    assert_eq!(plain(&d).unwrap(), SRC, "an untouched merge is the original bytes");
    // what a document without the added units (main) writes, translated or not
    let mut t = d.clone();
    translate(&mut t, "emails[other]", |s| s.replace("emails", "e-maili"));
    let mut old = t.clone();
    old.units.retain(|u| !u.has_hidden(icu::ADDED_HIDDEN));
    assert_eq!(plain(&t).unwrap(), plain(&old).unwrap());
    assert_eq!(localized(&t).unwrap(), localized(&old).unwrap());
    assert!(localized(&t).unwrap().contains("        [few] You have { $count } e-maili.\n"), "{}", localized(&t).unwrap());
}

#[test]
fn translated_added_forms_are_written_by_both_merges() {
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "emails[few]", |s| s.replace("emails", "e-maile"));
    let out = plain(&d).unwrap();
    assert!(out.contains("        [few] You have { $count } e-maile.\n       *[other]"), "{out}");
    assert!(!out.contains("[many]"), "plain merge writes only the translated added forms: {out}");
    let out = localized(&d).unwrap();
    assert!(out.contains("[few] You have { $count } e-maile.") && out.contains("[many] You have { $count } emails."), "{out}");
    let back = FtlFilter.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(back.units.iter().any(|u| u.context.key.as_deref() == Some("emails[few]")));
}

#[test]
fn a_form_added_for_another_target_is_refused() {
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "emails[few]", |s| s.replace("emails", "e-maile"));
    assert!(matches!(FtlFilter.merge_localized(&d, Some("sl")), Err(Error::Write(_))));
    d.target_lang = Some("sl".into());
    assert!(matches!(FtlFilter.merge(&d), Err(Error::Write(_))));
}

#[test]
fn nothing_is_filled_or_added_under_the_locos_profile() {
    let d = extract(Some("pl"), true);
    assert_eq!(d.units.len(), 6);
    assert!(d.units.iter().all(|u| u.plural().is_none() && !u.has_hidden(icu::ADDED_HIDDEN)));
}
