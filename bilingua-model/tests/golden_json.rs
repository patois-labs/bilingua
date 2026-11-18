//! Golden JSON (unit model step 1): Documents written by main load and write back to the same bytes, before any
//! storage change. Pins rule 1 (old JSON loads) and rule 2 (no new information, same bytes) of the unit model
//! design (`docs/DESIGN-BILINGUA-UNIT-MODEL-2026-11-17.md`, section 4).
//!
//! `tests/golden/` holds, all made by main `626e6f04` on the M5 (see `tests/golden/README.md`):
//! - CLI skeletons (`*.skl.json`, `{"bilingua_cli":1,"document":{..}}`) of corpus files: the three bench formats
//!   (docx, xlsx, pptx) and po, arb, xcstrings, json;
//! - a ChatterCAT `state.json` (`chattercat_docx.state.json`: the server's extract options, four segments edited,
//!   `serde_json::to_vec` as `persist` writes it);
//! - two `state.json` files stored by the old server image 5620cb3e (Bilingua before 0.9, `~` codes), with what
//!   main writes after reading them (`*.state.resaved.json`).

use bilingua_model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The CLI skeleton shape, fields in the CLI's order.
#[derive(Serialize, Deserialize)]
struct Skeleton {
    bilingua_cli: u32,
    document: Document,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn files(suffix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir()).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().ends_with(suffix)).collect();
    v.sort();
    v
}

fn name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

/// Every document the fixtures hold, with the bytes main wrote for it.
fn documents() -> Vec<(String, Document, Vec<u8>)> {
    let mut out = vec![];
    for p in files(".skl.json") {
        let raw = std::fs::read(&p).unwrap();
        let s: Skeleton = serde_json::from_slice(&raw).unwrap();
        // the document's own bytes, as main wrote them inside the wrapper
        let head = br#"{"bilingua_cli":1,"document":"#;
        assert!(raw.starts_with(head) && raw.ends_with(b"}"), "{}", name(&p));
        out.push((name(&p), s.document, raw[head.len()..raw.len() - 1].to_vec()));
    }
    for p in files(".state.json").into_iter().filter(|p| !name(p).starts_with("old_")).chain(files(".state.resaved.json")) {
        let raw = std::fs::read(&p).unwrap();
        out.push((name(&p), serde_json::from_slice(&raw).unwrap(), raw));
    }
    out
}

#[test]
fn the_fixtures_are_there() {
    assert_eq!(files(".skl.json").len(), 7);
    assert_eq!(files(".state.json").len(), 3);
    assert_eq!(files(".state.resaved.json").len(), 2);
    // what the fixtures exercise: writer-private codes, extras, hidden values and a target
    let all: Vec<String> = documents().into_iter().map(|(_, _, raw)| String::from_utf8(raw).unwrap()).collect();
    for needle in [r#""_o":{"#, r#""_base":{"#, r#""extra":{"cells""#, r#""hidden":{"#, r#""state":"confirmed""#, r#""label":"bold""#] {
        assert!(all.iter().any(|j| j.contains(needle)), "no fixture has {needle}");
    }
}

#[test]
fn cli_skeletons_from_main_write_back_the_same_bytes() {
    for p in files(".skl.json") {
        let raw = std::fs::read(&p).unwrap();
        let s: Skeleton = serde_json::from_slice(&raw).unwrap();
        assert!(!s.document.units.is_empty(), "{}", name(&p));
        assert_eq!(String::from_utf8(serde_json::to_vec(&s).unwrap()).unwrap(), String::from_utf8(raw).unwrap(), "{}", name(&p));
    }
}

#[test]
fn chattercat_state_from_main_writes_back_the_same_bytes() {
    let raw = std::fs::read(dir().join("chattercat_docx.state.json")).unwrap();
    let doc: Document = serde_json::from_slice(&raw).unwrap();
    assert!(doc.segments().any(|(_, s)| s.state == SegState::Confirmed && s.has_target()));
    assert_eq!(String::from_utf8(serde_json::to_vec(&doc).unwrap()).unwrap(), String::from_utf8(raw).unwrap());
}

