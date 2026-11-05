//! Byte comparison against files written by the Localization OS filter (`tools/gen_golden.py`, run once with the
//! LocOS venv): two catalogs x 11 target languages and the source language x full and half translations.
//!
//! The Xcode-style catalog must match byte for byte. The hand-formatted one (tabs, one-line objects) is
//! re-serialised by LocOS but spliced by Bilingua, so there the two files must agree once insignificant white
//! space is removed (same keys, same order, same text).
mod common;
use bilingua_model::*;
use bilingua_xcstrings::Xcstrings;
use common::*;

const LANGS: &[(&str, &str)] = &[
    ("de", "[de] "),
    ("fr", "[fr] "),
    ("es", "[es] "),
    ("ru", "\u{43f}\u{435}\u{440}\u{435}\u{432}\u{43e}\u{434} "),
    ("pl", "[pl] "),
    ("ar", "\u{62a}\u{631}\u{62c}\u{645}\u{629} "),
    ("ja", "\u{7ffb}\u{8a33} "),
    ("zh-Hans", "\u{7ffb}\u{8bd1} "),
    ("ko", "\u{bc88}\u{c5ed} "),
    ("pt_BR", "[pt] "),
    ("tr", "[tr] "),
];

fn strip_ws(b: &[u8]) -> Vec<u8> {
    let (mut out, mut in_str, mut esc) = (Vec::new(), false, false);
    for &c in b {
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
            out.push(c);
        } else if !matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
            out.push(c);
        }
    }
    out
}

fn golden(name: &str, mode: &str, loc: &str) -> Vec<u8> {
    let path = format!("{}/tests/data/golden/{name}.{mode}.{loc}.xcstrings", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn run(name: &str, data: &[u8], exact: bool) {
    run_with(name, data, exact, false)
}

/// `lf`: the LocOS file has LF line endings where the input had CRLF; Bilingua keeps CRLF, so compare after LF.
fn run_with(name: &str, data: &[u8], exact: bool, lf: bool) {
    let mut cases: Vec<(Option<&str>, &str)> = LANGS.iter().map(|(l, p)| (Some(*l), *p)).collect();
    cases.push((None, "[src] "));
    for (loc, prefix) in cases {
        for mode in ["full", "half"] {
            let mut d = ex(data, loc);
            for (n, u) in d.units.iter_mut().filter(|u| u.translate).enumerate() {
                if mode == "full" || n.is_multiple_of(2) {
                    let (text, map) = profile::tokens::to_locos_text(&u.source());
                    u.segments[0].target = profile::tokens::from_locos_text(&format!("{prefix}{text}"), &map);
                    u.segments[0].state = SegState::Draft;
                }
            }
            let mut got = Xcstrings.merge(&d).unwrap();
            if lf {
                got = String::from_utf8(got).unwrap().replace("\r\n", "\n").into_bytes();
            }
            let want = golden(name, mode, loc.unwrap_or("source"));
            let label = format!("{name} {mode} {}", loc.unwrap_or("source"));
            if let Ok(dir) = std::env::var("XC_DUMP") {
                std::fs::write(format!("{dir}/{name}.{mode}.{}.got", loc.unwrap_or("source")), &got).unwrap();
            }
            if exact {
                assert!(got == want, "{label}: bytes differ from LocOS\n--- got\n{}\n--- want\n{}", String::from_utf8_lossy(&got), String::from_utf8_lossy(&want));
            } else {
                assert!(
                    strip_ws(&got) == strip_ws(&want),
                    "{label}: differs from LocOS\n--- got\n{}\n--- want\n{}",
                    String::from_utf8_lossy(&got),
                    String::from_utf8_lossy(&want)
                );
            }
        }
    }
}

#[test]
fn xcode_style_catalog_matches_locos_byte_for_byte() {
    run("locos_Localizable", LOCOS_CATALOG, true);
}

#[test]
fn hand_formatted_catalog_matches_locos_after_white_space() {
    run("tabs_en_us", include_bytes!("data/tabs_en_us.xcstrings"), false);
}

const CORPUS: &str = "../oracle/corpus/xcstrings";

fn corpus(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/{CORPUS}/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn corpus_bakery_matches_locos_byte_for_byte() {
    run("bakery", &corpus("bakery.xcstrings"), true);
}

#[test]
fn corpus_compact_with_bom_matches_locos_byte_for_byte() {
    run("compact_bom", &corpus("compact_bom.xcstrings"), true);
}

#[test]
fn corpus_crlf_matches_locos_apart_from_the_line_endings() {
    run_with("bakery_crlf", &corpus("bakery_crlf.xcstrings"), true, true);
}
