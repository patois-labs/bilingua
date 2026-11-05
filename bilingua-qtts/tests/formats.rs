//! Round trips on every fixture, the 11 languages and their legacy encodings, target-language merges (language
//! attribute, numerus forms), the LocOS golden file, Okapi's test resources, and the details of what is kept byte
//! for byte.

mod common;
use bilingua_model::*;
use bilingua_qtts::QtTs;
use common::*;

fn json(name: &str) -> serde_json::Value {
    serde_json::from_slice(&data(name)).unwrap()
}

// -- round trips ------------------------------------------------------------------------------------------------

#[test]
fn untouched_merge_gives_back_the_exact_bytes_of_every_fixture() {
    for (name, bytes) in all_data().into_iter().chain(okapi_data()) {
        let d = QtTs.extract(&bytes, &opts(None)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(merged(&d), bytes, "{name}");
        // with locked units and a target language the file is still the same file
        let d = QtTs.extract(&bytes, &(opts(Some("es-ES"))).with_include_locked(true)).unwrap();
        assert_eq!(merged(&d), bytes, "{name} (target set)");
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_the_same_units_and_tags() {
    for (name, bytes) in all_data() {
        let mut d = ex(&bytes);
        let before: Vec<Vec<Inline>> = d.units.iter().map(|u| u.source()).collect();
        pseudo(&mut d);
        let out = merged(&d);
        let back = ex(&out);
        let after: Vec<Vec<Inline>> = back.units.iter().map(|u| u.source()).collect();
        assert_eq!(before, after, "{name}: the sources must survive");
        assert_eq!(d.units.len(), back.units.len(), "{name}");
        let text = decoded(&out);
        assert!(text.matches('\u{ab}').count() + text.matches("&#171;").count() >= d.units.len(), "{name}: every unit carries its translation");
        assert!(!text.contains("type=\"unfinished\"") || name.contains("edge"), "{name}");
    }
}

#[test]
fn eleven_languages_and_their_encodings_extract_the_same_five_units() {
    let langs = ["en", "de", "fr", "es", "pt", "ru", "ar", "ja", "zh", "ko", "tr"];
    for l in langs {
        let d = ex(&data(&format!("lang_{l}.ts")));
        assert_eq!(d.units.len(), 5, "{l}");
        assert_eq!(token_text(&d.units[0]), "&Open file...");
        assert_eq!(d.units[4].segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 1);
    }
    for (name, bytes) in all_data() {
        if name.starts_with("lang_") {
            let d = ex(&bytes);
            assert_eq!(texts(&d), texts(&ex(&data("lang_en.ts"))), "{name}");
        }
    }
}

#[test]
fn translation_in_each_language_survives_every_encoding() {
    // translate with the language's own text, merge, read back through the decoder
    let words = [
        ("lang_de_latin1.ts", "Datei öffnen", "iso-8859-1"),
        ("lang_fr_cp1252.ts", "Œuvre « ouverte »", "windows-1252"),
        ("lang_ru_cp1251.ts", "Открыть файл", "windows-1251"),
        ("lang_ru_koi8r.ts", "Открыть файл", "koi8-r"),
        ("lang_ar_cp1256.ts", "فتح الملف", "windows-1256"),
        ("lang_tr_cp1254.ts", "Dosyayı aç", "windows-1254"),
        ("lang_ja_shiftjis.ts", "ファイルを開く", "shift_jis"),
        ("lang_ja_eucjp.ts", "ファイルを開く", "euc-jp"),
        ("lang_ko_euckr.ts", "파일 열기", "euc-kr"),
        ("lang_zh_gbk.ts", "打开文件", "gbk"),
        ("lang_zh_gb2312.ts", "打开文件", "gbk"),
        ("lang_ar_utf16le.ts", "فتح الملف", "utf-16le"),
        ("lang_ko_utf16be.ts", "파일 열기", "utf-16be"),
        ("lang_zh_utf16le.ts", "打开文件", "utf-16le"),
        ("lang_en_utf8bom_crlf.ts", "Open file", "utf-8-bom"),
    ];
    for (name, word, enc_tag) in words {
        let mut d = ex(&data(name));
        let Skeleton::Text { encoding, .. } = &d.skeleton else { panic!() };
        assert!(enc::base(encoding) == enc_tag, "{name}: {encoding} vs {enc_tag}");
        set(&mut d, 0, word);
        let out = merged(&d);
        assert!(decoded(&out).contains(&format!("<translation>{word}</translation>")), "{name}");
        // the file stays in its encoding: re-reading finds the same encoding tag
        assert_eq!(ex(&out).skeleton, d.skeleton, "{name}");
    }
}

#[test]
fn a_character_the_code_page_cannot_hold_becomes_a_character_reference() {
    let mut d = ex(&data("lang_de_latin1.ts"));
    set(&mut d, 0, "ファイル");
    let out = merged(&d);
    let t = decoded(&out);
    assert!(t.contains("<translation>&#12501;&#12449;&#12452;&#12523;</translation>"), "{t}");
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>"));
}

#[test]
fn us_ascii_files_stay_ascii() {
    let src = s(&lf("locos_app_de.ts")).replace("encoding=\"utf-8\"", "encoding=\"us-ascii\"");
    let mut d = ex(src.as_bytes());
    set(&mut d, 0, "Öffnen €");
    let out = merged(&d);
    assert!(out.is_ascii(), "{}", s(&out));
    assert!(s(&out).contains("&#214;ffnen &#8364;"));
}

// -- target language -----------------------------------------------------------------------------------------------

#[test]
fn target_language_is_written_into_the_ts_element_only_when_something_was_translated() {
    let data = lf("locos_app_de.ts");
    let mut d = ex_to(&data, "fr-CA");
    assert_eq!(merged(&d), data, "nothing translated: nothing stamped");
    set(&mut d, 0, "Ouvrir");
    let out = s(&merged(&d));
    assert!(out.contains("<TS version=\"2.1\" language=\"fr_CA\" sourcelanguage=\"en_US\">"), "{out}");
    assert!(!out.contains("de_DE"));
}

#[test]
fn the_language_attribute_keeps_its_quote_style_and_a_missing_one_is_added() {
    let mut d = ex_to(&data("edge.ts"), "ja-JP");
    set(&mut d, 0, "x");
    let out = s(&merged(&d));
    assert!(out.contains("<TS version='2.1' sourcelanguage='en_US'  language='ja_JP' >"), "{out}");
    let none = "<TS version=\"2.1\">\n<context><name>A</name><message><source>Hi</source><translation type=\"unfinished\"/></message></context>\n</TS>\n";
    let mut d = ex_to(none.as_bytes(), "zh-Hans");
    set(&mut d, 0, "你好");
    assert!(s(&merged(&d)).contains("<TS version=\"2.1\" language=\"zh_Hans\">"));
    let selfclosing = "<TS/>";
    assert!(QtTs.extract(selfclosing.as_bytes(), &opts(Some("de"))).unwrap().units.is_empty());
}

#[test]
fn an_unusable_target_tag_leaves_the_language_alone() {
    let data = lf("locos_app_de.ts");
    for bad in ["*", "a b", "x", "\"><x"] {
        let mut d = ex_to(&data, bad);
        set(&mut d, 0, "Ouvrir");
        let out = s(&merged(&d));
        assert!(out.contains("language=\"de_DE\""), "{bad}: {out}");
    }
}

#[test]
fn a_target_in_the_files_own_language_is_not_rewritten() {
    let data = lf("locos_app_de.ts");
    for same in ["de_DE", "de-de", "DE-DE"] {
        let mut d = ex_to(&data, same);
        set(&mut d, 0, "Öffnen");
        assert!(s(&merged(&d)).contains("language=\"de_DE\""), "{same}");
    }
    // the file's own language in another spelling is rewritten the Qt way, as LocOS does
    let odd = String::from_utf8(data.clone()).unwrap().replace("language=\"de_DE\"", "language=\"DE-de\"");
    let mut d = ex_to(odd.as_bytes(), "de-DE");
    set(&mut d, 0, "Öffnen");
    let out = s(&merged(&d));
    assert!(out.contains("language=\"de_DE\"") && !out.contains("DE-de"), "{out}");
}

#[test]
fn numerus_forms_created_for_each_of_the_eleven_languages() {
    let bare = "<TS version=\"2.1\"><context><name>A</name><message numerus=\"yes\"><source>%n file(s)</source><translation type=\"unfinished\"/></message></context></TS>";
    let want = [
        ("en", 2),
        ("de-DE", 2),
        ("fr-FR", 2),
        ("es-ES", 2),
        ("pt-BR", 2),
        ("ru-RU", 3),
        ("ar-SA", 6),
        ("ja-JP", 1),
        ("zh-CN", 1),
        ("ko-KR", 1),
        ("tr-TR", 2),
        ("pl", 3),
        ("cs", 3),
        ("he", 3),
        ("sl", 4),
        ("cy", 6),
        ("xx-unknown", 2),
    ];
    for (loc, n) in want {
        let mut d = ex_to(bare.as_bytes(), loc);
        set(&mut d, 0, "F %n");
        let out = s(&merged(&d));
        assert_eq!(out.matches("<numerusform>F %n</numerusform>").count(), n, "{loc}: {out}");
    }
}

#[test]
fn numerus_uses_the_files_language_when_there_is_no_target() {
    let bare = "<TS version=\"2.1\" language=\"ru_RU\"><context><name>A</name><message numerus=\"yes\"><source>%n file(s)</source><translation type=\"unfinished\"/></message></context></TS>";
    let mut d = ex(bare.as_bytes());
    set(&mut d, 0, "F %n");
    assert_eq!(s(&merged(&d)).matches("<numerusform>").count(), 3);
    let no_lang = bare.replace(" language=\"ru_RU\"", "");
    let mut d = ex(no_lang.as_bytes());
    set(&mut d, 0, "F %n");
    assert_eq!(s(&merged(&d)).matches("<numerusform>").count(), 2, "no language: one and other");
}

#[test]
fn existing_numerus_forms_keep_their_layout_and_empty_ones_open_up() {
    let mut d = ex(&data("edge.ts"));
    let k = d.units.iter().position(|u| token_text(u) == "%n item(s)").unwrap();
    set(&mut d, k, "N");
    let out = s(&merged(&d));
    assert!(out.contains("<translation>\n            <numerusform>N</numerusform>\n            <numerusform>N</numerusform>\n        </translation>"), "{out}");
    let k = d.units.iter().position(|u| token_text(u) == "%n tight").unwrap();
    set(&mut d, k, "T");
    let out = s(&merged(&d));
    assert!(out.contains("<translation><numerusform>T</numerusform><numerusform>T</numerusform></translation>"), "{out}");
    // the uppercase YES message is numerus too
    let k = d.units.iter().position(|u| token_text(u) == "%n thing(s) in a row").unwrap();
    set(&mut d, k, "Y");
    assert!(s(&merged(&d)).contains("<numerusform>Y</numerusform><numerusform>Y</numerusform>"));
}

#[test]
fn tight_forms_are_topped_up_without_invented_white_space() {
    let mut d = ex_to(&data("edge.ts"), "ru");
    let k = d.units.iter().position(|u| token_text(u) == "%n tight").unwrap();
    set(&mut d, k, "T");
    let out = s(&merged(&d));
    assert!(out.contains("<translation><numerusform>T</numerusform><numerusform>T</numerusform><numerusform>T</numerusform></translation>"), "{out}");
}

// -- LocOS golden file ----------------------------------------------------------------------------------------------

#[test]
fn units_and_merges_equal_the_localization_os_filter() {
    let gold = json("locos_golden.json");
    let mut merges = 0;
    for (key, ent) in gold.as_object().unwrap() {
        let bytes = data(key);
        let d = QtTs.extract(&bytes, &opts(None)).unwrap_or_else(|e| panic!("{key}: {e}"));
        let got: Vec<(usize, String)> = d.units.iter().map(|u| (u.context.extra["p_index"].parse().unwrap(), token_text(u))).collect();
        let want: Vec<(usize, String)> =
            ent["units"].as_array().unwrap().iter().map(|p| (p[0].as_u64().unwrap() as usize, p[1].as_str().unwrap().to_string())).collect();
        assert_eq!(got, want, "{key}: units");
        let Some(m) = ent.get("merges") else { continue };
        for (loc, want) in m.as_object().unwrap() {
            let mut d = QtTs.extract(&bytes, &opts(if loc.is_empty() { None } else { Some(loc) })).unwrap();
            translate_all(&mut d, |s| format!("T:{s}"));
            // LocOS translates the whole token text "T:" + text; a source with a placeholder has the T: once
            for u in d.units.iter_mut() {
                let mut t = vec![Inline::text("T:")];
                t.extend(u.source());
                u.segments[0].target = merge_text(t);
            }
            assert_eq!(s(&merged(&d)), want.as_str().unwrap(), "{key} for {loc:?}");
            merges += 1;
        }
    }
    assert!(merges >= 60, "{merges}");
}

// -- Okapi okf_ts resources -----------------------------------------------------------------------------------------

/// Okapi's text units for its own test resources, from the Okapi oracle (Tikal `okf_ts`, source text, inline
/// codes as U+00A4, white space collapsed): see the README for how it was made. Kept with the fixtures.
#[test]
fn okapi_resources_match_okapi_except_where_locos_decides_otherwise() {
    let gold = json("okapi_units.json");
    let norm = |s: &str| {
        let s: String = s.replace(['\u{27e6}'], "").chars().collect();
        s
    };
    let _ = norm;
    for (name, ent) in gold.as_object().unwrap() {
        let d = ex_locked(&data(&format!("okapi/{name}")));
        let ours: Vec<String> = d.units.iter().filter(|u| u.translate).map(|u| collapse(&u.source())).collect();
        let theirs: Vec<String> = ent["units"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let allowed = ent["differs"].as_str().unwrap_or("");
        if allowed.is_empty() {
            assert_eq!(ours, theirs, "{name}");
        } else {
            // the documented differences: every one of our units is one of Okapi's, in order
            let mut it = theirs.iter();
            for o in &ours {
                assert!(it.any(|t| t == o), "{name}: {o:?} is not in Okapi's units; reason on file: {allowed}");
            }
        }
    }
}

/// Okapi's normal form: tags as one marker, white space collapsed.
fn collapse(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { .. } => s.push('\u{a4}'),
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// -- details --------------------------------------------------------------------------------------------------------

#[test]
fn entities_cdata_and_xml_odds_and_ends_in_the_source() {
    let d = ex(&data("edge.ts"));
    let t = texts(&d);
    assert_eq!(t[0], "Less < more > \u{e9}t\u{e9} \u{3042}");
    assert_eq!(t[1], "Raw <b>markup</b> & more");
    assert_eq!(t[2], "With \u{27e6}x1\u{27e7} comment and \u{27e6}x2\u{27e7} instruction");
    assert_eq!(t[3], "padded text");
    assert!(!t.iter().any(|x| x == "---" || x.trim().is_empty()), "no letters, no unit");
    assert_eq!(t[4], "\u{27e6}x1\u{27e7}", "a lone placeholder is a unit, as in LocOS");
    assert!(!t.contains(&"Gone".to_string()) && !t.contains(&"Old".to_string()));
    assert!(!t.contains(&"No translation element".to_string()), "LocOS skips a message with no <translation>");
    assert!(t.contains(&"Multi\nline &text".to_string()));
    assert!(t.contains(&"Same \"quoted\" 'text'".to_string()));
}

#[test]
fn white_space_around_the_source_is_kept_and_markup_comes_back_as_written() {
    let mut d = ex(&data("edge.ts"));
    let k = d.units.iter().position(|u| token_text(u) == "padded text").unwrap();
    set(&mut d, k, "gepolstert");
    let k2 = d.units.iter().position(|u| token_text(u).starts_with("With ")).unwrap();
    d.units[k2].segments[0].target = vec![
        Inline::text("Mit "),
        d.units[k2].segments[0].source[3].clone(),
        Inline::text(" und "),
        d.units[k2].segments[0].source[1].clone(),
    ];
    let out = s(&merged(&d));
    assert!(out.contains("<translation>   gepolstert   </translation>"), "{out}");
    assert!(out.contains("<translation>Mit <?pi data?> und <!-- a note --></translation>"), "{out}");
}

#[test]
fn tags_the_source_lacks_or_repeats_are_dropped_on_write() {
    let mut d = ex(&lf("locos_app_de.ts"));
    let k = d.units.iter().position(|u| token_text(u).starts_with("Tab")).unwrap();
    let tag = d.units[k].segments[0].source[1].clone();
    let invented = Inline::tag("99", TagKind::Standalone, "<x/>");
    d.units[k].segments[0].target = vec![Inline::text("A"), tag.clone(), tag.clone(), invented, Inline::text("B")];
    let out = s(&merged(&d));
    assert!(out.contains("<translation>A<byte value=\"x9\"/>B</translation>"), "{out}");
}

#[test]
fn text_is_escaped_and_xml_forbidden_characters_are_dropped() {
    let mut d = ex(&lf("locos_app_de.ts"));
    set(&mut d, 0, "a < b & c > d \u{1}\u{b}e\u{c}f");
    let out = s(&merged(&d));
    assert!(out.contains("<translation>a &lt; b &amp; c &gt; d \ne\nf</translation>"), "{out}");
    // and it reads back
    assert_eq!(ex(out.as_bytes()).units.len(), 4);
}

#[test]
fn new_text_uses_the_files_line_ends() {
    let crlf = s(&lf("locos_app_de.ts")).replace('\n', "\r\n");
    let mut d = ex(crlf.as_bytes());
    assert_eq!(merged(&d), crlf.as_bytes());
    set(&mut d, 0, "eins\nzwei");
    assert!(s(&merged(&d)).contains("<translation>eins\r\nzwei</translation>"));
    let mut d = ex(&lf("locos_app_de.ts"));
    set(&mut d, 0, "eins\r\nzwei");
    assert!(s(&merged(&d)).contains("<translation>eins\nzwei</translation>"));
}

#[test]
fn the_translation_start_tag_loses_only_the_unfinished_type() {
    let mut d = ex(&data("edge.ts"));
    set(&mut d, 0, "x");
    let out = s(&merged(&d));
    // type='unfinished' with single quotes and variants="no" next to it
    assert!(out.contains("<translation variants=\"no\">x</translation>"), "{out}");
    let k = d.units.iter().position(|u| token_text(u) == "Same \"quoted\" 'text'").unwrap();
    set(&mut d, k, "Gleich");
    assert!(s(&merged(&d)).contains("<translation>Gleich</translation>"));
}

#[test]
fn unit_ids_are_stable_whether_or_not_locked_units_are_listed() {
    let data = data("edge.ts");
    let a = ex(&data);
    let b = ex_locked(&data);
    for u in &a.units {
        let same = b.units.iter().find(|x| x.id == u.id).expect("the translatable unit keeps its id");
        assert_eq!(same.source(), u.source());
    }
    assert!(b.units.len() > a.units.len());
    assert!(b.units.iter().any(|u| !u.translate && u.id.starts_with('k')));
}

#[test]
fn notes_keys_locations_and_languages() {
    let d = ex(&data("edge.ts"));
    let multi = d.units.iter().find(|u| token_text(u).starts_with("Multi")).unwrap();
    assert_eq!(multi.context.notes, vec!["spaced comment".to_string(), "extra".to_string()]);
    assert_eq!(multi.context.location.as_deref(), Some("../src/a.cpp:7"));
    assert_eq!(multi.context.extra["context"], "Edge & Co");
    let ent = &d.units[0];
    assert_eq!(ent.context.key.as_deref(), Some("entities"), "the id attribute is the key");
    assert_eq!(d.source_lang.as_deref(), Some("en"));
    assert_eq!(ex(&data("lang_de.ts")).target_lang, None, "the target language is the caller's");
}

#[test]
fn nested_context_messages_are_not_read_as_locos_does() {
    // Okapi's "Complete_valid" file nests a context in a context; LocOS reads the messages of top-level contexts only
    let d = ex(&data("okapi/Complete_valid_utf8_bom_crlf.ts"));
    assert!(d.units.is_empty());
}

#[test]
fn skeleton_is_stamped_and_a_foreign_one_is_refused() {
    let mut d = ex(&lf("locos_app_de.ts"));
    let p = d.skeleton.parser().cloned().unwrap();
    assert_eq!(p.name, "bilingua-qtts");
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-docx", "0.1.0", 1));
    }
    assert!(matches!(QtTs.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn paragraph_mode_and_document_json_round_trip() {
    let d = ex(&lf("locos_app_de.ts"));
    let j = serde_json::to_string(&d).unwrap();
    let back: Document = serde_json::from_str(&j).unwrap();
    assert_eq!(merged(&back), lf("locos_app_de.ts"));
    for u in &d.units {
        assert_eq!(u.segments.len(), 1);
        assert_eq!(u.segments[0].id, u.id);
    }
}

#[test]
fn sniff_is_strict() {
    for (name, bytes) in all_data() {
        assert!(QtTs.sniff(&bytes), "{name}");
    }
    assert!(QtTs.sniff(b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<!-- c -->\n<!DOCTYPE TS>\n<TS version=\"2.1\">"));
    assert!(QtTs.sniff(b"<TS>"));
    assert!(QtTs.sniff(b"<!DOCTYPE TS><TS/>"));
    assert!(!QtTs.sniff(b"<TSX/>"));
    assert!(!QtTs.sniff(b"<ts>"));
    assert!(!QtTs.sniff(b"<html><TS></html>"));
    assert!(!QtTs.sniff(b"const a = '<TS>';"));
    assert!(!QtTs.sniff(b"<?xml version=\"1.0\"?><xliff><TS/></xliff>"));
}
