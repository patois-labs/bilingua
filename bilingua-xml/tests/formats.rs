//! The oracle corpus: untouched and pseudo-translated round trips, eleven languages, legacy encodings, target-locale
//! merges, ITS behaviour on the corpus documents, and the generic-XML cases of the LocOS resource suite.

mod common;

use bilingua_model::*;
use bilingua_xml::{Dialect, XmlFilter};
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/xml");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("corpus folder")
        .filter_map(|e| e.ok())
        .map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap()))
        .collect();
    v.sort();
    v
}

fn file(name: &str) -> Vec<u8> {
    corpus().into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}")).1
}

fn f() -> XmlFilter {
    XmlFilter::new()
}

const LANGS: [(&str, &str); 11] = [
    ("en", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es", "Bienvenido a Panader\u{ed}a Lumen. El pan fresco est\u{e1} listo a las seis de cada ma\u{f1}ana."),
    ("fr", "Bienvenue \u{e0} la Boulangerie Lumen. Le pain frais est pr\u{ea}t \u{e0} six heures chaque matin."),
    ("it", "Benvenuto al Forno Lumen. Il pane fresco \u{e8} pronto ogni mattina alle sei."),
    ("de", "Willkommen in der B\u{e4}ckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt", "Bem-vindo \u{e0} Padaria Lumen. O p\u{e3}o fresco fica pronto \u{e0}s seis horas todas as manh\u{e3}s."),
    ("ja", "\u{30eb}\u{30fc}\u{30e1}\u{30f3}\u{30fb}\u{30d9}\u{30fc}\u{30ab}\u{30ea}\u{30fc}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}\u{3002}"),
    ("zh", "\u{6b22}\u{8fce}\u{5149}\u{4e34}\u{6d41}\u{660e}\u{9762}\u{5305}\u{5e97}\u{3002}\u{65b0}\u{9c9c}\u{9762}\u{5305}\u{6bcf}\u{5929}\u{65e9}\u{4e0a}\u{516d}\u{70b9}\u{51fa}\u{7089}\u{3002}"),
    ("ko", "\u{b8e8}\u{baa8} \u{bca0}\u{c774}\u{ce74}\u{b9ac}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}."),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{645}\u{62e}\u{628}\u{632} \u{644}\u{648}\u{645}\u{646}."),
];

fn pseudo(doc: &mut Document) {
    for u in &mut doc.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
}

fn tags(d: &Document) -> usize {
    d.units.iter().map(|u| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

#[test]
fn every_corpus_file_round_trips() {
    let files = corpus();
    assert!(files.len() >= 15);
    for (name, bytes) in files {
        let flt = f();
        let d = flt.extract(&bytes, &opts(Some("de-DE"))).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!d.units.is_empty(), "{name}");
        assert_eq!(flt.merge(&d).unwrap(), bytes, "{name}: untouched merge");
        assert_eq!(echo(&flt, &bytes, None), bytes, "{name}: echo merge");
        let multilingual = flt.inspect(&bytes).unwrap().multilingual.is_some();
        for target in [None, Some("de-DE"), Some("ar"), Some("ja")] {
            let mut e = flt.extract(&bytes, &opts(target)).unwrap();
            pseudo(&mut e);
            let out = flt.merge(&e).unwrap_or_else(|er| panic!("{name} {target:?}: {er}"));
            let back = flt.extract(&out, &opts(None)).unwrap_or_else(|er| panic!("{name} {target:?} does not read back: {er}"));
            assert_eq!(back.units.len(), e.units.len(), "{name} {target:?}: unit count");
            if !multilingual || target.is_none() {
                assert_eq!(tags(&back), tags(&d), "{name} {target:?}: tag count");
                for (b, u) in back.units.iter().zip(&d.units) {
                    assert_eq!(show(b), format!("[{}]", show(u)).replace("[\u{27E6}", "[\u{27E6}"), "{name} {target:?}");
                }
            }
        }
    }
}

#[test]
fn eleven_languages_translate_and_read_back() {
    let bytes = file("bakery_languages.xml");
    let flt = f();
    let mut d = flt.extract(&bytes, &opts(None)).unwrap();
    assert_eq!(d.units.len(), 11);
    let orig: Vec<String> = d.units.iter().map(show).collect();
    // every section becomes the next language's text: all eleven scripts go through the merge
    for i in 0..11 {
        set_raw(&mut d, i, vec![Inline::text(orig[(i + 1) % 11].clone())]);
    }
    let out = flt.merge(&d).unwrap();
    let back = flt.extract(&out, &opts(None)).unwrap();
    for i in 0..11 {
        assert_eq!(show(&back.units[i]), orig[(i + 1) % 11], "language {}", LANGS[i].0);
    }
    // unchanged bytes outside the sentences
    let mut check = s(&out);
    for i in 0..11 {
        check = check.replacen(&orig[(i + 1) % 11], &format!("@@{i}@@"), 1);
    }
    #[allow(clippy::needless_range_loop)]
    for i in 0..11 {
        check = check.replacen(&format!("@@{i}@@"), &orig[i], 1);
    }
    assert_eq!(check, s(&bytes));
}

#[test]
fn target_locales_are_written_for_all_eleven_languages() {
    let dita = XmlFilter::with_id("dita", &["dita"]).dialect(Dialect::dita());
    let bytes = file("bakery_topic.xml");
    for (code, _) in LANGS {
        let mut d = dita.extract(&bytes, &opts(Some(code))).unwrap();
        pseudo(&mut d);
        let out = s(&dita.merge(&d).unwrap());
        assert!(out.contains(&format!(" xml:lang=\"{code}\">")), "{code}: {out}");
        assert!(!out.contains("xml:lang=\"en-US\""));
    }
    // region and script tags are written in BCP 47 case; underscores become hyphens
    for (given, want) in [("zh_hans_cn", "zh-Hans-CN"), ("PT-br", "pt-BR"), ("ar-EG", "ar-EG")] {
        let mut d = dita.extract(&bytes, &opts(Some(given))).unwrap();
        pseudo(&mut d);
        assert!(s(&dita.merge(&d).unwrap()).contains(&format!("xml:lang=\"{want}\"")), "{given}");
    }
    // an unusable locale leaves the language alone
    for bad in ["", "x", "fr'; <b>", "en US"] {
        let mut d = dita.extract(&bytes, &opts(Some(bad))).unwrap();
        pseudo(&mut d);
        assert!(s(&dita.merge(&d).unwrap()).contains("xml:lang=\"en-US\""), "{bad:?}");
    }
    // no direction attribute exists in generic XML: none is written for right-to-left targets
    let mut d = dita.extract(&bytes, &opts(Some("ar"))).unwrap();
    pseudo(&mut d);
    assert!(!s(&dita.merge(&d).unwrap()).contains(" dir="));
}

#[test]
fn legacy_encodings_are_read_and_written_back() {
    for (name, label, text) in [
        ("bakery_windows1252.xml", "windows-1252", "Cr\u{e8}me br\u{fb}l\u{e9}e \u{20ac}"),
        ("bakery_shiftjis.xml", "Shift_JIS", "\u{30d1}\u{30f3}\u{3092}\u{3069}\u{3046}\u{305e}"),
        ("bakery_gbk.xml", "GBK", "\u{6b22}\u{8fce}\u{5149}\u{4e34}"),
        ("bakery_euckr.xml", "EUC-KR", "\u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}"),
        ("bakery_win1256.xml", "windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627}"),
    ] {
        let bytes = file(name);
        let flt = f();
        let d = flt.extract(&bytes, &opts(None)).unwrap();
        assert_eq!(flt.merge(&d).unwrap(), bytes, "{name}: untouched");
        // text that fits the code page stays in it, declaration and all
        let out = merge_with(&flt, &bytes, None, &[(0, text)]);
        let enc = bilingua_model::encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        let (dec, _, bad) = enc.decode(&out);
        assert!(!bad && dec.contains(text) && dec.contains(&format!("encoding=\"{label}\"")), "{name}: {dec}");
        assert_eq!(flt.extract(&out, &opts(None)).unwrap().units.len(), d.units.len());
        // text outside the code page is written as UTF-8 with a UTF-8 declaration
        let out = merge_with(&flt, &bytes, None, &[(0, "\u{1f600} \u{2603}")]);
        if label == "GBK" {
            // GB18030 covers all of Unicode, so a GBK file keeps GBK
            assert!(String::from_utf8_lossy(&out).contains("encoding=\"GBK\""));
            continue;
        }
        let out = s(&out);
        assert!(out.contains("encoding=\"UTF-8\"") && out.contains("\u{1f600} \u{2603}"), "{name}: {out}");
    }
    // UTF-16 with a BOM and CRLF, UTF-8 with a BOM and CRLF
    for name in ["bakery_utf16le.xml", "bakery_utf8_bom_crlf.xml"] {
        let bytes = file(name);
        let flt = f();
        assert_eq!(flt.merge(&flt.extract(&bytes, &opts(None)).unwrap()).unwrap(), bytes, "{name}");
        let out = merge_with(&flt, &bytes, None, &[(1, "Changed \u{65e5}")]);
        assert_eq!(out[..2.min(out.len())], bytes[..2.min(bytes.len())], "{name}: byte order mark kept");
        assert_eq!(flt.extract(&out, &opts(None)).unwrap().units.len(), 2);
    }
}

#[test]
fn its_behaviour_on_the_menu() {
    let bytes = file("bakery_menu.xml");
    let doc = ex(&f(), &bytes);
    let t: Vec<String> = doc.units.iter().map(show).collect();
    assert_eq!(t[0], "Sourdough loaf");
    assert_eq!(doc.units[0].context.style.as_deref(), Some("attr:label"));
    // the pointer note comes from <hint>
    assert_eq!(doc.units[1].notes[0].text, "Product name, do not shorten.");
    assert_eq!(doc.units[3].notes[0].text, "Keep under 90 characters.");
    assert!(t[3].contains("\u{27E6}g1\u{27E7}crusty\u{27E6}/g1\u{27E7}") && t[3].contains("\u{27E6}x3\u{27E7}"), "{}", t[3]);
    // prices, sku and the its:translate="no" element are never units
    assert!(!t.iter().any(|x| x.contains("4.50") || x.contains("B-100") || x.contains("Autumn")));
    // preserved white space in <recipe>
    assert!(t.iter().any(|x| x.contains("500 g flour\n    with 300 ml water.\n      Rest overnight.")), "{t:?}");
    // entities decode to text and come back escaped
    assert!(t.iter().any(|x| x == "Butter & flour, nothing else. Serves 1 < 2 people."));
}

#[test]
fn edge_document() {
    let bytes = file("edge.xml");
    assert_eq!(
        texts(&f(), &bytes),
        vec![
            "Anna & Ben",
            "Fish & chips \u{263a} for \u{27E6}x1\u{27E7} at <noon>",
            "one \u{27E6}x1\u{27E7} two \u{27E6}x2\u{27E7} three",
            "a\u{a0}b",
            "1 < 2",
            "deep text",
            "namespaced text",
            "content",
            "text",
            "inner",
            "after",
        ]
    );
    // the undefined entity, the comment and the processing instruction come back exactly
    let out = s(&merge_with(&f(), &bytes, None, &[(1, "Poisson \u{27E6}x1\u{27E7} \u{e0} <midi>"), (2, "un \u{27E6}x2\u{27E7} deux \u{27E6}x1\u{27E7} trois")]));
    assert!(out.contains("<body>Poisson &company; \u{e0} &lt;midi&gt;</body>"), "{out}");
    assert!(out.contains("<mixed>un <?pi data?> deux <!-- gone --> trois</mixed>"), "{out}");
    assert!(out.contains("<attrs title=\"Don't &quot;panic&quot;\" other='single &amp; quoted'>content</attrs>"));
}

// ---- the generic cases of verify_xml_resource_filters.py (unknown root falls back to ITS; hostile files)

#[test]
fn unknown_root_falls_back_to_its() {
    let other = br#"<?xml version="1.0"?><catalog><item>Hello</item></catalog>"#;
    assert_eq!(texts(&f(), other), vec!["Hello"]);
    assert_eq!(merge_with(&f(), other, None, &[(0, "Hallo")]), String::from_utf8_lossy(other).replace("Hello", "Hallo").into_bytes());
    assert_eq!(f().merge(&ex(&f(), other)).unwrap(), other);
}

#[test]
fn resource_suite_hostile_cases() {
    let xml = f();
    let lol = br#"<?xml version="1.0"?><!DOCTYPE resources [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">]><resources><string name="x">&b;</string></resources>"#;
    // (a plain .xml keeps the DTD and never expands it; the entity is a code)
    let d = xml.extract(lol, &opts(None)).unwrap();
    assert_eq!(xml.merge(&d).unwrap(), lol);
    // a ">" inside a quoted system id must not hide the internal subset
    let hidden = br#"<?xml version="1.0"?><!DOCTYPE resources SYSTEM "a>b" [<!ENTITY a "x">]><resources><string name="x" y="&a;">hi</string></resources>"#;
    assert!(xml.extract(hidden, &opts(None)).is_ok());
    let xxe = br#"<?xml version="1.0"?><!DOCTYPE resources SYSTEM "file:///etc/passwd"><resources><string name="x">a &xxe; b</string></resources>"#;
    match xml.extract(xxe, &opts(None)) {
        Err(Error::Parse { .. }) | Ok(_) => {}
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    // the external DTD is never loaded: the entity stays a point tag
    let d = xml.extract(xxe, &opts(None)).unwrap();
    assert_eq!(show(&d.units[0]), "a \u{27E6}x1\u{27E7} b", "an undefined entity is a tag, never expanded");
    let deep = format!("<resources><string name='x'>{}t{}</string></resources>", "<b>".repeat(3000), "</b>".repeat(3000));
    assert!(xml.extract(deep.as_bytes(), &opts(None)).is_err());
    let lim = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let many = format!("<resources>{}</resources>", (0..20).map(|i| format!("<string name=\"s{i}\">Text {i}</string>")).collect::<String>());
    assert!(matches!(xml.extract(many.as_bytes(), &lim), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    // a stray ]]> in a translation for a CDATA value falls back to split CDATA, still valid
    let cd = b"<r><c><![CDATA[Read the guide]]></c></r>";
    let out = s(&merge_with(&xml, cd, None, &[(0, "a ]]> b")]));
    assert!(out.contains("a ]]]]><![CDATA[> b"), "{out}");
    xml.extract(out.as_bytes(), &opts(None)).unwrap();
}
