//! Track D: the LocOS-profile goldens. Each `tests/locos/<format>/<file>` has a `<file>.golden.json` written by
//! `oracle/locos_golden.py` with the Localization OS venv: the units LocOS's filter gives (p_index, text with its
//! tokens, style) and its builds under a few mappings. This test does what the LocOS adapter does: extract under
//! the `locos` profile at paragraph granularity, number the translatable segments (`locos.p_index` when every unit
//! has a distinct one, else the position), turn each segment into LocOS text with `profile::tokens`, then set each
//! mapping as targets and merge with the profile's merge options. Units and bytes must be equal.
use bilingua::*;
use bilingua::profile::tokens;
use serde_json::Value;
use std::path::{Path, PathBuf};

struct Numbered {
    p: i64,
    seg: String,
    text: String,
    map: tokens::LocosMap,
    style: String,
}

fn opts(locale: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_granularity(Granularity::Paragraph).with_target_lang(locale.map(|l| l.replace('_', "-")))
}

fn numbered(doc: &Document) -> Vec<Numbered> {
    let segs: Vec<(&Unit, &Segment)> = doc.units.iter().filter(|u| u.translate).flat_map(|u| u.segments.iter().map(move |s| (u, s))).collect();
    let metas: Vec<Option<i64>> = segs.iter().map(|(u, _)| u.context.extra.get(profile::P_INDEX).and_then(|p| p.parse().ok())).collect();
    let distinct = metas.iter().flatten().collect::<std::collections::BTreeSet<_>>().len() == metas.len();
    let use_meta = !segs.is_empty() && metas.iter().all(Option::is_some) && distinct;
    segs.iter()
        .enumerate()
        .map(|(i, (u, s))| {
            let (text, map) = tokens::to_locos_text(&s.source);
            let style = u
                .context
                .extra
                .get(profile::STYLE)
                .filter(|s| !s.is_empty())
                .cloned()
                .or_else(|| u.context.style.clone())
                .unwrap_or_default();
            Numbered { p: if use_meta { metas[i].unwrap() } else { i as i64 }, seg: s.id.clone(), text, map, style }
        })
        .collect()
}

/// The goldens' root: `tests/locos`, or `BILI_LOCOS_ROOT` (a corpus from `oracle/locos_fuzz.py`).
fn root() -> PathBuf {
    match std::env::var_os("BILI_LOCOS_ROOT") {
        Some(r) if !r.is_empty() => PathBuf::from(r),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("locos"),
    }
}

fn goldens() -> Vec<PathBuf> {
    let root = root();
    let mut out = Vec::new();
    for fmt in std::fs::read_dir(&root).unwrap() {
        let fmt = fmt.unwrap().path();
        if !fmt.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&fmt).unwrap() {
            let f = f.unwrap().path();
            if f.to_string_lossy().ends_with(".golden.json") {
                out.push(f);
            }
        }
    }
    out.sort();
    out
}

