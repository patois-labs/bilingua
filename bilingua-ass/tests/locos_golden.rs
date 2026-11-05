//! Differential test against the LocOS ASS filter: 260 random files, the corpus and hostile bytes, with the units and
//! the bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
//!
//! Known, intended differences (each normalised or skipped here and written down in the README):
//! - a UTF-16 upload: LocOS writes UTF-8 with a BOM, Bilingua's UTF-8 option writes UTF-8 without one;
//! - a `[Script Info]` header on the last line with no line end: LocOS glues `Language:` onto it, Bilingua adds the
//!   missing line end first;
//! - translations holding a literal `{` or a backslash are not in the golden (Bilingua writes them safely).
mod common;
use common::*;

fn mapping(v: &serde_json::Value) -> Vec<(usize, String)> {
    let mut m: Vec<(usize, String)> = v.as_object().unwrap().iter().map(|(k, t)| (k.parse().unwrap(), t.as_str().unwrap().to_string())).collect();
    m.sort();
    m
}

fn norm(data: &[u8], out: Vec<u8>) -> Vec<u8> {
    // (also: bytes that start like a UTF-8 BOM but are not UTF-8 are read as a code page, and LocOS still puts a BOM on)
    let odd_bom = data.starts_with(&[0xEF, 0xBB, 0xBF]) && std::str::from_utf8(data).is_err();
    let utf16 = data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]);
    if (utf16 || odd_bom) && out.starts_with(&[0xEF, 0xBB, 0xBF]) {
        out[3..].to_vec()
    } else {
        out
    }
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 290);
    let (mut ok, mut refused, mut builds, mut skipped) = (0, 0, 0, 0);
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
        if c.get("units_only").is_some() {
            ok += 1; // Turkish-i Title header: LocOS keeps a typed line break there, Bilingua flattens it
            continue;
        }
        for (label, hexv) in c["builds"].as_object().unwrap() {
            let m = if label == "empty" { vec![] } else { mapping(&c["maps"][label]) };
            let mm: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            // LocOS `build` hands back the upload itself when nothing is written or every text is its own source
            let untouched = mm.iter().all(|(i, t)| want[*i] == *t);
            let got = if untouched { build(&data, &[]) } else { norm(&data, build_locos(&data, &mm)) };
            let want_b = if untouched { hex(hexv.as_str().unwrap()) } else { norm(&data, hex(hexv.as_str().unwrap())) };
            assert_eq!(String::from_utf8_lossy(&got), String::from_utf8_lossy(&want_b), "{name}: build {label}");
            assert_eq!(got, want_b, "{name}: build {label} bytes");
            builds += 1;
        }
        for (key, hexv) in c["localized"].as_object().unwrap() {
            let (lc, which) = key.split_once('|').unwrap();
            let m = if which == "pseudo" { mapping(&c["maps"]["pseudo"]) } else { vec![] };
            let mm: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let want_b = norm(&data, hex(hexv.as_str().unwrap()));
            if String::from_utf8_lossy(&want_b).contains("]Language:") && !String::from_utf8_lossy(&want_b).contains("]\nLanguage:") && !String::from_utf8_lossy(&want_b).contains("]\r") {
                skipped += 1; // the LocOS glued-header quirk
                continue;
            }
            let got = norm(&data, build_localized(&data, &mm, Some(lc)));
            assert_eq!(String::from_utf8_lossy(&got), String::from_utf8_lossy(&want_b), "{name}: localized {key}");
            builds += 1;
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by both, {skipped} glued-header skips");
}