#[test]
fn old_server_state_loads_as_main_reads_it() {
    for p in files(".state.json").into_iter().filter(|p| name(p).starts_with("old_")) {
        let raw = std::fs::read(&p).unwrap();
        let doc: Document = serde_json::from_slice(&raw).unwrap();
        let expected = std::fs::read(p.with_file_name(name(&p).replace(".state.json", ".state.resaved.json"))).unwrap();
        assert_eq!(String::from_utf8(serde_json::to_vec(&doc).unwrap()).unwrap(), String::from_utf8(expected).unwrap(), "{}", name(&p));
    }
    // the old `~` codes moved into `hidden` on the way
    let po = std::fs::read_to_string(dir().join("old_po.state.json")).unwrap();
    let resaved = std::fs::read_to_string(dir().join("old_po.state.resaved.json")).unwrap();
    assert!(po.contains("\"~") && !resaved.contains("\"~") && resaved.contains("\"hidden\":{"));
}

#[test]
fn documents_survive_a_trip_through_a_json_value() {
    // a self-describing tree, as a dict from Python is: serde_json's Value (no `preserve_order`) holds its keys
    // sorted, so struct fields arrive in another order than the bytes have them; the document must not care
    for (n, doc, raw) in documents() {
        let v = serde_json::to_value(&doc).unwrap();
        let back: Document = serde_json::from_value(v).unwrap();
        assert_eq!(back, doc, "{n}");
        assert_eq!(serde_json::to_vec(&back).unwrap(), raw, "{n}");
    }
}

#[test]
fn the_compact_maps_write_the_same_json_as_the_fields() {
    // since step 3 the fields are the compact maps: each must write the same JSON as the BTreeMap it replaced
    let (mut units, mut codes, mut extras, mut hidden) = (0, 0, 0, 0);
    for (n, doc, _) in documents() {
        for u in &doc.units {
            units += 1;
            let field = serde_json::to_string(&u.codes).unwrap();
            assert_eq!(serde_json::to_string(&BTreeMap::<String, CodeData>::from(u.codes.clone())).unwrap(), field, "{n} {}", u.id);
            assert_eq!(serde_json::to_string(&serde_json::from_str::<Codes>(&field).unwrap()).unwrap(), field, "{n} {}", u.id);
            assert_eq!(serde_json::to_string(u.codes()).unwrap(), field);
            codes += u.codes.len();
            for (m, count) in [(&u.context.extra, &mut extras), (&u.hidden, &mut hidden)] {
                let field = serde_json::to_string(m).unwrap();
                assert_eq!(serde_json::to_string(&BTreeMap::<String, String>::from(m.clone())).unwrap(), field, "{n} {}", u.id);
                assert_eq!(serde_json::to_string(&serde_json::from_str::<TextMap>(&field).unwrap()).unwrap(), field, "{n} {}", u.id);
                *count += m.len();
            }
        }
    }
    assert!(units > 100 && codes > 100 && extras > 40 && hidden > 0, "{units} {codes} {extras} {hidden}");
}

#[test]
fn rebuilding_every_map_through_the_accessors_writes_the_same_bytes() {
    for (n, mut doc, raw) in documents() {
        for u in &mut doc.units {
            let codes = Codes::from(u.codes());
            u.set_codes(Codes::new());
            u.set_codes(codes.iter().map(|(k, c)| (k, c.clone())).collect::<Codes>());
            let hidden = TextMap::from(u.hidden_map());
            u.set_hidden_map(hidden);
            let extra: TextMap = u.context.extra().iter().collect();
            u.context.set_extra(extra);
        }
        assert_eq!(serde_json::to_vec(&doc).unwrap(), raw, "{n}");
    }
}

