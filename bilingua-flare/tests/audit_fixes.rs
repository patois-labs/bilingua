//! Regression tests for the 0.9.1 security audit: merge never takes its resource limits from the document.

mod common;
use bilingua_flare::Flare;
use bilingua_model::*;
use common::*;

fn swap(doc: &mut Document, name: &str, data: Vec<u8>) {
    let Skeleton::Zip { entries, .. } = &mut doc.skeleton else { panic!("a zip skeleton") };
    let e = entries.iter_mut().find(|e| e.name == name).unwrap_or_else(|| panic!("no entry {name}"));
    e.data = ZipData::Raw(data);
}

/// A document whose stored archive breaks the default zip ratio and whose stored options raise the limits is
/// refused at merge, as the same archive is at extract.
#[test]
fn stored_limits_are_not_the_merge_policy() {
    let pad = vec![0u8; 2 * 1024 * 1024];
    let padded = project_with(&[("Content/Resources/pad.bin", &pad)], &[]);
    assert!(matches!(Flare.extract(&padded, &ExtractOptions::new()), Err(Error::Limit(e)) if e.kind == LimitKind::ZipRatio));

    let mut doc = extract(&project());
    set(&mut doc, 0, "Titre");
    swap(&mut doc, "flprjzip", padded);
    swap(&mut doc, "flprjzip-options", br#"{"include_locked":false,"limits":{"max_zip_ratio":1000000,"zip_ratio_floor_bytes":1099511627776}}"#.to_vec());
    let doc: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert!(matches!(Flare.merge(&doc), Err(Error::Limit(e)) if e.kind == LimitKind::ZipRatio));
}

/// Stored limits cannot tighten the policy either, and options without limits (or with unreadable ones) are fine:
/// only `include_locked` is read.
#[test]
fn only_include_locked_is_read_from_the_stored_options() {
    let mut want = extract(&project());
    set(&mut want, 0, "Titre");
    let want_bytes = merge(&want);
    for stored in [r#"{"include_locked":false,"limits":{"max_zip_entries":2}}"#, r#"{"include_locked":false}"#, r#"{"include_locked":false,"limits":"none"}"#] {
        let mut doc = want.clone();
        swap(&mut doc, "flprjzip-options", stored.as_bytes().to_vec());
        assert_eq!(Flare.merge(&doc).unwrap(), want_bytes, "{stored}");
    }
}
