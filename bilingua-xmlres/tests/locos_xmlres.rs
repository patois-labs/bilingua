//! Port of the Android and Java XML properties cases of the Localization OS suite
//! `tests/verify_xml_resource_filters.py` (`test_android_units_and_skips`, `test_android_escaping_on_build`,
//! `test_android_raw_line_break_706`, `test_android_plurals_localized`, `test_java_xml_properties`, and the `.xml`
//! checks of `test_hostile_inputs`), plus the golden file `tests/data/locos_golden.json` written by
//! `tests/tools/gen_golden.py` from the LocOS filters (units, translated build, localized builds for pl and ja).

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_xmlres::{AndroidStrings, JavaXmlProperties};

const STRINGS: &[u8] = include_bytes!("data/strings.xml");
const MESSAGES: &[u8] = include_bytes!("data/messages.xml");
const GOLDEN: &str = include_str!("data/locos_golden.json");

fn filter_for(data: &[u8]) -> Box<dyn Filter> {
    if AndroidStrings.sniff(data) {
        Box::new(AndroidStrings)
    } else {
        assert!(JavaXmlProperties.sniff(data), "neither root");
        Box::new(JavaXmlProperties)
    }
}

fn opts(lang: &str) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(lang).with_granularity(Granularity::Paragraph)
}

/// The units as LocOS shows them (its token text).
fn locos_units(d: &Document) -> Vec<String> {
    d.units.iter().filter(|u| u.translate).map(|u| to_locos_text(&u.source()).0).collect()
}

/// Translates every unit from LocOS token texts, in order.
fn translate(f: &dyn Filter, data: &[u8], lang: &str, texts: &[String]) -> Vec<u8> {
    let mut d = f.extract(data, &opts(lang)).unwrap();
    let todo: Vec<(String, Vec<Inline>)> = d
        .units
        .iter()
        .filter(|u| u.translate)
        .zip(texts)
        .map(|(u, t)| {
            let (_, map) = to_locos_text(&u.source());
            (u.segments[0].id.clone(), from_locos_text(t, &map))
        })
        .collect();
    for (id, t) in todo {
        d.set_target(&id, t).unwrap();
    }
    f.merge(&d).unwrap()
}

fn by_text(f: &dyn Filter, data: &[u8], pairs: &[(&str, &str)]) -> String {
    let d = f.extract(data, &opts("fr")).unwrap();
    let mut texts = locos_units(&d);
    for (src, tr) in pairs {
        let i = texts.iter().position(|t| t == src).unwrap_or_else(|| panic!("{src:?} is not a unit"));
        texts[i] = tr.to_string();
    }
    String::from_utf8(translate(f, data, "", &texts)).unwrap()
}