#[test]
fn maps_written_out_of_order_load_and_write_sorted() {
    // one hand-written object per map type, keys out of order and one repeated: it loads, last wins, and writes
    // back sorted, exactly as the BTreeMap fields do
    let json = r#"{"id":"u1","segments":[{"id":"u1","source":[{"text":"x"}]}],
        "codes":{"2":{"open":null,"close":null,"standalone":"<b/>","label":null},"10":{"open":"<i>","close":"</i>","standalone":null,"label":"italic"},"1":{"open":null,"close":null,"standalone":"<x/>","label":null},"2":{"open":null,"close":null,"standalone":"<br/>","label":null}},
        "context":{"kind":null,"style":null,"key":null,"heading":null,"location":null,"max_len":null,"extra":{"slide":"3","part":"a.xml","cells":"A1","part":"b.xml"}},
        "hidden":{"raw":"r","lead":" ","raw":"r2"}}"#;
    let u: Unit = serde_json::from_str(json).unwrap();
    let out = serde_json::to_string(&u).unwrap();
    assert!(out.contains(r#""codes":{"1":{"open":null,"close":null,"standalone":"<x/>","label":null},"10":{"open":"<i>","close":"</i>","standalone":null,"label":"italic"},"2":{"open":null,"close":null,"standalone":"<br/>","label":null}}"#), "{out}");
    assert!(out.contains(r#""extra":{"cells":"A1","part":"b.xml","slide":"3"}"#), "{out}");
    assert!(out.contains(r#""hidden":{"lead":" ","raw":"r2"}"#), "{out}");
    // the compact maps read the same objects the same way
    let raw = r#"{"2":{"open":null,"close":null,"standalone":"<b/>","label":null},"10":{"open":"<i>","close":"</i>","standalone":null,"label":"italic"},"1":{"open":null,"close":null,"standalone":"<x/>","label":null},"2":{"open":null,"close":null,"standalone":"<br/>","label":null}}"#;
    let c: Codes = serde_json::from_str(raw).unwrap();
    assert_eq!(serde_json::to_string(&c).unwrap(), serde_json::to_string(&u.codes).unwrap());
    for (raw, field) in [(r#"{"slide":"3","part":"a.xml","cells":"A1","part":"b.xml"}"#, &u.context.extra), (r#"{"raw":"r","lead":" ","raw":"r2"}"#, &u.hidden)] {
        let t: TextMap = serde_json::from_str(raw).unwrap();
        assert_eq!(serde_json::to_string(&t).unwrap(), serde_json::to_string(field).unwrap());
    }
}

/// Main-built bench skeletons are too big for the repository (77 to 214 MB). On the M5:
/// `M5_TEST_ARGS="-p bilingua-model --test golden_json -- --include-ignored"` with `BILINGUA_GOLDEN_DIR` set, or
/// `~/bili/memb/base` (the memb bench skeletons of the base binary) when it is not.
#[test]
#[ignore = "reads the memb bench skeletons on the M5"]
fn bench_skeletons_write_back_the_same_bytes() {
    let d = std::env::var_os("BILINGUA_GOLDEN_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").expect("HOME")).join("bili/memb/base"));
    let mut seen = 0;
    for e in std::fs::read_dir(&d).unwrap() {
        let p = e.unwrap().path();
        if !p.to_string_lossy().ends_with(".skl.json") {
            continue;
        }
        let raw = std::fs::read(&p).unwrap();
        let s: Skeleton = serde_json::from_slice(&raw).unwrap();
        let out = serde_json::to_vec(&s).unwrap();
        assert!(out == raw, "{} differs", p.display());
        for u in &s.document.units {
            assert_eq!(serde_json::to_string(&BTreeMap::<String, CodeData>::from(u.codes.clone())).unwrap(), serde_json::to_string(&u.codes).unwrap());
            assert_eq!(serde_json::to_string(&BTreeMap::<String, String>::from(u.context.extra.clone())).unwrap(), serde_json::to_string(&u.context.extra).unwrap());
        }
        seen += 1;
    }
    assert!(seen > 0, "no skeletons in {}", d.display());
}
