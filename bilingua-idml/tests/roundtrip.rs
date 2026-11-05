//! Round trips: untouched merge gives the original file, a pseudo-translated merge reads back with the same
//! units and tags, a target-locale merge writes the language and direction, 11 languages, other encodings.

mod common;
use bilingua_idml::Idml;
use bilingua_model::*;
use common::*;

fn rich_package() -> Vec<u8> {
    let langs = "\t<Language Self=\"Language/$ID/English%3a USA\" Name=\"$ID/English: USA\" Id=\"269\"/>\n\
\t<Language Self=\"Language/$ID/Arabic\" Name=\"$ID/Arabic\" Id=\"1\"/>\n\
\t<Language Self=\"Language/$ID/Hebrew\" Name=\"$ID/Hebrew\" Id=\"2\"/>\n\
\t<Language Self=\"Language/$ID/Japanese\" Name=\"$ID/Japanese\" Id=\"3\"/>\n\
\t<Language Self=\"Language/$ID/Korean\" Name=\"$ID/Korean\" Id=\"4\"/>\n\
\t<Language Self=\"Language/$ID/Chinese%3a Simplified\" Name=\"$ID/Chinese: Simplified\" Id=\"5\"/>\n\
\t<Language Self=\"Language/$ID/Russian\" Name=\"$ID/Russian\" Id=\"6\"/>\n\
\t<Language Self=\"Language/$ID/Thai\" Name=\"$ID/Thai\" Id=\"7\"/>\n\
\t<Language Self=\"Language/$ID/de_DE_2006\" Name=\"$ID/de_DE_2006\" Id=\"275\"/>\n\
\t<Language Self=\"Language/$ID/French\" Name=\"$ID/French\" Id=\"8\"/>\n\
\t<Language Self=\"Language/$ID/Spanish%3a Castilian\" Name=\"$ID/Spanish: Castilian\" Id=\"9\"/>\n\
\t<Language Self=\"Language/$ID/hi_IN\" Name=\"$ID/hi_IN\" Id=\"348\"/>\n";
    let body = format!(
        "{}\n{}\n{}",
        psr_s(
            &format!(
                "{}{}{}",
                csr_x("<Content>Welcome to the </Content>", NOSTYLE, " AppliedLanguage=\"$ID/English: USA\" CharacterDirection=\"LeftToRightDirection\""),
                csr_b("<Content>InDesign</Content><Br/>", BOLD),
                csr("<Content>Page\t<?ACE 18?> of <HyperlinkTextSource Self=\"h1\" Name=\"l\"><Content>the site</Content></HyperlinkTextSource>.</Content>")
            ),
            "ParagraphStyle/Body"
        ),
        psr(&csr("<Content>Second paragraph, plain.</Content>")),
        psr(&csr("<Content>Last one<Footnote><ParagraphStyleRange><CharacterStyleRange><Content>Footnote text.</Content></CharacterStyleRange></ParagraphStyleRange></Footnote>.</Content>"))
    );
    Pkg::one(body).conditions(langs).build()
}

/// Every text piece gets a marker and accents; tags stay.
fn pseudo(src: &[Inline], word: &str) -> Vec<Inline> {
    src.iter()
        .map(|i| match i {
            Inline::Text(t) => {
                let lead: String = t.chars().take_while(|c| c.is_whitespace()).collect();
                let trail: String = t.chars().rev().take_while(|c| c.is_whitespace()).collect::<Vec<_>>().into_iter().rev().collect();
                if t.trim().is_empty() {
                    Inline::text(t.clone())
                } else {
                    Inline::text(format!("{lead}{word}{trail}"))
                }
            }
            other => other.clone(),
        })
        .collect()
}