#[test]
fn golden_units_and_builds_match_locos() {
    let g: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    let mut n = 0;
    for (name, case) in g.as_object().unwrap() {
        let data = case["input"].as_str().unwrap().as_bytes();
        let f = filter_for(data);
        let d = f.extract(data, &opts("fr")).unwrap();
        let want: Vec<String> = case["units"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert_eq!(locos_units(&d), want, "{name}: units");
        assert_eq!(f.merge(&d).unwrap(), data, "{name}: an untouched merge is the original bytes");
        let tr: Vec<String> = case["translations"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        // no target language: the translated build, byte for byte
        let out = String::from_utf8(translate(f.as_ref(), data, "", &tr)).unwrap();
        assert_eq!(out, case["built"].as_str().unwrap(), "{name}: translated build");
        for (lang, key) in [("pl", "built_pl"), ("ja", "built_ja")] {
            if let Some(want) = case.get(key) {
                let out = String::from_utf8(translate(f.as_ref(), data, lang, &tr)).unwrap();
                assert_eq!(out, want.as_str().unwrap(), "{name}: localized build for {lang}");
            }
        }
        n += 1;
    }
    assert_eq!(n, 9, "golden cases");
}

#[test]
fn android_units_and_skips() {
    let d = AndroidStrings.extract(STRINGS, &opts("fr")).unwrap();
    let texts = locos_units(&d);
    assert!(texts.contains(&"Can't open the file \"%s\".".to_string()));
    assert!(texts.contains(&"@home is the default tag".to_string()));
    assert!(!texts.contains(&"Northwind".to_string()), "translatable=false");
    assert!(!texts.iter().any(|t| t.starts_with("@string") || t.starts_with("?android")), "references");
    assert!(!texts.contains(&"MRC".to_string()) && !texts.contains(&"#FF4081".to_string()) && !texts.contains(&"16dp".to_string()));
    assert!(texts.contains(&"First line\\nSecond line".to_string()));
    assert!(texts.contains(&"It's   spaced   out".to_string()));
    assert!(texts.contains(&"Tap ⟦g1⟧Save⟦/g1⟧ to keep ⟦g2⟧your⟦/g2⟧ changes.".to_string()));
    assert!(texts.contains(&"Signed in as ⟦x1⟧.".to_string()));
    assert_eq!(AndroidStrings.merge(&d).unwrap(), STRINGS);
    // keys, plural forms, array positions and the comment before a resource
    let songs: Vec<&Unit> = d.units.iter().filter(|u| u.context.key.as_deref() == Some("songs")).collect();
    assert_eq!(songs.iter().map(|u| u.context.extra["plural"].as_str()).collect::<Vec<_>>(), ["one", "other"]);
    let venus = d.units.iter().find(|u| plain_text(&u.source()) == "Venus").unwrap();
    assert_eq!((venus.context.key.as_deref(), venus.context.extra.get("index").map(String::as_str)), (Some("planets"), Some("1")));
}

#[test]
fn android_escaping_on_build() {
    let out = by_text(
        &AndroidStrings,
        STRINGS,
        &[
            ("Can't open the file \"%s\".", "Impossible d'ouvrir \"%s\"."),
            ("@home is the default tag", "@accueil est l'étiquette"),
            ("It's   spaced   out", "C'est   espacé"),
            ("First line\\nSecond line", "Première\\nSeconde"),
            ("Signed in as ⟦x1⟧.", "Connecté : ⟦x1⟧."),
            ("Tap ⟦g1⟧Save⟦/g1⟧ to keep ⟦g2⟧your⟦/g2⟧ changes.", "Touchez ⟦g1⟧Enregistrer⟦/g1⟧ pour garder ⟦g2⟧vos⟦/g2⟧ modifications d'abord."),
            ("Read the <a href=\"https://example.com/terms\">terms</a> first.", "Lisez d'abord."),
        ],
    );
    assert!(out.contains(r#"<string name="cant_open">Impossible d\'ouvrir \"%s\".</string>"#), "{out}");
    assert!(out.contains(r#"<string name="at_literal">\@accueil est l\'étiquette</string>"#), "{out}");
    assert!(out.contains(r#"<string name="quoted">"C\'est   espacé"</string>"#), "{out}");
    assert!(out.contains(r"Première\nSeconde</string>"));
    assert!(out.contains(r#"<xliff:g id="user" example="Ana">%1$s</xliff:g>"#));
    assert!(!out.split("<resources").nth(1).unwrap().split_once('>').unwrap().1.contains("xmlns:xliff"), "ns re-declared");
    assert!(out.contains(r"<b>Enregistrer</b> pour garder <i>vos</i> modifications d\'abord."), "{out}");
    assert!(out.contains(r"<![CDATA[Lisez d\'abord.]]>"), "{out}");
    assert!(out.contains(r#"<string name="welcome">Welcome, %1$s! You have %2$d new messages.</string>"#));
    assert!(out.contains(r#"<string name="alias">@string/welcome</string>"#));
    let again = locos_units(&AndroidStrings.extract(out.as_bytes(), &opts("fr")).unwrap());
    assert!(again.contains(&"Impossible d'ouvrir \"%s\".".to_string()) && again.contains(&"@accueil est l'étiquette".to_string()));
    // an unescaped apostrophe never survives into an unquoted value (aapt2 would refuse the build)
    for line in out.lines().filter(|l| l.contains("<string name=")) {
        let v = line.split_once('>').unwrap().1;
        if !v.starts_with('"') {
            let b = v.as_bytes();
            assert!(!(1..b.len()).any(|i| b[i] == b'\'' && b[i - 1] != b'\\') && !v.starts_with('\''), "{v}");
        }
    }
}

#[test]
fn android_raw_line_break_706() {
    let data = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n    <string name=\"two\">Your download is complete.\\nOpen the file.</string>\n    <string name=\"kept\">One\\nTwo</string>\n    <string name=\"plain\">No break here.</string>\n    <plurals name=\"left\">\n        <item quantity=\"one\">%d file left.\\nKeep going.</item>\n        <item quantity=\"other\">%d files left.\\nKeep going.</item>\n    </plurals>\n</resources>\n";
    let out = by_text(
        &AndroidStrings,
        data,
        &[
            ("Your download is complete.\\nOpen the file.", "Ihr Download ist abgeschlossen.\nÖffnen Sie die Datei."),
            ("One\\nTwo", "Eins\\n\nZwei"),
            ("No break here.", "Hier kein\nUmbruch."),
            ("%d file left.\\nKeep going.", "Noch %d Datei.\r\n  Weiter so."),
            ("%d files left.\\nKeep going.", "Noch %d Dateien.\\nWeiter so."),
        ],
    );
    assert!(out.contains("Ihr Download ist abgeschlossen.\\nÖffnen Sie die Datei.</string>"), "{out}");
    assert!(out.contains("<string name=\"kept\">Eins\\nZwei</string>"), "{out}");
    assert!(out.contains("Hier kein\nUmbruch.</string>"), "{out}");
    assert!(out.contains("Noch %d Datei.\\nWeiter so.</item>"), "{out}");
    assert!(out.contains("Noch %d Dateien.\\nWeiter so.</item>"), "{out}");
}

#[test]
fn android_plurals_follow_the_target_language() {
    let d = AndroidStrings.extract(STRINGS, &opts("pl")).unwrap();
    let mut texts = locos_units(&d);
    let other = texts.iter().position(|t| t == "%d songs found.").unwrap();
    texts[other] = "Znaleziono %d utworów.".into();
    let out = String::from_utf8(translate(&AndroidStrings, STRINGS, "pl", &texts)).unwrap();
    let block = out.split("<plurals name=\"songs\">").nth(1).unwrap().split("</plurals>").next().unwrap();
    for q in ["one", "few", "many", "other"] {
        assert!(block.contains(&format!("quantity=\"{q}\"")), "{q}: {block}");
    }
    assert_eq!(block.matches("Znaleziono %d utworów.").count(), 3, "other + new few + new many: {block}");
    // Japanese keeps only other, once something is translated
    let mut ja = locos_units(&d);
    ja[0] = "ようこそ".into();
    let out = String::from_utf8(translate(&AndroidStrings, STRINGS, "ja", &ja)).unwrap();
    let block = out.split("<plurals name=\"files\">").nth(1).unwrap().split("</plurals>").next().unwrap();
    assert!(block.contains("quantity=\"other\"") && !block.contains("quantity=\"one\"") && !block.contains("quantity=\"few\""), "{block}");
    // nothing translated: the original bytes, whatever the target language (lead decision 11-04)
    assert_eq!(AndroidStrings.merge(&AndroidStrings.extract(STRINGS, &opts("pl")).unwrap()).unwrap(), STRINGS);
    // ... unless the caller asks for the LocOS behaviour
    let always = MergeOptions::new().with_stamp_target_always(true);
    let out = String::from_utf8(AndroidStrings.merge_with(&AndroidStrings.extract(STRINGS, &opts("ja")).unwrap(), &always).unwrap()).unwrap();
    assert!(!out.split("<plurals name=\"files\">").nth(1).unwrap().split("</plurals>").next().unwrap().contains("quantity=\"one\""));
}

#[test]
fn java_xml_properties() {
    let d = JavaXmlProperties.extract(MESSAGES, &opts("fr")).unwrap();
    assert_eq!(locos_units(&d), ["Hello {0}, it's {1,date,long}.", "Goodbye & thanks", "4.2.0"]);
    assert_eq!(JavaXmlProperties.merge(&d).unwrap(), MESSAGES);
    let out = by_text(&JavaXmlProperties, MESSAGES, &[("Hello {0}, it's {1,date,long}.", "Bonjour {0}, c'est le {1,date,long}.")]);
    assert!(out.contains("<entry key=\"greeting\">Bonjour {0}, c''est le {1,date,long}.</entry>"), "{out}");
    assert!(out.contains("<!DOCTYPE properties SYSTEM \"http://java.sun.com/dtd/properties.dtd\">"));
    assert!(out.contains("<comment>Hand-written fixture for Localization OS</comment>"));
    let out = by_text(&JavaXmlProperties, MESSAGES, &[("Goodbye & thanks", "Au revoir & merci d'avance")]);
    assert!(out.contains("Au revoir &amp; merci d'avance</entry>"), "{out}");
    assert_eq!(d.units.iter().map(|u| u.context.key.clone().unwrap()).collect::<Vec<_>>(), ["greeting", "farewell", "build.id"]);
}

#[test]
fn sniffing_by_root_element() {
    assert!(AndroidStrings.sniff(STRINGS) && !JavaXmlProperties.sniff(STRINGS));
    assert!(JavaXmlProperties.sniff(MESSAGES) && !AndroidStrings.sniff(MESSAGES));
    assert!(!AndroidStrings.sniff(b"<?xml version=\"1.0\"?><catalog><item>Hello</item></catalog>"));
    assert!(!AndroidStrings.sniff(b"{\"resources\": 1}"));
    assert!(AndroidStrings.extensions().is_empty() && JavaXmlProperties.extensions().is_empty());
    assert_eq!((AndroidStrings.id(), JavaXmlProperties.id()), ("android", "javaxml"));
}

fn refused(data: &[u8], needle: &str) {
    let e = AndroidStrings.extract(data, &opts("fr")).expect_err("expected a refusal").to_string().to_lowercase();
    assert!(e.contains(&needle.to_lowercase()), "{needle:?} not in {e:?}");
}

#[test]
fn hostile_inputs() {
    refused(b"<?xml version=\"1.0\"?><!DOCTYPE resources [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;\">]><resources><string name=\"x\">&b;</string></resources>", "");
    // a > inside a quoted system id must not hide the internal subset
    refused(b"<?xml version=\"1.0\"?><!DOCTYPE resources SYSTEM \"a>b\" [<!ENTITY a \"x\">]><resources><string name=\"x\" y=\"&a;\">hi</string></resources>", "");
    refused(b"<?xml version=\"1.0\"?><!DOCTYPE resources SYSTEM \"file:///etc/passwd\"><resources><string name=\"x\">&xxe;</string></resources>", "entity");
    let deep = [&b"<resources><string name='x'>"[..], &b"<b>".repeat(3000), b"t", &b"</b>".repeat(3000), b"</string></resources>"].concat();
    assert!(matches!(AndroidStrings.extract(&deep, &opts("fr")), Err(Error::Limit(_))));
    let many: String = (0..20).map(|i| format!("<string name=\"s{i}\">Text {i}</string>")).collect();
    let many = format!("<resources>{many}</resources>");
    let small = opts("fr").with_limits(Limits::default().with_max_units(5));
    assert!(matches!(AndroidStrings.extract(many.as_bytes(), &small), Err(Error::Limit(_))));
    // a stray ]]> in a translation for a CDATA value falls back to escaped text, still valid
    let out = by_text(&AndroidStrings, STRINGS, &[("Read the <a href=\"https://example.com/terms\">terms</a> first.", "a ]]> b")]);
    assert!(out.contains("a ]]&gt; b"), "{out}");
    AndroidStrings.extract(out.as_bytes(), &opts("fr")).unwrap();
    for bad in [&b"<resources><string name='a'>x</resources>"[..], b"<resources>", b"", b"<other/>", b"<resources/><resources/>", b"text<resources/>"] {
        assert!(AndroidStrings.extract(bad, &opts("fr")).is_err(), "{:?}", String::from_utf8_lossy(bad));
    }
}

#[test]
fn encodings_and_newlines_are_kept() {
    // UTF-16 with a BOM: units read, untouched merge byte for byte, a translated merge stays UTF-16
    let text = String::from_utf8(STRINGS.to_vec()).unwrap().replace("encoding=\"utf-8\"", "encoding=\"utf-16\"");
    let mut u16 = vec![0xFF, 0xFE];
    for c in text.encode_utf16() {
        u16.extend_from_slice(&c.to_le_bytes());
    }
    let d = AndroidStrings.extract(&u16, &opts("fr")).unwrap();
    assert_eq!(locos_units(&d), locos_units(&AndroidStrings.extract(STRINGS, &opts("fr")).unwrap()));
    assert_eq!(AndroidStrings.merge(&d).unwrap(), u16);
    let mut texts = locos_units(&d);
    texts[0] = "Bienvenue, %1$s !".into();
    let out = translate(&AndroidStrings, &u16, "", &texts);
    assert_eq!(&out[..2], &[0xFF, 0xFE]);
    // CRLF files keep CRLF outside the values
    let crlf = String::from_utf8(MESSAGES.to_vec()).unwrap().replace('\n', "\r\n");
    let d = JavaXmlProperties.extract(crlf.as_bytes(), &opts("fr")).unwrap();
    assert_eq!(JavaXmlProperties.merge(&d).unwrap(), crlf.as_bytes());
}

#[test]
fn sniff_survives_an_8_kb_cut_inside_a_multi_byte_character() {
    // Wild file: the root is early, a long run of 3-byte characters follows, and the 8192-byte head ends inside one.
    for pad in 0..4 {
        let mut doc = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- x -->\n<resources>{}\n", " ".repeat(pad)).into_bytes();
        while doc.len() < 9000 {
            doc.extend_from_slice("<string name=\"a\">\u{8bed}\u{8a00}</string>\n".as_bytes());
        }
        doc.extend_from_slice(b"</resources>");
        assert!(AndroidStrings.sniff(&doc), "pad {pad}");
    }
}
