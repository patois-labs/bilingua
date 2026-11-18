//! Unit model step 6: plural groups from the variations path, and a unit for each target category the source
//! lacks (outside the LocOS profile), which writes what the merge's fill from `other` writes until it is translated.

use bilingua_model::*;
use bilingua_xcstrings::Xcstrings;

const CAT: &str = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "%lld files" : {
      "localizations" : {
        "en" : {
          "variations" : {
            "plural" : {
              "one" : {
                "stringUnit" : {
                  "state" : "translated",
                  "value" : "%lld file"
                }
              },
              "other" : {
                "stringUnit" : {
                  "state" : "translated",
                  "value" : "%lld files"
                }
              }
            }
          }
        }
      }
    },
    "Got %#@items@" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Got %#@items@"
          },
          "substitutions" : {
            "items" : {
              "argNum" : 1,
              "formatSpecifier" : "lld",
              "variations" : {
                "plural" : {
                  "one" : {
                    "stringUnit" : {
                      "state" : "translated",
                      "value" : "%arg item"
                    }
                  },
                  "other" : {
                    "stringUnit" : {
                      "state" : "translated",
                      "value" : "%arg items"
                    }
                  }
                }
              }
            }
          }
        }
      }
    },
    "Hello" : {

    }
  },
  "version" : "1.0"
}
"#;

fn extract(target: Option<&str>, locos: bool) -> Document {
    let mut o = ExtractOptions::new().with_source_lang("en").with_target_lang(target.map(str::to_string));
    if locos {
        o = o.with_profile(profile::LOCOS);
    }
    Xcstrings.extract(CAT.as_bytes(), &o).unwrap()
}

/// Sets unit `id`'s target: its source with every text piece mapped by `f` (codes kept).
fn translate(d: &mut Document, id: &str, f: impl Fn(&str) -> String) {
    let u = d.units.iter_mut().find(|u| u.id == id).unwrap_or_else(|| panic!("{id}"));
    let t: Vec<Inline> = u.segments[0].source.iter().map(|i| if let Inline::Text(s) = i { Inline::text(f(s)) } else { i.clone() }).collect();
    u.segments[0].target = t;
    u.segments[0].state = SegState::Draft;
}

