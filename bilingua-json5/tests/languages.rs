//! The 11 test languages (including Arabic, Japanese, Chinese, Korean), the legacy encodings LocOS reads, byte
//! faithful round trips and pseudo-translated round trips.

use bilingua_json5::Json5Filter;
use bilingua_model::*;

const LANGS: [(&str, &str); 11] = [
    ("en", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es", "Bienvenido a Panader\u{ed}a Lumen. El pan fresco est\u{e1} listo a las seis de cada ma\u{f1}ana."),
    ("fr", "Bienvenue \u{e0} la Boulangerie Lumen. Le pain frais est pr\u{ea}t \u{e0} six heures chaque matin."),
    ("it", "Benvenuto al Forno Lumen. Il pane fresco \u{e8} pronto ogni mattina alle sei."),
    ("de", "Willkommen in der B\u{e4}ckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt", "Bem-vindo \u{e0} Padaria Lumen. O p\u{e3}o fresco fica pronto \u{e0}s seis horas todas as manh\u{e3}s."),
    ("ja", "\u{30eb}\u{30fc}\u{30e1}\u{30f3}\u{30fb}\u{30d9}\u{30fc}\u{30ab}\u{30ea}\u{30fc}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}\u{3002}\u{713c}\u{304d}\u{305f}\u{3066}\u{306e}\u{30d1}\u{30f3}\u{306f}\u{6bce}\u{671d}\u{516d}\u{6642}\u{306b}\u{3054}\u{7528}\u{610f}\u{3057}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}"),
    ("zh", "\u{6b22}\u{8fce}\u{5149}\u{4e34}\u{6d41}\u{660e}\u{9762}\u{5305}\u{5e97}\u{3002}\u{65b0}\u{9c9c}\u{9762}\u{5305}\u{6bcf}\u{5929}\u{65e9}\u{4e0a}\u{516d}\u{70b9}\u{51fa}\u{7089}\u{3002}"),
    ("ko", "\u{b8e8}\u{bc98} \u{bca0}\u{c774}\u{ce74}\u{b9ac}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}. \u{ac13} \u{ad6c}\u{c6b4} \u{bd75}\u{c740} \u{b9e4}\u{c77c} \u{c544}\u{ce68} \u{c5ec}\u{c12f} \u{c2dc}\u{c5d0} \u{c900}\u{be44}\u{b429}\u{b2c8}\u{b2e4}."),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{645}\u{62e}\u{628}\u{632} \u{644}\u{648}\u{645}\u{646}. \u{627}\u{644}\u{62e}\u{628}\u{632} \u{627}\u{644}\u{637}\u{627}\u{632}\u{62c} \u{62c}\u{627}\u{647}\u{632} \u{643}\u{644} \u{635}\u{628}\u{627}\u{62d} \u{641}\u{64a} \u{627}\u{644}\u{633}\u{627}\u{62f}\u{633}\u{629}."),
];

fn opts(src: &str) -> ExtractOptions {
    ExtractOptions::new().with_source_lang(src).with_target_lang("es-ES").with_granularity(Granularity::Paragraph)
}

fn pseudo(v: &[Inline]) -> Vec<Inline> {
    let mut out = vec![Inline::text("[")];
    out.extend(v.iter().cloned());
    out.push(Inline::text("]"));
    out
}

fn plain(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

/// A file with one string per language, alternating quote styles, with comments.
fn eleven() -> String {
    let mut s = String::from("// Eleven languages\n{\n");
    for (i, (c, t)) in LANGS.iter().enumerate() {
        let q = if i % 2 == 0 { '\'' } else { '"' };
        let body = t.replace('\'', "\\'");
        let body = if q == '"' { (*t).to_string() } else { body };
        s.push_str(&format!("  /* {c} */\n  {c}: {q}{body}{q},\n"));
    }
    s.push_str("}\n");
    s
}

#[test]
fn eleven_languages_extract_exactly() {
    let f = Json5Filter::default();
    let src = eleven();
    let d = f.extract(src.as_bytes(), &opts("en")).unwrap();
    assert_eq!(plain(&d), LANGS.iter().map(|l| l.1.to_string()).collect::<Vec<_>>());
    assert_eq!(f.merge(&d).unwrap(), src.as_bytes());
    for (i, (c, _)) in LANGS.iter().enumerate() {
        assert_eq!(d.units[i].context.key.as_deref(), Some(*c));
        assert_eq!(d.units[i].context.notes, vec![c.to_string()]);
    }
}

#[test]
fn eleven_languages_pseudo_translated_round_trip() {
    let f = Json5Filter::default();
    let src = eleven();
    let mut d = f.extract(src.as_bytes(), &opts("en")).unwrap();
    let ids: Vec<(String, Vec<Inline>)> = d.units.iter().map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone())).collect();
    for (id, s) in &ids {
        d.set_target(id, pseudo(s)).unwrap();
    }
    let out = f.merge(&d).unwrap();
    let back = f.extract(&out, &opts("en")).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    assert_eq!(plain(&back), LANGS.iter().map(|l| format!("[{}]", l.1)).collect::<Vec<_>>());
    // comments, keys and the surrounding layout survive; the BOM-less UTF-8 bytes stay UTF-8
    let o = String::from_utf8(out).unwrap();
    assert!(o.starts_with("// Eleven languages\n{\n  /* en */\n  en: '[Welcome"));
    assert!(o.contains("/* ar */\n  ar: '[") || o.contains("/* ar */\n  ar: \"["));
}

