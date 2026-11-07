//! Ports of the cases in LocOS `tests/verify_yaml_filter.py` (14 test functions; the two that test LocOS web, API
//! and CLI surfaces have no Bilingua counterpart). Fixtures are the LocOS fixtures, unchanged.

mod common;
use bilingua_model::*;
use bilingua_yaml::{Limits, Yaml};
use common::*;

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

#[test]
fn identity() {
    for name in ["generic.yaml", "rails.en.yml", "messages.en.yaml"] {
        let src = fixture(name);
        assert_eq!(merge(&extract(&src)), src, "{name}");
        // every unit "translated" to its own text
        assert_eq!(all(&src, None, |t| t.to_string()), src, "{name} unchanged");
        // a localized build with nothing translated
        let mut d = extract_to(&src, "xx");
        d.target_lang = None;
        assert_eq!(merge(&d), src);
    }
    let bom = b"\xef\xbb\xbfgreeting: Hello\nother: 'World'\n".to_vec();
    assert_eq!(merge(&extract(&bom)), bom);
    let crlf = b"a: Hello\r\nb: |\r\n  Line one\r\n  Line two\r\n\r\nc: \"x\"\r\n".to_vec();
    assert_eq!(merge(&extract(&crlf)), crlf);
    assert_eq!(merge(&extract(b"a: Hello")), b"a: Hello");
    assert!(Yaml.extensions().contains(&"yml") && Yaml.extensions().contains(&"yaml"));
}

#[test]
fn walk_generic() {
    let got = texts(&fixture("generic.yaml"));
    let want = [
        "Welcome to the store",
        "It's open: come in",
        "Line one\nLine two \"quoted\" \u{e9}",
        "Hello there",
        "Goodbye",
        "#ff0000",
        "1.0",
        "12",
        "First item",
        "Second item",
        "Red",
        "Green",
        "Blue sky",
        "Keep this line.\nAnd this one.",
        "A folded paragraph that spans lines.",
        "Complex value",
        "Hello %{name}, you have {count} items and %1$s",
        "<b>Bold</b> text",
        "https://example.com/path",
    ];
    assert_eq!(got, want);
    for skipped in ["yes", "off", "42", "0.5", "2026-10-10", "Not translated either", "complex"] {
        assert!(!got.iter().any(|g| g == skipped), "{skipped}");
    }
}

#[test]
fn translated_round_trip() {
    let src = fixture("generic.yaml");
    let out = all(&src, None, |t| format!("[de-DE] {t}"));
    let want: Vec<String> = texts(&src).iter().map(|t| format!("[de-DE] {t}")).collect();
    assert_eq!(texts(&out), want);
    let text = s(&out);
    for keep in [
        "# Generic YAML fixture",
        "# trailing comment stays",
        "defaults: &defaults",
        "<<: *defaults",
        "extra: *hello",
        "enabled: yes",
        "count: 42",
        "explicit_int: !!int \"7\"",
        "ruby: !ruby/object:Foo",
        "? [complex, key]",
        "# trailing comment at end of file",
    ] {
        assert!(text.contains(keep), "{keep}");
    }
    let v = scalars(&out);
    let at = |k: &str| v.iter().position(|x| x == k).map(|i| v[i + 1].clone()).unwrap();
    assert_eq!(at("greeting"), "[de-DE] Hello there");
    assert_eq!(at("literal"), "[de-DE] Keep this line.\nAnd this one.\n");
    assert_eq!(at("folded"), "[de-DE] A folded paragraph that spans lines.");
    assert_eq!(at("enabled"), "yes");
    assert_eq!(at("count"), "42");
    let i = v.iter().position(|x| x == "inline").unwrap();
    assert_eq!(&v[i + 1..i + 6], ["[de-DE] Red", "[de-DE] Green", "label", "[de-DE] Blue sky", "literal"]);
}

