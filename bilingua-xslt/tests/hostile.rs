//! Hostile input: typed errors and no panics, over random bytes, random structured XSLT, truncated and
//! byte-mutated fixtures, entity bombs, XXE, deep nesting and doubled byte order marks.

mod common;

use bilingua_model::*;
use bilingua_xslt::{Limits, Xslt};
use common::*;
use proptest::prelude::*;
use proptest::strategy::ValueTree;

const SEEDS: [&[u8]; 8] = [
    include_bytes!("data/locos_basic.xsl"),
    include_bytes!("data/locos_xslt3.xsl"),
    include_bytes!("data/locos_prefix_doe.xsl"),
    include_bytes!("data/edge.xsl"),
    include_bytes!("data/lang_ja_shiftjis.xsl"),
    include_bytes!("data/lang_ar_utf16le.xsl"),
    include_bytes!("data/lang_en_utf8bom_crlf.xsl"),
    include_bytes!("data/locos_simplified.xsl"),
];

/// Extract, and when it works: untouched merge is the same bytes; a pseudo-translated merge reads back with the
/// same unit count and a translated merge with a target language is still readable.
fn check(data: &[u8]) {
    let _ = Xslt.sniff(data);
    let o = ExtractOptions { target_lang: Some("de-DE".into()), ..opts() };
    let Ok(d) = Xslt.extract(data, &o) else { return };
    assert!(Xslt.merge(&d).expect("merge") == data, "untouched merge");
    let mut e = d.clone();
    for u in &mut e.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = Xslt.merge(&e).expect("pseudo merge");
    let back = Xslt.extract(&out, &o).expect("re-extract");
    assert_eq!(back.units.len(), d.units.len(), "unit count after translation");
}

fn soup() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        Just("<xsl:stylesheet version=\"3.0\" expand-text=\"yes\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">".to_string()),
        Just("</xsl:stylesheet>".to_string()),
        Just("<xsl:template match=\"/\">".to_string()),
        Just("</xsl:template>".to_string()),
        Just("<xsl:text>".to_string()),
        Just("</xsl:text>".to_string()),
        Just("<xsl:value-of select=\"a/b\"/>".to_string()),
        Just("<xsl:attribute name=\"title\">".to_string()),
        Just("</xsl:attribute>".to_string()),
        Just("<xsl:if test=\"x &gt; 1\">".to_string()),
        Just("</xsl:if>".to_string()),
        Just("<p>".to_string()),
        Just("</p>".to_string()),
        Just("<b>".to_string()),
        Just("</b>".to_string()),
        Just("<img alt=\"Photo {@n} {{x}}\" title='a'/>".to_string()),
        Just("<a href=\"{@h}\" title=\"Go { 'a}' }\">".to_string()),
        Just("</a>".to_string()),
        Just("<html lang=\"en\" xml:lang=\"en\">".to_string()),
        Just("<script>".to_string()),
        Just("</script>".to_string()),
        Just("<!--".to_string()),
        Just("-->".to_string()),
        Just("<![CDATA[".to_string()),
        Just("]]>".to_string()),
        Just("<?pi x?>".to_string()),
        Just("<?xml version=\"1.0\"?>".to_string()),
        Just("<!DOCTYPE a [<!ENTITY e \"x\">]>".to_string()),
        Just("<!DOCTYPE a [<!ENTITY e \"&#160;\">]>".to_string()),
        Just("&amp;".to_string()),
        Just("&#x41;".to_string()),
        Just("&#0;".to_string()),
        Just("&e;".to_string()),
        Just("&nbsp;".to_string()),
        Just("&".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just("\"".to_string()),
        Just("'".to_string()),
        Just("=".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("{{".to_string()),
        Just("{$x}".to_string()),
        Just(" ".to_string()),
        Just("\r\n".to_string()),
        Just("Hello".to_string()),
        Just("caf\u{e9}".to_string()),
        Just("\u{65e5}\u{672c}".to_string()),
        Just("\u{645}\u{631}".to_string()),
        Just("\u{feff}".to_string()),
        Just("\u{1f600}".to_string()),
    ];
    prop::collection::vec(piece, 0..60).prop_map(|v| v.concat().into_bytes())
}

fn mutate(seed: &[u8], edits: &[(usize, u8, u8)]) -> Vec<u8> {
    let mut v = seed.to_vec();
    for (pos, kind, byte) in edits {
        if v.is_empty() {
            break;
        }
        let p = pos % v.len();
        match kind % 3 {
            0 => v[p] = *byte,
            1 => v.insert(p, *byte),
            _ => {
                v.remove(p);
            }
        }
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        check(&data);
    }

    #[test]
    fn random_xslt_soup_never_panics(data in soup()) {
        check(&data);
    }

    #[test]
    fn mutated_fixtures_never_panic(seed in 0usize..SEEDS.len(), edits in prop::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..6)) {
        check(&mutate(SEEDS[seed], &edits));
    }

    #[test]
    fn soup_inside_a_template_never_panics(body in soup()) {
        let mut data = b"<xsl:stylesheet version=\"3.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">".to_vec();
        data.extend_from_slice(&body);
        data.extend_from_slice(b"</xsl:template></xsl:stylesheet>");
        check(&data);
    }
}