fn merged(d: &Document) -> String {
    String::from_utf8(Xcstrings.merge(d).unwrap()).unwrap()
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn plural_leaves_are_one_group_per_plural_map() {
    let d = extract(None, false);
    let slots: Vec<(&str, &str, Option<&str>, Option<&str>)> = d.units.iter().filter_map(|u| u.plural().map(|p| (u.id.as_str(), p.group(), p.category(), p.arg()))).collect();
    assert_eq!(
        slots,
        [
            ("u1", "%lld files#variations.plural", Some("one"), None),
            ("u2", "%lld files#variations.plural", Some("other"), None),
            ("u4", "Got %#@items@#substitutions.items.variations.plural", Some("one"), Some("items")),
            ("u5", "Got %#@items@#substitutions.items.variations.plural", Some("other"), Some("items")),
        ]
    );
    // the entry's own string and a key without localizations have none
    assert!(d.units.iter().filter(|u| u.id == "u3" || u.id == "u6").all(|u| u.plural().is_none()));
    assert!(d.units.iter().all(|u| u.plural().is_none_or(|p| p.kind() == PluralKind::Cardinal && !p.is_added() && p.parent().is_none())));
}

#[test]
fn a_polish_target_adds_few_and_many_after_other() {
    let d = extract(Some("pl"), false);
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u2.few", "u2.many", "u3", "u4", "u5", "u5.few", "u5.many", "u6"]);
    let few = d.units.iter().find(|u| u.id == "u5.few").unwrap();
    let p = few.plural().unwrap();
    assert_eq!((p.group(), p.category(), p.arg(), p.is_added()), ("Got %#@items@#substitutions.items.variations.plural", Some("few"), Some("items"), true));
    assert_eq!(plain_text(&few.source()), plain_text(&d.units.iter().find(|u| u.id == "u5").unwrap().source()));
    assert_eq!(few.context.location.as_deref(), Some("localizations.en.substitutions.items.variations.plural.few"));
    assert_eq!(few.context.extra().get("variation"), Some("plural:few"));
}

/// What main writes: the same document without the added units, where the merge fills the categories the source
/// lacks from `other`.
fn main_like() -> Document {
    let mut d = extract(None, false);
    d.target_lang = Some("pl".into());
    d
}

#[test]
fn an_untouched_merge_writes_what_the_fill_wrote() {
    let d = extract(Some("pl"), false);
    assert_eq!(merged(&d), CAT);
    let (mut d, mut m) = (d, main_like());
    for doc in [&mut d, &mut m] {
        translate(doc, "u1", |_| " plik".into());
        translate(doc, "u2", |_| " plików".into());
        translate(doc, "u3", |t| t.replace("Got", "Mam"));
    }
    let out = merged(&d);
    assert_eq!(out, merged(&m));
    let j = json(&out);
    let pl = &j["strings"]["%lld files"]["localizations"]["pl"]["variations"]["plural"];
    for (c, v) in [("one", "%lld plik"), ("few", "%lld plików"), ("many", "%lld plików"), ("other", "%lld plików")] {
        assert_eq!(pl[c]["stringUnit"]["value"], v, "{c}");
    }
}

#[test]
fn a_translated_added_unit_writes_its_own_form() {
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "u1", |_| " plik".into());
    translate(&mut d, "u2", |_| " plików".into());
    translate(&mut d, "u2.few", |_| " pliki".into());
    translate(&mut d, "u3", |t| t.replace("Got", "Mam"));
    translate(&mut d, "u5.many", |_| " rzeczy".into());
    let j = json(&merged(&d));
    let pl = &j["strings"]["%lld files"]["localizations"]["pl"]["variations"]["plural"];
    for (c, v) in [("one", "%lld plik"), ("few", "%lld pliki"), ("many", "%lld plików"), ("other", "%lld plików")] {
        assert_eq!(pl[c]["stringUnit"]["value"], v, "{c}");
    }
    let items = &j["strings"]["Got %#@items@"]["localizations"]["pl"]["substitutions"]["items"]["variations"]["plural"];
    assert_eq!(items["many"]["stringUnit"]["value"], "%arg rzeczy");
    // only an added unit translated: the target localization is still written
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "u2.few", |_| " pliki".into());
    let j = json(&merged(&d));
    assert_eq!(j["strings"]["%lld files"]["localizations"]["pl"]["variations"]["plural"]["few"]["stringUnit"]["value"], "%lld pliki");
}

#[test]
fn the_locos_profile_adds_nothing_and_fills_no_slot() {
    let d = extract(Some("pl"), true);
    assert_eq!(d.units.len(), extract(None, true).units.len());
    assert!(d.units.iter().all(|u| u.plural().is_none()));
    // no target, or the source language: nothing added either
    assert_eq!(extract(Some("en-US"), false).units.len(), extract(None, false).units.len());
}

/// cat-os#89: a document from 0.10.0 with the added units at the end merges to the same file (the merge finds them
/// by id); a source whose `other` is not its last form gets them after its last form.
#[test]
fn added_units_merge_in_either_order_and_follow_the_last_form() {
    let mut d = extract(Some("pl"), false);
    translate(&mut d, "u2.few", |s| s.replace("files", "pliki"));
    translate(&mut d, "u4", |s| s.replace("item", "rzecz"));
    let old = icu::reorder_added(&d, true);
    assert!(old.units.last().unwrap().plural().unwrap().is_added());
    assert_eq!(merged(&d), merged(&old));
    let zero = CAT.replacen("\"other\" : {\n                \"stringUnit\" : {\n                  \"state\" : \"translated\",\n                  \"value\" : \"%lld files\"\n                }\n              }", "\"other\" : {\n                \"stringUnit\" : {\n                  \"state\" : \"translated\",\n                  \"value\" : \"%lld files\"\n                }\n              },\n              \"zero\" : {\n                \"stringUnit\" : {\n                  \"state\" : \"translated\",\n                  \"value\" : \"no files\"\n                }\n              }", 1);
    assert_ne!(zero, CAT);
    let d = Xcstrings.extract(zero.as_bytes(), &ExtractOptions::new().with_source_lang("en").with_target_lang("pl")).unwrap();
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).take(5).collect();
    assert_eq!(ids, ["u1", "u2", "u3", "u2.few", "u2.many"]);
}
