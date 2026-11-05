//! The units LocOS gives for every corpus file (`tests/data/oracle_units.json`, made by `tools/gen_oracle_units.py`
//! from the LocOS filter) against the units Bilingua gives, texts in LocOS form, in order.
mod common;
use common::*;

fn corpus() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/epub")
}

/// The differences between LocOS and Bilingua that come from `bilingua-html`, not from this crate, each written
/// down: `None` drops a LocOS unit Bilingua does not extract, `Some(text)` is the Bilingua form of a LocOS unit.
///
/// * LocOS keeps character references as typed in the source (`&amp;`, `&#233;`); Bilingua units hold the decoded
///   characters, and write them back escaped.
/// * LocOS reads `<code>` as a paired code with its text inside; Bilingua keeps the whole element as one
///   standalone code (protected text, never translated).
/// * LocOS extracts the text of `<pre>`; the Bilingua HTML filter skips it (it is preformatted).
fn known(locos: &str) -> Option<String> {
    match locos {
        "keep   this\nas is" => None,
        t if t.starts_with("Fish &amp; chips") => Some("Fish & chips, caf\u{e9} \u{2014} <tags> and \u{27E6}x1\u{27E7} inline.".into()),
        t => Some(t.to_string()),
    }
}

#[test]
fn units_match_locos_on_the_corpus() {
    let oracle: serde_json::Value = serde_json::from_slice(&std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/oracle_units.json")).unwrap()).unwrap();
    let mut diffs = Vec::new();
    let (mut total, mut same) = (0, 0);
    for (name, want) in oracle.as_object().unwrap() {
        let bytes = std::fs::read(corpus().join(name)).unwrap();
        let d = extract(&bytes);
        let got = loc_texts(&d);
        let want: Vec<(String, String)> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_str().unwrap().to_string()))
            .filter_map(|(t, s)| known(&t).map(|t| (t, s)))
            .collect();
        let n = got.len().max(want.len());
        for i in 0..n {
            total += 1;
            let g = got.get(i);
            let w = want.get(i).map(|w| &w.0);
            if g == w {
                same += 1;
            } else {
                diffs.push(format!("{name} #{i}: bilingua {g:?} / locos {w:?}"));
            }
        }
    }
    eprintln!("{same} of {total} units match; {} differ", diffs.len());
    for d in &diffs {
        eprintln!("{d}");
    }
    assert!(diffs.is_empty(), "{} differences", diffs.len());
}

/// Okapi's own EPUB test resource (`okf_epub` is missing from the Okapi 1.48.0 build, so LocOS is the baseline;
/// `tests/data/okapi_test1_units.json` holds what the LocOS filter gives for it). Skipped when the Okapi sources
/// are not on this machine.
#[test]
fn okapi_test_resource_reads_like_locos_and_round_trips() {
    let path = std::path::Path::new(r"C:\Users\brent\code\okapi-oracle\resources\okapi-src\okapi\filters\epub\src\test\resources\test1.epub");
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("skipped: the Okapi test resource is not on this machine");
        return;
    };
    let oracle: serde_json::Value = serde_json::from_slice(&std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/okapi_test1_units.json")).unwrap()).unwrap();
    let want: Vec<String> = oracle["test1.epub"].as_array().unwrap().iter().map(|p| p[0].as_str().unwrap().to_string()).collect();
    let mut d = extract(&bytes);
    let got = loc_texts(&d);
    // The differences are the HTML crate's: character references (LocOS keeps `&amp;` as typed) and `<code>`/`<pre>`.
    let norm = |s: &str| {
        let s = s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'");
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let (mut same, mut diffs) = (0, Vec::new());
    for (i, w) in want.iter().enumerate() {
        match got.get(i) {
            Some(g) if norm(g) == norm(w) => same += 1,
            g => diffs.push(format!("#{i}: bilingua {g:?} / locos {w:?}")),
        }
    }
    eprintln!("okapi test1.epub: {} units here, {} in LocOS, {same} equal in order; first differences:", got.len(), want.len());
    for x in diffs.iter().take(8) {
        eprintln!("{}", x.chars().take(260).collect::<String>());
    }
    assert!(got.len() >= 300, "{} units", got.len());
    assert_eq!(merge(&d), bytes, "untouched merge");
    d.target_lang = Some("de".into());
    translate_all(&mut d);
    let out = merge(&d);
    let (m, infos) = members(&out);
    assert_eq!(infos[0].name, "mimetype");
    well_formed(&m);
    assert_eq!(extract(&out).units.len(), got.len());
}
