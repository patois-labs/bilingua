//! Units and language stamps against the Localization OS filter on the oracle corpus (`oracle/corpus/odf`) and on
//! Okapi's own OpenDocument test packages (`tests/data/okapi`, Apache 2.0). `tests/data/oracle_units.json` and
//! `tests/data/golden_stamps.json` were written by `tools/gen_oracle_units.py` and `tools/gen_golden.py` with the LocOS
//! venv. The tests read each file, compare unit texts (inline codes as LocOS tokens) one by one, check that an untouched
//! merge returns the file as it was, and that a pseudo-translated merge into 12 languages reads back with the same
//! units and writes the same language and direction attributes LocOS writes.
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;
use std::collections::BTreeMap;

fn path(name: &str) -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    match name.split_once('/').unwrap() {
        ("corpus", f) => root.join("../oracle/corpus/odf").join(f),
        (_, f) => root.join("tests/data/okapi").join(f),
    }
}

fn json(text: &str) -> serde_json::Map<String, serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(text).unwrap().as_object().unwrap().clone()
}

fn ext_of(name: &str) -> &str {
    name.rsplit('.').next().unwrap()
}

#[test]
fn units_match_locos_on_every_file() {
    let exp = json(include_str!("data/oracle_units.json"));
    assert!(exp.len() >= 19);
    let (mut files, mut total, mut comments) = (0, 0, 0);
    for (name, ent) in &exp {
        let bytes = std::fs::read(path(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ext = ext_of(name);
        let d = extract(ext, &bytes, None);
        let want: Vec<(usize, String, bool)> =
            ent["units"].as_array().unwrap().iter().map(|u| (u[0].as_u64().unwrap() as usize, u[1].as_str().unwrap().to_string(), u[2] == "heading")).collect();
        let got: Vec<(usize, String, bool)> = d.units.iter().map(|u| (p_index(u), ltext(u), u.context.kind.as_deref() == Some("heading"))).collect();
        assert_eq!(got, want, "{name}");
        total += got.len();
        // comments on: LocOS lists them after the body units, numbered from 1,000,000
        let o = OdfOptions { translate_comments: true };
        let f: Box<dyn Filter> = match ext {
            "ods" => Box::new(Ods::with_options(o)),
            "odp" => Box::new(Odp::with_options(o)),
            "odg" => Box::new(Odg::with_options(o)),
            _ => Box::new(Odt::with_options(o)),
        };
        let dc = f.extract(&bytes, &opts(None)).unwrap();
        let got_c: Vec<(usize, String)> = dc.units.iter().filter(|u| p_index(u) >= 1_000_000).map(|u| (p_index(u), ltext(u))).collect();
        let want_c: Vec<(usize, String)> =
            ent["comments"].as_array().map(|a| a.iter().map(|u| (u[0].as_u64().unwrap() as usize, u[1].as_str().unwrap().to_string())).collect()).unwrap_or_default();
        assert_eq!(got_c, want_c, "{name} comments");
        comments += got_c.len();
        assert_eq!(dc.units.len(), d.units.len() + got_c.len(), "{name}");
        // untouched merge
        assert_eq!(filter_for(ext).merge(&d).unwrap(), bytes, "{name}: untouched merge");
        assert_eq!(f.merge(&dc).unwrap(), bytes, "{name}: untouched merge with comments");
        files += 1;
    }
    println!("{files} files, {total} units and {comments} comments, all equal to LocOS");
}

/// Sorted `name=value` of the language and direction attributes in an XML text.
fn stamps(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (pfx, local) in [
        ("fo", "language"),
        ("fo", "country"),
        ("fo", "script"),
        ("style", "language-asian"),
        ("style", "country-asian"),
        ("style", "script-asian"),
        ("style", "language-complex"),
        ("style", "country-complex"),
        ("style", "script-complex"),
        ("style", "writing-mode"),
    ] {
        let pat = format!(" {pfx}:{local}=\"");
        let mut rest = xml;
        while let Some(p) = rest.find(&pat) {
            rest = &rest[p + pat.len()..];
            let v = &rest[..rest.find('"').unwrap()];
            out.push(format!("{local}={v}"));
        }
    }
    out.sort();
    out
}

fn dc_language(meta: &str) -> Option<String> {
    let p = meta.find("<dc:language>")? + "<dc:language>".len();
    Some(meta[p..p + meta[p..].find('<')?].to_string())
}

#[test]
fn pseudo_translated_merge_stamps_like_locos_in_12_languages() {
    let exp = json(include_str!("data/oracle_units.json"));
    let golden = json(include_str!("data/golden_stamps.json"));
    let mut checked = 0;
    for (name, _) in &exp {
        let Some(g) = golden.get(name) else { continue };
        let bytes = std::fs::read(path(name)).unwrap();
        let ext = ext_of(name);
        let f = filter_for(ext);
        let base = extract(ext, &bytes, None);
        if base.units.is_empty() {
            continue;
        }
        for (lang, want) in g.as_object().unwrap() {
            let mut d = f.extract(&bytes, &opts(Some(lang))).unwrap();
            for u in d.units.iter_mut() {
                let (text, map) = locos::to_locos_text(&u.source());
                u.segments[0].target = locos::from_locos_text(&format!("\u{ab}{text}\u{bb}"), &map);
            }
            let out = f.merge(&d).unwrap_or_else(|e| panic!("{name} {lang}: {e}"));
            // same units and tags after a re-read, texts wrapped
            let back = extract(ext, &out, None);
            assert_eq!(back.units.len(), d.units.len(), "{name} {lang}");
            for (a, b) in back.units.iter().zip(&base.units) {
                assert_eq!(ltext(a), format!("\u{ab}{}\u{bb}", ltext(b)), "{name} {lang} unit {}", a.id);
            }
            let ents = entries(&out);
            let part = |n: &str| ents.iter().find(|e| e.name == n).map(|e| String::from_utf8(e.data.clone()).unwrap());
            for (key, file) in [("content", "content.xml"), ("styles", "styles.xml")] {
                let want_v: Vec<String> = want[key].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
                assert_eq!(stamps(&part(file).unwrap_or_default()), want_v, "{name} {lang} {file}");
            }
            let want_meta = want["meta"].as_str().map(str::to_string);
            assert_eq!(part("meta.xml").and_then(|m| dc_language(&m)), want_meta, "{name} {lang} meta");
            checked += 1;
        }
    }
    assert!(checked >= 19 * 12 - 12, "{checked}");
    println!("{checked} file x language merges match LocOS stamps");
}

#[test]
fn okapi_files_round_trip_in_every_family() {
    let exp = json(include_str!("data/oracle_units.json"));
    let mut by_ext: BTreeMap<String, usize> = BTreeMap::new();
    for name in exp.keys() {
        *by_ext.entry(ext_of(name).to_string()).or_default() += 1;
    }
    for e in ["odt", "ods", "odp", "odg"] {
        assert!(by_ext.get(e).copied().unwrap_or(0) >= 1, "{e}");
    }
}
