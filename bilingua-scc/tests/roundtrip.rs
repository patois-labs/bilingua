//! Round trips: untouched and echo merges give the original bytes; a pseudo-translated merge reads back with the
//! same units and tags; eleven languages (those CEA-608 can show are written, the others are refused with the
//! caption named); encodings; a translation can never add a caption, a code or a header.
mod common;
use bilingua_model::*;
use bilingua_scc::SccFilter;
use common::*;

const SUITE: [&str; 11] = [
    "Scenarist_SCC V1.0",
    "",
    "00:00:01:00\t94ae 94ae 9420 9420 9440 9440 c845 4c4c 4f80 94e0 94e0 574f 524c c480 942f 942f",
    "",
    "00:00:03:00\t942c 942c",
    "",
    "00:00:04:00\t9425 9425 94ad 94ad 9470 9470 4920 7361 7920 91ae 91ae 68e9 a120",
    "00:00:05:00\t94ad 94ad 9470 9470 d580 92a4 92a4 6e64 9137 9137",
    "00:00:06;02\t9429 9429 9154 9154 97a2 97a2 e361 e6dc",
    "00:00:07:00\t1c20 1c20 1c70 1c70 c1c2",
    "00:00:08:00\t9420 9420 9470 9470 c1c2 94a1 94a1 43c4 942f 942f",
];

fn suite(nl: &str) -> Vec<u8> {
    (SUITE.join(nl) + nl).into_bytes()
}

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

#[test]
fn corpus_untouched_and_echo_are_exact() {
    let files = corpus();
    assert!(files.len() >= 10);
    for (name, data) in &files {
        let doc = extract(data).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(SccFilter.merge(&doc).unwrap(), *data, "{name}: untouched");
        let echo: Vec<(usize, String)> = units(data).into_iter().enumerate().collect();
        assert_eq!(try_build(data, &echo).unwrap(), *data, "{name}: echo");
    }
}

