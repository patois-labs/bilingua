//! Round trips and the 11 test languages on the oracle corpus (`oracle/corpus/strings`).
mod common;
use bilingua_model::*;
use bilingua_strings::Strings;
use common::*;

macro_rules! corpus {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../../oracle/corpus/strings/", $name)) as &[u8])),*]
    };
}

const APP: &[(&str, &[u8])] = corpus![
    "app_en.strings", "app_es.strings", "app_fr.strings", "app_de.strings", "app_pt.strings", "app_ru.strings", "app_ar.strings",
    "app_ja.strings", "app_zh.strings", "app_ko.strings", "app_hi.strings",
];
const EDGE: &[u8] = include_bytes!("../../oracle/corpus/strings/edge.strings");
const EDGE_BRACES: &[u8] = include_bytes!("../../oracle/corpus/strings/edge_braces_crlf.strings");

fn all() -> Vec<(&'static str, &'static [u8])> {
    let mut v = APP.to_vec();
    v.push(("edge.strings", EDGE));
    v.push(("edge_braces_crlf.strings", EDGE_BRACES));
    v
}

fn enc(d: &Document) -> &str {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => encoding,
        _ => panic!("text skeleton"),
    }
}

fn pseudo(u: &Unit) -> Vec<Inline> {
    let mut v = vec![Inline::text("[")];
    v.extend(u.source());
    v.push(Inline::text("]"));
    merge_text(v)
}

#[test]
fn eleven_languages_extract_and_keep_their_bytes() {
    assert_eq!(APP.len(), 11);
    for (name, b) in APP {
        let d = ex(b);
        assert_eq!(d.units.len(), 5, "{name}");
        assert!(d.units.iter().all(|u| u.translate && u.context.notes.len() == 1 && u.context.key.is_some()), "{name}");
        assert_eq!(d.units[0].context.key.as_deref(), Some("welcome"));
        assert_eq!(d.units[0].context.notes, ["Shown on launch"]);
        // The greeting has the %@ and the free %1$@ tags in every language, the delete text has %d.
        let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
        assert_eq!(tags(&d.units[2]), 2, "{name}");
        assert_eq!(tags(&d.units[3]), 1, "{name}");
        assert_identity(b);
    }
    // Spot checks of the text itself.
    let ar = ex(APP[6].1);
    assert_eq!(shown(&ar.units[0].source()), "\u{645}\u{631}\u{62d}\u{628}\u{627}");
    assert!(shown(&ar.units[2].source()).contains("%@"));
    let ja = ex(APP[7].1);
    assert_eq!(shown(&ja.units[4].source()), "\u{8a2d}\u{5b9a}");
    let ko = ex(APP[9].1);
    assert_eq!(shown(&ko.units[4].source()), "\u{c124}\u{c815}");
}

#[test]
fn encodings_are_kept() {
    let want = [
        ("app_en.strings", "utf-8"),
        ("app_de.strings", "utf-8-bom"),
        ("app_ja.strings", "utf-16le-bom"),
        ("app_zh.strings", "utf-16le-bom"),
        ("app_ar.strings", "utf-16le-bom"),
        ("app_ko.strings", "utf-16be-bom"),
        ("app_hi.strings", "utf-16le"),
    ];
    for (name, e) in want {
        let (_, b) = APP.iter().find(|(n, _)| *n == name).unwrap();
        let d = ex(b);
        assert_eq!(enc(&d), e, "{name}");
        let mut p = d.clone();
        for u in &d.units {
            p.set_target(&u.segments[0].id, pseudo(u)).unwrap();
        }
        let out = merged(&p);
        // Same BOM / endianness / newline style after translating.
        let head = |x: &[u8]| x.iter().take(3).copied().collect::<Vec<u8>>();
        let n = match e {
            "utf-8-bom" => 3,
            "utf-8" => 0,
            "utf-16le" | "utf-16be" => 0,
            _ => 2,
        };
        assert_eq!(head(&out)[..n], head(b)[..n], "{name}");
        assert_eq!(enc(&ex(&out)), e, "{name}");
        assert_eq!(out.windows(2).any(|w| w == b"\r\n"), b.windows(2).any(|w| w == b"\r\n"), "{name}");
    }
}

#[test]
fn pseudo_translated_merge_reads_back() {
    for (name, b) in all() {
        let d = ex(b);
        let mut p = d.clone();
        for u in d.units.iter().filter(|u| u.translate) {
            p.set_target(&u.segments[0].id, pseudo(u)).unwrap();
        }
        let out = merged(&p);
        let back = ex(&out);
        assert_eq!(back.units.len(), d.units.len(), "{name}");
        for (a, b2) in d.units.iter().filter(|u| u.translate).zip(&back.units) {
            assert_eq!(shown(&b2.source()), format!("[{}]", shown(&a.source())), "{name} {}", a.id);
            assert_eq!(b2.context.key, a.context.key, "{name}");
            let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            assert_eq!(tags(b2), tags(a), "{name}");
        }
    }
}

