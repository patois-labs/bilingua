//! Units against the Localization OS filters on the oracle corpus (`oracle/corpus/xcstrings` and `.../xcloc`).
//! `tests/data/oracle_units.json` was written by `tools/gen_oracle_units.py` with the LocOS venv; the test reads
//! the corpus and compares the unit texts (inline codes as LocOS tokens) one by one, and checks that an untouched
//! merge returns every file as it was and that a pseudo-translated merge reads back with the same units.
mod common;
use bilingua_model::*;
use bilingua_xcstrings::{Xcloc, Xcstrings};
use common::*;

fn corpus() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus")
}

fn expected() -> serde_json::Map<String, serde_json::Value> {
    let t = include_str!("data/oracle_units.json");
    serde_json::from_str::<serde_json::Value>(t).unwrap().as_object().unwrap().clone()
}

#[test]
fn units_match_locos_on_the_corpus() {
    let exp = expected();
    assert!(exp.len() >= 6);
    let mut total = 0;
    for (name, units) in &exp {
        let bytes = std::fs::read(corpus().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let f: &dyn Filter = if name.starts_with("xcloc/") { &Xcloc } else { &Xcstrings };
        let d = f.extract(&bytes, &opts(None)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let got: Vec<String> = d.units.iter().filter(|u| u.translate).map(locos_text).collect();
        let want: Vec<String> = units.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(got, want, "{name}");
        total += got.len();
        // untouched merge
        assert_eq!(f.merge(&d).unwrap(), bytes, "{name}: untouched merge");
        // pseudo-translated, into several languages: same units after a re-read in the source language
        for lang in ["de", "ar", "ja", "zh-Hans", "ko", "ru"] {
            let mut d = f.extract(&bytes, &opts(Some(lang))).unwrap();
            translate_all(&mut d, |t| format!("\u{ab}{t}\u{bb}"));
            let out = f.merge(&d).unwrap();
            let back = f.extract(&out, &opts(None)).unwrap();
            let got2: Vec<String> = back.units.iter().filter(|u| u.translate).map(locos_text).collect();
            assert_eq!(got2, want, "{name} {lang}");
        }
    }
    println!("{} files, {total} units, all equal to LocOS", exp.len());
}
