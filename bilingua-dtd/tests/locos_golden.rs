//! Differential test against the LocOS DTD filter: 500 random files plus the corpus, with the units and the bytes
//! LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use common::*;

fn maps(n: usize) -> Vec<(&'static str, Vec<(usize, String)>)> {
    vec![
        ("all", (0..n).map(|i| (i, format!("T{i} \"q\" 'a' 50% A&B &ref; \u{e9}\nline2"))).collect()),
        ("third", (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5}"))).collect()),
        ("plain", (0..n).map(|i| (i, format!("Plain{i}"))).collect()),
    ]
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(data_dir().join("locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 520);
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
            let w = c["builds"][label].as_str().unwrap();
            assert!(!w.starts_with("error:"), "{name}: LocOS build failed ({w})");
            assert_eq!(got, hex(w), "{name}: build {label}");
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS, {refused} refused by both");
    assert!(ok > 480);
}
