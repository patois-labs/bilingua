//! Unit model step 6, json ARB dialect (the same file as the arb crate's test): plural groups from the ICU parse, and a unit for each target category the source lacks
//! (outside the LocOS profile), which writes what the merge's fill from `other` writes until it is translated.

use bilingua_json::JsonFilter;
use bilingua_model::*;

const ARB: &str = "{\n  \"@@locale\": \"en\",\n  \"files\": \"{count, plural, one{# file} other{# files}}\",\n  \"who\": \"{g, select, female{{n, plural, one{# her} other{# hers}}} other{them}}\",\n  \"hi\": \"Hello\"\n}\n";

fn extract(target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_source_lang("en").with_target_lang(target.map(str::to_string)).with_format_option("json.dialect", "arb");
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    JsonFilter.extract(ARB.as_bytes(), &o).unwrap()
}

fn path(u: &Unit) -> &str {
    u.context.extra().get("icu").unwrap_or("")
}

/// The unit's source with every text piece mapped by `f` (tags kept).
fn translate(d: &mut Document, icu: &str, key: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| path(u) == icu && u.context.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key} {icu}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
    u.segments[0].state = SegState::Draft;
}

fn merged(d: &Document, opts: &MergeOptions) -> String {
    String::from_utf8(JsonFilter.merge_with(d, opts).unwrap()).unwrap()
}

/// (ICU path, group, parent, category, argument, kind)
type Slot = (String, String, Option<String>, String, Option<String>, PluralKind);

#[test]
fn branches_are_one_group_per_argument() {
    let d = extract(None, false);
    let slots: Vec<Slot> = d
        .units
        .iter()
        .filter_map(|u| {
            u.plural().map(|p| (path(u).to_string(), p.group().to_string(), p.parent().map(str::to_string), p.category().unwrap().to_string(), p.arg().map(str::to_string), p.kind()))
        })
        .collect();
    let s = |a: &str, g: &str, p: Option<&str>, c: &str, arg: &str, k| (a.to_string(), g.to_string(), p.map(str::to_string), c.to_string(), Some(arg.to_string()), k);
    assert_eq!(
        slots,
        [
            s("count: plural one", "u1#0", None, "one", "count", PluralKind::Cardinal),
            s("count: plural other", "u1#0", None, "other", "count", PluralKind::Cardinal),
            s("g: select female / n: plural one", "u3#1", Some("u3#0"), "one", "n", PluralKind::Cardinal),
            s("g: select female / n: plural other", "u3#1", Some("u3#0"), "other", "n", PluralKind::Cardinal),
            s("g: select other", "u3#0", None, "other", "g", PluralKind::Select),
        ]
    );
    // the message's own text has no slot; no target, nothing added
    assert!(d.units.iter().find(|u| u.context.key.as_deref() == Some("/hi")).unwrap().plural().is_none());
    assert!(d.units.iter().all(|u| !u.plural().is_some_and(PluralSlot::is_added)));
    assert_eq!(d.plural_categories(PluralKind::Cardinal), None);
}

#[test]
fn a_polish_target_adds_few_and_many() {
    let d = extract(Some("pl"), false);
    let added: Vec<(&str, &str)> = d.units.iter().filter(|u| u.plural().is_some_and(PluralSlot::is_added)).map(|u| (path(u), u.plural().unwrap().category().unwrap())).collect();
    assert_eq!(added, [("count: plural few", "few"), ("count: plural many", "many"), ("g: select female / n: plural few", "few"), ("g: select female / n: plural many", "many")]);
    assert_eq!(d.units.len(), extract(None, false).units.len() + 4);
    assert_eq!(d.plural_categories(PluralKind::Cardinal).unwrap(), ["one", "few", "many", "other"]);
    // an added unit holds the source `other` text
    let few = d.units.iter().find(|u| path(u) == "count: plural few").unwrap();
    assert_eq!(plain_text(&few.source()), " files");
}

/// What main writes: the same document without the added units (extracted with no target, the target set after),
/// where the merge fills the missing categories from `other`.
fn main_like() -> Document {
    let mut d = extract(None, false);
    d.target_lang = Some("pl".into());
    d
}

