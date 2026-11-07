//! Differential test against the LocOS DXF filter: random, mutated and truncated drawings plus the corpus, with the
//! units and the bytes LocOS builds recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`; the four
//! mappings below are written there too).
mod common;
use bilingua_dxf::Dxf;
use bilingua_model::*;
use common::*;

fn maps(tokens: &[String]) -> Vec<(&'static str, Vec<(usize, String)>)> {
    let mut all = vec![];
    let mut third = vec![];
    let mut plain = vec![];
    let mut longj = vec![];
    for (i, t) in tokens.iter().enumerate() {
        all.push((i, format!("[T{i}] {t} \u{e9}\u{65e5}\nx{i}\t^%%{{}}\\")));
        if i % 3 == 0 {
            third.push((i, format!("Zeta {i} \u{65e5}\u{672c}\u{8a9e} {}", "x".repeat(if i % 2 == 0 { 300 } else { 5 }))));
        }
        plain.push((i, format!("Plain{i}")));
        if i % 5 == 0 {
            longj.push((i, format!("{}{t}", "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}".repeat(40))));
        }
    }
    vec![("all", all), ("third", third), ("plain", plain), ("longj", longj)]
}

fn try_build(data: &[u8], mapping: &[(usize, String)]) -> Result<Vec<u8>, Error> {
    let mut doc = extract(data)?;
    for (i, t) in mapping {
        let u = &doc.units[*i];
        let id = u.segments[0].id.clone();
        let tg = target_of(u, t);
        doc.set_target(&id, tg)?;
    }
    Dxf.merge(&doc)
}

/// Cases where Bilingua now differs from the recorded LocOS output on purpose (issues 805 and 811): a code before white
/// space is closed (`trail\\ ` instead of `trail\ `), and DOS864 reads byte 0x25 as `%`. LocOS gets the same fixes next;
/// then these goldens are regenerated and this list goes away. The test fails if a listed case stops differing.
const FIXED_ON_PURPOSE: &[&str] = &["random-12", "random-17", "random-45", "random-52", "random-69", "random-70", "random-82", "random-88", "random-93", "random-97", "random-104", "mutated-14", "mutated-47", "escapes.dxf"];

#[test]
fn matches_locos_on_random_files_and_corpus() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 250);
    let (mut ok, mut refused, mut builds) = (0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            if extract(&data).is_ok() {
                bad.push(format!("{name}: LocOS refuses ({}), Bilingua accepts", c["error"]));
            }
            refused += 1;
            continue;
        }
        let doc = match extract(&data) {
            Ok(d) => d,
            Err(e) => {
                bad.push(format!("{name}: LocOS accepts, Bilingua refuses: {e}"));
                continue;
            }
        };
        let want: Vec<(String, String)> =
            c["units"].as_array().unwrap().iter().map(|u| (u[0].as_str().unwrap().to_string(), u[1].as_str().unwrap().to_string())).collect();
        let got: Vec<(String, String)> = doc.units.iter().map(|u| (u.context.style.clone().unwrap(), token_text(u))).collect();
        if got != want {
            bad.push(format!("{name}: units differ: LocOS {} units, Bilingua {}", want.len(), got.len()));
            continue;
        }
        let tokens: Vec<String> = want.iter().map(|w| w.1.clone()).collect();
        for (label, mapping) in maps(&tokens) {
            let expect = c["builds"][label].as_str().unwrap();
            if expect.starts_with("error:") {
                continue;
            }
            match try_build(&data, &mapping) {
                Ok(b) if b == hex(expect) => builds += 1,
                Ok(b) => {
                    let w = hex(expect);
                    let k = b.iter().zip(&w).position(|(x, y)| x != y).unwrap_or(b.len().min(w.len()));
                    let ctx = |v: &[u8]| String::from_utf8_lossy(&v[k.saturating_sub(30)..(k + 30).min(v.len())]).into_owned();
                    bad.push(format!("{name}: build {label} differs at byte {k}: ours {:?} LocOS {:?}", ctx(&b), ctx(&w)));
                }
                Err(e) => bad.push(format!("{name}: build {label} fails: {e}")),
            }
        }
        ok += 1;
    }
    for known in FIXED_ON_PURPOSE {
        assert!(bad.iter().any(|b| b.starts_with(&format!("{known}:"))), "{known} no longer differs: remove it from FIXED_ON_PURPOSE");
    }
    bad.retain(|b| !FIXED_ON_PURPOSE.iter().any(|k| b.starts_with(&format!("{k}:"))));
    println!("golden: {ok} files identical to LocOS ({builds} builds), {refused} refused by LocOS");
    assert!(bad.is_empty(), "{} differences, first 25:\n{}", bad.len(), bad.iter().take(25).cloned().collect::<Vec<_>>().join("\n"));
}
