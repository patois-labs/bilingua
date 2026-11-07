//! Every fixture in `tests/data`: round trips, the 11 languages and each encoding the filter reads.

mod common;

use bilingua_model::*;
use bilingua_xslt::Xslt;
use common::*;

include!("data_list.inc");

fn file(name: &str) -> &'static [u8] {
    FILES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no fixture {name}")).1
}

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => enc::base(encoding).to_string(),
        _ => String::new(),
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in FILES {
        let d = extract(data);
        assert_eq!(Xslt.merge(&d).unwrap(), data, "{name}");
        // a target language alone changes nothing: nothing is translated
        let d = Xslt.extract(data, &opts().with_target_lang("fr-FR")).unwrap();
        assert_eq!(Xslt.merge(&d).unwrap(), data, "{name} with a target language");
        assert_identity(data);
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in FILES {
        let before = extract(data);
        let out = translate_all(data, |t| format!("[{t}]"));
        let after = extract(&out);
        assert_eq!(after.units.len(), before.units.len(), "{name}");
        assert_eq!(tag_count(&after), tag_count(&before), "{name}");
        assert_eq!(encoding(&after), encoding(&before), "{name}: encoding kept");
        for (i, (a, b)) in after.units.iter().zip(&before.units).enumerate() {
            assert_eq!(locos(a), format!("[{}]", locos(b)), "{name} unit {i}");
        }
    }
}

#[test]
fn eleven_languages_and_their_encodings() {
    let langs = ["en", "de", "fr", "es", "pt", "ru", "tr", "ar", "ja", "zh", "ko"];
    for l in langs {
        let u = units(file(&format!("lang_{l}")));
        assert_eq!(u.len(), 4, "{l}");
        assert!(u[1].contains("\u{27E6}g1\u{27E7}") && u[1].contains("\u{27E6}x2\u{27E7}"), "{l}: {}", u[1]);
        assert_eq!(styles(file(&format!("lang_{l}")))[3], "attr:alt");
    }
    // the same text read from every encoding equals the UTF-8 file
    for (enc_name, utf8, expected) in [
        ("lang_ar_utf16le", "lang_ar", "utf-16le"),
        ("lang_ar_cp1256", "lang_ar", "windows-1256"),
        ("lang_ko_utf16be", "lang_ko", "utf-16be"),
        ("lang_ko_euckr", "lang_ko", "euc-kr"),
        ("lang_fr_cp1252", "lang_fr", "windows-1252"),
        ("lang_de_latin1", "lang_de", "iso-8859-1"),
        ("lang_ru_cp1251", "lang_ru", "windows-1251"),
        ("lang_ru_koi8r", "lang_ru", "koi8-r"),
        ("lang_tr_cp1254", "lang_tr", "windows-1254"),
        ("lang_ja_shiftjis", "lang_ja", "shift_jis"),
        ("lang_ja_eucjp", "lang_ja", "euc-jp"),
        ("lang_zh_gbk", "lang_zh", "gbk"),
        ("lang_zh_gb2312", "lang_zh", "gbk"),
        ("lang_en_utf8bom_crlf", "lang_en", "utf-8-bom"),
        ("lang_de_crlf", "lang_de", "utf-8"),
    ] {
        let data = file(enc_name);
        assert_eq!(encoding(&extract(data)), expected, "{enc_name}");
        assert_eq!(units(data), units(file(utf8)), "{enc_name}");
    }
}

#[test]
fn legacy_files_translated_into_another_script_stay_readable() {
    // a Latin-1 file given Japanese text: the characters become references, the file stays Latin-1 and valid
    let data = file("lang_de_latin1");
    let out = translate(data, &[(0, "\u{3088}\u{3046}\u{3053}\u{305d}"), (2, "\u{4fdd}\u{5b58}")]);
    assert_eq!(units(&out)[0], "\u{3088}\u{3046}\u{3053}\u{305d}");
    assert_eq!(units(&out)[2], "\u{4fdd}\u{5b58}");
    assert_eq!(encoding(&extract(&out)), "iso-8859-1");
    // Arabic into a Shift_JIS file
    let out = translate(file("lang_ja_shiftjis"), &[(2, "\u{62d}\u{641}\u{638}")]);
    assert_eq!(units(&out)[2], "\u{62d}\u{641}\u{638}");
    assert_eq!(encoding(&extract(&out)), "shift_jis");
}

#[test]
fn euro_in_a_gbk_file_is_a_character_reference() {
    for name in ["lang_zh_gbk", "lang_zh_gb2312"] {
        let out = translate(file(name), &[(0, "\u{4ef7}\u{683c} \u{20ac}5")]);
        assert!(out.windows(7).any(|w| w == b"&#8364;"), "{name}");
        assert!(!out.windows(2).any(|w| w == [0xA2, 0xE3]) && !out.contains(&0x80), "{name}");
        assert_eq!(units(&out)[0], "\u{4ef7}\u{683c} \u{20ac}5");
    }
}

