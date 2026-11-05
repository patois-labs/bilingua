//! Fixture files in `tests/data`: round trips, the 11 languages and every encoding the filter reads.

mod common;

use bilingua_model::*;
use bilingua_properties::Properties;
use common::*;

const FILES: [(&str, &[u8]); 7] = [
    ("locos_suite_ascii", include_bytes!("data/locos_suite_ascii.properties")),
    ("languages_utf8", include_bytes!("data/languages_utf8.properties")),
    ("languages_native2ascii", include_bytes!("data/languages_native2ascii.properties")),
    ("languages_utf8_bom_crlf", include_bytes!("data/languages_utf8_bom_crlf.properties")),
    ("languages_latin1", include_bytes!("data/languages_latin1.properties")),
    ("mozilla_downloads", include_bytes!("data/mozilla_downloads.properties")),
    ("edge", include_bytes!("data/edge.properties")),
];

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => encoding.clone(),
        _ => String::new(),
    }
}

#[test]
fn locos_fixture_file_is_the_suite_fixture() {
    let inline: &[u8] = b"# Greeting shown on the home page\r\n! second comment style\r\napp.title = Welcome to Northwind\r\n";
    assert!(FILES[0].1.starts_with(inline));
    assert_eq!(units(FILES[0].1).len(), 14);
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in FILES {
        let d = extract(data);
        assert_eq!(Properties.merge(&d).unwrap(), data, "{name}");
        assert_identity(data);
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in FILES {
        let before = extract(data);
        let mapping: Vec<(usize, String)> = before.units.iter().enumerate().map(|(i, u)| (i, format!("[{}]", show(u, &u.source())))).collect();
        let m: Vec<(usize, &str)> = mapping.iter().map(|(i, s)| (*i, s.as_str())).collect();
        let out = translate(data, &m);
        let after = extract(&out);
        assert_eq!(after.units.len(), before.units.len(), "{name}");
        assert_eq!(tag_count(&after), tag_count(&before), "{name}");
        assert_eq!(encoding(&after), encoding(&before), "{name}: encoding mode kept");
        for (i, (a, b)) in after.units.iter().zip(&before.units).enumerate() {
            assert_eq!(show(a, &a.source()), format!("[{}]", show(b, &b.source())), "{name} unit {i}");
            assert_eq!(a.context.key, b.context.key, "{name}: keys never change");
        }
    }
}

#[test]
fn the_eleven_languages_survive_in_every_mode() {
    let langs = ["en", "es", "fr", "it", "de", "nl", "pt", "ja", "zh", "ko", "ar"];
    for (name, data) in [FILES[1], FILES[2], FILES[3]] {
        let d = extract(data);
        assert_eq!(d.units.len(), 33, "{name}");
        // every language's greeting carries its own script
        for (k, l) in langs.iter().enumerate() {
            assert_eq!(d.units[k * 3].context.key.as_deref(), Some(format!("greeting.{l}").as_str()), "{name}");
        }
        let texts = units(data);
        assert!(texts[7 * 3].contains('\u{3078}'), "{name}: Japanese");
        assert!(texts[8 * 3].contains('\u{6b22}'), "{name}: Chinese");
        assert!(texts[9 * 3].contains('\u{c5d0}'), "{name}: Korean");
        assert!(texts[10 * 3].contains('\u{645}'), "{name}: Arabic");
        // The three encodings carry the same text.
        assert_eq!(texts, units(FILES[1].1), "{name}");
        // Translate every unit into a different language and read it back.
        let mapping: Vec<(usize, String)> = (0..33).map(|i| (i, texts[(i + 3 * 7) % 33].clone())).collect();
        let m: Vec<(usize, &str)> = mapping.iter().map(|(i, s)| (*i, s.as_str())).collect();
        let out = translate(data, &m);
        let back = units(&out);
        for (i, s) in back.iter().enumerate() {
            assert_eq!(s, &mapping[i].1, "{name} unit {i}");
        }
        match name {
            "languages_native2ascii" => assert!(out.is_ascii(), "all-ASCII file stays all-ASCII"),
            "languages_utf8_bom_crlf" => assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]) && out.windows(2).any(|w| w == b"\r\n")),
            _ => assert!(std::str::from_utf8(&out).is_ok()),
        }
    }
}

#[test]
fn latin1_file_keeps_its_bytes_and_escapes_the_rest() {
    let data = FILES[4].1;
    assert!(std::str::from_utf8(data).is_err());
    let d = extract(data);
    assert_eq!(encoding(&d), "iso-8859-1");
    assert_eq!(d.units.len(), 21);
    // French fits Latin-1 and is written as bytes; Japanese and Arabic are \uXXXX
    let out = translate(data, &[(0, "Bienvenue \u{e0} la boulangerie"), (3, "\u{3088}\u{3046}\u{3053}\u{305d}"), (6, "\u{645}\u{631}\u{62d}\u{628}\u{627}")]);
    assert!(std::str::from_utf8(&out).is_err());
    assert!(out.windows(11).any(|w| w == b"Bienvenue \xe0"));
    let s: String = out.iter().map(|&b| b as char).collect();
    assert!(s.contains("\\u3088\\u3046\\u3053\\u305D"), "{s}");
    assert!(s.contains("\\u0645\\u0631\\u062D\\u0628\\u0627"));
    let back = units(&out);
    assert_eq!(back[3], "\u{3088}\u{3046}\u{3053}\u{305d}");
    assert_eq!(back[6], "\u{645}\u{631}\u{62d}\u{628}\u{627}");
    // Latin-1 is not Windows-1252: 0x80 to 0x9F are C1 controls
    let c1 = b"k = a\x93b\x94 c\n";
    assert_eq!(units(c1), ["a\u{93}b\u{94} c"]);
    assert_identity(c1);
}

