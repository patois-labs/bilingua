//! Unit model step 7: Android `<plurals>` items share a plural group, and outside the LocOS profile a quantity the
//! target needs and the source lacks gets a unit of its own, which writes what the plural fill writes until it is
//! translated.

use bilingua_model::*;
use bilingua_xmlres::AndroidStrings;

const SRC: &str = "<resources>\n    <string name=\"hi\">Hello</string>\n    <plurals name=\"files\">\n        <item quantity=\"one\">%d file</item>\n        <item quantity=\"other\">%d files</item>\n    </plurals>\n</resources>\n";

fn extract(target: &str, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_target_lang(target);
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    AndroidStrings.extract(SRC.as_bytes(), &o).unwrap()
}

/// Sets unit `id`'s target: its source with every text piece mapped by `f` (tags kept).
fn translate(d: &mut Document, id: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| u.id == id).unwrap_or_else(|| panic!("{id}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
}

fn merged(d: &Document) -> Result<String, Error> {
    AndroidStrings.merge(d).map(|b| String::from_utf8(b).unwrap())
}

#[test]
fn items_share_a_group_and_polish_adds_few_and_many() {
    let d = extract("pl", false);
    let slots: Vec<(&str, &str, &str, bool)> = d
        .units
        .iter()
        .filter_map(|u| u.plural().map(|p| (u.id.as_str(), p.group(), p.category().unwrap(), p.is_added())))
        .collect();
    assert_eq!(slots, [("u2", "files#0", "one", false), ("u3", "files#0", "other", false), ("u3.few", "files#0", "few", true), ("u3.many", "files#0", "many", true)]);
    assert!(d.units[0].plural().is_none());
    assert!(d.units.iter().all(|u| u.plural().is_none_or(|p| p.kind() == PluralKind::Cardinal)));
    let few = d.units.iter().find(|u| u.id == "u3.few").unwrap();
    assert_eq!((few.context.key.as_deref(), few.context.extra().get("plural")), (Some("files"), Some("few")));
    // an untouched merge is the original bytes
    assert_eq!(merged(&d).unwrap(), SRC);
}

#[test]
fn untranslated_added_forms_write_the_fill_and_translated_ones_their_text() {
    let mut d = extract("pl", false);
    translate(&mut d, "u3", |s| s.replace("files", "plików"));
    let out = merged(&d).unwrap();
    // what a document without the added units (main) writes: few and many copy the translated `other`
    // added units sit after their group, so leaving them out moves the skeleton's unit numbers
    let old = icu::reorder_added(&d, false);
    assert_eq!(out, merged(&old).unwrap());
    assert_eq!(out.matches("%d plików").count(), 3, "{out}");
    translate(&mut d, "u3.few", |s| s.replace("files", "pliki"));
    let out = merged(&d).unwrap();
    assert!(out.contains("<item quantity=\"few\">%d pliki</item>"), "{out}");
    assert!(out.contains("<item quantity=\"many\">%d plików</item>"), "{out}");
    assert!(out.contains("<item quantity=\"other\">%d plików</item>"), "{out}");
    // a translated added form alone is written too (the fill runs)
    let mut only = extract("pl", false);
    translate(&mut only, "u3.many", |s| s.replace("files", "plików"));
    let out = merged(&only).unwrap();
    assert!(out.contains("<item quantity=\"many\">%d plików</item>") && out.contains("<item quantity=\"few\">%d files</item>"), "{out}");
}

#[test]
fn a_form_added_for_another_target_is_refused() {
    let mut d = extract("pl", false);
    translate(&mut d, "u3.few", |s| s.replace("files", "pliki"));
    // Slovenian has one, two, few, other: Polish `few` must not become Slovenian `few`
    d.target_lang = Some("sl".into());
    // its own error, so a host can offer to extract again for the new target
    assert!(matches!(merged(&d), Err(Error::TargetChanged { unit }) if unit == "u3.few"), "{:?}", merged(&d));
    // untranslated, the added units are left out and the fill runs for the new target
    let mut d = extract("pl", false);
    translate(&mut d, "u3", |s| s.replace("files", "datotek"));
    d.target_lang = Some("sl".into());
    let out = merged(&d).unwrap();
    assert!(out.contains("<item quantity=\"two\">%d datotek</item>") && !out.contains("quantity=\"many\""), "{out}");
}

#[test]
fn nothing_is_filled_or_added_under_the_locos_profile() {
    let d = extract("pl", true);
    assert_eq!(d.units.len(), 3);
    assert!(d.units.iter().all(|u| u.plural().is_none() && !u.has_hidden(icu::ADDED_HIDDEN)));
    // and nothing is added without a target
    let d = AndroidStrings.extract(SRC.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.len(), 3);
    assert_eq!(d.units[2].plural().map(|p| p.category()), Some(Some("other")));
}

/// cat-os#89: the added forms follow their own `<plurals>`, before the next entry; a document from 0.10.0 (added
/// units at the end) merges to the same file.
#[test]
fn added_forms_follow_their_own_plurals_and_0_10_0_documents_still_merge() {
    let src = "<resources>\n    <plurals name=\"files\">\n        <item quantity=\"one\">%d file</item>\n        <item quantity=\"other\">%d files</item>\n    </plurals>\n    <string name=\"bye\">Bye</string>\n</resources>\n";
    let mut d = AndroidStrings.extract(src.as_bytes(), &ExtractOptions::new().with_target_lang("pl")).unwrap();
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u2.few", "u2.many", "u3"]);
    assert_eq!(merged(&d).unwrap(), src);
    translate(&mut d, "u2.few", |s| s.replace("files", "pliki"));
    translate(&mut d, "u3", |_| "Pa".to_string());
    let old = icu::reorder_added(&d, true);
    assert_eq!(old.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "u2", "u3", "u2.few", "u2.many"]);
    let out = merged(&d).unwrap();
    assert_eq!(out, merged(&old).unwrap());
    assert!(out.contains("<item quantity=\"few\">%d pliki</item>") && out.contains(">Pa</string>"), "{out}");
}
