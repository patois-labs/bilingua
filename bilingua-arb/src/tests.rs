use super::*;

#[test]
fn icu_parse_keeps_every_byte() {
    for s in [
        "plain",
        "Hello {name}!",
        "{n, plural, =0{none} one{# x} other{# xs}}",
        "{ count , plural , offset:1 one { # item } other { # items } }",
        "{g, select, a{A} other{B}} tail",
        "It''s '{quoted}' text",
        "{d, date, ::yMMMd} and {n, number, '{'}",
        "{a, plural, one{{b, select, x{X} other{Y}}} other{Z}}",
    ] {
        let parts = icu::parse(s).unwrap_or_else(|_| panic!("{s}"));
        assert_eq!(render(&parts), s);
    }
}

fn render(parts: &[Part]) -> String {
    let mut out = String::new();
    for p in parts {
        match p {
            Part::Text(t) | Part::Simple(t) => out.push_str(t),
            Part::Pound => out.push('#'),
            Part::Complex(cx) => {
                out.push_str(&cx.head);
                for b in &cx.branches {
                    out.push_str(&format!("{}{}{{{}}}{}", b.selector, b.mid, render(&b.msg), b.trail));
                }
                out.push('}');
            }
        }
    }
    out
}

#[test]
fn icu_rejects_what_localization_os_rejects() {
    for bad in ["{", "}", "{n, plural, one{x}", "{n, plural}x{", "{n, plural, }", "{n, plural, one x}", "{, plural, other{x}}", "'{open", "{n, number, {x}", &format!("{}x{}", "{a, select, other{".repeat(60), "}".repeat(60))] {
        assert!(icu::parse(bad).is_err(), "{bad}");
    }
    // typed arguments with and without a style, and a bare plural keyword, are simple
    assert!(matches!(icu::parse("{n, number}").unwrap()[0], Part::Simple(_)));
    assert!(matches!(icu::parse("{n, plural}").unwrap()[0], Part::Simple(_)));
    // # is text outside a plural and inside a select branch
    assert_eq!(icu::parse("# {g, select, a{#} other{#}}").unwrap().len(), 2);
}

#[test]
fn unit_order_is_pre_order() {
    let plan = plan_message("A {n, plural, one{# b {g, select, x{X} other{Y}}} other{# c}} z");
    let paths: Vec<&str> = plan.units.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(paths, ["", "n: plural one", "n: plural one / g: select x", "n: plural one / g: select other", "n: plural other"]);
}

#[test]
fn cldr_lookup_reduces_like_babel() {
    assert_eq!(plural_categories("ru", false).unwrap(), ["one", "few", "many", "other"]);
    assert_eq!(plural_categories("RU-ru", false).unwrap(), ["one", "few", "many", "other"]);
    assert_eq!(plural_categories("sr_Latn_RS", false).unwrap(), ["one", "few", "other"]);
    assert_eq!(plural_categories("en", true).unwrap(), ["one", "two", "few", "other"]);
    assert!(plural_categories("1x", false).is_none());
    assert!(BABEL_VERSION.starts_with("2."));
    // Babel aliases, which LocOS resolves (ISO 639-2, legacy codes, und = English, root = other only)
    assert_eq!(plural_categories("iw", false).unwrap(), ["one", "two", "other"]);
    assert_eq!(plural_categories("deu-AT", false).unwrap(), ["one", "other"]);
    assert_eq!(plural_categories("tl", true).unwrap(), ["one", "other"]);
    assert_eq!(plural_categories("und", false).unwrap(), ["one", "other"]);
    assert_eq!(plural_categories("root", false).unwrap(), ["other"]);
    assert!(plural_categories("zz", false).is_none());
    // both tables must stay sorted for the binary search
    for t in [cldr::TABLE, cldr::ALIASES] {
        assert!(t.windows(2).all(|w| w[0].0 < w[1].0));
    }
}

#[test]
fn placeholder_tags_flatten_to_the_source_text() {
    let src = br#"{"@@locale": "en", "hi": "Hi {name}, {n, number} {g, select, a{A {who}} other{B}}", "@hi": {"placeholders": {"name": {"type": "String", "example": "Bo"}}}, "bad": "x {y} {"}"#;
    let d = Arb.extract(src, &ExtractOptions::default()).unwrap();
    for u in &d.units {
        for i in u.source() {
            if let Inline::Tag { id, display, free: true, .. } = i {
                let c = &u.codes[&id];
                assert_eq!(c.label.as_deref(), Some("placeholder"), "{display}");
                assert_eq!(c.standalone.as_deref(), Some(display.as_str()));
                assert!(display.starts_with('{') && display.ends_with('}'));
            }
        }
    }
    assert_eq!(d.units[0].context.notes, ["{name}: String, e.g. Bo"]);
}

#[test]
fn sniff_reads_the_last_locale_like_json_loads() {
    assert!(Arb.sniff(br#"{"@@locale": "en", "a": "A"}"#));
    assert!(!Arb.sniff(br#"{"@@locale": "en", "@@locale": 1}"#));
    assert!(Arb.sniff(br#"{"@@locale": 1, "@@locale": "en"}"#));
    assert!(!Arb.sniff(br#"{"a": "A"}"#));
}

#[test]
fn scanner_matches_json_loads() {
    for ok in ["{}", " {\"a\":NaN} ", "{\"a\":[1,-0.5e+3,true,null,-Infinity]}", "{\"\\ud83d\\ude00\":\"\\u00e9\"}"] {
        assert!(scan::scan(ok, 200).is_ok(), "{ok}");
    }
    for bad in ["", "{", "{\"a\":1,}", "{\"a\" 1}", "{'a':1}", "{\"a\":01}", "{\"a\":1.}", "{\"a\":\"\t\"}", "{\"a\":\"\\x\"}", "{\"a\":\"\\u12\"}", "{} {}", "{\"a\":+1}", "{\"a\":.5}", "// c\n{}"] {
        assert!(scan::scan(bad, 200).is_err(), "{bad}");
    }
    assert_eq!(scan::unescape(r"a\u00e9\ud83d\ude00\n").unwrap(), "a\u{e9}\u{1F600}\n");
    assert!(scan::unescape(r"\ud800").is_none());
    assert_eq!(scan::quote("a\"\\\n\u{1}é"), "\"a\\\"\\\\\\n\\u0001é\"");
}
