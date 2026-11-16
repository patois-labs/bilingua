//! Regression tests for the 0.9.1 security audit: a file full of repeated trans-unit ids opens in linear time
//! with the ids it always got, and a file over the unit limit is refused while it is scanned.

use bilingua_model::{Error, ExtractOptions, Filter, LimitKind, Limits};
use bilingua_xliff::{filters, Doc};
use std::collections::HashSet;
use std::time::{Duration, Instant};

fn xliff() -> Box<dyn Filter> {
    filters().into_iter().find(|f| f.id() == "xliff").unwrap()
}

fn file_of(ids: &[String]) -> Vec<u8> {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><xliff version=\"1.2\"><file source-language=\"en\" target-language=\"de\" datatype=\"plaintext\" original=\"f\"><body>");
    for id in ids {
        s.push_str(&format!("<trans-unit id=\"{id}\"><source>x</source></trans-unit>"));
    }
    s.push_str("</body></file></xliff>");
    s.into_bytes()
}

/// The rule as it was written before the fix: the first free of `base`, `base~2`, `base~3`, ..., searched from
/// the start every time.
fn naive(ids: &[String]) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    ids.iter()
        .map(|base| {
            let mut id = base.clone();
            let mut n = 1;
            while !taken.insert(id.clone()) {
                n += 1;
                id = format!("{base}~{n}");
            }
            id
        })
        .collect()
}

/// Repeats, and ids that already look like a made-up one (`a~2` written in the file before and after the repeats
/// of `a` reach it): every segment gets exactly the id the search from 2 gave.
#[test]
fn repeated_ids_get_the_same_names_as_before() {
    let mut ids: Vec<String> = ["a", "a", "a~2", "a", "a~3", "b", "a~2", "a", "a~2~2", "a~5", "a", "a", "b", "a~2", "b~2", "b", "a~7", "a", "a"].iter().map(|s| s.to_string()).collect();
    for i in 0..300 {
        ids.push(["a", "b", "a~2", "c"][i % 4].to_string());
    }
    let bytes = file_of(&ids);
    let want = naive(&ids);
    let doc = Doc::from_bytes(bytes.clone()).unwrap();
    let got: Vec<String> = (0..doc.len()).map(|i| doc.seg(i).id.to_string()).collect();
    assert_eq!(got, want);
    // the document's unit ids are made from the file's unit ids by the same rule, and each single segment is
    // named by its unit
    let d = xliff().extract(&bytes, &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), want);
    assert_eq!(d.units.iter().map(|u| u.segments[0].id.clone()).collect::<Vec<_>>(), want);
    assert_eq!(xliff().merge(&d).unwrap(), bytes);
}

/// 16 000 trans-units with one id took 28 s to open and 64 s to extract (debug build); each repeat walked all the
/// earlier ones.
#[test]
fn one_id_repeated_many_times_opens_in_linear_time() {
    let n = 16_000;
    let bytes = file_of(&vec!["a".to_string(); n]);
    let start = Instant::now();
    let doc = Doc::from_bytes(bytes.clone()).unwrap();
    assert_eq!(doc.len(), n);
    assert_eq!(doc.seg(n - 1).id, format!("a~{n}"));
    let d = xliff().extract(&bytes, &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.len(), n);
    assert_eq!(d.units[n - 1].id, format!("a~{n}"));
    assert_eq!(d.units[n - 1].segments[0].id, format!("a~{n}"));
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
}

