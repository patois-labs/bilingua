//! Port of `tests/verify_mozilla_properties.py`: the Mozilla dialect rule seen through the filter.

mod common;

use bilingua_model::*;
use bilingua_properties::Properties;
use common::*;

const MOZ: &str = "# This Source Code Form is subject to the terms of the Mozilla Public\n\
# License, v. 2.0.\n\
\n\
# LOCALIZATION NOTE (downloadsTitleFiles): Semicolon-separated list of plural forms.\n\
# #1 number of files; #2 overall download percent\n\
downloadsTitleFiles=#1 file - Downloads;#1 files - Downloads\n\
fileExists=The file %S already exists.\n\
copyOf=%1$S of %2$S\n";
const JAVA: &str = "# Application messages\ngreeting=Hello {0}, it''s {1,number} o''clock\nfarewell = Goodbye\n";

fn mode(data: &str) -> String {
    match extract(data.as_bytes()).skeleton {
        Skeleton::Text { encoding, .. } => encoding,
        _ => unreachable!(),
    }
}

#[test]
fn detection_rule() {
    // A file is Mozilla when it is written as UTF-8 for an all-ASCII file; Java ASCII stays ASCII.
    assert_eq!(mode(MOZ), "utf-8");
    assert_eq!(mode(JAVA), "ascii");
    // each marker on its own is enough
    for m in ["a=b\n# LOCALIZATION NOTE (a): x\n", "a=Open %S\n", "a=Open %2$S now\n", "a=#1 tab;#1 tabs\n", "# Mozilla Public License\na=b\n"] {
        assert_eq!(mode(m), "utf-8", "{m}");
    }
    // Java-only shapes are never Mozilla, even with a Mozilla marker
    for j in ["a=Open %S {0}\n", "a=Open {1,number}\n# LOCALIZATION NOTE\n"] {
        assert_eq!(mode(j), "ascii", "{j}");
    }
    // lower-case %s, &#1; and #10 are not markers
    assert_eq!(mode("a=Open %s\nb=&#1;\nc=#10 items\n"), "ascii");
}

#[test]
fn output_mode() {
    assert_eq!(mode(MOZ), "utf-8");
    assert_eq!(mode(JAVA), "ascii");
    assert_eq!(mode("a=caf\u{e9}\n"), "utf-8");
}

#[test]
fn mozilla_files_are_written_as_utf8() {
    let texts = units(MOZ.as_bytes());
    let k = texts.iter().position(|t| t == "The file %S already exists.").expect("unit");
    let out = translate(MOZ.as_bytes(), &[(k, "Die Datei %S existiert bereits (gr\u{f6}\u{df}er).")]);
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("fileExists=Die Datei %S existiert bereits (gr\u{f6}\u{df}er).\n"), "{out}");
    assert!(!out.contains("\\u00"));
    // a plain Java ASCII file keeps native2ascii escapes
    let j = units(JAVA.as_bytes()).iter().position(|t| t == "Goodbye").unwrap();
    let jout = String::from_utf8(translate(JAVA.as_bytes(), &[(j, "Auf Wiedersehen, gr\u{fc}\u{df}")])).unwrap();
    assert!(jout.contains("\\u00FC"), "{jout}");
    // %S and the plural #1 are tags in the Mozilla dialect
    let d = extract(MOZ.as_bytes());
    let n = |i: usize| d.units[i].segments[0].source.iter().filter(|x| matches!(x, Inline::Tag { .. })).count();
    assert_eq!(n(0), 2);
    assert_eq!(n(1), 1);
    assert_eq!(n(2), 2);
    let _ = Properties;
}
