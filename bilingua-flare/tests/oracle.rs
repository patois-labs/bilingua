//! The units LocOS gives for every corpus file (`tests/data/oracle_units.json`, made by `tools/gen_oracle_units.py`
//! from the LocOS filter) against the units Bilingua gives, texts in LocOS form, in order.
mod common;
use common::*;

fn corpus() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/flprjzip")
}

/// Differences between LocOS and Bilingua that come from `bilingua-html`: none since D4 (the HTML filter is LocOS's
/// on the shared engine). Before D4: character references were decoded in Bilingua units and `<code>` was one
/// standalone code.
fn known(locos: &str, _style: &str) -> Option<String> {
    Some(locos.to_string())
}

#[test]
fn units_match_locos_on_the_corpus() {
    let oracle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/oracle_units.json")).unwrap()).unwrap();
    let mut diffs = Vec::new();
    let (mut total, mut same) = (0, 0);
    for (name, want) in oracle.as_object().unwrap() {
        let bytes = std::fs::read(corpus().join(name)).unwrap();
        let d = extract(&bytes);
        let got = loc_texts(&d);
        let want: Vec<(String, String)> = want
            .as_array()
            .unwrap_or_else(|| panic!("{name}: LocOS refused it: {want}"))
            .iter()
            .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_str().unwrap().to_string()))
            .filter_map(|(t, s)| known(&t, &s).map(|t| (t, s)))
            .collect();
        let n = got.len().max(want.len());
        for i in 0..n {
            total += 1;
            let g = got.get(i);
            let w = want.get(i).map(|w| &w.0);
            if g == w {
                same += 1;
            } else {
                diffs.push(format!("{name} #{i}: bilingua {g:?} / locos {w:?}"));
            }
        }
    }
    eprintln!("{same} of {total} units match; {} differ", diffs.len());
    for d in &diffs {
        eprintln!("{d}");
    }
    assert!(diffs.is_empty(), "{} differences", diffs.len());
}

#[test]
fn styles_match_locos_on_the_corpus() {
    // LocOS's `style` of a unit: empty for text, `a11y:*` for attributes, `flare:*` for the Flare parts.
    let oracle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/oracle_units.json")).unwrap()).unwrap();
    for (name, want) in oracle.as_object().unwrap() {
        let d = extract(&std::fs::read(corpus().join(name)).unwrap());
        for (i, w) in want.as_array().unwrap().iter().enumerate() {
            let style = w[1].as_str().unwrap();
            let u = &d.units[i];
            let got = u.context.extra.get("locos-style").cloned().or_else(|| u.context.style.clone().filter(|s| s.starts_with("flare:"))).unwrap_or_default();
            assert_eq!(got, style, "{name} #{i}");
        }
    }
}
