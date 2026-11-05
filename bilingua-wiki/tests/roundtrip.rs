//! Round trips over the made-up corpus (`oracle/corpus/wiki`: eleven languages in both dialects, a BOM + CRLF file
//! and Windows-1252 files): an untouched merge gives the original bytes; a pseudo-translated merge reads back with
//! the same units and the same tags; the language of a merge is not stamped (neither wiki declares one).
mod common;
use bilingua_model::*;
use common::*;

fn pseudo(doc: &mut Document) {
    for u in doc.units.iter_mut().filter(|u| u.translate) {
        let mut t = vec![Inline::text("\u{ab}")];
        t.extend(u.source());
        t.push(Inline::text("\u{bb}"));
        u.segments[0].target = merge_text(t);
    }
}

fn shape(doc: &Document) -> Vec<Vec<(String, TagKind)>> {
    doc.units
        .iter()
        .map(|u| {
            u.source()
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn corpus_has_eleven_languages_in_both_dialects() {
    let names: Vec<String> = corpus().into_iter().map(|c| c.0).collect();
    for lang in ["en", "fr", "de", "es", "pt", "ru", "ar", "ja", "zh", "ko", "hi", "it"] {
        for ext in ["wiki", "dokuwiki"] {
            assert!(names.contains(&format!("orders_{lang}.{ext}")), "{lang}.{ext}");
        }
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in corpus() {
        let f = filter_for(&name);
        let doc = extract(f.as_ref(), &data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(f.merge(&doc).unwrap(), data, "{name}");
        assert_identity(f.as_ref(), &data);
    }
}

#[test]
fn pseudo_translated_merge_reads_back_the_same() {
    for (name, data) in corpus() {
        let f = filter_for(&name);
        let mut doc = f.extract(&data, &ExtractOptions { target_lang: Some("de-DE".into()), ..Default::default() }).unwrap();
        let n = doc.units.len();
        pseudo(&mut doc);
        let out = f.merge(&doc).unwrap();
        assert_ne!(out, data, "{name}: nothing was written");
        let back = extract(f.as_ref(), &out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), n, "{name}: unit count");
        assert_eq!(shape(&back), shape(&doc), "{name}: tags");
        // the source text is in the output between the marks
        for u in &doc.units {
            let plain = plain_text(&u.source());
            if !plain.trim().is_empty() {
                let enc = match &doc.skeleton {
                    Skeleton::Text { encoding, .. } => encoding.clone(),
                    _ => unreachable!(),
                };
                let text = enc::decode_text(&out, &enc::DecodeOptions::new(&ExtractOptions::default())).unwrap().text;
                let first = plain.split_whitespace().next().unwrap();
                assert!(text.contains(first), "{name} ({enc}): {first}");
            }
        }
    }
}

#[test]
fn encodings_are_kept() {
    for (name, want) in [("orders_fr_bom_crlf.wiki", "utf-8-bom"), ("orders_fr_cp1252.wiki", "windows-1252"), ("orders_de_cp1252.dokuwiki", "windows-1252"), ("orders_ja.wiki", "utf-8")] {
        let data = corpus().into_iter().find(|c| c.0 == name).unwrap().1;
        let f = filter_for(name);
        let mut doc = extract(f.as_ref(), &data).unwrap();
        assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding.starts_with(want)), "{name}: {:?}", doc.skeleton);
        pseudo(&mut doc);
        let out = f.merge(&doc).unwrap();
        if want == "utf-8-bom" {
            assert!(out.starts_with(b"\xef\xbb\xbf"));
            assert!(out.windows(2).filter(|w| w == b"\r\n").count() > 5 && !String::from_utf8_lossy(&out).replace("\r\n", "").contains('\n'));
        }
        if want == "windows-1252" {
            assert!(std::str::from_utf8(&out).is_err() || out.is_ascii());
        }
    }
    // UTF-16 with a BOM is read and written back as UTF-16
    let src = "Caf\u{e9} au lait is a drink.\n";
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    u16.extend(src.encode_utf16().flat_map(|u| u.to_le_bytes()));
    assert_identity(&mw(), &u16);
    let out = build(&mw(), &u16, &[(0, "Du caf\u{e9}.")]);
    assert_eq!(out[..2], [0xFF, 0xFE]);
}

#[test]
fn ids_are_stable_and_paragraph_mode_works() {
    for (name, data) in corpus() {
        let f = filter_for(&name);
        let a = extract(f.as_ref(), &data).unwrap();
        let b = f.extract(&data, &ExtractOptions { granularity: Granularity::Paragraph, source_lang: Some("en-US".into()), ..Default::default() }).unwrap();
        assert_eq!(a.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), b.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>());
        assert!(a.units.iter().all(|u| u.segments.len() == 1));
        for (i, u) in a.units.iter().enumerate() {
            assert_eq!(u.id, format!("u{}", i + 1));
        }
    }
}

#[test]
fn locked_units_for_text_without_letters() {
    let f = mw();
    let src = b"Text here.\n\n* 123\n* 45 678\n";
    let o = ExtractOptions { include_locked: true, ..Default::default() };
    let doc = f.extract(src, &o).unwrap();
    assert_eq!(doc.units.len(), 3);
    assert!(doc.units[1..].iter().all(|u| !u.translate && u.id.starts_with('k')));
    assert_eq!(f.merge(&doc).unwrap(), src);
    assert_eq!(extract(&f, src).unwrap().units.len(), 1);
}

#[test]
fn context_names_the_unit_kinds_and_headings() {
    let doc = extract(&mw(), MW.as_bytes()).unwrap();
    let kinds: Vec<&str> = doc.units.iter().map(|u| u.context.kind.as_deref().unwrap()).collect();
    assert_eq!(kinds[1], "heading");
    assert_eq!(doc.units[1].context.extra["level"], "2");
    assert_eq!(kinds[6], "term");
    assert_eq!(kinds[7], "definition");
    assert_eq!(doc.units[3].context.heading.as_deref(), Some("Early days <!-- note -->"));
    assert_eq!(doc.units[0].context.location.as_deref(), Some("line 6"));
}

#[test]
fn parser_stamp_is_checked() {
    let mut doc = extract(&mw(), MW.as_bytes()).unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(mw().merge(&doc), Err(Error::ParserVersion { .. })));
}
