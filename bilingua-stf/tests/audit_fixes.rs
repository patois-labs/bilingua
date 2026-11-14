//! Regression tests for the 0.9.1 security audit: a long Type header is not copied into every unit.
mod common;
use bilingua_model::*;
use bilingua_stf::{records, StfFilter};
use common::*;

/// A 20 000-character Type header over 20 000 rows used to put 400 MB of section names on the units of a 190 KB
/// file. The section name is now kept to the 61 characters a section line's name may have.
#[test]
fn a_long_type_header_is_not_copied_per_unit() {
    let rows = 20_000;
    let mut data = format!("Language code: en_US\nType: {}\n", "A".repeat(20_000));
    for i in 0..rows {
        data.push_str(&format!("k{i}\tx\n"));
    }
    let doc = extract(data.as_bytes()).expect("extract");
    assert_eq!(doc.units.len(), rows);
    let style = format!("stf:{}", "a".repeat(61));
    assert!(doc.units.iter().all(|u| u.context.style.as_deref() == Some(style.as_str())));
    let total: usize = doc.units.iter().map(|u| u.context.style.as_ref().map_or(0, String::len)).sum();
    assert!(total < 8 * data.len(), "{total} bytes of section names for a file of {} bytes", data.len());
    // the header line itself is untouched
    assert_eq!(StfFilter.merge(&doc).expect("merge"), data.as_bytes());
    let recs = records(data.as_bytes()).expect("records");
    assert!(!recs.is_empty() && recs.iter().all(|r| r.section.len() == 61));
}

/// An ordinary Type header names the section as before, and a section line still takes over from it.
#[test]
fn an_ordinary_type_header_names_the_section() {
    let data = "Language code: en_US\nType: Source\nk1\tOne\n--- Translated ---\nk2\tTwo\tZwei\n";
    let doc = extract(data.as_bytes()).expect("extract");
    let styles: Vec<&str> = doc.units.iter().filter_map(|u| u.context.style.as_deref()).collect();
    assert_eq!(styles, ["stf:source", "stf:translated"]);
    let data = "Language code: en_US\nType:  Bilingual \nk1\tOne\n";
    let doc = extract(data.as_bytes()).expect("extract");
    assert_eq!(doc.units[0].context.style.as_deref(), Some("stf:bilingual"));
    let data = "Language code: en_US\nk1\tOne\n";
    let doc = extract(data.as_bytes()).expect("extract");
    assert_eq!(doc.units[0].context.style.as_deref(), Some("stf:source"));
}
