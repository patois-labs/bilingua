//! Differential test against the LocOS rst filter: 2,400 generated reStructuredText files (inline fuzz, random
//! documents, mutated and broken ones, CRLF and BOM variants) plus the LocOS suite's own documents, with the unit
//! texts, styles, skipped tables and build digests LocOS produced in `data/locos_golden.json` (made by
//! `tools/gen_golden.py`).
mod common;
/// The profile the shared helpers extract with.
const PROFILE: Option<&str> = Some("locos");
use bilingua_model::{Error, Filter};
use bilingua_rst::Rst;
use common::*;

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &x in b {
        h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{}:{:016x}", b.len(), h)
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// The mappings of `tools/gen_golden.py`.
fn map_text(name: &str, i: usize, t: &str) -> Option<String> {
    match name {
        "echo" => Some(t.to_string()),
        "mark" => Some(format!("~{t}!")),
        "all" => Some(format!("T{i} *em* a_ b| `q` \\ \u{e9}\n\u{65e5}\u{672c}\u{8a9e}")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "cjk" => Some(format!("\u{65e5}\u{672c}{}\u{3067}\u{3059}", t.replace(' ', ""))),
        "long" => Some(format!("{t} {t}")),
        "short" => i.is_multiple_of(2).then(|| "Kurz".to_string()),
        "dash" => (i % 2 == 1).then(|| format!("- {t}")),
        "tok" => Some(format!("{t} \u{27e6}x9999\u{27e7} *a*")),
        _ => unreachable!(),
    }
}

/// Documented differences from LocOS (MIGRATION.md "Differences from LocOS", crate README): LocOS's units are
/// wrong in these cases, so they are skipped (each must still differ, or it comes off the list).
/// LITERAL_UNDER_ITEM: a literal block under a list item (`* Run it::`, blank line, indented lines). LocOS dedents
/// the item's body to column 0, so the block is no longer indented and its lines are offered as text.
const LITERAL_UNDER_ITEM: &[&str] = &["doc-8", "doc-214", "doc-539", "doc-693"];
/// LITERAL_BACKTICK: a literal that ends in a backtick (``:role:`x```) ends at the run of three backticks, as
/// docutils reads it; LocOS ends it at the first pair and offers the last backtick as text.
const LITERAL_BACKTICK: &[&str] = &["doc-829"];

/// Builds with the units' own tags, bypassing `set_target`'s tag-count check (the `long` mapping repeats tags).
#[test]
fn matches_locos_on_generated_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 2400);
    let (mut files, mut builds, mut fits, mut units_n) = (0, 0, 0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if let Some(e) = c.get("error") {
            assert!(extract(&data).is_err(), "{name}: LocOS refuses ({e}), so must we");
            continue;
        }
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        if LITERAL_UNDER_ITEM.contains(&name) || LITERAL_BACKTICK.contains(&name) {
            assert_ne!(units(&data), want, "{name}: on a difference list but now matches LocOS: take it off the list");
            continue;
        }
        assert_eq!(units(&data), want, "{name}: units");
        let wst: Vec<String> = c["styles"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        assert_eq!(styles(&data), wst, "{name}: styles");
        let doc = extract(&data).unwrap();
        let skipped: Vec<(u64, String)> = c["skipped"].as_array().unwrap().iter().map(|s| (s[0].as_u64().unwrap(), s[1].as_str().unwrap().to_string())).collect();
        let got: Vec<(u64, String)> = doc
            .warnings
            .iter()
            .filter(|w| w.kind == "rst_skipped_table")
            .map(|w| {
                let line: u64 = w.part.trim_start_matches("line ").parse().unwrap();
                let reason = skipped.iter().find(|s| s.0 == line).map(|s| s.1.clone()).unwrap_or_default();
                assert!(w.message.contains(&reason), "{name}: skipped reason");
                (line, reason)
            })
            .collect();
        assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), skipped.iter().map(|s| s.0).collect::<Vec<_>>(), "{name}: skipped tables");
        for (label, d) in c["builds"].as_object().unwrap() {
            let tr: Vec<(usize, String)> = want.iter().enumerate().filter_map(|(i, t)| map_text(label, i, t).map(|m| (i, m))).collect();
            let r = build_raw(&data, &tr);
            match (d.as_str(), r) {
                (Some(dig), Ok(out)) => assert_eq!(fnv(&out), dig, "{name}: build {label}"),
                (None, Err(Error::Unwritable { reason: m, .. })) => {
                    assert_eq!(m, d["msg"].as_str().unwrap(), "{name}: refusal {label}");
                    fits += 1;
                }
                (want, got) => panic!("{name}: build {label}: LocOS {want:?} but Rust {:?}", got.map(|v| fnv(&v))),
            }
            builds += 1;
        }
        files += 1;
        units_n += want.len();
    }
    println!("golden: {files} files ({units_n} units) identical to LocOS, {builds} builds, {fits} identical refusals");
    let _ = Rst.id();
}
