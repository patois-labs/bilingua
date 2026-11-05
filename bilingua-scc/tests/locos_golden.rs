//! Differential test against the LocOS SCC filter: random captions of every kind plus the corpus, with the units
//! and the bytes (or refusal text) LocOS builds for ten mappings recorded in `data/locos_golden.json` (made by
//! `tools/gen_golden.py`).
mod common;
use common::*;

fn maps(texts: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    let n = texts.len();
    let re_words = |t: &str| {
        // every run of letters becomes "Mot" (Python [^\W\d_]+)
        let mut out = String::new();
        let mut in_run = false;
        for c in t.chars() {
            if bilingua_model::py_isalpha_char(c) {
                if !in_run {
                    out.push_str("Mot");
                }
                in_run = true;
            } else {
                in_run = false;
                out.push(c);
            }
        }
        out
    };
    vec![
        ("same", texts.iter().cloned().enumerate().collect()),
        ("words", texts.iter().enumerate().map(|(i, t)| (i, re_words(t))).collect()),
        ("extra", texts.iter().enumerate().map(|(i, t)| (i, format!("{t} ok"))).collect()),
        ("long", (0..n).map(|i| (i, vec!["Voici une phrase vraiment longue"; i % 5 + 1].join(" "))).collect()),
        (
            "latin",
            (0..n)
                .map(|i| (i, "\u{dc}ber na\u{ef}ve caf\u{e9} \u{266a} \u{ab}oui\u{bb} \u{2013} l\u{2019}\u{e9}t\u{e9}\u{2026} \u{d1}and\u{fa} \u{bf}qu\u{e9}? {x} ~ | ^ _ \\ *".to_string()))
                .collect(),
        ),
        ("cjk", (0..n).map(|i| (i, if i % 4 == 0 { "\u{65e5}\u{672c}\u{8a9e}" } else { "ok" }.to_string())).collect()),
        ("third", (0..n).step_by(3).map(|i| (i, format!("Zeta {i}"))).collect()),
        ("short", (0..n).map(|i| (i, "x".repeat(i % 39 + 1))).collect()),
        ("lines", (0..n).map(|i| (i, "Ligne un\nLigne deux\nLigne trois".to_string())).collect()),
        ("nbsp", (0..n).map(|i| (i, "a\u{a0}b  c \u{2009}d".to_string())).collect()),
    ]
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 140);
    let (mut ok, mut refused, mut builds, mut refusals) = (0, 0, 0, 0);
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
        for (label, m) in maps(&want) {
            // the LocOS byte guard (app/filters/base.py) returns the upload when every written text is the unit's
            // own text; that is the adapter's job here, so the filter is asked for the untouched merge
            let identity = m.iter().all(|(i, t)| *t == want[*i]);
            let got = if identity {
                try_build(&data, &[])
            } else {
                try_build_with(&data, &m, &bilingua_model::MergeOptions::locos())
            };
            let g = &c["builds"][label];
            match g.get("error") {
                Some(msg) => {
                    let e = got.expect_err(&format!("{name}: {label}: LocOS refuses")).to_string().replacen("cannot write: ", "", 1);
                    assert_eq!(e, msg.as_str().unwrap(), "{name}: {label}: refusal text");
                    refusals += 1;
                }
                None => {
                    let got = got.unwrap_or_else(|e| panic!("{name}: {label}: {e}"));
                    if identity {
                        assert_eq!(got, data, "{name}: {label}: identity");
                    }
                    let want_bytes = hex(g["hex"].as_str().unwrap());
                    assert_eq!(String::from_utf8_lossy(&got), String::from_utf8_lossy(&want_bytes), "{name}: build {label}");
                    assert_eq!(got, want_bytes, "{name}: build {label}");
                    builds += 1;
                }
            }
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds, {refusals} refusals with the same text), {refused} refused by both");
}