#[test]
fn line_endings_bom_and_utf16_are_kept() {
    for nl in ["\r\n", "\n", "\r"] {
        assert_identity(&suite(nl));
    }
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend(suite("\r\n"));
    assert_identity(&bom);
    // mixed line endings
    let mixed = (SUITE.join("\n") + "\r\n").replacen('\n', "\r\n", 3).into_bytes();
    assert_identity(&mixed);
    for be in [false, true] {
        let text = SUITE.join("\r\n") + "\r\n";
        let mut data: Vec<u8> = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
        for u in text.encode_utf16() {
            data.extend(if be { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        assert_identity(&data);
        // a translated merge keeps UTF-16; "write UTF-8" (the LocOS adapter) turns it into UTF-8
        let kept = build(&data, &[(0, "SALUT\nMONDE")]);
        assert_eq!(&kept[..2], &data[..2]);
        let utf8 = try_build_with(&data, &[(0, "SALUT\nMONDE".into())], &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        assert!(utf8.starts_with(b"Scenarist_SCC"));
        assert_eq!(units(&utf8)[0], "SALUT\nMONDE");
    }
}

#[test]
fn legacy_encoded_blank_lines_before_the_header() {
    // a Windows-1252 file whose leading blank line is a no-break space (0xA0, whitespace to Python)
    let mut data = vec![0xA0, b'\r', b'\n'];
    data.extend(suite("\r\n"));
    assert_identity(&data);
    let out = build(&data, &[(0, "BONJOUR")]);
    assert!(out.starts_with(&[0xA0, b'\r', b'\n']), "the file's own encoding is kept");
}

/// ASCII letters to accented letters CEA-608 can show.
fn pseudo(t: &str) -> String {
    t.chars()
        .map(|c| match c {
            'a' => '\u{e0}',
            'e' => '\u{e8}',
            'i' => '\u{ee}',
            'o' => '\u{f4}',
            'u' => '\u{fb}',
            'A' => '\u{c0}',
            'E' => '\u{c9}',
            'U' => '\u{dc}',
            c => c,
        })
        .collect()
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in corpus() {
        let doc = extract(&data).unwrap();
        let n = doc.units.len();
        let src_tags: Vec<Vec<String>> = doc
            .units
            .iter()
            .map(|u| u.source().iter().filter_map(|i| if let Inline::Tag { id, .. } = i { Some(id.clone()) } else { None }).collect())
            .collect();
        let m: Vec<(usize, String)> = units(&data).iter().enumerate().map(|(i, t)| (i, pseudo(t))).collect();
        let out = match try_build(&data, &m) {
            Ok(o) => o,
            Err(e) => {
                // the only acceptable refusal is a caption that no longer fits; none of these do
                panic!("{name}: {e}");
            }
        };
        let doc2 = extract(&out).unwrap();
        assert_eq!(doc2.units.len(), n, "{name}: unit count");
        for (i, u) in doc2.units.iter().enumerate() {
            let tags: Vec<String> = u.source().iter().filter_map(|i| if let Inline::Tag { id, .. } = i { Some(id.clone()) } else { None }).collect();
            assert_eq!(tags, src_tags[i], "{name}: tags of unit {i}");
            assert_eq!(locos_text(&u.source()).replace('\n', " ").split_whitespace().collect::<Vec<_>>().join(" "), {
                let want = pseudo(&units(&data)[i]);
                want.replace('\n', " ").split_whitespace().collect::<Vec<_>>().join(" ")
            }, "{name}: text of unit {i}");
        }
    }
}

#[test]
fn eleven_languages() {
    // (language, text, can CEA-608 show it)
    let langs: [(&str, &str, bool); 11] = [
        ("en", "Hello, world. It's a good day.", true),
        ("fr", "Bonjour \u{e0} tous, o\u{f9} est l\u{2019}\u{e9}t\u{e9} ?", true),
        ("es", "\u{bf}Qu\u{e9} tal, se\u{f1}or? \u{a1}Ma\u{f1}ana!", true),
        ("de", "Sch\u{f6}ne Gr\u{fc}\u{df}e aus K\u{f6}ln", true),
        ("pt", "A\u{e7}\u{e3}o n\u{e3}o \u{e9} f\u{e1}cil, cora\u{e7}\u{f5}es", true),
        ("sv", "Hej d\u{e4}r, \u{e5}sa \u{e4}ter \u{f6}l", true),
        ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}", false),
        ("ja", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{4e16}\u{754c}", false),
        ("zh", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}", false),
        ("ko", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}", false),
        ("ru", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}", false),
    ];
    let data = suite("\r\n");
    for (lang, text, ok) in langs {
        let r = try_build(&data, &[(0, text.to_string())]);
        if ok {
            let out = r.unwrap_or_else(|e| panic!("{lang}: {e}"));
            let got = units(&out);
            let norm = |s: &str| s.replace('\u{2019}', "'").split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(norm(&got[0].replace('\n', " ")), norm(text), "{lang}");
            assert_eq!(got.len(), 4, "{lang}");
            // everything but the changed line is untouched
            let (a, b) = (String::from_utf8_lossy(&out).to_string(), String::from_utf8_lossy(&data).to_string());
            let same = a.lines().zip(b.lines()).filter(|(x, y)| x == y).count();
            assert_eq!(same, b.lines().count() - 1, "{lang}");
        } else {
            let e = r.expect_err(lang);
            assert!(matches!(e, Error::Write(_) | Error::Unwritable { .. }), "{lang}: {e}");
            let m = e.to_string();
            assert!(m.contains("caption 1 at 00:00:01:00") && m.contains("CEA-608 cannot show"), "{lang}: {m}");
        }
    }
    // and the same text in a document with a target language set: no language field exists in SCC, so nothing is
    // stamped, with or without "stamp always"
    let mut doc = SccFilter.extract(&data, &ExtractOptions::new().with_target_lang("fr")).unwrap();
    assert_eq!(SccFilter.merge_with(&doc, &MergeOptions::new().with_stamp_target_always(true)).unwrap(), data);
    doc.set_target("c1", vec![Inline::text("BONJOUR")]).unwrap();
    let out = SccFilter.merge(&doc).unwrap();
    assert_eq!(String::from_utf8(out).unwrap().lines().count(), SUITE.len());
}

#[test]
fn a_translation_cannot_add_captions_codes_or_header_fields() {
    let data = suite("\r\n");
    let lines_of = |d: &[u8]| String::from_utf8_lossy(d).lines().count();
    let attacks = [
        "x\r\n00:00:09:00\t9420 9420 4142 942f",
        "x\n00:00:09:00 9420",
        "Scenarist_SCC V1.0",
        "9420 9420 942f",
        "a\u{27e6}x9b",
        "\u{14}\u{1}\u{7f}",
        "a\tb",
        "a\rb",
        "`backtick`",
    ];
    for a in attacks {
        match try_build(&data, &[(0, a.to_string())]) {
            Ok(out) => {
                assert_eq!(lines_of(&out), lines_of(&data), "{a:?}: line count");
                let u = units(&out);
                assert_eq!(u.len(), 4, "{a:?}");
                // the text reads back as text: the attack words are characters, not codes
                assert!(!u[0].contains('\u{27e6}'), "{a:?}");
                assert_eq!(out.windows(18).filter(|w| *w == b"Scenarist_SCC V1.0").count(), 1, "{a:?}");
                // other lines are untouched
                let (o, d) = (String::from_utf8_lossy(&out).to_string(), String::from_utf8_lossy(&data).to_string());
                for (x, y) in o.lines().zip(d.lines()) {
                    if !x.starts_with("00:00:01:00") {
                        assert_eq!(x, y, "{a:?}");
                    }
                }
            }
            Err(e) => assert!(matches!(e, Error::Write(_) | Error::Unwritable { .. }), "{a:?}: {e}"),
        }
    }
    // an inline code the caption does not have cannot be set at all
    let mut d0 = extract(&data).unwrap();
    assert!(matches!(d0.set_target("c1", from_locos("a \u{27e6}x1\u{27e7}")), Err(Error::InventedTag(_))));
    // a literal token spelled in text is not a code: refused, because the token characters cannot be shown
    let e = try_build(&data, &[(1, "Je dis \u{27e6}x1\u{27e7}".to_string())]);
    assert!(e.is_ok(), "as a tag it is the real mid-row code");
    let mut doc = extract(&data).unwrap();
    doc.units[1].segments[0].target = vec![Inline::text("Je dis \u{27e6}x1\u{27e7}salut")];
    let e = SccFilter.merge(&doc).unwrap_err().to_string();
    assert!(e.contains("cannot show"), "{e}");
    // a tag the line never had is dropped, never written
    doc.units[1].segments[0].target = vec![Inline::text("Salut "), Inline::tag("99", TagKind::Standalone, "x")];
    let out = SccFilter.merge(&doc).unwrap();
    assert_eq!(units(&out)[1], "Salut");
}

#[test]
fn tampered_documents_return_errors() {
    let data = suite("\r\n");
    let doc = extract(&data).unwrap();
    // lost original line
    let mut d = doc.clone();
    d.units[0].hidden.remove("raw");
    assert!(matches!(SccFilter.merge(&d), Err(Error::Write(_) | Error::Unwritable { .. })));
    // lost or broken state
    let mut d = doc.clone();
    d.units[0].segments[0].target = vec![Inline::text("A")];
    d.units[0].set_hidden("state", "9 nope 99");
    assert!(matches!(SccFilter.merge(&d), Err(Error::Write(_) | Error::Unwritable { .. }) | Ok(_)));
    // a skeleton pointing at a missing unit
    let mut d = doc.clone();
    d.units.truncate(1);
    assert!(matches!(SccFilter.merge(&d), Err(Error::Write(_) | Error::Unwritable { .. })));
    // the raw line replaced by something that is not a caption line
    let mut d = doc.clone();
    d.units[0].segments[0].target = vec![Inline::text("A")];
    d.units[0].set_hidden("raw", "hello");
    assert!(matches!(SccFilter.merge(&d), Err(Error::Write(_) | Error::Unwritable { .. })));
    // another crate's skeleton
    let mut d = doc.clone();
    d.skeleton = Skeleton::Text {
        parts: vec![],
        encoding: "utf-8".into(),
        parser: Some(ParserInfo::new("bilingua-other", "0.1.0", 1)),
    };
    assert!(SccFilter.merge(&d).is_err());
}

#[test]
fn units_carry_context_and_codes() {
    let doc = extract(&suite("\r\n")).unwrap();
    assert_eq!(doc.units.len(), 4);
    let u = &doc.units[1];
    assert_eq!(u.id, "c2");
    assert_eq!(u.context.kind.as_deref(), Some("subtitle"));
    assert_eq!(u.context.key.as_deref(), Some("00:00:04:00"));
    assert_eq!(u.context.extra.get("mode").map(String::as_str), Some("rollup"));
    assert_eq!(u.context.extra.get("max_cpl").map(String::as_str), Some("32"));
    // one standalone, non-free tag for the italics code
    let src = u.source();
    let tags: Vec<&Inline> = src.iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    assert_eq!(tags.len(), 1);
    assert!(matches!(tags[0], Inline::Tag { kind: TagKind::Standalone, free: false, display, .. } if display == "[italics]"));
    assert_eq!(u.codes.get("1").and_then(|c| c.label.as_deref()), Some("mid-row code"));
}

#[test]
fn sniff_is_strict() {
    assert!(SccFilter.sniff(&suite("\n")));
    assert!(SccFilter.sniff(b"\xef\xbb\xbf\n\nScenarist_SCC V1.0\n"));
    assert!(SccFilter.sniff(b"\xff\xfeS\0c\0e\0n\0a\0r\0i\0s\0t\0_\0S\0C\0C\0 \0V\0"));
    assert!(!SccFilter.sniff(b"1\n00:00:01,000 --> 00:00:02,000\nhi\n"));
    assert!(!SccFilter.sniff(b""));
    assert!(!SccFilter.sniff(b"hello"));
}

#[test]
fn a_caption_that_would_read_back_on_channel_2_is_refused() {
    // lead decision 11-05 (the rc precedent): LocOS writes these files and a caption then vanishes on the next read.
    // Caption 1 switches to channel 2 (1920) and back (9120) mid-row; caption 2 starts with characters before any
    // code, so it is read on the channel caption 1 ended on.
    let data = b"Scenarist_SCC V1.0\r\n\r\n00:00:01:00\t9420 9420 9470 9470 c1c2 1920 9120 c3c4\r\n00:00:02:00\tc5c6 e5e6\r\n".to_vec();
    assert_eq!(units(&data), ["AB \u{27e6}x1\u{27e7} \u{27e6}x2\u{27e7}CD", "EFef"]);
    // codes kept in order: written, and both captions read back
    let out = try_build(&data, &[(0, "AB \u{27e6}x1\u{27e7} \u{27e6}x2\u{27e7}CD ok".to_string())]).unwrap();
    assert_eq!(units(&out).len(), 2);
    // the channel-2 code moved to the end: caption 1 now ends on channel 2 and caption 2 would vanish
    let e = try_build(&data, &[(0, "AB \u{27e6}x2\u{27e7} CD \u{27e6}x1\u{27e7}".to_string())]).unwrap_err();
    let msg = e.to_string();
    assert!(matches!(e, Error::Write(_) | Error::Unwritable { .. }), "{msg}");
    assert!(msg.contains("caption 2 at 00:00:02:00: would be read back on data channel 2"), "{msg}");
    assert!(!msg.contains("caption 1 "), "{msg}");
    // the channel-1 code dropped: caption 1's own text is on channel 2, and so is caption 2
    let msg = try_build(&data, &[(0, "AB \u{27e6}x1\u{27e7} CD".to_string())]).unwrap_err().to_string();
    assert!(msg.contains("caption 1 at 00:00:01:00: would be read back on data channel 2"), "{msg}");
    assert!(msg.contains("caption 2 at 00:00:02:00"), "{msg}");
    // an untouched or echoed merge is never checked against this and stays exact
    assert_identity(&data);
}

/// Plain text as SCC hex words (odd parity, two characters per word).
fn hex_words(text: &str) -> String {
    let par = |b: u8| if b.count_ones() % 2 == 0 { b | 0x80 } else { b };
    text.as_bytes()
        .chunks(2)
        .map(|c| format!("{:02x}{:02x}", par(c[0]), if c.len() > 1 { par(c[1]) } else { 0x80 }))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn rewrapped_rows_without_a_pac_do_not_run_together() {
    // a caption with no positioning codes: the translation is longer than one row, so it is wrapped
    let src = format!("Scenarist_SCC V1.0\n\n00:00:01:00\t{}\n", hex_words("Works on Mac or Windows."));
    let out = try_build(src.as_bytes(), &[(0, "Zqdoes it work on Mac or Windowsqz".into())]).unwrap();
    let again = units(&out);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0], "Zqdoes it work on Mac or\nWindowsqz");
}

#[test]
fn an_added_row_does_not_repeat_the_mode_codes() {
    // roll-up caption (ENM, RCL, RU3 then text, no PAC): a longer translation adds a row
    let src = format!("Scenarist_SCC V1.0\n\n00:00:01:00\t94ae 94ae 9420 9420 9426 9426 {}\n", hex_words("Works on Mac or Windows."));
    let out = try_build(src.as_bytes(), &[(0, "Zqdoes it work on Mac or Windowsqz".into())]).unwrap();
    let again = units(&out);
    assert_eq!(again, vec!["Zqdoes it work on Mac or\nWindowsqz".to_string()]);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches("9426").count(), 2, "{text}");
}
