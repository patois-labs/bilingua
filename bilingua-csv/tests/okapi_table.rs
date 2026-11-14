//! Okapi's table filters on Okapi's own table test resources (`data/okapi/`, Apache 2.0, Okapi tag v1.48.0, see
//! `data/okapi/NOTICE`). `data/okapi_table.json` (made by `tools/okapi_compare.py`) holds what Okapi 1.48.0
//! (`okf_table_csv`, `okf_table_tsv` and the `.fprm` configurations of Okapi's tests) and the LocOS filter make of
//! each file. Bilingua must equal LocOS (the baseline); the comparison with Okapi is printed and its documented
//! differences are pinned.
mod common;
use common::*;

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Length of the longest common subsequence.
fn lcs(a: &[String], b: &[String]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    for x in a {
        let mut cur = vec![0usize; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y { prev[j] + 1 } else { prev[j + 1].max(cur[j]) };
        }
        prev = cur;
    }
    prev[b.len()]
}

#[test]
fn equals_locos_and_reports_okapi() {
    let text = std::fs::read_to_string(data_dir().join("okapi_table.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 100);
    let strs = |v: &serde_json::Value| -> Vec<String> { v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect() };
    let (mut same_okapi, mut total, mut both_err) = (0, 0, 0);
    let (mut matched, mut okapi_units, mut bl_units) = (0usize, 0usize, 0usize);
    let mut table = vec![];
    for c in &cases {
        let file = c["file"].as_str().unwrap();
        let config = c["config"].as_str().unwrap();
        let data = std::fs::read(data_dir().join("okapi").join(file)).unwrap();
        let mut call = if c["fmt"] == "tsv" { Call::tsv() } else { Call::csv() };
        if let Some(cols) = c["columns"].as_str() {
            call = call.cols(cols);
        }
        // the LocOS side of the comparison was run with source `en` and target `fr`, no build locale
        call = call.langs("en", &["fr"], None);
        let got = call.extract(&data);
        if c["locos_error"].is_string() {
            assert!(got.is_err(), "{file} {config}: LocOS refuses, so must we");
            both_err += 1;
            continue;
        }
        let got: Vec<String> = got.unwrap_or_else(|e| panic!("{file} {config}: {e}")).units.iter().map(|u| norm(&bilingua_model::plain_text(&u.source()))).collect();
        assert_eq!(got, strs(&c["locos"]), "{file} {config}: Bilingua must equal LocOS");
        let okapi = strs(&c["okapi"]);
        total += 1;
        let m = lcs(&got, &okapi);
        matched += m;
        okapi_units += okapi.len();
        bl_units += got.len();
        if got == okapi {
            same_okapi += 1;
        }
        table.push(format!("{file:32} {config:22} okapi {:3} bilingua {:3} common {:3}{}", okapi.len(), got.len(), m, if got == okapi { "  identical" } else { "" }));
    }
    println!("{}", table.join("\n"));
    println!(
        "okapi: {same_okapi} of {total} runs identical; units in order {matched} of {okapi_units} (okapi) / {bl_units} (bilingua) = {:.1}% / {:.1}%; {both_err} configurations refused by both",
        100.0 * matched as f64 / okapi_units as f64,
        100.0 * matched as f64 / bl_units as f64
    );
    // Okapi's own configurations (.fprm) of the same shape as the LocOS mapping agree on these files
    for (file, config) in [("CSVTesting01.csv", "fprm:CSVTesting01"), ("CSVTest_96.txt", "fprm:copy-of-csv_96"), ("strong.csv", "fprm:strong"), ("Locale_defined_TSV_test.txt", "fprm:defined_locales")] {
        let c = cases.iter().find(|c| c["file"] == file && c["config"] == config).unwrap();
        assert_eq!(c["okapi"], c["locos"], "{file} {config}");
    }
}
