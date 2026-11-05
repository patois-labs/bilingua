//! Round trips on the oracle corpus (11 languages, BOM, CRLF, UTF-16, Windows-1252) and on the legacy encodings
//! LocOS reads for a source language (Shift_JIS, GBK, EUC-KR, Windows-1251, 1254, 1256).
mod common;
use bilingua_model::*;
use bilingua_php::Php;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(p).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 20, "corpus went missing");
    v
}

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn open(bytes: &[u8], lang: Option<&str>, g: Granularity) -> Document {
    Php.extract(bytes, &ExtractOptions { source_lang: lang.map(Into::into), target_lang: Some("de".into()), granularity: g, ..Default::default() })
        .unwrap()
}

#[test]
fn corpus_untouched_merge_is_exact() {
    for (name, bytes) in corpus() {
        for g in [Granularity::Sentence, Granularity::Paragraph] {
            let doc = open(&bytes, None, g);
            assert!(!doc.units.is_empty(), "{name}");
            assert_eq!(Php.merge(&doc).unwrap(), bytes, "{name}");
        }
        // an echo (every target = its source) is also untouched
        let mut doc = open(&bytes, None, Granularity::Paragraph);
        for u in doc.units.iter_mut().filter(|u| u.translate) {
            u.segments[0].target = u.segments[0].source.clone();
        }
        assert_eq!(Php.merge(&doc).unwrap(), bytes, "{name} echo");
        // a target language changes nothing: PHP arrays carry no language
        let doc = Php.extract(&bytes, &ExtractOptions { target_lang: Some("fr-CA".into()), ..Default::default() }).unwrap();
        assert_eq!(Php.merge(&doc).unwrap(), bytes, "{name} target lang");
    }
}

#[test]
fn corpus_pseudo_translation_reads_back() {
    for (name, bytes) in corpus() {
        let mut doc = open(&bytes, None, Granularity::Paragraph);
        let (n, tags) = (doc.units.len(), tag_count(&doc));
        let before: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
        for u in doc.units.iter_mut().filter(|u| u.translate) {
            let mut t: Vec<Inline> = vec![Inline::text("\u{ab}")];
            t.extend(u.segments[0].source.clone());
            t.push(Inline::text("\u{bb} \"q\" 'q' $x \\ \u{65e5}\u{672c}\n"));
            u.segments[0].target = t;
        }
        let out = Php.merge(&doc).unwrap();
        let again = open(&out, None, Granularity::Paragraph);
        assert_eq!(again.units.len(), n, "{name}: unit count");
        assert_eq!(tag_count(&again), tags, "{name}: tags");
        for (a, b) in again.units.iter().zip(&before) {
            // a raw line break in a single-quoted string follows the file's line ends (CRLF stays CRLF)
            assert_eq!(plain_text(&a.source()).replace("\r\n", "\n"), format!("\u{ab}{b}\u{bb} \"q\" 'q' $x \\ \u{65e5}\u{672c}\n"), "{name}");
        }
        // the encoding family is kept: BOM, UTF-16, CRLF
        for bom in [&[0xEF, 0xBB, 0xBF][..], &[0xFF, 0xFE], &[0xFE, 0xFF]] {
            assert_eq!(out.starts_with(bom), bytes.starts_with(bom), "{name}: bom");
        }
        if bytes.windows(2).any(|w| w == b"\r\n") {
            assert!(out.windows(2).any(|w| w == b"\r\n"), "{name}: crlf");
        }
    }
}

#[test]
fn eleven_languages_present() {
    let names: Vec<String> = corpus().into_iter().map(|c| c.0).collect();
    for l in ["en", "de", "fr", "es", "pt", "ru", "tr", "ar", "ja", "zh", "ko"] {
        assert!(names.contains(&format!("lang_{l}.php")), "{l}");
    }
    // each language file has the same units: 4 strings + buttons + label + legacy
    let counts: Vec<usize> = corpus().iter().filter(|(n, _)| n.starts_with("lang_") && n.ends_with(".php") && n.len() == 11).map(|(_, b)| extract(b).unwrap().units.len()).collect();
    assert!(counts.len() == 11 && counts.iter().all(|c| *c == counts[0]), "{counts:?}");
}

