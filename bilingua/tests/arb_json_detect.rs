//! A `.json` file in ARB shape (a string `@@locale`) goes to the ARB filter, as LocOS does; plain JSON stays JSON.

#[test]
fn json_with_a_string_locale_is_arb() {
    let arb = br#"{"@@locale": "en", "hello": "Hello {name}", "@hello": {"placeholders": {"name": {}}}}"#;
    let f = bilingua::detect("app_en.json", arb).expect("detected");
    assert_eq!(f.id(), "arb");
}

#[test]
fn plain_json_stays_json() {
    let plain = br#"{"hello": "Hello", "nested": {"bye": "Bye"}}"#;
    let f = bilingua::detect("strings.json", plain).expect("detected");
    assert_eq!(f.id(), "json");
}

#[test]
fn a_non_string_locale_does_not_make_json_arb() {
    let odd = br#"{"@@locale": 5, "hello": "Hello"}"#;
    let f = bilingua::detect("strings.json", odd).expect("detected");
    assert_eq!(f.id(), "json");
}
