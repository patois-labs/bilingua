//! Round trips, languages, encodings, paragraph mode, locked units, tags, serde, parser stamp, detection, limits.
mod common;
use bilingua_dtd::{DtdFilter, Limits};
use bilingua_model::*;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .expect("corpus folder")
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

/// Wraps every text piece of every translatable unit in `[[ ]]`, tags kept.
fn pseudo(doc: &mut Document) {
    for u in doc.units.iter_mut().filter(|u| u.translate) {
        let mut t = vec![Inline::text("[[")];
        t.extend(u.segments[0].source.clone());
        t.push(Inline::text("]]"));
        u.segments[0].target = merge_text(t);
    }
}

fn tag_shape(doc: &Document) -> Vec<Vec<(String, TagKind)>> {
    doc.units
        .iter()
        .map(|u| {
            u.segments[0]
                .source
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { display, kind, .. } => Some((display.clone(), *kind)),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn corpus_untouched_merge_is_byte_exact() {
    let files = corpus();
    assert!(files.len() >= 15, "corpus has {} files", files.len());
    for (name, data) in &files {
        let doc = extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(DtdFilter.merge(&doc).unwrap(), *data, "{name}");
        assert_identity(data);
    }
}

#[test]
fn corpus_pseudo_translation_reads_back_with_the_same_tags() {
    for (name, data) in &corpus() {
        let mut doc = extract(data).unwrap();
        let before = tag_shape(&doc);
        let texts: Vec<String> = doc.units.iter().map(|u| flat(&u.source())).collect();
        pseudo(&mut doc);
        let out = DtdFilter.merge(&doc).unwrap();
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), texts.len(), "{name}: unit count");
        assert_eq!(tag_shape(&back), before, "{name}: tags");
        for (b, src) in back.units.iter().zip(&texts) {
            assert_eq!(flat(&b.source()), format!("[[{src}]]"), "{name}");
        }
        // keys and notes are untouched
        let keys = |d: &Document| -> Vec<(Option<String>, Vec<String>)> { d.units.iter().map(|u| (u.context.key.clone(), u.context.notes.clone())).collect() };
        assert_eq!(keys(&back), keys(&extract(data).unwrap()), "{name}");
    }
}

#[test]
fn eleven_languages() {
    let langs = ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "ru", "pt", "tr"];
    let dir = corpus_dir();
    let mut seen = Vec::new();
    for l in langs {
        let data = std::fs::read(dir.join(format!("browser_{l}.dtd"))).unwrap();
        let u = units(&data);
        assert_eq!(u.len(), 7, "{l}: {u:?}");
        assert!(u[0].starts_with("&brandShortName; - "), "{l}");
        assert!(u.iter().all(|t| !t.is_empty()));
        assert!(u[5].contains("\u{a9} 100%"), "{l}: char reference and &#037; resolved");
        seen.push(u[1].clone());
        assert_identity(&data);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 11);
    // Arabic, Japanese, Chinese, Korean as translations of the English file, with quotes in the text
    let en = std::fs::read(dir.join("browser_en.dtd")).unwrap();
    for l in ["ar", "ja", "zh", "ko", "ru"] {
        let t = std::fs::read(dir.join(format!("browser_{l}.dtd"))).unwrap();
        let tu = units(&t);
        let mapping: Vec<(usize, &str)> = tu.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
        let out = build(&en, &mapping);
        assert_eq!(units(&out), tu, "{l}");
        assert!(std::str::from_utf8(&out).is_ok());
    }
}

