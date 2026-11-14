//! Differential test against the LocOS filters: 2,650 generated SBV / SubViewer / MicroDVD files (CRLF, CR, BOM,
//! UTF-16, cp1252, mutated, ambiguous), with the unit texts, inline maps and build digests LocOS produced in
//! `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
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
    let strip = |t: &str| {
        let mut out = String::new();
        let mut rest = t;
        while let Some(p) = rest.find('\u{27E6}') {
            out.push_str(&rest[..p]);
            match rest[p..].find('\u{27E7}') {
                Some(q) => rest = &rest[p + q + '\u{27E7}'.len_utf8()..],
                None => {
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    };
    match name {
        "echo" => Some(t.to_string()),
        "mark" => Some(format!("~{t}!")),
        "all" => Some(format!("T{i} \u{e9} x\n\u{65e5}\u{672c}\u{8a9e}")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "cjk" => Some(format!("\u{65e5}\u{672c}{}\u{3067}\u{3059}", t.replace(' ', ""))),
        "long" => Some(format!("{t} {t}")),
        "short" => i.is_multiple_of(2).then(|| "Kurz".to_string()),
        "rev" => Some(t.split('\n').rev().collect::<Vec<_>>().join("\n")),
        "notok" => Some(format!("{}!", strip(t))),
        "nl" => Some(t.replace(' ', "\n")),
        "ws" => Some(format!("  {t} \n")),
        // a token that names no code of the unit: LocOS keeps it as text (issue 814)
        "unk" => Some(format!("{t} \u{27E6}x99\u{27E7}")),
        _ => unreachable!(),
    }
}

#[test]
fn matches_locos_on_generated_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 2600);
    let (mut files, mut builds, mut refused, mut units_n, mut codes_n) = (0, 0, 0, 0, 0);
    let (mut bom_bad, mut both_refuse, mut hardened) = (0, 0, 0);
    let mut diffs: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let fmt = c["fmt"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        // a BOM on bytes that are not UTF-8: LocOS decodes them as UTF-8 with replacement characters, the model's
        // reader falls back to the code page (a model decision, see the report)
        if data.starts_with(&[0xEF, 0xBB, 0xBF]) && std::str::from_utf8(&data[3..]).is_err() {
            bom_bad += 1;
            continue;
        }
        if let Some(e) = c.get("error") {
            if extract(fmt, &data).is_ok() {
                diffs.push(format!("{name}: LocOS refuses ({e}), Rust reads it"));
            }
            refused += 1;
            continue;
        }
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let doc = match extract(fmt, &data) {
            Ok(d) => d,
            Err(e) => {
                diffs.push(format!("{name}: Rust refuses ({e})"));
                continue;
            }
        };
        let got = units(fmt, &data);
        if got != want {
            diffs.push(format!("{name}: units\n  rust  {got:?}\n  locos {want:?}"));
            continue;
        }
        // the codes behind the tokens
        for (u, m) in doc.units.iter().zip(c["maps"].as_array().unwrap()) {
            for (k, v) in m.as_object().unwrap() {
                if k.starts_with('/') {
                    continue;
                }
                let v = v.as_str().unwrap();
                let ok = match k.strip_prefix('g') {
                    Some(n) => u.codes.get(n).and_then(|c| c.open.as_deref()) == Some(v),
                    None => u.codes.get(k.as_str()).and_then(|c| c.standalone.as_deref()) == Some(v),
                };
                if !ok {
                    diffs.push(format!("{name}: code {k} should be {v:?}"));
                }
                codes_n += 1;
            }
        }
        assert_eq!(filter(fmt).merge(&doc).unwrap(), data, "{name}: untouched merge");
        for (label, d) in c["builds"].as_object().unwrap() {
            let tr: Vec<(usize, String)> = want.iter().enumerate().filter_map(|(i, t)| map_text(label, i, t).map(|m| (i, m))).collect();
            match (d.as_str(), try_build(fmt, &data, &tr)) {
                (Some(dig), Ok(out)) => {
                    if fnv(&out) != dig {
                        diffs.push(format!("{name}: build {label}\n  rust  {:?}", String::from_utf8_lossy(&out)));
                    }
                }
                // the one kept difference: a MicroDVD control code typed (or made by joining pieces) that LocOS still
                // writes and Bilingua refuses
                (Some(_), Err(e)) if e.to_string().contains("reads as a MicroDVD control code") => hardened += 1,
                // LocOS refuses these too since issue 805; the message differs, the refusal is the same
                (None, Err(_)) => both_refuse += 1,
                (want, Err(e)) => diffs.push(format!("{name}: build {label}: LocOS {want:?}, Rust refuses: {e}")),
                (None, Ok(_)) => diffs.push(format!("{name}: build {label}: LocOS refuses")),
            }
            builds += 1;
        }
        files += 1;
        units_n += want.len();
    }
    for d in diffs.iter().take(25) {
        eprintln!("{d}");
    }
    assert!(diffs.is_empty(), "{} differences from LocOS", diffs.len());
    println!("golden: {bom_bad} BOM-with-bad-UTF-8 files skipped, {both_refuse} builds refused by both, {hardened} builds refused on purpose (text that reads as a MicroDVD control code); {files} files ({units_n} units, {codes_n} codes), {builds} builds identical to LocOS, {refused} identical refusals");
}
