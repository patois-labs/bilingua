//! Differential test against the LocOS generic XML filter: 383 documents (the oracle corpus, the LocOS fixtures and
//! 360 random ones: plain XML with ITS rules and local ITS markup, DITA, DocBook 4.5 and 5, multilingual layouts),
//! with the units, styles, notes and the bytes LocOS builds recorded in `data/locos_golden.json`
//! (made by `tools/gen_golden.py`).

mod common;

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_xml::XmlFilter;
use common::*;

fn bytes(v: &serde_json::Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    match s.split_at(2) {
        ("t:", t) => t.as_bytes().to_vec(),
        ("h:", h) => (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect(),
        _ => panic!("bad golden value"),
    }
}

fn mapping(kind: &str, doc: &Document) -> Vec<(usize, String)> {
    doc.units
        .iter()
        .enumerate()
        .filter_map(|(i, u)| {
            let t = to_locos_text(&u.source()).0;
            match kind {
                "all" => Some((i, format!("T{i} \u{e9} {} <&>", t.replace('a', "\u{e0}")))),
                "third" if i % 3 == 0 => Some((i, format!("Zeta {i} \u{65e5}"))),
                _ => None,
            }
        })
        .collect()
}

/// LocOS builds are UTF-8 with the declaration rewritten; Bilingua writes the file back in its original encoding
/// (decision B-bytes). Read our bytes in that encoding and put the UTF-8 label back, to compare like with like.
fn as_locos(b: Vec<u8>) -> Vec<u8> {
    use bilingua_model::encoding_rs::{Encoding, UTF_16LE};
    if b.starts_with(&[0xFF, 0xFE]) {
        let (t, _) = UTF_16LE.decode_without_bom_handling(&b[2..]);
        let t = t.replace("encoding=\"UTF-16\"", "encoding=\"UTF-8\"");
        return t.into_bytes();
    }
    let head = String::from_utf8_lossy(&b[..b.len().min(200)]).to_string();
    if let Some(i) = head.find("encoding=\"") {
        let rest = &head[i + 10..];
        if let Some(j) = rest.find('"') {
            let label = &rest[..j];
            if !label.eq_ignore_ascii_case("utf-8") {
                if let Some(enc) = Encoding::for_label(label.as_bytes()) {
                    let (t, _, _) = enc.decode(&b);
                    return t.replacen(&format!("encoding=\"{label}\""), "encoding=\"UTF-8\"", 1).into_bytes();
                }
            }
        }
    }
    b
}

fn build(f: &XmlFilter, data: &[u8], target: Option<&str>, kind: &str) -> Result<Vec<u8>, Error> {
    let mut doc = f.extract(data, &opts(target))?;
    for (i, t) in mapping(kind, &doc.clone()) {
        let (_, map) = to_locos_text(&doc.units[i].source());
        doc.units[i].segments[0].target = from_locos_text(&t, &map);
    }
    f.merge(&doc)
}

#[test]
fn matches_locos_on_corpus_and_random_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 380);
    let f = XmlFilter::new();
    let (mut units_ok, mut builds_ok, mut fails) = (0usize, 0usize, Vec::<String>::new());
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = bytes(&c["data"]);
        let doc = match f.extract(&data, &opts(None)) {
            Ok(d) => d,
            Err(e) => {
                assert!(c.get("error").is_some(), "{name}: LocOS reads it, we refuse: {e}");
                continue;
            }
        };
        let want = c["units"].as_array().unwrap();
        let got: Vec<(String, String, String)> = doc
            .units
            .iter()
            .map(|u| (show(u), u.context.style.clone().unwrap_or_default(), u.notes.first().map(|n| n.text.clone()).unwrap_or_default()))
            .collect();
        let want_t: Vec<(String, String, String)> = want
            .iter()
            .map(|u| (u["text"].as_str().unwrap().into(), u["style"].as_str().unwrap().into(), u["note"].as_str().unwrap().into()))
            .collect();
        if got != want_t {
            fails.push(format!("{name}: units differ\n  ours  {got:?}\n  locos {want_t:?}"));
            continue;
        }
        let dialect = f.inspect(&data).unwrap().dialect;
        let want_dialect = c["dialect"].as_str().unwrap();
        assert_eq!(dialect.as_deref().unwrap_or("its"), want_dialect, "{name}: dialect");
        units_ok += 1;
        for (key, golden) in c["builds"].as_object().unwrap() {
            let (kind, target) = match key.split_once('@') {
                Some((k, t)) => (k, Some(t)),
                None => (key.as_str(), None),
            };
            let want = bytes(golden);
            if doc.units.is_empty() && target.is_some() {
                // known difference: LocOS stamps the target language even when nothing was translated; a
                // Bilingua merge with nothing translated is the original bytes
                assert_eq!(build(&f, &data, target, kind).unwrap(), data, "{name} [{key}]");
                builds_ok += 1;
                continue;
            }
            let got = build(&f, &data, target, kind);
            match got {
                Ok(b) if as_locos(b.clone()) == want => builds_ok += 1,
                Ok(b) => fails.push(format!("{name} [{key}]: bytes differ\n  ours  {:?}\n  locos {:?}", String::from_utf8_lossy(&b), String::from_utf8_lossy(&want))),
                Err(e) => fails.push(format!("{name} [{key}]: we fail: {e}")),
            }
        }
    }
    for x in fails.iter().take(12) {
        eprintln!("{x}");
    }
    assert!(fails.is_empty(), "{} differences ({} unit lists and {} builds agree)", fails.len(), units_ok, builds_ok);
    eprintln!("{units_ok} unit lists and {builds_ok} builds agree with LocOS");
}
