//! The D27 model contract on this format's own corpus (the facade's `contract.rs` runs the same checks over all
//! formats once the lead wires the facade): paragraph vs sentence mode, stable ids, parser stamp, one LocOS
//! token per tag.
mod common;
use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_seg::{segment, Rules};
use bilingua_strings::Strings;
use common::*;
use std::path::PathBuf;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/strings");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|f| (f.file_name().to_string_lossy().to_string(), std::fs::read(f.path()).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 13);
    v
}

fn open(bytes: &[u8], mode: Granularity) -> Document {
    let mut d = Strings.extract(bytes, &ExtractOptions { source_lang: Some("en".into()), target_lang: Some("es-ES".into()), granularity: mode, include_locked: true, ..Default::default() }).unwrap();
    segment(&mut d, "en", mode, Rules::bundled());
    d
}

#[test]
fn paragraph_and_sentence_modes() {
    for (name, bytes) in corpus() {
        let sent = open(&bytes, Granularity::Sentence);
        let para = open(&bytes, Granularity::Paragraph);
        let ids = |d: &Document| d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&sent), ids(&para), "{name}");
        for u in &para.units {
            assert_eq!(u.segments.len(), 1, "{name}");
            assert_eq!(u.segments[0].id, u.id, "{name}");
            assert!(u.separators.is_empty());
        }
        for (a, b) in sent.units.iter().zip(&para.units) {
            assert_eq!(a.source(), b.source(), "{name}");
        }
        assert_eq!(Strings.merge(&para).unwrap(), bytes, "{name}");
        assert_eq!(Strings.merge(&sent).unwrap(), bytes, "{name} sentence mode, untouched");
        let mut re = sent.clone();
        segment(&mut re, "en", Granularity::Paragraph, Rules::bundled());
        assert!(re.units.iter().all(|u| u.segments.len() == 1), "{name}");
        assert_eq!(Strings.merge(&re).unwrap(), bytes);
    }
}

#[test]
fn sentence_mode_translation_joins_back() {
    let src = b"\"k\" = \"First one. Second one with %@. Third.\";\n";
    let mut d = open(src, Granularity::Sentence);
    assert_eq!(d.units[0].segments.len(), 3);
    let ids: Vec<String> = d.units[0].segments.iter().map(|s| s.id.clone()).collect();
    for (i, id) in ids.iter().enumerate() {
        let u = &d.units[0];
        let seg = u.segments.iter().find(|s| &s.id == id).unwrap();
        let mut t = vec![Inline::text(format!("<{i}>"))];
        t.extend(seg.source.iter().cloned());
        d.set_target(id, t).unwrap();
    }
    let out = String::from_utf8(Strings.merge(&d).unwrap()).unwrap();
    assert_eq!(out, "\"k\" = \"<0>First one. <1>Second one with %@. <2>Third.\";\n");
}

#[test]
fn ids_stamp_and_tokens() {
    for (name, bytes) in corpus() {
        let a = open(&bytes, Granularity::Paragraph);
        let b = open(&bytes, Granularity::Paragraph);
        assert_eq!(a, b, "{name}: extract is deterministic");
        let mut seen = std::collections::HashSet::new();
        assert!(a.units.iter().all(|u| seen.insert(u.id.clone())), "{name}: ids unique");
        let p = a.skeleton.parser().expect("parser stamp");
        assert_eq!((p.name.as_str(), p.version.as_str(), p.schema), ("bilingua-strings", env!("CARGO_PKG_VERSION"), 1), "{name}");
        let mut newer = a.clone();
        if let Skeleton::Text { parser, .. } = &mut newer.skeleton {
            parser.as_mut().unwrap().schema = 2;
        }
        assert!(matches!(Strings.merge(&newer), Err(Error::ParserVersion { .. })));
        for (_, s) in a.segments() {
            let want = merge_text(s.source.clone());
            let tags = want.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            let (text, map) = to_locos_text(&want);
            assert_eq!(text.matches('\u{27E6}').count(), tags);
            assert_eq!(from_locos_text(&text, &map), want, "{name} {}", s.id);
            let free = want.iter().filter(|i| i.is_free()).count();
            assert_eq!(text.matches("\u{27E6}p").count(), free);
        }
    }
    // Unit ids count every entry, so they are the same whether or not an entry is locked.
    let b = b"a = \"---\";\nb = \"Beta\";\n";
    assert_eq!(ex(b).units[0].id, "s2");
    let locked = Strings.extract(b, &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    assert_eq!(locked.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["s1", "s2"]);
    assert!(!locked.units[0].translate && locked.units[1].translate);
}
