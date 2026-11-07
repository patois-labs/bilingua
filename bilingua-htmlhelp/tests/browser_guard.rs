//! #676 (LocOS `html_guard`): a translated Name / Keyword / See Also value must never become markup in a browser's
//! reading of the sitemap. `tests/data/browser_cases.json` holds sources where the sitemap walker and an HTML5
//! tokenizer part ways (tags the walker does not accept with a `<param>` inside a quoted attribute, Unicode
//! whitespace, abrupt comments, CDATA, `</ x>`, raw text, SVG), a hostile translation for each, and what Localization
//! OS does with it (the build bytes, a refusal, or no units). Bilingua must do the same.
use bilingua_htmlhelp::HhcFilter;
use bilingua_model::*;

struct Case {
    name: String,
    source: String,
    translation: String,
    expect: String,
    out: Option<String>,
}

fn cases() -> Vec<Case> {
    let v: serde_json::Value = serde_json::from_str(include_str!("data/browser_cases.json")).unwrap();
    let s = |c: &serde_json::Value, k: &str| c[k].as_str().map(str::to_string);
    v.as_array()
        .unwrap()
        .iter()
        .map(|c| Case {
            name: s(c, "name").unwrap(),
            source: s(c, "source").unwrap(),
            translation: s(c, "translation").unwrap(),
            expect: s(c, "expect").unwrap(),
            out: s(c, "out"),
        })
        .collect()
}

#[test]
fn hostile_translations_match_locos() {
    let cases = cases();
    assert!(cases.len() >= 25);
    for c in cases {
        let got = HhcFilter.extract(c.source.as_bytes(), &ExtractOptions::default());
        if c.expect == "parse_error" {
            // The walker finds no unit but a browser reads an entry name: not an empty file, so still refused
            let e = got.err().unwrap_or_else(|| panic!("{}: LocOS finds no unit", c.name)).to_string();
            assert!(e.contains("a browser reads"), "{}: {e}", c.name);
            continue;
        }
        let mut doc = got.unwrap_or_else(|e| panic!("{}: {e}", c.name));
        let id = doc.units[0].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(c.translation.clone())]).unwrap();
        let out = HhcFilter.merge(&doc);
        match c.expect.as_str() {
            "refused" => {
                let e = out.expect_err(&c.name).to_string();
                assert!(e.contains("cannot be exported safely"), "{}: {e}", c.name);
            }
            _ => assert_eq!(String::from_utf8(out.unwrap()).unwrap(), c.out.unwrap(), "{}", c.name),
        }
    }
}

/// The breakout the gate found in the first version (no browser parse): the walker reads a `<param>` inside the
/// quoted attribute of a tag it does not accept, writes the unquoted value in double quotes, and the browser sees
/// those quotes end the attribute.
#[test]
fn param_inside_an_attribute_never_becomes_an_attribute() {
    let src = "<OBJECT type=\"text/sitemap\"><a b=\"x\"c=\"<param name=Name value=X>\"><param name=\"Local\" value=\"y\"></OBJECT>";
    for tr in ["\" onmouseover=alert(1) x=\"", "a onmouseover=alert(1) b", "onmouseover"] {
        let mut doc = HhcFilter.extract(src.as_bytes(), &ExtractOptions::default()).unwrap();
        let id = doc.units[0].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(tr)]).unwrap();
        assert!(HhcFilter.merge(&doc).is_err(), "{tr}");
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig { cases: 400, ..Default::default() })]
    /// Markup soup around an entry: the browser parse never panics, and a merge either is refused or reads back.
    #[test]
    fn markup_soup_never_panics(
        before in "[<>/!?\"'= a-zA-Z\u{a0}\u{e9}\u{3042}\\[\\]`-]{0,80}",
        after in "[<>/!?\"'= a-zA-Z\u{a0}\u{e9}\u{3042}\\[\\]`-]{0,80}",
        tr in "[a-z\"'<>= /`&;#-]{1,30}",
    ) {
        let src = format!("{before}<OBJECT type=\"text/sitemap\"><param name=Name value=Entry>{after}<param name=\"Name\" value=\"Two\"></OBJECT>");
        if let Ok(mut doc) = HhcFilter.extract(src.as_bytes(), &ExtractOptions::default()) {
            let n = doc.units.len();
            let id = doc.units[0].segments[0].id.clone();
            doc.set_target(&id, vec![Inline::text(format!("x{tr}"))]).unwrap();
            if let Ok(out) = HhcFilter.merge(&doc) {
                let back = HhcFilter.extract(&out, &ExtractOptions::default()).unwrap();
                proptest::prop_assert_eq!(back.units.len(), n);
            }
        }
    }
}
