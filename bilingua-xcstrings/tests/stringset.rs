//! `stringSet` localizations (App Shortcuts phrases): one unit per string, merged back in order.
mod common;
use bilingua_model::*;
use bilingua_xcstrings::Xcstrings;
use common::*;

const SET: &str = r#"{
  "sourceLanguage" : "en",
  "version" : "1.1",
  "strings" : {
    "Start workout" : {
      "localizations" : {
        "de" : {
          "stringSet" : {
            "state" : "translated",
            "values" : [
              "Training starten",
              "Training beginnen"
            ]
          }
        },
        "en" : {
          "stringSet" : {
            "state" : "translated",
            "values" : [
              "Start workout",
              "Begin workout",
              "123"
            ]
          }
        }
      }
    },
    "Old phrase" : {
      "extractionState" : "stale",
      "localizations" : {
        "en" : {
          "stringSet" : {
            "state" : "translated",
            "values" : [
              "Old one"
            ]
          }
        }
      }
    },
    "Plain" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Plain text"
          }
        }
      }
    }
  }
}
"#;

#[test]
fn each_value_is_a_unit() {
    let d = ex(SET.as_bytes(), None);
    assert_eq!(texts(&d), ["Start workout", "Begin workout", "Plain text"]);
    let u = &d.units[1];
    assert_eq!(u.context.key.as_deref(), Some("Start workout"));
    assert_eq!(u.context.kind.as_deref(), Some("stringSet"));
    assert_eq!(u.context.location.as_deref(), Some("localizations.en.stringSet[1]"));
    assert_eq!(u.context.extra.get("string_set_index").map(String::as_str), Some("1"));
}

#[test]
fn stale_set_is_locked_not_extracted() {
    let d = Xcstrings.extract(SET.as_bytes(), &opts(None).with_include_locked(true)).unwrap();
    let l: Vec<_> = d.units.iter().filter(|u| !u.translate).map(locos_text).collect();
    assert_eq!(l, ["123", "Old one"]);
}

#[test]
fn unchanged_merge_is_identical() {
    for t in [None, Some("de"), Some("fr")] {
        assert_eq!(Xcstrings.merge(&ex(SET.as_bytes(), t)).unwrap(), SET.as_bytes());
    }
}

#[test]
fn source_language_merge_rewrites_values_in_place() {
    let mut d = ex(SET.as_bytes(), None);
    translate_all(&mut d, |t| format!("Zq{t}qZ"));
    let out = String::from_utf8(Xcstrings.merge(&d).unwrap()).unwrap();
    assert_eq!(out, SET.replace("\"Start workout\",", "\"ZqStart workoutqZ\",").replace("\"Begin workout\",", "\"ZqBegin workoutqZ\",").replace("Plain text", "ZqPlain textqZ"));
    let again = ex(out.as_bytes(), None);
    assert_eq!(texts(&again), ["ZqStart workoutqZ", "ZqBegin workoutqZ", "ZqPlain textqZ"]);
}

#[test]
fn target_language_gets_a_string_set_in_order() {
    let mut d = ex(SET.as_bytes(), Some("fr"));
    translate_all(&mut d, |t| format!("FR {t}"));
    let v = json(&Xcstrings.merge(&d).unwrap());
    let fr = &v["strings"]["Start workout"]["localizations"]["fr"];
    assert_eq!(fr["stringSet"]["state"], "translated");
    // the non-translatable third source string is carried over so the positions stay aligned
    assert_eq!(fr["stringSet"]["values"], serde_json::json!(["FR Start workout", "FR Begin workout", "123"]));
    assert!(fr.get("stringUnit").is_none());
    assert_eq!(v["strings"]["Plain"]["localizations"]["fr"]["stringUnit"]["value"], "FR Plain text");
    // the stale entry gets nothing
    assert!(v["strings"]["Old phrase"]["localizations"].get("fr").is_none());
}

#[test]
fn existing_target_set_is_replaced_and_untranslated_values_keep_old() {
    let mut d = ex(SET.as_bytes(), Some("de"));
    // only the first phrase is translated
    set_all(&mut d, &[(0, "Los geht es")]);
    let v = json(&Xcstrings.merge(&d).unwrap());
    let de = &v["strings"]["Start workout"]["localizations"]["de"]["stringSet"];
    assert_eq!(de["values"], serde_json::json!(["Los geht es", "Training beginnen", "123"]));
    // a unit with no translation at all leaves the target untouched
    let d = ex(SET.as_bytes(), Some("de"));
    assert_eq!(Xcstrings.merge(&d).unwrap(), SET.as_bytes());
}

#[test]
fn string_set_inside_a_plural_variation() {
    let cat = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "k" : {
      "localizations" : {
        "en" : {
          "variations" : {
            "plural" : {
              "one" : { "stringSet" : { "state" : "translated", "values" : ["One file", "A file"] } },
              "other" : { "stringSet" : { "state" : "translated", "values" : ["Many files", "Files"] } }
            }
          }
        }
      }
    }
  }
}
"#;
    let d = ex(cat.as_bytes(), Some("fr"));
    assert_eq!(texts(&d), ["One file", "A file", "Many files", "Files"]);
    assert_eq!(Xcstrings.merge(&d).unwrap(), cat.as_bytes());
    let mut d = ex(cat.as_bytes(), Some("fr"));
    translate_all(&mut d, |t| format!("FR {t}"));
    let v = json(&Xcstrings.merge(&d).unwrap());
    let p = &v["strings"]["k"]["localizations"]["fr"]["variations"]["plural"];
    assert_eq!(p["one"]["stringSet"]["values"], serde_json::json!(["FR One file", "FR A file"]));
    assert_eq!(p["many"]["stringSet"]["values"], serde_json::json!(["FR Many files", "FR Files"]));
}
