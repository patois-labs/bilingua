//! Differential test against the LocOS STF filter: the corpus plus 330 random files, with the units, records and the
//! bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::*;
use bilingua_stf::records;
use common::*;

fn mapping(kind: &str, units: &[String]) -> Vec<(usize, String)> {
    units
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match kind {
            "all" => Some((i, format!("[de] {t}"))),
            "plain" => Some((i, format!("Plain{i}"))),
            "esc" => Some((i, format!("Z\tq{i}\r\nline\rend{t}"))),
            "third" => (i % 3 == 0).then(|| (i, format!("Zeta \u{65e5} {t}"))),
            _ => None,
        })
        .collect()
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 350);
    let locales = [("loc", "pt-BR"), ("zh", "zh-Hant-TW"), ("he", "he")];
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
        let off = bilingua_stf::StfFilter.extract(&data, &locos_opts()).unwrap();
        let off_units: Vec<String> = off.units.iter().map(|u| bilingua_model::profile::tokens::to_locos_text(&u.source()).0).collect();
        assert_eq!(off_units, want, "{name}: units without targets");
        assert!(off.units.iter().all(|u| u.segments[0].target.is_empty()), "{name}: stf.load_targets=false loads nothing");
        let styles: Vec<String> = extract(&data).unwrap().units.iter().map(|u| u.context.style.clone().unwrap()).collect();
        let want_styles: Vec<&str> = c["styles"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        assert_eq!(styles, want_styles, "{name}: styles");
        let recs = records(&data).unwrap();
        let want_recs = c["records"].as_array().unwrap();
        assert_eq!(recs.len(), want_recs.len(), "{name}: record count");
        for (r, w) in recs.iter().zip(want_recs) {
            assert_eq!(r.key, w["key"].as_str().unwrap(), "{name}");
            assert_eq!(r.value, w["value"].as_str().unwrap(), "{name}");
            assert_eq!(r.translation.as_deref(), w["translation"].as_str(), "{name}");
            assert_eq!(r.out_of_date, w["out_of_date"].as_bool().unwrap(), "{name}");
            assert_eq!(r.section, w["section"].as_str().unwrap(), "{name}");
            assert_eq!(r.max_length.map(|n| n as u64), w["max_length"].as_u64(), "{name}");
        }
        let utf8 = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8).with_stamp_target_always(false);
        for (label, want_hex) in c["builds"].as_object().unwrap() {
            let (kind, loc) = match label.split_once('@') {
                Some((k, l)) => (k, locales.iter().find(|(n, _)| *n == l).map(|(_, v)| *v)),
                None => (label.as_str(), None),
            };
            let m = mapping(kind, &want);
            let (target, opts) = if loc.is_some() || label == "none@loc" { (Some(loc.unwrap_or("pt-BR")), bilingua_model::profile::locos().merge_options("stf")) } else { (None, utf8.clone()) };
            let opts = if target.is_none() && m.is_empty() { MergeOptions::default() } else { opts };
            // LocOS B-bytes: a plain build() with nothing written returns the upload as it came
            let want_bytes = if target.is_none() && m.is_empty() { data.clone() } else { hex(want_hex.as_str().unwrap()) };
            // both modes: the adapter's (no targets loaded) and the default (existing translations loaded as targets)
            for (mode, eo) in [("locos", locos_opts()), ("default", common::opts())] {
                let eo = eo.with_target_lang(target.map(String::from));
                let got = build_opts(&data, &m, &eo, &opts);
                // the one deliberate difference: a literal backslash in a translation is written as two (issue 804),
                // which LocOS does not do yet. Such a build reads back with the same unit count, and is LocOS's build
                // byte for byte once each translated backslash is written raw: build again with a stand-in character
                // for the backslash and put a single backslash back in its place.
                if m.iter().any(|(_, t)| t.contains('\\')) {
                    assert_eq!(common::extract(&got).unwrap().units.len(), common::extract(&data).unwrap().units.len(), "{name}: build {label} ({mode})");
                    let stand_in: Vec<_> = m.iter().map(|(k, t)| (*k, t.replace('\\', "\u{E000}"))).collect();
                    let raw = build_opts(&data, &stand_in, &eo, &opts);
                    let pua = "\u{E000}".as_bytes();
                    let mut back: Vec<u8> = Vec::with_capacity(raw.len());
                    let mut i = 0;
                    while i < raw.len() {
                        if raw[i..].starts_with(pua) {
                            back.push(b'\\');
                            i += pua.len();
                        } else {
                            back.push(raw[i]);
                            i += 1;
                        }
                    }
                    assert_eq!(back, want_bytes, "{name}: build {label} ({mode}), backslashes written raw");
                    builds += 1;
                    continue;
                }
                assert_eq!(got, want_bytes, "{name}: build {label} ({mode})");
                builds += 1;
            }
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by both");
}
