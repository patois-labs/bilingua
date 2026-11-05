//! Port of the Localization OS suite `tests/verify_xml_multilingual_filter.py` (11 cases).

mod common;

use bilingua_model::*;
use bilingua_xml::{MlConfig, MlMode, XmlFilter};
use common::*;

const FIXTURES: [&str; 3] = ["catalog-attr.xml", "items-element.xml", "xpath-pi.xml"];

fn f() -> XmlFilter {
    XmlFilter::new()
}

fn xx(doc: &mut Document) {
    for i in 0..doc.units.len() {
        let t = format!("XX {}", show(&doc.units[i]));
        set(doc, i, &t);
    }
}

fn refused(r: Result<Document, Error>, needle: &str) {
    match r {
        Err(e) => assert!(e.to_string().contains(needle), "{needle} not in {e}"),
        Ok(d) => panic!("not refused ({needle}): {} units", d.units.len()),
    }
}

#[test]
fn identity() {
    for name in FIXTURES {
        let d = data(name);
        let insp = f().inspect(&d).unwrap();
        assert_eq!(insp.dialect.as_deref(), Some("multilingual"), "{name}");
        let doc = ex(&f(), &d);
        assert_eq!(s(&f().merge(&doc).unwrap()), s(&d), "{name}: untouched");
        assert_eq!(s(&echo(&f(), &d, None)), s(&d), "{name}: echo in place");
        assert_eq!(s(&f().merge(&ex_to(&f(), &d, "fr-FR")).unwrap()), s(&d), "{name}: untouched with target");
    }
}

const CATALOG_FR: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!-- Product strings, one entry per message, every language as a sibling -->
<catalog xml:lang=\"en\" version='2'>
  <entry id=\"welcome\">
    <text xml:lang=\"en\" id=\"t1\">Welcome to <b>Northwind</b>, {0}!</text>
    <text xml:lang=\"de\" id=\"t2\">Willkommen bei <b>Northwind</b>, {0}!</text>
    <text xml:lang=\"fr\">Bienvenue chez <b>Northwind</b>, {0} !</text>
  </entry>
  <entry id=\"save\">
    <text xml:lang=\"en\">Save &amp; close</text>
    <text xml:lang=\"de\">Speichern &amp; schliessen</text>
    <text xml:lang=\"fr\">Enregistrer &amp; fermer</text>
  </entry>
  <entry id=\"count\">
    <text xml:lang=\"en\">You have %1$s new messages</text>
  </entry>
  <entry id=\"skip\">
    <text xml:lang=\"en\">   </text>
    <text xml:lang=\"de\">   </text>
  </entry>
</catalog>
";

#[test]
fn attr_layout() {
    let d = data("catalog-attr.xml");
    let insp = f().inspect(&d).unwrap();
    let ml = insp.multilingual.unwrap();
    assert_eq!((ml.mode, ml.source.as_str(), ml.lang_attr.as_str()), (MlMode::Attr, "en", "xml:lang"));
    assert_eq!(ml.languages, vec!["en", "de", "fr"]);
    assert_eq!(texts(&f(), &d), vec!["Welcome to \u{27E6}g1\u{27E7}Northwind\u{27E6}/g1\u{27E7}, {0}!", "Save & close", "You have %1$s new messages"]);
    let tx = [(0, "Bienvenue chez \u{27E6}g1\u{27E7}Northwind\u{27E6}/g1\u{27E7}, {0} !"), (1, "Enregistrer & fermer")];
    assert_eq!(s(&merge_with(&f(), &d, Some("fr-FR"), &tx)), CATALOG_FR);
    // unit 2 translated too: a missing fr is created after the group, indentation copied
    let all = [tx[0], tx[1], (2, "Vous avez %1$s nouveaux messages")];
    let out = s(&merge_with(&f(), &d, Some("fr-FR"), &all));
    assert!(out.contains("    <text xml:lang=\"en\">You have %1$s new messages</text>\n    <text xml:lang=\"fr\">Vous avez %1$s nouveaux messages</text>\n  </entry>"), "{out}");
    // a new language: created in every group, after the last sibling; id / xml:id dropped
    let mut doc = ex_to(&f(), &d, "it");
    xx(&mut doc);
    let out = s(&f().merge(&doc).unwrap());
    assert!(
        out.contains("<text xml:lang=\"de\" id=\"t2\">Willkommen bei <b>Northwind</b>, {0}!</text>\n    <text xml:lang=\"it\">XX Welcome to <b>Northwind</b>, {0}!</text>"),
        "{out}"
    );
    assert!(out.contains("<text xml:lang=\"fr\"/>\n    <text xml:lang=\"it\">XX Save &amp; close</text>"), "{out}");
    assert_eq!(out.matches("xml:lang=\"it\"").count(), 3);
    assert!(out.contains("id=\"t1\"") && out.matches("id=\"t").count() == 2);
    well_formed(&f(), out.as_bytes());
}

