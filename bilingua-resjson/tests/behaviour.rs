//! Units as Localization OS lists them on the fixtures (checked by running the LocOS filter), notes and metadata,
//! round trips, the 11 languages, UTF-16 and the escapes.

mod common;
use bilingua_model::*;
use bilingua_resjson::Resjson;
use common::*;

fn unit_by_key<'a>(d: &'a Document, pointer: &str) -> &'a Unit {
    d.units.iter().find(|u| u.context.key.as_deref() == Some(pointer)).unwrap_or_else(|| panic!("no unit {pointer}"))
}

#[test]
fn vs_template_units_match_locos() {
    let d = ex(VS_TEMPLATE);
    let want = [
        "Larchmoor Reader",
        "Welcome back, {0}!",
        "You have {0} unread {1}.",
        "%d%% of the book is read",
        "Caf\u{e9} \u{2013} d\u{e9}j\u{e0} vu / \u{1F600}",
        "First line\nSecond line\ttabbed",
        "Settings",
        "Dark",
        "Light",
        "Follow system",
        "Swipe to turn the page",
        "Tap twice to zoom",
        "Nested tip",
        "First",
        "Second",
    ];
    assert_eq!(texts(&d), want);
    assert_eq!(merged(&d), VS_TEMPLATE);
}

#[test]
fn language_fixtures_match_locos_and_agree_across_encodings() {
    let want = [
        "Welcome to Larchmoor",
        "Bienvenido a Larchmoor",
        "Bienvenue \u{e0} Larchmoor",
        "Benvenuti a Larchmoor",
        "Willkommen in Larchmoor, Gr\u{fc}\u{df}!",
        "Welkom in Larchmoor",
        "Bem-vindo a Larchmoor",
        "ラーチムアへようこそ",
        "欢迎来到拉奇莫尔",
        "라치모어에 오신 것을 환영합니다",
        "مرحبا بكم في لارتشمور",
        "Добро пожаловать в Ларчмур \u{1F600}",
        "Order #{0}: \u{202b}مرحبا\u{202c} ready",
    ];
    for (name, bytes) in [("utf8", LANGUAGES), ("crlf_bom", LANGUAGES_CRLF_BOM), ("utf16le", LANGUAGES_UTF16LE), ("utf16be", LANGUAGES_UTF16BE)] {
        let d = ex(bytes);
        assert_eq!(texts(&d), want, "{name}");
        assert_eq!(merged(&d), bytes, "{name}: untouched merge must give the file back");
    }
    let enc = |b: &[u8]| match ex(b).skeleton {
        Skeleton::Text { encoding, .. } => encoding,
        _ => unreachable!(),
    };
    assert_eq!(enc(LANGUAGES), "utf-8");
    assert_eq!(enc(LANGUAGES_CRLF_BOM), "utf-8-bom");
    assert_eq!(enc(LANGUAGES_UTF16LE), "utf-16le");
    assert_eq!(enc(LANGUAGES_UTF16BE), "utf-16be");
}

#[test]
fn all_eleven_languages_translate_and_read_back() {
    // Each language is both a source and a target text: pseudo-translate with every other language's greeting.
    for (name, bytes) in [("utf8", LANGUAGES), ("crlf_bom", LANGUAGES_CRLF_BOM), ("utf16le", LANGUAGES_UTF16LE), ("utf16be", LANGUAGES_UTF16BE)] {
        let mut d = ex(bytes);
        let src = texts(&d);
        let mut i = 0usize;
        translate_all(&mut d, |_| {
            i += 1;
            format!("{} / {}", src[i % src.len()], i)
        });
        let out = merged(&d);
        // Same encoding and byte order mark as the source.
        let head = bytes.len().min(3);
        match name {
            "utf16le" | "utf16be" => assert_eq!(&out[..2], &bytes[..2]),
            "crlf_bom" => assert_eq!(&out[..head], &bytes[..head]),
            _ => assert!(!out.starts_with(&[0xEF, 0xBB, 0xBF])),
        }
        let back = ex(&out);
        assert_eq!(back.units.len(), d.units.len(), "{name}");
        for (u, b) in d.units.iter().zip(&back.units) {
            assert_eq!(plain_text(&u.output()), plain_text(&b.source()), "{name}");
        }
        // CRLF stays CRLF.
        if name == "crlf_bom" {
            let s = String::from_utf8(out).unwrap();
            assert_eq!(s.matches('\n').count(), s.matches("\r\n").count());
        }
    }
}

