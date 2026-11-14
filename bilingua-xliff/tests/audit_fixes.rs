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
