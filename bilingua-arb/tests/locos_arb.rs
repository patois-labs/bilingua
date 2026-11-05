//! The ARB cases of the Localization OS suite `tests/verify_json_dialects_filter.py`, ported to Rust (fresh code,
//! same expectations). The suite has 17 test functions; 6 of them are other JSON dialects (Chrome, FormatJS,
//! Structured JSON, go-i18n, plain JSON, i18next keys) and belong to the JSON crate. The other 11 are here.
//!
//! LocOS puts `#` and complex arguments in unit text as `\u{27E6}xN\u{27E7}` tokens and leaves `{name}` as plain
//! text. Bilingua keeps `{name}` as a free tag; `lo()` renders a unit the way LocOS lists it so the expected
//! strings below are LocOS's own.

use bilingua_arb::{Arb, Limits};
use bilingua_model::*;
use serde_json::Value;

const MAIN: &str = include_str!("data/locos_main.arb");
const INSERT: &str = include_str!("data/locos_insert.arb");
const SUFFIX: &str = include_str!("data/locos_suffix.arb");

fn extract(text: &str) -> Document {
    Arb.extract(text.as_bytes(), &ExtractOptions::default()).unwrap()
}

/// The unit text as LocOS lists it.
fn lo(u: &Unit) -> String {
    let mut out = String::new();
    let mut n = 0;
    for i in u.source() {
        match i {
            Inline::Text(t) => out.push_str(&t),
            Inline::Tag { free: true, display, .. } => out.push_str(&display),
            Inline::Tag { .. } => {
                n += 1;
                out.push_str(&format!("\u{27E6}x{n}\u{27E7}"));
            }
        }
    }
    out
}

fn units(text: &str) -> Vec<String> {
    extract(text).units.iter().map(lo).collect()
}

/// Every unit translated to prefix + its source (tags kept). Set by position: `Document::set_target` searches
/// the whole document for the segment, which is quadratic on the 20000-message case.
fn translate_all(doc: &mut Document, prefix: &str) {
    for u in doc.units.iter_mut() {
        for s in u.segments.iter_mut() {
            let mut t = vec![Inline::text(prefix)];
            t.extend(s.source.clone());
            s.target = merge_text(t);
            s.state = SegState::Draft;
        }
    }
}

fn localized(text: &str, target: &str) -> Value {
    let mut d = extract(text);
    d.target_lang = Some(target.into());
    translate_all(&mut d, "T:");
    serde_json::from_slice(&Arb.merge(&d).unwrap()).unwrap()
}

#[test]
fn registration() {
    assert_eq!(Arb.id(), "arb");
    assert_eq!(Arb.extensions(), &["arb"]);
    assert_eq!(extract(MAIN).format, "arb");
}

#[test]
fn detection_of_the_arb_shape() {
    // test_detection: a file with a string @@locale is ARB; plain JSON, lists and strings are not
    assert!(Arb.sniff(MAIN.as_bytes()));
    assert!(Arb.sniff(&[&[0xEF, 0xBB, 0xBF][..], MAIN.as_bytes()].concat()));
    for no in [r#"{"title": "Hello", "meta": {"description": "A description"}}"#, r#"["a", "b"]"#, r#""just a string""#, r#"{"a": {"message": "x"}}"#, r#"{"@@locale": 3}"#] {
        assert!(!Arb.sniff(no.as_bytes()), "{no}");
    }
}

#[test]
fn units_skip_metadata() {
    let got = units(MAIN);
    let want = [
        "Hello {name}",
        "No items",
        "\u{27E6}x1\u{27E7} item",
        "\u{27E6}x1\u{27E7} items",
        "You have \u{27E6}x1\u{27E7} from {who}",
        "\u{27E6}x1\u{27E7} new message",
        "\u{27E6}x1\u{27E7} new messages",
        "He liked it",
        "She liked it",
        "They liked it",
        "\u{27E6}x1\u{27E7}st place",
        "\u{27E6}x1\u{27E7}th place",
        "Use '{braces}' and it''s fine",
        "4711",
    ];
    assert_eq!(got, want);
    let joined = got.join(" ");
    for meta in ["Greeting on the home page", "Shown under the cart", "Bob", "2026-10-10"] {
        assert!(!joined.contains(meta), "{meta}");
    }
}

#[test]
fn identity_round_trip_keeps_every_byte() {
    for text in [MAIN, INSERT, SUFFIX] {
        let d = extract(text);
        assert_eq!(Arb.merge(&d).unwrap(), text.as_bytes());
        // translations equal to the source text are byte-identical too
        let mut same = extract(text);
        translate_all(&mut same, "");
        assert_eq!(Arb.merge(&same).unwrap(), text.as_bytes());
    }
    let bom = [&[0xEF, 0xBB, 0xBF][..], MAIN.as_bytes()].concat();
    let d = Arb.extract(&bom, &ExtractOptions::default()).unwrap();
    assert_eq!(Arb.merge(&d).unwrap(), bom);
}

