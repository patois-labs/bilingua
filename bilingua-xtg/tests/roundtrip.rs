//! Round trips over the corpus (eleven languages, every encoding `<eN>` selects): untouched merge is the original
//! bytes, a pseudo-translated merge reads back with the same units and codes, a language that does not fit the
//! legacy code page switches the file to UTF-8.
mod common;
use bilingua_model::*;
use bilingua_xtg::Xtg;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/xtg");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

fn codes_of(u: &Unit) -> Vec<(String, TagKind)> {
    u.source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
            _ => None,
        })
        .collect()
}

#[test]
fn corpus_is_complete() {
    let c = corpus();
    assert!(c.len() >= 15, "{}", c.len());
    for ext in ["xtg", "tag"] {
        assert!(c.iter().any(|(n, _)| n.ends_with(ext)));
    }
}

#[test]
fn untouched_and_echo_merges_are_exact() {
    for (name, data) in corpus() {
        let doc = Xtg.extract(&data, &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(doc.units.len() >= 5, "{name}: {} units", doc.units.len());
        assert_eq!(Xtg.merge(&doc).unwrap(), data, "{name}: untouched");
        assert_identity(&data);
        // a target language changes nothing when nothing is translated
        let mut d2 = doc.clone();
        d2.target_lang = Some("de-DE".into());
        assert_eq!(Xtg.merge(&d2).unwrap(), data, "{name}: target language only");
    }
}

#[test]
fn pseudo_translation_reads_back() {
    for (name, data) in corpus() {
        let mut doc = Xtg.extract(&data, &opts()).unwrap();
        doc.target_lang = Some("fr-FR".into());
        let before: Vec<_> = doc.units.iter().map(codes_of).collect();
        for u in &mut doc.units {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            u.segments[0].target = merge_text(t);
        }
        let out = Xtg.merge(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = Xtg.extract(&out, &opts()).unwrap_or_else(|e| panic!("{name}: reread {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}");
        for (i, u) in back.units.iter().enumerate() {
            assert_eq!(codes_of(u), before[i], "{name}: unit {i} codes");
            let text = plain_text(&u.source());
            let orig = plain_text(&doc.units[i].source());
            assert_eq!(text, format!("[{orig}]"), "{name}: unit {i}");
        }
        // the file keeps its envelope: BOM / UTF-16 / code page unless the text no longer fits
        match (&doc.skeleton, &back.skeleton) {
            (Skeleton::Text { encoding: a, .. }, Skeleton::Text { encoding: b, .. }) => assert_eq!(a, b, "{name}"),
            _ => unreachable!(),
        }
    }
}

#[test]
fn eleven_languages_survive_translation_into_each_other() {
    // every language's heading goes into every file: legacy pages that cannot hold it switch to UTF-8
    let langs = ["Getting started", "Erste Schritte", "Pour commencer", "Primeros pasos", "Per iniziare", "С чего начать", "البدء", "はじめに", "시작하기", "入门", "入門"];
    for (name, data) in corpus() {
        for l in langs {
            let mut doc = Xtg.extract(&data, &opts()).unwrap();
            doc.units[0].segments[0].target = vec![Inline::text(l)];
            let out = Xtg.merge(&doc).unwrap();
            let back = Xtg.extract(&out, &opts()).unwrap_or_else(|e| panic!("{name} <- {l}: {e}"));
            assert_eq!(plain_text(&back.units[0].source()), l, "{name} <- {l}");
            assert_eq!(back.units.len(), doc.units.len());
        }
    }
}

#[test]
fn legacy_file_switches_to_utf8_with_e9() {
    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/xtg/ja_cp932.xtg")).unwrap();
    let mut doc = Xtg.extract(&data, &opts()).unwrap();
    doc.units[0].segments[0].target = vec![Inline::text("مرحبا")];
    let out = Xtg.merge(&doc).unwrap();
    assert!(out.starts_with(b"<v19.00><e9>"), "{:?}", String::from_utf8_lossy(&out[..20]));
    assert!(std::str::from_utf8(&out).is_ok());
}

#[test]
fn newlines_and_bom_are_kept() {
    for nl in ["\r\n", "\n", "\r"] {
        for bom in [false, true] {
            let mut b = if bom { vec![0xEF, 0xBB, 0xBF] } else { vec![] };
            b.extend(format!("<v19.00><e9>{nl}@Body:One <B>two<P>{nl}Three{nl}").as_bytes());
            assert_identity(&b);
            let out = build(&b, &[(1, "Drei")]);
            let s = String::from_utf8(out).unwrap();
            assert!(s.contains(&format!("{nl}Drei{nl}")), "{s:?}");
            assert_eq!(s.starts_with('\u{FEFF}'), bom);
        }
    }
}

#[test]
fn sniff_is_strict() {
    for (name, data) in corpus() {
        // the no-header file is found by extension only
        assert_eq!(Xtg.sniff(&data), !name.contains("noheader"), "{name}");
    }
    for no in [&b"<html><body>hi</body></html>"[..], b"<?xml version=\"1.0\"?><a/>", b"plain text", b"", b"<v>x", b"<B>bold<P>", b"{\"a\":1}"] {
        assert!(!Xtg.sniff(no), "{:?}", String::from_utf8_lossy(no));
    }
    assert!(Xtg.sniff(b"<e0>\r@Normal:x"));
    assert!(Xtg.sniff(b"\xEF\xBB\xBF<v8.00><e1>\r\n"));
}
