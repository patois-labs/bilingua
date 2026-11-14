//! Port of the Localization OS suite `tests/verify_xslt_filter.py`: 11 of its 13 cases. The other two are
//! `test_registration` (the LocOS registry, upload list, MIME and Graph maps) and `test_api_and_cli_round_trip`
//! (the LocOS web API and CLI); the facade contract test and the oracle cover their Bilingua counterparts.
//! Same inputs, same expected bytes. The fixtures are the suite's own strings (`tests/data/locos_*.xsl`).

mod common;

use bilingua_model::*;
use bilingua_xslt::Xslt;
use common::*;

const BASIC: &[u8] = include_bytes!("data/locos_basic.xsl");
const XSLT3: &[u8] = include_bytes!("data/locos_xslt3.xsl");
const SIMPLIFIED: &[u8] = include_bytes!("data/locos_simplified.xsl");

const BASIC_UNITS: [&str; 13] = [
    "Welcome to the \u{27E6}g1\u{27E7}shop\u{27E6}/g1\u{27E7}, \u{27E6}x2\u{27E7}!",
    "Price:\u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7} {0} %1$s {{x}}",
    "You have \u{27E6}x1\u{27E7} items.",
    "Tooltip text",
    "Body",
    "A cell",
    "B cell",
    "Yes branch",
    "No branch",
    "Thanks for reading",
    "\u{27E6}x1\u{27E7} 2026 Example",
    "Main \u{27E6}x1\u{27E7} heading",
    "Photo of {the} product",
];

fn refused(data: &[u8], needle: &str) -> Error {
    let e = Xslt.extract(data, &opts()).expect_err("expected a refusal");
    assert!(e.to_string().to_lowercase().contains(&needle.to_lowercase()), "{needle}: {e}");
    e
}

fn is_limit(e: &Error, kind: LimitKind) -> bool {
    matches!(e, Error::Limit(l) if l.kind == kind)
}

