//! Round trips on the corpus (11 languages, SSA and ASS, UTF-8 / BOM / UTF-16 / code page), safety of translated
//! text, locked comments, ids, language stamping.
mod common;
use bilingua_ass::AssFilter;
use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::*;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus_dir())
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

/// Each text piece of each unit wrapped in tildes; tags untouched.
fn pseudo(doc: &mut Document) {
    let items: Vec<(String, Vec<Inline>)> = doc
        .units
        .iter()
        .filter(|u| u.translate)
        .map(|u| {
            let t = u
                .source()
                .into_iter()
                .map(|i| match i {
                    Inline::Text(t) => Inline::Text(format!("~{t}~")),
                    tag => tag,
                })
                .collect();
            (u.segments[0].id.clone(), t)
        })
        .collect();
    doc.set_targets(items).unwrap();
}

fn shape(u: &Unit) -> Vec<String> {
    u.source()
        .iter()
        .map(|i| match i {
            Inline::Tag { kind, display, .. } => format!("{kind:?}:{display}"),
            Inline::Text(_) => "text".into(),
        })
        .collect()
}

#[test]
fn corpus_is_complete() {
    let names: Vec<String> = corpus().into_iter().map(|c| c.0).collect();
    for l in ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "ru", "pt", "tr"] {
        assert!(names.contains(&format!("lumen_{l}.ass")), "{l}");
    }
    assert!(names.iter().any(|n| n.ends_with(".ssa")));
}

