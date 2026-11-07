//! Units as Localization OS lists them on the fixtures (checked by running the LocOS filter, see the report),
//! references as tags, Culture, round trips, the 11 languages and the legacy encodings.

mod common;
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_wxl::Wxl;
use common::*;

fn s(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn unit_by_key<'a>(d: &'a Document, key: &str) -> &'a Unit {
    d.units.iter().find(|u| u.context.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no unit {key}"))
}

#[test]
fn edge_units_match_locos() {
    let d = ex(EDGE);
    let want = [
        "It's \"fine\" & ok",
        "Line one\nLine two\ttab",
        "Use <b>bold</b> & more",
        "Fish & chips \u{e9}\u{e8} <tag>",
        "Padded text",
        "Touch this",
        "1234",
        "[ProductName]",
        "[1] of [2]: [%TEMP] [!File] [$Comp] [ ] [~] [not a ref] [a b]",
        "Look-alike id",
        "No control",
        "Only an id",
    ];
    assert_eq!(texts(&d), want);
    // LocOS p_index of each (slot position among all String and UI children of the root).
    let p: Vec<&str> = d.units.iter().map(|u| u.context.extra["p_index"].as_str()).collect();
    assert_eq!(p, ["0", "1", "2", "3", "4", "10", "13", "15", "16", "17", "18", "19"]);
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u3", "u4", "u5", "u11", "u14", "u16", "u17", "u18", "u19", "u20"]);
    assert_eq!(merged(&d), EDGE);
}

#[test]
fn a_ui_without_text_is_not_a_locos_slot() {
    // LocOS (checked with its filter): a `<UI>` without `Text`, empty or with content, is not a slot, so the
    // p_index of the entries after it does not move.
    let src = b"<WixLocalization Culture=\"en-us\">\n  <UI Dialog=\"D\" Control=\"C\" />\n  <UI Dialog=\"D2\" Control=\"C2\">Inner text</UI>\n  <String Id=\"A\" Value=\"First\" />\n  <UI Dialog=\"D3\" Control=\"C3\" Text=\"Shown\" />\n  <String Id=\"B\">Second</String>\n</WixLocalization>\n";
    let d = ex(src);
    let p: Vec<&str> = d.units.iter().map(|u| u.context.extra["p_index"].as_str()).collect();
    assert_eq!(texts(&d), ["First", "Shown", "Second"]);
    assert_eq!(p, ["0", "1", "2"]);
    assert_eq!(merged(&d), src);
}

#[test]
fn include_locked_adds_the_entries_locos_counts_but_skips() {
    let d = ex_locked(EDGE);
    let locked: Vec<(String, String)> = d.units.iter().filter(|u| !u.translate).map(|u| (u.context.key.clone().unwrap(), flat(&u.source()))).collect();
    // Localizable="no" and " False ", and values with no letter or digit; an empty value has no text at all.
    let keys: Vec<&str> = locked.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["Off", "Off2", "Sym"]);
    assert!(d.units.iter().filter(|u| !u.translate).all(|u| u.lock_reason.is_some()));
    // A locked unit cannot be set, and the file merges back unchanged.
    assert!(matches!(d.clone().set_target(&unit_by_key(&d, "Off").id, vec![Inline::text("x")]), Err(Error::Locked(_))));
    assert_eq!(merged(&d), EDGE);
    // The ids of the translatable units do not move.
    let a: Vec<_> = ex(EDGE).units.iter().map(|u| u.id.clone()).collect();
    let b: Vec<_> = d.units.iter().filter(|u| u.translate).map(|u| u.id.clone()).collect();
    assert_eq!(a, b);
}

#[test]
fn references_are_free_placeholder_tags() {
    let d = ex(EDGE);
    let u = unit_by_key(&d, "Refs");
    let tags: Vec<&Inline> = u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let shown: Vec<&str> = tags.iter().map(|t| if let Inline::Tag { display, .. } = t { display.as_str() } else { "" }).collect();
    assert_eq!(shown, ["[1]", "[2]", "[%TEMP]", "[!File]", "[$Comp]", "[~]"]);
    assert!(tags.iter().all(|t| t.is_free()));
    // Every tag has its code with the label and the exact text, as the LocOS adapter needs.
    for (n, sh) in shown.iter().enumerate() {
        let c = &u.codes[&(n + 1).to_string()];
        assert_eq!(c.label.as_deref(), Some("placeholder"));
        assert_eq!(c.standalone.as_deref(), Some(*sh));
    }
    // One LocOS token per tag, and back.
    let (text, map) = to_locos_text(&u.segments[0].source);
    assert_eq!(text.matches('\u{27E6}').count(), 6);
    let back = from_locos_text(&text, &map);
    assert_eq!(back, u.segments[0].source);
    // A unit that is only a reference is still a unit (LocOS: it has a letter).
    assert_eq!(unit_by_key(&d, "OnlyRef").segments[0].source.len(), 1);
    // The text of "[not a ref]" and "[a b]" stays text.
    assert!(flat(&u.segments[0].source).ends_with("[not a ref] [a b]"));
}

