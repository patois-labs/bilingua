//! Round trips in eleven languages with the target language stamped (xml:lang for DITA and DocBook 5, lang for
//! DocBook 4.x), and the legacy encodings LocOS reads for XML (a declared code page, UTF-16).

mod common;

use bilingua_model::encoding_rs::Encoding;
use bilingua_model::*;
use common::*;

const LANGS: [(&str, &str); 11] = [
    ("en-US", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es-ES", "Bienvenido a Panader\u{ed}a Lumen. El pan fresco est\u{e1} listo a las seis de cada ma\u{f1}ana."),
    ("fr-FR", "Bienvenue \u{e0} la Boulangerie Lumen. Le pain frais est pr\u{ea}t \u{e0} six heures chaque matin."),
    ("it-IT", "Benvenuto al Forno Lumen. Il pane fresco \u{e8} pronto ogni mattina alle sei."),
    ("de-DE", "Willkommen in der B\u{e4}ckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl-NL", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt-BR", "Bem-vindo \u{e0} Padaria Lumen. O p\u{e3}o fresco fica pronto \u{e0}s seis horas todas as manh\u{e3}s."),
    ("ja-JP", "\u{30eb}\u{30fc}\u{30e1}\u{30f3}\u{30fb}\u{30d9}\u{30fc}\u{30ab}\u{30ea}\u{30fc}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}\u{3002}"),
    ("zh-CN", "\u{6b22}\u{8fce}\u{5149}\u{4e34}\u{6d41}\u{660e}\u{9762}\u{5305}\u{5e97}\u{3002}"),
    ("ko-KR", "\u{b8e8}\u{baa8} \u{bca0}\u{c774}\u{ce74}\u{b9ac}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}."),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{645}\u{62e}\u{628}\u{632} \u{644}\u{648}\u{645}\u{646}."),
];

const TOPIC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE task PUBLIC \"-//OASIS//DTD DITA Task//EN\" \"task.dtd\">\n<task id=\"t\" xml:lang=\"en-US\">\n  <title>Bake the <ph>bread</ph></title>\n  <taskbody><steps><step><cmd>Press <uicontrol>Start</uicontrol> and wait.</cmd></step></steps></taskbody>\n</task>\n";
const DB45: &str = "<?xml version=\"1.0\"?>\n<!DOCTYPE article PUBLIC \"-//OASIS//DTD DocBook XML V4.5//EN\" \"docbookx.dtd\">\n<article lang=\"en\"><title>Bake</title><para>Press <guibutton>Start</guibutton>.</para></article>\n";
const DB5: &str = "<?xml version=\"1.0\"?>\n<article xmlns=\"http://docbook.org/ns/docbook\" version=\"5.0\"><title>Bake</title><para>Press <guibutton>Start</guibutton>.</para></article>\n";

/// Every unit gets the sentence in front of its source (codes and all).
fn translate(f: &dyn Filter, src: &[u8], lang: &str, sentence: &str) -> Vec<u8> {
    let mut d = ex_to(f, src, lang);
    for (i, u) in d.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("{sentence} {i} "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    f.merge(&d).unwrap()
}

#[test]
fn eleven_languages_round_trip_with_the_language_stamped() {
    let cases: [(Box<dyn Filter>, &str, &str); 3] = [(Box::new(Dita::new()), TOPIC, "xml:lang"), (Box::new(DocBook::new()), DB45, "lang"), (Box::new(DocBook::new()), DB5, "xml:lang")];
    for (f, src, attr) in cases {
        let f = f.as_ref();
        let n = ex(f, src.as_bytes()).units.len();
        let tags = tag_count(&ex(f, src.as_bytes()));
        for (lang, sentence) in LANGS {
            let out = s(&translate(f, src.as_bytes(), lang, sentence));
            assert!(out.contains(&format!(" {attr}=\"{lang}\"")), "{lang}: {out}");
            assert_eq!(out.matches(&format!("{attr}=")).count(), 1, "{lang}: one language attribute: {out}");
            assert!(!out.contains(" dir="), "XML vocabularies here have no direction attribute to write");
            let back = ex(f, out.as_bytes());
            assert_eq!(back.units.len(), n, "{lang}");
            assert_eq!(tag_count(&back), tags, "{lang}");
            assert!(show(&back.units[0]).starts_with(&format!("{sentence} 0 ")), "{lang}");
            // the rest of the file is untouched: the DOCTYPE and the declaration
            assert_eq!(out.lines().next(), src.lines().next());
        }
    }
}

#[test]
fn the_corpus_eleven_language_topic_reads_every_script() {
    let bytes = corpus_file("lumen_languages.dita");
    let f = Dita::new();
    let d = ex(&f, &bytes);
    let sentences: Vec<String> = d.units.iter().map(show).filter(|t| t.len() > 12).collect();
    assert_eq!(sentences.len(), 12, "the topic title and eleven sentences: {sentences:?}");
    // every script is there: accented Latin, kana, Han, Hangul, Arabic
    let has = |lo: char, hi: char| sentences.iter().any(|s| s.chars().any(|c| (lo..=hi).contains(&c)));
    for (what, lo, hi) in [("accented Latin", '\u{e0}', '\u{ff}'), ("kana", '\u{3040}', '\u{30ff}'), ("Han", '\u{4e00}', '\u{9fff}'), ("Hangul", '\u{ac00}', '\u{d7a3}'), ("Arabic", '\u{600}', '\u{6ff}')] {
        assert!(has(lo, hi), "{what} not read");
    }
    // each sentence becomes the next language's; the section languages stay, the root language changes
    let mut e = ex_to(&f, &bytes, "ar");
    let all: Vec<String> = e.units.iter().map(show).collect();
    for i in 0..all.len() {
        e.units[i].segments[0].target = vec![Inline::text(all[(i + 1) % all.len()].clone())];
    }
    let out = s(&f.merge(&e).unwrap());
    assert!(out.contains("<topic id=\"languages\" xml:lang=\"ar\">") && out.contains("xml:lang=\"ja\""), "{out}");
    let back: Vec<String> = ex(&f, out.as_bytes()).units.iter().map(show).collect();
    assert_eq!(back, (0..all.len()).map(|i| all[(i + 1) % all.len()].clone()).collect::<Vec<_>>());
}

#[test]
fn legacy_encodings_are_read_and_kept() {
    for (label, text, fits) in [
        ("windows-1252", "Cr\u{e8}me br\u{fb}l\u{e9}e \u{20ac}", "na\u{ef}ve"),
        ("ISO-8859-1", "Cr\u{e8}me br\u{fb}l\u{e9}e", "na\u{ef}ve"),
        ("Shift_JIS", "\u{30d1}\u{30f3}\u{3092}\u{3069}\u{3046}\u{305e}", "\u{65e5}\u{672c}"),
        ("EUC-JP", "\u{30d1}\u{30f3}\u{3092}\u{3069}\u{3046}\u{305e}", "\u{65e5}\u{672c}"),
        ("GBK", "\u{6b22}\u{8fce}\u{5149}\u{4e34}", "\u{9762}\u{5305}"),
        ("Big5", "\u{6b61}\u{8fce}\u{5149}\u{81e8}", "\u{9eb5}\u{5305}"),
        ("EUC-KR", "\u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}", "\u{be75}"),
        ("windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627}", "\u{62e}\u{628}\u{632}"),
        ("windows-1251", "\u{414}\u{43e}\u{431}\u{440}\u{43e}", "\u{445}\u{43b}\u{435}\u{431}"),
    ] {
        let enc = Encoding::for_label(label.as_bytes()).unwrap();
        for (f, src) in [
            (&Dita::new() as &dyn Filter, format!("<?xml version=\"1.0\" encoding=\"{label}\"?>\n<!DOCTYPE topic PUBLIC \"-//OASIS//DTD DITA Topic//EN\" \"topic.dtd\">\n<topic id=\"t\"><title>{text}</title><body><p>two</p></body></topic>\n")),
            (&DocBook::new(), format!("<?xml version=\"1.0\" encoding=\"{label}\"?>\n<article lang=\"en\"><title>{text}</title><para>two</para></article>\n")),
        ] {
            let (bytes, _, bad) = enc.encode(&src);
            assert!(!bad, "{label}");
            let bytes = bytes.into_owned();
            let d = ex(f, &bytes);
            assert_eq!(show(&d.units[0]), text, "{label}");
            assert_eq!(f.merge(&d).unwrap(), bytes, "{label}: untouched = as uploaded");
            // text that fits the code page keeps it, declaration and all
            let out = merge_map(f, &bytes, Some("ja-JP"), &[(1, fits)]);
            let (dec, _, bad) = enc.decode(&out);
            assert!(!bad && dec.contains(fits) && dec.contains(&format!("encoding=\"{label}\"")) && dec.contains("\"ja-JP\""), "{label}: {dec}");
            // text outside it is written as UTF-8 under a UTF-8 declaration (GB18030 covers all of Unicode)
            if label != "GBK" {
                let out = s(&merge_map(f, &bytes, None, &[(1, "\u{1f600} \u{2603}")]));
                assert!(out.contains("encoding=\"UTF-8\"") && out.contains("\u{1f600} \u{2603}") && out.contains(text), "{label}: {out}");
            }
        }
    }
    // an undeclared legacy byte stream is refused, never guessed (as LocOS and the XML specification)
    assert!(matches!(Dita::new().extract(b"<topic id=\"t\"><title>caf\xe9</title></topic>", &opts(None)), Err(Error::Encoding(_))));
}

#[test]
fn utf16_both_byte_orders() {
    for be in [false, true] {
        let mut b = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
        for c in TOPIC.replace("UTF-8", "UTF-16").encode_utf16() {
            b.extend_from_slice(&if be { c.to_be_bytes() } else { c.to_le_bytes() });
        }
        let f = Dita::new();
        assert!(f.sniff(&b), "be={be}");
        let d = ex(&f, &b);
        assert_eq!(d.units.len(), 2);
        assert_eq!(f.merge(&d).unwrap(), b);
        let out = merge_map(&f, &b, Some("ko-KR"), &[(0, "\u{be75} \u{27E6}g1\u{27E7}x\u{27E6}/g1\u{27E7}")]);
        assert_eq!(&out[..2], &b[..2], "byte order mark kept");
        assert_eq!(show(&ex(&f, &out).units[0]), "\u{be75} \u{27E6}g1\u{27E7}x\u{27E6}/g1\u{27E7}");
    }
}