#[test]
fn bom_and_newlines_kept() {
    let data = std::fs::read(corpus_dir().join("bom_crlf.dtd")).unwrap();
    assert!(data.starts_with(&[0xEF, 0xBB, 0xBF]));
    let out = build(&data, &[(0, "Eins"), (1, "Zwei")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(String::from_utf8_lossy(&out).contains("\r\n<!ENTITY a \"Eins\">\r\n"));
    // a note is detached by a blank line, even with CRLF
    let doc = extract(&data).unwrap();
    assert!(doc.units[0].context.notes == vec!["c".to_string()]);
    assert_eq!(doc.units[1].context.notes, vec!["note b".to_string()]);
    let nb = b"<!-- one -->\r\n\r\n<!ENTITY a \"A\">\r<!-- two -->\r\r<!ENTITY b \"B\">";
    let d = extract(nb).unwrap();
    assert!(d.units[0].context.notes.is_empty() && d.units[1].context.notes.is_empty());
    // a lone CR file
    assert_eq!(units(b"<!ENTITY a \"One\">\r<!ENTITY b \"Two\">\r"), vec!["One", "Two"]);
}

#[test]
fn target_locale_and_paragraph_mode() {
    let data = std::fs::read(corpus_dir().join("browser_en.dtd")).unwrap();
    let opts = ExtractOptions::new().with_source_lang("en").with_target_lang("ar").with_granularity(Granularity::Paragraph);
    let mut doc = DtdFilter.extract(&data, &opts).unwrap();
    assert_eq!(doc.granularity, Granularity::Paragraph);
    assert_eq!(doc.target_lang.as_deref(), Some("ar"));
    assert!(doc.units.iter().all(|u| u.segments.len() == 1 && u.separators.is_empty()));
    pseudo(&mut doc);
    // DTD has no language or direction field: the merge changes values only.
    let out = String::from_utf8(DtdFilter.merge(&doc).unwrap()).unwrap();
    assert!(out.starts_with("<!-- This Source Code Form"));
    assert!(out.contains("<!ENTITY login.label \"[[Log in]]\">"), "{out}");
    assert!(out.contains("<!ENTITY window.title \"[[&brandShortName; - Lumen Bakery]]\">"), "{out}");
}

#[test]
fn tags_move_and_escape_once() {
    let src = b"<!ENTITY a \"Hello <html:b>big</html:b> &brandShortName; users\">\n";
    let mut doc = extract(src).unwrap();
    let s = doc.units[0].segments[0].source.clone();
    let tag = |i: usize| s[i].clone();
    assert_eq!(s.len(), 7, "{s:?}");
    // the named reference moves freely; the pair stays in order
    let target = vec![tag(5), Inline::text(" "), tag(1), Inline::text("gro\u{df}"), tag(3), Inline::text(" & \"x\" 5%")];
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, target).unwrap();
    assert_eq!(
        String::from_utf8(DtdFilter.merge(&doc).unwrap()).unwrap(),
        "<!ENTITY a \"&brandShortName; <html:b>gro\u{df}</html:b> &amp; &quot;x&quot; 5&#037;\">\n"
    );
    // a tag that is not in the source is refused by the model
    assert!(doc.set_target(&id, vec![Inline::tag("99", TagKind::Standalone, "&x;")]).is_err());
    // unmatched markup is a standalone tag
    let d = extract(b"<!ENTITY a \"One<br>two</i> three<hr/>\">").unwrap();
    let kinds: Vec<TagKind> = d.units[0].segments[0].source.iter().filter_map(|i| if let Inline::Tag { kind, .. } = i { Some(*kind) } else { None }).collect();
    assert_eq!(kinds, vec![TagKind::Standalone; 3]);
    // the character reference stays text and survives a changed value
    let out = build(b"<!ENTITY a \"Price &#8364; 5\">", &[(0, "Preis \u{20ac} 6")]);
    assert_eq!(out, b"<!ENTITY a \"Preis \xe2\x82\xac 6\">");
}

#[test]
fn locked_units_and_context() {
    let data = b"<!-- the note -->\n<!ENTITY a \"&#037;\">\n<!ENTITY b \"---\">\n<!ENTITY c \"Hello\">\n<!ENTITY e \"\">\n<!ENTITY f \"&brandShortName;\">\n";
    assert_eq!(units(data), vec!["Hello"]);
    let opts = ExtractOptions::new().with_include_locked(true);
    let doc = DtdFilter.extract(data, &opts).unwrap();
    let flags: Vec<(String, bool)> = doc.units.iter().map(|u| (flat(&u.source()), u.translate)).collect();
    assert_eq!(flags, vec![("%".into(), false), ("---".into(), false), ("Hello".into(), true), ("&brandShortName;".into(), false)]);
    assert_eq!(DtdFilter.merge(&doc).unwrap(), data);
    assert_eq!(doc.units[0].context.notes, vec!["the note".to_string()]);
    assert_eq!(doc.units[2].context.key.as_deref(), Some("c"));
    let mut doc = doc;
    let id = doc.units[0].segments[0].id.clone();
    assert!(doc.set_target(&id, vec![Inline::text("x")]).is_err());
}

#[test]
fn ids_are_stable_and_serde_round_trips() {
    let data = std::fs::read(corpus_dir().join("browser_en.dtd")).unwrap();
    let a = extract(&data).unwrap();
    assert_eq!(a, extract(&data).unwrap());
    let ids: Vec<&str> = a.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, vec!["u1", "u2", "u3", "u4", "u5", "u6", "u7"]);
    let mut doc: Document = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(doc, a);
    pseudo(&mut doc);
    let out = DtdFilter.merge(&doc).unwrap();
    assert_eq!(units(&out).len(), 7);
}

#[test]
fn parser_stamp_is_checked() {
    let mut doc = extract(b"<!ENTITY a \"Hello\">").unwrap();
    if let Skeleton::Text { parser, .. } = &mut doc.skeleton {
        *parser = Some(ParserInfo::new("bilingua-json", "0.1.0", 1));
    }
    assert!(matches!(DtdFilter.merge(&doc), Err(Error::ParserVersion { .. })));
}

