//! Differential test against the LocOS email filter: generated messages in 11 languages and their legacy
//! charsets, with the units and the bytes LocOS builds recorded in `data/locos_golden.json` (made by
//! `tools/gen_golden.py`). Units must be equal; builds must be byte for byte what LocOS writes.

mod common;
use bilingua_model::Filter;
use common::*;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// What LocOS wrote, with its two invalid charset labels corrected: Python's `Header` names the codec, not the
/// charset, for gb2312 (`eucgb2312_cn`) and big5 (`big5-tw`), and no mail reader knows those names.
fn want_bytes(h: &str) -> Vec<u8> {
    let mut v = hex(h);
    for (from, to) in [(&b"=?eucgb2312_cn?"[..], &b"=?gb2312?"[..]), (&b"=?big5-tw?"[..], &b"=?big5?"[..])] {
        let mut out = Vec::with_capacity(v.len());
        let mut i = 0;
        while i < v.len() {
            if v[i..].starts_with(from) {
                out.extend_from_slice(to);
                i += from.len();
            } else {
                out.push(v[i]);
                i += 1;
            }
        }
        v = out;
    }
    v
}

fn has_bad_label(h: &str) -> bool {
    let v = hex(h);
    [&b"=?eucgb2312_cn?"[..], &b"=?big5-tw?"[..]].iter().any(|n| v.windows(n.len()).any(|w| w == *n))
}

fn mapping(label: &str, units: &[String]) -> Vec<(usize, String)> {
    let tok = |t: &str| t.contains('\u{27e6}');
    let mut m = Vec::new();
    for (i, t) in units.iter().enumerate() {
        match label {
            "all" => m.push((i, format!("\u{ab}{t}\u{bb}"))),
            "third" if i % 3 == 0 => m.push((i, format!("Zeta {i} \u{65e5}\u{672c} {t}"))),
            "plain" => m.push((i, if tok(t) { t.clone() } else { format!("Plain{i}") })),
            "long" => m.push((i, if tok(t) { t.clone() } else { [t.as_str(); 6].join(" ") })),
            "same" => m.push((i, t.clone())),
            _ => {}
        }
    }
    m
}

fn build_map(data: &[u8], m: &[(usize, String)], locale: Option<&str>) -> Result<Vec<u8>, String> {
    let mut d = try_ex_to(data, locale).map_err(|e| e.to_string())?;
    for (i, t) in m {
        let u = &d.units[*i];
        let (_, map) = bilingua_model::profile::tokens::to_locos_text(&u.source());
        let inl = bilingua_model::profile::tokens::from_locos_text(t, &map);
        let seg = u.segments[0].id.clone();
        d.set_target(&seg, inl).map_err(|e| e.to_string())?;
    }
    bilingua_email::Email.merge(&d).map_err(|e| e.to_string())
}

fn try_ex_to(data: &[u8], locale: Option<&str>) -> Result<bilingua_model::Document, bilingua_model::Error> {
    use bilingua_model::Filter;
    bilingua_email::Email.extract(data, &opts(locale))
}

fn golden() -> Vec<serde_json::Value> {
    let path = std::env::var("EMAIL_GOLDEN").unwrap_or_else(|_| format!("{}/tests/data/locos_golden.json", env!("CARGO_MANIFEST_DIR")));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn matches_locos_on_generated_messages() {
    let cases = golden();
    assert!(cases.len() >= 100);
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
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let got = texts(&data);
        if got != want {
            bad.push(format!("{name}: units {got:?} vs {want:?}"));
            continue;
        }
        parsed += 1;
        assert_eq!(merged(&ex(&data)), data, "{name}: untouched merge");
        let mut check = |what: String, locale: Option<&str>, label: &str, want_hex: &serde_json::Value| {
            let m = mapping(label, &want);
            let got = build_map(&data, &m, locale);
            builds += 1;
            match (got, want_hex) {
                (Ok(g), serde_json::Value::String(w)) => {
                    if has_bad_label(w) {
                        // the label is LocOS's bug and moves the line breaks of a long Subject
                        return;
                    }
                    // since D4 the HTML parts are LocOS's byte for byte (`'` in an attribute is `&#x27;`, as LocOS writes it)
                    if g != want_bytes(w) {
                        if let Ok(dir) = std::env::var("EMAIL_GOLDEN_DUMP") {
                            std::fs::write(format!("{dir}/{name}.{}.got.eml", what.replace(':', "_")), &g).unwrap();
                            std::fs::write(format!("{dir}/{name}.{}.want.eml", what.replace(':', "_")), want_bytes(w)).unwrap();
                            std::fs::write(format!("{dir}/{name}.in.eml"), &data).unwrap();
                        }
                        bad.push(format!("{name}: {what}: bytes differ"));
                    }
                }
                (Err(_), serde_json::Value::Object(_)) => {}
                (g, w) => bad.push(format!("{name}: {what}: got {:?} want {}", g.map(|v| v.len()), w.to_string().chars().take(80).collect::<String>())),
            }
        };
        for (label, h) in c["builds"].as_object().unwrap() {
            check(format!("build {label}"), None, label, h);
        }
        for (key, h) in c["localized"].as_object().unwrap() {
            let (label, loc) = key.split_once(':').unwrap();
            check(format!("localized {key}"), Some(loc), label, h);
        }
    }
    println!("golden: {parsed} messages parsed like LocOS, {refused} refused like LocOS, {builds} builds compared");
    assert!(bad.is_empty(), "{} differences, first ones:\n{}", bad.len(), bad.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}
