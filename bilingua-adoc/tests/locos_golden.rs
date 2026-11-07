//! Differential test against the LocOS AsciiDoc filter: 600 random (and randomly broken) files in LF, CRLF and CR
//! with and without a BOM, with the units, styles, inline maps and build digests LocOS produced recorded in
//! `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_model::*;
use common::*;
use regex::Regex;

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &x in b {
        h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    format!("{}:{:016x}", b.len(), h)
}

fn tokens(t: &str) -> Vec<String> {
    Regex::new("\u{27e6}/?[a-z]?\\d+\u{27e7}").unwrap().find_iter(t).map(|m| m.as_str().to_string()).collect()
}

fn map_text(name: &str, i: usize, t: &str) -> Option<String> {
    let toks = tokens(t);
    match name {
        "all" => Some(format!("FR {t}")),
        "third" => i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}")),
        "plain" => Some(format!("Plain{i}")),
        "echo" => Some(t.to_string()),
        "cjk" => Some(if toks.is_empty() {
            format!("\u{65e5}\u{672c}\u{8a9e}{i}")
        } else {
            toks.iter().map(|x| format!("\u{3042}{x}")).collect::<String>() + "\u{3002}"
        }),
        "zh" => Some(format!("\u{8bf7}{}\u{3002}", Regex::new(r"\s+").unwrap().replace_all(t, ""))),
        "sep" => Some(format!("A | B , C : D ! {t}\\")),
        "block" => Some(format!(
            "{}{t}",
            ["== X ", "* X ", "// x ", ".T ", "NOTE: x ", "[x] ", "-- ", "image::x[] ", "|=== ", "term:: ", "X "][i % 11]
        )),
        "space" => Some(format!("  {}\n\nend   \n", t.replace(' ', "  "))),
        "lead" => Some(format!(" \t{t} + \\")),
        _ => unreachable!(),
    }
}

fn mapping(name: &str, texts: &[String]) -> Vec<(usize, String)> {
    texts.iter().enumerate().filter_map(|(i, t)| map_text(name, i, t).map(|m| (i, m))).collect()
}

fn run(data: &[u8], map: &[(usize, String)], target: Option<&str>) -> Vec<u8> {
    let m: Vec<(usize, &str)> = map.iter().map(|(i, s)| (*i, s.as_str())).collect();
    build_for(data, &m, target)
}

/// LocOS inline map of a unit as JSON: `"1": "raw"` or `"1": [open, close]`.
fn codes_json(u: &Unit) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for (k, c) in &u.codes {
        if k.starts_with('~') {
            continue;
        }
        let v = match (&c.standalone, &c.open, &c.close) {
            (Some(s), _, _) => serde_json::Value::String(s.clone()),
            (None, Some(o), Some(cl)) => serde_json::json!([o, cl]),
            _ => panic!("odd code {k}"),
        };
        m.insert(k.clone(), v);
    }
    serde_json::Value::Object(m)
}

