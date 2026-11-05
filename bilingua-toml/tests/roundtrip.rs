//! Round trips, the 11 test languages, encodings, and the corpus files.

mod common;
use bilingua_model::*;
use bilingua_toml::TomlFilter;
use common::*;
use std::path::PathBuf;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/toml");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 5);
    v
}

fn tag_shape(d: &Document) -> Vec<Vec<String>> {
    d.units
        .iter()
        .map(|u| {
            u.source()
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { id, kind, .. } => Some(format!("{id}:{kind:?}")),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn untouched_merge_is_exact_for_every_corpus_file() {
    for (name, bytes) in corpus() {
        let d = extract(&bytes);
        assert_eq!(merge(&d), bytes, "{name}");
        // and through serde
        let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(merge(&back), bytes, "{name} (json)");
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_same_units_and_tags() {
    for (name, bytes) in corpus() {
        let mut d = extract(&bytes);
        translate_all(&mut d, |u| {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("] \u{e9}\u{4e2d}\u{5b57}\u{645}"));
            merge_text(t)
        });
        let out = merge(&d);
        toml_value(&out);
        let d2 = extract(&out);
        assert_eq!(d2.units.len(), d.units.len(), "{name}");
        assert_eq!(tag_shape(&d2), tag_shape(&d), "{name}");
        assert_eq!(keys_of(&d2), keys_of(&d), "{name}");
        for (a, b) in d.units.iter().zip(&d2.units) {
            let want = format!("[{}] \u{e9}\u{4e2d}\u{5b57}\u{645}", shown(&a.source()));
            assert_eq!(shown(&b.source()), want, "{name}");
        }
        // target locale: TOML has no language field, so only the values change
        let mut d3 = extract(&bytes);
        d3.target_lang = Some("ar-SA".into());
        translate_all(&mut d3, prefix_de);
        let out3 = merge(&d3);
        assert_eq!(extract(&out3).units.len(), d3.units.len());
    }
}

fn shown(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

fn keys_of(d: &Document) -> Vec<String> {
    bilingua_toml::keys(d)
}

#[test]
fn eleven_languages_extract_and_round_trip() {
    let (_, bytes) = corpus().into_iter().find(|(n, _)| n == "languages.toml").unwrap();
    let d = extract(&bytes);
    let text = |k: &str| {
        let i = bilingua_toml::keys(&d).iter().position(|x| x == k).unwrap_or_else(|| panic!("{k}"));
        plain_text(&d.units[i].source())
    };
    assert!(text("greet.ja").starts_with("ルーメン"));
    assert!(text("greet.zh").starts_with("欢迎"));
    assert!(text("greet.ko").starts_with("루멘"));
    assert!(text("greet.ko").contains('\n'), "multi-line Korean keeps its line break");
    assert!(text("greet.ar").starts_with("مرحبا"));
    assert!(text("greet.fr").contains("Boulangerie"));
    for l in ["en", "es", "fr", "it", "de", "nl", "pt", "ja", "zh", "ko", "ar"] {
        assert!(bilingua_toml::keys(&d).contains(&format!("greet.{l}")), "{l}");
    }
    // translate every language's greeting into every other one and read it back
    let langs = ["es", "fr", "it", "de", "nl", "pt", "ja", "zh", "ko", "ar", "en"];
    for l in langs {
        let mut d = extract(&bytes);
        let sample = text(&format!("greet.{l}"));
        for i in 0..d.units.len() {
            let id = d.units[i].id.clone();
            let src = d.units[i].source();
            let mut t = vec![Inline::text(format!("{sample} | "))];
            t.extend(src);
            d.set_target(&id, merge_text(t)).unwrap();
        }
        let out = merge(&d);
        let v = toml_value(&out);
        assert!(v["greet"]["en"].as_str().unwrap().starts_with(&sample));
        assert!(v["greet"]["ko"].as_str().unwrap().starts_with(&sample));
    }
}

#[test]
fn encodings_other_than_utf8_are_refused() {
    let utf8 = "a = \"caf\u{e9} \u{65e5}\u{672c}\"\n";
    // UTF-16 LE/BE with BOM, Latin-1, Shift-JIS bytes, GBK bytes
    let mut le = vec![0xFF, 0xFE];
    le.extend(utf8.encode_utf16().flat_map(|u| u.to_le_bytes()));
    let mut be = vec![0xFE, 0xFF];
    be.extend(utf8.encode_utf16().flat_map(|u| u.to_be_bytes()));
    let latin1: Vec<u8> = b"a = \"caf\xe9\"\n".to_vec();
    let sjis: Vec<u8> = b"a = \"\x93\xfa\x96\x7b\x8c\xea\"\n".to_vec();
    let gbk: Vec<u8> = b"a = \"\xd6\xd0\xce\xc4\"\n".to_vec();
    for (n, data) in [("utf-16le", le), ("utf-16be", be), ("latin-1", latin1), ("shift-jis", sjis), ("gbk", gbk)] {
        match try_extract(&data) {
            Err(Error::Unsupported(m)) => assert!(m.contains("UTF-8"), "{n}: {m}"),
            other => panic!("{n}: {other:?}"),
        }
    }
    // UTF-8 with a BOM is fine and is written back with its BOM; without one it is not added
    let with = [b"\xef\xbb\xbf".as_slice(), utf8.as_bytes()].concat();
    assert_eq!(merge(&extract(&with)), with);
    assert_eq!(merge(&extract(utf8.as_bytes())), utf8.as_bytes());
}

#[test]
fn style_survives_for_each_kind_and_escapes_are_kept_when_untouched() {
    let (_, bytes) = corpus().into_iter().find(|(n, _)| n == "edge.toml").unwrap();
    let mut d = extract(&bytes);
    // Touch only one unit: every other byte of the file stays.
    let k = bilingua_toml::keys(&d).iter().position(|x| x == "single").unwrap();
    set(&mut d, k, "Er sagte \"hallo\"");
    let out = String::from_utf8(merge(&d)).unwrap();
    let orig = String::from_utf8(bytes).unwrap();
    assert!(out.contains("single = 'Er sagte \"hallo\"'"), "literal style kept");
    assert_eq!(out.replace("single = 'Er sagte \"hallo\"'", "single = 'It said \"hi\"'"), orig);
}

#[test]
fn paragraph_mode_gives_one_segment_per_unit() {
    let d = TomlFilter.extract(include_bytes!("data/hugo.toml"), &ExtractOptions { granularity: Granularity::Paragraph, ..Default::default() }).unwrap();
    assert!(d.units.iter().all(|u| u.segments.len() == 1));
    assert_eq!(d.granularity, Granularity::Paragraph);
}

#[test]
fn include_locked_adds_context_units_only_when_asked() {
    let src = b"id = \"abc\"\nname = \"Name\"\ndash = \"---\"\nblank = \"\"\n";
    assert_eq!(extract(src).units.len(), 1);
    let d = TomlFilter.extract(src, &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    let flags: Vec<bool> = d.units.iter().map(|u| u.translate).collect();
    assert_eq!(flags, vec![false, true, false]);
    assert_eq!(merge(&d), src);
}

#[test]
fn parser_stamp_is_checked() {
    let mut d = extract(b"a = \"Hello\"\n");
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(TomlFilter.merge(&d), Err(Error::ParserVersion { .. })));
}
