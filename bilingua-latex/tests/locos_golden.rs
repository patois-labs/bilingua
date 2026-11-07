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

/// Documented difference from LocOS (MIGRATION.md "Differences from LocOS", crate README). GLUED_WORD: when a
/// translation that starts with a letter follows a control word of the skeleton (`\chapter` + `T2 ...`), LocOS writes
/// `\chapterT2`, a different (undefined) control word. Bilingua writes `\chapter{}T2`. In these cases the build is
/// compared with that `{}` taken out again, so every other byte must still match LocOS.
const GLUED_WORD: &[&str] = &[
    "random-3", "random-13", "random-17", "random-26", "random-54", "random-61", "random-72", "random-88",
    "random-120", "random-125", "random-132", "random-155", "random-156", "random-159", "random-165", "random-177",
    "random-192", "random-196", "random-211", "random-223", "random-226", "random-242", "random-259", "random-263",
    "random-274", "random-281", "random-282", "random-288", "random-296", "random-304", "random-308", "random-324",
    "random-343", "random-368", "random-386", "random-402", "random-407", "random-440", "random-444", "random-462",
    "random-469", "random-494", "random-509", "random-553", "random-556", "random-566", "random-567", "random-573",
    "random-596",
];

/// Our build with each `{}` that Bilingua puts between a control word and a letter taken out (the GLUED_WORD cases).
fn unglued(b: &[u8]) -> Vec<u8> {
    let re = regex::bytes::Regex::new(r"(\\[A-Za-z]+)\{\}([A-Za-z])").unwrap();
    re.replace_all(b, &b"$1$2"[..]).into_owned()
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
            let glued = GLUED_WORD.contains(&name);
            let mut differs = false;
            for (label, d) in c["builds"].as_object().unwrap() {
                let map: Vec<(usize, String)> =
                    want.iter().enumerate().filter_map(|(i, t)| map_text(label, i, t).map(|m| (i, m))).collect();
                let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
                let got = build(&data, &m);
                if glued && fnv(&got) != d.as_str().unwrap() {
                    differs = true;
                    assert_eq!(fnv(&unglued(&got)), d.as_str().unwrap(), "{name}: build {label} (GLUED_WORD)");
                } else {
                    assert_eq!(fnv(&got), d.as_str().unwrap(), "{name}: build {label}");
                }
                builds += 1;
            }
            assert!(!glued || differs, "{name}: in GLUED_WORD but now matches LocOS: take it off the list");
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by both");
}
