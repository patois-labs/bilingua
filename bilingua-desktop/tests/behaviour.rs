//! Round trips, the 11 languages, stamping rules, ids, modes and the facade-facing details.

mod common;
use bilingua_desktop::*;
use bilingua_model::*;
use common::*;

const LANGS: [&str; 11] = ["en", "de", "fr", "es", "ru", "ar", "ja", "zh", "ko", "tr", "hi"];

fn lang_fixture(code: &str, ext: &str) -> Vec<u8> {
    fixtures().into_iter().find(|(n, _)| *n == format!("lang_{code}.{ext}")).unwrap().1
}

#[test]
fn every_fixture_extracts_and_an_untouched_merge_is_byte_exact() {
    let all = fixtures();
    assert!(all.len() >= 29);
    for (name, bytes) in &all {
        let d = extract_any(name, bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!d.units.is_empty(), "{name}");
        assert_eq!(merge(&d), *bytes, "{name}");
        // with a target language set, nothing translated still returns the original bytes
        let mut d2 = d.clone();
        d2.target_lang = Some("de-DE".into());
        assert_eq!(merge(&d2), *bytes, "{name} with target_lang");
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_the_same_units() {
    for (name, bytes) in fixtures() {
        let mut d = extract_any(&name, &bytes).unwrap();
        let n = d.units.len();
        let src = texts(&d);
        pseudo(&mut d);
        // no target language: the values are spliced in place (.desktop) / the translation column filled
        let out = merge(&d);
        let back = extract_any(&name, &out).unwrap();
        if name.ends_with(".desktop") {
            assert_eq!(back.units.len(), n, "{name}");
            let want: Vec<String> = src.iter().map(|t| format!("\u{ab}{t}\u{bb}")).collect();
            assert_eq!(texts(&back), want, "{name}");
        } else {
            // catkeys: the source column never changes; the translation column carries the pseudo text
            assert_eq!(texts(&back), src, "{name}");
            let recs = catkeys_records(&out).unwrap();
            let tr: Vec<&str> = recs.iter().filter(|r| r.unit_index.is_some()).map(|r| r.value.as_str()).collect();
            let want: Vec<String> = src.iter().map(|t| format!("\u{ab}{t}\u{bb}")).collect();
            assert_eq!(tr, want, "{name}");
        }
    }
}

#[test]
fn target_locale_merge_writes_localized_lines_and_header_language() {
    for (name, bytes) in fixtures() {
        let mut d = extract_any(&name, &bytes).unwrap();
        d.target_lang = Some("fr-FR".into());
        let n = d.units.len();
        pseudo(&mut d);
        let out = merge(&d);
        let back = extract_any(&name, &out).unwrap();
        assert_eq!(back.units.len(), n, "{name}");
        assert_eq!(texts(&back), texts(&extract_any(&name, &bytes).unwrap()), "{name}: sources unchanged");
        let text = s(&out);
        if name.ends_with(".desktop") {
            assert!(text.contains("[fr_FR]=\u{ab}"), "{name}: {text}");
        } else {
            assert!(text.contains("\tfrench (france)\t"), "{name}: {text}");
        }
    }
}

#[test]
fn eleven_languages_round_trip_and_localize() {
    let targets = [
        ("en", "english"),
        ("de", "german"),
        ("fr", "french"),
        ("es", "spanish"),
        ("ru", "russian"),
        ("ar", "arabic"),
        ("ja", "japanese"),
        ("zh-Hans", "chinese (simplified)"),
        ("ko", "korean"),
        ("tr", "turkish"),
        ("hi", "hindi"),
    ];
    for code in LANGS {
        for ext in ["desktop", "catkeys"] {
            let bytes = lang_fixture(code, ext);
            let mut d = extract_any(&format!("x.{ext}"), &bytes).unwrap();
            assert!(d.units.len() >= 3, "{code}.{ext}");
            assert!(d.units.iter().all(|u| !flat(&u.source()).is_empty()));
            assert_eq!(merge(&d), bytes);
            // translate into every other language and read it back
            for (loc, name) in targets {
                d.target_lang = Some(loc.into());
                for i in 0..d.units.len() {
                    set(&mut d, i, &format!("{loc}:{i}"));
                }
                let out = s(&merge(&d));
                if ext == "desktop" {
                    let p = posix_locale(loc);
                    assert!(out.contains(&format!("Name[{p}]={loc}:0\n")), "{code}->{loc}: {out}");
                } else {
                    assert!(out.contains(&format!("\t{name}\t")) || out.starts_with(&format!("1\t{name}\t")), "{code}->{loc}: {out}");
                    assert!(out.contains(&format!("\t{loc}:0\n")), "{out}");
                }
                assert_eq!(texts(&extract_any(&format!("x.{ext}"), out.as_bytes()).unwrap()), texts(&d).iter().map(String::as_str).map(str::to_string).collect::<Vec<_>>());
            }
        }
    }
}

#[test]
fn unit_ids_are_stable_and_context_is_filled() {
    let a = desktop(LOCOS_DESKTOP).unwrap();
    let b = desktop(LOCOS_DESKTOP).unwrap();
    assert_eq!(a, b);
    let ids: Vec<&str> = a.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u3", "u4", "u5", "u6", "u7"]);
    assert_eq!(a.units[0].context.key.as_deref(), Some("Desktop Entry/Name"));
    assert_eq!(a.units[0].context.notes, ["Shown in menus"]);
    assert_eq!(a.units[4].context.key.as_deref(), Some("Desktop Entry/Keywords[1]"));
    assert_eq!(a.units[6].context.extra["group"], "Desktop Action new-window");
    assert!(a.units.iter().all(|u| u.segments.len() == 1 && u.translate));
    assert_eq!(a.format, "desktop");
    let c = catkeys(LOCOS_CATKEYS).unwrap();
    let ids: Vec<&str> = c.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u3", "u4"]);
    assert_eq!(c.units[1].context.notes, ["file name"]);
    assert_eq!(c.units[1].context.extra["context"], "MainWindow");
    assert_eq!(c.units[3].context.extra["translation"], "Beenden");
    assert!(c.units.iter().all(|u| u.segments[0].target.is_empty()));
}

#[test]
fn no_inline_tags_placeholders_stay_text() {
    let c = catkeys(LOCOS_CATKEYS).unwrap();
    for u in c.units.iter().chain(desktop(LOCOS_DESKTOP).unwrap().units.iter()) {
        assert!(u.segments[0].source.iter().all(|i| matches!(i, Inline::Text(_))));
    }
    assert_eq!(flat(&c.units[1].source()), "Save %s?");
}

#[test]
fn paragraph_sentence_and_document_modes_round_trip() {
    for g in [Granularity::Paragraph, Granularity::Sentence, Granularity::Document] {
        for (name, bytes) in [("a.desktop", LOCOS_DESKTOP), ("a.catkeys", LOCOS_CATKEYS)] {
            let o = ExtractOptions::new().with_granularity(g).with_source_lang("en");
            let mut d = if name.ends_with("desktop") { DesktopFilter.extract(bytes, &o) } else { CatkeysFilter.extract(bytes, &o) }.unwrap();
            bilingua_seg::segment(&mut d, "en", g, bilingua_seg::Rules::bundled());
            assert_eq!(merge(&d), bytes, "{name} {g:?}");
            let n = d.units.len();
            d.target_lang = Some("de".into());
            for u in d.units.iter_mut() {
                for seg in u.segments.iter_mut() {
                    let t = flat(&seg.source);
                    seg.target = vec![Inline::text(format!("<{t}>"))];
                }
            }
            let out = merge(&d);
            assert_eq!(extract_any(name, &out).unwrap().units.len(), n, "{name} {g:?}");
        }
    }
}

#[test]
fn only_translated_units_are_written_in_a_localized_merge() {
    // Name and Comment translated, GenericName not: no GenericName[de] line
    let out = s(&build_d(LOCOS_DESKTOP, &[(0, "Texteditor 2"), (2, "Bearbeiten")], Some("it")));
    assert!(out.contains("Name=Text Editor\nName[it]=Texteditor 2\n"));
    assert!(out.contains("Comment[it]=Bearbeiten\n"));
    assert!(!out.contains("GenericName[it]"));
    // an untranslated list item is kept in its raw spelling
    let out = s(&build_d(LOCOS_DESKTOP, &[(4, "Bearbeiter")], Some("de")));
    assert!(out.contains("Keywords[de]=text;Bearbeiter;plain\\;simple;\n"), "{out}");
    // a translation equal to its source is still a translation in a localized merge (LocOS does the same)
    let out = s(&build_d(LOCOS_DESKTOP, &[(0, "Text Editor")], Some("de")));
    assert!(out.contains("Name[de]=Text Editor\n") && !out.contains("Texteditor"), "{out}");
    // ... but in the in-place merge only a changed value is written
    assert_eq!(build_d(LOCOS_DESKTOP, &[(0, "Text Editor")], None), LOCOS_DESKTOP);
}

#[test]
fn catkeys_header_language_is_set_once_something_is_translated() {
    let out = s(&build_c(LOCOS_CATKEYS, &[(3, "Beenden")], Some("de")));
    // the translation equals the existing one, so only the header changes
    assert!(out.starts_with("1\tgerman\tx-vnd.Example-App\t123456789\n"));
    assert_eq!(out.len(), LOCOS_CATKEYS.len() + "german".len() - "english".len());
    // the fingerprint is kept
    assert_eq!(catkeys_fingerprint(out.as_bytes()).unwrap().0, 123456789);
    // no target language: the header stays
    let out = s(&build_c(LOCOS_CATKEYS, &[(0, "X")], None));
    assert!(out.starts_with("1\tenglish\t"));
}

#[test]
fn escapes_in_catkeys_translations() {
    let out = s(&build_c(LOCOS_CATKEYS, &[(0, "a\tb\nc\"d\\e\rf")], None));
    assert!(out.contains("Open\tMainWindow\t\ta\\tb\\nc\\\"d\\\\e\\rf\n"), "{out}");
    let recs = catkeys_records(out.as_bytes()).unwrap();
    assert_eq!(recs[0].value, "a\tb\nc\"d\\e\rf");
    assert_eq!(catkeys_unescape("\\x41\\q\\a\\xZ"), "Aq\u{7}xZ");
    assert_eq!(catkeys_escape("\""), "\\\"");
}

#[test]
fn include_locked_adds_letterless_catkeys_entries_as_locked_units() {
    let o = ExtractOptions::new().with_include_locked(true);
    let d = CatkeysFilter.extract(LOCOS_CATKEYS, &o).unwrap();
    assert_eq!(d.units.len(), 5);
    assert!(!d.units[2].translate && d.units[2].lock_reason.is_some());
    assert_eq!(CatkeysFilter.merge(&d).unwrap(), LOCOS_CATKEYS);
}

#[test]
fn sniffing_is_tight() {
    assert!(DesktopFilter.sniff(LOCOS_DESKTOP));
    assert!(DesktopFilter.sniff(b"\xef\xbb\xbf[Desktop Entry]\nName=x\n"));
    assert!(!DesktopFilter.sniff(b"[section]\nkey=value\n"));
    assert!(!DesktopFilter.sniff(LOCOS_CATKEYS));
    assert!(CatkeysFilter.sniff(LOCOS_CATKEYS));
    assert!(CatkeysFilter.sniff(b"1\tportuguese (brazil)\tx-vnd.A\t5\r\nA\t\t\tB\r\n"));
    assert!(!CatkeysFilter.sniff(b"1\t2\t3\t4\n"));
    assert!(!CatkeysFilter.sniff(b"hello world\n"));
    assert!(!CatkeysFilter.sniff(LOCOS_DESKTOP));
    assert!(!CatkeysFilter.sniff(b""));
}

#[test]
fn a_document_of_the_other_format_is_refused_on_merge() {
    let d = desktop(LOCOS_DESKTOP).unwrap();
    assert!(CatkeysFilter.merge(&d).is_err());
    let c = catkeys(LOCOS_CATKEYS).unwrap();
    assert!(DesktopFilter.merge(&c).is_err());
}

#[test]
fn duplicate_keys_and_odd_layouts() {
    // the same key twice: both are units, the second is flagged
    let data = b"[Desktop Entry]\nName=A\nName=B\n";
    let d = desktop(data).unwrap();
    assert_eq!(d.units.len(), 2);
    assert_eq!(d.units[1].context.extra.get("duplicate").map(String::as_str), Some("true"));
    // empty values and whitespace-only values are not units
    let d = desktop(b"[Desktop Entry]\nName=\nComment=\\s\nKeywords=;;a\n").unwrap();
    assert_eq!(texts(&d), ["a"]);
    assert_eq!(merge(&d), b"[Desktop Entry]\nName=\nComment=\\s\nKeywords=;;a\n");
    // keys in a non-launcher group and in a group before [Desktop Entry] are not units
    let d = desktop(b"[Other]\nName=x\n[Desktop Entry]\nName=y\n").unwrap();
    assert_eq!(texts(&d), ["y"]);
}