#[test]
fn translated_with_locale_and_target_plural_categories() {
    let ru = localized(MAIN, "ru-RU");
    assert_eq!(ru["@@locale"], "ru_RU");
    assert_eq!(ru["@@last_modified"], "2026-10-10T09:00:00");
    assert_eq!(ru["@title"]["description"], "Greeting on the home page");
    assert_eq!(ru["title"], "T:Hello {name}");
    assert_eq!(ru["items"], "{count, plural, =0{T:No items} one{T:# item} few{T:# items} many{T:# items} other{T:# items}}");
    assert_eq!(
        ru["mixed"],
        "T:You have {count, plural, one{T:# new message} few{T:# new messages} many{T:# new messages} other{T:# new messages}} from {who}"
    );
    assert_eq!(ru["sel"], "{gender, select, male{T:He liked it} female{T:She liked it} other{T:They liked it}}");
    assert_eq!(ru["id"], "T:4711");

    // ja: only `other` (=0 kept); selectordinal in ja has only `other` too
    let ja = localized(MAIN, "ja");
    assert_eq!(ja["items"], "{count, plural, =0{T:No items} other{T:# items}}");
    assert_eq!(ja["rank"], "{n, selectordinal, other{T:#th place}}");
    assert_eq!(ja["@@locale"], "ja");

    // en ordinals: one two few other
    let en = localized(MAIN, "en");
    assert_eq!(en["rank"], "{n, selectordinal, one{T:#st place} two{T:#th place} few{T:#th place} other{T:#th place}}");

    // ar: zero one two few many other
    let ar = localized(MAIN, "ar");
    assert_eq!(ar["items"].as_str().unwrap().matches("{T:").count(), 7); // =0 + six categories

    // a plain merge (no target language): no locale switch, no category change
    let mut d = extract(MAIN);
    translate_all(&mut d, "T:");
    let plain: Value = serde_json::from_slice(&Arb.merge(&d).unwrap()).unwrap();
    assert_eq!(plain["@@locale"], "en");
    assert_eq!(plain["items"], "{count, plural, =0{T:No items} one{T:# item} other{T:# items}}");
}

#[test]
fn locale_is_inserted_as_the_first_entry_when_absent() {
    let mut d = extract(INSERT);
    d.target_lang = Some("de-DE".into());
    d.set_target("u1", vec![Inline::text("Hallo")]).unwrap();
    let out = String::from_utf8(Arb.merge(&d).unwrap()).unwrap();
    assert_eq!(out, "{\n    \"@@locale\": \"de_DE\",\n    \"hello\": \"Hallo\",\n    \"@hello\": {\"description\": \"d\"}\n}\n");
}

#[test]
fn suffix_style_plurals_get_the_targets_missing_categories() {
    let mut d = extract(SUFFIX);
    d.target_lang = Some("cs-CZ".into());
    let tr = ["{count} polozka", "{count} polozek", "Jine", "Z", "F", "D"];
    for (i, t) in tr.iter().enumerate() {
        // the {count} placeholders stay tags
        let src = d.units[i].source();
        let mut target = vec![];
        for part in t.split_inclusive("{count}") {
            if let Some(rest) = part.strip_suffix("{count}") {
                if !rest.is_empty() {
                    target.push(Inline::text(rest));
                }
                let tag = src.iter().find(|x| matches!(x, Inline::Tag { .. })).cloned().unwrap();
                target.push(tag);
            } else {
                target.push(Inline::text(part));
            }
        }
        let id = d.units[i].segments[0].id.clone();
        d.set_target(&id, target).unwrap();
    }
    let cs = String::from_utf8(Arb.merge(&d).unwrap()).unwrap();
    let data: Value = serde_json::from_str(&cs).unwrap();
    assert_eq!(data["itemCountFew"], "{count} polozek");
    assert_eq!(data["itemCountMany"], "{count} polozek");
    assert_eq!(data["dayFew"], "F");
    assert_eq!(data["dayMany"], "D");
    assert_eq!(data["dayOne"], "D");
    let at = |k: &str| cs.find(&format!("\"{k}\"")).unwrap();
    assert!(at("itemCountOne") < at("itemCountFew") && at("itemCountFew") < at("itemCountOther"));
    assert!(cs.contains("  \"itemCountFew\": \"{count} polozek\",\n  \"itemCountMany\""), "{cs}");
    assert!(cs.contains("\"@itemCountOther\": {\"placeholders\": {\"count\": {}}},"));
    assert!(!cs.contains("showOtherFew") && !cs.contains("showFew"));
    assert_eq!(data["@@locale"], "cs_CZ");

    // ja needs only Other: nothing added; a plain merge never fills
    let mut ja = d.clone();
    ja.target_lang = Some("ja-JP".into());
    let ja: Value = serde_json::from_slice(&Arb.merge(&ja).unwrap()).unwrap();
    assert!(ja.get("itemCountFew").is_none() && ja.get("itemCountOne").is_some());
    let mut plain = d.clone();
    plain.target_lang = None;
    assert!(!String::from_utf8(Arb.merge(&plain).unwrap()).unwrap().contains("itemCountFew"));
}

