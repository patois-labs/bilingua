//! Differential test against the LocOS tex filter: 600 random (and randomly broken) LaTeX documents in four
//! encodings, the corpus and the Okapi test files, with the units and build digests LocOS produced recorded in
//! `data/locos_golden.json` (made by `tools/gen_golden.py`). Builds are compared for UTF-8 inputs only: LocOS exports
//! a Windows-1252 file as UTF-8, Bilingua keeps the file's encoding.
mod common;
use common::*;

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &x in b {
        h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{}:{:016x}", b.len(), h)
}

fn map_text(name: &str, i: usize, t: &str) -> Option<String> {
    match name {
        "all" => Some(format!("T{i} q \u{e9} & 100% {{x}} ~ _ ^ #\nline2\t$")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "plain" => Some(format!("Plain{i}")),
        "echo" => Some(format!("{t}!")),
        "spaced" => Some(format!("  \n{t} \n\n ")),
        _ => unreachable!(),
    }
}

#[test]
fn matches_locos_on_random_files_corpus_and_okapi_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 620);
    let (mut ok, mut refused, mut builds) = (0, 0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            assert!(extract(&data).is_err(), "{name}: LocOS refuses, so must we");
            refused += 1;
            continue;
        }
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        assert_eq!(units(&data), want, "{name}: units");
        if c["builds_compared"].as_bool() == Some(true) {
            for (label, d) in c["builds"].as_object().unwrap() {
                let map: Vec<(usize, String)> =
                    want.iter().enumerate().filter_map(|(i, t)| map_text(label, i, t).map(|m| (i, m))).collect();
                let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
                let got = build(&data, &m);
                assert_eq!(fnv(&got), d.as_str().unwrap(), "{name}: build {label}");
                builds += 1;
            }
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by both");
}