#[test]
fn fixtures_are_the_suite_fixtures() {
    assert!(BASIC.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xsl:stylesheet [\n  <!ENTITY nbsp"));
    assert!(XSLT3.starts_with(b"<xsl:stylesheet version=\"3.0\" expand-text=\"yes\""));
    assert!(SIMPLIFIED.starts_with(b"<html xsl:version=\"1.0\""));
}

#[test]
fn basic_units_and_identity() {
    assert_eq!(units(BASIC), BASIC_UNITS);
    let st = styles(BASIC);
    assert_eq!(&st[11..], ["attr:title", "attr:alt"]);
    assert!(st[..11].iter().all(String::is_empty), "{st:?}");
    assert_identity(BASIC);
}

#[test]
fn translated_round_trip() {
    let out = translate_all(BASIC, |t| format!("<{t}> & co"));
    let text = s(&out);
    assert!(text.contains("title=\"&lt;Main {@id} heading> &amp; co\""), "{text}");
    assert!(text.contains("&lt;Welcome to the <b>shop</b>, <xsl:value-of select=\"user/name\"/>!&gt; &amp; co</h1>"), "{text}");
    assert!(text.contains("&lt;Price:&nbsp;<xsl:value-of select=\"price\"/> {0} %1$s {{x}}&gt; &amp; co"), "{text}");
    assert!(text.contains("alt=\"&lt;Photo of {{the}} product> &amp; co\""), "{text}");
    // never touched: instructions, XPath, code text, messages, translate="no"
    for keep in [
        "<xsl:variable name=\"mode\">compact</xsl:variable>",
        "<xsl:param name=\"title\" select=\"'Report'\"/>",
        "<xsl:if test=\"count(item) &gt; 0\">",
        "<xsl:message terminate=\"no\">Debug only</xsl:message>",
        "<xsl:comment>Generated file</xsl:comment>",
        "<p translate=\"no\">ACME-42</p>",
        "<xsl:attribute name=\"class\">big</xsl:attribute>",
        "<xsl:with-param name=\"note\">raw-code</xsl:with-param>",
        "<img src=\"{@src}\"",
        "<html lang=\"en\">",
    ] {
        assert!(text.contains(keep), "{keep}");
    }
    assert!(text.contains("<xsl:otherwise><xsl:text>&lt;No branch&gt; &amp; co</xsl:text></xsl:otherwise>"));
    assert!(text.contains("<footer>&lt;&copy; 2026 Example&gt; &amp; co</footer>"));
    // the rebuilt file parses back to the same number of units
    assert_eq!(well_formed(&out).units.len(), BASIC_UNITS.len());
}

#[test]
fn value_templates() {
    let texts = units(XSLT3);
    assert_eq!(
        texts,
        [
            "Total: \u{27E6}x1\u{27E7} for \u{27E6}x2\u{27E7} customer, {literal}",
            "Raw {braces} stay",
            "Line \u{27E6}x1\u{27E7} done",
            "x",
            "\u{27E6}x1\u{27E7} items",
        ]
    );
    assert_identity(XSLT3);
    let out = s(&translate(
        XSLT3,
        &[
            (0, "Gesamt {x}: \u{27E6}x1\u{27E7} / \u{27E6}x2\u{27E7}"),
            (1, "Roh {x}"),
            (2, "Zeile \u{27E6}x1\u{27E7} {ok}"),
            (4, "\u{27E6}x1\u{27E7} {Stk}"),
        ],
    ));
    assert!(out.contains("<p>Gesamt {{x}}: {sum(line/@price)} / {map{'a':1}?a}</p>"), "{out}");
    assert!(out.contains("<p xsl:expand-text=\"no\">Roh {x}</p>"), "{out}");
    assert!(out.contains("<xsl:text>Zeile {position()} {{ok}}</xsl:text>"), "{out}");
    assert!(out.contains("title=\"{concat('a}', @x)} {{Stk}}\""), "{out}");
    well_formed(out.as_bytes());
}

#[test]
fn simplified_and_inline_attr_inside_text_unit() {
    let texts = units(SIMPLIFIED);
    assert_eq!(texts, ["Expense Report Summary", "Total Amount: \u{27E6}x1\u{27E7}", "\u{27E6}x1\u{27E7} Inline image here", "Company logo"]);
    let mut d = Xslt.extract(SIMPLIFIED, &opts().with_target_lang("fr-FR")).unwrap();
    for i in 0..d.units.len() {
        let t = format!("[fr] {}", locos(&d.units[i]));
        set_locos(&mut d, i, &t);
    }
    let out = s(&Xslt.merge(&d).unwrap());
    assert!(out.contains("<p>[fr] <img src=\"a.png\" alt=\"[fr] Company logo\"/> Inline image here</p>"), "{out}");
    assert!(out.contains("xml:lang=\"fr-FR\"") || out.contains("xml:lang=\"fr\""), "{out}");
    assert!(out.contains("xsl:version=\"1.0\""));
}

#[test]
fn token_damage() {
    let mut d = extract(BASIC);
    let t = |kind, id: &str| Inline::tag(id, kind, "");
    // a stray open/close/unknown token is dropped, an unclosed pair is closed, a dropped point is appended
    d.units[0].segments[0].target = vec![
        Inline::text("Hallo "),
        t(TagKind::Open, "1"),
        Inline::text("Laden "),
        t(TagKind::Standalone, "9"),
        t(TagKind::Close, "3"),
    ];
    // a repeated point is written once
    d.units[1].segments[0].target = vec![t(TagKind::Standalone, "2"), t(TagKind::Standalone, "2"), Inline::text(" Preis")];
    let out = s(&Xslt.merge(&d).unwrap());
    assert!(out.contains("<h1 title=\"Main {@id} heading\">Hallo <b>Laden </b><xsl:value-of select=\"user/name\"/></h1>"), "{out}");
    assert!(out.contains("<p><xsl:value-of select=\"price\"/> Preis&nbsp;</p>"), "{out}");
    well_formed(out.as_bytes());
}

#[test]
fn whitespace_and_crlf() {
    let data: &[u8] = include_bytes!("data/locos_whitespace_crlf.xsl");
    assert_eq!(units(data), ["First line second line", "keep   this\n  layout", "padded"]);
    assert_identity(data);
    let out = s(&translate(data, &[(1, "a\nb")]));
    assert!(out.contains("<pre xml:space=\"preserve\">a\r\nb</pre>"), "{out:?}");
    let out = s(&translate(data, &[(2, "voll")]));
    assert!(out.contains("<xsl:text>  voll  </xsl:text>"), "{out:?}");
}

#[test]
fn encodings_and_bom() {
    let bom: &[u8] = include_bytes!("data/locos_basic_utf8bom.xsl");
    let u16le: &[u8] = include_bytes!("data/locos_basic_utf16le.xsl");
    for raw in [bom, u16le] {
        assert_eq!(units(raw), BASIC_UNITS);
        assert_identity(raw);
        let out = translate(raw, &[(5, "Zelle \u{e4}")]);
        assert_eq!(out[..2], raw[..2]);
        assert_eq!(units(&out)[5], "Zelle \u{e4}");
    }
    let cp: &[u8] = include_bytes!("data/locos_basic_cp1252.xsl");
    let d = extract(cp);
    assert_eq!(locos(&d.units[5]), "Caf\u{e9} cell");
    assert!(matches!(&d.skeleton, Skeleton::Text { encoding, .. } if encoding.starts_with("windows-1252")));
    assert_identity(cp);
    let out = translate(cp, &[(5, "\u{4e2d}\u{6587} \u{e9}")]);
    assert!(out.windows(27).any(|w| w == b"<td>&#20013;&#25991; \xe9</td>"), "{}", String::from_utf8_lossy(&out));
}

#[test]
fn hostile_inputs() {
    refused(b"<xsl:stylesheet", "");
    refused(b"\x00\x01\x02binary", "");
    refused(include_bytes!("data/locos_hostile_lol.xsl"), "DOCTYPE");
    refused(include_bytes!("data/locos_hostile_xxe.xsl"), "DOCTYPE");
    refused(include_bytes!("data/locos_hostile_pe.xsl"), "DOCTYPE");
    // a comment that only MENTIONS a DOCTYPE is not a declaration; a hostile prolog DOCTYPE behind a leading
    // comment is still refused
    let mention: &[u8] = include_bytes!("data/locos_mention_doctype.xsl");
    assert_eq!(units(mention), ["Hello"]);
    assert_identity(mention);
    refused(include_bytes!("data/locos_hostile_hidden_doctype.xsl"), "DOCTYPE");
    refused(include_bytes!("data/locos_hostile_undefined_entity.xsl"), "well-formed");
    let deep = format!(
        "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">{}x{}</xsl:template></xsl:stylesheet>",
        "<div>".repeat(260),
        "</div>".repeat(260)
    );
    assert!(is_limit(&refused(deep.as_bytes(), ""), LimitKind::XmlDepth));
    // LocOS refuses a stylesheet with no literal text; Bilingua returns an empty document
    assert!(extract(include_bytes!("data/locos_no_text.xsl")).units.is_empty());
    let small = Limits::new().with_max_units(3);
    let many = format!(
        "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">{}</xsl:template></xsl:stylesheet>",
        (0..6).map(|i| format!("<p>Line {i}</p>")).collect::<String>()
    );
    assert!(matches!(Xslt.extract_with(many.as_bytes(), &opts(), &small), Err(Error::Limit(_))));
    assert!(matches!(Xslt.extract_with("<p>".repeat(200).as_bytes(), &opts(), &small), Err(Error::Limit(_))));
    let chars = Limits::new().with_max_unit_chars(1000);
    let big = format!(
        "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><p>{}</p></xsl:template></xsl:stylesheet>",
        "word ".repeat(400)
    );
    assert!(matches!(Xslt.extract_with(big.as_bytes(), &opts(), &chars), Err(Error::Limit(_))));
    let e = Xslt.extract(include_bytes!("data/locos_basic_ebcdic.xsl"), &opts()).unwrap_err();
    assert!(matches!(e, Error::Encoding(_)) && e.to_string().contains("encoding"), "{e}");
}

#[test]
fn illegal_xml_chars_in_translation() {
    let out = translate(BASIC, &[(5, "bad\u{1}char\u{b}next")]);
    assert!(out.windows(21).any(|w| w == b"<td>badchar\nnext</td>"), "{}", String::from_utf8_lossy(&out));
    well_formed(&out);
}

#[test]
fn prefix_doe_and_inline_runs() {
    let data: &[u8] = include_bytes!("data/locos_prefix_doe.xsl");
    assert_eq!(
        units(data),
        ["Bold", "Italic", "Read \u{27E6}g1\u{27E7}the guide\u{27E6}/g1\u{27E7} now.", "Saved label", "Not an instruction", "Open the guide"]
    );
    let out = s(&translate(data, &[(2, "Lies \u{27E6}g1\u{27E7}die Anleitung\u{27E6}/g1\u{27E7}."), (5, "Anleitung \u{f6}ffnen")]));
    assert!(out.contains("<p>Lies <a href=\"{@u}\" title=\"Anleitung \u{f6}ffnen\">die Anleitung</a>.</p>"), "{out}");
    assert!(out.contains("<t:text disable-output-escaping=\"yes\">&lt;br/&gt;</t:text>"));
}

#[test]
fn script_and_style_are_code() {
    let xsl: &[u8] = include_bytes!("data/locos_script_style.xsl");
    assert_eq!(units(xsl), ["Help", "Topic Title:"]);
    assert_identity(xsl);
    let out = translate(xsl, &[(0, "Hilfe"), (1, "Thema:")]);
    let o = s(&out);
    assert!(o.contains("<title>Hilfe</title>") && o.contains("<p>Thema:</p>"), "{o}");
    assert!(o.contains("color: #000}</style>") && o.contains("return x + \"Baggage\"; }</SCRIPT>"), "{o}");
    // <style> in another vocabulary's namespace is document text, still a unit
    assert_eq!(units(include_bytes!("data/locos_catalog.xsl")), ["Classic leather"]);
}
