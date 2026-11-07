//! Behaviour beyond the LocOS suite: round trips in 11 languages and the four legacy code tables, the merge options,
//! context fields, skeleton checks, paragraph granularity.

mod common;
use bilingua_model::*;
use bilingua_stl::Stl;
use common::*;

fn ascii_file() -> Vec<u8> {
    cat(&[
        &gsi_default(3),
        &t(1, &cat(&[DH, b"Hello there,", END, b"\x8a\x8a", DH, b"how are you?", END])),
        &tti(&Tti { sn: 2, tci: [10, 0, 4, 0], tco: [10, 0, 6, 0], ..Default::default() }, &cat(&[DH, b"Second \x01line\x07 here", END])),
        &t(3, b"Plain one"),
    ])
}

/// (locale, three sample translations, whether LocOS can write it)
fn languages() -> Vec<(&'static str, [&'static str; 3], bool)> {
    vec![
        ("fr-FR", ["Bonjour,\nçà va très bien ?", "Seconde \u{27E6}x1\u{27E7}ligne \u{27E6}x2\u{27E7}ici", "Simple « un »"], true),
        ("de-DE", ["Schöne Grüße,\nÄÖÜ und ß", "Zweite \u{27E6}x1\u{27E7}Zeile \u{27E6}x2\u{27E7}hier", "Einfach"], true),
        ("es-ES", ["¿Cómo estás?\n¡Muy bien, señor!", "Segunda \u{27E6}x1\u{27E7}línea \u{27E6}x2\u{27E7}aquí", "Uno"], true),
        ("ru-RU", ["Привет,\nкак дела?", "Вторая \u{27E6}x1\u{27E7}строка \u{27E6}x2\u{27E7}здесь", "Один"], true),
        ("ar-EG", ["مرحبا،\nكيف حالك", "الثاني \u{27E6}x1\u{27E7}سطر \u{27E6}x2\u{27E7}هنا", "واحد"], true),
        ("el-GR", ["Καλημέρα,\nτι κάνεις;", "Δεύτερη \u{27E6}x1\u{27E7}γραμμή \u{27E6}x2\u{27E7}εδώ", "Ένα"], true),
        ("he-IL", ["שלום,\nמה שלומך", "שני \u{27E6}x1\u{27E7}שורה \u{27E6}x2\u{27E7}כאן", "אחד"], true),
        ("pl-PL", ["Zażółć gęślą,\njaźń", "Druga \u{27E6}x1\u{27E7}linia \u{27E6}x2\u{27E7}tutaj", "Jeden"], true),
        ("ja-JP", ["こんにちは", "二番目", "一つ"], false),
        ("zh-CN", ["你好世界", "第二行", "一"], false),
        ("ko-KR", ["안녕하세요", "두 번째", "하나"], false),
    ]
}

#[test]
fn round_trips_in_eleven_languages() {
    let data = ascii_file();
    for (locale, tr, ok) in languages() {
        let mut d = Stl.extract(&data, &opts(Some(locale))).unwrap();
        for (k, s) in tr.iter().enumerate() {
            set(&mut d, k, s);
        }
        let r = Stl.merge(&d);
        if !ok {
            match r {
                Err(e @ (Error::Write(_) | Error::Unwritable { .. })) => { let m = e.to_string(); assert!(m.contains("subtitle 1"), "{locale}: {m}") }
                other => panic!("{locale}: expected a refusal, got {:?}", other.map(|b| b.len())),
            }
            continue;
        }
        let out = r.unwrap_or_else(|e| panic!("{locale}: {e}"));
        let back = Stl.extract(&out, &opts(None)).unwrap();
        assert_eq!(back.units.len(), 3, "{locale}");
        // same text back (« » fold nowhere here; the 00 table has them)
        for (k, s) in tr.iter().enumerate() {
            assert_eq!(locos_text(&back.units[k]), *s, "{locale} unit {k}");
        }
        // the tags are the same as the source's
        let tags = |d: &Document, k: usize| d.units[k].source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
        assert_eq!(tags(&back, 1), 2, "{locale}");
        // the language is stamped
        assert_eq!(&out[14..16], format!("{:02X}", bilingua_stl::language_code(locale)).as_bytes(), "{locale}");
        // timecodes, position and justification of every block stay
        for (a, b) in blocks(&out).iter().zip(blocks(&data)) {
            assert_eq!(a[4..13], b[4..13], "{locale}");
            assert_eq!(a[14..16], b[14..16], "{locale}");
        }
    }
}