/// Okapi-style pseudo translation: vowels get accents, so every text keeps its length (and with it which
/// formatting range is the untagged one).
fn set_accented(doc: &mut Document) {
    for u in &mut doc.units {
        let t: Vec<Inline> = u.segments[0]
            .source
            .iter()
            .map(|i| match i {
                Inline::Text(t) => Inline::text(
                    t.chars()
                        .map(|c| match c {
                            'a' => '\u{e0}',
                            'e' => '\u{e9}',
                            'i' => '\u{ec}',
                            'o' => '\u{f2}',
                            'u' => '\u{f9}',
                            c => c,
                        })
                        .collect::<String>(),
                ),
                other => other.clone(),
            })
            .collect();
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
}

fn set_all(doc: &mut Document, word: &str) {
    for u in &mut doc.units {
        let t = pseudo(&u.segments[0].source, word);
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
}

fn tags(v: &[Inline]) -> Vec<(String, TagKind)> {
    v.iter().filter_map(|i| if let Inline::Tag { id, kind, .. } = i { Some((id.clone(), *kind)) } else { None }).collect()
}

#[test]
fn untouched_merge_is_the_original_file() {
    let data = rich_package();
    let doc = extract(&data);
    assert_eq!(merge(&doc), data);
    // through JSON, as the Python wheel and the editor do
    let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_eq!(merge(&back), data);
}

#[test]
fn pseudo_translated_merge_reads_back_with_same_units_and_tags() {
    let data = rich_package();
    let mut doc = extract(&data);
    set_accented(&mut doc);
    let out = merge(&doc);
    let again = extract(&out);
    assert_eq!(again.units.len(), doc.units.len());
    for (a, b) in doc.units.iter().zip(&again.units) {
        assert_eq!(tags(&a.segments[0].source), tags(&b.segments[0].source), "unit {}", a.id);
        assert_eq!(b.segments[0].source, a.segments[0].target, "unit {}", a.id);
    }
    // members the translation did not touch (everything but the story) are byte-identical
    let (src, dst) = (members_of(&data), members_of(&out));
    for (a, b) in src.iter().zip(&dst) {
        assert_eq!(a.name, b.name);
        if !a.name.starts_with("Stories/") {
            assert_eq!(a.data, b.data, "{}", a.name);
        }
    }
    assert!(dst[0].name == "mimetype" && dst[0].meta.stored);
}

#[test]
fn only_changed_stories_are_rewritten() {
    let data = Pkg::new(&[("a", psr(&csr("<Content>One.</Content>"))), ("b", psr(&csr("<Content>Two.</Content>")))]).build();
    let mut doc = extract(&data);
    translate(&mut doc, &[("p1", "Zwei.".into())]);
    let out = merge(&doc);
    let (src, dst) = (members_of(&data), members_of(&out));
    for (a, b) in src.iter().zip(&dst) {
        assert_eq!(a.data == b.data, a.name != "Stories/Story_b.xml", "{}", a.name);
    }
}

const LANGS: [(&str, &str, bool); 11] = [
    ("de-DE", "Willkommen auf der Seite \u{00fc}\u{00df}", false),
    ("fr-FR", "Bienvenue \u{00e0} la page \u{00e9}t\u{00e9}", false),
    ("es-ES", "Bienvenido a la p\u{00e1}gina \u{00f1}", false),
    ("ar-SA", "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0643}\u{0645} \u{0641}\u{064a} \u{0627}\u{0644}\u{0635}\u{0641}\u{062d}\u{0629}", true),
    ("he-IL", "\u{05e9}\u{05dc}\u{05d5}\u{05dd} \u{05e2}\u{05d5}\u{05dc}\u{05dd}", true),
    ("ja-JP", "\u{30da}\u{30fc}\u{30b8}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}", false),
    ("zh-CN", "\u{6b22}\u{8fce}\u{6765}\u{5230}\u{672c}\u{9875}\u{9762}", false),
    ("ko-KR", "\u{d398}\u{c774}\u{c9c0}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}", false),
    ("ru-RU", "\u{0414}\u{043e}\u{0431}\u{0440}\u{043e} \u{043f}\u{043e}\u{0436}\u{0430}\u{043b}\u{043e}\u{0432}\u{0430}\u{0442}\u{044c}", false),
    ("hi-IN", "\u{0938}\u{094d}\u{0935}\u{093e}\u{0917}\u{0924} \u{0939}\u{0948}", false),
    ("th-TH", "\u{0e22}\u{0e34}\u{0e19}\u{0e14}\u{0e35}\u{0e15}\u{0e49}\u{0e2d}\u{0e19}\u{0e23}\u{0e31}\u{0e1a}", false),
];

#[test]
fn eleven_languages_round_trip_and_stamp() {
    let data = rich_package();
    for (lang, text, rtl) in LANGS {
        let mut doc = extract_for(&data, lang);
        set_all(&mut doc, text);
        let out = merge(&doc);
        let again = extract(&out);
        assert_eq!(again.units.len(), doc.units.len(), "{lang}");
        for (a, b) in doc.units.iter().zip(&again.units) {
            assert_eq!(tags(&a.segments[0].source), tags(&b.segments[0].source), "{lang}");
        }
        assert!(plain_text(&again.units[1].segments[0].source).contains(text), "{lang}");
        let t = story_tree(&out, "a");
        let psrs = named(&t, "ParagraphStyleRange");
        let rtl_marked = psrs.iter().filter(|p| t.attr(**p, "ParagraphDirection") == Some("RightToLeftDirection")).count();
        if rtl {
            assert!(rtl_marked >= 3, "{lang}: {rtl_marked}");
            assert!(psrs.iter().filter(|p| t.attr(**p, "Composer") == Some("HL Composer Optyca")).count() >= 3, "{lang}");
            // the explicit left-to-right direction of the first range is released
            assert!(named_attr(&t, "CharacterStyleRange", "CharacterDirection", "LeftToRightDirection").is_empty(), "{lang}");
        } else {
            assert_eq!(rtl_marked, 0, "{lang}");
        }
        // a language the package declares is stamped on every rebuilt range of the body paragraphs
        let stamped: Vec<&str> = named(&t, "CharacterStyleRange").into_iter().filter_map(|r| t.attr(r, "AppliedLanguage")).filter(|l| *l != "$ID/English: USA").collect();
        assert!(!stamped.is_empty(), "{lang}");
        assert!(stamped.iter().all(|l| *l == stamped[0]), "{lang}: {stamped:?}");
    }
}

#[test]
fn a_rewritten_paragraph_without_a_declared_language_is_not_stamped() {
    let data = package(psr(&csr("<Content>Hello</Content>")));
    let mut doc = extract_for(&data, "de-DE");
    translate(&mut doc, &[("p0", "Hallo".into())]);
    assert!(!raw_story(&merge(&doc), "a").contains("AppliedLanguage"));
    assert!(!raw_story(&merge(&doc), "a").contains("ParagraphDirection"));
}

#[test]
fn untranslated_units_stay_as_they_were_in_a_translated_story() {
    let data = rich_package();
    let mut doc = extract_for(&data, "de-DE");
    let id = doc.units[1].id.clone();
    translate(&mut doc, &[(&id, "Zweiter Absatz.".into())]);
    let out = merge(&doc);
    let t = story_tree(&out, "a");
    assert!(visible_text(&t).contains("Welcome to the "));
    assert!(visible_text(&t).contains("Zweiter Absatz."));
    assert_eq!(units_of(&out)[0], units_of(&data)[0]);
}

fn with_story_bytes(bytes: Vec<u8>) -> Vec<u8> {
    let mut m = Pkg::one(String::new()).members();
    for e in &mut m {
        if e.0 == "Stories/Story_a.xml" {
            e.1 = bytes.clone();
        }
    }
    zip_of(&m)
}

#[test]
fn utf16_story_keeps_its_encoding() {
    let xml = String::from_utf8(story_xml("a", &psr(&csr("<Content>Gr\u{00fc}\u{00df}e \u{65e5}\u{672c}</Content>")))).unwrap().replace("UTF-8", "UTF-16");
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml.encode_utf16().flat_map(|u| u.to_le_bytes()));
    let data = with_story_bytes(bytes);
    let mut doc = extract(&data);
    assert_eq!(texts(&doc), ["Gr\u{00fc}\u{00df}e \u{65e5}\u{672c}"]);
    assert_eq!(merge(&doc), data);
    translate(&mut doc, &[("p0", "Hallo \u{4e16}\u{754c}".into())]);
    let out = merge(&doc);
    let raw = members_of(&out).into_iter().find(|e| e.name == "Stories/Story_a.xml").unwrap().data;
    assert_eq!(&raw[..2], &[0xFF, 0xFE]);
    assert_eq!(units_of(&out), ["Hallo \u{4e16}\u{754c}"]);
}

#[test]
fn declared_legacy_encoding_is_read_and_written_back() {
    let xml = String::from_utf8(story_xml("a", &psr(&csr("<Content>Caf\u{00e9} cr\u{00e8}me</Content>")))).unwrap().replace("UTF-8", "windows-1252");
    let (bytes, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode(&xml);
    let data = with_story_bytes(bytes.into_owned());
    let mut doc = extract(&data);
    assert_eq!(texts(&doc), ["Caf\u{00e9} cr\u{00e8}me"]);
    translate(&mut doc, &[("p0", "Th\u{00e9} noir".into())]);
    let out = merge(&doc);
    let raw = members_of(&out).into_iter().find(|e| e.name == "Stories/Story_a.xml").unwrap().data;
    assert!(raw.contains(&0xE9), "the story is still windows-1252");
    assert_eq!(units_of(&out), ["Th\u{00e9} noir"]);
}

#[test]
fn undeclared_non_utf8_is_refused() {
    let mut bytes = story_xml("a", &psr(&csr("<Content>Caf\u{00e9}</Content>")));
    let p = bytes.windows(2).position(|w| w == [0xC3, 0xA9]).unwrap();
    bytes.splice(p..p + 2, [0xE9]);
    assert!(Idml.extract(&with_story_bytes(bytes), &ExtractOptions::default()).is_err());
}

#[test]
fn okapi_corpus_files_round_trip() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/idml");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "idml") {
            continue;
        }
        let data = std::fs::read(&p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let mut doc = Idml.extract(&data, &ExtractOptions { target_lang: Some("de-DE".into()), ..Default::default() }).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!doc.units.is_empty(), "{name}");
        assert_eq!(merge(&doc), data, "{name}: untouched merge");
        set_accented(&mut doc);
        let out = merge(&doc);
        let again = extract(&out);
        assert_eq!(again.units.len(), doc.units.len(), "{name}");
        for (a, b) in doc.units.iter().zip(&again.units) {
            assert_eq!(tags(&a.segments[0].source), tags(&b.segments[0].source), "{name} {}", a.id);
            assert_eq!(plain_text(&b.segments[0].source), plain_text(&a.segments[0].target), "{name} {}", a.id);
        }
        // every XML part still parses
        for e in members_of(&out) {
            if e.name.ends_with(".xml") {
                bilingua_idml::xml::Tree::parse(std::str::from_utf8(&e.data).unwrap(), &Default::default()).unwrap_or_else(|m| panic!("{name} {}: {m}", e.name));
            }
        }
        n += 1;
    }
    assert!(n >= 5);
}