/// A file over the unit limit is refused with the unit limit's error, by the scanner (no work per repeated id
/// comes first), for XLIFF 1.2 and for XLIFF 2. A file at the limit is read.
#[test]
fn a_file_over_the_unit_limit_is_refused_while_scanning() {
    let small = Limits::new().with_max_units(100);
    let opts = ExtractOptions::new().with_limits(small);
    let big = file_of(&vec!["a".to_string(); 60_000]);
    let start = Instant::now();
    assert!(matches!(xliff().extract(&big, &opts), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    assert!(matches!(Doc::from_bytes_limited(big, false, &small), Err(bilingua_xliff::Error::Limit(e)) if e.kind == LimitKind::Units));
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
    assert_eq!(xliff().extract(&file_of(&vec!["a".to_string(); 100]), &opts).unwrap().units.len(), 100);

    let mut x2 = String::from("<xliff version=\"2.0\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" srcLang=\"en\"><file id=\"f\">");
    for _ in 0..101 {
        x2.push_str("<unit id=\"u\"><segment><source>x</source></segment></unit>");
    }
    x2.push_str("</file></xliff>");
    assert!(matches!(xliff().extract(x2.as_bytes(), &opts), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    assert_eq!(xliff().extract(x2.as_bytes(), &ExtractOptions::new()).unwrap().units.len(), 101);
}

/// Thousands of tags in source and target: the target's tags are matched to the source's through a set, and the
/// unit still learns every tag once (source first, then the ones only the target has).
#[test]
fn many_tags_on_both_sides_are_merged_once() {
    let t = 6000;
    let src: String = (0..t).map(|k| format!("<x id=\"s{k}\"/>")).collect();
    let tgt: String = (0..t).map(|k| format!("<x id=\"{}{k}\"/>", if k % 2 == 0 { "s" } else { "t" })).collect();
    let body = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><xliff version=\"1.2\"><file source-language=\"en\" target-language=\"de\" datatype=\"plaintext\" original=\"f\"><body><trans-unit id=\"a\"><source>{src}</source><target>{tgt}</target></trans-unit></body></file></xliff>");
    let doc = Doc::from_bytes(body.clone().into_bytes()).unwrap();
    let ids: Vec<String> = doc.seg(0).raws.iter().map(|r| r.id.clone()).collect();
    let want: Vec<String> = (0..t).map(|k| format!("s{k}")).chain((0..t).filter(|k| k % 2 == 1).map(|k| format!("t{k}"))).collect();
    assert_eq!(ids, want);
}

/// Resource limits (security review run 2): writing a target looks each tag's original markup up once, so a
/// target of tens of thousands of tags is written in linear time, with the markup the source gave each tag.
#[test]
fn a_target_of_many_tags_is_written_in_linear_time() {
    let t = 50_000;
    let src: String = (0..t).map(|k| format!("<x id=\"s{k}\" ctype=\"k{k}\"/>")).collect();
    let body = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><xliff version=\"1.2\"><file source-language=\"en\" target-language=\"de\" datatype=\"plaintext\" original=\"f\"><body><trans-unit id=\"a\"><source>{src}</source></trans-unit></body></file></xliff>");
    let mut doc = Doc::from_bytes(body.into_bytes()).unwrap();
    let source = doc.seg(0).source.to_vec();
    let start = Instant::now();
    assert!(doc.put_target(0, source));
    let out = String::from_utf8(doc.to_bytes()).unwrap();
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
    let target = &out[out.find("<target").unwrap()..];
    assert!(target.contains("<x id=\"s0\" ctype=\"k0\"/>") && target.contains(&format!("<x id=\"s{}\" ctype=\"k{}\"/>", t - 1, t - 1)));
}

/// Resource limits (security review run 2): a second byte order mark used to shift every content range three
/// bytes early (the reader skips it without counting it), so a merge wrote into the start tags. It is refused.
#[test]
fn a_second_byte_order_mark_is_refused() {
    let body = "<xliff version=\"1.2\"><file source-language=\"en\" datatype=\"plaintext\" original=\"f\"><body><trans-unit id=\"a\"><source>Hello</source><target>Hallo</target></trans-unit></body></file></xliff>";
    let one = [&[0xEF, 0xBB, 0xBF][..], body.as_bytes()].concat();
    let two = [&[0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF][..], body.as_bytes()].concat();
    let d = xliff().extract(&one, &ExtractOptions::new()).expect("one mark");
    assert_eq!(d.units[0].segments[0].source, vec![bilingua_model::Inline::text("Hello")]);
    assert!(Doc::from_bytes(two.clone()).is_err());
    assert!(xliff().extract(&two, &ExtractOptions::new()).is_err());
}

/// A source whose content starts with U+FEFF keeps it as text, and its tags keep their own markup (the reader of
/// the content used to drop it and cut every tag three bytes early).
#[test]
fn a_source_that_starts_with_u_feff_keeps_its_tags() {
    let body = "<xliff version=\"1.2\"><file source-language=\"en\" datatype=\"plaintext\" original=\"f\"><body><trans-unit id=\"a\"><source>\u{feff}a<x id=\"1\" ctype=\"image\"/>b</source></trans-unit></body></file></xliff>";
    let mut doc = Doc::from_bytes(body.as_bytes().to_vec()).unwrap();
    let source = doc.seg(0).source.to_vec();
    assert_eq!(source[0], bilingua_model::Inline::text("\u{feff}a"));
    assert!(doc.put_target(0, source));
    let out = String::from_utf8(doc.to_bytes()).unwrap();
    assert!(out.contains(">\u{feff}a<x id=\"1\" ctype=\"image\"/>b</target>"), "{out}");
}

/// BILI-55: a root with XLIFF 2's namespace and no `version` attribute is XLIFF 2 (it was read as 1.2, so its
/// units were not found); an own `version` still decides, and a prefixed `m:version` is not one.
#[test]
fn an_xliff_2_namespace_without_a_version_is_xliff_2() {
    let x2 = "<xliff xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" srcLang=\"en\" trgLang=\"de\"><file id=\"f\"><unit id=\"u1\"><segment><source>Hello</source></segment></unit></file></xliff>";
    assert_eq!(bilingua_xliff::detect(x2.as_bytes()).unwrap(), bilingua_xliff::Format::Xliff2);
    let d = xliff().extract(x2.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.len(), 1);
    let v12 = "<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\"><file><body/></file></xliff>";
    assert_eq!(bilingua_xliff::detect(v12.as_bytes()).unwrap(), bilingua_xliff::Format::Xliff12);
    let none = "<xliff xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:m=\"x\" m:version=\"2.9\"><file><body/></file></xliff>";
    assert_eq!(bilingua_xliff::detect(none.as_bytes()).unwrap(), bilingua_xliff::Format::Xliff12);
    let v2 = "<xliff version=\"2.1\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\"><file id=\"f\"/></xliff>";
    assert_eq!(bilingua_xliff::detect(v2.as_bytes()).unwrap(), bilingua_xliff::Format::Xliff2);
}

/// BILI-59: U+0000 (and the other characters XML forbids) is refused, not passed into the units; under the
/// LocOS profile as well.
#[test]
fn a_nul_character_is_refused() {
    let x = "<xliff version=\"1.2\"><file source-language=\"en\" datatype=\"plaintext\" original=\"f\"><body><trans-unit id=\"a\"><source>He\u{0}llo</source></trans-unit></body></file></xliff>";
    assert!(Doc::from_bytes(x.as_bytes().to_vec()).is_err());
    assert!(xliff().extract(x.as_bytes(), &ExtractOptions::new()).is_err());
    assert!(xliff().extract(x.as_bytes(), &bilingua_model::profile::locos().extract_options()).is_err());
    assert!(xliff().extract(x.replace('\u{0}', "\u{ffff}").as_bytes(), &ExtractOptions::new()).is_err());
    assert!(xliff().extract(x.replace('\u{0}', "\t").as_bytes(), &ExtractOptions::new()).is_ok());
}

/// BILI-39: a GlobalLink segment that was finished and is set back to draft says `in_progress` in its
/// `iws:status`, so it does not read back as translated; accepting it again says `finished`.
#[test]
fn a_finished_globallink_segment_set_back_to_draft_is_no_longer_finished() {
    let gl = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:iws=\"http://www.idiominc.com/ws/asset\"><file source-language=\"en\" target-language=\"de\" datatype=\"x\"><body><trans-unit id=\"g1\"><source>One</source><target>Eins</target><iws:segment-metadata><iws:status translation_status=\"finished\" lock_status=\"unlocked\"/></iws:segment-metadata></trans-unit></body></file></xliff>\n";
    let f = xliff();
    let mut d = f.extract(gl.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(d.find("g1").unwrap().1.state, bilingua_model::SegState::Translated);
    d.set_state("g1", bilingua_model::SegState::Draft).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("translation_status=\"in_progress\" lock_status=\"unlocked\""), "{out}");
    let back = f.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_ne!(back.find("g1").unwrap().1.state, bilingua_model::SegState::Translated);
    // untouched, it stays finished
    let d = f.extract(gl.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(f.merge(&d).unwrap(), gl.as_bytes());
    // edited (a new target) and set back to draft
    let mut d = f.extract(gl.as_bytes(), &ExtractOptions::new()).unwrap();
    d.set_target("g1", vec![bilingua_model::Inline::text("Eins!")]).unwrap();
    d.set_state("g1", bilingua_model::SegState::Draft).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("translation_status=\"in_progress\""), "{out}");
    assert!(!out.contains("translation_status=\"finished\""), "{out}");
}

/// BILI-39: `iws:status` is one per trans-unit. A finished unit of several segments with one of them set back to
/// draft is no longer finished, also while its other segments stay accepted; accepting that segment again (every
/// segment accepted) says `finished` once more.
#[test]
fn a_finished_globallink_unit_with_one_segment_back_at_draft_is_in_progress() {
    let gl = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:iws=\"http://www.idiominc.com/ws/asset\"><file source-language=\"en\" target-language=\"de\" datatype=\"x\"><body><trans-unit id=\"g1\"><source>One. Two.</source><seg-source><mrk mtype=\"seg\" mid=\"1\">One.</mrk> <mrk mtype=\"seg\" mid=\"2\">Two.</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\">Eins.</mrk> <mrk mtype=\"seg\" mid=\"2\">Zwei.</mrk></target><iws:segment-metadata><iws:status translation_status=\"finished\" lock_status=\"unlocked\"/></iws:segment-metadata></trans-unit></body></file></xliff>\n";
    let f = xliff();
    let d = f.extract(gl.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(d.units[0].segments.len(), 2, "{:?}", d.units[0].segments);
    assert!(d.units[0].segments.iter().all(|s| s.state == bilingua_model::SegState::Translated));
    let second = d.units[0].segments[1].id.clone();
    let mut back = d.clone();
    back.set_state(&second, bilingua_model::SegState::Draft).unwrap();
    let out = String::from_utf8(f.merge(&back).unwrap()).unwrap();
    assert!(out.contains("translation_status=\"in_progress\" lock_status=\"unlocked\""), "{out}");
    let again = f.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_ne!(again.units[0].segments[1].state, bilingua_model::SegState::Translated);
    // accepted again: finished
    let mut done = again.clone();
    done.set_state(&second, bilingua_model::SegState::Translated).unwrap();
    let out2 = String::from_utf8(f.merge(&done).unwrap()).unwrap();
    assert!(out2.contains("translation_status=\"finished\" lock_status=\"unlocked\""), "{out2}");
}