/// A file written natively in each code table: bytes 0xA0.. mean different letters in each.
#[test]
fn native_code_tables_read_and_round_trip() {
    let cases: [(&[u8; 2], &[u8], std::ops::RangeInclusive<char>); 4] = [
        (b"01", b"Hello \xbf\xe0\xd8\xd2\xd5\xe2 end", '\u{400}'..='\u{4ff}'),
        (b"02", b"abc \xc7\xe4\xd3\xe4 \xe1\xe3\xe4", '\u{600}'..='\u{6ff}'),
        (b"03", b"x \xe1\xe2\xe3 \xdc y", '\u{370}'..='\u{3ff}'),
        (b"04", b"a \xf9\xec\xe5\xed b", '\u{590}'..='\u{5ff}'),
    ];
    for (cct, tf, script) in cases {
        let data = cat(&[&gsi(cct, b"09", b'1', b"40", 1), &t(1, tf)]);
        let d = ex(&data);
        let text = locos_text(&d.units[0]);
        assert!(text.chars().any(|c| script.contains(&c)), "table {}: {text}", String::from_utf8_lossy(cct));
        assert_eq!(Stl.merge(&d).unwrap(), data);
        // pseudo translation keeps the table
        let mut d = d;
        pseudo(&mut d);
        let out = Stl.merge(&d).unwrap();
        assert_eq!(&out[12..14], cct, "the table is kept");
        let back = locos_text(&ex(&out).units[0]);
        assert!(back.contains(text.trim()), "{back} vs {text}");
    }
}

#[test]
fn stamp_and_nothing_translated() {
    let data = ascii_file();
    // default: nothing translated => original bytes, even with a target language
    let d = Stl.extract(&data, &opts(Some("fr"))).unwrap();
    assert_eq!(Stl.merge(&d).unwrap(), data);
    // LocOS behaviour: stamp even so
    let out = Stl.merge_with(&d, &MergeOptions::new().with_stamp_target_always(true)).unwrap();
    assert_eq!(&out[14..16], b"0F");
    assert_eq!(out[16..], data[16..]);
    // no target language: never stamped
    let d = ex(&data);
    assert_eq!(Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).unwrap(), data);
    // translated with a target: stamped by the default merge
    let mut d = Stl.extract(&data, &opts(Some("de"))).unwrap();
    set(&mut d, 2, "Einfach");
    let out = Stl.merge(&d).unwrap();
    assert_eq!(&out[14..16], b"08");
    // output_encoding has no meaning for a binary format: same bytes
    let a = Stl.merge_with(&d, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)).unwrap();
    assert_eq!(a, out);
}

#[test]
fn context_and_tags() {
    let data = ascii_file();
    let d = ex(&data);
    assert_eq!(d.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["c1", "c2", "c3"]);
    let u = &d.units[1];
    assert_eq!(u.context.kind.as_deref(), Some("subtitle"));
    assert_eq!(u.context.key.as_deref(), Some("2"));
    assert_eq!(u.context.extra["start"], "10:00:04:00");
    assert_eq!(u.context.extra["end"], "10:00:06:00");
    assert_eq!(u.context.extra["max_cpl"], "40");
    let tags: Vec<&Inline> = u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    assert_eq!(tags.len(), 2);
    for t in &tags {
        match t {
            Inline::Tag { kind, free, .. } => assert!(*kind == TagKind::Standalone && !*free),
            _ => unreachable!(),
        }
    }
    assert_eq!(u.codes["1"].standalone.as_deref(), Some("<01>"));
    assert_eq!(u.codes["1"].label.as_deref(), Some("colour red"));
    // a tag the translator moved still maps to its own code byte
    let mut d = ex(&data);
    d.set_target("c2", from_locos("\u{27E6}x2\u{27E7}Zweite \u{27E6}x1\u{27E7}Zeile")).unwrap();
    let out = Stl.merge(&d).unwrap();
    let want = cat(&[DH, b"\x07Zweite\x01Zeile", END]);
    assert!(blocks(&out)[1].windows(want.len()).any(|w| w == want.as_slice()), "{:?}", &blocks(&out)[1][16..]);
}

