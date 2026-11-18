//! Unit model step 7: the category forms of a `.stringsdict` plural variable share a plural group, and outside the
//! LocOS profile a category the target needs and the variable lacks gets a unit of its own, which writes what the
//! plural fill writes until it is translated.

mod common;
use bilingua_model::*;
use bilingua_plist::Stringsdict;

const SD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n\t<key>files</key>\n\t<dict>\n\t\t<key>NSStringLocalizedFormatKey</key>\n\t\t<string>%#@items@</string>\n\t\t<key>items</key>\n\t\t<dict>\n\t\t\t<key>NSStringFormatSpecTypeKey</key>\n\t\t\t<string>NSStringPluralRuleType</string>\n\t\t\t<key>NSStringFormatValueTypeKey</key>\n\t\t\t<string>lld</string>\n\t\t\t<key>one</key>\n\t\t\t<string>%lld file</string>\n\t\t\t<key>other</key>\n\t\t\t<string>%lld files</string>\n\t\t</dict>\n\t</dict>\n</dict>\n</plist>\n";

fn extract(src: &[u8], target: &str, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_target_lang(target);
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    Stringsdict.extract(src, &o).unwrap()
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

fn merged(d: &Document) -> Result<Vec<u8>, Error> {
    Stringsdict.merge(d)
}

fn text(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

#[test]
fn forms_share_a_group_and_russian_adds_few_and_many() {
    let d = extract(SD.as_bytes(), "ru", false);
    let slots: Vec<(&str, &str, &str, Option<&str>, bool)> = d
        .units
        .iter()
        .filter_map(|u| u.plural().map(|p| (u.context.key.as_deref().unwrap(), p.group(), p.category().unwrap(), p.arg(), p.is_added())))
        .collect();
    assert_eq!(
        slots,
        [
            ("files/items/one", "files/items", "one", Some("items"), false),
            ("files/items/other", "files/items", "other", Some("items"), false),
            ("files/items/few", "files/items", "few", Some("items"), true),
            ("files/items/many", "files/items", "many", Some("items"), true),
        ]
    );
    assert_eq!(by_key(&d, "files/items/few").id, format!("{}.few", by_key(&d, "files/items/other").id));
    assert_eq!(text(merged(&d).unwrap()), SD, "an untouched merge is the original bytes");
}

#[test]
fn untranslated_added_forms_write_the_fill_and_translated_ones_their_text() {
    let mut d = extract(SD.as_bytes(), "ru", false);
    translate(&mut d, "files/items/other", |s| s.replace("files", "файлов"));
    let out = text(merged(&d).unwrap());
    let mut old = d.clone();
    old.units.retain(|u| !u.has_hidden(icu::ADDED_HIDDEN));
    assert_eq!(out, text(merged(&old).unwrap()), "what a document without the added units writes");
    assert_eq!(out.matches("%lld файлов").count(), 3, "{out}");
    translate(&mut d, "files/items/few", |s| s.replace("files", "файла"));
    let out = text(merged(&d).unwrap());
    assert!(out.contains("<key>few</key>\n\t\t\t<string>%lld файла</string>"), "{out}");
    assert!(out.contains("<key>many</key>\n\t\t\t<string>%lld файлов</string>"), "{out}");
    let back = Stringsdict.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(back.units.iter().any(|u| u.context.key.as_deref() == Some("files/items/few")));
}

#[test]
fn a_binary_stringsdict_writes_the_added_forms_too() {
    use common::bp::{dict, s, V};
    let src = dict(vec![(
        "files",
        dict(vec![
            ("NSStringLocalizedFormatKey", s("%#@items@")),
            ("items", dict(vec![("NSStringFormatSpecTypeKey", s("NSStringPluralRuleType")), ("NSStringFormatValueTypeKey", s("lld")), ("one", s("%lld file")), ("other", s("%lld files"))])),
        ]),
    )]);
    let data = common::bp::write(&src);
    let mut d = extract(&data, "ru", false);
    assert_eq!(merged(&d).unwrap(), data);
    translate(&mut d, "files/items/few", |s| s.replace("files", "файла"));
    let out = common::bp::read(&merged(&d).unwrap());
    let V::Dict(top) = out else { panic!() };
    let V::Dict(entry) = &top[0].1 else { panic!() };
    let V::Dict(items) = &entry[1].1 else { panic!() };
    let get = |k: &str| items.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    assert_eq!(get("few"), Some(s("%lld файла")));
    assert_eq!(get("many"), Some(s("%lld files")));
}

#[test]
fn a_form_added_for_another_target_is_refused() {
    let mut d = extract(SD.as_bytes(), "ru", false);
    translate(&mut d, "files/items/few", |s| s.replace("files", "файла"));
    // Arabic has zero, one, two, few, many, other: Russian `few` must not become Arabic `few`
    d.target_lang = Some("ar".into());
    // its own error, so a host can offer to extract again for the new target
    assert!(matches!(merged(&d), Err(Error::TargetChanged { .. })));
    let mut d = extract(SD.as_bytes(), "ru", false);
    translate(&mut d, "files/items/other", |s| s.replace("files", "ملفات"));
    d.target_lang = Some("ar".into());
    let out = text(merged(&d).unwrap());
    assert_eq!(out.matches("%lld ملفات").count(), 5, "{out}");
}

#[test]
fn nothing_is_filled_or_added_under_the_locos_profile() {
    let d = extract(SD.as_bytes(), "ru", true);
    assert_eq!(d.units.len(), 2);
    assert!(d.units.iter().all(|u| u.plural().is_none() && !u.has_hidden(icu::ADDED_HIDDEN)));
}
