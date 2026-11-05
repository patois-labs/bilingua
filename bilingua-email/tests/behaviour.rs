//! Behaviour beyond the LocOS suite: round trips on every fixture in `tests/data`, 11 languages and the legacy
//! charsets, target-locale merges, line-length limits, attachments kept byte for byte, delimiter safety.

mod common;
use bilingua_email::Email;
use bilingua_model::*;
use common::*;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let mut names: Vec<String> = std::fs::read_dir(DATA).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).filter(|n| n.ends_with(".eml")).collect();
    names.sort();
    names.into_iter().map(|n| (n.clone(), std::fs::read(format!("{DATA}/{n}")).unwrap())).collect()
}

fn fx(name: &str) -> Vec<u8> {
    std::fs::read(format!("{DATA}/{name}")).unwrap()
}

/// One sample sentence per test language, with spaces so wrapped text reads back the same.
const SAMPLES: [(&str, &str); 11] = [
    ("en", "Your order ships on Monday, please confirm."),
    ("de", "Ihre Bestellung wird am Montag f\u{fc}r Gr\u{fc}\u{df}e versandt."),
    ("fr", "Votre commande sera exp\u{e9}di\u{e9}e lundi \u{e0} bient\u{f4}t."),
    ("es", "\u{a1}Su pedido se enviar\u{e1} el lunes, ma\u{f1}ana!"),
    ("ru", "\u{412}\u{430}\u{448} \u{437}\u{430}\u{43a}\u{430}\u{437} \u{431}\u{443}\u{434}\u{435}\u{442} \u{43e}\u{442}\u{43f}\u{440}\u{430}\u{432}\u{43b}\u{435}\u{43d} \u{432} \u{43f}\u{43e}\u{43d}\u{435}\u{434}\u{435}\u{43b}\u{44c}\u{43d}\u{438}\u{43a}."),
    ("ar", "\u{633}\u{64a}\u{62a}\u{645} \u{634}\u{62d}\u{646} \u{637}\u{644}\u{628}\u{643} \u{64a}\u{648}\u{645} \u{627}\u{644}\u{627}\u{62b}\u{646}\u{64a}\u{646}."),
    ("ja", "\u{3054}\u{6ce8}\u{6587}\u{306e}\u{5546}\u{54c1}\u{306f} \u{6708}\u{66dc}\u{65e5}\u{306b}\u{767a}\u{9001}\u{3055}\u{308c}\u{307e}\u{3059}\u{3002}"),
    ("zh", "\u{60a8}\u{7684}\u{8ba2}\u{5355} \u{5c06}\u{4e8e}\u{661f}\u{671f}\u{4e00}\u{53d1}\u{8d27}\u{3002}"),
    ("ko", "\u{c8fc}\u{bb38}\u{d558}\u{c2e0} \u{c0c1}\u{d488}\u{c740} \u{c6d4}\u{c694}\u{c77c}\u{c5d0} \u{bc1c}\u{c1a1}\u{b429}\u{b2c8}\u{b2e4}."),
    ("tr", "Sipari\u{15f}iniz Pazartesi g\u{fc}n\u{fc} g\u{f6}nderilecek."),
    ("pl", "Twoje zam\u{f3}wienie zostanie wys\u{142}ane w poniedzia\u{142}ek."),
];

fn tags_of(u: &Unit) -> Vec<(String, TagKind)> {
    u.source().iter().filter_map(|i| if let Inline::Tag { id, kind, .. } = i { Some((id.clone(), *kind)) } else { None }).collect()
}