#[test]
fn every_truncation_of_every_fixture_is_handled() {
    for seed in SEEDS {
        // a char boundary is not required: cut anywhere
        let step = (seed.len() / 400).max(1);
        for n in (0..seed.len()).step_by(step) {
            check(&seed[..n]);
        }
    }
}

#[test]
fn entity_bombs_and_xxe_are_refused() {
    let wrap = |dtd: &str, body: &str| {
        format!("<?xml version=\"1.0\"?><!DOCTYPE xsl:stylesheet [{dtd}]><xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><p>{body}</p></xsl:template></xsl:stylesheet>")
    };
    let lol9 = (0..9).map(|i| format!("<!ENTITY l{} \"{}\">", i + 1, format!("&l{i};").repeat(10))).collect::<String>();
    for (dtd, body) in [
        (format!("<!ENTITY l0 \"lol\">{lol9}"), "&l9;"),
        ("<!ENTITY x SYSTEM \"file:///etc/passwd\">".to_string(), "&x;"),
        ("<!ENTITY x SYSTEM \"http://localhost/evil\">".to_string(), "&x;"),
        ("<!ENTITY % p SYSTEM \"http://localhost/evil.dtd\">%p;".to_string(), "x"),
        ("<!ENTITY a \"&#38;#38;\">".to_string(), "&a;"),
        ("<!ENTITY a \"a very long replacement text\">".to_string(), "&a;"),
        ("<!ELEMENT a (b)>".to_string(), "x"),
        ("<!ENTITY a \"<b/>\">".to_string(), "&a;"),
    ] {
        let e = Xslt.extract(wrap(&dtd, body).as_bytes(), &opts());
        assert!(e.is_err(), "{dtd}");
    }
    // a simple character entity is accepted and stays a protected token
    let ok = wrap("<!ENTITY nbsp \"&#160;\">", "A&nbsp;B");
    let d = extract(ok.as_bytes());
    assert_eq!(locos(&d.units[0]), "A\u{27E6}x1\u{27E7}B");
    assert_identity(ok.as_bytes());
    // an external subset is never loaded
    let ext = "<!DOCTYPE xsl:stylesheet SYSTEM \"http://localhost/evil.dtd\"><xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><p>Hi</p></xsl:template></xsl:stylesheet>";
    assert_eq!(units(ext.as_bytes()), ["Hi"]);
}