#[test]
fn a_translation_may_move_a_reference() {
    let mut d = ex(LANGUAGES);
    let u = unit_by_key(&d, "Greeting").clone();
    assert_eq!(flat(&u.source()), "Hello, [UserName]. See [#ReadmeFile] in [INSTALLFOLDER].");
    let tag = |n: usize| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).nth(n).unwrap().clone();
    let target = vec![Inline::text("Siehe "), tag(1), Inline::text(" in "), tag(2), Inline::text(", "), tag(0), Inline::text("!")];
    d.set_target(&u.id, target).unwrap();
    let out = s(&merged(&d));
    assert!(out.contains(">Siehe [#ReadmeFile] in [INSTALLFOLDER], [UserName]!</String>"), "{out}");
}

#[test]
fn culture_is_rewritten_only_when_something_is_translated() {
    // Untouched merge with a target language set: the original bytes.
    let d = ex(LOCOS_PACKAGE);
    assert_eq!(d.target_lang.as_deref(), Some("de"));
    assert_eq!(merged(&d), LOCOS_PACKAGE);
    // Translated: Culture follows the target, lower case, `_` and `-` alike; Codepage and xmlns stay.
    for (lang, want) in [("de-DE", "de-de"), ("ar_SA", "ar-sa"), ("zh-Hans-CN", "zh-hans-cn"), ("ja", "ja"), ("ko-KR", "ko-kr")] {
        let mut t = ex(LOCOS_PACKAGE);
        pseudo(&mut t);
        t.target_lang = Some(lang.into());
        let out = s(&merged(&t));
        assert!(out.contains(&format!("<WixLocalization Culture=\"{want}\" Codepage=\"1252\" xmlns=")), "{lang}: {out}");
    }
    // No target language, or one LocOS would not stamp: untouched.
    for lang in [None, Some("not a tag!".to_string()), Some(String::new()), Some("x".to_string())] {
        let mut t = ex(LOCOS_PACKAGE);
        pseudo(&mut t);
        t.target_lang = lang.clone();
        let out = s(&merged(&t));
        assert!(out.contains("<WixLocalization Culture=\"en-us\" Codepage"), "{lang:?}: {out}");
    }
    // A single-quoted Culture keeps its quote; a missing one is added before the closing bracket.
    let mut t = ex(EDGE);
    pseudo(&mut t);
    t.target_lang = Some("fr-FR".into());
    let out = s(&merged(&t));
    assert!(out.contains("Culture='fr-fr' Language=\"1033\">"), "{out}");
    let no_culture = b"<?xml version=\"1.0\"?>\n<!-- a <WixLocalization Culture=\"x\"> in a comment -->\n<WixLocalization xmlns=\"urn:x\" >\n<String Id=\"a\" Value=\"Hello\"/></WixLocalization>";
    let mut t = ex(no_culture);
    pseudo(&mut t);
    let out = s(&merged(&t));
    assert!(out.contains("<WixLocalization xmlns=\"urn:x\" Culture=\"de\" >"), "{out}");
    let selfclosed = b"<WixLocalization><String Id=\"a\" Value=\"Hello\"/></WixLocalization>";
    let mut t = ex(selfclosed);
    pseudo(&mut t);
    assert!(s(&merged(&t)).starts_with("<WixLocalization Culture=\"de\"><String"));
}

