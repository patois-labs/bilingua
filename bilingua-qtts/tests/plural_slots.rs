//! Unit model step 7 and bilingua#23: a numerus message gets a plural group. Without a target it is one unit (one
//! source for every form). With a target, outside the LocOS profile, each numerus form Qt has for the target
//! language is a unit (the message's own unit is the first form, the others are added right after it), in Qt's
//! order, with the CLDR category as a hint; untranslated, the added forms write what the merge wrote before (the
//! first form's translation). Nothing changes under the LocOS profile.

use bilingua_model::*;
use bilingua_qtts::QtTs;

const TS: &str = "<TS version=\"2.1\" language=\"pl\"><context><name>A</name><message numerus=\"yes\"><source>%n file(s)</source><translation type=\"unfinished\"/></message><message><source>Hello</source><translation type=\"unfinished\"/></message></context></TS>";

fn extract(src: &str, target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new();
    if let Some(t) = target {
        o = o.with_target_lang(t);
    }
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    QtTs.extract(src.as_bytes(), &o).unwrap()
}

fn set(d: &mut Document, id: &str, text: &str) {
    let u = d.units.iter_mut().find(|u| u.id == id).unwrap_or_else(|| panic!("{id}"));
    u.segments[0].target = vec![Inline::text(text)];
}

fn merged(d: &Document) -> Result<String, Error> {
    QtTs.merge(d).map(|b| String::from_utf8(b).unwrap())
}

type Slot = (String, Option<u8>, Option<String>, bool);

/// (id, index, category, added) of each unit with a slot.
fn slots(d: &Document) -> Vec<Slot> {
    d.units.iter().filter_map(|u| u.plural().map(|p| (u.id.clone(), p.index(), p.category().map(str::to_string), p.is_added()))).collect()
}

fn s(id: &str, i: u8, c: Option<&str>, added: bool) -> Slot {
    (id.to_string(), Some(i), c.map(str::to_string), added)
}

#[test]
fn without_a_target_a_numerus_message_is_one_unit() {
    let d = extract(TS, None, false);
    assert_eq!(d.units.len(), 2);
    let p = d.units[0].plural().expect("numerus slot");
    assert_eq!((p.group(), p.kind(), p.category(), p.index(), p.is_added()), (d.units[0].id.as_str(), PluralKind::Cardinal, None, None, false));
    assert!(d.units[1].plural().is_none());
    assert_eq!(merged(&d).unwrap(), TS);
}

#[test]
fn polish_and_russian_get_three_forms_in_qt_order() {
    for t in ["pl", "ru"] {
        let d = extract(TS, Some(t), false);
        assert_eq!(slots(&d), [s("u1", 0, Some("one"), false), s("u1.1", 1, Some("few"), true), s("u1.2", 2, Some("many"), true)], "{t}");
        // the added forms follow their message, before the next one
        let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(ids, ["u1", "u1.1", "u1.2", "u2"], "{t}");
        assert!(d.units[1..3].iter().all(|u| u.plural().unwrap().group() == "u1" && u.source() == d.units[0].source()));
        // an untouched merge is the original bytes
        assert_eq!(merged(&d).unwrap(), TS, "{t}");
    }
    // Czech: one, few, other (Qt's Slovak rule)
    assert_eq!(slots(&extract(TS, Some("cs"), false)).into_iter().map(|x| x.2.unwrap()).collect::<Vec<_>>(), ["one", "few", "other"]);
}

#[test]
fn arabic_gets_qt_s_six_forms_english_two_japanese_one() {
    let d = extract(TS, Some("ar"), false);
    let cats: Vec<Option<String>> = slots(&d).into_iter().map(|x| x.2).collect();
    let want: Vec<Option<String>> = ["zero", "one", "two", "few", "many", "other"].iter().map(|c| Some(c.to_string())).collect();
    assert_eq!(cats, want);
    assert_eq!(merged(&d).unwrap(), TS);
    let d = extract(TS, Some("en"), false);
    assert_eq!(slots(&d), [s("u1", 0, Some("one"), false), s("u1.1", 1, Some("other"), true)]);
    assert_eq!(merged(&d).unwrap(), TS);
    let d = extract(TS, Some("ja"), false);
    assert_eq!(slots(&d), [s("u1", 0, Some("other"), false)]);
}