#[test]
fn skeleton_carries_the_parser_stamp() {
    let d = extract(MAIN);
    let p = d.skeleton.parser().unwrap();
    assert_eq!((p.name.as_str(), p.schema), ("bilingua-arb", 1));
}

#[test]
fn hostile_inputs() {
    let ex = |b: &[u8]| Arb.extract(b, &ExtractOptions::default());
    for bad in [&br#"["not", "an", "object"]"#[..], br#""str""#, b"", b"{", br#"{"a": }"#, br#"{"a": "x",}"#] {
        assert!(matches!(ex(bad), Err(Error::Parse { .. })), "{:?}", String::from_utf8_lossy(bad));
    }
    // non-UTF-8 is refused with an honest message, not decoded with replacement characters
    match ex(b"{\"a\": \"caf\xe9\"}") {
        Err(Error::Parse { msg, .. }) => assert!(msg.contains("UTF-8"), "{msg}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(ex(&[0xFF, 0xFE, b'{', 0]), Err(Error::Unsupported(_))));
    // nesting deeper than any resource file
    let deep = format!("{{{}{}}}", "\"a\": {".repeat(300), "}".repeat(300));
    assert!(matches!(ex(deep.as_bytes()), Err(Error::Parse { .. })));
    // malformed ICU: one opaque unit, written back verbatim
    let src = r#"{"bad": "{count, plural, one{# x} other{# y}", "@bad": {}}"#;
    let d = ex(src.as_bytes()).unwrap();
    assert_eq!(d.units.iter().map(lo).collect::<Vec<_>>(), ["{count, plural, one{# x} other{# y}"]);
    assert_eq!(Arb.merge(&d).unwrap(), src.as_bytes());
    // deeply nested ICU is refused as ICU (opaque unit), never a crash
    let icu = format!("{}x{}", "{a, select, other{".repeat(60), "}".repeat(60));
    let doc = format!("{{\"d\": \"{icu}\"}}");
    assert_eq!(ex(doc.as_bytes()).unwrap().units.len(), 1);
    // limits: too many units and too large a file are typed errors (the LocOS suite lowers MAX_DOCUMENT_SEGMENTS)
    let many: String = format!("{{{}}}", (0..10).map(|i| format!("\"k{i}\": \"word {i}\"")).collect::<Vec<_>>().join(", "));
    let small = Limits { max_units: 3, ..Limits::default() };
    assert!(matches!(Arb.extract_with(many.as_bytes(), &ExtractOptions::default(), &small), Err(Error::Parse { .. })));
    let tiny = Limits { max_bytes: 20, ..Limits::default() };
    assert!(matches!(Arb.extract_with(many.as_bytes(), &ExtractOptions::default(), &tiny), Err(Error::Parse { .. })));
    // plural branches count as units
    let branches = format!("{{\"p\": \"{{n, plural, {}}}\"}}", (0..6).map(|i| format!("={i}{{w{i}}}")).collect::<Vec<_>>().join(" "));
    let four = Limits { max_units: 4, ..Limits::default() };
    assert!(Arb.extract_with(branches.as_bytes(), &ExtractOptions::default(), &four).is_err());
}

#[test]
fn nan_and_infinity_are_accepted_like_json_loads() {
    let d = extract(r#"{"a": NaN, "b": "hello there", "c": [1, -Infinity, Infinity]}"#);
    assert_eq!(d.units.len(), 1);
}

#[test]
fn rebuild_of_20000_messages_is_linear() {
    let mut s = String::from("{\n  \"@@locale\": \"en\"");
    for i in 0..20000 {
        s.push_str(&format!(",\n  \"k{i}\": \"Message number {i} with some text\""));
    }
    s.push_str("\n}\n");
    let mut d = extract(&s);
    assert_eq!(d.units.len(), 20000);
    translate_all(&mut d, "X");
    let t = std::time::Instant::now();
    let out = Arb.merge(&d).unwrap();
    assert!(t.elapsed().as_secs() < 8, "{:?}", t.elapsed());
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["k19999"], "XMessage number 19999 with some text");
}

#[test]
fn localized_plural_arb_gains_the_targets_categories() {
    // the connector case: 3 units in, a Russian plural out (few and many added)
    let src = "{\n  \"@@locale\": \"en\",\n  \"n\": \"{count, plural, one{# file} other{# files}}\",\n  \"hi\": \"Hello\"\n}\n";
    let ru = localized(src, "ru");
    assert_eq!(ru["@@locale"], "ru");
    assert!(ru["n"].as_str().unwrap().contains("few{"), "{ru}");
    assert_eq!(extract(src).units.len(), 3);
}
