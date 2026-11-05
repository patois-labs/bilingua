//! Differential test against the LocOS mif filter: the corpus, 300 random and damaged files, and Okapi's own mif
//! test resources (read from the Okapi oracle checkout when it is there), with the units, the inline codes and the
//! SHA-1 of the bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_mif::Mif;
use bilingua_model::*;
use common::*;
use std::collections::BTreeMap;

fn tokens_split(t: &str) -> Vec<String> {
    // pieces alternate: text, token, text, ... (like Python's re.split with a capture group)
    let chars: Vec<char> = t.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{27e6}' {
            let mut j = i + 1;
            if j < chars.len() && chars[j] == '/' {
                j += 1;
            }
            if j < chars.len() && chars[j].is_ascii_lowercase() {
                j += 1;
            }
            let ds = j;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > ds && j < chars.len() && chars[j] == '\u{27e7}' {
                out.push(std::mem::take(&mut cur));
                out.push(chars[i..=j].iter().collect());
                i = j + 1;
                continue;
            }
        }
        cur.push(chars[i]);
        i += 1;
    }
    out.push(cur);
    out
}

fn mappings(texts: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    let n = texts.len();
    vec![
        ("all", (0..n).map(|i| (i, format!("T{i} \"q\" \u{e9}\nline2"))).collect()),
        (
            "third",
            (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5} `x' > \\"))).collect(),
        ),
        ("tok", texts.iter().enumerate().map(|(i, t)| (i, format!("{t} Z"))).collect()),
        ("dup", texts.iter().enumerate().map(|(i, t)| (i, format!("{t} {t}"))).collect()),
        (
            "rev",
            texts
                .iter()
                .enumerate()
                .map(|(i, t)| (i, tokens_split(t).into_iter().rev().collect::<String>()))
                .collect(),
        ),
    ]
}

fn okapi_dir() -> Option<std::path::PathBuf> {
    let p = std::env::var("BILINGUA_OKAPI_MIF").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::PathBuf::from(
            r"C:\Users\brent\code\okapi-oracle\resources\okapi-src\okapi\filters\mif\src\test\resources",
        )
    });
    p.is_dir().then_some(p)
}

#[test]
fn matches_locos_on_corpus_random_files_and_okapi_resources() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json"),
    )
    .unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 340, "{}", cases.len());
    let okapi = okapi_dir();
    let (mut parsed, mut refused, mut okapi_done, mut okapi_skipped) = (0, 0, 0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = if c.get("okapi").is_some() {
            let Some(dir) = &okapi else {
                okapi_skipped += 1;
                continue;
            };
            match std::fs::read(dir.join(name)) {
                Ok(d) if sha1(&d) == c["sha1"].as_str().unwrap() => d,
                _ => {
                    okapi_skipped += 1;
                    continue;
                }
            }
        } else {
            hex(c["hex"].as_str().unwrap())
        };
        if c.get("error").is_some() {
            assert!(extract(&data).is_err(), "{name}: LocOS refuses ({}), so must we", c["error"]);
            refused += 1;
            continue;
        }
        let doc = extract(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want: Vec<(String, String)> = c["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| (u[0].as_str().unwrap().to_string(), u[1].as_str().unwrap().to_string()))
            .collect();
        let got: Vec<(String, String)> = doc
            .units
            .iter()
            .map(|u| (u.context.style.clone().unwrap_or_default(), locos_text(u)))
            .collect();
        assert_eq!(got.len(), want.len(), "{name}: unit count");
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "{name}: unit {i}");
        }
        assert_eq!(bilingua_mif::mif_version(&data).unwrap(), c["version"].as_str().unwrap(), "{name}");
        assert_eq!(doc.warnings.len() as u64 > 0, c["skipped"].as_u64().unwrap() > 0, "{name}: skipped marker paragraphs");
        // the inline codes: LocOS inline_map, key by key
        if let Some(maps) = c.get("maps").and_then(|m| m.as_array()) {
            for (i, (u, m)) in doc.units.iter().zip(maps).enumerate() {
                let (_, lmap) = bilingua_model::locos::to_locos_text(&u.source());
                let mut mine: BTreeMap<String, String> = BTreeMap::new();
                for (num, code) in &lmap {
                    if code.letter == 'x' {
                        mine.insert(format!("x{num}"), code.open.clone().unwrap_or_default());
                    } else {
                        mine.insert(format!("g{num}"), code.open.clone().unwrap_or_default());
                        mine.insert(format!("/g{num}"), code.close.clone().unwrap_or_default());
                    }
                }
                let theirs: BTreeMap<String, String> = m
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                    .collect();
                assert_eq!(mine, theirs, "{name}: unit {i} inline codes");
            }
        }
        // builds
        let texts: Vec<String> = got.iter().map(|(_, t)| t.clone()).collect();
        for (key, map) in mappings(&texts) {
            let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let out = build(&data, &m);
            let want = c["builds"][key].as_array().unwrap_or_else(|| panic!("{name}: LocOS failed {key}: {}", c["builds"][key]));
            assert_eq!(
                (sha1(&out), out.len() as u64),
                (want[0].as_str().unwrap().to_string(), want[1].as_u64().unwrap()),
                "{name}: build {key}"
            );
        }
        // the target-language stamp (only when something is translated; LocOS stamps always)
        let third = mappings(&texts).into_iter().find(|(k, _)| *k == "third").unwrap().1;
        if !third.is_empty() {
            for loc in ["de-DE", "ja", "he"] {
                let m: Vec<(usize, &str)> = third.iter().map(|(i, s)| (*i, s.as_str())).collect();
                let out = build_for(&data, &m, Some(loc));
                let want = c["builds"][format!("loc-{loc}")].as_array().unwrap();
                assert_eq!(
                    (sha1(&out), out.len() as u64),
                    (want[0].as_str().unwrap().to_string(), want[1].as_u64().unwrap()),
                    "{name}: localized {loc}"
                );
            }
        }
        // untouched, with a target language: original bytes (the lead's decision; LocOS stamps here)
        assert!(build_for(&data, &[], Some("de-DE")) == data, "{name}");
        assert!(Mif.merge(&doc).unwrap() == data, "{name}");
        parsed += 1;
        if c.get("okapi").is_some() {
            okapi_done += 1;
        }
    }
    println!("golden: {parsed} parsed, {refused} refused, {okapi_done} Okapi files compared, {okapi_skipped} skipped");
    assert!(parsed >= 250 && refused >= 30);
}