#[test]
fn legacy_encodings_with_source_language() {
    let src = |s: &str| format!("<?php\nreturn [\n    'a' => '{s}',\n    'b' => \"{s} 2\",\n];\n");
    let cases: [(&str, &str, &'static bilingua_model::encoding_rs::Encoding, &str); 6] = [
        ("ja", "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}", encoding_rs::SHIFT_JIS, "\u{7ffb}\u{8a33}"),
        ("zh", "\u{7b80}\u{4f53}\u{4e2d}\u{6587}\u{6587}\u{672c}", encoding_rs::GBK, "\u{7ffb}\u{8bd1}"),
        ("ko", "\u{d55c}\u{ad6d}\u{c5b4} \u{d14d}\u{c2a4}\u{d2b8}", encoding_rs::EUC_KR, "\u{bc88}\u{c5ed}"),
        ("ru", "\u{420}\u{443}\u{441}\u{441}\u{43a}\u{438}\u{439} \u{442}\u{435}\u{43a}\u{441}\u{442}", encoding_rs::WINDOWS_1251, "\u{43f}\u{435}\u{440}\u{435}\u{432}\u{43e}\u{434}"),
        ("ar", "\u{646}\u{635} \u{639}\u{631}\u{628}\u{64a}", encoding_rs::WINDOWS_1256, "\u{62a}\u{631}\u{62c}\u{645}\u{629}"),
        ("tr", "T\u{fc}rk\u{e7}e metin \u{131}\u{15f}\u{11f}", encoding_rs::WINDOWS_1254, "\u{e7}eviri"),
    ];
    for (lang, text, enc, tr) in cases {
        let full = src(text);
        let (bytes, _, bad) = enc.encode(&full);
        let bytes = bytes.into_owned();
        assert!(!bad, "{lang}");
        let opts = ExtractOptions { source_lang: Some(lang.into()), ..Default::default() };
        let doc = Php.extract(&bytes, &opts).unwrap();
        assert_eq!(plain_text(&doc.units[0].source()), text, "{lang}");
        assert!(doc.warnings.is_empty(), "{lang}: {:?}", doc.warnings);
        assert_eq!(Php.merge(&doc).unwrap(), bytes, "{lang}: untouched");
        // a translation that fits stays in the code page
        let mut d = Php.extract(&bytes, &opts).unwrap();
        d.set_target("u1", vec![Inline::text(tr)]).unwrap();
        let replaced = full.replacen(text, tr, 1);
        let (want, _, bad) = enc.encode(&replaced);
        assert!(!bad);
        assert_eq!(Php.merge(&d).unwrap(), want.to_vec(), "{lang}: in code page");
        // one that does not fit becomes UTF-8
        let mut d = Php.extract(&bytes, &opts).unwrap();
        d.set_target("u1", vec![Inline::text("\u{1f600} \u{e9}\u{4e2d}\u{15f}\u{62a}\u{44f}")]).unwrap();
        let out = Php.merge(&d).unwrap();
        // UTF-8, or GB18030 for a GBK file (it holds every character; the file stays in its own family)
        let s = match String::from_utf8(out.clone()) {
            Ok(s) => s,
            Err(_) => {
                assert_eq!(lang, "zh");
                encoding_rs::GB18030.decode_without_bom_handling(&out).0.into_owned()
            }
        };
        assert!(s.contains("'\u{1f600} \u{e9}"), "{lang}");
    }
}

#[test]
fn undeclared_non_utf8_without_hint_is_windows_1252() {
    let (b, _, _) = encoding_rs::WINDOWS_1252.encode("<?php return ['a' => 'Caf\u{e9} cr\u{e8}me'];");
    let doc = extract(&b).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "Caf\u{e9} cr\u{e8}me");
    assert_eq!(Php.merge(&doc).unwrap(), b.to_vec());
}

