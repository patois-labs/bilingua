//! Round trips on the corpus, eleven languages and their stamps, UTF-8 and BOM kept, zip details kept, sentence
//! segments written back, locked units.
mod common;
use bilingua_flare::Flare;
use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use common::*;
use std::path::Path;

fn corpus_files() -> Vec<(String, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/flprjzip");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "flprjzip"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 3);
    v
}

fn pseudo(doc: &mut Document, mark: &str) {
    for i in 0..doc.units.len() {
        let u = &doc.units[i];
        let tgt: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(t) => Inline::text(format!("{mark}{t}")),
                o => o.clone(),
            })
            .collect();
        let id = u.segments[0].id.clone();
        doc.set_target(&id, tgt).unwrap();
    }
}

#[test]
fn every_corpus_file_round_trips() {
    for (name, bytes) in corpus_files() {
        let mut d = extract(&bytes);
        assert!(!d.units.is_empty(), "{name}");
        assert_eq!(merge(&d), bytes, "{name}: untouched merge");
        // the JSON form of the skeleton keeps the merge exact
        let json = serde_json::to_string(&d).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(merge(&back), bytes, "{name}: after JSON");
        d.target_lang = Some("de-DE".into());
        pseudo(&mut d, "~");
        let out = merge(&d);
        assert_ne!(out, bytes);
        let (m, _) = members(&out);
        well_formed(&m);
        let again = extract(&out);
        assert_eq!(again.units.len(), d.units.len(), "{name}");
        for (a, b) in again.units.iter().zip(&d.units) {
            assert_eq!(a.id, b.id, "{name}");
            let (ta, _) = to_locos_text(&a.source());
            let (tb, _) = to_locos_text(&b.output());
            assert_eq!(ta, tb, "{name} {}", a.id);
        }
        // same members in the same order
        assert_eq!(m.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), members(&bytes).0.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), "{name}");
    }
}

#[test]
fn members_that_did_not_change_are_copied_byte_for_byte() {
    let src = project();
    let mut d = extract(&src);
    // only the glossary changes
    let i = index_of(&d, "spooler");
    set(&mut d, i, "Spool");
    let (m, infos) = members(&merge(&d));
    let (o, oinfos) = members(&src);
    for ((n, b), (_, a)) in o.iter().zip(&m) {
        if n != "Project/Glossaries/Glossary.flglo" {
            assert_eq!(a, b, "{n}");
        }
    }
    assert_eq!(infos.iter().map(|i| i.stored).collect::<Vec<_>>(), oinfos.iter().map(|i| i.stored).collect::<Vec<_>>());
    assert!(text_of(&m, "Project/Glossaries/Glossary.flglo").contains("<Term>Spool</Term>"));
    // the glossary is the only difference, and only the term text changed
    assert_eq!(text_of(&m, "Project/Glossaries/Glossary.flglo"), FLGLO.replace("<Term>spooler</Term>", "<Term>Spool</Term>"));
}

const LANGS: [(&str, &str, bool); 11] = [
    ("en", "Bake the bread", false),
    ("ar", "\u{627}\u{62e}\u{628}\u{632} \u{627}\u{644}\u{62e}\u{628}\u{632}", true),
    ("ja", "\u{30d1}\u{30f3}\u{3092}\u{713c}\u{304f}", false),
    ("zh", "\u{70d8}\u{70e4}\u{9762}\u{5305}", false),
    ("ko", "\u{be75} \u{ad7d}\u{ae30}", false),
    ("ru", "\u{418}\u{441}\u{43f}\u{435}\u{447}\u{44c} \u{445}\u{43b}\u{435}\u{431}", false),
    ("de", "Brot backen", false),
    ("fr", "Cuire le pain", false),
    ("es", "Hornear el pan", false),
    ("hi", "\u{930}\u{94b}\u{91f}\u{940} \u{92c}\u{928}\u{93e}\u{90f}\u{902}", false),
    ("th", "\u{e2d}\u{e1a}\u{e02}\u{e19}\u{e21}\u{e1b}\u{e31}\u{e07}", false),
];