#[test]
fn every_language_round_trips_in_both_quote_styles() {
    let f = Json5Filter::default();
    for (c, t) in LANGS {
        for q in ['\'', '"'] {
            let body = t.replace('\\', "\\\\").replace(q, &format!("\\{q}"));
            let src = format!("{{{c}: {q}{body}{q}}}");
            let mut d = f.extract(src.as_bytes(), &opts(c)).unwrap();
            assert_eq!(plain(&d), vec![t.to_string()]);
            assert_eq!(f.merge(&d).unwrap(), src.as_bytes(), "{c} untouched");
            let id = d.units[0].segments[0].id.clone();
            d.set_target(&id, vec![Inline::text(format!("{t} \"x\" 'y'"))]).unwrap();
            let back = f.extract(&f.merge(&d).unwrap(), &opts(c)).unwrap();
            assert_eq!(plain(&back), vec![format!("{t} \"x\" 'y'")]);
        }
    }
}

#[test]
fn tags_survive_a_translation_that_moves_them() {
    let f = Json5Filter::default();
    let src = "{a: 'Hello {name}, you have %d <b>new</b> items'}";
    let mut d = f.extract(src.as_bytes(), &opts("en")).unwrap();
    let source = d.units[0].segments[0].source.clone();
    let tags: Vec<&Inline> = source.iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    assert_eq!(tags.len(), 4);
    // reorder: the named placeholder first
    let mut t = vec![tags[0].clone(), Inline::text(" hola, "), tags[1].clone(), Inline::text(" "), tags[2].clone(), Inline::text("nuevos"), tags[3].clone()];
    t.insert(0, Inline::text(""));
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, t).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert_eq!(out, "{a: '{name} hola, %d <b>nuevos</b>'}");
}

// ---------------------------------------------------------------- legacy encodings

fn encode(label: &str, text: &str) -> Vec<u8> {
    let enc = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
    let (b, _, bad) = enc.encode(text);
    assert!(!bad, "{label}");
    b.into_owned()
}

