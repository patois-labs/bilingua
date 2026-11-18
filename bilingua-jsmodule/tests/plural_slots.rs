//! Unit model step 7 and bilingua#23: outside the LocOS profile a FormatJS (ICU MessageFormat) string with a
//! plural, selectordinal or select argument is a unit per branch, as the json ICU dialects and arb cut it, with a
//! unit for each plural category the target needs and the message lacks (right after the message's own branches).
//! The merge rebuilds the message in the literal's own quote style; an untouched one keeps its literal. A template
//! literal or a concatenation stays one unit with its structure protected. Nothing changes under the LocOS profile.

use bilingua_jsmodule::JsModule;
use bilingua_model::*;

const JS: &str = "export default {\n  files: '{count, plural, one {# file} other {# files}}',\n  who: \"{g, select, female {her} other {their}} files\",\n  hi: 'Hello {name}',\n};\n";

fn extract(src: &str, target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new();
    if let Some(t) = target {
        o = o.with_target_lang(t);
    }
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    JsModule.extract(src.as_bytes(), &o).unwrap()
}

/// Sets unit `id`'s target: its source with every text piece mapped by `f` (tags kept).
fn translate(d: &mut Document, id: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| u.id == id).unwrap_or_else(|| panic!("{id}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
}

fn merged(d: &Document) -> Result<String, Error> {
    JsModule.merge(d).map(|b| String::from_utf8(b).unwrap())
}

fn icu_path(u: &Unit) -> Option<&str> {
    u.context.extra().get("icu")
}

#[test]
fn polish_gets_one_few_many_and_other_units() {
    let d = extract(JS, Some("pl"), false);
    let got: Vec<(&str, Option<&str>, Option<&str>, bool)> = d.units.iter().map(|u| (u.id.as_str(), icu_path(u), u.plural().and_then(|p| p.category()), u.plural().is_some_and(|p| p.is_added()))).collect();
    assert_eq!(
        got,
        [
            ("u1", Some("count: plural one"), Some("one"), false),
            ("u2", Some("count: plural other"), Some("other"), false),
            ("u3", Some("count: plural few"), Some("few"), true),
            ("u4", Some("count: plural many"), Some("many"), true),
            ("u5", None, None, false),
            ("u6", Some("g: select female"), Some("female"), false),
            ("u7", Some("g: select other"), Some("other"), false),
            ("u8", None, None, false),
        ]
    );
    // every branch of one argument shares a group named after the message's first unit
    assert!(d.units[..4].iter().all(|u| u.plural().unwrap().group() == "u1#0" && u.context.key.as_deref() == Some("files")));
    assert_eq!(d.units[2].hidden(icu::ADDED_HIDDEN), Some("count: plural few"));
    assert!(d.units[2].source() == d.units[1].source(), "an added branch is a copy of other");
    // `#` is a tag, the text is the branch's
    assert!(matches!(d.units[0].source()[0], Inline::Tag { .. }));
    assert_eq!(merged(&d).unwrap(), JS, "an untouched merge is the original bytes");
}

#[test]
fn translated_branches_are_written_in_the_literal_s_quotes() {
    let mut d = extract(JS, Some("pl"), false);
    translate(&mut d, "u1", |s| s.replace("file", "plik"));
    translate(&mut d, "u2", |s| s.replace("files", "plików"));
    translate(&mut d, "u3", |s| s.replace("files", "pliki"));
    translate(&mut d, "u7", |s| s.replace("their", "ich'"));
    let out = merged(&d).unwrap();
    // few is translated, many takes other's translation (what the fill from other writes)
    assert!(out.contains("files: '{count, plural, one {# plik} few {# pliki} many {# plików} other {# plików}}',"), "{out}");
    // the double-quoted literal stays double-quoted; the apostrophe needs no escape there
    assert!(out.contains("who: \"{g, select, female {her} other {ich'}} files\","), "{out}");
    assert!(out.contains("hi: 'Hello {name}',"), "{out}");
    // only `other` translated: the added branches copy it
    let mut d = extract(JS, Some("pl"), false);
    translate(&mut d, "u2", |s| s.replace("files", "plików"));
    let out = merged(&d).unwrap();
    assert!(out.contains("'{count, plural, one {# file} few {# plików} many {# plików} other {# plików}}'"), "{out}");
    // a quote in a single-quoted literal is escaped
    let mut d = extract(JS, Some("pl"), false);
    translate(&mut d, "u1", |_| " l'x".to_string());
    assert!(merged(&d).unwrap().contains("one {# l\\'x}"));
}

#[test]
fn a_branch_added_for_another_target_is_refused() {
    let mut d = extract(JS, Some("pl"), false);
    translate(&mut d, "u3", |s| s.replace("files", "pliki"));
    d.target_lang = Some("ar".into());
    assert!(matches!(merged(&d), Err(Error::TargetChanged { unit }) if unit == "u3"), "{:?}", merged(&d));
}

#[test]
fn without_a_target_no_branch_is_added_and_a_merge_for_one_fills_from_other() {
    let mut d = extract(JS, None, false);
    assert_eq!(d.units.iter().filter(|u| u.plural().is_some_and(|p| p.is_added())).count(), 0);
    assert_eq!(d.units.len(), 6);
    translate(&mut d, "u2", |s| s.replace("files", "plików"));
    d.target_lang = Some("pl".into());
    assert!(merged(&d).unwrap().contains("'{count, plural, one {# file} few {# plików} many {# plików} other {# plików}}'"));
}

#[test]
fn template_literals_and_concatenation_stay_one_unit() {
    let src = "export default {\n  a: `{count, plural, one {# file} other {# files}}`,\n  b: '{count, plural, one {# file} ' + 'other {# files}}',\n};\n";
    let mut d = extract(src, Some("pl"), false);
    assert_eq!(d.units.len(), 2);
    assert!(d.units.iter().all(|u| icu_path(u).is_none() && u.plural().is_some_and(|p| p.category().is_none())));
    assert_eq!(merged(&d).unwrap(), src);
    translate(&mut d, "u2", |s| s.replace("files", "plików"));
    assert!(merged(&d).unwrap().contains("b: '{count, plural, one {# file} other {# plików}}',"));
}

#[test]
fn the_locos_profile_keeps_one_unit_and_no_slots() {
    let l = extract(JS, Some("pl"), true);
    assert_eq!(l.units.len(), 3);
    assert!(l.units.iter().all(|u| u.plural().is_none() && icu_path(u).is_none()));
    assert_eq!(merged(&l).unwrap(), JS);
}