#[test]
fn deep_and_wide_files_return_typed_errors() {
    for depth in [199usize, 201, 257, 100_000] {
        let deep = format!(
            "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">{}x{}</xsl:template></xsl:stylesheet>",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        );
        let r = Xslt.extract(deep.as_bytes(), &opts());
        if depth > 197 {
            assert!(matches!(r, Err(Error::Limit(_))), "depth {depth}");
        } else {
            assert!(r.is_ok(), "depth {depth}");
        }
    }
    // unclosed deep nesting
    assert!(Xslt.extract("<a>".repeat(100_000).as_bytes(), &opts()).is_err());
    // an element flood against a small limit
    let small = Limits { max_xml_elements: 50, ..Limits::default() };
    let flood = format!("<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">{}</xsl:template></xsl:stylesheet>", "<br/>".repeat(500));
    assert!(matches!(Xslt.extract_with(flood.as_bytes(), &opts(), &small), Err(Error::Limit(_))));
    // input size
    let tiny = Limits { max_input_bytes: 100, ..Limits::default() };
    assert!(matches!(Xslt.extract_with(include_bytes!("data/locos_basic.xsl"), &opts(), &tiny), Err(Error::Limit(_))));
}

#[test]
fn malformed_files_return_typed_errors() {
    let wrap = |body: &str| format!("<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">{body}</xsl:stylesheet>");
    for bad in [
        wrap("<xsl:template match=\"/\"><p>unclosed</xsl:template>"),
        wrap("<xsl:template match=\"/\"><p>x</q></xsl:template>"),
        wrap("<p a=\"1\" a=\"2\">x</p>"),
        wrap("<p a=1>x</p>"),
        wrap("<p a=\"<\">x</p>"),
        wrap("<p>]]></p>"),
        wrap("<p>&#99999999999;</p>"),
        wrap("<p>&#xD800;</p>"),
        wrap("<p>&bad;</p>"),
        wrap("<p>& </p>"),
        wrap("<p><!-- a -- b --></p>"),
        wrap("<q:p>x</q:p>"),
        wrap("<p q:a=\"1\">x</p>"),
        wrap("<!ELEMENT x>"),
        wrap("<p>\u{1}</p>"),
        wrap("<p>x</p>") + "<p>second root</p>",
        wrap("<p>x</p>") + "text after the root",
        "text before the root".to_string() + &wrap("<p>x</p>"),
        wrap("<?xml version=\"1.0\"?><p>x</p>"),
        String::new(),
        "<".to_string(),
    ] {
        assert!(matches!(Xslt.extract(bad.as_bytes(), &opts()), Err(Error::Parse { .. } | Error::Limit(_))), "{bad}");
    }
}

#[test]
fn doubled_bom_and_odd_boms() {
    let base = include_bytes!("data/locos_basic.xsl");
    let mut two = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    two.extend_from_slice(base);
    assert!(Xslt.extract(&two, &opts()).is_err());
    let mut one = vec![0xEF, 0xBB, 0xBF];
    one.extend_from_slice(base);
    assert_identity(&one);
    // a UTF-16 BOM followed by UTF-8 bytes
    let mut mixed = vec![0xFF, 0xFE];
    mixed.extend_from_slice(base);
    assert!(Xslt.extract(&mixed, &opts()).is_err());
    // an odd-length UTF-16 body
    let mut odd = vec![0xFF, 0xFE];
    odd.extend_from_slice(&[b'<', 0, b'a']);
    assert!(Xslt.extract(&odd, &opts()).is_err());
}

#[test]
fn a_skeleton_from_another_crate_or_a_tampered_one_is_refused_not_panicked() {
    let mut d = extract(include_bytes!("data/locos_basic.xsl"));
    d.skeleton = Skeleton::text(vec![SkelPart::Unit(999)], "utf-8", bilingua_model::ParserInfo::new("bilingua-xslt", env!("CARGO_PKG_VERSION"), 1));
    assert!(Xslt.merge(&d).is_err());
    let mut d = extract(include_bytes!("data/locos_basic.xsl"));
    d.skeleton = Skeleton::text(vec![], "utf-8", bilingua_model::ParserInfo::new("bilingua-po", "0.1.0", 1));
    assert!(Xslt.merge(&d).is_err());
    // damaged hidden codes never panic
    let mut d = extract(include_bytes!("data/edge.xsl"));
    for u in &mut d.units {
        for (k, c) in u.codes.iter_mut() {
            if k.starts_with('~') {
                c.standalone = Some("9999-1-77;x-y-z;5-3-0;;".into());
            }
        }
        u.segments[0].target = vec![Inline::text("changed")];
    }
    let _ = Xslt.merge(&d);
    // nest lists that point back at their own unit (or at a text unit) never recurse, translated or not
    for translated in [false, true] {
        let mut d = extract(include_bytes!("data/edge.xsl"));
        let n = d.units.len();
        for (i, u) in d.units.iter_mut().enumerate() {
            let all: Vec<String> = (0..n).map(|j| format!("0-1-{j}")).collect();
            u.codes.insert("~nest".into(), CodeData { standalone: Some(format!("0-1-{i};{}", all.join(";"))), ..Default::default() });
            for id in 1..4 {
                u.codes.insert(format!("~n{id}"), CodeData { standalone: Some(all.join(";")), ..Default::default() });
            }
            if translated {
                let mut t = u.source();
                t.push(Inline::text(" x"));
                u.segments[0].target = t;
            }
        }
        let _ = Xslt.merge(&d);
    }
}

/// A balanced random stylesheet body: the interesting walks (inline runs, nested attribute units, value
/// templates, instructions inside sentences) need well-formed input, which byte soup rarely makes.
fn tree() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        Just("Hello world".to_string()),
        Just("  spaced \n text  ".to_string()),
        Just("caf\u{e9} \u{65e5}\u{672c}".to_string()),
        Just("&amp; &lt; &#169; &nbsp;".to_string()),
        Just("{$a} and {{b}}".to_string()),
        Just("{ 'x}' }".to_string()),
        Just("<xsl:value-of select=\"a\"/>".to_string()),
        Just("<xsl:apply-templates/>".to_string()),
        Just("<xsl:number/>".to_string()),
        Just("<img alt=\"Pic {@n}\" src=\"{@s}\"/>".to_string()),
        Just("<br/>".to_string()),
        Just("<!-- note -->".to_string()),
        Just("<![CDATA[raw <x>]]>".to_string()),
        Just("<?pi data?>".to_string()),
        Just("<xsl:text>t&amp;t</xsl:text>".to_string()),
        Just("<xsl:attribute name=\"title\">Tip</xsl:attribute>".to_string()),
        Just("<p translate=\"no\">KEEP</p>".to_string()),
        Just("<script>var a = '<xsl:value-of select=\"q\"/>';</script>".to_string()),
        Just("".to_string()),
    ];
    leaf.prop_recursive(5, 48, 6, |inner| {
        let kids = prop::collection::vec(inner, 0..5).prop_map(|v| v.concat());
        (prop_oneof![
            Just(("<p>", "</p>")),
            Just(("<b>", "</b>")),
            Just(("<a href=\"{@h}\" title=\"Go {{x}} now\">", "</a>")),
            Just(("<span class=\"c\" expand-text=\"yes\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">", "</span>")),
            Just(("<xsl:if test=\"a\">", "</xsl:if>")),
            Just(("<xsl:for-each select=\"x\">", "</xsl:for-each>")),
            Just(("<xsl:choose><xsl:when test=\"a\">", "</xsl:when><xsl:otherwise>no</xsl:otherwise></xsl:choose>")),
            Just(("<xsl:variable name=\"v\">", "</xsl:variable>")),
            Just(("<xsl:message>", "</xsl:message>")),
            Just(("<td xml:space=\"preserve\">", "</td>")),
            Just(("<xsl:text>", "</xsl:text>")),
            Just(("<xsl:element name=\"e\">", "</xsl:element>")),
        ], kids)
            .prop_map(|((o, c), k)| format!("{o}{k}{c}"))
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 600, ..ProptestConfig::default() })]

    #[test]
    fn balanced_random_stylesheets_round_trip(body in tree(), tvt in any::<bool>(), crlf in any::<bool>()) {
        let attr = if tvt { " expand-text=\"yes\"" } else { "" };
        let mut src = format!("<xsl:stylesheet version=\"3.0\"{attr} xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">\n{body}\n</xsl:template></xsl:stylesheet>\n");
        if crlf {
            src = src.replace('\n', "\r\n");
        }
        // these are well-formed apart from the odd unbound or unbalanced case the generator cannot rule out
        check(src.as_bytes());
        if let Ok(d) = Xslt.extract(src.as_bytes(), &opts()) {
            // echoing every unit with LocOS text gives the original back
            let echo = translate_all(src.as_bytes(), |t| t.to_string());
            prop_assert_eq!(echo, src.as_bytes().to_vec());
            // dropping every tag from a translation never breaks the file
            let mut e = d.clone();
            for u in &mut e.units {
                u.segments[0].target = vec![Inline::text("Text")];
            }
            let out = Xslt.merge(&e).expect("merge");
            // a dropped pair takes the attribute units inside its opening tag with it, nothing else goes
            let n = Xslt.extract(&out, &opts()).expect("readable").units.len();
            prop_assert!(n <= d.units.len());
        }
    }
}

#[test]
fn the_generators_reach_the_interesting_paths() {
    // a guard on the property tests: most balanced stylesheets and many mutated fixtures must actually extract
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let (mut ok, mut units) = (0, 0);
    for _ in 0..200 {
        let body = tree().new_tree(&mut runner).unwrap().current();
        let src = format!("<xsl:stylesheet version=\"3.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\">{body}</xsl:template></xsl:stylesheet>");
        if let Ok(d) = Xslt.extract(src.as_bytes(), &opts()) {
            ok += 1;
            units += d.units.len();
        }
    }
    assert!(ok > 150 && units > 150, "ok {ok}, units {units}");
    let mut readable = 0;
    for i in 0..400usize {
        let edit = [(i * 37, (i % 3) as u8, b'a' + (i % 26) as u8)];
        if Xslt.extract(&mutate(SEEDS[i % SEEDS.len()], &edit), &opts()).is_ok() {
            readable += 1;
        }
    }
    assert!(readable > 100, "readable {readable}");
}

/// A translation that starts or ends in text, next to a whitespace-only node the stylesheet strips, must not join
/// that node: the units stay apart on re-read and the stripped whitespace stays stripped.
#[test]
fn edge_text_next_to_stripped_whitespace_keeps_units_apart() {
    let src = b"<t:stylesheet version=\"2.0\" xmlns:t=\"http://www.w3.org/1999/XSL/Transform\"><t:template match=\"/\">
  <b>Bold</b> <i>Italic</i>!
  <p>Para</p>
</t:template></t:stylesheet>";
    check(src);
    let o = ExtractOptions { target_lang: Some("de-DE".into()), ..opts() };
    let d = Xslt.extract(src, &o).unwrap();
    let mut e = d.clone();
    for u in &mut e.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = String::from_utf8(Xslt.merge(&e).unwrap()).unwrap();
    assert!(out.contains("</b> <t:text>[</t:text><i>Italic</i>"), "{out}");
    assert!(out.contains("</i>!]"), "{out}");
    assert_eq!(Xslt.extract(out.as_bytes(), &o).unwrap().units.len(), d.units.len());
}