#[test]
fn untranslated_added_forms_write_what_main_wrote() {
    for t in ["pl", "ru", "ar", "en", "fr"] {
        let mut d = extract(TS, Some(t), false);
        set(&mut d, "u1", "%n plików");
        let out = merged(&d).unwrap();
        // a document without the added units (what 0.10.0 extracted) writes the same file
        let mut old = d.clone();
        old.units.retain(|u| !u.has_hidden(icu::ADDED_HIDDEN));
        assert_eq!(out, merged(&old).unwrap(), "{t}");
    }
    let mut d = extract(TS, Some("pl"), false);
    set(&mut d, "u1", "%n plików");
    let out = merged(&d).unwrap();
    assert_eq!(out.matches("<numerusform>%n plików</numerusform>").count(), 3, "{out}");
}

#[test]
fn translated_forms_are_written_each_in_its_place() {
    let mut d = extract(TS, Some("pl"), false);
    set(&mut d, "u1", "%n plik");
    set(&mut d, "u1.1", "%n pliki");
    set(&mut d, "u1.2", "%n plików");
    let out = merged(&d).unwrap();
    assert!(out.contains("<translation><numerusform>%n plik</numerusform><numerusform>%n pliki</numerusform><numerusform>%n plików</numerusform></translation>"), "{out}");
    // only an added form translated: the others keep the source, as the plural fills of the other formats do
    let mut d = extract(TS, Some("pl"), false);
    set(&mut d, "u1.2", "%n plików");
    let out = merged(&d).unwrap();
    assert!(out.contains("<numerusform>%n file(s)</numerusform><numerusform>%n file(s)</numerusform><numerusform>%n plików</numerusform>"), "{out}");
}

#[test]
fn a_form_added_for_another_target_is_refused() {
    let mut d = extract(TS, Some("pl"), false);
    set(&mut d, "u1.1", "%n pliki");
    // Czech has three forms too, by another rule: Polish form 2 must not become Czech form 2
    d.target_lang = Some("cs".into());
    assert!(matches!(merged(&d), Err(Error::TargetChanged { unit }) if unit == "u1.1"), "{:?}", merged(&d));
    // untranslated, the added forms are left out
    let mut d = extract(TS, Some("pl"), false);
    set(&mut d, "u1", "%n plików");
    d.target_lang = Some("cs".into());
    assert!(merged(&d).is_ok());
}

#[test]
fn a_bilingual_file_s_forms_are_read_as_targets_and_kept() {
    let src = "<TS version=\"2.1\" language=\"pl\"><context><name>A</name><message numerus=\"yes\"><source>%n file(s)</source><translation><numerusform>%n plik</numerusform><numerusform>%n pliki</numerusform><numerusform>%n plik&#243;w</numerusform></translation></message></context></TS>";
    let d = extract(src, Some("pl"), false);
    let text = |u: &Unit| u.segments[0].target.iter().map(|i| if let Inline::Text(t) = i { t.clone() } else { String::new() }).collect::<String>();
    let targets: Vec<String> = d.units.iter().map(text).collect();
    assert_eq!(targets, ["%n plik", "%n pliki", "%n plików"]);
    // untouched: the original bytes (the references stay as written)
    assert_eq!(merged(&d).unwrap(), src);
    // one form changed: that form is written, the others stay as the file has them
    let mut e = d.clone();
    set(&mut e, "u1.1", "%n pliki!");
    let out = merged(&e).unwrap();
    assert!(out.contains("<numerusform>%n plik</numerusform><numerusform>%n pliki!</numerusform><numerusform>%n plik&#243;w</numerusform>"), "{out}");
    // the LocOS profile reads no targets
    assert!(extract(src, Some("pl"), true).units[0].segments[0].target.is_empty());
}

#[test]
fn the_locos_profile_is_unchanged() {
    let mut l = extract(TS, Some("pl"), true);
    assert_eq!(l.units.len(), 2);
    assert!(l.units.iter().all(|u| u.plural().is_none() && !u.has_hidden(icu::ADDED_HIDDEN)));
    l.units[0].segments[0].target = vec![Inline::text("%n plików")];
    let out = merged(&l).unwrap();
    assert_eq!(out.matches("<numerusform>%n plików</numerusform>").count(), 3, "{out}");
}
