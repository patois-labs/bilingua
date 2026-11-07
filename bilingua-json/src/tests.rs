use super::*;

fn extract(s: &str) -> Document {
    JsonFilter.extract(s.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn merged(d: &Document) -> String {
    String::from_utf8(JsonFilter.merge(d).unwrap()).unwrap()
}

fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

#[test]
fn round_trip_is_byte_identical_in_every_primary_language() {
    let doc = r#"{
  "en": "Welcome to Larchmoor",
  "es": "Bienvenido a Larchmoor",
  "fr": "Bienvenue \u00e0 Larchmoor",
  "it": "Benvenuti a Larchmoor",
  "de": "Willkommen in Larchmoor",
  "nl": "Welkom in Larchmoor",
  "pt": "Bem-vindo a Larchmoor",
  "ja": "ラーチムアへようこそ",
  "zh": "欢迎来到拉奇莫尔",
  "ko": "라치모어에 오신 것을 환영합니다",
  "ar": "مرحبا بكم في لارتشمور",
  "esc": "tab\there \"quoted\" back\\slash \/ slash \ud83d\ude00"
}
"#;
    let d = extract(doc);
    assert_eq!(d.units.len(), 12);
    assert_eq!(merged(&d), doc);
}

#[test]
fn pointers_keys_and_odd_formatting() {
    let doc = "\u{feff}{\"product\":{\"title\":\"Harbor lamp\",\"tags\":[\"warm light\",\"for desks\"],\"a/b\":\"slash key\",\"m~n\":\"tilde key\"},\n\t\"count\": 3 , \"on\":true,\"none\":null,\"price\":-1.5e+3}";
    let d = extract(doc);
    assert_eq!(pointers(&d), vec!["/product/title", "/product/tags/0", "/product/tags/1", "/product/a~1b", "/product/m~0n"]);
    assert_eq!(d.units[0].context.kind.as_deref(), Some("string"));
    assert_eq!(merged(&d).as_bytes(), doc.as_bytes());
    // keys are never units
    assert!(!texts(&d).iter().any(|t| t == "title" || t == "product"));
}

#[test]
fn bom_is_kept_and_not_added() {
    let with = "\u{feff}[\"Hello world\"]";
    assert_eq!(merged(&extract(with)).as_bytes(), with.as_bytes());
    assert_eq!(merged(&extract("[\"Hello world\"]")), "[\"Hello world\"]");
}

#[test]
fn translating_rewrites_only_the_string_token() {
    let doc = "{ \"a\" : \"Hello \\u00e9 there\",\n  \"b\" : \"Keep\" }";
    let mut d = extract(doc);
    d.set_target("u1", vec![Inline::text("Hola \"amigo\"\n\u{e9}\\ \u{1}")]).unwrap();
    let out = merged(&d);
    assert_eq!(out, "{ \"a\" : \"Hola \\\"amigo\\\"\\n\u{e9}\\\\ \\u0001\",\n  \"b\" : \"Keep\" }");
    // the output is valid JSON and has the same shape
    assert!(JsonFilter.sniff(out.as_bytes()));
    let again = extract(&out);
    assert_eq!(pointers(&again), pointers(&d));
}

#[test]
fn translation_keeps_non_ascii_unescaped_and_untranslated_units_keep_their_spelling() {
    let mut d = extract(r#"["Caf\u00e9 menu", "Second \/ one"]"#);
    d.set_target("u2", vec![Inline::text("Segundo / uno")]).unwrap();
    assert_eq!(merged(&d), r#"["Caf\u00e9 menu", "Segundo / uno"]"#);
    let mut d = extract(r#"["Caf\u00e9 menu"]"#);
    d.set_target("u1", vec![Inline::text("Men\u{fa} de caf\u{e9}")]).unwrap();
    assert_eq!(merged(&d), "[\"Men\u{fa} de caf\u{e9}\"]");
}

#[test]
fn placeholders_become_tags() {
    let d = extract(r#"{"m": "Hi {name}, you have {{count}} new %s items (%1$d of %2$d) and ${total}, 100%% sure %(who)s"}"#);
    let u = &d.units[0];
    let displays: Vec<String> = u
        .source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { display, kind, .. } => {
                assert_eq!(*kind, TagKind::Standalone);
                Some(display.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(displays, vec!["{name}", "{{count}}", "%s", "%1$d", "%2$d", "${total}", "%%", "%(who)s"]);
    assert_eq!(plain_text(&u.source()), "Hi , you have  new  items ( of ) and , 100 sure ");
    assert_eq!(merged(&d), r#"{"m": "Hi {name}, you have {{count}} new %s items (%1$d of %2$d) and ${total}, 100%% sure %(who)s"}"#);
}

#[test]
fn html_tags_pair_up() {
    let d = extract(r#"["Click <b>Save</b> or <a href=\"/x\">go back</a><br/>now<br>"]"#);
    let src = d.units[0].source();
    let kinds: Vec<(TagKind, String)> = src
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { kind, display, .. } => Some((*kind, display.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (TagKind::Open, "<b>".into()),
            (TagKind::Close, "</b>".into()),
            (TagKind::Open, "<a href=\"/x\">".into()),
            (TagKind::Close, "</a>".into()),
            (TagKind::Standalone, "<br/>".into()),
            (TagKind::Standalone, "<br>".into()),
        ]
    );
    let (Inline::Tag { id: i1, .. }, Inline::Tag { id: i2, .. }) = (&src[1], &src[3]) else { panic!() };
    assert_eq!(i1, i2);
}

#[test]
fn unpaired_html_tags_are_standalone_and_overlap_is_kept() {
    let d = extract(r#"["Text </b> and <i>open only", "<b>one <i>two</b> three</i>"]"#);
    for i in d.units[0].source() {
        if let Inline::Tag { kind, .. } = i {
            assert_eq!(kind, TagKind::Standalone);
        }
    }
    let kinds: Vec<TagKind> = d.units[1].source().iter().filter_map(|i| if let Inline::Tag { kind, .. } = i { Some(*kind) } else { None }).collect();
    assert_eq!(kinds, vec![TagKind::Open, TagKind::Open, TagKind::Close, TagKind::Close]);
}

#[test]
fn translated_markup_is_rebuilt_from_the_codes() {
    let mut d = extract(r#"["Click <b>Save</b> for {name}"]"#);
    let src = d.units[0].source();
    let (b_open, b_close, name) = (src[1].clone(), src[3].clone(), src[5].clone());
    d.set_target("u1", vec![Inline::text("Pulse "), b_open, Inline::text("Guardar"), b_close, Inline::text(" para "), name]).unwrap();
    assert_eq!(merged(&d), r#"["Pulse <b>Guardar</b> para {name}"]"#);
    // a tag the translator dropped is dropped from the output, never invented
    d.set_target("u1", vec![Inline::text("Guardar")]).unwrap();
    assert_eq!(merged(&d), r#"["Guardar"]"#);
    // a tag from elsewhere is refused
    assert!(d.set_target("u1", vec![Inline::tag("9", TagKind::Standalone, "{x}")]).is_err());
}

#[test]
fn values_that_are_not_prose_are_skipped() {
    let skipped = [
        "https://example.test/a?b=1",
        "mailto:hi@example.test",
        "www.example.test/x",
        "someone@example.test",
        "/api/v1/users",
        "./img/logo.png",
        "C:\\\\temp\\\\x",
        "2026-10-28",
        "2026-10-28T10:30:00Z",
        "123e4567-e89b-12d3-a456-426614174000",
        "9f86d081884c7d659a2feaa0c55ad015",
        "#ff00aa",
        "user.profile.title",
        "app_title",
        "page-title-main",
        "MAX_RETRY_COUNT",
        "userName",
        "en-US",
        "pt_BR",
        "42",
        "3.14",
        "12:30",
        "{count}",
        "%s",
        "   ",
        "",
        "...",
    ];
    for s in skipped {
        let d = extract(&format!("[\"{s}\"]"));
        assert_eq!(d.units.len(), 0, "should skip {s:?}");
        assert_eq!(merged(&d), format!("[\"{s}\"]"));
    }
    let kept = ["Home", "Sign in", "e-mail", "well-known", "OK", "Terms of use.", "iOS app", "A1 grade", "Dr. Smith", "No", "\u{4fdd}\u{5b58}", "\u{62d}\u{641}\u{638}", "\u{c800}\u{c7a5}"];
    for s in kept {
        let d = extract(&format!("[\"{s}\"]"));
        assert_eq!(d.units.len(), 1, "should keep {s:?}");
    }
}

#[test]
fn skipped_values_can_be_extracted_as_locked_units() {
    let opts = ExtractOptions::new().with_include_locked(true);
    let src = r#"{"url": "https://example.test/x", "t": "Hello", "empty": ""}"#;
    let d = JsonFilter.extract(src.as_bytes(), &opts).unwrap();
    assert_eq!(d.units.len(), 2);
    assert!(!d.units[0].translate);
    assert!(d.units[1].translate);
    assert!(d.units.iter().all(|u| u.context.key.is_some()));
    assert_eq!(JsonFilter.merge(&d).unwrap(), src.as_bytes());
}

#[test]
fn lone_surrogates_are_left_alone() {
    let src = r#"["bad \ud800 here", "good one"]"#;
    let d = extract(src);
    assert_eq!(texts(&d), vec!["good one"]);
    assert_eq!(merged(&d), src);
    let d = extract(r#"["smile \ud83d\ude00 now"]"#);
    assert_eq!(texts(&d), vec!["smile \u{1f600} now"]);
}

#[test]
fn invalid_json_is_an_error_with_a_position() {
    let bad = [
        "",
        "   ",
        "{",
        "[1,]",
        "{\"a\":}",
        "{\"a\" \"b\"}",
        "{a: 1}",
        "[\"x\"] trailing",
        "[\"unclosed]",
        "[\"bad \\q escape\"]",
        "[\"bad \\u12 escape\"]",
        "[\"tab\there\"]",
        "[01]",
        "[1.]",
        "[tru]",
        "['single']",
        "// comment\n[1]",
    ];
    for b in bad {
        match JsonFilter.extract(b.as_bytes(), &Default::default()) {
            Err(Error::Parse { msg, .. }) => assert!(msg.contains("line"), "{b:?}: {msg}"),
            other => panic!("{b:?} should be a parse error, got {other:?}"),
        }
    }
    assert!(matches!(JsonFilter.extract(&[b'[', b'"', 0xFF, 0, b'"', b']'], &Default::default()), Err(Error::Encoding(_))));
    // an odd-length UTF-16 body is not UTF-16; read as Windows-1252 it is not JSON
    assert!(matches!(JsonFilter.extract(&[0xFF, 0xFE, b'[', 0, b'x'], &Default::default()), Err(Error::Encoding(_))));
}

#[test]
fn deep_nesting_is_refused_not_crashed() {
    let deep = format!("{}{}", "[".repeat(500), "]".repeat(500));
    // the nesting cap is a limit, as in every other crate (B-SIDE 16)
    assert!(matches!(JsonFilter.extract(deep.as_bytes(), &Default::default()), Err(Error::Limit(e)) if e.kind == bilingua_model::LimitKind::Depth));
    let ok = format!("{}\"fine text\"{}", "[".repeat(100), "]".repeat(100));
    assert_eq!(extract(&ok).units.len(), 1);
    // the LocOS profile reads 300 levels (text to json.loads), refusing past 400
    let o = ExtractOptions::new().with_profile(Some(profile::LOCOS));
    let deep = format!("{}\"deep text\"{}", "[".repeat(300), "]".repeat(300));
    assert_eq!(JsonFilter.extract(deep.as_bytes(), &o).unwrap().units.len(), 1);
    let edge = format!("{}\"edge text\"{}", "[".repeat(400), "]".repeat(400));
    assert_eq!(JsonFilter.extract(edge.as_bytes(), &o).unwrap().units.len(), 1);
    let deeper = format!("{}{}", "[".repeat(401), "]".repeat(401));
    assert!(matches!(JsonFilter.extract(deeper.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == bilingua_model::LimitKind::Depth));
}

#[test]
fn scalar_top_level_and_sniffing() {
    let d = extract("\"Just a string\"");
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].context.key.as_deref(), Some(""));
    assert_eq!(merged(&d), "\"Just a string\"");
    assert!(JsonFilter.sniff(b"  {\"a\": \"b\"}"));
    assert!(!JsonFilter.sniff(b"{not json"));
    assert!(!JsonFilter.sniff(b"hello"));
    assert!(!JsonFilter.sniff(b"12"));
}

#[test]
fn output_stays_valid_json_for_awkward_translations() {
    let mut d = extract(r#"{"a": "Hello there"}"#);
    for t in ["quote \" and \\ and \n and \u{2028} and \u{7f}", "\u{1f600} emoji", "\u{645}\u{631}\u{62d}\u{628}\u{627}", "\u{4f60}\u{597d}"] {
        d.set_target("u1", vec![Inline::text(t)]).unwrap();
        let out = merged(&d);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["a"], t);
    }
}

#[test]
fn works_with_segmentation_separators() {
    let mut d = extract(r#"{"a": "One. Two."}"#);
    d.units[0].segments = vec![
        Segment::new("u1#1", vec![Inline::text("One.")]).with_target(vec![Inline::text("Uno.")]).with_state(SegState::Draft),
        Segment::new("u1#2", vec![Inline::text("Two.")]),
    ];
    d.units[0].separators = vec![" ".into()];
    assert_eq!(merged(&d), r#"{"a": "Uno. Two."}"#);
}

#[test]
fn a_vue_i18n_linked_message_is_not_text() {
    // "Zq@:a.bqZ" would look like an e-mail address, so the reference must be skipped before it is wrapped
    let d = extract(r#"{"a": "Save", "b": "@:common.save", "c": "@.lower:(common.save)", "d": "Press @:common.save now"}"#);
    assert_eq!(texts(&d), ["Save", "Press @:common.save now"]);
}

#[test]
fn value_and_string_floods_are_refused_as_limits_like_locos() {
    // LocOS check_json_budget: more than 20 x max_units commas and brackets, or 40 x max_units quotes, is a
    // limit refusal before any value is read (a file of `{},` holds no text but millions of values)
    let l = Limits::new().with_max_units(10);
    for profile in [None, Some(profile::LOCOS)] {
        let o = ExtractOptions::new().with_limits(l).with_profile(profile);
        let flood = format!("[{}{{}}]", "{},".repeat(200));
        match JsonFilter.extract(flood.as_bytes(), &o) {
            Err(Error::Limit(e)) => {
                assert_eq!(e.kind, LimitKind::Units);
                assert!(e.msg.contains("far more values than a file of 10 segments. Split"), "{}", e.msg);
            }
            other => panic!("{profile:?}: {other:?}"),
        }
        let strings = format!("[{}\"x\"]", "\"\",".repeat(200));
        assert!(matches!(JsonFilter.extract(strings.as_bytes(), &o), Err(Error::Limit(e)) if e.msg.contains("far more strings")));
        // under the budget: read as before
        let ok = format!("[{}\"Hello\"]", "{},".repeat(50));
        assert_eq!(JsonFilter.extract(ok.as_bytes(), &o).unwrap().units.len(), 1);
    }
}
