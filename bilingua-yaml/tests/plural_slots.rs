//! Unit model step 7: the forms of a plural map share a plural group, and outside the LocOS profile a Rails plural
//! map gets a unit for each category the target needs and it lacks, which writes what the plural fill writes until
//! it is translated.

use bilingua_model::*;
use bilingua_yaml::Yaml;

const RAILS: &str = "en:\n  hi: Hello\n  files:\n    one: \"%{count} file\"\n    other: \"%{count} files\"\n";

fn extract(src: &str, target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_target_lang(target.map(str::to_string));
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    Yaml.extract(src.as_bytes(), &o).unwrap()
}

/// Sets unit `id`'s target: its source with every text piece mapped by `f` (tags kept).
fn translate(d: &mut Document, id: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| u.id == id).unwrap_or_else(|| panic!("{id}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
}

fn merged(d: &Document) -> Result<String, Error> {
    Yaml.merge(d).map(|b| String::from_utf8(b).unwrap())
}

fn line<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines().find(|l| l.trim_start().starts_with(&format!("{key}:"))).unwrap_or_else(|| panic!("no {key} in {out}"))
}

#[test]
fn forms_share_a_group_and_polish_adds_few_and_many() {
    let d = extract(RAILS, Some("pl"), false);
    let slots: Vec<(&str, &str, bool)> = d.units.iter().filter_map(|u| u.plural().map(|p| (u.id.as_str(), p.category().unwrap(), p.is_added()))).collect();
    assert_eq!(slots, [("u2", "one", false), ("u3", "other", false), ("u3.few", "few", true), ("u3.many", "many", true)]);
    let groups: Vec<&str> = d.units.iter().filter_map(|u| u.plural().map(|p| p.group())).collect();
    assert!(groups.iter().all(|g| *g == groups[0]) && groups[0].ends_with("#0"), "{groups:?}");
    assert_eq!(d.units.iter().find(|u| u.id == "u3.few").unwrap().context.key.as_deref(), Some("files.few"));
    assert_eq!(merged(&d).unwrap(), RAILS, "an untouched merge is the original bytes");
    // a plain (not Rails) file gets the slots, and no units are added
    let plain = "files:\n  one: a file\n  other: some files\n";
    let d = extract(plain, Some("pl"), false);
    assert_eq!(d.units.len(), 2);
    assert_eq!(d.units[1].plural().and_then(|p| p.category()), Some("other"));
}

#[test]
fn untranslated_added_forms_write_the_fill_and_translated_ones_their_text() {
    let mut d = extract(RAILS, Some("pl"), false);
    translate(&mut d, "u3", |s| s.replace("files", "plików"));
    let out = merged(&d).unwrap();
    let mut old = d.clone();
    old.units.retain(|u| !u.has_hidden(icu::ADDED_HIDDEN));
    assert_eq!(out, merged(&old).unwrap(), "what a document without the added units writes");
    assert!(line(&out, "few").contains("plików") && line(&out, "many").contains("plików"), "{out}");
    translate(&mut d, "u3.few", |s| s.replace("files", "pliki"));
    let out = merged(&d).unwrap();
    assert!(line(&out, "few").contains("pliki") && line(&out, "many").contains("plików"), "{out}");
    assert!(out.starts_with("pl:\n"), "{out}");
    // the merged file reads back with the forms as units of their own
    let back = extract(&out, None, false);
    assert!(back.units.iter().any(|u| u.context.key.as_deref() == Some("files.few")));
}

#[test]
fn a_form_added_for_another_target_is_refused() {
    let mut d = extract(RAILS, Some("pl"), false);
    translate(&mut d, "u3.few", |s| s.replace("files", "pliki"));
    d.target_lang = Some("sl".into());
    assert!(matches!(merged(&d), Err(Error::Write(_))));
    let mut d = extract(RAILS, Some("pl"), false);
    translate(&mut d, "u3", |s| s.replace("files", "datotek"));
    d.target_lang = Some("sl".into());
    let out = merged(&d).unwrap();
    assert!(line(&out, "two").contains("datotek") && !out.contains("many:"), "{out}");
}

#[test]
fn nothing_is_filled_or_added_under_the_locos_profile() {
    let d = extract(RAILS, Some("pl"), true);
    assert_eq!(d.units.len(), 3);
    assert!(d.units.iter().all(|u| u.plural().is_none() && !u.has_hidden(icu::ADDED_HIDDEN)));
}