#[test]
fn eleven_languages_read_translate_and_stamp() {
    let (_, bytes) = corpus_files().into_iter().find(|(n, _)| n == "languages.flprjzip").unwrap();
    let d0 = extract(&bytes);
    let texts = loc_texts(&d0);
    for (_, h, _) in LANGS {
        assert!(texts.iter().any(|x| x == h), "{h}");
    }
    let targets = [
        ("en", false), ("es-ES", false), ("fr", false), ("it", false), ("de", false), ("nl", false), ("pt-BR", false),
        ("ja", false), ("zh-Hant", false), ("ko", false), ("ar", true), ("he", true), ("fa-IR", true), ("ur", true), ("ku-Arab", true),
    ];
    for (loc, rtl) in targets {
        let mut d = extract(&bytes);
        d.target_lang = Some(loc.into());
        pseudo(&mut d, "<<");
        let (m, _) = members(&merge(&d));
        well_formed(&m);
        for (code, _, _) in LANGS {
            let t = text_of(&m, &format!("Content/{code}/Bread.htm"));
            assert!(t.contains(&format!("lang=\"{loc}\"")) && t.contains(&format!("xml:lang=\"{loc}\"")), "{loc} {code}: {t}");
            let html_tag = &t[t.find("<html").unwrap()..];
            let html_tag = &html_tag[..html_tag.find('>').unwrap()];
            assert_eq!(html_tag.contains("dir=\"rtl\""), rtl, "{loc} {code}: {html_tag}");
            assert!(t.contains("&lt;&lt;"), "targets are escaped: {t}");
        }
    }
    // a right-to-left topic turned back to left to right loses its dir
    let mut d = extract(&bytes);
    d.target_lang = Some("en".into());
    pseudo(&mut d, "x");
    let (m, _) = members(&merge(&d));
    assert!(text_of(&m, "Content/ar/Bread.htm").contains("dir=\"ltr\""));
    // each language's own text survives a translated round trip
    let mut d = extract(&bytes);
    for (_, h, _) in LANGS {
        for i in 0..d.units.len() {
            if loc_text(&d.units[i]) == h && d.units[i].translate {
                set(&mut d, i, &format!("{h}!"));
            }
        }
    }
    let (m, _) = members(&merge(&d));
    for (code, h, _) in LANGS {
        assert!(text_of(&m, &format!("Content/{code}/Bread.htm")).contains(&format!("<h1>{h}!</h1>")), "{code}");
    }
}

#[test]
fn utf8_with_a_mark_and_crlf_are_kept() {
    let (_, bytes) = corpus_files().into_iter().find(|(n, _)| n == "rich.flprjzip").unwrap();
    let mut d = extract(&bytes);
    let i = index_of(&d, "Daily care");
    set(&mut d, i, "Soins quotidiens");
    let (m, _) = members(&merge(&d));
    let raw = get(&m, "Content/Oven.htm");
    assert!(raw.starts_with(&[0xEF, 0xBB, 0xBF]));
    let t = text_of(&m, "Content/Oven.htm");
    assert!(t.contains("<h2>Soins quotidiens</h2>\r\n"), "{t}");
    assert!(!t.replace("\r\n", "").contains('\n'), "line ends stay CRLF");
    let o = text_of(&members(&bytes).0, "Content/Oven.htm");
    assert_eq!(t, o.replace("<h2>Daily care</h2>", "<h2>Soins quotidiens</h2>"));
}

#[test]
fn sentence_segments_are_written_back_in_order() {
    let topic = TOPIC.replace("<p>Text for", "<p>First sentence here. Second one has <b>bold</b> text. Third one ends.</p>\n        <p>Text for");
    let mut doc = Flare.extract(&project_with(&[("Content/Topic.htm", topic.as_bytes())], &[]), &ExtractOptions::default()).unwrap();
    bilingua_seg::segment(&mut doc, "en", Granularity::Sentence, bilingua_seg::Rules::bundled());
    let u = doc.units.iter().find(|u| loc_text(u).starts_with("First sentence")).unwrap().clone();
    assert!(u.segments.len() >= 3, "{} segments", u.segments.len());
    for s in &u.segments {
        let id = s.id.clone();
        let (_, map) = to_locos_text(&s.source);
        let t = format!("[{}]", to_locos_text(&s.source).0);
        doc.set_target(&id, from_locos_text(&t, &map)).unwrap();
    }
    let (m, _) = members(&merge(&doc));
    let t = text_of(&m, "Content/Topic.htm");
    assert!(t.contains("[First sentence here.] [Second one has <b>bold</b> text.] [Third one ends.]"), "{t}");
    well_formed(&m);
}

