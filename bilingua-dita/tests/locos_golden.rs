//! Differential test against the LocOS `dita` and `docbook` filters: 409 documents (the LocOS fixtures, the oracle
//! corpus, hand-written edge cases (19 from the gate) and 370 random ones: DITA topics of six types, maps, bookmaps, DocBook 4.5 and 5,
//! roots neither vocabulary names), with the units, styles, notes and the bytes LocOS builds for two mappings, in
//! place and for three target languages, recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).

mod common;

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
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

fn build(f: &dyn Filter, data: &[u8], target: Option<&str>, kind: &str) -> Result<Vec<u8>, Error> {
    let mut doc = f.extract(data, &opts(target))?;
    for (i, t) in mapping(kind, &doc.clone()) {
        let (_, map) = to_locos_text(&doc.units[i].source());
        doc.units[i].segments[0].target = from_locos_text(&t, &map);
    }
    f.merge(&doc)
}

#[test]
fn matches_locos_dita_and_docbook_filters() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 409);
    let (dita, docbook) = (Dita::new(), DocBook::new());
    let (mut units_ok, mut unit_count, mut builds_ok, mut both_refuse, mut only_locos_refuses) = (0usize, 0usize, 0usize, 0usize, Vec::<String>::new());
    let mut fails = Vec::<String>::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let f: &dyn Filter = if c["filter"] == "docbook" { &docbook } else { &dita };
        let data = bytes(&c["data"]);
        let doc = match f.extract(&data, &opts(None)) {
            Ok(d) => d,
            Err(e) => {
                assert!(c.get("error").is_some(), "{name}: LocOS reads it, we refuse: {e}");
                both_refuse += 1;
                continue;
            }
        };
        if let Some(err) = c.get("error") {
            // known difference: lxml refuses a repeated xml:id (an ID validity rule, not well-formedness)
            let err = err.as_str().unwrap();
            // and LocOS refuses a DTD internal subset, which Bilingua keeps (no entity is expanded)
            assert!(err.contains("already defined") || err.contains("already de") || err.contains("internal subset"), "{name}: LocOS refuses ({err}), we read it");
            only_locos_refuses.push(name.to_string());
            continue;
        }
        let want: Vec<(String, String, String)> = c["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| (u["text"].as_str().unwrap().into(), u["style"].as_str().unwrap().into(), u["note"].as_str().unwrap().into()))
            .collect();
        let got: Vec<(String, String, String)> =
            doc.units.iter().map(|u| (show(u), u.context.style.clone().unwrap_or_default(), u.notes.first().map(|n| n.text.clone()).unwrap_or_default())).collect();
        // known difference (issue 809.3, fixed in Bilingua first): no attribute unit under translate="no" (or on a
        // conref element), so LocOS's extra attribute units are not ours; the builds are not comparable then
        let known_809 = got != want && {
            let mut it = want.iter().peekable();
            let mut ok = true;
            for g in &got {
                while it.peek().is_some_and(|w| *w != g && (w.1.starts_with("attr:") || w.1.starts_with("a11y:"))) {
                    it.next();
                }
                if it.next() != Some(g) {
                    ok = false;
                    break;
                }
            }
            ok && it.all(|w| w.1.starts_with("attr:") || w.1.starts_with("a11y:"))
        };
        if got != want && !known_809 {
            fails.push(format!("{name}: units differ\n  ours  {got:?}\n  locos {want:?}"));
            continue;
        }
        units_ok += 1;
        unit_count += got.len();
        if known_809 {
            continue;
        }
        for (key, golden) in c["builds"].as_object().unwrap() {
            let (kind, target) = match key.split_once('@') {
                Some((k, t)) => (k, Some(t)),
                None => (key.as_str(), None),
            };
            let mut want = bytes(golden);
            if name == "gate_db_prefix_decl.dbk" && target.is_some() {
                // known difference (a LocOS bug): a DocBook 4.x root (no namespace) that merely declares the DocBook
                // 5 namespace for a prefix gets `xml:lang` from LocOS, which tests for the namespace URI anywhere in
                // the start tag; the root is outside the namespace, so Bilingua writes 4.x's `lang`
                want = String::from_utf8(want).unwrap().replace(" xml:lang=\"", " lang=\"").into_bytes();
            }
            if doc.units.is_empty() && target.is_some() {
                // known difference: LocOS stamps the target language even when nothing was translated; a Bilingua
                // merge with nothing translated is the original bytes (the LocOS profile stamps, see locos_its.rs)
                assert_eq!(build(f, &data, target, kind).unwrap(), data, "{name} [{key}]");
                builds_ok += 1;
                continue;
            }
            match build(f, &data, target, kind) {
                // known difference (issue 809.2, fixed in Bilingua first): a nested element that still declares the source
                // language is restamped; only the root's language is compared here (locos_its.rs tests the rest)
                Ok(b) if b == want || (target.is_some() && inner_lang_blanked(&b) == inner_lang_blanked(&want)) => builds_ok += 1,
                Ok(b) => fails.push(format!("{name} [{key}]: bytes differ\n  ours  {:?}\n  locos {:?}", String::from_utf8_lossy(&b), String::from_utf8_lossy(&want))),
                Err(e) => fails.push(format!("{name} [{key}]: we fail: {e}")),
            }
        }
    }
    for x in fails.iter().take(12) {
        eprintln!("{x}");
    }
    assert!(fails.is_empty(), "{} differences ({units_ok} unit lists and {builds_ok} builds agree)", fails.len());
    eprintln!(
        "{units_ok} unit lists ({unit_count} units) and {builds_ok} builds agree with LocOS; {both_refuse} refused by both; {} read by us only (repeated xml:id): {:?}",
        only_locos_refuses.len(),
        only_locos_refuses
    );
    assert!(units_ok >= 355, "{units_ok}");
}

/// The text with every `lang="..."` value after the first one blanked.
fn inner_lang_blanked(b: &[u8]) -> String {
    let s = String::from_utf8_lossy(b).into_owned();
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    let mut first = true;
    while let Some(i) = rest.find("lang=\"") {
        let v = i + 6;
        out.push_str(&rest[..v]);
        let end = rest[v..].find('"').map_or(rest.len(), |e| v + e);
        if first {
            out.push_str(&rest[v..end]);
            first = false;
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}
