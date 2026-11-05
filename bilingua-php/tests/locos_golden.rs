//! Differential test against the LocOS php filter: 500 random language files plus the suite fixtures (and the oracle
//! corpus once it is added to the golden), with the units, the placeholders, the records and the bytes LocOS builds
//! recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::encoding_rs;
use bilingua_php::records;
use common::*;

fn maps(texts: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    vec![
        (
            "all",
            (0..texts.len())
                .map(|i| (i, format!("T{i} \"q\" \u{e9}\nline2 $x \\")))
                .collect(),
        ),
        (
            "third",
            (0..texts.len())
                .step_by(3)
                .map(|i| (i, format!("Zeta {i} \u{65e5}")))
                .collect(),
        ),
        (
            "tok",
            texts
                .iter()
                .enumerate()
                .map(|(i, t)| (i, format!("{t} Z")))
                .collect(),
        ),
    ]
}

/// LocOS writes a changed file as UTF-8 without a byte order mark and with LF in new text; Bilingua keeps the file's
/// own encoding (UTF-16, Windows-1252, BOM) and line ends (B6). Compare the text: decode, drop the BOM, unify line ends.
fn lf(b: &[u8]) -> Vec<u8> {
    let text: String = if b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) {
        let enc = if b[0] == 0xFF { encoding_rs::UTF_16LE } else { encoding_rs::UTF_16BE };
        enc.decode_without_bom_handling(&b[2..]).0.into_owned()
    } else if let Ok(t) = std::str::from_utf8(b) {
        t.to_string()
    } else {
        encoding_rs::WINDOWS_1252.decode_without_bom_handling(b).0.into_owned()
    };
    let b = text.trim_start_matches('\u{feff}').as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json"),
    )
    .unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 500);
    let (mut ok, mut refused) = (0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            assert!(
                extract(&data).is_err(),
                "{name}: LocOS refuses ({}), so must we",
                c["error"]
            );
            refused += 1;
            continue;
        }
        let want: Vec<String> = c["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u.as_str().unwrap().to_string())
            .collect();
        assert_eq!(texts(&data), want, "{name}: units");
        let doc = extract(&data).unwrap();
        let want_points = c["points"].as_array().unwrap();
        for (u, w) in doc.units.iter().filter(|u| u.translate).zip(want_points) {
            let mut ids: Vec<&String> = u
                .codes
                .keys()
                .filter(|k| k.as_bytes()[0].is_ascii_digit())
                .collect();
            ids.sort_by_key(|k| k.parse::<usize>().unwrap());
            let got: Vec<&str> = ids
                .iter()
                .map(|k| u.codes[*k].standalone.as_deref().unwrap())
                .collect();
            let w: Vec<&str> = w
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap())
                .collect();
            assert_eq!(got, w, "{name}: placeholders of {}", u.id);
        }
        for (label, m) in maps(&want) {
            let mapping: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = build(&data, &mapping);
            let exp = hex(c["builds"][label].as_str().unwrap());
            if exp == data {
                assert_eq!(got, data, "{name}: build {label} (nothing changed)");
            } else {
                assert_eq!(lf(&got), lf(&exp), "{name}: build {label}");
            }
        }
        let recs = records(&data).unwrap();
        let want_recs = c["records"].as_array().unwrap();
        assert_eq!(recs.len(), want_recs.len(), "{name}: record count");
        for (r, w) in recs.iter().zip(want_recs) {
            assert_eq!(r.key, w[0].as_str().unwrap(), "{name}: record key");
            assert_eq!(r.value, w[1].as_str().unwrap(), "{name}: record value");
        }
        ok += 1;
    }
    assert!(ok >= 350, "only {ok} files opened");
    assert!(refused >= 80, "only {refused} refusals");
    println!("{ok} files match LocOS, {refused} refused by both");
}