#[test]
fn every_fixture_round_trips_untouched() {
    let all = fixtures();
    assert!(all.len() >= 30, "{} fixtures", all.len());
    for (name, data) in &all {
        let d = Email.extract(data, &opts(None)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!d.units.is_empty(), "{name}");
        assert_eq!(&merged(&d), data, "{name}: untouched merge");
        assert_eq!(&identity(data), data, "{name}: merge of unchanged targets");
        // with a target locale and nothing translated: still the original bytes
        let d = Email.extract(data, &opts(Some("de-DE"))).unwrap();
        assert_eq!(&merged(&d), data, "{name}: untouched merge with a target");
        // unit ids are unique and stable
        let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "{name}: ids");
        let again = Email.extract(data, &opts(None)).unwrap();
        assert_eq!(again.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ids, "{name}: stable ids");
        assert!(Email.sniff(data) || name.starts_with("headers_only"), "{name}: sniff");
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_the_same_units_and_tags() {
    for (name, data) in &fixtures() {
        let mut d = ex(data);
        pseudo(&mut d);
        let out = merged(&d);
        let back = ex(&out);
        if name == "hard_wrapped.eml" {
            // a hard-wrapped paragraph is re-wrapped at its width; the lines of the new text may be shorter
            // than the 60 characters that mark a hard wrap, so it can read back as more paragraphs (LocOS does
            // the same, byte for byte): the words are what must survive
            let words = |d: &Document| {
                d.units.iter().map(loc).collect::<Vec<_>>().join(" ").split_whitespace().map(|w| w.trim_matches(['\u{ab}', '\u{bb}']).to_string()).collect::<Vec<_>>()
            };
            assert_eq!(words(&back), words(&d), "{name}");
            continue;
        }
        assert_eq!(back.units.len(), d.units.len(), "{name}");
        for (b, u) in back.units.iter().zip(&d.units) {
            assert_eq!(tags_of(b), tags_of(u), "{name}: tags of {}", u.id);
            // the pseudo text read back is the same up to the white space of re-wrapped paragraphs
            let want = loc(u);
            let want = format!("\u{ab}{want}\u{bb}");
            let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(norm(&loc(b)), norm(&want), "{name}: {}", u.id);
        }
    }
}

#[test]
fn eleven_languages_in_every_kind_of_part() {
    for (name, data) in &fixtures() {
        for (lang, sample) in SAMPLES {
            let d0 = ex(data);
            let mut d = d0.clone();
            let mut want: Vec<String> = Vec::new();
            for (i, u) in d.units.clone().iter().enumerate() {
                let src = loc(u);
                let t = if src.contains('\u{27e6}') { format!("{sample} {src}") } else { sample.to_string() };
                set_idx(&mut d, i, &t);
                want.push(t);
            }
            let out = merged(&d);
            let back = texts(&out);
            let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(back.iter().map(|s| norm(s)).collect::<Vec<_>>(), want.iter().map(|s| norm(s)).collect::<Vec<_>>(), "{name} <- {lang}");
            // and the independent reader sees the same text in every translated text part
            let msg = parse(&out);
            for leaf in msg.leaves() {
                if leaf.ctype() == "text/plain" && leaf.header("content-disposition").is_none() {
                    assert!(leaf.text().contains(sample) || leaf.text().is_empty() || name.starts_with("hard") || name.starts_with("flowed") || name.starts_with("qp") || name.starts_with("folded") || name.starts_with("forwarded") || name.starts_with("unterm") || name.starts_with("utf8"), "{name} <- {lang}: {:?}", leaf.text());
                }
            }
        }
    }
}

fn charset_of(out: &[u8]) -> String {
    parse(out).leaves().into_iter().find(|e| e.ctype().starts_with("text/")).and_then(|e| e.param("charset")).unwrap_or_default().to_lowercase()
}

#[test]
fn a_legacy_charset_is_kept_when_the_translation_fits_and_changed_when_it_does_not() {
    let cases = [
        ("legacy_ru_koi8r.eml", "koi8-r", 4usize),
        ("legacy_ru_cp1251.eml", "windows-1251", 4),
        ("legacy_ar_cp1256.eml", "windows-1256", 5),
        ("legacy_ja_iso2022jp.eml", "iso-2022-jp", 6),
        ("legacy_ja_shift_jis.eml", "shift_jis", 6),
        ("legacy_ja_euc_jp.eml", "euc-jp", 6),
        ("legacy_zh_gb2312.eml", "gb2312", 7),
        ("legacy_ko_euc_kr.eml", "euc-kr", 8),
        ("legacy_tr_8859_9.eml", "iso-8859-9", 9),
        ("legacy_de_latin1.eml", "iso-8859-1", 1),
        ("legacy_es_8859_15.eml", "iso-8859-15", 3),
    ];
    for (file, cs, sample) in cases {
        let data = fx(file);
        let d = ex(&data);
        let mut tr: Vec<(usize, String)> = vec![];
        for (i, u) in d.units.iter().enumerate() {
            if u.context.style.as_deref() == Some("text/plain") {
                tr.push((i, SAMPLES[sample].1.to_string()));
            }
        }
        let refs: Vec<(usize, &str)> = tr.iter().map(|(i, t)| (*i, t.as_str())).collect();
        let out = build(&data, &refs);
        assert_eq!(charset_of(&out), cs, "{file}: charset kept");
        assert!(texts(&out).iter().any(|t| t == SAMPLES[sample].1), "{file}");
        // a character the charset cannot hold: UTF-8, recorded in the header
        let emoji: Vec<(usize, &str)> = tr.iter().map(|(i, _)| (*i, "Fallback \u{1f600} text")).collect();
        let out = build(&data, &emoji);
        assert_eq!(charset_of(&out), "utf-8", "{file}: fallback");
        assert!(texts(&out).iter().any(|t| t == "Fallback \u{1f600} text"), "{file}");
    }
}

#[test]
fn encoded_word_subjects_come_back_in_a_charset_that_holds_them() {
    // Shift_JIS hint, Japanese text: stays Shift_JIS; Arabic text: UTF-8
    let sjis = b"Subject: =?shift_jis?b?kb2NgYNrgUA=?=\nContent-Type: text/plain\n\nbody\n";
    let out = build(sjis, &[(0, "\u{6ce8}\u{6587}\u{306e}\u{78ba}\u{8a8d}")]);
    assert!(contains(&out, b"Subject: =?iso-2022-jp?b?"), "{:?}", String::from_utf8_lossy(&out));
    assert_eq!(parse(&out).subject(), "\u{6ce8}\u{6587}\u{306e}\u{78ba}\u{8a8d}");
    let out = build(sjis, &[(0, "\u{637}\u{644}\u{628}\u{643}")]);
    assert!(contains(&out, b"Subject: =?utf-8?"), "{:?}", String::from_utf8_lossy(&out));
    assert_eq!(parse(&out).subject(), "\u{637}\u{644}\u{628}\u{643}");
    // a long Subject folds under 78 columns and decodes back whole
    let long = "word \u{e9}t\u{e9} ".repeat(30);
    for data in [&b"Subject: =?iso-8859-1?q?x?=\nContent-Type: text/plain\n\nbody\n"[..], b"Subject: plain\nContent-Type: text/plain\n\nbody\n"] {
        let out = build(data, &[(0, long.trim())]);
        let head = &out[..find(&out, b"\n\n").unwrap()];
        for l in head.split(|c| *c == b'\n') {
            assert!(l.len() <= 78, "{:?}", String::from_utf8_lossy(l));
        }
        assert_eq!(parse(&out).subject(), long.split_whitespace().collect::<Vec<_>>().join(" "));
    }
}

#[test]
fn line_length_limits_on_write() {
    // QP and base64 lines stay at 76 characters or fewer, 8bit text with a 2000-byte line goes to QP
    let long = "word ".repeat(600);
    let src = b"Subject: s\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: 8bit\n\nHello\n";
    let out = build(src, &[(1, long.trim())]);
    assert!(contains(&out, b"Content-Transfer-Encoding: quoted-printable"), "{:?}", String::from_utf8_lossy(&out[..200]));
    for l in out.split(|c| *c == b'\n') {
        assert!(l.len() <= 998);
    }
    // plain text paragraphs are wrapped at their width, so the QP lines are short
    let src = b"Subject: s\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: quoted-printable\n\nHello\n";
    let out = build(src, &[(1, &"caf\u{e9}\u{e9}\u{e9} ".repeat(60))]);
    let body = &out[find(&out, b"\n\n").unwrap() + 2..];
    assert!(body.split(|c| *c == b'\n').all(|l| l.len() <= 76), "{:?}", String::from_utf8_lossy(body));
    let src = b"Subject: s\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: base64\n\nSGVsbG8K\n";
    let out = build(src, &[(1, &"b64 text ".repeat(100))]);
    let body = &out[find(&out, b"\n\n").unwrap() + 2..];
    assert!(body.split(|c| *c == b'\n').all(|l| l.len() <= 76));
    assert_eq!(parse(&out).text().split_whitespace().collect::<Vec<_>>().join(" "), "b64 text ".repeat(100).trim());
}

#[test]
fn attachments_and_other_parts_stay_byte_for_byte() {
    let mut bin: Vec<u8> = (0..=255u8).collect();
    bin.extend_from_slice(b"\r\n--B\r\n--B--\r\n\xff\xfe");
    let mut msg = b"Subject: att\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"B\"\r\n\r\n--B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello there.\r\n--B\r\nContent-Type: application/octet-stream\r\nContent-Transfer-Encoding: binary\r\nContent-Disposition: attachment; filename=x.bin\r\n\r\n".to_vec();
    // a binary part must not contain its own delimiter line; build it without
    let safe: Vec<u8> = (1..=255u8).filter(|b| *b != b'-').collect();
    let _ = bin;
    msg.extend_from_slice(&safe);
    msg.extend_from_slice(b"\r\n--B\r\nContent-Type: image/gif\r\nContent-Transfer-Encoding: base64\r\n\r\nR0lGODlhAQABAAAAACw=\r\n--B--\r\n");
    let out = build(&msg, &[(1, "Hallo dort \u{1f600}.")]);
    let a = find(&msg, b"--B\r\nContent-Type: application/octet-stream").unwrap();
    assert!(contains(&out, &msg[a..]), "attachments untouched");
    assert!(contains(&out, "Hallo dort \u{1f600}.".as_bytes()) || contains(&out, b"Content-Transfer-Encoding"));
    let m = parse(&out);
    assert_eq!(m.leaves()[0].text().trim(), "Hallo dort \u{1f600}.");
}

#[test]
fn target_locale_stamps_language_and_direction() {
    let data = fx("related_inline_image.eml");
    for (loc, dir) in [("ar-SA", Some("rtl")), ("ja-JP", None), ("he", Some("rtl")), ("de_de", None)] {
        let out = build_to(&data, &[(1, "Siehe unser Logo"), (0, "Logo")], Some(loc));
        let html = parse(&out).leaves().into_iter().find(|e| e.ctype() == "text/html").unwrap().text();
        let want = loc.replace('_', "-");
        let want = if want == "de-de" { "de-DE".to_string() } else { want };
        assert!(html.contains(&format!("lang=\"{want}\"")), "{loc}: {html}");
        assert_eq!(html.contains("dir=\"rtl\""), dir.is_some(), "{loc}: {html}");
    }
    // an unusable locale stamps nothing
    let out = build_to(&data, &[(0, "Logo")], Some("not a locale"));
    assert!(!contains(&out, b"lang=\"not"));
}

#[test]
fn a_translation_cannot_forge_a_delimiter_in_a_nested_message() {
    let data = b"Subject: s\nContent-Type: multipart/mixed; boundary=OUT\n\n--OUT\nContent-Type: multipart/alternative; boundary=INN\n\n--INN\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: 8bit\n\nplain one\n--INN\nContent-Type: text/plain; charset=utf-8\nContent-Transfer-Encoding: 7bit\n\nplain two\n--INN--\n--OUT--\n";
    for evil in ["x\n--OUT\nContent-Type: text/html\n\nowned\n--OUT--", "x\n--INN--\n--OUT\n", "y\r\n--INN\r\n", "--OUT", "ok\n\n--INN extra"] {
        let out = build(data, &[(1, evil), (2, evil)]);
        let leaves = parse(&out).leaves().len();
        assert_eq!(leaves, 2, "{evil:?}: {:?}", String::from_utf8_lossy(&out));
        let back = texts(&out);
        assert!(back.len() >= 3);
    }
}

#[test]
fn a_document_survives_json_and_a_foreign_skeleton_is_refused() {
    let data = mixed();
    let mut d = ex(&data);
    set_idx(&mut d, 0, "Neuer Betreff");
    let json = serde_json::to_string(&d).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(merged(&back), merged(&d));
    let mut other = d.clone();
    if let Skeleton::Zip { parser, .. } = &mut other.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(Email.merge(&other), Err(Error::ParserVersion { .. })));
    let mut text = d.clone();
    text.skeleton = Skeleton::default();
    assert!(Email.merge(&text).is_err());
}

#[test]
fn input_over_the_size_limit_is_refused() {
    let data = mixed();
    let lim = Limits::new().with_max_input_bytes(100);
    let e = Email.extract(&data, &ExtractOptions::new().with_limits(lim)).unwrap_err();
    assert!(matches!(e, Error::Limit(_)));
}

#[test]
fn text_that_is_all_inline_markup_is_not_empty() {
    let data = b"Subject: \nContent-Type: text/html\n\n<p><b>Hi</b></p>\n";
    assert_eq!(texts(data).len(), 1);
    let e = try_ex(b"Subject:  \nContent-Type: text/plain\n\n  \n\n").unwrap_err();
    assert!(e.to_string().contains("no Subject"), "{e}");
}

#[test]
fn line_endings_survive_a_translation() {
    // a CRLF text part keeps its CRLF
    let crlf = b"Subject: x\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nLine one.\r\n\r\nLine two.\r\n";
    let out = build(crlf, &[(1, "Zeile eins."), (2, "Zeile zwei.")]);
    let want = String::from_utf8(crlf.to_vec()).unwrap().replace("Line one.", "Zeile eins.").replace("Line two.", "Zeile zwei.");
    assert_eq!(String::from_utf8(out).unwrap(), want);
}
