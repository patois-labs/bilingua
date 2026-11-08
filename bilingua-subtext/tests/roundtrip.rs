//! Round trips in eleven languages and the encodings LocOS reads: an untouched merge is the original bytes, a
//! pseudo-translated merge reads back with the same units and tags (in the file's own encoding, or UTF-8 when asked),
//! and a target locale changes nothing (neither format names a language).
mod common;
use bilingua_model::encoding_rs::Encoding;
use bilingua_model::*;
use common::*;

/// (language, source_lang, legacy code page, sample lines)
const LANGS: &[(&str, &str, &str, [&str; 3])] = &[
    ("en", "en", "windows-1252", ["Hello, my name is Alice.", "Line one {0} and %s", "See you at 5 o'clock."]),
    ("fr", "fr", "windows-1252", ["Bonjour, je m'appelle Alice.", "Caf\u{e9} cr\u{e8}me {0}", "\u{c0} bient\u{f4}t !"]),
    ("de", "de", "windows-1252", ["Guten Tag, ich hei\u{df}e Alice.", "Gr\u{fc}\u{df}e aus K\u{f6}ln %s", "Sch\u{f6}ne Gr\u{fc}\u{df}e"]),
    ("ru", "ru", "windows-1251", ["\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{435}\u{43d}\u{44f} \u{437}\u{43e}\u{432}\u{443}\u{442} \u{410}\u{43b}\u{438}\u{441}\u{430}.", "\u{41a}\u{43e}\u{444}\u{435} {0}", "\u{414}\u{43e} \u{432}\u{441}\u{442}\u{440}\u{435}\u{447}\u{438}"]),
    ("el", "el", "windows-1253", ["\u{393}\u{3b5}\u{3b9}\u{3b1} \u{3c3}\u{3bf}\u{3c5}, \u{3b5}\u{3af}\u{3bc}\u{3b1}\u{3b9} \u{3b7} \u{391}\u{3bb}\u{3af}\u{3ba}\u{3b7}.", "\u{39a}\u{3b1}\u{3c6}\u{3ad}\u{3c2} %s", "\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1}"]),
    ("he", "he", "windows-1255", ["\u{5e9}\u{5dc}\u{5d5}\u{5dd}, \u{5d0}\u{5e0}\u{5d9} \u{5d0}\u{5dc}\u{5d9}\u{5e1}.", "\u{5e7}\u{5e4}\u{5d4} {0}", "\u{5dc}\u{5d4}\u{5ea}\u{5e8}\u{5d0}\u{5d5}\u{5ea}"]),
    ("ar", "ar", "windows-1256", ["\u{645}\u{631}\u{62d}\u{628}\u{627}\u{60c} \u{627}\u{633}\u{645}\u{64a} \u{623}\u{644}\u{64a}\u{633}.", "\u{642}\u{647}\u{648}\u{629} {0}", "\u{625}\u{644}\u{649} \u{627}\u{644}\u{644}\u{642}\u{627}\u{621}"]),
    ("ja", "ja", "shift_jis", ["\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{3001}\u{79c1}\u{306f}\u{30a2}\u{30ea}\u{30b9}\u{3067}\u{3059}\u{3002}", "\u{30b3}\u{30fc}\u{30d2}\u{30fc} {0}", "\u{307e}\u{305f}\u{306d}"]),
    ("zh", "zh", "gbk", ["\u{4f60}\u{597d}\u{ff0c}\u{6211}\u{53eb}\u{7231}\u{4e3d}\u{4e1d}\u{3002}", "\u{5496}\u{5561} %s", "\u{518d}\u{89c1}"]),
    ("ko", "ko", "euc-kr", ["\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}, \u{c800}\u{b294} \u{c568}\u{b9ac}\u{c2a4}\u{c785}\u{b2c8}\u{b2e4}.", "\u{ce74}\u{d398} {0}", "\u{b610} \u{ac11}\u{c2dc}\u{b2e4}"]),
    ("tr", "tr", "windows-1254", ["Merhaba, ad\u{131}m Alice.", "\u{15e}ehir kahvesi {0}", "G\u{f6}r\u{fc}\u{15f}\u{fc}r\u{fc}z"]),
];

