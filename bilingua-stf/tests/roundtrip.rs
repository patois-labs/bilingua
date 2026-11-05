//! Round trips on the corpus (11 languages and the encodings), target-locale stamping, plural-free options and the
//! legacy code pages.
mod common;
use bilingua_model::*;
use bilingua_stf::StfFilter;
use common::*;

#[test]
fn every_corpus_file_round_trips_exactly() {
    let files = corpus();
    assert!(files.len() >= 20, "corpus went missing");
    for (name, data) in &files {
        assert_identity(data);
        let doc = extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!doc.units.is_empty(), "{name}: no units");
    }
}

#[test]
fn pseudo_translation_reads_back_with_same_units_and_tags() {
    for (name, data) in corpus() {
        let doc = extract(&data).unwrap();
        let before: Vec<Vec<Inline>> = doc.units.iter().map(|u| u.source()).collect();
        let m: Vec<(usize, String)> = all_with(&data, |t| format!("\u{ab}{t}\u{bb}"));
        for opts in [MergeOptions::default(), MergeOptions::locos()] {
            let out = build_with(&data, &m, Some("de-DE"), &opts);
            let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(back.units.len(), before.len(), "{name}");
            for (b, src) in back.units.iter().zip(&before) {
                let tags = |v: &[Inline]| v.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
                assert_eq!(tags(&b.source()), tags(src), "{name}: tags");
            }
        }
    }
}

#[test]
fn target_locale_is_stamped_in_salesforce_form() {
    for (lang, code) in [("de-DE", "de_DE"), ("zh-Hant", "zh_TW"), ("he-IL", "iw_IL"), ("ar", "ar"), ("ja", "ja"), ("ko", "ko"), ("ru", "ru"), ("fr-CA", "fr_CA")] {
        for (name, data) in corpus().into_iter().filter(|(n, _)| n.starts_with("bilingual_en") || n.starts_with("source_en")) {
            let out = build(&data, &[(0, "x")], Some(lang));
            let want = format!("Language code: {code}");
            assert!(String::from_utf8(out).unwrap().lines().any(|l| l == want), "{name} {lang}");
        }
    }
}

#[test]
fn untouched_merge_does_not_stamp_unless_asked() {
    let data = &corpus().into_iter().find(|(n, _)| n == "bilingual_en.stf").unwrap().1;
    let doc = extract_for(data, Some("de-DE")).unwrap();
    assert_eq!(StfFilter.merge(&doc).unwrap(), *data);
    let stamped = StfFilter.merge_with(&doc, &MergeOptions { stamp_target_always: true, ..Default::default() }).unwrap();
    assert!(String::from_utf8(stamped).unwrap().contains("Language code: de_DE\n"));
}

#[test]
fn keeps_the_encoding_unless_utf8_is_asked() {
    for name in ["bilingual_zh_utf16le.stf", "bilingual_ko_utf16be.stf", "bilingual_de_utf8bom.stf", "bilingual_fr_cp1252.stf"] {
        let data = &corpus().into_iter().find(|(n, _)| n == name).unwrap().1;
        let m = all_with(data, |t| format!("~{t}"));
        let kept = build_with(data, &m, None, &MergeOptions::default());
        assert_eq!(&kept[..2.min(kept.len())], &data[..2.min(data.len())], "{name}: byte-order mark kept");
        assert_eq!(units(&kept).len(), units(data).len(), "{name}");
        let utf8 = build_with(data, &m, None, &MergeOptions { output_encoding: OutputEncoding::Utf8, ..Default::default() });
        assert!(std::str::from_utf8(&utf8).is_ok(), "{name}: UTF-8 out");
        assert_eq!(units(&utf8), units(&kept), "{name}");
    }
}

#[test]
fn legacy_code_pages_follow_the_source_language() {
    // Shift_JIS and Windows-1253 files, read with the project's source language as LocOS ingest does
    for (lang, enc, labels) in [
        ("ja", "shift_jis", "\u{30ed}\u{30b0}\u{30a4}\u{30f3}"),
        ("el", "windows-1253", "\u{395}\u{3af}\u{3c3}\u{3bf}\u{3b4}\u{3bf}\u{3c2}"),
        ("ru", "windows-1251", "\u{412}\u{43e}\u{439}\u{442}\u{438}"),
    ] {
        let text = format!("Language code: {lang}\n# KEY\tLABEL\nCustomLabel.A\t{labels}\n");
        let (bytes, _, _) = bilingua_model::encoding_rs::Encoding::for_label(enc.as_bytes()).unwrap().encode(&text);
        let o = ExtractOptions { source_lang: Some(lang.into()), ..Default::default() };
        let doc = StfFilter.extract(&bytes, &o).unwrap();
        assert_eq!(plain_text(&doc.units[0].source()), labels, "{lang}");
        assert_eq!(StfFilter.merge(&doc).unwrap(), bytes.as_ref(), "{lang}: untouched merge");
        let mut d2 = doc.clone();
        let id = d2.units[0].segments[0].id.clone();
        d2.set_target(&id, vec![Inline::text(labels.to_string() + "!")]).unwrap();
        let out = StfFilter.merge(&d2).unwrap();
        let back = StfFilter.extract(&out, &o).unwrap();
        assert_eq!(plain_text(&back.units[0].source()), format!("{labels}!"), "{lang}");
    }
}


#[test]
fn sniff_is_strict() {
    for (name, data) in corpus() {
        assert!(StfFilter.sniff(&data), "{name}");
    }
    for no in [&b""[..], b"[General]\ntitle = Hi\n", b"key=value\nother=thing\n", b"Language code: de\n", b"# KEY\tLABEL\n", b"hello world"] {
        assert!(!StfFilter.sniff(no), "{:?}", String::from_utf8_lossy(no));
    }
}

#[test]
fn units_carry_context_and_the_existing_translation() {
    let data = &corpus().into_iter().find(|(n, _)| n == "bilingual_de.stf").unwrap().1;
    let doc = extract(data).unwrap();
    let u = &doc.units[1];
    assert_eq!(u.context.key.as_deref(), Some("CustomField.Account.Region.HelpText"));
    assert_eq!(u.context.extra.get("out_of_date").map(String::as_str), Some("true"));
    assert!(u.context.extra.contains_key("translation"));
    assert!(doc.units.iter().all(|u| u.segments[0].target.is_empty()));
}