#[test]
fn paragraph_granularity_and_set_target_checks() {
    let data = ascii_file();
    let o = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    let d = Stl.extract(&data, &o).unwrap();
    assert!(d.units.iter().all(|u| u.segments.len() == 1));
    assert_eq!(d.granularity, Granularity::Paragraph);
    let mut d = d;
    assert!(matches!(d.set_target("c1", vec![Inline::tag("9", TagKind::Standalone, "<09>")]), Err(Error::InventedTag(_))));
}

#[test]
fn skeleton_checks() {
    let data = ascii_file();
    let mut d = ex(&data);
    // units dropped: refused, not mis-merged
    d.units.pop();
    assert!(matches!(Stl.merge(&d), Err(Error::Write(_) | Error::Unwritable { .. })));
    // a text skeleton (not ours)
    let mut d = ex(&data);
    d.skeleton = Skeleton::default();
    assert!(matches!(Stl.merge(&d), Err(Error::Write(_))));
    // another parser's skeleton
    let mut d = ex(&data);
    if let Skeleton::Binary { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(Stl.merge(&d), Err(Error::ParserVersion { .. })));
    // JSON round trip of the document keeps merging
    let d = ex(&data);
    let j = serde_json::to_string(&d).unwrap();
    let mut back: Document = serde_json::from_str(&j).unwrap();
    set(&mut back, 2, "Simple");
    assert!(Stl.merge(&back).is_ok());
}

#[test]
fn comment_and_user_data_blocks_are_never_text() {
    let data = cat(&[
        &gsi_default(3),
        &tti(&Tti { sn: 1, cf: 1, ..Default::default() }, b"note"),
        &t(2, b"Real text"),
        &tti(&Tti { sn: 3, ebn: 0xFE, ..Default::default() }, b"user data"),
    ]);
    let d = ex(&data);
    assert_eq!(texts(&d), ["Real text"]);
    let out = build(&data, &[(0, "Texte reel".into())]).unwrap();
    assert_eq!(blocks(&out)[0], blocks(&data)[0]);
    assert_eq!(blocks(&out)[2], blocks(&data)[2]);
}

#[test]
fn dfc_30_and_blank_cct_read() {
    let mut g = gsi_default(1);
    g[3..11].copy_from_slice(b"STL30.01");
    g[12..14].copy_from_slice(b"  ");
    let data = cat(&[&g, &t(1, b"Thirty fps")]);
    let d = ex(&data);
    assert_eq!(texts(&d), ["Thirty fps"]);
    assert_eq!(Stl.merge(&d).unwrap(), data);
}

#[test]
fn a_letter_with_a_floating_diacritic_and_no_single_code_is_written_back() {
    // grave + D has no precomposed character: it reads as D and U+0300 and must be written as the byte pair
    let data = cat(&[&gsi_default(1), &t(1, &cat(&[DH, b"x\xc1Dy", END]))]);
    let d = ex(&data);
    assert_eq!(texts(&d), vec!["xD\u{300}y".to_string()]);
    let mut d = d;
    set(&mut d, 0, "ZqxD\u{300}yqZ");
    let out = Stl.merge(&d).unwrap();
    assert_eq!(texts(&ex(&out)), vec!["ZqxD\u{300}yqZ".to_string()]);
}

#[test]
fn a_row_wider_than_the_header_mnc_takes_a_translation_as_long_as_its_source() {
    // the header says 20 characters per row, but the source row has 35: a translation of the same length fits
    let data = cat(&[&gsi(b"00", b"09", b'1', b"20", 1), &t(1, &cat(&[DH, b"The keeper waves from the old rocks", END]))]);
    let mut d = ex(&data);
    set(&mut d, 0, "El guarda saluda desde las rocas ya");
    let out = Stl.merge(&d).unwrap();
    assert_eq!(texts(&ex(&out)), vec!["El guarda saluda desde las rocas ya".to_string()]);
    // a longer one is wrapped at the source row width
    set(&mut d, 0, "El guarda del faro saluda desde las rocas viejas");
    let out = Stl.merge(&d).unwrap();
    let back = texts(&ex(&out));
    let rows: Vec<&str> = back[0].split('\n').collect();
    assert_eq!(rows.len(), 2, "{back:?}");
    assert!(rows.iter().all(|r| r.chars().count() <= 35) && rows.iter().any(|r| r.chars().count() > 20), "{back:?}");
    // the locos profile keeps LocOS's header width: the same-length translation is wrapped at 20
    set(&mut d, 0, "El guarda saluda desde las rocas ya");
    let out = Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
    assert!(texts(&ex(&out))[0].contains('\n'));
}