/// Documented differences from LocOS (MIGRATION.md "Differences from LocOS", crate README). LocOS's output is
/// broken in these cases, so the golden cannot match byte for byte; everything else in the case is still checked.
///
/// SETEXT: a two-line title (`Title` over `=====`) is a title only while the underline is within one character of
/// the title. LocOS keeps the old underline after a translation changes the length, so its output is no longer a
/// title and the next block runs into it. Bilingua redraws the underline. Units, styles, inline maps and the echo
/// build are still compared; the other builds of these cases differ from LocOS.
const SETEXT: &[&str] = &[
    "random-1", "random-3", "random-95", "random-100", "random-108", "random-110", "random-111", "random-114",
    "random-126", "random-133", "random-160", "random-161", "random-171", "random-173", "random-176", "random-194",
    "random-201", "random-212", "random-218", "random-227", "random-231", "random-234", "random-258", "random-272",
    "random-288", "random-294", "random-311", "random-322", "random-325", "random-347", "random-364", "random-365",
    "random-370", "random-376", "random-377", "random-383", "random-384", "random-408", "random-413", "random-422",
    "random-428", "random-431", "random-432", "random-443", "random-451", "random-458", "random-462", "random-465",
    "random-490", "random-525", "random-542", "random-553", "random-576", "random-578", "random-586", "random-597",
];
/// CELL_GUARD: translated table cells (#788). A `|` in a nested-table cell is escaped, and a cell that starts a line
/// gets the `{empty}` guard when the translation opens with block syntax or a directive; LocOS does neither, so the
/// builds of these cases differ. Units, styles, inline maps and the echo build are still compared, and every other
/// build must match LocOS once both have those two things taken out (`norm`; LocOS digests in
/// `data/cell_guard_norm.json`, made by `tools/gen_cell_guard.py`).
const CELL_GUARD: &[&str] = &[
    "random-5", "random-8", "random-9", "random-32", "random-41", "random-53", "random-54", "random-57",
    "random-59", "random-60", "random-61", "random-63", "random-76", "random-77", "random-80", "random-91",
    "random-94", "random-101", "random-105", "random-124", "random-132", "random-141", "random-146", "random-149",
    "random-166", "random-168", "random-196", "random-197", "random-210", "random-225", "random-228", "random-230",
    "random-240", "random-247", "random-259", "random-264", "random-265", "random-271", "random-275", "random-304",
    "random-316", "random-323", "random-326", "random-334", "random-346", "random-349", "random-357", "random-361",
    "random-371", "random-378", "random-385", "random-394", "random-398", "random-401", "random-402", "random-405",
    "random-421", "random-426", "random-454", "random-456", "random-460", "random-464", "random-467", "random-475",
    "random-478", "random-491", "random-500", "random-501", "random-509", "random-520", "random-527", "random-550",
    "random-551", "random-554", "random-579", "random-590", "random-595",
];
/// GLUED_MACRO: `footnofootnote:[A note.]`: Asciidoctor reads `footnote:[..]` (and `link:`, `xref:`, `image:`, `kbd:`,
/// `btn:`, `mailto:`) even right after a letter; LocOS offered the macro syntax as text.
const GLUED_MACRO: &[&str] = &["random-558"];
/// MARK_REF (structure fuzz): in the `zh` builds (blanks taken out) a translated mark character lands next to a
/// code's mark of the same character; Bilingua writes it as a character reference and the writer then also doubles
/// the code's mark (a `;` is before it), where LocOS writes the run joined (`*b**x`).
const MARK_REF: &[&str] = &["random-206", "random-281"];

