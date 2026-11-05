//! The LocOS suite `tests/verify_angular_xmb_filter.py`, ported case by case. LocOS has 8 test functions: the
//! registration, units, XTB export, XTB identity and splice, tolerant tokens, hostile input and Angular XLIFF
//! cases are here (7); the upload/download case belongs to the LocOS app (database, HTTP) and is not a filter test.

mod common;
use bilingua_model::*;
use bilingua_xmb::{Xmb, Xtb};
use common::*;

fn ids_in(xtb: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut rest = xtb;
    while let Some(p) = rest.find("<translation id=\"") {
        let r = &rest[p + 17..];
        let e = r.find('"').unwrap();
        v.push(r[..e].to_string());
        rest = &r[e..];
    }
    v
}

fn ph_names(s: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut rest = s;
    while let Some(p) = rest.find("<ph name=\"") {
        let r = &rest[p + 10..];
        let e = r.find('"').unwrap();
        v.push(r[..e].to_string());
        rest = &r[e..];
    }
    v
}

fn translation<'a>(xtb: &'a str, id: &str) -> &'a str {
    let start = xtb.find(&format!("<translation id=\"{id}\">")).unwrap_or_else(|| panic!("no translation {id}"));
    let end = xtb[start..].find("</translation>").unwrap() + start;
    &xtb[start..end]
}

#[test]
fn registered() {
    assert_eq!((Xmb.id(), Xmb.extensions()), ("xmb", &["xmb"][..]));
    assert_eq!((Xtb.id(), Xtb.extensions()), ("xtb", &["xtb"][..]));
    assert!(Xmb.sniff(XMB) && !Xmb.sniff(XTB));
    assert!(Xtb.sniff(XTB) && !Xtb.sniff(XMB));
    assert!(!Xmb.sniff(b"{\"a\": 1}") && !Xtb.sniff(b"plain text"));
    assert!(!Xmb.sniff(b"<html><body>messagebundle</body></html>"));
}

#[test]
fn units_tokens_notes() {
    let d = ex(XMB);
    // msg 5 (placeholder only) and msg 7 (duplicate id) are not units
    assert_eq!(d.format, "xmb");
    assert_eq!(d.units.len(), 6);
    let idx: Vec<&str> = d.units.iter().map(|u| u.context.extra["index"].as_str()).collect();
    assert_eq!(idx, ["0", "1", "2", "3", "4", "6"]);
    let t = texts(&d);
    assert_eq!(t[0], "Hello i18n!");
    assert!(!t.concat().contains("src/app"), "<source> must be skipped");
    assert_eq!(d.units[0].context.notes, ["Description: An introduction header for this sample | Meaning: User welcome"]);
    assert_eq!(t[1], "Hello, \u{27E6}x1\u{27E7}! You have \u{27E6}x2\u{27E7} new messages.");
    assert!(d.units[1].context.notes[0].contains("INTERPOLATION = {{ user.name }}"));
    assert_eq!(t[2], "The \u{27E6}g1\u{27E7}sky\u{27E6}/g1\u{27E7} is blue & \u{27E6}g2\u{27E7}clear\u{27E6}/g2\u{27E7}\u{27E6}x3\u{27E7}today.");
    assert!(t[4].starts_with("{VAR_PLURAL, plural, =0 {just now}"), "ICU stays text: {}", t[4]);
    assert_eq!(t[5], "Save changes", "edge white space stays outside the unit");
    assert_eq!(d.units[5].context.key.as_deref(), Some("custom.save"));
    let loc = ex(XMB_LOCALIZE);
    assert_eq!(locos(&loc.units[0]), "You have \u{27E6}x1\u{27E7} items in \u{27E6}g2\u{27E7}your cart\u{27E6}/g2\u{27E7}.");
    assert!(loc.units[0].context.notes[0].starts_with("Description: Cart summary"));
}

#[test]
fn xmb_exports_xtb() {
    let mut d = ex_to(XMB, "fr-CA");
    pseudo(&mut d);
    let out = String::from_utf8(merged(&d)).unwrap();
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE translationbundle>"));
    assert!(out.contains("<translationbundle lang=\"fr-CA\">"));
    assert_eq!(
        ids_in(&out),
        ["introductionHeader", "2367646551098052009", "5811701742971715242", "4606963464835766483", "2002272803511843863", "3967965900462880190", "custom.save"],
        "XMB order, duplicates once, ids byte-exact"
    );
    let t = translation(&out, "5811701742971715242");
    assert_eq!(ph_names(t), ["START_BOLD_TEXT", "CLOSE_BOLD_TEXT", "START_LINK", "CLOSE_LINK", "LINE_BREAK"]);
    assert!(!out.contains("<ex>"), "no <ex> in an XTB");
    assert!(t.starts_with("<translation id=\"5811701742971715242\">FR The <ph name=\"START_BOLD_TEXT\"/>"), "{t}");
    assert!(out.contains("is blue &amp; "));
    assert_eq!(ph_names(translation(&out, "3967965900462880190")), ["PH"], "placeholder-only message copied as is");
    assert!(translation(&out, "custom.save").contains("FR Save changes"));
    // untranslated messages fall back to the source text; no target locale means no lang
    let mut plain = ex(XMB);
    plain.target_lang = None;
    let out = String::from_utf8(Xmb.to_xtb(&plain).unwrap()).unwrap();
    assert!(out.contains("<translationbundle>\n"));
    assert!(out.contains("<translation id=\"introductionHeader\">Hello i18n!</translation>"));
    // a reordered pair and a moved point token survive
    let mut d = ex_to(XMB, "fr");
    set_locos(&mut d, 2, "\u{27E6}x3\u{27E7}Le \u{27E6}g2\u{27E7}ciel\u{27E6}/g2\u{27E7} \u{27E6}g1\u{27E7}bleu\u{27E6}/g1\u{27E7}");
    let out = String::from_utf8(merged(&d)).unwrap();
    assert_eq!(ph_names(translation(&out, "5811701742971715242")), ["LINE_BREAK", "START_LINK", "CLOSE_LINK", "START_BOLD_TEXT", "CLOSE_BOLD_TEXT"]);
}