#[test]
fn quote_style_is_kept_per_string() {
    let src = "<?php return [\n 'a' => 'Single one',\n 'b' => \"Double one\",\n 'c' => <<<X\nHeredoc one\nX,\n 'd' => <<<'Y'\nNowdoc one\nY,\n 'e' => 'One ' . \"two\",\n];\n";
    let out = build_str(
        src,
        &[(0, "Eins 'x' \\"), (1, "Zwei \"x\" $y \\ \n\t"), (2, "Drei"), (3, "Vier"), (4, "F\u{fc}nf")],
    );
    assert!(out.contains("'a' => 'Eins \\'x\\' \\\\',"), "{out}");
    assert!(out.contains("'b' => \"Zwei \\\"x\\\" \\$y \\\\ \\n\\t\","), "{out}");
    assert!(out.contains("'c' => 'Drei',") && out.contains("'d' => 'Vier',"), "{out}");
    assert!(out.contains("'e' => 'F\u{fc}nf',"), "{out}");
    // re-reading gives the translations back exactly
    let t = texts_str(&out);
    assert_eq!(t[1], "Zwei \"x\" $y \\ \n\t");
    assert_eq!(t[0], "Eins 'x' \\");
}

#[test]
fn free_tags_may_move_and_ordered_tags_are_kept() {
    let src = "<?php return ['a' => 'Hi :first and :second %s',];";
    let mut doc = extract_str(src).unwrap();
    let src_tags: Vec<Inline> = doc.units[0].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    assert!(src_tags[0].is_free() && src_tags[1].is_free() && !src_tags[2].is_free());
    doc.units[0].segments[0].target = vec![Inline::text("A "), src_tags[1].clone(), Inline::text(" B "), src_tags[0].clone(), Inline::text(" "), src_tags[2].clone()];
    let out = String::from_utf8(Php.merge(&doc).unwrap()).unwrap();
    assert_eq!(out, "<?php return ['a' => 'A :second B :first %s',];");
}

#[test]
fn locked_units_with_include_locked() {
    let src = "<?php return ['a' => 'Text', 'b' => '---', 'c' => '', 'd' => ' ', 'e' => '...'];";
    let doc = Php.extract(src.as_bytes(), &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    let locked: Vec<&str> = doc.units.iter().filter(|u| !u.translate).map(|u| u.context.key.as_deref().unwrap()).collect();
    assert_eq!(locked, ["b", "e"]);
    assert_eq!(Php.merge(&doc).unwrap(), src.as_bytes());
    assert_eq!(extract_str(src).unwrap().units.len(), 1);
}

#[test]
fn unit_ids_context_and_lines_are_stable() {
    let doc = extract_str(include_str!("data/locos_laravel.php")).unwrap();
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids[..3], ["u1", "u2", "u3"]);
    let k: Vec<_> = doc.units.iter().map(|u| u.context.key.clone().unwrap()).collect();
    assert!(k.contains(&"nested.deep.concat".to_string()) && k.contains(&"legacy.6".to_string()));
    assert_eq!(doc.format, "php");
    assert!(doc.units.iter().all(|u| u.context.location.as_deref().is_some_and(|l| l.starts_with("line "))));
    // a second extract gives the same ids
    let again = extract_str(include_str!("data/locos_laravel.php")).unwrap();
    assert_eq!(doc.units.iter().map(|u| &u.id).collect::<Vec<_>>(), again.units.iter().map(|u| &u.id).collect::<Vec<_>>());
}

#[test]
fn skeleton_from_another_parser_is_refused() {
    let mut doc = extract_str("<?php return ['a' => 'Text'];").unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo { name: "bilingua-other".into(), version: "0.1.0".into(), schema: 1 });
    }
    assert!(matches!(Php.merge(&doc), Err(Error::ParserVersion { .. })));
}

#[test]
fn old_style_and_closing_tag_forms() {
    let src = "<?php\n$lang = array('a' => 'First', 'b' => array('c' => 'Second'));\n$lang['d'][] = 'Third';\n$other['e'] = 'Fourth';\n?>\n";
    let doc = extract_str(src).unwrap();
    let keys: Vec<_> = doc.units.iter().map(|u| u.context.key.clone().unwrap()).collect();
    assert_eq!(keys, ["a", "b.c", "d.[]", "other.e"]);
    assert_identity(src.as_bytes());
}
