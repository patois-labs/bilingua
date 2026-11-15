//! Regression test for the TTX lead of security review run 2 (L6): a stored Document whose splice data was edited
//! to point at less than a self-closed tag is refused at merge, not a panic.

mod common;

use bilingua_model::*;
use common::*;

#[test]
fn damaged_splice_data_of_an_empty_target_is_refused() {
    let raw = "<?xml version='1.0'?>\r\n<TRADOStag Version=\"2.0\"><FrontMatter><UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/></FrontMatter><Body><Raw>\r\n<Tu><Tuv Lang=\"EN-US\">Hello</Tuv><Tuv Lang=\"DE-DE\"/></Tu>\r\n</Raw></Body></TRADOStag>";
    let mut d = extract_with(&DEFAULT, raw.as_bytes());
    set(&mut d, 0, "Hallo");
    // the untouched splice data writes the translation
    assert!(String::from_utf8(DEFAULT.merge(&d).unwrap()).unwrap().contains("Hallo"));
    let meta = d.units[0].hidden("meta").expect("splice data").to_string();
    let mut f: Vec<String> = meta.split('\u{1f}').map(str::to_string).collect();
    assert_eq!(f[0], "s");
    for len in [0usize, 1] {
        f[3] = (f[2].parse::<usize>().unwrap() + len).to_string();
        let mut bad = d.clone();
        bad.units[0].set_hidden("meta", f.join("\u{1f}"));
        let r = std::thread::spawn(move || DEFAULT.merge(&bad).map(|_| ())).join().expect("the merge panicked");
        assert!(matches!(r, Err(Error::Write(_))), "{r:?}");
    }
}