#[test]
fn only_translated_values_change() {
    // Substituting the new values back with the old ones gives the original bytes exactly.
    let mut d = ex(EDGE);
    let olds: Vec<String> = d.units.iter().map(|u| flat(&u.source())).collect();
    let news: Vec<String> = olds.iter().map(|o| format!("\u{ab}{o}\u{bb}")).collect();
    for (u, n) in d.units.iter_mut().zip(&news) {
        u.segments[0].target = vec![Inline::text(n.clone())];
        u.segments[0].state = SegState::Draft;
    }
    d.target_lang = None;
    let out = s(&merged(&d));
    // The first, third and last text pieces of the edge file are re-escaped, so compare through a re-read.
    let back = ex(out.as_bytes());
    assert_eq!(texts(&back), news);
    assert!(out.contains("<String Id=\"Pad\">\n    \u{ab}Padded text\u{bb}\n  </String>"), "{out}");
    // The escaping: single quote form, entities, newline and tab as character references.
    assert!(out.contains("Value='\u{ab}It&apos;s \"fine\" &amp; ok\u{bb}'"), "{out}");
    assert!(out.contains("Value=\"\u{ab}Line one&#10;Line two&#9;tab\u{bb}\""), "{out}");
    assert!(out.contains("<![CDATA[\u{ab}Use <b>bold</b> & more\u{bb}]]>"), "{out}");
    assert!(out.contains("<String Id=\"Ent\">\u{ab}Fish &amp; chips \u{e9}\u{e8} &lt;tag&gt;\u{bb}</String>"), "{out}");
    // Everything that is not a translated value is byte for byte the original.
    let mut restored = out.clone();
    for marker in ["<!-- header comment -->", "<?pi keep?>", "<!-- a note -->", "<String Id=\"Empty\" Value=\"\" />", "<String Id=\"EmptyEl\"></String>", "<String Id=\"SelfClosed\"/>", "<String Id=\"Off\" Localizable=\"no\" Value=\"Do not touch\" />", "<String Id=\"Off2\" Localizable=\" False \">Nor this</String>", "<String Id=\"WithKids\">Has <b>kids</b></String>", "<String Id=\"WithComment\">Before<!-- c -->after</String>", "<String Id=\"Sym\" Value=\"---\" />", "<Other Value=\"Not read\" />", "<UI Dialog=\"D\" Control=\"C\" />", "<String Id=\"Nested\"><Deep Value=\"x\"/></String>"] {
        assert!(restored.contains(marker), "lost {marker}");
        restored = restored.replacen(marker, "", 1);
    }
}

#[test]
fn whitespace_around_the_text_is_kept_outside_the_unit() {
    let d = ex(EDGE);
    let u = unit_by_key(&d, "Pad");
    assert_eq!(flat(&u.source()), "Padded text");
    let mut t = d.clone();
    t.set_target(&u.id.clone(), vec![Inline::text("Gepolsterter Text")]).unwrap();
    assert!(s(&merged(&t)).contains("<String Id=\"Pad\">\n    Gepolsterter Text\n  </String>"));
}

#[test]
fn invalid_characters_in_a_translation_are_removed() {
    let mut d = ex(EDGE);
    let id = unit_by_key(&d, "Single").id.clone();
    d.set_target(&id, vec![Inline::text("a\u{1}b\u{b}c\u{fffe}d")]).unwrap();
    let out = merged(&d);
    assert!(s(&out).contains("Value='ab&#10;cd'"), "{}", s(&out));
    ex(&out); // still well-formed
}

#[test]
fn eleven_languages_translate_and_read_back_in_every_encoding() {
    for (name, bytes) in [("utf8", LANGUAGES), ("crlf_bom", LANGUAGES_CRLF_BOM), ("utf16le", LANGUAGES_UTF16LE), ("utf16be", LANGUAGES_UTF16BE)] {
        let mut d = ex(bytes);
        assert_eq!(d.units.len(), 14, "{name}");
        let src = texts(&d);
        assert!(src.iter().any(|t| t == "مرحبا بكم في لارتشمور"));
        assert!(src.iter().any(|t| t == "ラーチムアへようこそ"));
        assert!(src.iter().any(|t| t == "欢迎来到拉奇莫尔"));
        assert!(src.iter().any(|t| t == "라치모어에 오신 것을 환영합니다"));
        assert_eq!(merged(&d), bytes, "{name}: untouched merge");
        let mut i = 0usize;
        translate_all(&mut d, |_| {
            i += 1;
            format!("{} / {}", src[i % src.len()], i)
        });
        let out = merged(&d);
        let back = ex(&out);
        assert_eq!(back.units.len(), 14, "{name}");
        assert_eq!(texts(&back)[0], format!("{} / 1", src[1]));
        assert_eq!(texts(&back)[7], format!("{} / 8", src[8]));
        // The encoding the file came in is the encoding it goes out in.
        assert_eq!(out.starts_with(&[0xFF, 0xFE]), name == "utf16le");
        assert_eq!(out.starts_with(&[0xFE, 0xFF]), name == "utf16be");
        assert_eq!(out.starts_with(&[0xEF, 0xBB, 0xBF]), name == "crlf_bom");
    }
}

