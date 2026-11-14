//! The unit's hidden slot (and the move of old `~` codes into it) and the binary skeleton.

use bilingua_model::*;

#[test]
fn hidden_values_are_not_codes_and_round_trip_in_json() {
    let u = Unit::new("u1", vec![Inline::text("Hi")]).with_hidden("raw", "\"Hi\"").with_hidden("lead", " ");
    assert_eq!(u.hidden("raw"), Some("\"Hi\""));
    assert!(u.has_hidden("lead") && !u.has_hidden("trail"));
    assert!(u.codes.is_empty());
    let j = serde_json::to_string(&u).unwrap();
    assert!(j.contains(r#""hidden":{"lead":" ","raw":"\"Hi\""}"#), "{j}");
    assert_eq!(serde_json::from_str::<Unit>(&j).unwrap(), u);
    // empty: left out of the JSON
    assert!(!serde_json::to_string(&Unit::new("u2", vec![])).unwrap().contains("hidden"));
}

#[test]
fn old_tilde_codes_read_as_hidden_values() {
    let old = r#"{"id":"u1","segments":[{"id":"u1","source":[{"text":"x"},{"tag":{"id":"1","kind":"standalone","display":"%s"}}]}],
        "codes":{"1":{"open":null,"close":null,"standalone":"%s","label":"placeholder"},
                 "~raw":{"open":null,"close":null,"standalone":"x%s","label":null},
                 "~v":{"open":null,"close":null,"standalone":null,"label":"attr"}}}"#;
    let u: Unit = serde_json::from_str(old).unwrap();
    assert_eq!(u.hidden("raw"), Some("x%s"));
    assert_eq!(u.hidden("v"), Some("attr"));
    assert_eq!(u.codes.keys().collect::<Vec<_>>(), vec!["1"]);
}

#[test]
fn binary_skeleton_is_base64_in_json() {
    let s = Skeleton::binary(vec![0xDE, 0x12, 0x04, 0x95], ParserInfo::new("bilingua-mo", "0.1.0", 2));
    let j = serde_json::to_string(&s).unwrap();
    assert!(j.starts_with(r#"{"binary":{"data":"3hIElQ==""#), "{j}");
    assert_eq!(serde_json::from_str::<Skeleton>(&j).unwrap(), s);
    assert_eq!(s.parser().map(|p| p.schema), Some(2));
}
