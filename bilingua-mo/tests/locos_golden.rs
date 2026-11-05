//! Differential test against the LocOS MO filter: random catalogs plus the fixtures, with the units and the bytes
//! LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
//!
//! UTF-8 catalogs must come out byte for byte as LocOS writes them. A catalog in a legacy charset is compared as a
//! catalog (LocOS always writes UTF-8; this crate keeps the file's charset when every character fits).

mod common;
use common::*;
use std::collections::BTreeMap;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn mapping(name: &str, n: usize) -> Vec<(usize, String)> {
    match name {
        "all" => (0..n).map(|i| (i, if i % 2 == 1 { format!("T{i} \u{e9} %d {{0}}") } else { format!("T{i} \u{e9}\n[b] two") })).collect(),
        "third" => (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5}"))).collect(),
        _ => (0..n).map(|i| (i, format!("Plain{i}"))).collect(),
    }
}

fn build(data: &[u8], m: &[(usize, String)], locale: Option<&str>) -> Vec<u8> {
    let mut d = match locale {
        Some(l) => ex_to(data, l),
        None => ex(data),
    };
    for (i, t) in m {
        let id = d.units[*i].id.clone();
        set(&mut d, &id, t);
    }
    merged(&d)
}

/// True when the catalog declares a legacy charset (LocOS writes UTF-8 for it, this crate keeps the charset).
fn is_legacy(bytes: &[u8]) -> bool {
    let label = Gettext::parse(bytes).charset;
    bilingua_model::encoding_rs::Encoding::for_label(label.as_bytes()).is_some_and(|e| e != bilingua_model::encoding_rs::UTF_8)
}

/// The catalog as decoded text; the charset lines are left out (they say different, equally true, things).
fn catalog(bytes: &[u8]) -> BTreeMap<String, String> {
    let g = Gettext::parse(bytes);
    let enc = bilingua_model::encoding_rs::Encoding::for_label(g.charset.as_bytes()).unwrap_or(bilingua_model::encoding_rs::UTF_8);
    let dec = |b: &[u8]| enc.decode_without_bom_handling(b).0.into_owned();
    g.messages
        .iter()
        .map(|(k, v)| {
            let mut v = dec(v);
            if k.is_empty() {
                v = v.lines().filter(|l| !l.to_ascii_lowercase().starts_with("content-type")).collect::<Vec<_>>().join("
");
            }
            (dec(k), v)
        })
        .collect()
}

fn same(case: &str, what: &str, legacy: bool, got: &[u8], want: &[u8], bad: &mut Vec<String>) {
    let ok = if legacy { catalog(got) == catalog(want) } else { got == want };
    if !ok {
        if let Ok(dir) = std::env::var("MO_GOLDEN_DUMP") {
            let tag: String = what.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
            std::fs::write(format!("{dir}/{case}.{tag}.got.mo"), got).unwrap();
        }
        let (g, w) = (catalog(got), catalog(want));
        let diff: Vec<String> = g.keys().chain(w.keys()).filter(|k| g.get(*k) != w.get(*k)).map(|k| format!("{k:?}: got {:?} want {:?}", g.get(k), w.get(k))).collect();
        bad.push(format!("{case}: {what}: {}", if diff.is_empty() { "same catalog, different bytes".to_string() } else { diff.join("; ") }));
    }
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 180);
    let (mut parsed, mut refused, mut builds) = (0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            assert!(try_ex(&data).is_err(), "{name}: LocOS refuses ({}), so must we", c["error"]);
            refused += 1;
            continue;
        }
        let want: Vec<&str> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        let doc = ex(&data);
        let got = texts(&doc);
        if got.iter().map(String::as_str).collect::<Vec<_>>() != want {
            bad.push(format!("{name}: units {got:?} vs {want:?}"));
            continue;
        }
        parsed += 1;
        let legacy = is_legacy(&data);
        let Some(b) = c.get("builds") else { continue };
        for (label, h) in b.as_object().unwrap() {
            let m = mapping(label, want.len());
            same(name, &format!("build {label}"), legacy, &build(&data, &m, None), &hex(h.as_str().unwrap()), &mut bad);
            builds += 1;
        }
        for (key, h) in c["localized"].as_object().unwrap() {
            let (label, loc) = key.split_once(':').unwrap();
            let m = mapping(label, want.len());
            same(name, &format!("localized {key}"), legacy, &build(&data, &m, Some(loc)), &hex(h.as_str().unwrap()), &mut bad);
            builds += 1;
        }
    }
    println!("golden: {parsed} catalogs parsed like LocOS, {refused} refused like LocOS, {builds} builds compared");
    assert!(bad.is_empty(), "{} differences, first ones:\n{}", bad.len(), bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
}