const ITEMS_FR: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<items>
\t<item key=\"menu.file\">
\t\t<en>File</en>
\t\t<fr>Fichier (nouveau)</fr>
\t</item>
\t<item key=\"menu.open\">
\t\t<en>Open <![CDATA[recent]]> files</en>
\t\t<fr>Ouvrir les fichiers r\u{e9}cents</fr>
\t</item>
\t<item key=\"menu.quit\">
\t\t<en>Quit {{app}}</en>
\t\t<fr>Quitter {{app}}</fr>
\t\t<!-- fr missing on purpose -->
\t</item>
</items>
";

#[test]
fn element_layout() {
    let d = data("items-element.xml");
    assert_eq!(f().inspect(&d).unwrap().multilingual.unwrap().mode, MlMode::Element);
    assert_eq!(texts(&f(), &d), vec!["File", "Open recent files", "Quit {{app}}"]);
    let out = merge_with(&f(), &d, Some("fr"), &[(0, "Fichier (nouveau)"), (2, "Quitter {{app}}")]);
    assert_eq!(s(&out), ITEMS_FR);
    // namespace-prefixed language elements keep their prefix
    let pre = br#"<r xmlns:l="urn:l"><g><l:en>One</l:en><l:de>Eins</l:de></g><g><l:en>Two</l:en><l:de>Zwei</l:de></g></r>"#;
    let out = merge_with(&f(), pre, Some("fr"), &[(1, "Deux")]);
    assert_eq!(s(&out), r#"<r xmlns:l="urn:l"><g><l:en>One</l:en><l:de>Eins</l:de></g><g><l:en>Two</l:en><l:de>Zwei</l:de><l:fr>Deux</l:fr></g></r>"#);
}

#[test]
fn xpath_pi() {
    let d = data("xpath-pi.xml");
    let ml = f().inspect(&d).unwrap().multilingual.unwrap();
    assert_eq!((ml.mode, ml.source.as_str()), (MlMode::Xpath, "en_US"));
    let out = s(&merge_with(&f(), &d, Some("fr-FR"), &[(0, "Bonjour"), (1, "Au revoir")]));
    assert!(out.contains("<msg name=\"greet\"><v loc=\"en_US\">Hello</v><v loc=\"fr_FR\">Bonjour</v><v loc=\"ja_JP\">\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}</v></msg>"), "{out}");
    assert!(out.contains("<msg name=\"bye\"><v loc=\"en_US\">Goodbye</v><v loc=\"fr_FR\">Au revoir</v></msg>"), "{out}");
    let out = s(&merge_with(&f(), &d, Some("ja-JP"), &[(0, "Konnichiwa")]));
    assert!(out.contains("<v loc=\"ja_JP\">Konnichiwa</v></msg>") && !out.contains("\u{3053}\u{3093}"), "{out}");
    assert_eq!(out.matches("<v ").count(), 3, "nothing created for the untranslated unit");
}

#[test]
fn in_place_build() {
    let d = data("items-element.xml");
    let out = s(&merge_with(&f(), &d, None, &[(0, "Fichier")]));
    assert!(out.contains("<en>Fichier</en>\n\t\t<fr>Fichier</fr>"), "{out}");
    assert_eq!(out.replace("<en>Fichier</en>", "<en>File</en>"), s(&d));
}

#[test]
fn token_damage() {
    let d = data("catalog-attr.xml");
    let doc0 = ex(&f(), &d);
    let tags: Vec<Inline> = doc0.units[0].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let (o, c) = (tags[0].clone(), tags[1].clone());
    let cases: Vec<Vec<Inline>> = vec![
        vec![c.clone(), Inline::text("Bienvenue "), o.clone(), Inline::text("chez")],
        vec![Inline::text("Bienvenue "), o.clone(), o.clone(), Inline::text(" "), Inline::tag("9", TagKind::Open, "x"), Inline::tag("4", TagKind::Standalone, "x")],
        vec![Inline::text("<b>raw</b> & ]]> stuff")],
    ];
    for bad in cases {
        let mut doc = ex_to(&f(), &d, "fr");
        set_raw(&mut doc, 0, bad);
        let out = f().merge(&doc).unwrap();
        well_formed(&f(), &out);
        let st = s(&out);
        assert_eq!(st.matches("<text xml:lang=\"fr\">").count(), 1, "{st}");
    }
}

#[test]
fn config() {
    let d = data("catalog-attr.xml");
    // explicit source: German becomes the source language
    let g = XmlFilter::new().multilingual(MlConfig::from_json(r#"{"source": "de"}"#).unwrap());
    assert_eq!(texts(&g, &d), vec!["Willkommen bei \u{27E6}g1\u{27E7}Northwind\u{27E6}/g1\u{27E7}, {0}!", "Speichern & schliessen"]);
    // JSON, element mode on an English-only file (explicit: no detection threshold)
    let only = b"<r><i><en>Yes</en></i></r>";
    let g = XmlFilter::new().multilingual(MlConfig::from_json(r#"{"mode": "element", "source": "en"}"#).unwrap());
    assert_eq!(texts(&g, only), vec!["Yes"]);
    assert_eq!(merge_with(&g, only, Some("fr"), &[(0, "Oui")]), b"<r><i><en>Yes</en><fr>Oui</fr></i></r>");
    // create = no
    let g = XmlFilter::new().multilingual(MlConfig::from_json(r#"{"mode": "element", "source": "en", "create": "no"}"#).unwrap());
    assert_eq!(merge_with(&g, only, Some("fr"), &[(0, "Oui")]), only);
    // PI with enabled = no: back to plain ITS (every text is a unit)
    let off = s(&d).replacen("<catalog", "<?loc-multilingual enabled=\"no\"?>\n<catalog", 1);
    let insp = f().inspect(off.as_bytes()).unwrap();
    assert!(insp.multilingual.is_none());
    assert_eq!(ex(&f(), off.as_bytes()).units.len(), 9);
    // an explicit config that finds nothing is a plain refusal
    let g = XmlFilter::new().multilingual(MlConfig::from_json(r#"{"source": "pt"}"#).unwrap());
    refused(g.extract(&d, &opts(None)), "no source-language");
    let long = format!(r#"{{"source": "{}"}}"#, "x".repeat(30_000));
    for (cfg, needle) in [
        (r#"{"colour": "red"}"#.to_string(), "unknown key"),
        (r#"{"mode": "rows"}"#.into(), "mode must be"),
        (r#"{"source": "en'] | //*"}"#.into(), "not a language code"),
        (r#"{"lang_attr": "a b"}"#.into(), "not an attribute name"),
        (r#"{"create": "maybe"}"#.into(), "create must be yes or no"),
        (r#"{"mode": "xpath", "source": "en", "source_xpath": "//v"}"#.into(), "{lang} placeholder"),
        (r#"{"mode": "xpath", "source_xpath": "//v[@l='{lang}']", "target_xpath": "../v[@l='{lang}']"}"#.into(), "needs a source"),
        (r#"{"source": "en", "source_xpath": "//v[@l='{lang}']", "target_xpath": "//v[@l='{lang}']"}"#.into(), "pointer must be relative"),
        (r#"{"source": "en", "source_xpath": "//v[@l='{lang}']/following::v", "target_xpath": "../v[@l='{lang}']"}"#.into(), "following and preceding"),
        (r#"{"source": "en", "source_xpath": "//v[.//w[@l='{lang}']]", "target_xpath": "../v[@l='{lang}']"}"#.into(), "inside a predicate"),
        ("{not json".into(), "not valid JSON"),
        ("[1]".into(), "must be an object"),
        (long, "larger than 20 KB"),
        (r#"{"source": 5}"#.into(), "must be text"),
    ] {
        match MlConfig::from_json(&cfg) {
            Err(e) => assert!(e.to_string().contains(needle), "{needle} not in {e}"),
            Ok(_) => panic!("accepted: {needle}"),
        }
    }
    // a bad PI is refused the same way at parse
    refused(f().extract(br#"<?loc-multilingual mode="rows"?><r/>"#, &opts(None)), "mode must be");
}

#[test]
fn detection_negatives() {
    let kind = |d: &[u8]| f().inspect(d).unwrap().multilingual.is_some();
    assert!(!kind(br#"<r><g><t xml:lang="en">Hi</t><t xml:lang="de">Hallo</t></g></r>"#), "a single group");
    assert!(!kind(b"<log><msg><to>Ann</to><id>Bob</id><body>Hello</body></msg><msg><to>Cy</to><id>Di</id><body>Bye</body></msg></log>"), "records");
    assert!(!kind(br#"<doc xml:lang="en"><p>One</p><p>Two</p></doc>"#), "plain XML");
    // two groups, two languages each: detected; the source is the language present in most groups
    let two = br#"<r><g><t lang="de">Hallo</t><t lang="en">Hi</t></g><g><t lang="en">Bye</t></g><g><t lang="en">Yes</t><t lang="de">Ja</t></g></r>"#;
    let ml = f().inspect(two).unwrap().multilingual.unwrap();
    assert_eq!((ml.source.as_str(), ml.lang_attr.as_str()), ("en", "lang"));
}

#[test]
fn encodings() {
    let d = data("items-element.xml");
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(&d);
    assert_eq!(f().merge(&ex(&f(), &bom)).unwrap(), bom);
    let out = merge_with(&f(), &bom, Some("fr"), &[(2, "Quitter")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]) && s(&out).contains("<fr>Quitter</fr>"));
    let crlf: Vec<u8> = s(&d).replace('\n', "\r\n").into_bytes();
    let out = merge_with(&f(), &crlf, Some("fr"), &[(2, "Quitter")]);
    assert!(s(&out).contains("<en>Quit {{app}}</en>\r\n\t\t<fr>Quitter</fr>\r\n"), "{}", s(&out));
    assert_eq!(s(&out).replace("\r\n\t\t<fr>Quitter</fr>", ""), s(&crlf));
    let u16s = s(&d).replace("encoding=\"utf-8\"", "encoding=\"UTF-16\"");
    let mut u16b = vec![0xFF, 0xFE];
    for c in u16s.encode_utf16() {
        u16b.extend_from_slice(&c.to_le_bytes());
    }
    let out = merge_with(&f(), &u16b, Some("fr"), &[(2, "Quitter")]);
    let (dec, _) = bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(&out[2..]);
    assert!(out.starts_with(&[0xFF, 0xFE]) && dec.contains("<fr>Quitter</fr>"));
}

#[test]
fn hostile() {
    let bomb = br#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY a "aaaa"><!ENTITY b "&a;&a;&a;">]><r><g><t xml:lang="en">&b;</t><t xml:lang="de">x</t></g><g><t xml:lang="en">y</t><t xml:lang="de">z</t></g></r>"#;
    refused(f().extract(bomb, &opts(None)), "DTD");
    let xxe = br#"<?xml version="1.0"?><!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/passwd">]><r/>"#;
    refused(f().extract(xxe, &opts(None)), "DTD");
    // an unvalidated language code never reaches the XPath or the output
    let d = data("xpath-pi.xml");
    let mut doc = ex_to(&f(), &d, "fr'] | //*[@a='");
    set(&mut doc, 0, "x");
    assert!(f().merge(&doc).is_err() || { !s(&f().merge(&doc).unwrap()).contains("//*") });
    // caps
    let lim = |n| ExtractOptions::new().with_limits(Limits::new().with_max_units(n));
    assert!(matches!(f().extract(&data("catalog-attr.xml"), &lim(2)), Err(Error::Limit(_))));
    let many = format!("<r>{}</r>", r#"<g><t xml:lang="en">a</t><t xml:lang="de">b</t></g>"#.repeat(30));
    match f().extract(many.as_bytes(), &lim(20)) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // xpath mode: sources x siblings bounded before the per-source target rule runs
    let wide = format!("<?loc-multilingual source=\"en\" source-xpath=\"/r/v[@l='{{lang}}']\" target-xpath=\"../v[@l='{{lang}}']\"?><r>{}</r>", r#"<v l="en">a</v>"#.repeat(70));
    match f().extract(wide.as_bytes(), &lim(20)) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("too many nodes"), "{}", e.msg),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
}

#[test]
fn scale_linear_build() {
    // 20 000 created targets build in one linear splice, in unit order, every target present
    let n = 20000;
    let mut data = String::from("<r>\n");
    for i in 0..n {
        data.push_str(&format!("  <g><t xml:lang=\"en\">Text {i}</t><t xml:lang=\"de\">T {i}</t></g>\n"));
    }
    data.push_str("</r>\n");
    let lim = ExtractOptions::new().with_target_lang("fr").with_limits(Limits::new().with_max_units(100_000));
    let filt = f();
    let mut doc = filt.extract(data.as_bytes(), &lim).unwrap();
    assert_eq!(doc.units.len(), n);
    for i in 0..n {
        set_raw(&mut doc, i, vec![Inline::text(format!("FText {i}"))]);
    }
    let t0 = std::time::Instant::now();
    let out = filt.merge(&doc).unwrap();
    let took = t0.elapsed().as_secs_f64();
    assert!(took < 20.0, "build took {took:.1} s");
    let st = s(&out);
    let frs: Vec<&str> = st.split("<t xml:lang=\"fr\">").skip(1).map(|p| p.split('<').next().unwrap()).collect();
    assert_eq!(frs.len(), n);
    assert!(frs.iter().enumerate().all(|(i, t)| *t == format!("FText {i}")));
}