/// A build without the #788 differences: every `{empty}` that starts a line and every backslash right before `|`
/// are dropped (the same rule as `norm` in `tools/gen_cell_guard.py`).
fn norm(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"{empty}") && (i == 0 || matches!(b[i - 1], b'\r' | b'\n')) {
            i += 7;
            continue;
        }
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'|') {
            i += 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// A build with the description list delimiters LocOS (before its #788 gate fix) wrote plain: the golden builds
/// put `term:: ` before some units (`block`), which Bilingua now writes as `term{two-colons} `. Also the
/// structure fuzz fix: a translated mark character next to a code's mark of the same character is written as a
/// character reference (`*b*&#42;x`), which LocOS writes plain (`zh` takes the blanks out).
fn undo_dlist(b: &[u8]) -> Vec<u8> {
    let mut s = String::from_utf8_lossy(b).replace("{two-colons}", "::").replace("{two-semicolons}", ";;");
    for c in ['*', '_', '`', '#', '+', '^', '~', ']'] {
        s = s.replace(&format!("&#{};", c as u32), &c.to_string());
    }
    s.into_bytes()
}

/// For a CELL_GUARD case: true when the build differs from LocOS by more than the cell escapes and the guard.
fn beyond_cell_guard(norms: &serde_json::Value, name: &str, key: &str, build: &[u8]) -> bool {
    norms[name][key].as_str() != Some(fnv(&norm(build)).as_str())
}

#[test]
fn matches_locos_on_random_files() {
    let norms: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cell_guard_norm.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(norms.as_object().unwrap().len(), CELL_GUARD.len(), "regenerate data/cell_guard_norm.json");
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 609);
    let (mut refused, mut builds, mut nunits) = (0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        if GLUED_MACRO.contains(&name) {
            continue;
        }
        let setext = SETEXT.contains(&name) || CELL_GUARD.contains(&name);
        let data = hex(c["hex"].as_str().unwrap());
        if c.get("error").is_some() {
            if extract(&data).is_ok() {
                bad.push(format!("{name}: LocOS refuses, we accept"));
            }
            refused += 1;
            continue;
        }
        let doc = match extract(&data) {
            Ok(d) => d,
            Err(e) => {
                bad.push(format!("{name}: we refuse ({e}), LocOS accepts"));
                continue;
            }
        };
        let want: Vec<String> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        let got: Vec<String> = doc.units.iter().map(token_text).collect();
        if got != want {
            bad.push(format!("{name}: units\n  want {want:?}\n  got  {got:?}"));
            continue;
        }
        nunits += want.len();
        let styles: Vec<String> = doc.units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect();
        let wst: Vec<String> = c["styles"].as_array().unwrap().iter().map(|u| u.as_str().unwrap().to_string()).collect();
        if styles != wst {
            bad.push(format!("{name}: styles {styles:?} vs {wst:?}"));
        }
        let maps = c["maps"].as_array().unwrap();
        for (k, u) in doc.units.iter().enumerate() {
            if codes_json(u) != maps[k] {
                bad.push(format!("{name}: inline map of unit {k}\n  want {}\n  got  {}", maps[k], codes_json(u)));
            }
        }
        // LocOS writes UTF-8 for a file that was not; Bilingua keeps the file's encoding (B6), so the bytes differ
        if std::str::from_utf8(&data).is_err() {
            continue;
        }
        let mut differs = false;
        for (label, d) in c["builds"].as_object().unwrap() {
            let b = undo_dlist(&run(&data, &mapping(label, &want), None));
            let got = fnv(&b);
            if got != d.as_str().unwrap() {
                if CELL_GUARD.contains(&name) && beyond_cell_guard(&norms, name, label, &b) {
                    bad.push(format!("{name}: build {label} differs beyond the cell guard"));
                } else if setext && label != "echo" {
                    differs = true;
                } else if !(MARK_REF.contains(&name) && label == "zh") {
                    bad.push(format!("{name}: build {label}"));
                }
            }
            builds += 1;
        }
        if let Some(loc) = c["localized"].as_object() {
            for (lc, d) in loc {
                if lc.ends_with("|map") {
                    continue;
                }
                let label = loc[&format!("{lc}|map")].as_str().unwrap();
                let m = mapping(label, &want);
                let got = undo_dlist(&run(&data, &m, Some(lc)));
                if fnv(&got) != d.as_str().unwrap() {
                    if CELL_GUARD.contains(&name) && beyond_cell_guard(&norms, name, lc, &got) {
                        bad.push(format!("{name}: localized {lc} differs beyond the cell guard"));
                    } else if setext && label != "echo" {
                        differs = true;
                    } else if !(MARK_REF.contains(&name) && label == "zh") {
                        bad.push(format!("{name}: localized {lc} ({label})"));
                    }
                }
                builds += 1;
            }
        }
        if setext && !differs {
            bad.push(format!("{name}: in SETEXT or CELL_GUARD but now matches LocOS: take it off the list"));
        }
    }
    println!("{} cases, {refused} refused, {nunits} units, {builds} builds, {} differences", cases.len(), bad.len());
    assert!(bad.is_empty(), "{}\n{}", bad.len(), bad.join("\n"));
}
