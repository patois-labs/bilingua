//! Differential test against the LocOS AsciiDoc filter: 600 random (and randomly broken) files in LF, CRLF and CR
//! with and without a BOM, with the units, styles, inline maps and build digests LocOS produced recorded in
//! `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::*;
use common::*;
use regex::Regex;

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &x in b {
        h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{}:{:016x}", b.len(), h)
}

fn tokens(t: &str) -> Vec<String> {
    Regex::new("\u{27e6}/?[a-z]?\\d+\u{27e7}").unwrap().find_iter(t).map(|m| m.as_str().to_string()).collect()
}

fn map_text(name: &str, i: usize, t: &str) -> Option<String> {
    let toks = tokens(t);
    match name {
        "all" => Some(format!("FR {t}")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "plain" => Some(format!("Plain{i}")),
        "echo" => Some(t.to_string()),
        "cjk" => Some(if toks.is_empty() {
            format!("\u{65e5}\u{672c}\u{8a9e}{i}")
        } else {
            toks.iter().map(|x| format!("\u{3042}{x}")).collect::<String>() + "\u{3002}"
        }),
        "zh" => Some(format!("\u{8bf7}{}\u{3002}", Regex::new(r"\s+").unwrap().replace_all(t, ""))),
        "sep" => Some(format!("A | B , C : D ! {t}\\")),
        "block" => Some(format!(
            "{}{t}",
            ["== X ", "* X ", "// x ", ".T ", "NOTE: x ", "[x] ", "-- ", "image::x[] ", "|=== ", "term:: ", "X "][i % 11]
        )),
        "space" => Some(format!("  {}\n\nend   \n", t.replace(' ', "  "))),
        "lead" => Some(format!(" \t{t} + \\")),
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

/// LocOS inline map of a unit as JSON: `"1": "raw"` or `"1": [open, close]`.
fn codes_json(u: &Unit) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (k, c) in &u.codes {
        if k.starts_with('~') {
            continue;
        }
        let v = match (&c.standalone, &c.open, &c.close) {
            (Some(s), _, _) => serde_json::Value::String(s.clone()),
            (None, Some(o), Some(cl)) => serde_json::json!([o, cl]),
            _ => panic!("odd code {k}"),
        };
        m.insert(k.clone(), v);
    }
    serde_json::Value::Object(m)
}

#[test]
fn matches_locos_on_random_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 609);
    let (mut refused, mut builds, mut nunits) = (0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            if extract(&data).is_ok() {
                bad.push(format!("{name}: LocOS refuses, we accept"));
            }
            refused += 1;
            continue;
        }
        let doc = match extract(&data) {
            Ok(d) => d,
            Err(e) => {
                bad.push(format!("{name}: we refuse ({e}), LocOS accepts"));
                continue;
            }
        };
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let got: Vec<String> = doc.units.iter().map(token_text).collect();
        if got != want {
            bad.push(format!("{name}: units\n  want {want:?}\n  got  {got:?}"));
            continue;
        }
        nunits += want.len();
        let styles: Vec<String> = doc.units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect();
        let wst: Vec<String> = c["styles"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        if styles != wst {
            bad.push(format!("{name}: styles {styles:?} vs {wst:?}"));
        }
        let maps = c["maps"].as_array().unwrap();
        for (k, u) in doc.units.iter().enumerate() {
            if codes_json(u) != maps[k] {
                bad.push(format!("{name}: inline map of unit {k}\n  want {}\n  got  {}", maps[k], codes_json(u)));
            }
        }
        // LocOS writes UTF-8 for a file that was not; Bilingua keeps the file's encoding (B6), so the bytes differ
        if std::str::from_utf8(&data).is_err() {
            continue;
        }
        for (label, d) in c["builds"].as_object().unwrap() {
            let got = fnv(&run(&data, &mapping(label, &want), None));
            if got != d.as_str().unwrap() {
                bad.push(format!("{name}: build {label}"));
            }
            builds += 1;
        }
        if let Some(loc) = c["localized"].as_object() {
            for (lc, d) in loc {
                if lc.ends_with("|map") {
                    continue;
                }
                let label = loc[&format!("{lc}|map")].as_str().unwrap();
                let m = mapping(label, &want);
                let got = run(&data, &m, Some(lc));
                if fnv(&got) != d.as_str().unwrap() {
                    bad.push(format!("{name}: localized {lc} ({label})"));
                }
                builds += 1;
            }
        }
    }
    println!("{} cases, {refused} refused, {nunits} units, {builds} builds, {} differences", cases.len(), bad.len());
    assert!(bad.is_empty(), "{}\n{}", bad.len(), bad.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}