#[test]
fn edge_file_units_and_keys() {
    let d = extract(FILES[6].1);
    let keys: Vec<&str> = d.units.iter().map(|u| u.context.key.as_deref().unwrap()).collect();
    assert!(keys.contains(&"esc.key with blanks"));
    let t = units(FILES[6].1);
    let get = |k: &str| t[keys.iter().position(|x| *x == k).unwrap()].clone();
    assert_eq!(get("multi.line"), "One two three");
    assert_eq!(get("even.backslashes"), "ends with two \\");
    assert_eq!(get("colon"), "value after colon");
    assert_eq!(get("space.sep"), "value after spaces");
    assert_eq!(get("tab.sep"), "value after tab");
    assert_eq!(get("eq.in.value"), "a=b and c:d");
    assert_eq!(get("esc.chars"), "tab\there\nnew line\\ backslash");
    assert_eq!(get("unicode"), "Hello");
    assert_eq!(get("trailing.space"), "value with trailing space");
    assert_eq!(get("percent"), "100%% sure, 50% off");
    assert_eq!(get("last.no.newline"), "Last line without a line break");
    // numbers-only, empty, key-only and placeholder-only values are not units
    for k in ["rule", "empty", "only.key", "placeholders"] {
        assert!(!keys.contains(&k), "{k}");
    }
    // The trailing space of a value survives a translation.
    let i = keys.iter().position(|k| *k == "trailing.space").unwrap();
    let out = String::from_utf8(translate(FILES[6].1, &[(i, "Wert mit Leerzeichen")])).unwrap();
    assert!(out.contains("trailing.space = Wert mit Leerzeichen \n"), "{out}");
    // the multi-line value becomes one line, continuation gone
    let i = keys.iter().position(|k| *k == "multi.line").unwrap();
    let out = String::from_utf8(translate(FILES[6].1, &[(i, "Eins zwei drei")])).unwrap();
    assert!(out.contains("multi.line = Eins zwei drei\neven.backslashes"), "{out}");
}

#[test]
fn include_locked_lists_the_values_left_alone() {
    let o = ExtractOptions { include_locked: true, ..opts() };
    let d = Properties.extract(FILES[6].1, &o).unwrap();
    let locked: Vec<&str> = d.units.iter().filter(|u| !u.translate).map(|u| u.context.key.as_deref().unwrap()).collect();
    assert!(locked.contains(&"rule") && locked.contains(&"placeholders"));
    assert!(!locked.contains(&"empty"));
    assert_eq!(Properties.merge(&d).unwrap(), FILES[6].1);
    // unit ids stay the same with or without them
    let plain = extract(FILES[6].1);
    let ids: Vec<&str> = d.units.iter().filter(|u| u.translate).map(|u| u.id.as_str()).collect();
    assert_eq!(ids, plain.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>());
}

#[test]
fn ids_and_target_language_are_stable() {
    let a = extract(FILES[1].1);
    let b = extract(FILES[1].1);
    assert_eq!(a.units.iter().map(|u| &u.id).collect::<Vec<_>>(), b.units.iter().map(|u| &u.id).collect::<Vec<_>>());
    assert_eq!(a.units[0].id, "u1");
    assert_eq!(a.target_lang.as_deref(), Some("de-DE"));
    // .properties has no language tag inside the file: a target-locale merge changes nothing else
    let mut d = a.clone();
    d.target_lang = Some("ar-SA".into());
    assert_eq!(Properties.merge(&d).unwrap(), FILES[1].1);
}

#[test]
fn paragraph_and_sentence_granularity_both_round_trip() {
    let src = b"a = First sentence. Second sentence.\nb = Only one\n";
    for g in [Granularity::Paragraph, Granularity::Sentence, Granularity::Document] {
        let o = ExtractOptions { granularity: g, ..opts() };
        let d = Properties.extract(src, &o).unwrap();
        assert_eq!(d.units.len(), 2);
        assert_eq!(d.units[0].segments.len(), 1);
        assert_eq!(Properties.merge(&d).unwrap(), src);
    }
}

#[test]
fn skeleton_from_another_crate_is_refused() {
    let mut d = extract(b"a = b c\n");
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-po", "0.1.0", 1));
    }
    assert!(matches!(Properties.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn sniff_is_conservative() {
    assert!(Properties.sniff(FILES[1].1));
    assert!(Properties.sniff(FILES[0].1));
    assert!(!Properties.sniff(b"{\"a\": \"b\"}"));
    assert!(!Properties.sniff(b"<a>b</a>"));
    assert!(!Properties.sniff(b"msgid \"a\"\nmsgstr \"b\"\n"));
    assert!(!Properties.sniff(b"Just some prose with no keys at all.\nAnother line of prose.\n"));
    assert!(!Properties.sniff(&[0xFF, 0xFE, 0x41, 0x00]));
}
