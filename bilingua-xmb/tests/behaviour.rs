//! Round trips, eleven languages, legacy encodings and the markup corners of XMB and XTB.

mod common;
use bilingua_model::encoding_rs;
use bilingua_model::*;
use bilingua_xmb::{Xmb, Xtb};
use common::*;

const LANGUAGES_XMB: &[u8] = include_bytes!("data/languages.xmb");
const LANGUAGES_XTB: &[u8] = include_bytes!("data/languages.xtb");

fn all() -> Vec<(&'static str, &'static [u8])> {
    vec![("messages.xmb", XMB), ("localize.xmb", XMB_LOCALIZE), ("fr.xtb", XTB), ("languages.xmb", LANGUAGES_XMB), ("languages.xtb", LANGUAGES_XTB)]
}

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

fn kinds(d: &Document) -> Vec<Vec<TagKind>> {
    d.units
        .iter()
        .map(|u| {
            u.source()
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { kind, .. } => Some(*kind),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

#[test]
fn untouched_merge_is_byte_exact_in_every_mode() {
    for (name, bytes) in all() {
        let d = ex(bytes);
        assert_eq!(merged(&d), bytes, "{name}");
        // paragraph mode is the only mode this crate produces: one segment per unit, ids stable between runs
        for u in &d.units {
            assert_eq!(u.segments.len(), 1, "{name}");
            assert_eq!(u.segments[0].id, u.id);
            assert!(u.separators.is_empty());
        }
        let again = ex(bytes);
        assert_eq!(d.units.iter().map(|u| &u.id).collect::<Vec<_>>(), again.units.iter().map(|u| &u.id).collect::<Vec<_>>());
        assert_eq!(d.skeleton.parser().unwrap().name, "bilingua-xmb");
        // json round trip of the whole document keeps merging
        let j: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(merged(&j), bytes, "{name} through JSON");
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_the_same_units_and_tags() {
    for (name, bytes) in all() {
        let mut d = ex(bytes);
        pseudo(&mut d);
        let out = merged(&d);
        let back = Xtb.extract(&out, &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.format, "xtb");
        assert_eq!(back.units.len(), d.units.len(), "{name}");
        assert_eq!(kinds(&back), kinds(&d), "{name}");
        for (a, b) in d.units.iter().zip(&back.units) {
            assert_eq!(locos(b), format!("FR {}", locos(a)).trim_end(), "{name}");
            assert_eq!(b.context.key, a.context.key);
        }
        // merging the XTB again with the same texts is a fixed point
        assert_eq!(merged(&back), out, "{name}");
    }
}

#[test]
fn target_locale_is_written_as_lang() {
    let cases = [("fr", "fr"), ("pt_BR", "pt-BR"), ("zh-hans-cn", "zh-Hans-CN"), ("ar", "ar"), ("ja_JP", "ja-JP"), ("ko", "ko"), ("es-419", "es-419")];
    for (given, want) in cases {
        let mut d = ex_to(XMB, given);
        pseudo(&mut d);
        let out = s(&merged(&d));
        assert!(out.contains(&format!("<translationbundle lang=\"{want}\">")), "{given}: {out}");
        let mut d = ex_to(XTB, given);
        pseudo(&mut d);
        assert!(s(&merged(&d)).contains(&format!("<translationbundle lang='{want}'>")), "{given}");
    }
    // an unusable locale writes no lang at all (and an XTB keeps the one it had)
    for bad in ["", "  ", "*", "fr\"><x", "a b"] {
        let mut d = ex_to(XMB, bad);
        pseudo(&mut d);
        assert!(s(&merged(&d)).contains("<translationbundle>\n"), "{bad:?}");
        let mut d = ex_to(XTB, bad);
        pseudo(&mut d);
        assert!(s(&merged(&d)).contains("<translationbundle lang='en'>"), "{bad:?}");
    }
    // the language is written without any change to the text too, only on request
    let d = ex_to(XTB, "de");
    assert_eq!(merged(&d), XTB);
    assert!(s(&Xtb.merge_localized(&d).unwrap()).contains("lang='de'"));
}

const TARGETS: [(&str, &str); 11] = [
    ("fr", "Bonjour le monde \u{e9}\u{e8}\u{e0}"),
    ("de", "Gr\u{fc}\u{df}e aus M\u{fc}nchen"),
    ("es", "\u{bf}C\u{f3}mo est\u{e1}s? \u{a1}Ma\u{f1}ana!"),
    ("ru", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{438}\u{440}"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645} 123"),
    ("ja", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{4e16}\u{754c}"),
    ("zh-Hans", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}"),
    ("ko", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} \u{c138}\u{acc4}"),
    ("tr", "G\u{fc}nayd\u{131}n, d\u{fc}nya \u{15f}\u{11f}"),
    ("hi", "\u{928}\u{92e}\u{938}\u{94d}\u{924}\u{947} \u{926}\u{941}\u{928}\u{93f}\u{92f}\u{93e}"),
    ("th", "\u{e2a}\u{e27}\u{e31}\u{e2a}\u{e14}\u{e35}\u{e0a}\u{e32}\u{e27}\u{e42}\u{e25}\u{e01}"),
];

#[test]
fn eleven_languages_round_trip_through_xtb_and_xmb() {
    for (lang, text) in TARGETS {
        // XMB -> XTB, the ph tags carried around the text
        let mut d = ex_to(XMB, lang);
        translate_all(&mut d, |src| format!("{text} | {src}"));
        let out = merged(&d);
        let xtb = Xtb.extract(&out, &opts()).unwrap();
        assert_eq!(xtb.units.len(), d.units.len(), "{lang}");
        for (a, b) in d.units.iter().zip(&xtb.units) {
            assert_eq!(locos(b), format!("{text} | {}", locos(a)).trim_end(), "{lang}");
        }
        // XTB splice in place
        let mut d = ex_to(XTB, lang);
        translate_all(&mut d, |src| format!("{text} {src}"));
        let spliced = merged(&d);
        let back = Xtb.extract(&spliced, &opts()).unwrap();
        for (a, b) in d.units.iter().zip(&back.units) {
            assert_eq!(locos(b), format!("{text} {}", locos(a)).trim_end(), "{lang}");
        }
        // every untouched region is still the original
        assert!(s(&spliced).contains("<translation id=\"empty\"/>"));
    }
}

#[test]
fn languages_fixture_keeps_scripts_intact() {
    let d = ex(LANGUAGES_XMB);
    assert_eq!(d.units.len(), 11);
    let ar = d.units.iter().find(|u| u.context.key.as_deref() == Some("greet.ar")).unwrap();
    assert!(locos(ar).starts_with("\u{645}\u{631}\u{62d}\u{628}\u{627}\u{64b}"));
    assert_eq!(ar.context.notes, ["Description: Greeting in ar | Meaning: home"]);
    assert_eq!(merged(&d), LANGUAGES_XMB);
}

fn legacy(label: &str, decl: &str, body: &str) -> (&'static encoding_rs::Encoding, Vec<u8>) {
    let enc = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
    let text = format!("<?xml version=\"1.0\" encoding=\"{decl}\"?>\n<translationbundle lang=\"x\">\n  <translation id=\"a\">{body}</translation>\n</translationbundle>\n");
    let (bytes, _, bad) = enc.encode(&text);
    assert!(!bad);
    (enc, bytes.into_owned())
}

#[test]
fn legacy_encodings_are_kept() {
    let cases = [
        ("windows-1251", "windows-1251", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"),
        ("windows-1253", "windows-1253", "\u{393}\u{3b5}\u{3b9}\u{3b1} \u{3c3}\u{3bf}\u{3c5}"),
        ("windows-1252", "windows-1252", "caf\u{e9} cr\u{e8}me"),
        ("shift_jis", "Shift_JIS", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"),
        ("euc-kr", "EUC-KR", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}"),
        ("gbk", "GBK", "\u{4f60}\u{597d}\u{4e16}\u{754c}"),
        ("big5", "Big5", "\u{4f60}\u{597d}\u{4e16}\u{754c}"),
        ("windows-1256", "windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627}"),
    ];
    for (label, decl, body) in cases {
        let (enc, bytes) = legacy(label, decl, body);
        let mut d = Xtb.extract(&bytes, &opts()).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(plain_text(&d.units[0].source()), body, "{label}");
        assert_eq!(Xtb.merge(&d).unwrap(), bytes, "{label} untouched");
        // a translation the code page can hold is written in it, the declaration kept
        d.units[0].segments[0].target = vec![Inline::text(body.to_string() + body)];
        let out = Xtb.merge(&d).unwrap();
        let (decoded, _, bad) = enc.decode(&out);
        assert!(!bad, "{label}");
        assert!(decoded.contains(&format!("{body}{body}</translation>")), "{label}");
        assert!(decoded.contains(&format!("encoding=\"{decl}\"")), "{label}");
        // one it cannot hold becomes a character reference in the same encoding
        d.units[0].segments[0].target = vec![Inline::text("\u{1f600} \u{2603}")];
        let out = Xtb.merge(&d).unwrap();
        let (decoded, _, bad) = enc.decode(&out);
        assert!(!bad, "{label}");
        assert!(decoded.contains("&#128512; ") || decoded.contains("\u{2603}") || decoded.contains("&#9731;"), "{label}: {decoded}");
        assert!(Xtb.extract(&out, &opts()).is_ok(), "{label}");
    }
}

#[test]
fn utf16_files_keep_their_byte_order_mark() {
    for be in [false, true] {
        let text = s(XTB).replace("UTF-8", "UTF-16");
        let mut bytes = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
        for u in text.encode_utf16() {
            bytes.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        let mut d = Xtb.extract(&bytes, &opts()).unwrap();
        assert_eq!(d.units.len(), 4);
        assert_eq!(Xtb.merge(&d).unwrap(), bytes);
        d.units[0].segments[0].target = vec![Inline::text("\u{645}\u{631}\u{62d}\u{628}\u{627}")];
        let out = Xtb.merge(&d).unwrap();
        assert_eq!(&out[..2], &bytes[..2]);
        let back = Xtb.extract(&out, &opts()).unwrap();
        assert_eq!(plain_text(&back.units[0].source()), "\u{645}\u{631}\u{62d}\u{628}\u{627}");
    }
}

#[test]
fn utf8_bom_and_crlf_survive_a_real_edit() {
    let bom_crlf: Vec<u8> = [&b"\xef\xbb\xbf"[..], s(XTB).replace('\n', "\r\n").as_bytes()].concat();
    let mut d = Xtb.extract(&bom_crlf, &opts()).unwrap();
    d.units[0].segments[0].target = vec![Inline::text("Salut\nmonde")];
    let out = Xtb.merge(&d).unwrap();
    assert!(out.starts_with(b"\xef\xbb\xbf"));
    assert!(out.windows(11).any(|w| w == b"Salut\r\nmond"), "newlines follow the file");
    assert!(!s(&out[3..]).replace("\r\n", "").contains('\n'));
}

#[test]
fn markup_corners() {
    // CDATA, character references, a comment and an unknown element inside a translation; the single-quoted attribute
    let xtb = "<?xml version='1.0'?>\n<translationbundle lang='en'>\n<translation id='a&amp;b' note='x'>A<![CDATA[ <b> & ]]>&#x41;&#10;<!-- keep --><ph name='P'/>tail<foo bar='1'>in</foo></translation>\n</translationbundle>\n";
    let mut d = Xtb.extract(xtb.as_bytes(), &opts()).unwrap();
    let u = &d.units[0];
    assert_eq!(u.context.key.as_deref(), Some("a&b"));
    assert_eq!(locos(u), "A <b> & A\n\u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7}tail\u{27E6}x3\u{27E7}");
    assert_eq!(Xtb.merge(&d).unwrap(), xtb.as_bytes());
    // translating keeps the comment and the unknown element as the codes they are
    let (_, map) = bilingua_model::profile::tokens::to_locos_text(&d.units[0].source());
    d.units[0].segments[0].target =
        bilingua_model::profile::tokens::from_locos_text("Z \u{27E6}x3\u{27E7} \u{27E6}x2\u{27E7} <&> \u{27E6}x1\u{27E7}", &map);
    let out = s(&Xtb.merge(&d).unwrap());
    assert!(out.contains("<translation id='a&amp;b' note='x'>Z <foo bar='1'>in</foo> <ph name='P'/> &lt;&amp;&gt; <!-- keep --></translation>"), "{out}");
    // and an XMB conversion drops them (an XTB carries only text and ph)
    let xmb = "<messagebundle><msg id='1'>A<!-- c --><ph name='P'><ex>e</ex>o</ph><foo/>B</msg></messagebundle>";
    let mut d = Xmb.extract(xmb.as_bytes(), &opts()).unwrap();
    d.units[0].segments[0].target = d.units[0].segments[0].source.clone();
    let out = s(&Xmb.merge(&d).unwrap());
    assert!(out.contains("<translation id=\"1\">A<ph name=\"P\"/>B</translation>"), "{out}");
}

#[test]
fn ph_pairing_follows_the_start_close_names() {
    let xmb = "<messagebundle><msg id='1'><ph name='START_A'/>x<ph name='START_B'/>y<ph name='CLOSE_A'/>z<ph name='CLOSE_B'/></msg>\
        <msg id='2'><ph name='START_A'/>x<ph name='CLOSE_Z'/>y<ph name='CLOSE_A'/></msg>\
        <msg id='3'><ph name='CLOSE_A'/>x<ph name='START_A'/></msg>\
        <msg id='4'><ph name='START_A'/><ph name='START_A'/>t<ph name='CLOSE_A'/><ph name='CLOSE_A'/></msg></messagebundle>";
    let d = Xmb.extract(xmb.as_bytes(), &opts()).unwrap();
    let t = texts(&d);
    // 1: B is on top when CLOSE_A arrives, so nothing pairs with A; CLOSE_B pairs with B
    assert_eq!(t[0], "\u{27E6}x1\u{27E7}x\u{27E6}g2\u{27E7}y\u{27E6}x3\u{27E7}z\u{27E6}/g2\u{27E7}");
    // 2: an unrelated close in between does not break the pair
    assert_eq!(t[1], "\u{27E6}g1\u{27E7}x\u{27E6}x2\u{27E7}y\u{27E6}/g1\u{27E7}");
    // 3: close before start, and a start with no close, are point tokens
    assert_eq!(t[2], "\u{27E6}x1\u{27E7}x\u{27E6}x2\u{27E7}");
    // 4: nested equal names pair inside out
    assert_eq!(t[3], "\u{27E6}g1\u{27E7}\u{27E6}g2\u{27E7}t\u{27E6}/g2\u{27E7}\u{27E6}/g1\u{27E7}");
}

#[test]
fn notes_cap_and_attribute_normalisation() {
    let long = "d".repeat(3000);
    let xmb = format!("<messagebundle><msg id=\"1\" desc=\"{long}\" meaning=\"a\tb\nc\">Hi</msg><msg id=\"2\" desc=\"\">Yo</msg></messagebundle>");
    let d = Xmb.extract(xmb.as_bytes(), &opts()).unwrap();
    assert_eq!(d.units[0].context.notes[0].chars().count(), 2000);
    assert!(d.units[1].context.notes.is_empty());
    assert_eq!(d.units[0].context.extra["meaning"], "a b c");
}

#[test]
fn messages_without_text_or_id_are_not_units() {
    let xmb = "<messagebundle><msg>no id</msg><msg id='a'>   </msg><msg id='b'/><msg id='c'>ok</msg><msg id='c'>dup</msg><other id='z'>x</other></messagebundle>";
    let d = Xmb.extract(xmb.as_bytes(), &opts()).unwrap();
    assert_eq!(texts(&d), ["ok"]);
    assert_eq!(merged(&d), xmb.as_bytes());
    // the XTB keeps whitespace-only and empty messages, once per id, and skips the id-less one
    let mut d = d;
    d.units[0].segments[0].target = vec![Inline::text("fait")];
    let out = s(&Xmb.merge(&d).unwrap());
    assert_eq!(
        out,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE translationbundle>\n<translationbundle lang=\"fr\">\n  <translation id=\"a\">   </translation>\n  <translation id=\"b\"></translation>\n  <translation id=\"c\">fait</translation>\n</translationbundle>\n"
    );
}

#[test]
fn an_edited_document_that_no_longer_matches_its_skeleton_is_refused() {
    let mut d = ex(XMB);
    d.units.pop();
    d.units[0].segments[0].target = vec![Inline::text("x")];
    assert!(matches!(Xmb.merge(&d), Err(Error::Write(_))));
    let mut d = ex(XMB);
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(Xmb.merge(&d).is_err());
    let mut d = ex(XMB);
    if let Skeleton::Text { parser, .. } = &mut d.skeleton {
        parser.as_mut().unwrap().name = "bilingua-other".into();
    }
    assert!(matches!(Xmb.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn locked_and_untranslatable_units_keep_their_text() {
    let mut d = ex(XTB);
    d.units[0].translate = false;
    d.units[0].segments[0].target = vec![Inline::text("changed")];
    assert_eq!(merged(&d), XTB);
}
