//! Bilingua against the LocOS filter (frozen output of `oracle/locos_dump.py` over the corpus files in
//! `tests/data/locos_oracle.json`): the same units, the same token text, the same message index.

mod common;
use common::*;

const FROZEN: &str = include_str!("data/locos_oracle.json");

fn file(name: &str) -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read(root.join("../oracle/corpus/xmb").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn units_match_locos_on_every_corpus_file() {
    let v: serde_json::Value = serde_json::from_str(FROZEN).unwrap();
    let mut total = 0;
    for (name, units) in v.as_object().unwrap() {
        let d = ex(&file(name));
        let want: Vec<(String, String)> =
            units.as_array().unwrap().iter().map(|p| (p[0].to_string(), p[1].as_str().unwrap().to_string())).collect();
        let got: Vec<(String, String)> = d.units.iter().map(|u| (u.context.extra["index"].clone(), locos(u))).collect();
        assert_eq!(got, want, "{name}");
        total += want.len();
    }
    assert!(total >= 25, "{total}");
}

#[test]
fn pseudo_translated_files_match_the_locos_builds_byte_for_byte() {
    // `build_localized(file, {index: "FR " + text}, "fr")` of the LocOS filter, frozen as locos_build_<name>.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    for name in ["app.fr.xtb", "app.xmb", "cart.xmb", "languages.xmb", "languages.xtb"] {
        let mut d = ex_to(&file(name), "fr");
        pseudo(&mut d);
        let got = match d.format.as_str() {
            "xmb" => bilingua_xmb::Xmb.to_xtb(&d),
            _ => bilingua_xmb::Xtb.merge_localized(&d),
        }
        .unwrap();
        let want = std::fs::read(root.join(format!("locos_build_{name}"))).unwrap();
        assert_eq!(String::from_utf8_lossy(&got), String::from_utf8_lossy(&want), "{name}");
    }
}
