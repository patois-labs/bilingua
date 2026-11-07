//! Differential test against the LocOS mdx filter: 260 random MDX files plus the LocOS fixture, with the units, the
//! inline maps and the bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::profile::tokens::to_locos_text;
use common::*;

fn mappings(texts: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    let n = texts.len();
    vec![
        ("all", (0..n).map(|i| (i, format!("T{i} \"q\" \u{e9} {{b}} <c> `{{d}}` \\{{e"))).collect()),
        ("third", (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5}"))).collect()),
        ("tok", texts.iter().enumerate().map(|(i, t)| (i, format!("{t} Z"))).collect()),
        ("dup", texts.iter().enumerate().map(|(i, t)| (i, format!("{t} {t}"))).collect()),
        ("cjk", texts.iter().enumerate().map(|(i, t)| (i, format!("__{}__\u{30aa}\u{30d7}\u{30b7}\u{30e7}\u{30f3}\u{3002} https://x.org/p", t.replace(' ', "")))).collect()),
    ]
}

#[test]
fn matches_locos_on_random_files() {
    let path = std::env::var("BILINGUA_MDX_GOLDEN").unwrap_or_else(|_| format!("{}/tests/data/locos_golden.json", env!("CARGO_MANIFEST_DIR")));
    let text = std::fs::read_to_string(path).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 260);
    let (mut ok, mut refused) = (0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = c["text"].as_str().unwrap().as_bytes().to_vec();
        if c.get("error").is_some() {
            if c["error"] == "AttributeError" {
                // LocOS crashes (an unhandled None) on a Unicode space inside a JSX tag; Bilingua reads the space as
                // a separator. Known difference: we only require that it does not panic.
                let _ = extract(&data);
                refused += 1;
                continue;
            }
            assert!(extract(&data).is_err(), "{name}: LocOS refuses ({}), so must we", c["error"]);
            refused += 1;
            continue;
        }
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let doc = extract(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let got: Vec<String> = doc.units.iter().map(locos_text).collect();
        assert_eq!(got, want, "{name}: units");
        // the inline map of every unit
        for (i, (u, m)) in doc.units.iter().zip(c["map"].as_array().unwrap()).enumerate() {
            let map = to_locos_text(&u.source()).1;
            let m = m.as_object().unwrap();
            assert_eq!(map.len(), m.len(), "{name}: unit {i} tokens");
            for (k, v) in m {
                let code = &map[k];
                if let Some(p) = v.get("point") {
                    assert_eq!((code.open.as_deref(), code.close.as_deref()), (p.as_str(), None), "{name}: unit {i} token {k}");
                } else {
                    assert_eq!((code.open.as_deref(), code.close.as_deref()), (v["open"].as_str(), v["close"].as_str()), "{name}: unit {i} token {k}");
                }
            }
        }
        // attribute units carry the attribute style
        for (u, st) in doc.units.iter().zip(c["styles"].as_array().unwrap()) {
            assert_eq!(u.context.style.as_deref().unwrap_or(""), st.as_str().unwrap(), "{name}: style");
        }
        assert_eq!(bilingua_model::Filter::merge(&bilingua_mdx::Mdx, &doc).unwrap(), data, "{name}: untouched merge");
        for (label, m) in mappings(&want) {
            let mapping: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = String::from_utf8_lossy(&build(&data, &mapping)).into_owned();
            let exp = c["builds"][label].as_str().unwrap();
            // kept difference (issue 814): LocOS's CJK emphasis repair moves a mark into a token here (`⟦*x1⟧`) and
            // writes the broken token as text, losing its code; Bilingua drops a repair that changes the tokens
            if (name, label) == ("f-random-007", "cjk") {
                assert_ne!(got, exp, "{name}: the kept difference is gone");
                assert!(!got.contains('\u{27e6}') && extract(got.as_bytes()).is_ok(), "{name}: build {label}: {got}");
                continue;
            }
            assert_eq!(got, exp, "{name}: build {label}");
        }
        ok += 1;
    }
    assert!(ok >= 260, "only {ok} files opened");
    println!("{ok} files match LocOS, {refused} refused by both");
}