#[test]
fn notes_metadata_and_whitespace() {
    let d = ex(VS_TEMPLATE);
    let title = unit_by_key(&d, "/home_title");
    assert_eq!(title.context.notes, vec!["Product name. Do not translate the brand."]);
    assert_eq!(title.context.extra.get("source").map(String::as_str), Some("Home.resx"));
    let welcome = unit_by_key(&d, "/home_welcome");
    assert_eq!(welcome.context.max_len, Some(60));
    assert_eq!(welcome.context.notes, vec!["{0} is the reader's first name."]);
    // A comment that comes after its target (`_title.comment` follows `title` here too) is joined all the same.
    let settings_title = unit_by_key(&d, "/settings/title");
    assert_eq!(settings_title.context.notes, vec!["Page title."]);
    assert_eq!(settings_title.context.max_len, Some(24));
    assert_eq!(settings_title.context.extra.get("path").map(String::as_str), Some("settings"));
    // White space around the text stays outside the unit and is written back.
    assert_eq!(plain_text(&welcome.source()), "Welcome back, {0}!");
    let mut d2 = d.clone();
    d2.set_target(&welcome.segments[0].id, vec![Inline::text("Willkommen zur\u{fc}ck, {0}!")]).unwrap();
    let out = String::from_utf8(merged(&d2)).unwrap();
    assert!(out.contains("\"  Willkommen zur\u{fc}ck, {0}!  \""), "{out}");
    // Pointers: objects, arrays, nested arrays; the LocOS context adds no array index.
    for p in ["/settings/theme/dark", "/settings/theme/auto", "/tips/0", "/tips/1", "/tips/3/0"] {
        unit_by_key(&d, p);
    }
    assert_eq!(unit_by_key(&d, "/tips/3/0").context.extra.get("path").map(String::as_str), Some("tips"));
    assert_eq!(unit_by_key(&d, "/tips/0").context.extra.get("name").map(String::as_str), Some(""));
    // The second of two equal keys is marked.
    let dups: Vec<&Unit> = d.units.iter().filter(|u| u.context.key.as_deref() == Some("/dup")).collect();
    assert_eq!(dups.len(), 2);
    assert!(!dups[0].context.extra.contains_key("duplicate"));
    assert_eq!(dups[1].context.extra.get("duplicate").map(String::as_str), Some("true"));
}

#[test]
fn locked_units_only_when_asked() {
    let plain = ex(VS_TEMPLATE);
    let locked = ex_locked(VS_TEMPLATE);
    assert!(plain.units.iter().all(|u| u.translate));
    assert!(locked.units.len() > plain.units.len());
    // Ids are sequential over the units present.
    for (i, u) in locked.units.iter().enumerate() {
        assert_eq!(u.id, format!("u{}", i + 1));
    }
    let find = |key: &str| locked.units.iter().find(|u| u.context.key.as_deref() == Some(key));
    // Placeholder-only and symbol-only values are locked; notes and metadata strings are locked; empty and blank
    // values are not units at all.
    assert!(!find("/home_code").unwrap().translate);
    assert!(!find("/home_dash").unwrap().translate);
    assert!(!find("/_home_title.comment").unwrap().translate);
    assert!(!find("/_home_title.source").unwrap().translate);
    assert!(find("/home_empty").is_none() && find("/home_blank").is_none());
    assert!(locked.units.iter().filter(|u| u.translate).count() == plain.units.len());
    assert_eq!(merged(&locked), VS_TEMPLATE);
    // A locked unit cannot be given a translation.
    let mut d = locked.clone();
    let id = find("/home_code").unwrap().segments[0].id.clone();
    assert!(matches!(d.set_target(&id, vec![Inline::text("x")]), Err(Error::Locked(_))));
}

#[test]
fn pseudo_translation_round_trips_on_every_fixture() {
    for (name, bytes) in fixtures() {
        for locked in [false, true] {
            let mut d = if locked { ex_locked(bytes) } else { ex(bytes) };
            pseudo(&mut d);
            let out = merged(&d);
            let back = if locked { ex_locked(&out) } else { ex(&out) };
            assert_eq!(back.units.len(), d.units.len(), "{name}");
            for (a, b) in d.units.iter().zip(&back.units) {
                assert_eq!(a.context.key, b.context.key, "{name}");
                assert_eq!(a.translate, b.translate, "{name}");
                assert_eq!(a.context.notes, b.context.notes, "{name}");
                assert!(a.segments[0].source.iter().all(|i| matches!(i, Inline::Text(_))));
                if a.translate {
                    assert_eq!(plain_text(&a.output()), plain_text(&b.source()), "{name}");
                }
            }
            // Pseudo-translating the result again changes the same bytes only.
            assert_ne!(out, bytes, "{name}");
        }
    }
}