#[test]
fn untouched_and_echo_merges_are_exact() {
    for (name, data) in corpus() {
        let doc = extract(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(AssFilter.merge(&doc).unwrap(), data, "{name}: untouched");
        assert_identity(&data);
        assert!(!doc.units.is_empty() || name == "edge.ass" || name.starts_with("mixed"), "{name}: units");
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in corpus() {
        let mut doc = extract(&data).unwrap();
        pseudo(&mut doc);
        let out = AssFilter.merge(&doc).unwrap();
        assert_ne!(out, data, "{name}: something changed");
        let back = extract(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}: unit count");
        for (a, b) in doc.units.iter().zip(&back.units) {
            assert_eq!(shape(a), shape(b), "{name}: tags of {}", a.id);
            let want: String = plain_text(&a.output()).replace('{', "\u{ff5b}");
            let got = plain_text(&b.source());
            assert_eq!(got, want, "{name}: text of {}", a.id);
        }
        // the file keeps its own encoding and line endings
        let again = AssFilter.extract(&out, &ExtractOptions::default()).unwrap();
        assert!(matches!((&doc.skeleton, &again.skeleton), (Skeleton::Text { encoding: a, .. }, Skeleton::Text { encoding: b, .. }) if a == b), "{name}: encoding");
        // everything outside the units is the same bytes
        let (Skeleton::Text { parts: pa, .. }, Skeleton::Text { parts: pb, .. }) = (&doc.skeleton, &back.skeleton) else { panic!() };
        let raws = |p: &Vec<SkelPart>| p.iter().filter_map(|x| if let SkelPart::Raw(s) = x { Some(s.clone()) } else { None }).collect::<Vec<_>>();
        assert_eq!(raws(pa), raws(pb), "{name}: skeleton");
    }
}

#[test]
fn eleven_languages_translate_into_each_other() {
    // every language file gets the text of the next one (same unit count by construction)
    let langs = ["en", "de", "fr", "es", "ja", "zh", "ko", "ar", "ru", "pt", "tr"];
    let docs: Vec<Document> = langs.iter().map(|l| extract(&std::fs::read(corpus_dir().join(format!("lumen_{l}.ass"))).unwrap()).unwrap()).collect();
    for (i, l) in langs.iter().enumerate() {
        let from = &docs[i];
        let to = &docs[(i + 1) % langs.len()];
        assert_eq!(from.units.len(), to.units.len(), "{l}");
        let mut d = from.clone();
        let items: Vec<(String, Vec<Inline>)> = from
            .units
            .iter()
            .zip(&to.units)
            .map(|(a, b)| {
                // the other language's words in this file's tags
                let words: Vec<String> = b.source().iter().filter_map(|x| if let Inline::Text(t) = x { Some(t.clone()) } else { None }).collect();
                let mut w = words.iter().cycle();
                let t = a
                    .source()
                    .into_iter()
                    .map(|x| match x {
                        Inline::Text(_) => Inline::Text(w.next().cloned().unwrap_or_default()),
                        tag => tag,
                    })
                    .collect();
                (a.segments[0].id.clone(), t)
            })
            .collect();
        d.set_targets(items).unwrap();
        d.target_lang = Some(["de", "fr", "es", "ja", "zh-Hans", "ko", "ar", "ru", "pt-BR", "tr", "en-US"][i].to_string());
        let out = AssFilter.merge_with(&d, &MergeOptions::default()).unwrap();
        let s = String::from_utf8(out.clone()).unwrap();
        let two = d.target_lang.as_deref().unwrap().split('-').next().unwrap().to_string();
        assert!(s.contains(&format!("Language: {two}\n")), "{l}: {}", &s[..120]);
        let back = extract(&out).unwrap();
        assert_eq!(back.units.len(), from.units.len(), "{l}");
        for (u, b) in d.units.iter().zip(&back.units) {
            assert_eq!(plain_text(&u.output()), plain_text(&b.source()), "{l} {}", u.id);
        }
    }
}

#[test]
fn legacy_code_pages_keep_their_encoding() {
    // Windows-1252 (the default for an undeclared non-UTF-8 file)
    let (cp, _) = {
        let (b, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode("[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Caf\u{e9} cr\u{e8}me\n");
        (b.into_owned(), ())
    };
    let mut doc = extract(&cp).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "Caf\u{e9} cr\u{e8}me");
    doc.set_target("l2", vec![Inline::text("Th\u{e9}")]).unwrap();
    let out = AssFilter.merge(&doc).unwrap();
    assert!(out.ends_with(b"Th\xe9\n"), "written back in Windows-1252: {out:?}");
    // a character outside the code page: the file becomes UTF-8
    doc.set_target("l2", vec![Inline::text("\u{65e5}\u{672c}")]).unwrap();
    let out = AssFilter.merge(&doc).unwrap();
    assert!(String::from_utf8(out).unwrap().ends_with("\u{65e5}\u{672c}\n"));
    // Shift_JIS, GBK and EUC-KR files, with the source language naming the code page
    for (enc, lang, text) in [
        (bilingua_model::encoding_rs::SHIFT_JIS, "ja", "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{5b57}\u{5e55}\u{3067}\u{3059}"),
        (bilingua_model::encoding_rs::GBK, "zh", "\u{4e2d}\u{6587}\u{5b57}\u{5e55}\u{5728}\u{8fd9}\u{91cc}"),
        (bilingua_model::encoding_rs::EUC_KR, "ko", "\u{d55c}국\u{c5b4} \u{c790}\u{b9c9}\u{c785}\u{b2c8}\u{b2e4}"),
    ] {
        let src = format!("[Script Info]\nTitle: {text}\n[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{text}\\N{{\\i1}}{text}{{\\i0}}\n");
        let (b, _, bad) = enc.encode(&src);
        assert!(!bad, "{lang}");
        let b = b.into_owned();
        let o = ExtractOptions::new().with_source_lang(lang);
        let d = AssFilter.extract(&b, &o).unwrap();
        assert_eq!(d.units.len(), 2, "{lang}");
        assert!(plain_text(&d.units[1].source()).contains(text), "{lang}: read as {}", plain_text(&d.units[1].source()));
        assert_eq!(AssFilter.merge(&d).unwrap(), b, "{lang}: untouched");
        let mut d2 = d.clone();
        let id = d2.units[1].segments[0].id.clone();
        let t = d2.units[1].source();
        d2.set_target(&id, t).unwrap();
        d2.units[0].segments[0].target = vec![Inline::text(text.chars().rev().collect::<String>())];
        let out = AssFilter.merge(&d2).unwrap();
        let back = AssFilter.extract(&out, &o).unwrap();
        assert_eq!(plain_text(&back.units[0].source()), text.chars().rev().collect::<String>(), "{lang}: kept in its code page");
        assert!(out.len() < b.len() + 8, "{lang}: not widened to UTF-8");
        let utf8 = AssFilter.merge_with(&d2, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        assert!(std::str::from_utf8(&utf8).is_ok(), "{lang}: UTF-8 on request");
    }
}

#[test]
fn translated_text_cannot_become_markup() {
    let src = "[Script Info]\nTitle: Name\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Hello {\\i1}there{\\i0}\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,Second\n";
    let hostile = [
        "{\\an8}Top", "{\\p1}m 0 0 l 9 9{\\p0}", "a\\Nb", "a\\nb\\hc", "x\\", "\\{\\i1}", "{", "}{", "a}{b", "\\\\N", "\\ N",
        "line one\r\nDialogue: 0,0:00:09.00,0:00:10.00,Default,,0,0,0,,Injected cue", "x\n[Events]\nFormat: Text", "x\rTitle: injected header",
        "{\\r}", "\u{2028}x\u{85}", ",,,,,,,,,", "Comment: 0,0,0", "{\\b1}", "tail\\",
    ];
    for h in hostile {
        let data = src.as_bytes();
        let doc0 = extract(data).unwrap();
        let mut doc = doc0.clone();
        // the first event (a unit with tags): only text, no tags
        doc.set_target("l5", vec![Inline::text(h)]).unwrap();
        doc.set_target("l6", vec![Inline::text(h)]).unwrap();
        doc.set_target("l2", vec![Inline::text(h)]).unwrap();
        let out = AssFilter.merge(&doc).unwrap();
        let s = String::from_utf8(out.clone()).unwrap();
        // no new line, no new cue, no new header: the line structure is the original's
        assert_eq!(s.lines().count(), src.lines().count(), "{h:?}: {s}");
        let back = extract(&out).unwrap();
        assert_eq!(back.units.len(), doc0.units.len(), "{h:?}");
        // no code outside the original ones: the only tags are the source's, and the second event has none
        // (a typed line break is the one code a translation may make: it reads back as a `\N` code)
        let only_breaks = |u: &Unit| u.source().iter().all(|i| matches!(i, Inline::Text(_)) || matches!(i, Inline::Tag { display, .. } if display == "\\N"));
        assert!(only_breaks(&back.units[2]), "{h:?}: {:?}", back.units[2].source());
        assert!(only_breaks(&back.units[1]), "{h:?}");
        let ev = s.lines().find(|l| l.contains(",,Hello") || l.starts_with("Dialogue: 0,0:00:01")).unwrap();
        // the override block the source had is gone (the unit was replaced by plain text)
        assert!(!ev.contains("{\\i1}") || h.contains('\u{ff5b}'), "{h:?}: {ev}");
        // the raw control sequences never appear in a Dialogue text
        let text_of = |l: &str| l.splitn(10, ',').nth(9).unwrap_or("").to_string();
        let t2 = text_of(s.lines().find(|l| l.starts_with("Dialogue: 0,0:00:03")).unwrap());
        assert!(!t2.contains('{'), "{h:?}: {t2}");
        let stripped = t2.replace("\\N", "");
        assert!(!stripped.contains("\\n") && !stripped.contains("\\h") && !stripped.contains("\\N"), "{h:?}: {t2}");
        // the title stays one line
        assert!(s.lines().any(|l| l.starts_with("Title: ")), "{h:?}");
        assert!(s.lines().filter(|l| l.starts_with("Title:")).count() == 1, "{h:?}: {s}");
    }
}

#[test]
fn a_translation_with_typed_codes_keeps_the_tags_it_was_given() {
    let src = b"[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\an8}Hello {\\i1}big{\\i0} world\\Nagain\n";
    let mut doc = extract(src).unwrap();
    let u = &doc.units[0];
    assert_eq!(to_locos_text(&u.source()).0, "Hello \u{27E6}g1\u{27E7}big\u{27E6}/g1\u{27E7} world\u{27E6}x2\u{27E7}again");
    // Hola mundo grande: tags moved
    let t = vec![
        Inline::text("Hola "),
        Inline::tag("2", TagKind::Standalone, "\\N"),
        Inline::tag("1", TagKind::Open, ""),
        Inline::text("gran"),
        Inline::tag("1", TagKind::Close, ""),
        Inline::text(" mundo"),
    ];
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, t).unwrap();
    let out = String::from_utf8(AssFilter.merge(&doc).unwrap()).unwrap();
    assert!(out.ends_with(",,{\\an8}Hola \\N{\\i1}gran{\\i0} mundo\n"), "{out}");
}

#[test]
fn ids_context_and_locked_comments() {
    let (_, data) = corpus().into_iter().find(|c| c.0 == "lumen_en.ass").unwrap();
    let a = extract(&data).unwrap();
    let b = extract(&data).unwrap();
    assert_eq!(a, b, "stable");
    let ids: Vec<&str> = a.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["l3", "l13", "l15", "l16", "l18"]);
    let d = &a.units[1];
    assert_eq!(d.context.kind.as_deref(), Some("subtitle"));
    assert_eq!(d.context.extra.get("start").map(String::as_str), Some("0:00:01.00"));
    assert_eq!(d.context.extra.get("end").map(String::as_str), Some("0:00:04.00"));
    assert_eq!(d.context.extra.get("style").map(String::as_str), Some("Default"));
    assert_eq!(d.context.extra.get("name").map(String::as_str), Some("Anna"));
    assert_eq!(a.units[0].context.kind.as_deref(), Some("title"));
    // locked comments on request
    let o = ExtractOptions::new().with_include_locked(true);
    let l = AssFilter.extract(&data, &o).unwrap();
    assert_eq!(l.units.len(), a.units.len() + 1);
    let c = l.units.iter().find(|u| !u.translate).unwrap();
    assert_eq!(plain_text(&c.source()), "Note for the editor: keep it short");
    assert_eq!(AssFilter.merge(&l).unwrap(), data);
    let mut l2 = l.clone();
    assert!(matches!(l2.set_target(&c.id, vec![Inline::text("x")]), Err(Error::Locked(_))));
    let flag = ExtractOptions::new().with_format_options([("ass.include_locked".to_string(), "yes".to_string())].into());
    assert_eq!(AssFilter.extract(&data, &flag).unwrap().units.len(), l.units.len());
    // paragraph mode and segment splitting keep working
    let p = AssFilter.extract(&data, &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    assert_eq!(p.granularity, Granularity::Paragraph);
    assert!(p.units.iter().all(|u| u.segments.len() == 1));
}

#[test]
fn language_stamping_rules() {
    let (_, data) = corpus().into_iter().find(|c| c.0 == "lumen_en.ass").unwrap();
    let mut doc = AssFilter.extract(&data, &ExtractOptions::new().with_target_lang("fr-FR")).unwrap();
    // nothing translated: the original bytes, no stamp
    assert_eq!(AssFilter.merge(&doc).unwrap(), data);
    // unless asked (the LocOS behaviour)
    let always = MergeOptions::new().with_stamp_target_always(true);
    assert!(String::from_utf8(AssFilter.merge_with(&doc, &always).unwrap()).unwrap().starts_with("[Script Info]\nLanguage: fr\n"));
    // something translated: stamped
    doc.set_target("l13", vec![Inline::text("Bonjour")]).unwrap();
    assert!(String::from_utf8(AssFilter.merge(&doc).unwrap()).unwrap().starts_with("[Script Info]\nLanguage: fr\n"));
    // a header on the very last line with no line end gets one before the new header line
    let mut d = AssFilter.extract(b"[Script Info]", &ExtractOptions::new().with_target_lang("de")).unwrap();
    d.units.clear();
    assert_eq!(AssFilter.merge_with(&d, &always).unwrap(), b"[Script Info]\nLanguage: de\n");
    // a CRLF file gets a CRLF header line; a bad tag changes nothing
    let crlf = b"[Script Info]\r\nTitle: x\r\n";
    let mut d = AssFilter.extract(crlf, &ExtractOptions::new().with_target_lang("de")).unwrap();
    d.set_target("l2", vec![Inline::text("y")]).unwrap();
    assert_eq!(AssFilter.merge(&d).unwrap(), b"[Script Info]\r\nLanguage: de\r\nTitle: y\r\n");
    d.target_lang = Some("not a tag".into());
    assert_eq!(AssFilter.merge(&d).unwrap(), b"[Script Info]\r\nTitle: y\r\n");
}

#[test]
fn utf8_option_matches_locos_for_utf8_files() {
    let (_, data) = corpus().into_iter().find(|c| c.0 == "lumen_de_utf8bom_crlf.ass").unwrap();
    assert!(data.starts_with(&[0xEF, 0xBB, 0xBF]));
    let mut doc = extract(&data).unwrap();
    pseudo(&mut doc);
    let out = AssFilter.merge_with(&doc, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]) && !out[3..].starts_with(&[0xEF, 0xBB, 0xBF]));
}