#[test]
fn target_locale_merge_is_the_same_file() {
    // .strings carries no language tag (the folder `xx.lproj` does), so a target locale changes nothing.
    for (name, b) in all() {
        let d = Strings.extract(b, &ExtractOptions::new().with_target_lang("ar-SA")).unwrap();
        assert_eq!(d.target_lang.as_deref(), Some("ar-SA"));
        assert_eq!(merged(&d), b, "{name}");
    }
}

#[test]
fn edge_file() {
    let d = ex(EDGE);
    let got: Vec<(String, String)> = d.units.iter().map(|u| (u.context.key.clone().unwrap(), shown(&u.source()))).collect();
    let want: Vec<(&str, &str)> = vec![
        ("abc-12-xyz.text", "Title"),
        ("plain_key", "plainValue"),
        ("emoji", "Party \u{1F389} time"),
        ("octal", "Tab\tdone"),
        ("padded", "keep my spaces"),
        // LocOS treats the variable as text (its regex leaves "files@"), so it is a unit here too.
        ("stringsdict", "%#@files@"),
        ("percent", "100%% sure, %d%% done, 50% off"),
        ("multi", "line1\nline2\r\nline3\ttab"),
        ("quote", "He said \"hi\" and C:\\dir"),
        ("dup", "First"),
        ("dup", "Second"),
    ];
    let got_ref: Vec<(&str, &str)> = got.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    assert_eq!(got_ref, want);
    // `%%` stays text; `%d` and `50% off`: only the real specifier is a tag.
    let pct = d.units.iter().find(|u| u.context.key.as_deref() == Some("percent")).unwrap();
    let tags: Vec<String> = pct.source().iter().filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None }).collect();
    assert_eq!(tags, ["%d"]);
    // The padded value keeps its white space around a translation.
    let mut p = d.clone();
    set_nth(&mut p, 4, "bleib");
    let out = String::from_utf8(merged(&p)).unwrap();
    assert!(out.contains("\"padded\" = \"  bleib  \";"), "{out}");
    // An unquoted word becomes a quoted string when translated.
    set_nth(&mut p, 1, "Klartext");
    let out = String::from_utf8(merged(&p)).unwrap();
    assert!(out.contains("plain_key = \"Klartext\";"));
    // Escapes are written back.
    set_nth(&mut p, 8, "Er sagte \"hallo\"\n");
    let out = String::from_utf8(merged(&p)).unwrap();
    assert!(out.contains("\"quote\" = \"Er sagte \\\"hallo\\\"\\n\";"), "{out}");
    // Locked units on request: the empty-ish values, never touched by a merge.
    let locked = Strings.extract(EDGE, &ExtractOptions::new().with_include_locked(true)).unwrap();
    assert!(locked.units.len() > d.units.len() && locked.units.iter().any(|u| !u.translate));
    assert_eq!(merged(&locked), EDGE);
    // Notes: comments above a key.
    assert_eq!(d.units[0].context.notes, ["Class = \"UILabel\"; text = \"Title\"; ObjectID = \"abc-12-xyz\";"]);
    assert_eq!(d.units[1].context.notes, ["unquoted forms"]);
}

#[test]
fn braces_crlf_file() {
    let d = ex(EDGE_BRACES);
    assert_eq!(texts(&d), ["Alpha %1$@ and %2$@", "Beta"]);
    let a = d.units[0].source();
    assert!(a.iter().filter(|i| i.is_free()).count() == 2);
    // Moving the positional tags is allowed.
    let mut p = d.clone();
    set_nth(&mut p, 0, "%2$@ und %1$@ Alpha");
    assert!(String::from_utf8(merged(&p)).unwrap().contains("\"a\" = \"%2$@ und %1$@ Alpha\";\r\n"));
}

#[test]
fn stale_skeleton_is_refused() {
    let mut d = ex(UTF8_SMALL);
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        parser.as_mut().unwrap().name = "bilingua-po".into();
    }
    assert!(matches!(Strings.merge(&d), Err(Error::ParserVersion { .. })));
}

const UTF8_SMALL: &[u8] = b"\"a\" = \"b\";\n";

#[test]
fn sniff_and_extension() {
    assert_eq!(Strings.extensions(), ["strings"]);
    for (name, b) in all() {
        assert!(Strings.sniff(b), "{name}");
    }
    assert!(!Strings.sniff(b"hello world"));
    assert!(!Strings.sniff(b"msgid \"a\"\nmsgstr \"b\"\n"));
    assert!(!Strings.sniff(b"{\"a\": \"b\"}"));
    assert!(!Strings.sniff(b"bplist00\x00"));
    assert!(!Strings.sniff(b""));
}
