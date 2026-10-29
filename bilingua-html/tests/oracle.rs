//! Regression tests for patterns found by comparing Bilingua with other filters on sample files.

use bilingua_html::Html;
use bilingua_model::*;

fn extract(src: &str) -> Document {
    Html.extract(src.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

/// Ruby annotations are inline: the sentence around them stays one unit.
#[test]
fn ruby_annotations_stay_inside_their_paragraph() {
    let cases = [
        ("en", "<p>Ruby: <ruby>wei<rt>kan</rt></ruby> and a value.</p>"),
        ("ja", "<p>漢字は<ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp></ruby>と読みます。</p>"),
        ("zh", "<p>这是<ruby>汉<rt>hàn</rt></ruby>字的例子。</p>"),
        ("ko", "<p>한국어 <ruby>韓<rt>한</rt></ruby> 문장입니다.</p>"),
    ];
    for (lang, src) in cases {
        let d = extract(src);
        assert_eq!(d.units.len(), 1, "{lang}: {:?}", texts(&d));
        // unchanged merge is byte for byte, and a translation keeps the markup
        assert_eq!(Html.merge(&d).unwrap(), src.as_bytes());
    }
    let mut d = extract(cases[0].1);
    let seg = d.units[0].segments[0].id.clone();
    let mut t = d.units[0].source();
    if let Some(Inline::Text(first)) = t.first_mut() {
        *first = "Ruby translated: ".into();
    }
    d.set_target(&seg, t).unwrap();
    let out = String::from_utf8(Html.merge(&d).unwrap()).unwrap();
    assert!(out.starts_with("<p>Ruby translated: <ruby>wei<rt>kan</rt></ruby>"), "{out}");
}