fn sbv(l: &[&str; 3], nl: &str) -> String {
    format!("0:00:01.000,0:00:03.500{nl}{}{nl}{nl}0:00:04.000,0:00:06.000{nl}{}{nl}{}{nl}{nl}", l[0], l[1], l[2])
}

fn sv2(l: &[&str; 3], nl: &str) -> String {
    format!("[INFORMATION]{nl}[TITLE]demo{nl}[END INFORMATION]{nl}[SUBTITLE]{nl}00:00:01.00,00:00:03.50{nl}{}[br]{}{nl}{nl}00:00:04.00,00:00:06.00{nl}{}{nl}{nl}", l[0], l[1], l[2])
}

fn mdvd(l: &[&str; 3], nl: &str) -> String {
    format!("{{1}}{{1}}25{nl}{{25}}{{87}}{{y:i}}{}|{}{nl}{{100}}{{150}}{{c:$0000FF}}{}|{{y:b}}{}{nl}", l[0], l[1], l[2], l[0])
}

fn enc(label: &str, s: &str) -> Vec<u8> {
    Encoding::for_label(label.as_bytes()).unwrap().encode(s).0.into_owned()
}

fn pseudo(fmt: &str, data: &[u8], lang: &str, mo: &MergeOptions) -> (Document, Vec<u8>) {
    let o = opts().with_source_lang(lang).with_target_lang("es-ES");
    let mut doc = filter(fmt).extract(data, &o).unwrap();
    for u in &mut doc.units {
        let mut t = vec![Inline::text("\u{3042}\u{627} Zq ")];
        t.extend(u.source());
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
    let out = filter(fmt).merge_with(&doc, mo).unwrap();
    (doc, out)
}

fn check(fmt: &str, data: &[u8], lang: &str, expect_units: usize) {
    let o = opts().with_source_lang(lang);
    let doc = filter(fmt).extract(data, &o).unwrap_or_else(|e| panic!("{fmt} {lang}: {e}"));
    assert_eq!(doc.units.len(), expect_units, "{fmt} {lang}");
    assert_eq!(filter(fmt).merge(&doc).unwrap(), data, "{fmt} {lang}: untouched merge");
    // a target locale and the stamp-always option change nothing: neither format names a language
    let mut with_target = doc.clone();
    with_target.target_lang = Some("fr-FR".into());
    let always = MergeOptions::new().with_stamp_target_always(true);
    assert_eq!(filter(fmt).merge_with(&with_target, &always).unwrap(), data, "{fmt} {lang}: stamp");
    for mo in [MergeOptions::default(), bilingua_model::profile::locos().merge_options("xml")] {
        let (before, out) = pseudo(fmt, data, lang, &mo);
        let back = filter(fmt).extract(&out, &opts().with_source_lang(lang)).unwrap_or_else(|e| panic!("{fmt} {lang}: reread: {e}"));
        assert_eq!(back.units.len(), before.units.len(), "{fmt} {lang}: unit count");
        for (a, b) in before.units.iter().zip(&back.units) {
            let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            assert_eq!(tags(a), tags(b), "{fmt} {lang}: tags of {}", a.id);
            assert!(plain_text(&b.source()).starts_with("\u{3042}\u{627} Zq"), "{fmt} {lang}: {:?}", plain_text(&b.source()));
        }
        if mo.output_encoding == OutputEncoding::Utf8 {
            assert!(std::str::from_utf8(&out).is_ok(), "{fmt} {lang}: UTF-8 requested");
        }
    }
}

#[test]
fn eleven_languages_in_utf8_bom_crlf_and_their_code_pages() {
    for (lang, src, cp, lines) in LANGS {
        for (fmt, make, n) in [("sbv", sbv as fn(&[&str; 3], &str) -> String, 2usize), ("sub", sv2, 2), ("sub", mdvd, 2)] {
            for nl in ["\n", "\r\n", "\r"] {
                let text = make(lines, nl);
                check(fmt, text.as_bytes(), src, n);
                let mut bom = b"\xEF\xBB\xBF".to_vec();
                bom.extend(text.as_bytes());
                check(fmt, &bom, src, n);
                // the language's own code page (every sample line fits it)
                let legacy = enc(cp, &text);
                if std::str::from_utf8(&legacy).is_err() {
                    check(fmt, &legacy, src, n);
                    // and the file keeps that code page when asked to keep it
                    let (_, out) = pseudo_keep(fmt, &legacy, src);
                    assert!(std::str::from_utf8(&out).is_err() || legacy.is_ascii(), "{lang}: {cp} kept");
                }
            }
        }
    }
}

/// A pseudo translation that fits the code page (ASCII), merged keeping the encoding.
fn pseudo_keep(fmt: &str, data: &[u8], lang: &str) -> (Document, Vec<u8>) {
    let o = opts().with_source_lang(lang);
    let mut doc = filter(fmt).extract(data, &o).unwrap();
    for u in &mut doc.units {
        let mut t = vec![Inline::text("Zq ")];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = filter(fmt).merge(&doc).unwrap();
    let back = filter(fmt).extract(&out, &o).unwrap();
    assert_eq!(back.units.len(), doc.units.len());
    (doc, out)
}

#[test]
fn utf16_both_orders() {
    for (lang, src, _, lines) in LANGS {
        for (fmt, make) in [("sbv", sbv as fn(&[&str; 3], &str) -> String), ("sub", sv2), ("sub", mdvd)] {
            let text = make(lines, "\r\n");
            let mut le = vec![0xFF, 0xFE];
            le.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            let mut be = vec![0xFE, 0xFF];
            be.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
            check(fmt, &le, src, 2);
            check(fmt, &be, src, 2);
            // kept as UTF-16 by default, UTF-8 for the adapter
            let (_, kept) = pseudo(fmt, &le, src, &MergeOptions::default());
            assert!(kept.starts_with(&[0xFF, 0xFE]), "{lang}");
            let (_, utf8) = pseudo(fmt, &le, src, &bilingua_model::profile::locos().merge_options("xml"));
            assert!(!utf8.starts_with(&[0xFF, 0xFE]) && std::str::from_utf8(&utf8).is_ok(), "{lang}");
        }
    }
}

#[test]
fn a_translation_outside_the_code_page_is_written_as_utf8() {
    let (_, _, cp, lines) = LANGS[1];
    let data = enc(cp, &sbv(&lines, "\n"));
    let o = opts().with_source_lang("fr");
    let mut doc = Sbv.extract(&data, &o).unwrap();
    doc.units[0].segments[0].target = vec![Inline::text("\u{65e5}\u{672c}\u{8a9e}")];
    let out = Sbv.merge(&doc).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("\u{65e5}\u{672c}\u{8a9e}"));
}

use bilingua_subtext::Sbv;

#[test]
fn units_carry_their_timing_and_stable_ids() {
    let (_, _, _, lines) = LANGS[0];
    let doc = Sbv.extract(sbv(&lines, "\n").as_bytes(), &opts()).unwrap();
    assert_eq!(doc.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["c1", "c2"]);
    assert_eq!(doc.units[0].context.extra["start"], "0:00:01.000");
    assert_eq!(doc.units[1].context.extra["end"], "0:00:06.000");
    assert_eq!(doc.units[0].context.kind.as_deref(), Some("subtitle"));
    let md = filter("sub").extract(mdvd(&lines, "\n").as_bytes(), &opts()).unwrap();
    assert_eq!(md.units[0].context.extra["start"], "25");
    assert_eq!(md.units[0].context.extra["end"], "87");
}

#[test]
fn nothing_translated_keeps_every_byte_even_with_the_adapter_options() {
    let (_, _, cp, lines) = LANGS[6];
    for data in [enc(cp, &sbv(&lines, "\r\n")), sbv(&lines, "\r\n").into_bytes()] {
        let o = opts().with_source_lang("ar");
        let doc = Sbv.extract(&data, &o).unwrap();
        assert_eq!(Sbv.merge_with(&doc, &bilingua_model::profile::locos().merge_options("xml")).unwrap(), data);
        // a localized export is written even when nothing is translated (LocOS's guard hands back the upload only for
        // a plain build): the same text, as UTF-8 (B-SIDE W6 st-1 goldens)
        let doc = Sbv.extract(&data, &o.clone().with_target_lang("en")).unwrap();
        let out = Sbv.merge_with(&doc, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        assert_eq!(out, sbv(&lines, "\r\n").into_bytes());
    }
}