fn comment_file(comment: &[u8], other: &[u8]) -> Vec<u8> {
    cat(&[
        &gsi_default(3),
        &tti(&Tti { sn: 1, cf: 1, ..Default::default() }, comment),
        &t(2, &cat(&[DH, b"Hello", END])),
        &t(3, &cat(&[DH, other, END])),
    ])
}

#[test]
fn a_code_table_switch_moves_comment_subtitles_too() {
    // table 00 holds the section sign at A7; table 01 (Cyrillic) holds it at FD. A comment must move with the file.
    let data = comment_file(b"a\xa7b", b"Bye");
    let ru = "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}";
    for profile in [false, true] {
        let mut d = ex(&data);
        set(&mut d, 0, ru);
        let out = if profile { Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")) } else { Stl.merge(&d) }.unwrap();
        assert_eq!(&out[12..14], b"01");
        let comment = blocks(&out)[0];
        assert_eq!(&comment[16..19], b"a\xfdb", "profile={profile}: the comment is re-encoded in the new table");
        assert_eq!(texts(&ex(&out)), vec![ru.to_string(), "Bye".to_string()]);
    }
}

#[test]
fn a_code_table_switch_is_refused_when_a_comment_cannot_move() {
    // e with an acute accent exists in table 00 but not in 8859-5: the comment would read back as other letters
    let data = comment_file(b"caf\xc2e", b"Bye");
    let mut d = ex(&data);
    set(&mut d, 0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}");
    let e = Stl.merge(&d).unwrap_err().to_string();
    assert!(e.contains("needs code table 01") && e.contains("comment subtitles hold characters that table does not have: comment subtitle at block 1."), "{e}");
    assert!(Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).is_err());
}

#[test]
fn a_code_table_switch_does_not_copy_bytes_the_old_table_has_no_character_for() {
    // C0 is undefined in table 00 (a token); in table 01 it is a Cyrillic letter, so the file must not switch
    let data = comment_file(b"note", b"ab\xc0cd");
    let mut d = ex(&data);
    set(&mut d, 0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}");
    assert!(Stl.merge(&d).is_err());
    assert!(Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).is_err());
    // the same file without a switch still takes a Latin translation
    let mut d = ex(&data);
    set(&mut d, 0, "Bonjour");
    assert!(Stl.merge(&d).is_ok());
}

#[test]
fn a_code_table_switch_names_the_subtitles_whose_bytes_cannot_move() {
    // unit 2 holds C0 (no character in table 00, a letter in table 01): the refusal says why, not "characters"
    let data = comment_file(b"note", b"ab\xc0cd");
    let mut d = ex(&data);
    set(&mut d, 0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}");
    let e = Stl.merge(&d).unwrap_err().to_string();
    assert!(e.contains("needs code table 01") && e.contains("hold bytes with no equivalent in that table (they would read as other characters): subtitle 2."), "{e}");
    assert!(!e.contains("subtitle 1") && !e.contains("comment"), "{e}");
}

#[test]
fn a_code_table_switch_keeps_bytes_no_table_has_a_character_for() {
    // 7F has no character in any table: it means the same in table 01, so the switch goes ahead; the unchanged
    // unit's section sign (A7 in table 00) is re-encoded as FD
    let data = comment_file(b"x\x7fy", b"\xa7 2");
    for profile in [false, true] {
        let mut d = ex(&data);
        set(&mut d, 0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}");
        let out = if profile { Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")) } else { Stl.merge(&d) }.unwrap();
        assert_eq!(&out[12..14], b"01");
        assert_eq!(&blocks(&out)[0][16..19], b"x\x7fy", "profile={profile}");
        assert_eq!(texts(&ex(&out))[1], "\u{a7} 2");
        assert!(blocks(&out)[2][16..].contains(&0xFD));
    }
}
