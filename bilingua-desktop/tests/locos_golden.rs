//! Differential test against the LocOS filters: 600 random `.desktop` and 600 random `.catkeys` files plus the
//! fixtures, with the units, `records()` and a digest of every build LocOS makes recorded in
//! `data/locos_golden_*.json` (made by `tools/gen_golden.py`); and the locale spellings in `data/locos_locales.json`.
//!
//! Known, documented differences: LocOS refuses a file with nothing to translate (Bilingua returns a document
//! with no units), and LocOS sets a catkeys header language even when nothing is translated (Bilingua returns the
//! original bytes; the lead decision of 11-04). The generator only builds with non-empty mappings.

mod common;
use bilingua_desktop::*;
use common::*;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &x in b {
        h = (h ^ x as u64).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn mapping(label: &str, n: usize) -> Vec<(usize, String)> {
    match label {
        "all" => (0..n).map(|i| (i, format!("T{i} \"q\" \u{e9}; x\nline2\t\\ end "))).collect(),
        "third" => (0..n).step_by(3).map(|i| (i, format!(" Zeta {i} \u{65e5};\u{672c}"))).collect(),
        _ => (0..n).map(|i| (i, format!("Plain{i}"))).collect(),
    }
}

fn run(fmt: &str, file: &str) -> (usize, usize) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(file);
    let cases: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(cases.len() >= 600);
    let (mut ok, mut refused) = (0, 0);
    let name_ext = format!("x.{fmt}");
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if let Some(err) = c.get("error") {
            let r = extract_any(&name_ext, &data);
            if err == "EmptyDocumentError" {
                assert_eq!(r.unwrap().units.len(), 0, "{name}: LocOS says empty");
            } else {
                assert!(r.is_err(), "{name}: LocOS refuses ({err}), so must we");
                refused += 1;
            }
            continue;
        }
        let want: Vec<&str> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        let got = texts(&extract_any(&name_ext, &data).unwrap_or_else(|e| panic!("{name}: {e}")));
        assert_eq!(got, want, "{name}: units");
        for (key, digest) in c["builds"].as_object().unwrap() {
            let digest = digest.as_str().unwrap();
            if digest.starts_with("ERR:") {
                continue;
            }
            let (label, loc) = key.split_once('|').unwrap();
            let loc = (loc != "None").then_some(loc);
            let m = mapping(label, want.len());
            let tr: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let out = if fmt == "desktop" { build_d(&data, &tr, loc) } else { build_c(&data, &tr, loc) };
            // The one documented difference: LocOS stamps a catkeys header even when nothing is translated.
            if tr.is_empty() && fmt == "catkeys" && loc.is_some() {
                continue;
            }
            assert_eq!(fnv(&out), digest, "{name}: build {key}\n{:?}\n{:?}", String::from_utf8_lossy(&data), String::from_utf8_lossy(&out));
        }
        let recs = if fmt == "desktop" { desktop_records(&data) } else { catkeys_records(&data) }.unwrap();
        let want_recs = c["records"].as_array().unwrap();
        assert_eq!(recs.len(), want_recs.len(), "{name}: record count");
        for (r, w) in recs.iter().zip(want_recs) {
            assert_eq!(r.key, w["key"].as_str().unwrap(), "{name}");
            assert_eq!(r.value, w["value"].as_str().unwrap(), "{name}");
            assert_eq!(r.comment, w["comment"].as_str().unwrap(), "{name}");
            assert_eq!(r.context, w["context"].as_str().unwrap(), "{name}");
            assert_eq!(r.unit_index.map(|i| i as u64), w["unit_index"].as_u64(), "{name}");
            assert_eq!(r.duplicate, w["duplicate"].as_bool().unwrap(), "{name}");
        }
        ok += 1;
    }
    (ok, refused)
}

#[test]
fn desktop_matches_locos_on_random_files_and_fixtures() {
    let (ok, refused) = run("desktop", "locos_golden_desktop.json");
    println!("desktop golden: {ok} files identical to LocOS, {refused} refused by both");
    assert!(ok > 400);
}

#[test]
fn catkeys_matches_locos_on_random_files_and_fixtures() {
    let (ok, refused) = run("catkeys", "locos_golden_catkeys.json");
    println!("catkeys golden: {ok} files identical to LocOS, {refused} refused by both");
    assert!(ok > 400);
}

#[test]
fn locale_spellings_match_locos() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_locales.json");
    let rows: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(rows.len() > 1500);
    let mut bad = Vec::new();
    for r in &rows {
        let code = r["code"].as_str().unwrap();
        let (name, posix) = (r["name"].as_str().unwrap(), r["posix"].as_str().unwrap());
        if language_name(code) != name {
            bad.push(format!("name {code:?}: ours {:?}, LocOS {name:?}", language_name(code)));
        }
        if posix_locale(code) != posix {
            bad.push(format!("posix {code:?}: ours {:?}, LocOS {posix:?}", posix_locale(code)));
        }
    }
    assert!(bad.is_empty(), "{} differences:\n{}", bad.len(), bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}
