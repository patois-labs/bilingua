//! Differential test against the LocOS tabular filter: random CSV and TSV files, the LocOS suite's samples and the
//! fixture, with the units, notes and the bytes LocOS builds recorded in `data/locos_golden.json` (made by
//! `tools/gen_golden.py`).
mod common;
use common::*;

fn maps(units: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    let n = units.len();
    let guard = ["=1+1", "+x", "-y", "@z", "\tt", "'q", "\u{ff1d}X"];
    vec![
        ("all", (0..n).map(|i| (i, format!("T{i}, \"q\" \u{e9}\nline2"))).collect()),
        ("third", (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5}"))).collect()),
        ("plain", (0..n).map(|i| (i, format!("Plain{i}"))).collect()),
        ("echo", units.iter().cloned().enumerate().collect()),
        ("guard", (0..n).map(|i| (i, guard[i % 7].to_string())).collect()),
    ]
}

#[test]
fn matches_locos_on_random_files_and_fixtures() {
    let text = std::fs::read_to_string(data_dir().join("locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 800, "{} cases", cases.len());
    let (mut ok, mut refused, mut builds) = (0, 0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        let mut call = if c["fmt"] == "tsv" { Call::tsv() } else { Call::csv() };
        if let Some(cols) = c["columns"].as_str() {
            call = call.cols(cols);
        }
        let targets: Vec<&str> = c["targets"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        call = call.langs(c["src"].as_str().unwrap(), &targets, c["locale"].as_str());
        if c.get("error").is_some() {
            assert!(call.extract(&data).is_err(), "{name}: LocOS refuses ({}), so must we", c["error"]);
            refused += 1;
            continue;
        }
        let want: Vec<&str> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        let doc = call.extract(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let got: Vec<String> = doc.units.iter().map(|u| bilingua_model::plain_text(&u.source())).collect();
        assert_eq!(got, want, "{name}: units");
        // notes: LocOS joins the parts with " | "
        let notes: Vec<String> = doc.units.iter().map(|u| u.context.notes.join(" | ")).collect();
        let want_notes: Vec<&str> = c["notes"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        assert_eq!(notes, want_notes, "{name}: notes");
        // untouched merge: the original bytes
        assert_eq!(call.merge(&doc).unwrap(), data, "{name}: untouched merge");
        assert!(c["untouched"].as_bool().unwrap(), "{name}: LocOS untouched build");
        for (label, m) in maps(&got) {
            let mapping: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let want_build = c["builds"][label].as_str().unwrap();
            let got_build = call.build(&data, &mapping);
            assert_eq!(got_build, hex(want_build), "{name}: build {label} ({:?})", String::from_utf8_lossy(&data));
            builds += 1;
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by both");
}
