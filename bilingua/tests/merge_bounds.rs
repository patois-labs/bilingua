//! BILI-105: a merge bounds what a stored Document can make it do. A Document is plain JSON between extract and
//! merge; these documents are changed by hand the way a holder of the JSON could change them, and each one must be
//! refused under the caller's merge limits, while an ordinary file still merges byte for byte.

use bilingua::*;

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph)
}

fn limit_kind(r: Result<Vec<u8>, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => e.kind,
        other => panic!("expected a limit refusal, got {:?}", other.map(|b| b.len())),
    }
}

fn text_parts(doc: &mut Document) -> &mut Vec<SkelPart> {
    match &mut doc.skeleton {
        Skeleton::Text { parts, .. } => parts,
        _ => panic!("a text skeleton"),
    }
}

#[test]
fn an_ordinary_file_merges_byte_for_byte_under_the_bounds() {
    for (name, bytes) in [("a.txt", &b"Hello world.\n\nSecond paragraph.\n"[..]), ("page.html", include_bytes!("fixtures/page.html")), ("page.docx", include_bytes!("fixtures/page.docx"))] {
        let doc = extract(name, bytes, &opts()).unwrap();
        assert_eq!(merge(&doc).unwrap(), filter_for_test(&doc).merge(&doc).unwrap(), "{name}");
        if !name.ends_with(".docx") {
            assert_eq!(merge(&doc).unwrap(), bytes, "{name}");
        }
    }
}

fn filter_for_test(doc: &Document) -> Box<dyn Filter> {
    filters().into_iter().find(|f| f.id() == doc.format).unwrap()
}

#[test]
fn a_document_that_refers_to_one_unit_many_times_is_refused() {
    let mut doc = extract("a.txt", b"Hello world.\n", &opts()).unwrap();
    let parts = text_parts(&mut doc);
    assert!(parts.iter().any(|p| matches!(p, SkelPart::Unit(0))));
    // more unit slots than the merge allows for a file of 10 units
    parts.extend(std::iter::repeat_n(SkelPart::Unit(0), 4 * 10 + 1));
    let small = MergeOptions::new().with_limits(Limits::new().with_max_units(10));
    assert_eq!(limit_kind(merge_with(&doc, &small)), LimitKind::OutputBytes);
    // within the slot cap, but the repeats would write more than the output limit: refused before writing
    let mut doc = extract("a.txt", "x".repeat(1000).as_bytes(), &opts()).unwrap();
    text_parts(&mut doc).extend(std::iter::repeat_n(SkelPart::Unit(0), 30));
    let cap = MergeOptions::new().with_limits(Limits::new().with_max_output_bytes(20_000));
    assert_eq!(limit_kind(merge_with(&doc, &cap)), LimitKind::OutputBytes);
    // with the default limits the same document is still small enough to write
    assert!(merge(&doc).is_ok());
}

#[test]
fn output_over_the_cap_is_refused() {
    let big = "word ".repeat(2000);
    let doc = extract("a.txt", big.as_bytes(), &opts()).unwrap();
    let cap = MergeOptions::new().with_limits(Limits::new().with_max_output_bytes(5_000));
    assert_eq!(limit_kind(merge_with(&doc, &cap)), LimitKind::OutputBytes);
    assert_eq!(merge(&doc).unwrap(), big.as_bytes());
    assert_eq!(Limits::default().max_output_bytes, 1024 * 1024 * 1024);
}

#[test]
fn an_unsafe_zip_entry_name_is_not_written() {
    let mut doc = extract("page.docx", include_bytes!("fixtures/page.docx"), &opts()).unwrap();
    let Skeleton::Zip { entries, .. } = &mut doc.skeleton else { panic!("a zip skeleton") };
    let last = entries.last_mut().unwrap();
    last.name = format!("../{}", last.name);
    let e = merge(&doc).unwrap_err();
    assert!(e.to_string().contains(".."), "{e}");
}

#[test]
fn a_stored_target_with_tags_the_source_does_not_have_is_refused() {
    let doc = extract("a.html", b"<p>Hello <b>bold</b> world.</p>", &opts()).unwrap();
    let seg = &doc.units[0].segments[0];
    let open = seg.source.iter().find(|i| matches!(i, Inline::Tag { kind: TagKind::Open, .. })).cloned().expect("an open tag");
    let close = seg.source.iter().find(|i| matches!(i, Inline::Tag { kind: TagKind::Close, .. })).cloned().expect("a close tag");
    // an ordinary target merges
    let mut ok = doc.clone();
    ok.units[0].segments[0].target = vec![Inline::text("Hallo "), open.clone(), Inline::text("fett"), close.clone(), Inline::text(" Welt.")];
    assert!(String::from_utf8(merge(&ok).unwrap()).unwrap().contains("Hallo <b>fett</b> Welt."));
    // a pair repeated: more tags than the source has, written straight into the stored target
    let mut twice = doc.clone();
    twice.units[0].segments[0].target = vec![open.clone(), Inline::text("a"), close.clone(), open.clone(), Inline::text("b"), close.clone()];
    assert!(matches!(merge(&twice), Err(Error::DuplicatedTag(_))));
    // an open with no close in the source: a tag the source does not have
    let mut invented = doc.clone();
    invented.units[0].segments[0].target = vec![Inline::tag("zz", TagKind::Open, "<i>"), Inline::text("Hallo")];
    assert!(matches!(merge(&invented), Err(Error::InventedTag(_))));
}