#[test]
fn untouched_xmb_merge_is_the_original_file() {
    for f in [XMB, XMB_LOCALIZE] {
        assert_eq!(merged(&ex(f)), f);
    }
}

#[test]
fn xtb_identity_and_splice() {
    let d = ex(XTB);
    assert_eq!(d.format, "xtb");
    assert_eq!(merged(&d), XTB);
    // a translation identical to its source changes nothing
    let mut same = ex(XTB);
    translate_all(&mut same, |s| s.to_string());
    assert_eq!(merged(&same), XTB);
    let bom: Vec<u8> = [&b"\xef\xbb\xbf"[..], XTB].concat();
    let crlf = String::from_utf8(XTB.to_vec()).unwrap().replace('\n', "\r\n").into_bytes();
    for variant in [bom, crlf] {
        assert_eq!(merged(&ex(&variant)), variant);
    }
    let mut d = ex_to(XTB, "fr");
    let t = texts(&d);
    assert_eq!(t[1], "Hello, \u{27E6}x1\u{27E7}! You have \u{27E6}x2\u{27E7} new messages.");
    set_locos(&mut d, 1, "Bonjour, \u{27E6}x1\u{27E7} ! \u{27E6}x2\u{27E7} nouveaux.");
    let out = String::from_utf8(merged(&d)).unwrap();
    assert!(out.contains("<translationbundle lang='fr'>"), "{out}");
    assert!(out.contains(
        "<translation id=\"2367646551098052009\">Bonjour, <ph name=\"INTERPOLATION\"/> ! <ph name=\"INTERPOLATION_1\" /> nouveaux.</translation>"
    ));
    // everything else is byte-identical
    let back = out
        .replace("lang='fr'", "lang='en'")
        .replace(
            "Bonjour, <ph name=\"INTERPOLATION\"/> ! <ph name=\"INTERPOLATION_1\" /> nouveaux.",
            "Hello, <ph name=\"INTERPOLATION\"/>! You have <ph name=\"INTERPOLATION_1\" /> new messages.",
        )
        .into_bytes();
    assert_eq!(back, XTB);
    // white space edges kept; a missing lang is added
    let nolang = String::from_utf8(XTB.to_vec()).unwrap().replace(" lang='en'", "").into_bytes();
    let mut d = ex_to(&nolang, "fr");
    let i = d.units.iter().position(|u| u.context.key.as_deref() == Some("custom.save")).unwrap();
    set_locos(&mut d, i, "Enregistrer");
    let out = String::from_utf8(merged(&d)).unwrap();
    assert!(out.contains("<translationbundle lang=\"fr\">"));
    assert!(out.contains("<translation id=\"custom.save\">\n      Enregistrer\n  </translation>"), "{out}");
    // a self-closed translation is never a unit; a changed one is escaped
    let one = br#"<?xml version="1.0"?><translationbundle><translation id="a">Hi</translation><translation id="b"/></translationbundle>"#;
    let mut d = ex(one);
    d.target_lang = None; // LocOS build() without a locale: lang untouched
    assert_eq!(d.units.len(), 1);
    d.units[0].segments[0].target = vec![Inline::text("Salut & co")];
    let want = String::from_utf8(one.to_vec()).unwrap().replace(">Hi<", ">Salut &amp; co<").into_bytes();
    assert_eq!(merged(&d), want);
}

#[test]
fn tolerant_tokens() {
    let mut d = ex(XMB);
    // LocOS text "A g1 b x9 x3 x3 /g7 g2 c": unknown, repeated and stray tokens dropped, unclosed pairs closed
    let tag = |id: &str, kind| Inline::tag(id, kind, "");
    d.units[2].segments[0].target = vec![
        Inline::text("A "),
        tag("1", TagKind::Open),
        Inline::text("b "),
        tag("9", TagKind::Standalone),
        Inline::text(" "),
        tag("3", TagKind::Standalone),
        Inline::text(" "),
        tag("3", TagKind::Standalone),
        Inline::text(" "),
        tag("7", TagKind::Close),
        Inline::text(" "),
        tag("2", TagKind::Open),
        Inline::text("c"),
    ];
    let out = String::from_utf8(merged(&d)).unwrap();
    let t = translation(&out, "5811701742971715242");
    assert_eq!(ph_names(t), ["START_BOLD_TEXT", "LINE_BREAK", "START_LINK", "CLOSE_LINK", "CLOSE_BOLD_TEXT"], "{t}");
    assert!(!t.contains('\u{27E6}'));
}

