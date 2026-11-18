//! Unit model step 7, json: i18next plural keys (`files_one`, `files_other`) share a plural group, and outside the
//! LocOS profile each key a target needs and the group lacks gets a unit of its own, which writes what today's
//! merge writes until it is translated (nothing with `json.i18next_plurals` off, the copy of `_other` with it on).
//! Suffix-style ARB plurals and go-i18n plural objects get the group only.

use bilingua_json::JsonFilter;
use bilingua_model::*;

const I18N: &str = "{\n  \"hi\": \"Hello\",\n  \"files_one\": \"{{count}} file\",\n  \"files_other\": \"{{count}} files\",\n  \"place_ordinal_one\": \"{{count}}st\",\n  \"place_ordinal_other\": \"{{count}}th\"\n}\n";

fn extract(src: &str, dialect: &str, target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_target_lang(target.map(str::to_string)).with_format_option("json.dialect", dialect);
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    JsonFilter.extract(src.as_bytes(), &o).unwrap()
}

/// Sets the target of the unit with this key: its source with every text piece mapped by `f` (tags kept).
fn translate(d: &mut Document, key: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| u.context.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
}

fn merged(d: &Document, fill: bool) -> Result<String, Error> {
    let o = if fill { MergeOptions::new().with_format_option("json.i18next_plurals", "true") } else { MergeOptions::new() };
    JsonFilter.merge_with(d, &o).map(|b| String::from_utf8(b).unwrap())
}

type Slot<'d> = (&'d str, &'d str, PluralKind, &'d str, bool);

fn slots(d: &Document) -> Vec<Slot<'_>> {
    d.units.iter().filter_map(|u| u.plural().map(|p| (u.context.key.as_deref().unwrap(), p.group(), p.kind(), p.category().unwrap(), p.is_added()))).collect()
}

#[test]
fn i18next_keys_share_a_group_and_polish_adds_few_and_many() {
    let d = extract(I18N, "plain", Some("pl"), false);
    assert_eq!(
        slots(&d),
        [
            ("/files_one", "/files", PluralKind::Cardinal, "one", false),
            ("/files_other", "/files", PluralKind::Cardinal, "other", false),
            // the added keys right after their own group's (cat-os#89)
            ("/files_few", "/files", PluralKind::Cardinal, "few", true),
            ("/files_many", "/files", PluralKind::Cardinal, "many", true),
            ("/place_ordinal_one", "/place_ordinal", PluralKind::Ordinal, "one", false),
            ("/place_ordinal_other", "/place_ordinal", PluralKind::Ordinal, "other", false),
        ]
    );
    // Polish ordinals have only `other`: nothing is added for them
    assert_eq!(merged(&d, false).unwrap(), I18N, "an untouched merge is the original bytes");
    assert_eq!(merged(&d, true).unwrap(), I18N, "with the fill on too, while nothing is translated");
}

#[test]
fn untranslated_added_keys_write_todays_merge_and_translated_ones_their_text() {
    let mut d = extract(I18N, "plain", Some("pl"), false);
    translate(&mut d, "/files_other", |s| s.replace("files", "plików"));
    // added units sit after their group, so leaving them out moves the skeleton's unit numbers
    let old = icu::reorder_added(&d, false);
    for fill in [false, true] {
        assert_eq!(merged(&d, fill).unwrap(), merged(&old, fill).unwrap(), "fill {fill}: what a document without the added units writes");
    }
    assert!(merged(&d, true).unwrap().contains("\"files_few\": \"{{count}} plików\""));
    translate(&mut d, "/files_few", |s| s.replace("files", "pliki"));
    // the fill off: only the translated added key is written, before `_other`
    let out = merged(&d, false).unwrap();
    assert!(out.contains("\"files_few\": \"{{count}} pliki\",\n  \"files_other\": \"{{count}} plików\""), "{out}");
    assert!(!out.contains("files_many"), "{out}");
    let out = merged(&d, true).unwrap();
    assert!(out.contains("\"files_few\": \"{{count}} pliki\"") && out.contains("\"files_many\": \"{{count}} plików\""), "{out}");
}

#[test]
fn a_key_added_for_another_target_is_refused() {
    let mut d = extract(I18N, "plain", Some("pl"), false);
    translate(&mut d, "/files_few", |s| s.replace("files", "pliki"));
    d.target_lang = Some("sl".into());
    // its own error, so a host can offer to extract again for the new target
    assert!(matches!(merged(&d, false), Err(Error::TargetChanged { .. })));
    assert!(matches!(merged(&d, true), Err(Error::TargetChanged { .. })));
}

#[test]
fn suffix_arb_and_go_plurals_get_groups_only() {
    let arb = "{\n  \"@@locale\": \"en\",\n  \"itemCountOne\": \"One item\",\n  \"itemCountOther\": \"{n} items\",\n  \"phone\": \"Phone\"\n}\n";
    let d = extract(arb, "arb", Some("pl"), false);
    assert_eq!(slots(&d), [("/itemCountOne", "itemCount", PluralKind::Cardinal, "one", false), ("/itemCountOther", "itemCount", PluralKind::Cardinal, "other", false)]);
    let go = "{\n  \"files\": {\n    \"description\": \"Files\",\n    \"one\": \"{{.Count}} file\",\n    \"other\": \"{{.Count}} files\"\n  }\n}\n";
    let d = extract(go, "go", Some("pl"), false);
    assert_eq!(slots(&d), [("/files/one", "/files", PluralKind::Cardinal, "one", false), ("/files/other", "/files", PluralKind::Cardinal, "other", false)]);
    assert_eq!(merged(&d, false).unwrap(), go);
}

#[test]
fn nothing_is_filled_or_added_under_the_locos_profile() {
    let d = extract(I18N, "plain", Some("pl"), true);
    assert_eq!(d.units.len(), 5);
    assert!(d.units.iter().all(|u| u.plural().is_none() && !u.has_hidden(icu::ADDED_HIDDEN)));
}

/// cat-os#89: a document from 0.10.0 (added units at the end) merges to the same file as one with the added keys
/// after their own group.
#[test]
fn documents_with_added_keys_at_the_end_still_merge() {
    let mut d = extract(I18N, "plain", Some("pl"), false);
    translate(&mut d, "/files_few", |s| s.replace("files", "pliki"));
    translate(&mut d, "/place_ordinal_other", |s| s.replace("th", "."));
    let old = icu::reorder_added(&d, true);
    assert!(old.units.last().unwrap().plural().unwrap().is_added());
    for fill in [false, true] {
        assert_eq!(merged(&d, fill).unwrap(), merged(&old, fill).unwrap(), "fill {fill}");
    }
}
