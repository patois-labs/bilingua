//! Differential test against the LocOS rc filter: 500 random (and randomly broken) resource scripts in ten
//! encodings plus the sample, with the units, styles and build digests LocOS produced recorded in
//! `data/locos_golden.json` (made by `tools/gen_golden.py`).
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
        "all" => Some(format!("T{i} \"q\" \u{e9}\nline2\t&x")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "plain" => Some(format!("Plain{i}")),
        "echo" => Some(format!("{t}!")),
        "acc" => Some(["Datei", "&Info &\u{fc}ber", "\u{30d5}\u{30a1}\u{30a4}\u{30eb}", "Open...", "X"][i % 5].to_string()),
        _ => unreachable!(),
    }
}

fn mapping(name: &str, texts: &[String]) -> Vec<(usize, String)> {
    texts.iter().enumerate().filter_map(|(i, t)| map_text(name, i, t).map(|m| (i, m))).collect()
}

fn run(data: &[u8], map: &[(usize, String)], target: Option<&str>) -> Vec<u8> {
    let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
    build_for(data, &m, target)
}

#[test]
fn matches_locos_on_random_files_and_sample() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 519);
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
        let wst: Vec<String> = c["styles"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        assert_eq!(styles(&data), wst, "{name}: styles");
        for (label, d) in c["builds"].as_object().unwrap() {
            let got = run(&data, &mapping(label, &want), None);
            // a build that translates nothing is the original in both
            assert_eq!(fnv(&got), d.as_str().unwrap(), "{name}: build {label}");
            builds += 1;
        }
        if let Some(loc) = c["localized"].as_object() {
            for (lc, d) in loc {
                if lc.ends_with("|map") {
                    continue;
                }
                let mname = loc[&format!("{lc}|map")].as_str().unwrap();
                let got = run(&data, &mapping(mname, &want), Some(lc));
                assert_eq!(fnv(&got), d.as_str().unwrap(), "{name}: localized {lc}");
                builds += 1;
            }
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by both");
}