#[test]
fn us_ascii_declaration_gets_character_references() {
    let data = b"<?xml version=\"1.0\" encoding=\"us-ascii\"?>\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><p>Save</p></xsl:template></xsl:stylesheet>\n";
    assert_identity(data);
    let out = translate(data, &[(0, "Caf\u{e9} \u{65e5}\u{672c}")]);
    assert!(out.is_ascii(), "{}", String::from_utf8_lossy(&out));
    assert!(s(&out).contains(">Caf&#233; &#26085;&#26412;<"));
    assert_eq!(units(&out), ["Caf\u{e9} \u{65e5}\u{672c}"]);
}

#[test]
fn undeclared_non_utf8_is_refused() {
    // LocOS and the XML specification refuse a file that is neither UTF-8 nor declares its encoding
    let mut data = b"<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><p>Caf".to_vec();
    data.push(0xE9);
    data.extend_from_slice(b"</p></xsl:template></xsl:stylesheet>");
    assert!(matches!(Xslt.extract(&data, &opts()), Err(Error::Encoding(_))));
    // the same bytes under a declaration that says UTF-8 are not valid either
    let mut decl = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_vec();
    decl.extend_from_slice(&data);
    assert!(matches!(Xslt.extract(&decl, &opts()), Err(Error::Encoding(_))));
    // a doubled byte order mark is not well-formed
    let mut bom2 = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    bom2.extend_from_slice(file("lang_en"));
    assert!(Xslt.extract(&bom2, &opts()).is_err());
    // UTF-16 declared on UTF-8 bytes
    let wide = file("lang_en").to_vec();
    let wide = String::from_utf8(wide).unwrap().replace("encoding=\"UTF-8\"", "encoding=\"UTF-16\"");
    assert!(matches!(Xslt.extract(wide.as_bytes(), &opts()), Err(Error::Encoding(_))));
}

#[test]
fn edge_file() {
    let data = file("edge");
    assert_eq!(
        units(data),
        [
            "Click \u{27E6}g1\u{27E7}here\u{27E6}/g1\u{27E7}\u{27E6}x2\u{27E7}now\u{27E6}x3\u{27E7} and \u{27E6}x4\u{27E7} then \u{27E6}x5\u{27E7} done.",
            "Tab\tand   many   spaces across lines",
            "Navigation",
            "Go",
            "Element text",
            "Item \u{27E6}x1\u{27E7}",
            "5 < 6 && 7 > 3 \u{a9} 2026",
            "12345",
            "It's a \"box\"",
            "Open & read",
            "{no} \u{27E6}x1\u{27E7}",
        ]
    );
    // the nested title sits inside the first unit's `<a>` tag; both units are written, the rest is byte for byte
    let out = s(&translate(
        data,
        &[(0, "Klick \u{27E6}g1\u{27E7}hier\u{27E6}/g1\u{27E7}\u{27E6}x2\u{27E7}jetzt\u{27E6}x3\u{27E7} und \u{27E6}x4\u{27E7} dann \u{27E6}x5\u{27E7} fertig."), (8, "Es ist 'ne \"Box\""), (9, "\u{d6}ffnen & lesen"), (10, "{kein} \u{27E6}x1\u{27E7}")],
    ));
    assert!(out.starts_with("<?xml version='1.0' encoding='utf-8' standalone=\"yes\"?>\n<?pi data?>\n<!DOCTYPE xsl:stylesheet [\n  <!ENTITY nbsp \"&#160;\">"), "{out}");
    assert!(out.contains("<div class = 'box'  title = 'Es ist &apos;ne \"Box\"'>"), "{out}");
    assert!(out.contains("<p>Klick <a href=\"{@href}\" title=\"\u{d6}ffnen &amp; lesen\">hier</a>&nbsp;jetzt<!-- inline note --> und <![CDATA[CDATA <text> kept]]> dann &reg; fertig.</p>"), "{out}");
    assert!(out.contains("placeholder=\"{{kein}} {$x}\""), "{out}");
    assert!(out.contains("<!-- the main page -->") && out.contains("<p its:translate=\"no\">DO-NOT-TRANSLATE</p>"));
    assert!(out.contains("<xsl:with-param name=\"x\">Param text</xsl:with-param>") && out.contains("<xsl:message>Never <b>shown</b></xsl:message>"));
    assert!(out.contains("<p>   </p>"));
    well_formed(out.as_bytes());
    // the tab inside the paragraph and the three spaces are display only: an echo keeps them
    assert_identity(data);
}

#[test]
fn language_stamp_on_html_lang() {
    let data = file("lang_de");
    let with = |lang: &str| Xslt.extract(data, &opts().with_target_lang(lang)).unwrap();
    // nothing translated: no stamp
    assert_eq!(Xslt.merge(&with("fr_fr")).unwrap(), data);
    let mut d = with("fr_fr");
    set_locos(&mut d, 0, "Bienvenue");
    let out = s(&Xslt.merge(&d).unwrap());
    assert!(out.contains("<html lang=\"fr-FR\">") && out.contains("<h1>Bienvenue</h1>"), "{out}");
    // a bad tag stamps nothing; an expression in the attribute is never touched
    let mut d = with("fr fr<>");
    set_locos(&mut d, 0, "x");
    assert!(s(&Xslt.merge(&d).unwrap()).contains("<html lang=\"en\">"));
    let src = b"<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><HTML lang=\"{$l}\" xml:lang='en'><p>Hi</p></HTML><p xml:lang=\"en\">Other</p></xsl:template></xsl:stylesheet>";
    let mut d = Xslt.extract(src, &opts().with_target_lang("ja")).unwrap();
    set_locos(&mut d, 0, "Hallo");
    let out = s(&Xslt.merge(&d).unwrap());
    assert!(out.contains("<HTML lang=\"{$l}\" xml:lang='ja'><p>Hallo</p></HTML><p xml:lang=\"en\">Other</p>"), "{out}");
}