#[test]
fn legacy_encodings_are_read_and_written_back() {
    let cases: [(&str, &[u8], &str, &str); 6] = [
        ("windows-1252", LEGACY_1252, "windows-1252", "Willkommen in Larchmoor, Gr\u{fc}\u{df}e!"),
        ("windows-1251", LEGACY_1251, "windows-1251", "Добро пожаловать в Ларчмур"),
        ("shift_jis", LEGACY_SJIS, "shift_jis", "ラーチムアへようこそ"),
        ("gbk", LEGACY_GBK, "gbk", "欢迎来到拉奇莫尔"),
        ("euc-kr", LEGACY_EUCKR, "euc-kr", "라치모어에 오신 것을 환영합니다"),
        ("windows-1256", LEGACY_1256, "windows-1256", "مرحبا بكم في لارتشمور"),
    ];
    for (name, bytes, label, text) in cases {
        let mut d = ex(bytes);
        let second = format!("{text}!");
        assert_eq!(texts(&d), [text, second.as_str()], "{name}");
        assert_eq!(merged(&d), bytes, "{name}: untouched merge");
        // A translation in the same script goes back in the same legacy encoding, declaration included.
        translate_all(&mut d, |t| t.replace('!', "?"));
        let out = merged(&d);
        assert!(s(&out).contains(&format!("encoding=\"{label}\"")) || out.windows(label.len()).any(|w| w == label.as_bytes()), "{name}");
        let (a, b) = (text.replace('!', "?"), second.replace('!', "?"));
        assert_eq!(texts(&ex(&out)), [a.as_str(), b.as_str()], "{name}");
        assert!(std::str::from_utf8(&out).is_err(), "{name} must not have become UTF-8");
    }
    // A target character the code page lacks: the file becomes UTF-8 and says so.
    let mut d = ex(LEGACY_1251);
    translate_all(&mut d, |_| "日本語 and more".to_string());
    let out = merged(&d);
    let text = std::str::from_utf8(&out).expect("UTF-8");
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{text}");
    assert!(text.contains("日本語 and more"));
}

#[test]
fn sniff_needs_the_wix_root() {
    assert!(Wxl.sniff(LOCOS_PACKAGE));
    assert!(Wxl.sniff(EDGE));
    assert!(Wxl.sniff(LANGUAGES_UTF16LE));
    assert!(Wxl.sniff(LANGUAGES_CRLF_BOM));
    assert!(Wxl.sniff(b"<w:WixLocalization xmlns:w=\"urn:x\"/>"));
    assert!(!Wxl.sniff(b"<?xml version=\"1.0\"?><resources><string name=\"a\">x</string></resources>"));
    assert!(!Wxl.sniff(b"{\"a\": 1}"));
    assert!(!Wxl.sniff(b""));
    assert_eq!(Wxl.extensions(), &["wxl"]);
    assert_eq!(Wxl.id(), "wxl");
}

#[test]
fn a_prefixed_v3_file_reads_and_writes() {
    let data = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<w:WixLocalization xmlns:w=\"http://schemas.microsoft.com/wix/2006/localization\" Culture=\"en-US\">\n  <w:String Id=\"A\" Overridable=\"yes\">Install</w:String>\n</w:WixLocalization>\n";
    let mut d = ex(data);
    assert_eq!(texts(&d), ["Install"]);
    assert_eq!(unit_by_key(&d, "A").context.extra.get("overridable").map(String::as_str), Some("yes"));
    assert_eq!(merged(&d), data);
    translate_all(&mut d, |_| "Installieren".into());
    assert!(s(&merged(&d)).contains("<w:String Id=\"A\" Overridable=\"yes\">Installieren</w:String>"));
}

