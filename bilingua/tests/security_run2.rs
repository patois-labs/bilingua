//! Regression tests for the facade fixes of security review run 2: a file with no extension is sniffed only up to
//! `SNIFF_MAX_BYTES` (G5-8), and two batch members never share one document name (F12).

use bilingua::batch;
use bilingua_model::{Error, ExtractOptions};
use bilingua_zip::{write_zip, EntryMeta};

/// A JSON file with no extension over the sniffing cap is refused at once (each sniffer used to parse all of it
/// under its own default limits, one after another); a small one is still found by sniffing.
#[test]
fn extensionless_sniffing_reads_a_bounded_prefix() {
    let small = br#"{"greeting": "Hello there"}"#;
    assert!(bilingua::detect("strings", small).is_some());
    let long = "word ".repeat(600);
    let items: String = (0..2_000).map(|i| format!("\"k{i}\": \"Hello there {i} {long}\",")).collect();
    let big = format!("{{{}}}", items.trim_end_matches(','));
    assert!(big.len() > 4 * 1024 * 1024);
    assert!(bilingua::detect("strings", big.as_bytes()).is_none());
    assert!(matches!(bilingua::extract("strings", big.as_bytes(), &ExtractOptions::new()), Err(Error::Unsupported(_))));
    // with its extension the same file opens
    assert!(bilingua::extract("strings.json", big.as_bytes(), &ExtractOptions::new()).is_ok());
}

/// `a/b.md` and `a__b.md` are both written as `a__b.md`: the second one is skipped with its reason, as a repeated
/// path is, so no document overwrites another.
#[test]
fn batch_document_names_unique() {
    let entry = |n: &str, d: &str| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.as_bytes().to_vec());
    let zip = write_zip(&[entry("a/b.md", "# One\n"), entry("a__b.md", "# Two\n"), entry("c.md", "# Three\n")]).unwrap();
    let b = batch::expand(&zip).unwrap();
    let names: Vec<&str> = b.members.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["a__b.md", "c.md"]);
    assert_eq!(b.skipped.len(), 1);
    assert_eq!(b.skipped[0].path, "a__b.md");
    assert!(b.skipped[0].reason.contains("duplicate name"), "{}", b.skipped[0].reason);
}