/// Checks one golden; returns the failures as text (empty when it matches).
fn check(golden: &Path) -> (usize, usize, usize, Vec<String>) {
    let g: Value = serde_json::from_str(&std::fs::read_to_string(golden).unwrap()).unwrap();
    let input = PathBuf::from(golden.to_string_lossy().trim_end_matches(".golden.json"));
    let name = input.file_name().unwrap().to_string_lossy().to_string();
    let data = std::fs::read(&input).unwrap();
    let mut fails = Vec::new();
    // LocOS refused the file (fuzz corpus): Bilingua must refuse it too, or say so
    if let Some(e) = g["parse_error"].as_str() {
        return match extract(&name, &data, &opts(None)) {
            Err(_) => (1, 0, 0, Vec::new()),
            Ok(_) => (0, 0, 0, vec![format!("{name}: LocOS refuses ({e}), Bilingua extracts")]),
        };
    }
    let doc = match extract(&name, &data, &opts(None)) {
        Ok(d) => d,
        Err(e) => return (0, 0, 0, vec![format!("{name}: extract failed: {e}")]),
    };
    let have: Vec<(i64, String, String)> = numbered(&doc).into_iter().map(|n| (n.p, n.text, n.style)).collect();
    let want: Vec<(i64, String, String)> = g["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["p_index"].as_i64().unwrap(), u["text"].as_str().unwrap().to_string(), u["style"].as_str().unwrap().to_string()))
        .collect();
    let units_ok = have == want;
    if !units_ok {
        fails.push(format!("{name}: units differ\n  locos:    {want:?}\n  bilingua: {have:?}"));
    }
    let (mut builds, mut built) = (0, 0);
    for b in g["builds"].as_array().unwrap() {
        let Some(out) = b["out_b64"].as_str() else { continue };
        builds += 1;
        let want = b64::decode(out).unwrap();
        let locale = b["locale"].as_str();
        let label = format!("{name} build {}", b["name"].as_str().unwrap());
        let map = b["map"].as_object().unwrap();
        // LocOS's B-bytes guard (base.py `_byte_guard`) wraps every filter class, the adapter's included: a plain
        // build that writes nothing returns the upload without calling the parser
        if locale.is_none() && map.is_empty() {
            if want == data {
                built += 1;
            } else {
                fails.push(format!("{label}: LocOS changed an untouched file"));
            }
            continue;
        }
        let mut doc = match extract(&name, &data, &opts(locale)) {
            Ok(d) => d,
            Err(e) => {
                fails.push(format!("{label}: extract failed: {e}"));
                continue;
            }
        };
        let mut set_fail = None;
        for n in numbered(&doc) {
            if let Some(t) = map.get(&n.p.to_string()).and_then(Value::as_str) {
                let inl = tokens::from_locos_text(t, &n.map);
                if let Err(e) = profile::locos_set_target(&mut doc, &n.seg, inl) {
                    set_fail = Some(format!("{label}: set_target {}: {e}", n.p));
                }
            }
        }
        if let Some(f) = set_fail {
            fails.push(f);
            continue;
        }
        match merge_with(&doc, &profile::locos().merge_options(&doc.format)) {
            Ok(have) if have == want => built += 1,
            Ok(have) => fails.push(format!(
                "{label}: bytes differ\n  locos:    {:?}\n  bilingua: {:?}",
                String::from_utf8_lossy(&want),
                String::from_utf8_lossy(&have)
            )),
            Err(e) => fails.push(format!("{label}: merge failed: {e}")),
        }
    }
    (units_ok as usize, builds, built, fails)
}

/// Goldens that still differ, with the reason; their failures are printed, not fatal. Remove a line when its
/// file matches.
const KNOWN: &[(&str, &str)] = &[
    ("json_dialects_filter_chrome.json", "LocOS JSON resource dialects (Chrome messages.json): not in group A"),
    ("json_dialects_filter_formatjs.json", "LocOS JSON resource dialects (FormatJS): not in group A"),
    ("json_dialects_filter_go.json", "LocOS JSON resource dialects (go-i18n): not in group A"),
    ("json_dialects_filter_go_v1.json", "LocOS JSON resource dialects (go-i18n v1): not in group A"),
    ("json_dialects_filter_structured.json", "LocOS JSON resource dialects (Structured JSON): not in group A"),
];

#[test]
fn locos_profile_goldens() {
    let mut all: Vec<String> = Vec::new();
    let mut summary = std::collections::BTreeMap::<String, [usize; 4]>::new();
    for g in goldens() {
        let fmt = g.parent().unwrap().file_name().unwrap().to_string_lossy().to_string();
        let (u, b, ok, f) = check(&g);
        let file = g.file_name().unwrap().to_string_lossy().trim_end_matches(".golden.json").to_string();
        if let Some((_, why)) = KNOWN.iter().find(|(k, _)| *k == file) {
            println!("LOCOS-GOLDEN known difference {file} ({why}): units {u}/1 builds {ok}/{b}");
            for x in &f {
                println!("  {x}");
            }
            continue;
        }
        let s = summary.entry(fmt).or_default();
        s[0] += u;
        s[1] += 1;
        s[2] += ok;
        s[3] += b;
        all.extend(f);
    }
    for (fmt, s) in &summary {
        println!("LOCOS-GOLDEN {fmt}: units {}/{} builds {}/{}", s[0], s[1], s[2], s[3]);
    }
    assert!(!summary.is_empty(), "no goldens found");
    assert!(all.is_empty(), "{} LocOS golden failures:\n{}", all.len(), all.join("\n"));
}