#[test]
fn the_json_form_of_a_document_merges_the_same() {
    let mut d = extract(&project());
    translate_all(&mut d);
    let direct = merge(&d);
    let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(merge(&back), direct);
}

#[test]
fn a_skeleton_from_another_parser_is_refused() {
    let mut d = extract(&project());
    if let Skeleton::Zip { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-docx", "0.1.0", 1));
    }
    assert!(matches!(Flare.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn locked_units_keep_the_ids_consistent_between_extract_and_merge() {
    let topic = TOPIC.replace("<p>Text for", "<p translate=\"no\">Colophon text stays.</p>\n        <p>Text for");
    let src = project_with(&[("Content/Topic.htm", topic.as_bytes())], &[]);
    let o = ExtractOptions { include_locked: true, ..Default::default() };
    let mut d = Flare.extract(&src, &o).unwrap();
    let locked: Vec<Unit> = d.units.iter().filter(|u| !u.translate).cloned().collect();
    assert_eq!(locked.len(), 1);
    assert!(loc_text(&locked[0]).starts_with("Colophon"));
    let i = d.units.iter().position(|u| loc_text(u).starts_with("Brand line")).unwrap();
    set(&mut d, i, "Marque : \u{27E6}x1\u{27E7} fin.");
    let id = locked[0].segments[0].id.clone();
    assert!(matches!(d.set_target(&id, vec![Inline::text("x")]), Err(Error::Locked(_))));
    let (m, _) = members(&merge(&d));
    let t = text_of(&m, "Content/Topic.htm");
    assert!(t.contains("<p translate=\"no\">Colophon text stays.</p>") && t.contains(r#"<p>Marque : <MadCap:snippetText src="../Resources/Snippets/Brand.flsnp" /> fin.</p>"#), "{t}");
}

#[test]
fn the_cached_variable_and_other_point_codes_cannot_be_moved_out_of_the_source() {
    // a translation may move a code, never invent or repeat one
    let mut d = extract(&project());
    let i = d.units.iter().position(|u| loc_text(u).starts_with("Read the")).unwrap();
    let id = d.units[i].segments[0].id.clone();
    assert!(matches!(d.set_target(&id, vec![Inline::tag("9", TagKind::Open, "<x>")]), Err(Error::InventedTag(_))));
    set(&mut d, i, "\u{27E6}x3\u{27E7}Lisez \u{27E6}g2\u{27E7}Autre\u{27E6}/g2\u{27E7} puis \u{27E6}g1\u{27E7}spooler\u{27E6}/g1\u{27E7}.");
    let (m, _) = members(&merge(&d));
    let t = text_of(&m, "Content/Topic.htm");
    assert!(
        t.contains(r#"<p><MadCap:keyword term="Printing;Printing:Color;\:Colon start" />Lisez <MadCap:xref href="Other.htm">Autre</MadCap:xref> puis <MadCap:glossaryTerm glossTerm="Glossary.Term0">spooler</MadCap:glossaryTerm>.</p>"#),
        "{t}"
    );
}

#[test]
fn custom_limits_travel_with_the_skeleton() {
    // a file read under raised limits can be merged again under the same ones
    let big = vec![b'a'; 3 * 1024 * 1024];
    let src = project_with(&[("Content/Resources/pad.bin", &big)], &[]);
    let o = ExtractOptions { limits: Some(Limits { max_zip_ratio: 1_000_000, ..Limits::default() }), ..Default::default() };
    let mut d = Flare.extract(&src, &o).unwrap();
    set(&mut d, 0, "Guide");
    let (m, _) = members(&merge(&d));
    assert!(text_of(&m, "Content/Topic.htm").contains("<title>Guide</title>"));
    assert_eq!(get(&m, "Content/Resources/pad.bin"), big.as_slice());
}

#[test]
fn keywords_with_separators_in_the_translation_stay_one_entry() {
    let mut d = extract(&project());
    let i = d.units.iter().position(|u| u.context.style.as_deref() == Some("flare:index-keyword") && loc_text(u) == "Color").unwrap();
    set(&mut d, i, "Couleur; rouge: vif");
    let (m, _) = members(&merge(&d));
    let t = text_of(&m, "Content/Topic.htm");
    assert!(t.contains(r#"term="Printing;Printing:Couleur, rouge\: vif;\:Colon start""#), "{t}");
}
