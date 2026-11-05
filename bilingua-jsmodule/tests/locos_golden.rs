//! Differential test against the LocOS js filter: 450 random modules plus the oracle corpus, with the units, the
//! placeholders, the records and the bytes LocOS builds recorded in `data/locos_golden.json`
//! (made by `tools/gen_golden.py`).
mod common;
use bilingua_jsmodule::records;
use common::*;

fn maps(texts: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    vec![
        (
            "all",
            (0..texts.len())
                .map(|i| (i, format!("T{i} \"q\" \u{e9}\nline2")))
                .collect(),
        ),
        (
            "third",
            (0..texts.len())
                .step_by(3)
                .map(|i| (i, format!("Zeta {i} \u{65e5}")))
                .collect(),
        ),
        (
            "tok",
            texts
                .iter()
                .enumerate()
                .map(|(i, t)| (i, format!("{t} Z")))
                .collect(),
        ),
    ]
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json"),
    )
    .unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 450);
    let (mut ok, mut refused) = (0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if name == "qt_linguist.ts" {
            // LocOS hands an XML .ts to the Qt Linguist filter; Bilingua's js filter refuses it (the facade routes it).
            assert!(extract(&data).is_err());
            continue;
        }
        if c.get("error").is_some() {
            assert!(
                extract(&data).is_err(),
                "{name}: LocOS refuses ({}), so must we",
                c["error"]
            );
            refused += 1;
            continue;
        }
        let want: Vec<String> = c["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u.as_str().unwrap().to_string())
            .collect();
        assert_eq!(texts(&data), want, "{name}: units");
        // the placeholder strings of every unit, in order
        let doc = extract(&data).unwrap();
        let want_points = c["points"].as_array().unwrap();
        for (u, w) in doc.units.iter().filter(|u| u.translate).zip(want_points) {
            let mut ids: Vec<&String> = u
                .codes
                .keys()
                .filter(|k| k.as_bytes()[0].is_ascii_digit())
                .collect();
            ids.sort_by_key(|k| k.parse::<usize>().unwrap());
            let got: Vec<&str> = ids
                .iter()
                .map(|k| u.codes[*k].standalone.as_deref().unwrap())
                .collect();
            let w: Vec<&str> = w
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap())
                .collect();
            assert_eq!(got, w, "{name}: placeholders of {}", u.id);
        }
        for (label, m) in maps(&want) {
            let mapping: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = build(&data, &mapping);
            assert_eq!(
                got,
                hex(c["builds"][label].as_str().unwrap()),
                "{name}: build {label}"
            );
        }
        let recs = records(&data).unwrap();
        let want_recs = c["records"].as_array().unwrap();
        assert_eq!(recs.len(), want_recs.len(), "{name}: record count");
        for (r, w) in recs.iter().zip(want_recs) {
            assert_eq!(r.key, w[0].as_str().unwrap(), "{name}: record key");
            assert_eq!(r.value, w[1].as_str().unwrap(), "{name}: record value");
        }
        ok += 1;
    }
    assert!(ok >= 400, "only {ok} files opened");
    assert!(refused >= 30, "only {refused} refusals");
    println!("{ok} files match LocOS, {refused} refused by both");
}
