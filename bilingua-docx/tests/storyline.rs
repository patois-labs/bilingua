//! Storyline "Export to Word" bilingual tables: only the Translation cells are units (LocOS
//! `app/filters/docx_bilingual.py`). Fixtures and golden come from `tests/tools/gen_storyline.py` (python-docx files,
//! LocOS units). Ports the detection cases of LocOS `tests/verify_docx_bilingual*` through the golden: the merged
//! label row, the repeated header row, the empty Translation cell, the `Translation:` header, and a style guide whose
//! prose outweighs its term table.

use bilingua_docx::Docx;
use bilingua_model::*;
use proptest::prelude::*;

const EXPORT: &[u8] = include_bytes!("data/storyline_export.docx");
const GUIDE: &[u8] = include_bytes!("data/storyline_styleguide.docx");
const GOLDEN: &str = include_str!("data/storyline_golden.json");

fn golden(name: &str) -> (Vec<String>, Vec<String>) {
    let g: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    let list = |k: &str| g[name][k].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    (list("units"), list("warnings"))
}

fn texts(d: &Document) -> Vec<String> {
    // document properties (meta-*) are Bilingua units LocOS does not have (a known X4 difference)
    d.units.iter().filter(|u| u.translate && !u.id.starts_with("meta-")).map(|u| plain_text(&u.source())).collect()
}

#[test]
fn only_the_translation_column_is_translated() {
    let d = Docx.extract(EXPORT, &ExtractOptions::default()).unwrap();
    let (want, warn) = golden("storyline_export.docx");
    assert_eq!(texts(&d), want);
    assert_eq!(warn, ["docx_bilingual_table"]);
    let w = d.warnings.iter().find(|w| w.kind == "docx_bilingual_table").expect("the upload notice");
    assert!(w.message.contains("(4 rows in 2 tables)") && w.message.contains("1 rows have an empty Translation cell"), "{}", w.message);
    // ids, source text, headings and the project table are still there, as locked context
    let locked: Vec<String> = d.units.iter().filter(|u| !u.translate).map(|u| plain_text(&u.source())).collect();
    for s in ["6Xyz1", "Original Text", "Scene 1: Welcome", "Lumen Bakery onboarding", "Slide 1.1 Opening", "This row was never filled."] {
        assert!(locked.iter().any(|t| t == s), "{s} should be locked: {locked:?}");
    }
    // an untouched merge gives the same parts
    let out = Docx.merge(&d).unwrap();
    let back = Docx.extract(&out, &ExtractOptions::default()).unwrap();
    assert_eq!(back.units, d.units);
}

#[test]
fn a_translated_merge_writes_only_the_translation_cells() {
    let mut d = Docx.extract(EXPORT, &ExtractOptions::default()).unwrap();
    let todo: Vec<(String, Vec<Inline>)> = d
        .units
        .iter()
        .filter(|u| u.translate)
        .map(|u| (u.segments[0].id.clone(), vec![Inline::text(format!("FR {}", plain_text(&u.source())))]))
        .collect();
    d.set_targets(todo).unwrap();
    let back = Docx.extract(&Docx.merge(&d).unwrap(), &ExtractOptions::default()).unwrap();
    assert_eq!(texts(&back), ["FR Welcome to the bakery.", "FR Wash your hands before every shift.", "FR Ovens are hot.", "FR Use the gloves.", "FR Thank you!"]);
    let src: Vec<String> = back.units.iter().filter(|u| !u.translate).map(|u| plain_text(&u.source())).collect();
    assert!(src.iter().any(|t| t == "Welcome to the bakery."), "the source column keeps its text");
}

#[test]
fn a_document_with_more_prose_than_table_is_ordinary() {
    let d = Docx.extract(GUIDE, &ExtractOptions::default()).unwrap();
    let (want, warn) = golden("storyline_styleguide.docx");
    assert_eq!(texts(&d), want);
    assert!(warn.is_empty() && !d.warnings.iter().any(|w| w.kind == "docx_bilingual_table"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn mutated_exports_never_panic(at in 0usize..1_000_000, val in any::<u8>()) {
        let mut b = EXPORT.to_vec();
        let i = at % b.len();
        b[i] = val;
        if let Ok(d) = Docx.extract(&b, &ExtractOptions::default()) {
            let _ = Docx.merge(&d);
        }
    }
}

/// W4 docx-5: under the LocOS profile the upload notice comes out too, with LocOS's counts and wording.
#[test]
fn the_locos_profile_gives_the_notice_too() {
    let o = profile::locos().extract_options().with_granularity(Granularity::Paragraph);
    let d = Docx.extract(EXPORT, &o).unwrap();
    let native = Docx.extract(EXPORT, &ExtractOptions::default()).unwrap();
    let w = d.warnings.iter().find(|w| w.kind == "docx_bilingual_table").expect("the upload notice under the profile");
    let n = native.warnings.iter().find(|w| w.kind == "docx_bilingual_table").unwrap();
    assert_eq!(w.message, n.message);
    let g = Docx.extract(GUIDE, &o).unwrap();
    assert!(!g.warnings.iter().any(|w| w.kind == "docx_bilingual_table"));
}