#[test]
fn an_untouched_merge_writes_what_the_fill_wrote() {
    let stamp = MergeOptions::new().with_stamp_target_always(true);
    let (d, m) = (extract(Some("pl"), false), main_like());
    let out = merged(&d, &stamp);
    assert_eq!(out, merged(&m, &stamp));
    assert!(out.contains("{count, plural, one{# file} few{# files} many{# files} other{# files}}"), "{out}");
    // `other` translated, the added units not: they take `other`'s translation, as the fill did
    let (mut d, mut m) = (d, m);
    for doc in [&mut d, &mut m] {
        translate(doc, "count: plural other", "/files", |_| " plików".into());
        translate(doc, "", "/hi", |_| "Cześć".into());
    }
    let out = merged(&d, &MergeOptions::new());
    assert_eq!(out, merged(&m, &MergeOptions::new()));
    assert!(out.contains("one{# file} few{# plików} many{# plików} other{# plików}"), "{out}");
    // and without a target nothing changes at all
    let d = extract(Some("pl"), false);
    let mut plain = d.clone();
    plain.target_lang = None;
    assert_eq!(merged(&plain, &MergeOptions::new()), ARB);
}

#[test]
fn a_translated_added_unit_writes_its_own_form() {
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "count: plural one", "/files", |_| " plik".into());
    translate(&mut d, "count: plural few", "/files", |_| " pliki".into());
    translate(&mut d, "count: plural other", "/files", |_| " plików".into());
    translate(&mut d, "g: select female / n: plural many", "/who", |_| " jej".into());
    let out = merged(&d, &MergeOptions::new());
    assert!(out.contains("{count, plural, one{# plik} few{# pliki} many{# plików} other{# plików}}"), "{out}");
    assert!(out.contains("{n, plural, one{# her} few{# hers} many{# jej} other{# hers}}"), "{out}");
    // the merged file reads back as a Polish one: every category is there, nothing is added any more
    let back = JsonFilter.extract(out.as_bytes(), &ExtractOptions::new().with_target_lang("pl").with_format_option("json.dialect", "arb")).unwrap();
    assert!(back.units.iter().all(|u| !u.plural().is_some_and(PluralSlot::is_added)));
    // the same document merges under the LocOS profile too
    let l = merged(&d, &MergeOptions::new().with_profile(profile::LOCOS));
    assert!(l.contains("few{# pliki}"), "{l}");
}

#[test]
fn the_locos_profile_adds_nothing_and_fills_no_slot() {
    let d = extract(Some("pl"), true);
    assert_eq!(d.units.len(), extract(None, true).units.len());
    assert!(d.units.iter().all(|u| u.plural().is_none()));
    let j = serde_json::to_string(&d).unwrap();
    assert!(!j.contains("\"plural\":{"), "{j}");
}

#[test]
fn a_document_extracted_for_one_language_never_writes_its_added_forms_into_another() {
    // extracted for Polish (few, many added), merged for Slovenian (one, two, few, other): the counts match, the
    // categories do not; the merge refuses with its own error (Slovenian adds no `many`), never writing `few`'s
    // text as `two`
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "count: plural few", "/files", |_| " pliki".into());
    d.target_lang = Some("sl".into());
    let merge = |d: &Document| JsonFilter.merge_with(d, &MergeOptions::new()).map(|b| String::from_utf8(b).unwrap());
    let many = d.units.iter().find(|u| path(u) == "count: plural many").unwrap().id.clone();
    assert!(matches!(merge(&d), Err(Error::TargetChanged { unit }) if unit == many), "{:?}", merge(&d));
    // with the added units untranslated too, once anything is translated (an untouched merge gives the file back)
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "", "/hi", |_| "Zdravo".into());
    d.target_lang = Some("sl".into());
    assert!(matches!(merge(&d), Err(Error::TargetChanged { .. })));
    // the same target merges; a plain skeleton mismatch (the last message's unit gone) stays a write error
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "count: plural one", "/files", |_| "# plik".into());
    assert!(merge(&d).is_ok());
    assert_eq!(d.units.pop().and_then(|u| u.context.key), Some("/hi".to_string()));
    assert!(matches!(merge(&d), Err(Error::Write(_))), "{:?}", merge(&d));
    // the added units carry their ICU path as a hidden value
    let d = extract(Some("pl"), false);
    let few = d.units.iter().find(|u| path(u) == "count: plural few").unwrap();
    assert_eq!(few.hidden(bilingua_model::icu::ADDED_HIDDEN), Some("count: plural few"));
    assert!(d.units.iter().filter(|u| !u.plural().is_some_and(PluralSlot::is_added)).all(|u| u.hidden(bilingua_model::icu::ADDED_HIDDEN).is_none()));
}
