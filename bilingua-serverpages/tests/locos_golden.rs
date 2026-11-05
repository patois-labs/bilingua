//! Differential test against the LocOS server-page filters: the suite fixtures plus 750 random pages (150 per
//! dialect), with the units, the inline markup behind every token, the bytes LocOS builds with four mappings and
//! the localized builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::*;
use common::*;

fn mapping(units: &[Unit], label: &str, want: &[serde_json::Value], bom: bool) -> Vec<(usize, String)> {
    let n = units.len();
    match label {
        "all" => (0..n).map(|i| (i, format!("T{i} \"q\" \u{e9} <b> & &amp; ${{x}} #{{y}} \\"))).collect(),
        "third" => (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5} 'it's'"))).collect(),
        "tok" => want
            .iter()
            .enumerate()
            .map(|(i, u)| {
                let t = u["text"].as_str().unwrap();
                (i, format!("{} Z", if i == 0 && bom { t.trim_start_matches('\u{feff}') } else { t }))
            })
            .collect(),
        "swap" => {
            let mut out = Vec::new();
            for (i, u) in want.iter().enumerate() {
                let t = u["text"].as_str().unwrap();
                let mut toks: Vec<&str> = Vec::new();
                let mut rest = t;
                while let Some(a) = rest.find('\u{27E6}') {
                    let Some(b) = rest[a..].find('\u{27E7}') else { break };
                    let tk = &rest[a..a + b + '\u{27E7}'.len_utf8()];
                    toks.push(tk);
                    rest = &rest[a + b + '\u{27E7}'.len_utf8()..];
                }
                if !toks.is_empty() {
                    toks.reverse();
                    out.push((i, format!("{} Q", toks.join(" "))));
                }
            }
            out
        }
        _ => unreachable!(),
    }
}

/// LocOS's server-page build writes UTF-8 with no envelope: a byte order mark survives only as a text character
/// of the first token (lost when the first unit is translated). Bilingua keeps the mark (B6), so compare without.
///
/// One more deliberate difference: a JSP translation that ends in a backslash right before a `${` island gets the
/// backslash written as `&#92;` (so it cannot turn the expression into literal text); LocOS writes it raw. LocOS
/// output never holds `&#92;` for these pages, so the reference form is restored before comparing.
fn nobom(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_start_matches('\u{feff}').replace("&#92;", "\\")
}

fn check_units(name: &str, doc: &Document, want: &[serde_json::Value], has_bom: bool) {
    assert_eq!(doc.units.len(), want.len(), "{name}: unit count");
    for (k, (u, w)) in doc.units.iter().zip(want).enumerate() {
        // LocOS decodes a UTF-8 file with its byte order mark as a text character; Bilingua (B6) reads the mark as
        // part of the envelope, so the first unit has no U+FEFF in front.
        let want_text = w["text"].as_str().unwrap();
        let want_text = if k == 0 && has_bom { want_text.trim_start_matches('\u{feff}') } else { want_text };
        assert_eq!(locos_text(u), want_text, "{name}: unit {k} text");
        let style = w["style"].as_str().filter(|s| !s.is_empty());
        assert_eq!(u.context.style.as_deref(), style, "{name}: unit {k} style");
        let map = w["map"].as_object().unwrap();
        // every token id of the unit's text has the original markup of LocOS's inline map
        let mut seen = 0;
        for i in u.source() {
            let Inline::Tag { id, kind, .. } = i else { continue };
            let entry = &map[&id];
            let code = &u.codes[&id];
            match kind {
                TagKind::Open => {
                    seen += 1;
                    assert_eq!(Some(entry["open"].as_str().unwrap()), code.open.as_deref(), "{name}: unit {k} open {id}");
                    assert_eq!(entry["close"].as_str(), code.close.as_deref(), "{name}: unit {k} close {id}");
                }
                TagKind::Standalone => {
                    seen += 1;
                    assert_eq!(Some(entry["point"].as_str().unwrap()), code.standalone.as_deref(), "{name}: unit {k} point {id}");
                }
                TagKind::Close => {}
            }
        }
        assert_eq!(seen, map.len(), "{name}: unit {k} inline map size");
    }
}

#[test]
fn matches_locos_on_random_pages_and_fixtures() {
    let text = std::fs::read_to_string(data_dir().join("locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 700);
    let (mut ok, mut refused_n, mut builds, mut with_units) = (0, 0, 0, 0);
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let fmt = c["fmt"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if let Some(err) = c.get("error") {
            match extract(fmt, &data) {
                Ok(_) => panic!("{name}: LocOS refuses ({err}), so must we"),
                Err(Error::Parse { msg, .. }) if err == "ServerPageRefused" => {
                    assert_eq!(msg, c["message"].as_str().unwrap(), "{name}: refusal text");
                }
                Err(_) => {}
            }
            refused_n += 1;
            continue;
        }
        let bom = data.starts_with(&[0xEF, 0xBB, 0xBF]);
        // LocOS reads the mark as a text character, so white space right after it is not leading white space there
        // (the first unit's text and the bytes around it differ by that white space); not comparable
        if bom && std::str::from_utf8(&data[3..]).is_ok_and(|s| s.starts_with(|c: char| c.is_whitespace())) {
            continue;
        }
        let doc = extract(fmt, &data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = c["units"].as_array().unwrap();
        check_units(name, &doc, want, data.starts_with(&[0xEF, 0xBB, 0xBF]));
        if !want.is_empty() {
            with_units += 1;
        }
        // LocOS writes every server page as UTF-8; Bilingua writes the file's own encoding (B6), so the built bytes
        // of a legacy-encoded or UTF-16 fixture are not comparable here (tests/roundtrip.rs covers them)
        let plain = std::str::from_utf8(&data).is_ok();
        if !plain {
            ok += 1;
            continue;
        }
        for label in ["all", "third", "tok", "swap"] {
            let m = mapping(&doc.units, label, want, bom);
            let mp: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = build_doc(fmt, &data, &mp, None);
            match &c["builds"][label] {
                serde_json::Value::String(h) => {
                    let exp = hex(h);
                    let got = got.unwrap_or_else(|e| panic!("{name}: build {label}: {e}"));
                    assert_eq!(nobom(&got), nobom(&exp), "{name}: build {label}");
                    builds += 1;
                }
                err => assert!(got.is_err(), "{name}: LocOS refuses build {label} ({err}), so must we"),
            }
        }
        for loc in ["fr-FR", "ar"] {
            let m = mapping(&doc.units, "tok", want, bom);
            let mp: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            if mp.is_empty() {
                continue;
            }
            let got = build_doc(fmt, &data, &mp, Some(loc));
            match &c["loc"][loc] {
                serde_json::Value::String(h) => {
                    let exp = hex(h);
                    let got = got.unwrap_or_else(|e| panic!("{name}: localized {loc}: {e}"));
                    assert_eq!(nobom(&got), nobom(&exp), "{name}: localized {loc}");
                    builds += 1;
                }
                err => assert!(got.is_err(), "{name}: LocOS refuses localized {loc} ({err}), so must we"),
            }
        }
        ok += 1;
    }
    assert!(ok >= 500, "only {ok} pages opened");
    assert!(refused_n >= 40, "only {refused_n} refusals");
    println!("{ok} pages ({with_units} with units) match LocOS, {builds} builds identical, {refused_n} refused by both");
}