#[test]
fn detection() {
    let f = DtdFilter;
    assert_eq!(f.id(), "dtd");
    assert!(f.sniff(b"<!ENTITY a \"One\">\n"));
    assert!(f.sniff(b"\xef\xbb\xbf<!-- licence -->\n\n<!ENTITY a \"One\">"));
    assert!(f.sniff(b"<?xml version=\"1.0\"?>\n<!ELEMENT r (#PCDATA)>"));
    // a licence header longer than the sniffed head still counts when an ENTITY follows inside it
    let long = format!("<!-- {} -->\n<!ENTITY a \"One\">\n", "x".repeat(20000));
    assert!(!f.sniff(long.as_bytes()));
    let mid = format!("<!-- {} ", "x".repeat(20000));
    assert!(!f.sniff(mid.as_bytes()));
    assert!(!f.sniff(b"<html><body>hi</body></html>"));
    assert!(!f.sniff(b"<?xml version=\"1.0\"?><root/>"));
    assert!(!f.sniff(b"[General]\na=b\n"));
    assert!(!f.sniff(b"\x00\x01<!ENTITY"));
    assert!(!f.bilingual());
}

#[test]
fn limits_and_quirks() {
    let long = format!("<!ENTITY a \"{}\">\n", "x".repeat(101));
    let tight = Limits::new().with_max_unit_chars(100);
    assert!(DtdFilter.extract_with(long.as_bytes(), &opts(), &tight).is_err());
    let tight = Limits::new().with_max_lines(20);
    assert!(DtdFilter.extract_with("\n".repeat(30).as_bytes(), &opts(), &tight).is_err());
    let tight = Limits::new().with_max_input_bytes(10);
    assert!(DtdFilter.extract_with(b"<!ENTITY a \"One\">", &opts(), &tight).is_err());
    assert!(units(b"").is_empty());
    // an unchanged value keeps its spelling (references, odd quoting)
    let src = b"<!ENTITY a  'It&apos;s &#x41; &quot;ok&quot;'  >";
    assert_eq!(build(src, &[(0, "It's A \"ok\"")]), src);
    // a changed one is escaped for its own quote only
    assert_eq!(build(b"<!ENTITY a 'One'>", &[(0, "It's \"two\"")]), b"<!ENTITY a 'It&apos;s \"two\"'>");
    assert_eq!(build(b"<!ENTITY a \"One\">", &[(0, "It's \"two\"")]), b"<!ENTITY a \"It's &quot;two&quot;\">");
    // a percent sign never reaches the file bare
    assert_eq!(build(b"<!ENTITY a \"One\">", &[(0, "100%")]), b"<!ENTITY a \"100&#037;\">");
    // surrounding white space is kept, the text is trimmed
    assert_eq!(units(b"<!ENTITY a \"  padded  \">"), vec!["padded"]);
    assert_eq!(build(b"<!ENTITY a \"  padded  \">", &[(0, "Neu")]), b"<!ENTITY a \"  Neu  \">");
    // a name that starts with a non-ASCII letter is refused, as LocOS does
    assert!(extract("<!ENTITY \u{fc}ber \"x\">".as_bytes()).is_err());
    // white space inside the name class (U+3000, U+2028) still ends the name, as LocOS's backtracking regex does
    let src = "<!ENTITY b\u{3000}\"Ideo\">\n<!ENTITY c\u{3000}d\u{2028}'Two'>\n";
    let doc = extract(src.as_bytes()).unwrap();
    let keys: Vec<_> = doc.units.iter().map(|u| u.context.key.clone().unwrap()).collect();
    assert_eq!(keys, vec!["b".to_string(), "c\u{3000}d".to_string()]);
    assert_identity(src.as_bytes());
    // ... and then `c"x"` is no quoted value, so LocOS keeps the declaration as it is (no unit)
    assert!(units("<!ENTITY b\u{3000}c\"x\">".as_bytes()).is_empty());
    // Latin-1 and UTF-16 are refused with a typed error
    // B-side 1: an encoding refusal is code `encoding` with LocOS's sentence, not `parse` ("not a dtd file: ...")
    assert!(matches!(
        extract(b"<!ENTITY a \"caf\xe9\">"),
        Err(Error::Encoding(m)) if m == "Mozilla .dtd files are UTF-8; byte 0xe9 at position 15 is not. Re-save the file as UTF-8."
    ));
    assert!(matches!(extract(b"<!ENTITY a \"x\0y\">"), Err(Error::Encoding(m)) if m.contains("NUL bytes")));
    assert!(matches!(extract(b"\x00\x01\x02\xff<!ENTITY a \"x\">"), Err(Error::Encoding(m)) if m.contains("binary file")));
    assert!(extract(b"\xff\xfe<\x00!\x00").is_err());
}
