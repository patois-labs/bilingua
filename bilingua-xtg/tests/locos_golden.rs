//! Golden test: random XPress Tags files (all encodings) and the corpus, with what the LocOS filter parsed and built
//! (tests/tools/gen_golden.py). Units and styles must be the same; built bytes must be the same.
mod common;
use bilingua_model::Filter;
use bilingua_xtg::Xtg;
use common::*;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

#[test]
fn matches_locos() {
    let golden: serde_json::Value = serde_json::from_str(include_str!("data/locos_golden.json")).unwrap();
    let (mut cases, mut builds, mut bad) = (0, 0, Vec::new());
    for rec in golden.as_array().unwrap() {
        let name = rec["name"].as_str().unwrap();
        let data = unhex(rec["input"].as_str().unwrap());
        cases += 1;
        let doc = match Xtg.extract(&data, &opts()) {
            Ok(d) => d,
            Err(e) => {
                assert!(!rec["error"].is_null(), "{name}: LocOS parsed it but Bilingua refused: {e}");
                continue;
            }
        };
        assert!(rec["error"].is_null(), "{name}: LocOS refused ({}) but Bilingua parsed it", rec["error"]);
        let got: Vec<(String, String)> =
            doc.units.iter().map(|u| (token_text(u), u.context.style.clone().unwrap_or_default())).collect();
        let want: Vec<(String, String)> = rec["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| (u[0].as_str().unwrap().to_string(), u[1].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(got, want, "{name}: units");
        assert_eq!(Xtg.merge(&doc).unwrap(), data, "{name}: untouched merge");
        for b in rec["builds"].as_array().unwrap() {
            if !b["error"].is_null() {
                continue;
            }
            let mut d = Xtg.extract(&data, &opts()).unwrap();
            for (k, v) in b["mapping"].as_object().unwrap() {
                let i: usize = k.parse().unwrap();
                d.units[i].segments[0].target = from_tokens(v.as_str().unwrap());
            }
            let out = Xtg.merge(&d).unwrap();
            let want = unhex(b["out"].as_str().unwrap());
            builds += 1;
            if out != want {
                bad.push(format!("{name} [{}]\n  mapping {}\n  got  {:?}\n  want {:?}", b["variant"], b["mapping"], String::from_utf8_lossy(&out), String::from_utf8_lossy(&want)));
            }
        }
    }
    assert!(bad.is_empty(), "{} of {builds} builds differ:\n{}", bad.len(), bad.iter().take(6).cloned().collect::<Vec<_>>().join("\n"));
    println!("golden: {cases} files, {builds} builds identical to LocOS");
}
