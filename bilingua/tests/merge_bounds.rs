//! BILI-105: a merge bounds what a stored Document can make it do. A Document is plain JSON between extract and
//! merge; these documents are changed by hand the way a holder of the JSON could change them, and each one must be
//! refused under the caller's merge limits with a typed error, while an ordinary file still merges byte for byte.

use bilingua::*;
use std::path::PathBuf;

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

fn zip_entries(doc: &mut Document) -> &mut Vec<ZipEntry> {
    match &mut doc.skeleton {
        Skeleton::Zip { entries, .. } => entries,
        _ => panic!("a zip skeleton"),
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

/// A small Document whose skeleton writes one unit over and over: before the per-unit cap, a JSON file of about
/// 140 KB merged to 64 MB. Now any unit written more than `MERGE_REFS_PER_UNIT` times is refused before anything is
/// written, whatever the output limit.
#[test]
fn a_document_that_writes_one_unit_many_times_is_refused() {
    let value = "word ".repeat(60);
    let src = format!("{{\"k\": \"{}\"}}\n", value.trim_end());
    let mut doc = extract("a.json", src.as_bytes(), &opts()).unwrap();
    let parts = text_parts(&mut doc);
    let slot = parts.iter().find_map(|p| if let SkelPart::Unit(i) = p { Some(*i) } else { None }).expect("a unit slot");
    // the slots an extraction makes are fine; one more than the cap to that unit is not
    let have = parts.iter().filter(|p| matches!(p, SkelPart::Unit(i) if *i == slot)).count();
    let mut ok = doc.clone();
    text_parts(&mut ok).extend(std::iter::repeat_n(SkelPart::Unit(slot), bilingua_model::guard::MERGE_REFS_PER_UNIT - have));
    assert!(merge(&ok).is_ok());
    let mut over = doc.clone();
    text_parts(&mut over).extend(std::iter::repeat_n(SkelPart::Unit(slot), bilingua_model::guard::MERGE_REFS_PER_UNIT - have + 1));
    assert_eq!(limit_kind(merge(&over)), LimitKind::Skeleton);
    // the amplification case: thousands of slots to one unit of a small file, refused at once
    let mut amp = doc.clone();
    text_parts(&mut amp).extend(std::iter::repeat_n(SkelPart::Unit(slot), 200_000));
    let t = std::time::Instant::now();
    assert_eq!(limit_kind(merge(&amp)), LimitKind::Skeleton);
    assert!(t.elapsed().as_secs() < 5);
    // and through the JSON a host would store
    let json = serde_json::to_string(&amp).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(limit_kind(merge(&back)), LimitKind::Skeleton);
}

#[test]
fn more_unit_slots_than_the_merge_allows_are_refused() {
    let text: String = (0..12).map(|i| format!("Paragraph {i}.\n\n")).collect();
    let mut doc = extract("a.txt", text.as_bytes(), &opts()).unwrap();
    let n = doc.units.len();
    // every unit referred to the cap, more slots in all than a file of 10 units allows
    text_parts(&mut doc).extend((0..n).flat_map(|i| std::iter::repeat_n(SkelPart::Unit(i), 3)));
    let small = MergeOptions::new().with_limits(Limits::new().with_max_units(n.max(10)).with_max_lines(0));
    assert_eq!(limit_kind(merge_with(&doc, &small)), LimitKind::Skeleton);
}

#[test]
fn a_slot_for_a_unit_the_document_does_not_have_is_refused() {
    let mut doc = extract("a.txt", b"Hello world.\n", &opts()).unwrap();
    text_parts(&mut doc).push(SkelPart::Unit(7));
    assert_eq!(limit_kind(merge(&doc)), LimitKind::Skeleton);
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
fn the_limits_of_extract_apply_at_merge() {
    let doc = extract("a.txt", b"Hello world.\n\nTwo.\n\nThree.\n", &opts()).unwrap();
    // more units than the caller's limit
    let few = MergeOptions::new().with_limits(Limits::new().with_max_units(2));
    assert_eq!(limit_kind(merge_with(&doc, &few)), LimitKind::Units);
    // a source made longer than an extraction allows
    let mut long = doc.clone();
    long.units[0].segments[0].source = vec![Inline::text("x".repeat(200))];
    let short = MergeOptions::new().with_limits(Limits::new().with_max_unit_chars(100));
    assert_eq!(limit_kind(merge_with(&long, &short)), LimitKind::UnitChars);
    // a target longer than four times the unit limit
    let mut t = doc.clone();
    t.units[0].segments[0].target = vec![Inline::text("y".repeat(401))];
    assert_eq!(limit_kind(merge_with(&t, &short)), LimitKind::UnitChars);
    t.units[0].segments[0].target = vec![Inline::text("y".repeat(400))];
    assert!(merge_with(&t, &short).is_ok());
    // context text over the limit
    let mut c = doc.clone();
    c.units[0].context.notes = vec!["n".repeat(2000)];
    let tight = MergeOptions::new().with_limits(Limits::new().with_max_context_bytes(1000));
    assert_eq!(limit_kind(merge_with(&c, &tight)), LimitKind::Context);
}

#[test]
fn an_unsafe_zip_entry_name_is_not_written() {
    let doc = extract("page.docx", include_bytes!("fixtures/page.docx"), &opts()).unwrap();
    // the last entry the archive itself holds (a format's own records start with "\0bilingua-")
    let at = zip_entries(&mut doc.clone()).iter().rposition(|e| !e.name.starts_with('\0')).unwrap();
    let last = zip_entries(&mut doc.clone())[at].name.clone();
    let first = zip_entries(&mut doc.clone()).iter().find(|e| !e.name.starts_with('\0') && e.name != last).unwrap().name.clone();
    for bad in [
        format!("../{last}"),
        format!("/{last}"),
        format!("C:{last}"),
        format!("word\\..\\{last}"),
        format!("word/./{last}"),
        format!("word//{last}"),
        format!("{last}\u{1}"),
        format!("{last}\n"),
        "word/ ../x.xml".to_string(),
        String::new(),
        first.clone(),
        first.to_uppercase(),
    ] {
        let mut d = doc.clone();
        zip_entries(&mut d)[at].name = bad.clone();
        let r = merge(&d);
        assert_eq!(limit_kind(r), LimitKind::ZipMalformed, "{bad:?}");
    }
    // the message names the reason
    let mut d = doc.clone();
    zip_entries(&mut d)[at].name = format!("../{last}");
    assert!(merge(&d).unwrap_err().to_string().contains(".."));
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
    // a close tag where the source has an open of that id: not one of the source's tags
    let mut wrong_kind = doc.clone();
    let Inline::Tag { id, .. } = &open else { unreachable!() };
    wrong_kind.units[0].segments[0].target = vec![Inline::tag(id.clone(), TagKind::Standalone, "<b>"), Inline::text("Hallo")];
    assert!(matches!(merge(&wrong_kind), Err(Error::InventedTag(_))));
}

#[test]
fn a_bilingual_file_gets_the_tag_check_too() {
    let x = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file source-language=\"en\" target-language=\"de\" datatype=\"plaintext\" original=\"f\"><body><trans-unit id=\"a\"><source>Hi <g id=\"1\">bold</g></source><target>Hallo <g id=\"1\">fett</g></target></trans-unit></body></file></xliff>\n";
    let doc = extract("a.xlf", x.as_bytes(), &opts()).unwrap();
    assert_eq!(merge(&doc).unwrap(), x.as_bytes());
    let mut bad = doc.clone();
    bad.units[0].segments[0].target = vec![Inline::text("Hallo "), Inline::tag("99", TagKind::Standalone, "<x/>")];
    assert!(matches!(merge(&bad), Err(Error::InventedTag(_))));
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap().flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            out.push((name, std::fs::read(f.path()).unwrap()));
        }
    }
    out.sort();
    assert!(out.len() >= 30, "corpus went missing: {} files", out.len());
    out
}

/// Every file of the corpus, extracted in each mode and with every segment pseudo-translated, passes the merge
/// bounds: the caps sit above anything an extraction makes.
#[test]
fn no_extraction_of_the_corpus_trips_a_merge_bound() {
    let mut failures = Vec::new();
    for (name, bytes) in corpus() {
        for mode in [Granularity::Paragraph, Granularity::Sentence] {
            for locos in [false, true] {
                let eo = if locos { bilingua_model::profile::locos().extract_options().with_source_lang("en").with_target_lang("de-DE").with_granularity(mode) } else { opts().with_target_lang("de-DE").with_granularity(mode) };
                let Ok(doc) = extract(&name, &bytes, &eo) else { continue };
                let mo = if locos { bilingua_model::profile::locos().merge_options(&doc.format) } else { MergeOptions::default() };
                let mut t = doc.clone();
                let ids: Vec<(String, Vec<Inline>)> = t.units.iter().flat_map(|u| u.segments.iter().map(|s| (s.id.clone(), s.source.clone()))).collect();
                for (id, src) in ids {
                    let mut tgt = vec![Inline::text("\u{dc} ")];
                    tgt.extend(src);
                    let _ = t.set_target(&id, tgt);
                }
                for (what, d) in [("untouched", &doc), ("translated", &t)] {
                    if let Err(e @ (Error::Limit(_) | Error::InventedTag(_) | Error::DuplicatedTag(_))) = merge_with(d, &mo) {
                        failures.push(format!("{name} {mode:?} locos={locos} {what}: {e}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
