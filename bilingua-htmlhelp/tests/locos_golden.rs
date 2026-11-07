//! Differential test against the LocOS HTML Help filters: 580 files (random hhc, hhk and hhp in several encodings and
//! line-break styles, randomly broken, the corpus, hostile bytes) with the units and the build digests LocOS produced
//! recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_htmlhelp::*;
use bilingua_model::*;
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

fn map_text(name: &str, i: usize, t: &str) -> Option<String> {
    Some(match name {
        "all" => format!("FR {t}"),
        "third" => return i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}\u{672c}")),
        "plain" => format!("Plain{i}"),
        "echo" => t.to_string(),
        "cjk" => format!("\u{65e5}\u{672c}\u{8a9e}{i}"),
        "ru" => format!("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} {t}"),
        "sym" => format!("A & \"B\" <c> 'd' {t}"),
        "nl" => format!("{t}\nline2\r\n"),
        "typo" => format!("Aide\u{202f}: {t}\u{2011}\u{200b}"),
        "pad" => format!("  {t}  "),
        _ => unreachable!("{name}"),
    })
}

fn filter(fmt: &str) -> Box<dyn Filter> {
    match fmt {
        "hhc" => Box::new(HhcFilter),
        "hhk" => Box::new(HhkFilter),
        _ => Box::new(HhpFilter),
    }
}

fn run(f: &dyn Filter, data: &[u8], map: &[(usize, String)], target: Option<&str>) -> Result<Vec<u8>, Error> {
    let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
    merge_with(f, data, &m, target, MergeOptions::default())
}

fn check(bad: &mut Vec<String>, name: &str, what: &str, want: &str, got: Result<Vec<u8>, Error>) {
    match (want.strip_prefix("ERR:"), got) {
        (Some(_), Err(_)) => {}
        (Some(e), Ok(_)) => bad.push(format!("{name}: {what}: LocOS refuses ({e}), we accept")),
        (None, Err(e)) => bad.push(format!("{name}: {what}: we refuse ({e}), LocOS accepts")),
        (None, Ok(b)) => {
            if fnv(&b) != want {
                if let Ok(dir) = std::env::var("HH_DUMP") {
                    let safe: String = what.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                    let _ = std::fs::write(format!("{dir}/{name}__{safe}.ours"), &b);
                }
                bad.push(format!("{name}: {what}: bytes differ"));
            }
        }
    }
}

#[test]
fn matches_locos_on_random_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 580);
    let (mut refused, mut builds, mut nunits) = (0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        let f = filter(c["fmt"].as_str().unwrap());
        if c.get("error").is_some() {
            // Allowlisted difference (MIGRATION.md "Differences from LocOS: empty HTML Help files"): LocOS refuses a
            // valid file with nothing to translate, Bilingua opens it with zero units and a warning. Any other file
            // LocOS refuses must still be refused.
            if let Ok(d) = extract(f.as_ref(), &data) {
                if !(d.units.is_empty() && d.warnings.iter().any(|w| w.kind == "no_translatable_text")) {
                    bad.push(format!("{name}: LocOS refuses, we accept"));
                }
            }
            refused += 1;
            continue;
        }
        let doc = match extract(f.as_ref(), &data) {
            Ok(d) => d,
            Err(e) => {
                bad.push(format!("{name}: we refuse ({e}), LocOS accepts"));
                continue;
            }
        };
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let got: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
        if got != want {
            bad.push(format!("{name}: units\n  want {want:?}\n  got  {got:?}"));
            continue;
        }
        nunits += want.len();
        let mapping = |label: &str| -> Vec<(usize, String)> { want.iter().enumerate().filter_map(|(i, t)| map_text(label, i, t).map(|m| (i, m))).collect() };
        for (label, d) in c["builds"].as_object().unwrap() {
            check(&mut bad, name, &format!("build {label}"), d.as_str().unwrap(), run(f.as_ref(), &data, &mapping(label), None));
            builds += 1;
        }
        for (lc, d) in c["localized"].as_object().unwrap() {
            if lc.ends_with("|map") {
                continue;
            }
            let map: Vec<(usize, String)> = if let Some(base) = lc.strip_suffix("#2") {
                (0..want.len())
                    .map(|i| {
                        (
                            i,
                            match base {
                                "ja-JP" => format!("\u{65e5}\u{672c}\u{8a9e}{i}"),
                                "vi-VN" => format!("Vi\u{1ec7}t Nam {i}"),
                                "fr-FR" => format!("Salut \u{e9}\u{e8} {i}"),
                                _ => format!("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} {i}"),
                            },
                        )
                    })
                    .collect()
            } else {
                mapping(c["localized"][format!("{lc}|map")].as_str().unwrap())
            };
            // Nothing translated: Bilingua returns the original bytes (no language stamp), LocOS stamps.
            if map.is_empty() {
                continue;
            }
            let target = lc.strip_suffix("#2").unwrap_or(lc);
            check(&mut bad, name, &format!("localized {lc}"), d.as_str().unwrap(), run(f.as_ref(), &data, &map, Some(target)));
            builds += 1;
        }
    }
    println!("{} cases, {refused} refused, {nunits} units, {builds} builds, {} differences", cases.len(), bad.len());
    assert!(bad.is_empty(), "{}\n{}", bad.len(), bad.iter().take(if std::env::var("HH_ALL").is_ok() { usize::MAX } else { 25 }).cloned().collect::<Vec<_>>().join("\n"));
}
