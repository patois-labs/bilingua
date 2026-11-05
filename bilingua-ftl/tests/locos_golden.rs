//! Differential test against the LocOS Fluent filter: 260 random files plus the corpus, with the units and the
//! bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_ftl::{FtlFilter, Limits};
use bilingua_model::*;
use common::*;

fn mappings(n: usize) -> Vec<(&'static str, Vec<(usize, String)>)> {
    vec![
        ("all", (0..n).map(|i| (i, format!("T{i} {{\"x\"}} \u{e9}\n[b] two\n  .c *d\n\n*e"))).collect()),
        ("third", (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5}"))).collect()),
        ("plain", (0..n).map(|i| (i, format!("Plain{i}"))).collect()),
    ]
}

fn pairs(m: &[(usize, String)]) -> Vec<(usize, &str)> {
    m.iter().map(|(i, s)| (*i, s.as_str())).collect()
}

fn check(c: &serde_json::Value, key: &str, terms: bool) -> bool {
    let name = c["name"].as_str().unwrap();
    let data = hex(c["hex"].as_str().unwrap());
    let g = &c[key];
    let doc = if terms { extract_terms(&data) } else { extract(&data) };
    if g.get("error").is_some() {
        assert!(doc.is_err(), "{name}: LocOS refuses, so must we");
        return false;
    }
    let doc = doc.unwrap_or_else(|e| panic!("{name}: {e}"));
    let want: Vec<(&str, &str)> = g["units"].as_array().unwrap().iter().map(|u| (u[0].as_str().unwrap(), u[1].as_str().unwrap())).collect();
    let got: Vec<(String, String)> = doc.units.iter().map(|u| (flat(u), u.context.style.clone().unwrap())).collect();
    assert_eq!(got.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>(), want, "{name} ({key}): units");
    let lim = { let mut built = Limits::default(); built.translate_terms = terms; built };
    for (label, m) in mappings(want.len()) {
        let merged = build_with(&data, &lim, &pairs(&m), None);
        assert_eq!(String::from_utf8_lossy(&merged), String::from_utf8_lossy(&hex(g["builds"][label].as_str().unwrap())), "{name} ({key}): build {label}");
        if !terms && label == "all" {
            for (loc, h) in g["localized"].as_object().unwrap() {
                let merged = build_with(&data, &lim, &pairs(&m), Some(loc));
                assert_eq!(String::from_utf8_lossy(&merged), String::from_utf8_lossy(&hex(h.as_str().unwrap())), "{name}: localized {loc}");
            }
        }
    }
    true
}

fn build_with(data: &[u8], lim: &Limits, mapping: &[(usize, &str)], locale: Option<&str>) -> Vec<u8> {
    build_doc_with(FtlFilter.extract_with(data, &ExtractOptions::default(), lim).unwrap(), mapping, locale)
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 275);
    let mut ok = 0;
    for c in &cases {
        if check(c, "main", false) {
            ok += 1;
        }
        check(c, "terms", true);
    }
    println!("golden: {ok} files identical to LocOS (units and builds, terms off and on, five locales)");
}
