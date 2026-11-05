//! Differential test against the LocOS wiki filters: 300 random (and randomly broken) pages in both dialects, a few
//! hostile files and the two samples, with the units and build digests LocOS produced recorded in
//! `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::*;
use bilingua_wiki::{DokuWiki, MediaWiki};
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
        "all" => Some(format!("T{i} | = [[x]] ''y'' {{{{z}}}} * \u{e9}\nline2 ~~~~ __TOC__ <b> [http://q.r s] **b** //i// \\\\ ")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "plain" => Some(format!("Plain{i}")),
        "echo" => Some(format!("{t}!")),
        "pre" => Some(format!("FR {t}")),
        "list" => Some(if i % 2 == 1 { format!("* {t}") } else { format!("= {t}") }),
        "cjk" => Some(format!("\u{65e5}\u{672c}{}\u{8a9e}", t.replace(' ', ""))),
        _ => unreachable!(),
    }
}

#[test]
fn matches_locos_on_random_files_and_samples() {
    // BILINGUA_WIKI_GOLDEN points at a bigger file made with GOLDEN_SEED / GOLDEN_N (see tools/gen_golden.py)
    let path = std::env::var("BILINGUA_WIKI_GOLDEN").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/locos_golden.json").into());
    let text = std::fs::read_to_string(path).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 318 || std::env::var("BILINGUA_WIKI_GOLDEN").is_ok());
    let (mut ok, mut refused, mut builds, mut units_n) = (0, 0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        let f: Box<dyn Filter> = if c["dialect"] == "wiki" { Box::new(MediaWiki::with_template_params(c["allow"].as_str().unwrap())) } else { Box::new(DokuWiki) };
        if c.get("error").is_some() {
            if extract(f.as_ref(), &data).is_ok() {
                bad.push(format!("{name}: LocOS refuses, Bilingua reads"));
            }
            refused += 1;
            continue;
        }
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let got = match extract(f.as_ref(), &data) {
            Ok(d) => d.units.iter().map(token_text).collect::<Vec<_>>(),
            Err(e) => {
                bad.push(format!("{name}: extract failed: {e}"));
                continue;
            }
        };
        if got != want {
            let i = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
            bad.push(format!("{name}: units differ at {i} (got {} want {}): {:?} vs {:?}", got.len(), want.len(), got.get(i), want.get(i)));
            continue;
        }
        units_n += want.len();
        let legacy = std::str::from_utf8(&data).is_err();
        for (label, d) in c["builds"].as_object().unwrap() {
            if legacy && matches!(label.as_str(), "third" | "cjk") {
                continue; // Japanese text cannot be written in Windows-1252: LocOS switches to UTF-8, Bilingua keeps the file's encoding
            }
            let m: Vec<(usize, String)> = want.iter().enumerate().filter_map(|(i, t)| map_text(label, i, t).map(|m| (i, m))).collect();
            let m2: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let mut out = build(f.as_ref(), &data, &m2);
            if legacy && out != data {
                // known difference: LocOS writes UTF-8 for a file that was not UTF-8; Bilingua keeps the encoding
                out = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&out).0.into_owned().into_bytes();
            }
            if fnv(&out) != d.as_str().unwrap() {
                bad.push(format!("{name}: build {label}: {}", String::from_utf8_lossy(&out).chars().take(0).collect::<String>()));
            }
            builds += 1;
        }
        ok += 1;
    }
    println!("golden: {ok} files identical to LocOS ({units_n} units, {builds} builds), {refused} refused by both");
    assert!(bad.is_empty(), "{} differences, first ones:\n{}", bad.len(), bad.iter().take(15).cloned().collect::<Vec<_>>().join("\n"));
}