#[test]
fn sniff() {
    assert!(Xslt.sniff(file("locos_basic")));
    assert!(Xslt.sniff(file("lang_ar_utf16le")));
    assert!(Xslt.sniff(file("lang_ko_utf16be")));
    assert!(!Xslt.sniff(b"<html><body>hi</body></html>"));
    assert!(!Xslt.sniff(b"<root xmlns=\"urn:x\"/>"));
    assert!(!Xslt.sniff(b"key=value\n"));
    assert_eq!(Xslt.extensions(), ["xsl", "xslt"]);
    assert_eq!(Xslt.id(), "xslt");
}

#[test]
fn units_carry_context() {
    let d = extract(file("locos_basic"));
    assert_eq!(d.units[0].id, "u1");
    assert_eq!(d.units[0].context.kind.as_deref(), Some("text"));
    assert_eq!(d.units[11].context.kind.as_deref(), Some("title"));
    assert_eq!(d.units[11].context.style.as_deref(), Some("attr:title"));
    assert_eq!(d.units[0].context.location.as_deref(), Some("line 15"));
}

#[test]
fn carriage_return_from_a_character_reference_survives_a_translation() {
    // `&#13;&#10;` is a CR LF in the text; a translation that keeps it must write the CR as a reference again
    let data = b"<?xml version=\"1.0\"?>\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">\n<xsl:template match=\"/\"><xsl:text>&#13;&#10;</xsl:text><p>Hello</p></xsl:template>\n</xsl:stylesheet>\n";
    let d = extract(data);
    let i = d.units.iter().position(|u| u.source().iter().any(|x| matches!(x, Inline::Text(t) if t.contains('\r')))).expect("a unit with the CR");
    let mut d2 = d.clone();
    d2.units[i].segments[0].target = vec![Inline::text("Zq\r\nqZ")];
    let out = s(&Xslt.merge(&d2).unwrap());
    assert!(out.contains("Zq&#13;\nqZ"), "{out}");
    let back = extract(out.as_bytes());
    assert!(back.units.iter().any(|u| u.source().iter().any(|x| matches!(x, Inline::Text(t) if t == "Zq\r\nqZ"))), "{out}");
}

#[test]
fn entity_from_an_external_subset_is_a_protected_token() {
    // the DOCTYPE points at an external subset that defines `&pfx;`; it is not read, the reference is kept as it is
    let data = b"<?xml version=\"1.0\"?>\n<!DOCTYPE xsl:stylesheet SYSTEM \"prefixes.ent\">\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">\n<xsl:template match=\"/\"><p>See &pfx; for details</p></xsl:template>\n</xsl:stylesheet>\n";
    let d = extract(data);
    assert_eq!(units(data), ["See \u{27E6}x1\u{27E7} for details"]);
    assert_eq!(Xslt.merge(&d).unwrap(), data);
    let out = s(&translate(data, &[(0, "Siehe \u{27E6}x1\u{27E7} f\u{fc}r Details")]));
    assert!(out.contains("<p>Siehe &pfx; f\u{fc}r Details</p>"), "{out}");
    // without an external subset the same reference is still an error
    let bad = b"<?xml version=\"1.0\"?>\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">\n<xsl:template match=\"/\"><p>See &pfx; now</p></xsl:template>\n</xsl:stylesheet>\n";
    assert!(Xslt.extract(bad, &opts()).is_err());
}

#[test]
fn carriage_return_reference_in_a_crlf_file_keeps_the_file_line_breaks() {
    // a CRLF file: the CR of `&#13;` is written back as a reference and the translation's other line breaks as CRLF
    let data = b"<?xml version=\"1.0\"?>\r\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">\r\n<xsl:template match=\"/\"><xsl:text>One&#13;\r\nTwo</xsl:text></xsl:template>\r\n</xsl:stylesheet>\r\n";
    let d = extract(data);
    assert_eq!(Xslt.merge(&d).unwrap(), data);
    let i = d.units.iter().position(|u| u.source().iter().any(|x| matches!(x, Inline::Text(t) if t.contains('\r')))).expect("a unit with the CR");
    let mut d2 = d.clone();
    d2.units[i].segments[0].target = vec![Inline::text("Eins\r\nZwei\nDrei")];
    let out = s(&Xslt.merge(&d2).unwrap());
    assert!(out.contains("Eins&#13;\r\nZwei\r\nDrei"), "{out:?}");
    assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
}
