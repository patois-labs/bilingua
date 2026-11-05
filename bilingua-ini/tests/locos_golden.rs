//! Differential test against the LocOS INI filter: 400 random files plus the corpus, with the units and the
//! bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_ini::records;
use common::*;

fn maps(n: usize) -> Vec<(&'static str, Vec<(usize, String)>)> {
    vec![
        ("all", (0..n).map(|i| (i, format!("T{i} \"q\" \u{e9}\nline2"))).collect()),
        ("third", (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5}"))).collect()),
        ("plain", (0..n).map(|i| (i, format!("Plain{i}"))).collect()),
    ]
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 429);
    let (mut ok, mut refused) = (0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            assert!(extract(&data).is_err(), "{name}: LocOS refuses, so must we");
            refused += 1;
            continue;
        }
        let want: Vec<&str> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        assert_eq!(units(&data), want, "{name}: units");
        for (label, m) in maps(want.len()) {
            let mapping: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = build(&data, &mapping);
            assert_eq!(got, hex(c["builds"][label].as_str().unwrap()), "{name}: build {label}");
        }
        let recs = records(&data).unwrap();
        let want_recs = c["records"].as_array().unwrap();
        assert_eq!(recs.len(), want_recs.len(), "{name}: record count");
        for (r, w) in recs.iter().zip(want_recs) {
            assert_eq!(r.key, w["key"].as_str().unwrap(), "{name}");
            assert_eq!(r.value, w["value"].as_str().unwrap(), "{name}");
            assert_eq!(r.comment, w["comment"].as_str().unwrap(), "{name}");
            assert_eq!(r.section, w["context"].as_str().unwrap(), "{name}");
            assert_eq!(r.unit_index.map(|i| i as u64), w["unit_index"].as_u64(), "{name}");
            assert_eq!(r.duplicate, w["duplicate"].as_bool().unwrap(), "{name}");
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS, {refused} refused by both");
}
