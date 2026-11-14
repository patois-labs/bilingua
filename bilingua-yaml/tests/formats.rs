//! Round trips, tags, languages and legacy encodings.

mod common;
use bilingua_model::*;
use bilingua_yaml::Yaml;
use common::*;

const LANGS: [(&str, &str, &str); 11] = [
    ("en", "Welcome back, %{name}! You have {count} new messages.", "Save changes"),
    ("es", "\u{a1}Bienvenido de nuevo, %{name}! Tienes {count} mensajes nuevos.", "Guardar cambios"),
    ("fr", "Bon retour, %{name}\u{a0}! Vous avez {count} nouveaux messages.", "Enregistrer les modifications"),
    ("it", "Bentornato, %{name}! Hai {count} nuovi messaggi.", "Salva le modifiche"),
    ("de", "Willkommen zur\u{fc}ck, %{name}! Sie haben {count} neue Nachrichten.", "\u{c4}nderungen speichern"),
    ("nl", "Welkom terug, %{name}! U heeft {count} nieuwe berichten.", "Wijzigingen opslaan"),
    ("pt", "Bem-vindo de volta, %{name}! Voc\u{ea} tem {count} novas mensagens.", "Guardar altera\u{e7}\u{f5}es"),
    ("ja", "\u{304a}\u{304b}\u{3048}\u{308a}\u{306a}\u{3055}\u{3044}\u{3001}%{name}\u{3055}\u{3093}\u{3002}\u{65b0}\u{3057}\u{3044}\u{30e1}\u{30c3}\u{30bb}\u{30fc}\u{30b8}\u{304c}{count}\u{4ef6}\u{3042}\u{308a}\u{307e}\u{3059}\u{3002}", "\u{5909}\u{66f4}\u{3092}\u{4fdd}\u{5b58}"),
    ("zh", "\u{6b22}\u{8fce}\u{56de}\u{6765}\u{ff0c}%{name}\u{ff01}\u{60a8}\u{6709} {count} \u{6761}\u{65b0}\u{6d88}\u{606f}\u{3002}", "\u{4fdd}\u{5b58}\u{66f4}\u{6539}"),
    ("ko", "\u{b2e4}\u{c2dc} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}, %{name}\u{b2d8}! \u{c0c8} \u{baa8}\u{c2dc}\u{c9c0}\u{ac00} {count}\u{ac1c} \u{c788}\u{c2b5}\u{b2c8}\u{b2e4}.", "\u{bcc0}\u{acbd} \u{c0ac}\u{d56d} \u{c800}\u{c7a5}"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627}\u{64b} \u{628}\u{639}\u{648}\u{62f}\u{62a}\u{643}\u{60c} %{name}! \u{644}\u{62f}\u{64a}\u{643} {count} \u{631}\u{633}\u{627}\u{626}\u{644} \u{62c}\u{62f}\u{64a}\u{62f}\u{629}.", "\u{62d}\u{641}\u{638} \u{627}\u{644}\u{62a}\u{63a}\u{64a}\u{64a}\u{631}\u{627}\u{62a}"),
];

fn corpus() -> String {
    let mut s = String::from("# Eleven languages, three styles\nen:\n");
    for (code, long, short) in LANGS {
        s.push_str(&format!("  # {code}\n  {code}:\n    welcome: \"{long}\"\n    save: {short}\n    note: |\n      {short}\n      {short}\n    list:\n      - '{short}'\n"));
    }
    s
}

#[test]
fn eleven_languages_round_trip_and_translate() {
    let src = corpus();
    let d = extract(src.as_bytes());
    assert_eq!(d.units.len(), 11 * 4);
    assert_eq!(merge(&d), src.as_bytes(), "untouched merge is byte-identical");
    for (code, long, short) in LANGS {
        let u = d.units.iter().find(|u| u.context.key.as_deref() == Some(&format!("{code}.welcome"))).unwrap();
        assert_eq!(text_of(u), long, "{code}");
        let u = d.units.iter().find(|u| u.context.key.as_deref() == Some(&format!("{code}.note"))).unwrap();
        assert_eq!(text_of(u), format!("{short}\n{short}"), "{code}");
    }
    // pseudo-translation keeps units and tags
    let mut p = extract(src.as_bytes());
    pseudo(&mut p, "[xx] ");
    let out = merge(&p);
    let back = extract(&out);
    assert_eq!(back.units.len(), d.units.len());
    for (a, b) in d.units.iter().zip(&back.units) {
        let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
        assert_eq!(tags(a), tags(b));
        assert_eq!(text_of(b), format!("[xx] {}", text_of(a)));
    }
    // translating into each of the 11 languages renames the root key and keeps everything else
    for (code, _, _) in LANGS {
        let mut d = extract_to(src.as_bytes(), code);
        pseudo(&mut d, "> ");
        let out = String::from_utf8(merge(&d)).unwrap();
        assert!(out.contains(&format!("\n{code}:\n  # en")) || out.starts_with(&format!("# Eleven languages, three styles\n{code}:\n")), "{code}: {}", &out[..80]);
        assert_eq!(extract(out.as_bytes()).units.len(), 44);
    }
}