#[test]
fn target_locale_adds_nothing_the_format_has_no_place_for() {
    // .resjson has no language or direction field: a merge with a target locale is the plain merge.
    for (name, bytes) in fixtures() {
        for lang in ["ar", "ja", "zh-CN", "ko", "es-ES"] {
            let d = Resjson.extract(bytes, &ExtractOptions { target_lang: Some(lang.into()), ..Default::default() }).unwrap();
            assert_eq!(d.target_lang.as_deref(), Some(lang));
            assert_eq!(merged(&d), bytes, "{name} {lang}");
        }
    }
}

#[test]
fn escapes_are_kept_untouched_and_written_once_when_changed() {
    let src = br#"{"a": "A\/B \u00e9 \ud83d\ude00 tab\there", "b": "Plain", "c": "x\\y"}"#;
    let mut d = ex(src);
    assert_eq!(texts(&d), vec!["A/B \u{e9} \u{1F600} tab\there", "Plain", "x\\y"]);
    assert_eq!(merged(&d), src.to_vec());
    // Only the changed value is re-escaped; the others keep their spelling.
    d.set_target("u2", vec![Inline::text("Simple \"q\" \\ \u{1} \u{7f} \u{2028} \u{1F600} /")]).unwrap();
    let out = String::from_utf8(merged(&d)).unwrap();
    assert_eq!(
        out,
        "{\"a\": \"A\\/B \\u00e9 \\ud83d\\ude00 tab\\there\", \"b\": \"Simple \\\"q\\\" \\\\ \\u0001 \u{7f} \u{2028} \u{1F600} /\", \"c\": \"x\\\\y\"}"
    );
    let back = ex(out.as_bytes());
    assert_eq!(texts(&back)[1], "Simple \"q\" \\ \u{1} \u{7f} \u{2028} \u{1F600} /");
}

#[test]
fn lone_surrogate_value_stays_in_the_file() {
    // A Rust string cannot hold a lone surrogate, so the value is left alone (not a unit) and written back as is.
    let src = br#"{"a": "\ud800 broken", "b": "Fine text"}"#;
    let d = ex(src);
    assert_eq!(texts(&d), vec!["Fine text"]);
    assert_eq!(merged(&d), src.to_vec());
}

#[test]
fn grammar_edges_follow_locos() {
    // Trailing commas, comments between tokens, and the BOM character as white space are accepted.
    let src = "{ /* a */ \"a\" /* b */ : // c\n \"One\" , \"o\": { \"x\": [ \"Two\", ], }, }\u{feff}\n";
    let d = ex(src.as_bytes());
    assert_eq!(texts(&d), vec!["One", "Two"]);
    assert_eq!(merged(&d), src.as_bytes());
    // Empty object and empty file contents.
    assert_eq!(ex(b"{}").units.len(), 0);
    assert_eq!(ex(b"  {  // only a comment\n}\n").units.len(), 0);
    // Numbers and words as values are accepted when valid.
    assert_eq!(ex(br#"{"a": -0.5e-3, "b": [1, 2.5, true, false, null], "c": "Text"}"#).units.len(), 1);
    // An `_id.comment` for a missing id is just metadata; a key like `_x` (no dot) is an ordinary key.
    let d = ex_locked(br#"{"_x": "Plain text", "_y.comment": "Orphan note"}"#);
    assert!(d.units[0].translate && !d.units[1].translate);
}

#[test]
fn sniff_needs_a_comment_or_note() {
    use bilingua_model::Filter;
    assert!(Resjson.sniff(VS_TEMPLATE));
    assert!(Resjson.sniff(LOCOS_SUITE));
    assert!(Resjson.sniff(LANGUAGES_UTF16LE));
    // Plain JSON stays JSON.
    assert!(!Resjson.sniff(br#"{"a": "b"}"#));
    assert!(!Resjson.sniff(b"not json at all"));
    assert!(!Resjson.sniff(b"{\"a\": // x\n"));
    assert_eq!(Resjson.extensions(), &["resjson"]);
    assert_eq!(Resjson.id(), "resjson");
}

#[test]
fn skeleton_is_stamped_and_foreign_skeletons_are_refused() {
    let d = ex(LOCOS_SUITE);
    let p = d.skeleton.parser().unwrap();
    assert_eq!(p.name, "bilingua-resjson");
    assert_eq!(p.schema, bilingua_resjson::SKELETON_SCHEMA);
    let mut foreign = d.clone();
    if let Skeleton::Text { parser, .. } = &mut foreign.skeleton {
        parser.as_mut().unwrap().name = "bilingua-json".into();
    }
    assert!(matches!(Resjson.merge(&foreign), Err(Error::ParserVersion { .. })));
    let mut odd = d.clone();
    if let Skeleton::Text { encoding, .. } = &mut odd.skeleton {
        *encoding = "latin-1".into();
    }
    assert!(matches!(Resjson.merge(&odd), Err(Error::Write(_))));
}