#[test]
fn paragraph_and_sentence_modes_both_round_trip() {
    for g in [Granularity::Paragraph, Granularity::Sentence, Granularity::Document] {
        let mut d = Wxl.extract(LANGUAGES, &opts(false).with_granularity(g)).unwrap();
        bilingua_seg::segment(&mut d, "en", g, bilingua_seg::Rules::bundled());
        assert_eq!(merged(&d), LANGUAGES, "{g:?}");
        // Translate every segment and read the file back: same number of units.
        let n = d.units.len();
        for u in d.units.iter_mut().filter(|u| u.translate) {
            for seg in u.segments.iter_mut() {
                let t = flat(&seg.source);
                seg.target = vec![Inline::text(format!("<{t}>"))];
            }
        }
        assert_eq!(ex(&merged(&d)).units.len(), n, "{g:?}");
    }
}

#[test]
fn a_foreign_skeleton_is_refused() {
    let mut d = ex(LOCOS_PACKAGE);
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    }
    assert!(matches!(Wxl.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn okapi_wix_config_reads_a_subset_of_what_locos_and_bilingua_read() {
    // Okapi `okf_xml-WixLocalization` (run on this file with the okapi-oracle) extracts only the text-form strings
    // with an Overridable attribute other than "no": units 1, 2 and 3 below, the same text as here. LocOS and
    // Bilingua read every string, so they also have the two Okapi leaves out.
    let d = ex(V3_OVERRIDABLE);
    assert_eq!(
        texts(&d),
        ["Overridable text with [ProductName].", "Locked by Overridable.", "No attribute at all.", "Fish & chips <here>", "1234"]
    );
    assert_eq!(unit_by_key(&d, "No").context.extra["overridable"], "no");
    assert_eq!(merged(&d), V3_OVERRIDABLE);
}

#[test]
fn comment_before_or_after_the_text_is_not_a_mixed_string() {
    // Wild file: WiX loc tools put `_locID_text` comments inside the string. One run of text around the comments is
    // the value, the comments stay; two runs of text stay a skipped mixed string.
    let src = b"<WixLocalization Culture=\"en-US\" xmlns=\"urn:x\">\n<String Id=\"A\" Overridable=\"yes\">\n<!-- _locID_text=\"A\" -->Click Install to continue.\n</String>\n<String Id=\"B\">Hello &amp; welcome<!-- note --></String>\n<String Id=\"C\">Before<!-- c -->after</String>\n<String Id=\"D\"><!-- only a note --></String>\n</WixLocalization>";
    let mut d = ex(src);
    assert_eq!(texts(&d), ["Click Install to continue.", "Hello & welcome"]);
    assert_eq!(unit_by_key(&d, "A").id, "u1");
    assert_eq!(unit_by_key(&d, "B").id, "u2");
    assert_eq!(s(&merged(&d)), s(src));
    pseudo(&mut d);
    let out = s(&merged(&d));
    assert!(out.contains("<!-- _locID_text=\"A\" -->\u{ab}"), "{out}");
    assert!(out.contains("\u{bb}<!-- note --></String>"), "{out}");
    assert!(out.contains("Before<!-- c -->after"), "{out}");
}

/// Issue 813.6: a translation's line breaks take the file's style in element text, in both modes.
#[test]
fn translation_line_breaks_follow_the_file() {
    let crlf = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<WixLocalization Culture=\"en-us\" xmlns=\"http://schemas.microsoft.com/wix/2006/localization\">\r\n  <String Id=\"A\">Hello there</String>\r\n  <String Id=\"B\" Value=\"Bye now\" />\r\n</WixLocalization>\r\n";
    let lf = crlf.replace("\r\n", "\n");
    let profile = bilingua_model::profile::locos().merge_options("wxl");
    for (src, want) in [(&crlf, "\r\n"), (&lf, "\n")] {
        for mode in 0..2 {
            let mut d = ex(src.as_bytes());
            d.units[0].segments[0].target = vec![Inline::text("Line one\nline two\r\nline three\rend")];
            d.units[1].segments[0].target = vec![Inline::text("a\nb")];
            let out = if mode == 0 { merged(&d) } else { Wxl.merge_with(&d, &profile).unwrap() };
            let out = s(&out);
            let text = format!("<String Id=\"A\">Line one{want}line two{want}line three{want}end</String>");
            assert!(out.contains(&text), "mode {mode}: {out:?}");
            assert!(out.contains("Value=\"a&#10;b\""), "attribute breaks stay character references: {out:?}");
            let bare = out.replace("\r\n", "");
            assert_eq!(bare.contains('\n'), want == "\n" && out.contains('\n'), "mode {mode}: {out:?}");
        }
    }
}