#[test]
fn requoting() {
    let plain = [
        "@user", "a: b", "x #y", "yes", "123", "- item", "two\nlines", "  padded", "tab\there", "bell\x07", "sep\u{2028}x", "emoji \u{1f600} ok", "ends with colon:", "{brace}", "%{count} items", "'quote", "null",
    ];
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("k: plain\n", plain.to_vec()),
        ("k: 'single'\n", vec!["It's", "two\nlines", "bell\x07"]),
        ("k: \"double\"\n", vec!["say \"hi\"\\", "tab\t", "\u{e9}\u{e8}"]),
        ("k: |\n  literal\n  block\nnext: x\n", vec!["one\ntwo", " leading space", "single"]),
        ("k: >\n  folded\n  block\nnext: x\n", vec!["one\ntwo", "one line"]),
        ("k: [plain]\n", vec!["a, b", "c]"]),
    ];
    for (src, news) in cases {
        for new in news {
            let out = translate(src.as_bytes(), None, |i, _| (i == 0).then(|| new.to_string()));
            let v = scalars(&out);
            if src.starts_with("k: |") || src.starts_with("k: >") {
                assert_eq!(v, ["k".to_string(), format!("{new}\n"), "next".into(), "x".into()], "{src:?} {new:?} {}", s(&out));
            } else {
                assert_eq!(v, ["k", new], "{src:?} {new:?} {}", s(&out));
            }
            assert_eq!(texts(&out)[0], new.trim_matches(|c: char| c.is_whitespace()), "{src:?} {new:?}");
        }
    }
    let one = |src: &str, new: &str| s(&translate(src.as_bytes(), None, |_, _| Some(new.into())));
    assert_eq!(one("k: plain\n", "Klar"), "k: Klar\n");
    assert_eq!(one("k: 'x'\n", "Ja"), "k: 'Ja'\n");
    assert_eq!(one("k: >-\n  a\n  b\n", "x\ny"), "k: |-\n  x\n  y\n");
    let src = "k: \"  spaced  \"\n";
    assert_eq!(texts(src.as_bytes()), ["spaced"]);
    assert_eq!(scalars(&translate(src.as_bytes(), None, |_, _| Some("Raum".into())))[1], "  Raum  ");
}

#[test]
fn rails_plural_fill_729() {
    let src = concat!(
        "en:\n  files:\n    deleted:\n      one: \"%{count} file was deleted\"\n      other: \"%{count} files were deleted\"\n",
        "    flow: {one: \"a %{count}\", other: \"b %{count}\"}\n  title: Home\n"
    );
    let tr = |t: &str, i: usize| ["S-pl", "%{count} P-pl: x", "a", "b", "Dom"].get(i).map(|x| x.to_string()).filter(|_| !t.is_empty());
    let out = s(&translate(src.as_bytes(), Some("pl-PL"), |i, t| tr(t, i)));
    assert!(
        out.contains("      one: \"S-pl\"\n      few: \"%{count} P-pl: x\"\n      many: \"%{count} P-pl: x\"\n      other: \"%{count} P-pl: x\"\n"),
        "{out}"
    );
    assert!(out.starts_with("pl-PL:\n"));
    assert_eq!(out.matches("few:").count(), 1, "flow map untouched");
    let ar = s(&translate(src.as_bytes(), Some("ar"), |i, t| tr(t, i)));
    for c in ["zero:", "two:", "few:", "many:"] {
        assert_eq!(ar.matches(c).count(), 1, "{c} in {ar}");
    }
    // other untranslated: nothing added
    let only = s(&translate(src.as_bytes(), Some("pl-PL"), |i, _| (i == 0).then(|| "S".to_string())));
    assert_eq!(only.matches("few:").count(), 0);
    // a plain merge adds nothing
    assert!(!s(&translate(src.as_bytes(), None, |i, t| tr(t, i))).contains("few:"));
    // generic YAML: untouched
    let generic = src.replacen("en:\n", "app:\n", 1);
    assert!(!s(&translate(generic.as_bytes(), Some("pl-PL"), |i, t| tr(t, i))).contains("few:"));
}