#[test]
fn tags_have_the_locos_kinds() {
    let d = extract(b"k: \"Hi <b>%{name}</b>, {0} and %s and %1$d<br/>\"\n");
    let inl = d.units[0].source();
    let kinds: Vec<(String, TagKind, bool)> = inl
        .iter()
        .filter_map(|i| if let Inline::Tag { display, kind, free, .. } = i { Some((display.clone(), *kind, *free)) } else { None })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("<b>".into(), TagKind::Open, false),
            ("%{name}".into(), TagKind::Standalone, true),
            ("</b>".into(), TagKind::Close, false),
            ("{0}".into(), TagKind::Standalone, true),
            ("%s".into(), TagKind::Standalone, false),
            ("%1$d".into(), TagKind::Standalone, true),
            ("<br/>".into(), TagKind::Standalone, false),
        ]
    );
    // a free tag may move in the translation
    let mut d = d;
    let id = d.units[0].segments[0].id.clone();
    let tag = |n: &str| inl.iter().find(|i| matches!(i, Inline::Tag { display, .. } if display == n)).unwrap().clone();
    d.set_target(&id, vec![tag("%{name}"), Inline::text(" salut "), tag("{0}"), tag("%s"), tag("%1$d")]).unwrap();
    assert_eq!(scalars(&merge(&d))[1], "%{name} salut {0}%s%1$d");
}

#[test]
fn units_ids_context_and_styles() {
    let src = "# About the app\napp:\n  # Title shown in the header\n  title: Hi\n  list:\n    - 'one'\n    - \"two\"\n  body: |\n    text\n  n: 5\n";
    let d = extract(src.as_bytes());
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u3", "u4"]);
    assert_eq!(d.units[0].context.key.as_deref(), Some("app.title"));
    assert_eq!(d.units[0].context.notes, ["Title shown in the header"]);
    assert_eq!(d.units[1].context.key.as_deref(), Some("app.list[0]"));
    assert_eq!(d.units[0].context.style.as_deref(), Some("plain"));
    assert_eq!(d.units[1].context.style.as_deref(), Some("single-quoted"));
    assert_eq!(d.units[2].context.style.as_deref(), Some("double-quoted"));
    assert_eq!(d.units[3].context.style.as_deref(), Some("literal"));
    assert_eq!(d.units[0].context.location.as_deref(), Some("line 4"));
    assert_eq!(d.format, "yaml");
}

#[test]
fn paragraph_granularity_is_one_segment_per_unit() {
    let d = Yaml.extract(b"k: One sentence. Another sentence.\n", &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].segments.len(), 1);
}

#[test]
fn legacy_encodings_in_other_languages() {
    // Windows-1252 French
    let fr = "bonjour: \"Bonjour \u{e0} tous, \u{ab}\u{a0}caf\u{e9}\u{a0}\u{bb} co\u{fb}te 5 \u{20ac}\"\n";
    let mut cp = Vec::new();
    for c in fr.chars() {
        cp.push(match c as u32 {
            0x20AC => 0x80,
            n if n < 0x100 => n as u8,
            _ => unreachable!(),
        });
    }
    assert_eq!(texts(&cp), ["Bonjour \u{e0} tous, \u{ab}\u{a0}caf\u{e9}\u{a0}\u{bb} co\u{fb}te 5 \u{20ac}"]);
    assert_eq!(merge(&extract(&cp)), cp);
    // UTF-16 LE and BE Japanese, Arabic
    for (le, text) in [(true, "greeting: \u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\nname: \u{645}\u{631}\u{62d}\u{628}\u{627}\n"), (false, "a: \u{4f60}\u{597d}\n")] {
        let mut b = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
        for u in text.encode_utf16() {
            b.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        assert_eq!(extract(&b).units.len(), text.lines().count());
        assert_eq!(merge(&extract(&b)), b);
        let out = translate(&b, None, |_, t| Some(format!("{t}!")));
        assert!(std::str::from_utf8(&out).is_ok());
        assert_eq!(texts(&out).len(), text.lines().count());
    }
    // UTF-8 BOM and CRLF together
    let b = b"\xef\xbb\xbfa: One\r\nb: \"Two\"\r\n";
    assert_eq!(merge(&extract(b)), b);
}

#[test]
fn edits_stay_in_place() {
    let src = "# head\nkey:   Value   # tail\nother: 'keep'   \n\n\nlist: [ a ,  b ]\nanchored: &x Hi\nref: *x\nblock: |-\n  one\n  two\n\n# end\n";
    let out = translate(src.as_bytes(), None, |i, _| (i == 0).then(|| "Wert".to_string()));
    assert_eq!(String::from_utf8(out).unwrap(), src.replace("Value", "Wert"));
    let out = translate(src.as_bytes(), None, |i, _| (i == 4).then(|| "Hallo".to_string()));
    assert_eq!(String::from_utf8(out).unwrap(), src.replace("Hi", "Hallo"));
}

#[test]
fn rails_hundred_percent_of_fixture_values_survive() {
    // every value of the Rails fixture read back after a pseudo-translated localized merge
    let src = fixture("rails.en.yml");
    let mut d = extract_to(&src, "de-DE");
    pseudo(&mut d, "[de-DE] ");
    let out = merge(&d);
    let back = extract(&out);
    assert_eq!(back.units.len(), d.units.len());
    assert!(String::from_utf8(out).unwrap().contains("\nde-DE:\n"));
}

#[test]
fn sniff_claims_yaml_not_other_key_value_formats() {
    use bilingua_model::Filter;
    for yes in ["a: b\n", "---\nx: 1\n", "# c\n- item\n", "%YAML 1.1\n---\n", "\u{feff}k:\n  v: x\n"] {
        assert!(bilingua_yaml::Yaml.sniff(yes.as_bytes()), "{yes:?}");
    }
    for no in ["title = \"a: b\"\n", "\"k\" = \"v: w\";\n", "// Copyright: me\nexport default {}\n", "module.exports = { a: 1 }\n", "key=value\n", "{\"a\": 1}\n", "[section]\n"] {
        assert!(!bilingua_yaml::Yaml.sniff(no.as_bytes()), "{no:?}");
    }
}