fn refused(data: &[u8]) -> String {
    msg(try_ex(data))
}

#[test]
fn hostile() {
    let lol = br#"<?xml version="1.0"?><!DOCTYPE messagebundle [<!ENTITY a "aaaa"><!ENTITY b "&a;&a;&a;">]><messagebundle><msg id="1">&b;</msg></messagebundle>"#;
    assert!(refused(lol).contains("entities"), "{}", refused(lol));
    let pe = br#"<?xml version="1.0"?><!DOCTYPE messagebundle [<!ENTITY % p SYSTEM "file:///etc/passwd">%p;]><messagebundle/>"#;
    assert!(refused(pe).contains("entities"));
    let ext = br#"<?xml version="1.0"?><!DOCTYPE messagebundle SYSTEM "http://127.0.0.1/x.dtd"><messagebundle><msg id="1">x</msg></messagebundle>"#;
    assert!(refused(ext).contains("external DTD"));
    let public = br#"<?xml version="1.0"?><!DOCTYPE messagebundle PUBLIC "-//x//y" "http://127.0.0.1/x.dtd"><messagebundle/>"#;
    assert!(refused(public).contains("external DTD"));
    assert!(refused(b"<html><msg id='1'>x</msg></html>").contains("root element"));
    assert!(refused(b"<messagebundle><msg id='1'>x</messagebundle>").contains("well-formed"));
    assert!(refused(b"<messagebundle><msg id='1'>&nbsp;</msg></messagebundle>").contains("well-formed"));
    // the segment cap
    let many: Vec<u8> =
        [b"<messagebundle>".to_vec(), (0..5).flat_map(|i| format!("<msg id=\"{i}\">m{i}</msg>").into_bytes()).collect(), b"</messagebundle>".to_vec()].concat();
    let small = Limits { max_units: 3, ..Limits::default() };
    let e = Xmb.extract_with(&many, &opts(), &small).err().expect("refused");
    assert!(matches!(e, Error::Limit(LimitError { kind: LimitKind::Units, .. })), "{e}");
    let markup: Vec<u8> = [b"<messagebundle>".to_vec(), b"<x/>".repeat(200), b"</messagebundle>".to_vec()].concat();
    let e = Xmb.extract_with(&markup, &opts(), &small).err().expect("refused");
    assert!(matches!(e, Error::Limit(_)), "{e}");
    // the Angular internal-subset DOCTYPE itself is accepted and kept, never resolved
    assert!(!ex(XMB).units.is_empty());
    assert!(String::from_utf8_lossy(XMB).contains("<!ATTLIST msg desc CDATA #IMPLIED>"));
    assert_eq!(merged(&ex(XMB)), XMB);
}

fn xliff_translate(src: &[u8], lang: &str) -> (Document, String) {
    let xliff = bilingua_xliff::filters().into_iter().find(|f| f.id() == "xliff").unwrap();
    let mut d = xliff
        .extract(src, &ExtractOptions { source_lang: Some("en-US".into()), target_lang: Some(lang.into()), ..Default::default() })
        .unwrap();
    for u in d.units.iter_mut() {
        for s in u.segments.iter_mut() {
            let mut t = vec![Inline::text("FR ")];
            t.extend(s.source.clone());
            s.target = t;
            s.state = SegState::Draft;
        }
    }
    let out = xliff.merge(&d).unwrap();
    (d, String::from_utf8(out).unwrap())
}

#[test]
fn angular_xliff_round_trip() {
    // The Angular `ng extract-i18n` XLIFF 1.2 and 2.0 fixtures go through Bilingua's XLIFF crate: inline codes
    // stay intact and the target language is written.
    let (d, s) = xliff_translate(include_bytes!("data/messages.xlf"), "fr");
    // note: Bilingua's XLIFF 1.2 writer does not add target-language to <file> (LocOS does); that belongs to bilingua-xliff
    assert!(s.contains("id=\"START_BOLD_TEXT\"") && s.contains("id=\"CLOSE_BOLD_TEXT\""), "{s}");
    assert!(s.contains("equiv-text=\"&lt;b&gt;\"") || s.contains("equiv-text=\"<b>\""), "{s}");
    assert!(d.units.len() >= 3);
    let (_, s) = xliff_translate(include_bytes!("data/messages.xlf2.xlf"), "fr");
    assert!(s.contains("srcLang=\"en-US\""), "{s}"); // trgLang: not written by bilingua-xliff (LocOS does)
    assert!(s.contains("equivStart=\"START_BOLD_TEXT\"") && s.contains("equivEnd=\"CLOSE_BOLD_TEXT\""), "{s}");
    assert!(s.contains("equiv=\"INTERPOLATION\""), "{s}");
}