#[test]
fn legacy_code_pages_read_by_source_language_and_written_back() {
    let f = Json5Filter::default();
    // (source language, encoding label, text)
    let cases = [
        ("en", "windows-1252", "caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e"),
        ("de", "windows-1252", "Gr\u{fc}\u{df}e aus K\u{f6}ln"),
        ("ru", "windows-1251", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{438}\u{440}"),
        ("pl", "windows-1250", "Za\u{17c}\u{f3}\u{142}\u{107} g\u{119}\u{15b}l\u{105} ja\u{17a}\u{144}"),
        ("el", "windows-1253", "\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5}"),
        ("tr", "windows-1254", "\u{15e}\u{f6}yle g\u{fc}zel \u{131}\u{15f}\u{131}k"),
        ("ar", "windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645}"),
        ("ja", "shift_jis", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{4e16}\u{754c}"),
        ("zh-CN", "gbk", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}"),
        ("zh-TW", "big5", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}"),
        ("ko", "euc-kr", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}"),
    ];
    for (lang, label, text) in cases {
        let mut file = b"// legacy\n{greeting: '".to_vec();
        file.extend(encode(label, text));
        file.extend(b"', n: 1}\n");
        let mut d = f.extract(&file, &opts(lang)).unwrap_or_else(|e| panic!("{lang}: {e}"));
        assert_eq!(plain(&d), vec![text.to_string()], "{lang}");
        assert!(matches!(&d.skeleton, Skeleton::Text { encoding, .. } if encoding != "utf-8"), "{lang}");
        assert_eq!(f.merge(&d).unwrap(), file, "{lang}: untouched bytes");
        // a translation that fits the code page is written in it
        let id = d.units[0].segments[0].id.clone();
        d.set_target(&id, vec![Inline::text(text.chars().rev().collect::<String>())]).unwrap();
        let out = f.merge(&d).unwrap();
        let mut want = b"// legacy\n{greeting: '".to_vec();
        want.extend(encode(label, &text.chars().rev().collect::<String>()));
        want.extend(b"', n: 1}\n");
        assert_eq!(out, want, "{lang}: translated in the same code page");
    }
}

#[test]
fn a_translation_outside_the_code_page_is_written_as_utf8() {
    let f = Json5Filter::default();
    let mut file = b"{a: 'caf".to_vec();
    file.push(0xE9);
    file.extend(b"'}");
    let mut d = f.extract(&file, &opts("en")).unwrap();
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("\u{65e5}\u{672c}\u{8a9e}")]).unwrap();
    assert_eq!(f.merge(&d).unwrap(), "{a: '\u{65e5}\u{672c}\u{8a9e}'}".as_bytes());
}

#[test]
fn undefined_windows_1252_bytes_and_unreadable_files_are_refused() {
    let f = Json5Filter::default();
    let mut file = b"{a: 'x".to_vec();
    file.push(0x81);
    file.extend(b"y'}");
    assert!(matches!(f.extract(&file, &opts("en")), Err(Error::Encoding(_))));
}

#[test]
fn utf16_both_byte_orders_and_utf8_bom() {
    let f = Json5Filter::default();
    let text = "// c\n{a: 'Gr\u{fc}\u{df}e \u{65e5}\u{672c} \u{1f600}'}\n";
    for le in [true, false] {
        let mut b = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
        for u in text.encode_utf16() {
            b.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        let d = f.extract(&b, &opts("en")).unwrap();
        assert_eq!(plain(&d), vec!["Gr\u{fc}\u{df}e \u{65e5}\u{672c} \u{1f600}".to_string()]);
        assert_eq!(f.merge(&d).unwrap(), b);
    }
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend(text.as_bytes());
    let d = f.extract(&b, &opts("en")).unwrap();
    assert_eq!(f.merge(&d).unwrap(), b);
}

#[test]
fn locked_units_for_strings_without_letters() {
    let f = Json5Filter::default();
    let o = (opts("en")).with_include_locked(true);
    let d = f.extract(b"{a: 'Hello', b: '---', c: '', d: '12:30'}", &o).unwrap();
    // 'Hello' and '12:30' (it has digits) are units; '---' is locked context; the empty string is skipped
    let flags: Vec<(String, bool)> = d.units.iter().map(|u| (plain_text(&u.source()), u.translate)).collect();
    assert_eq!(flags, vec![("Hello".into(), true), ("---".into(), false), ("12:30".into(), true)]);
    assert_eq!(f.merge(&d).unwrap(), b"{a: 'Hello', b: '---', c: '', d: '12:30'}");
}