#[test]
fn rails() {
    let src = fixture("rails.en.yml");
    let d = extract(&src);
    assert_eq!(d.units[0].context.extra.get("rails_locale").map(String::as_str), Some("en"));
    let t = texts(&src);
    assert!(t.contains(&"is too long (maximum is %{count} character)".to_string()) && t.contains(&"%{count} messages".to_string()));
    assert!(t.contains(&"%Y-%m-%d".to_string()));
    let out = all(&src, Some("de-DE"), |t| format!("[de-DE] {t}"));
    let o = s(&out);
    assert!(o.starts_with("# Rails i18n fixture (hand-written for verify_yaml_filter.py).\nde-DE:\n"));
    assert!(o.contains("other: \"[de-DE] %{count} messages\"") || o.contains("other: [de-DE] %{count} messages"));
    assert_eq!(o.matches("%{count}").count(), s(&src).matches("%{count}").count());
    assert!(o.contains("# Shown under a required field"));
    // plain merge never renames
    assert_eq!(s(&all(&src, None, |t| t.to_string())), s(&src));
    // underscores kept / produced (one unit translated: the root key follows the target once something is translated)
    let one = |src: &[u8], target: &str| translate(src, Some(target), |i, t| (i == 0).then(|| t.to_string()));
    assert!(s(&one(&src, "pt_BR")).contains("
pt-BR:
"));
    assert_eq!(one(b"en_US:
  hi: Hello
", "de-DE"), b"de_DE:
  hi: Hello
");
    // Norwegian `no` is a YAML 1.1 boolean: the new key is quoted
    assert_eq!(one(b"en:
  hi: Hello
", "no"), b"\"no\":
  hi: Hello
");
    // quoted source key keeps its quote style
    assert_eq!(one(b"'en':
  hi: Hello
", "fr"), b"'fr':
  hi: Hello
");
    // nothing translated: the file comes back untouched, root key included (LocOS renames anyway)
    assert_eq!(merge(&extract_to(b"en:
  hi: Hello
", "fr")), b"en:
  hi: Hello
");
    for not_rails in ["en:\n  a: A\nde:\n  a: B\n", "app:\n  a: A\n", "en: Hello\n", "en:\n  a: A\n---\nen:\n  a: B\n"] {
        let d = extract_to(not_rails.as_bytes(), "de");
        assert!(!d.units[0].context.extra.contains_key("rails_locale"), "{not_rails:?}");
        assert_eq!(merge(&d), not_rails.as_bytes(), "{not_rails:?}");
    }
}

#[test]
fn records_equivalent_keys_notes_plurals() {
    let d = extract(&fixture("rails.en.yml"));
    let by_key = |k: &str| d.units.iter().find(|u| u.context.key.as_deref() == Some(k)).unwrap_or_else(|| panic!("{k}"));
    let blank = by_key("activerecord.errors.messages.blank");
    assert_eq!(text_of(blank), "can't be blank");
    assert_eq!(blank.context.notes, ["Shown under a required field"]);
    for (k, form) in [("inbox.messages.zero", "zero"), ("inbox.messages.one", "one"), ("inbox.messages.other", "other")] {
        assert_eq!(by_key(k).context.extra.get("plural").map(String::as_str), Some(form), "{k}");
    }
    assert_eq!(text_of(by_key("inbox.messages.other")), "%{count} messages");
    assert_eq!(text_of(by_key("date.abbr_day_names[1]")), "Mon");
    let sym = extract(&fixture("messages.en.yaml"));
    let sk = |k: &str| sym.units.iter().find(|u| u.context.key.as_deref() == Some(k)).map(text_of);
    assert_eq!(sk("symfony.great").as_deref(), Some("Symfony is great"));
    assert_eq!(sk("symfony.quoted").as_deref(), Some("Quoted key, quoted value"));
    assert_eq!(sk("user.login.error").as_deref(), Some("Invalid credentials for {username}."));
    let gen = extract(&fixture("generic.yaml"));
    assert!(gen.units.iter().all(|u| u.context.key.as_deref() != Some("app.enabled")));
    assert!(gen.units.iter().any(|u| u.context.key.as_deref() == Some("app.items[0]") && text_of(u) == "First item"));
    // duplicate keys are two units
    assert_eq!(extract(b"a: One\na: Two\n").units.len(), 2);
}

#[test]
fn symfony() {
    let src = fixture("messages.en.yaml");
    let t = texts(&src);
    assert_eq!(t[0], "Symfony is great");
    assert!(t.contains(&"{0} No users|{1} One user|]1,Inf[ %count% users".to_string()));
    assert!(t.contains(&"{apples, plural, one {# apple} other {# apples}}".to_string()));
    assert!(!extract(&src).units[0].context.extra.contains_key("rails_locale"));
    let out = all(&src, Some("fr"), |t| format!("[fr] {t}"));
    let kv = kv(&out);
    assert_eq!(kv["symfony.great"], "[fr] Symfony is great");
    assert_eq!(kv["user.login.title"], "[fr] Sign in");
    assert!(s(&out).starts_with("# Symfony"), "no rename");
}

#[test]
fn encodings() {
    let bom = b"\xef\xbb\xbfgreeting: Hello\n";
    assert_eq!(translate(bom, None, |_, _| Some("Hallo".into())), b"\xef\xbb\xbfgreeting: Hallo\n");
    let crlf = b"a: Hello\r\nb: |\r\n  Line one\r\n  Line two\r\n\r\nc: \"x\"\r\n";
    let out = translate(crlf, None, |i, _| (i == 1).then(|| "Eins\nZwei\nDrei".to_string()));
    assert_eq!(out, b"a: Hello\r\nb: |\r\n  Eins\r\n  Zwei\r\n  Drei\r\n\r\nc: \"x\"\r\n");
    // UTF-16 with BOM: read, untouched comes back as it was, translated is UTF-8
    let mut u16 = vec![0xFF, 0xFE];
    for u in "greeting: Gr\u{fc}\u{df}e\n".encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    assert_eq!(texts(&u16), ["Gr\u{fc}\u{df}e"]);
    assert_eq!(extract(&u16).skeleton.parser().map(|p| p.name.as_str()), Some("bilingua-yaml"));
    assert_eq!(merge(&extract(&u16)), u16);
    assert_eq!(translate(&u16, None, |_, _| Some("Hi".into())), b"greeting: Hi\n");
    // Windows-1252
    let cp = b"greeting: caf\xe9\n";
    assert_eq!(texts(cp), ["caf\u{e9}"]);
    assert_eq!(merge(&extract(cp)), cp);
    assert_eq!(translate(cp, None, |_, _| Some("Gr\u{fc}\u{df}e".into())), "greeting: Gr\u{fc}\u{df}e\n".as_bytes());
    // binary and control characters are refused
    let bin = b"\xff\xd8\xff\x00\x81binary\x00".repeat(10);
    assert!(matches!(Yaml.extract(&bin, &opts(None)), Err(Error::Encoding(_))));
    match Yaml.extract(b"k: \x00\x01\x02 text\n", &opts(None)) {
        Err(Error::Parse { msg, .. }) => assert!(msg.contains("YAML")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn hostile() {
    let bomb = concat!(
        "a: &a [\"lol\",\"lol\",\"lol\",\"lol\",\"lol\",\"lol\",\"lol\",\"lol\",\"lol\"]\n",
        "b: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a]\nc: &c [*b,*b,*b,*b,*b,*b,*b,*b,*b]\nd: &d [*c,*c,*c,*c,*c,*c,*c,*c,*c]\n",
        "e: &e [*d,*d,*d,*d,*d,*d,*d,*d,*d]\nf: &f [*e,*e,*e,*e,*e,*e,*e,*e,*e]\ng: &g [*f,*f,*f,*f,*f,*f,*f,*f,*f]\n"
    );
    let t0 = std::time::Instant::now();
    assert_eq!(extract(bomb.as_bytes()).units.len(), 9);
    assert_eq!(merge(&extract(bomb.as_bytes())), bomb.as_bytes());
    assert!(t0.elapsed().as_secs() < 5);
    let evil = b"x: !!python/object/apply:os.system ['echo PWNED']\ny: Safe text\n";
    assert_eq!(texts(evil), ["Safe text"]);
    assert!(translate(evil, None, |_, _| Some("Sicher".into())).starts_with(b"x: !!python/object/apply:os.system"));
    let deep = format!("k: {}x{}\n", "[".repeat(500), "]".repeat(500));
    assert!(matches!(Yaml.extract(deep.as_bytes(), &opts(None)), Err(Error::Limit(_))));
    for bad in ["a: [unclosed\n", "a: b: c\n", "key: 'unterminated\n"] {
        match Yaml.extract(bad.as_bytes(), &opts(None)) {
            Err(Error::Parse { msg, .. }) => assert!(msg.contains("YAML"), "{msg}"),
            other => panic!("{bad:?}: {other:?}"),
        }
    }
    let small = { let mut built = Limits::default(); built.max_units = 5; built };
    let many: String = (0..10).map(|i| format!("k{i}: Value {i}\n")).collect();
    assert!(matches!(Yaml.extract_with(many.as_bytes(), &opts(None), &small), Err(Error::Limit(_))));
    let flow = format!("k: [{}]\n", vec!["1"; 200].join(","));
    assert!(matches!(Yaml.extract_with(flow.as_bytes(), &opts(None), &{ let mut built = Limits::default(); built.max_units = 5; built }), Err(Error::Limit(_))));
    let chars = { let mut built = Limits::default(); built.max_unit_chars = 50; built };
    let long = format!("k: {}\n", "word ".repeat(40));
    assert!(matches!(Yaml.extract_with(long.as_bytes(), &opts(None), &chars), Err(Error::Limit(_))));
}

#[test]
fn parsers_agree_multibyte_anchor() {
    // LocOS test_pure_python_parser_matches: the extra source with an anchored multi-byte value (LocOS output pinned)
    let src = "k: &a \u{65e5}\u{672c}\u{8a9e} \u{1d518}\nl: *a\nm: \"\u{e9}\"\n";
    assert_eq!(texts(src.as_bytes()), ["\u{65e5}\u{672c}\u{8a9e} \u{1d518}", "\u{e9}"]);
    let out = all(src.as_bytes(), Some("de"), |t| format!("[x] {t}"));
    assert_eq!(s(&out), "k: &a \"[x] \u{65e5}\u{672c}\u{8a9e} \u{1d518}\"\nl: *a\nm: \"[x] \u{e9}\"\n");
}

#[test]
fn gate_fixes() {
    // Rails %{name} is one tag (a dropped % is then caught as a tag problem)
    let d = extract(b"k: \"Hello %{name}, {count} left\"\n");
    let src_inl = d.units[0].source();
    let tags: Vec<&str> = src_inl.iter().filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.as_str()) } else { None }).collect();
    assert_eq!(tags, ["%{name}", "{count}"]);
    assert!(d.units[0].source().iter().all(|i| !matches!(i, Inline::Tag { free: false, kind: TagKind::Standalone, .. })));
    // LocOS keeps placeholders and HTML as plain text: every tag is labelled "placeholder" and displays its source text
    let d = extract(b"k: \"<b>Hi</b> %{name} %s\"\n");
    let u = &d.units[0];
    for i in u.source() {
        if let Inline::Tag { id, kind, display, .. } = i {
            let c = &u.codes[&id];
            assert_eq!(c.label.as_deref(), Some("placeholder"), "{display}");
            let raw = match kind {
                TagKind::Open => c.open.as_deref(),
                TagKind::Close => c.close.as_deref(),
                TagKind::Standalone => c.standalone.as_deref(),
            };
            assert_eq!(raw, Some(display.as_str()));
        }
    }
    // a 3-letter root key that is not a Rails locale is not renamed
    let src = b"nav:\n  home: Home page\n  about: About us\n";
    let mut d = extract_to(src, "de");
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Startseite")]).unwrap();
    assert_eq!(merge(&d), b"nav:\n  home: Startseite\n  about: About us\n");
    for key in ["en", "pt-BR", "zh_CN", "fil", "fil-PH", "ast"] {
        let t = format!("{key}:\n  a: Hi\n");
        assert!(extract(t.as_bytes()).units[0].context.extra.contains_key("rails_locale"), "{key}");
    }
    for key in ["nav", "spa", "app", "dev"] {
        let t = format!("{key}:\n  a: Hi\n");
        assert!(!extract(t.as_bytes()).units[0].context.extra.contains_key("rails_locale"), "{key}");
    }
}

/// The document as the LocOS adapter sees it: extracted with the profile, then annotated.
fn locos_doc(b: &[u8]) -> Document {
    let mut d = Yaml.extract(b, &profile::locos().extract_options()).unwrap();
    profile::annotate(&mut d, Some("locos"));
    d
}

/// B-side 11 and 12 (35 of 36 wild yaml files): LocOS's yaml filter offers the plain value (no inline map) and no
/// style; under the profile Rails `%{name}`, `{name}`, HTML and printf codes are text, and `locos.style` is empty.
#[test]
fn locos_profile_offers_plain_text_and_no_style() {
    let src = "a: \"Validation failed: %{errors}\"\nb: '<b>%{email}</b> and {email}'\nc: '%d-%m-%Y'\nd: plain\n";
    let d = locos_doc(src.as_bytes());
    let units = profile::locos_units(&d);
    let texts: Vec<&str> = units.iter().map(|u| u.text.as_str()).collect();
    assert_eq!(texts, ["Validation failed: %{errors}", "<b>%{email}</b> and {email}", "%d-%m-%Y", "plain"]);
    assert!(units.iter().all(|u| u.style.is_empty() && u.map.is_empty()), "{units:?}");
    // a plain translation is written back and re-reads as the same plain text (test_requoting:174)
    let mut d = d;
    let id = d.units[3].segments[0].id.clone();
    profile::locos_set_target(&mut d, &id, vec![Inline::text("{brace}")]).unwrap();
    let out = Yaml.merge_with(&d, &profile::locos().merge_options("yaml")).unwrap();
    assert_eq!(scalars(&out)[7], "{brace}");
    let again = profile::locos_units(&locos_doc(&out));
    assert_eq!(again[3].text, "{brace}");
    // without the profile the codes stay inline tags
    assert!(extract(src.as_bytes()).units[0].source().iter().any(|i| matches!(i, Inline::Tag { .. })));
}
