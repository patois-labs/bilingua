//! Random bytes and byte-mutated pages never panic; whatever opens merges back to its own bytes untouched, and a
//! pseudo-translation either merges or is refused with a typed error.
use bilingua_html::{Html, HtmlFilter};
use bilingua_model::*;
use proptest::prelude::*;

const SEED_PAGES: &[&str] = &[
    "<!DOCTYPE html><html lang=\"en\"><head><title>T <b>x</b></title><meta name=\"description\" content=\"D\"></head><body><p>Hello <b>big</b> <img alt=\"Cat\" src=a.png> world &amp; co.</p><script>if(a<b){}</script></body></html>",
    "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>A<br/>B <a href=\"#\" title=\"T\">c</a></p></body></html>",
    "<!--><a title=\"--> <p>Hi <img alt=\"cat\"> there</p><svg><style><a title=\"</style><img alt=\"x\"></svg>",
    "<p>Hello <?php echo $n; ?> and <span translate=\"no\">kept</span> <code>x</code></p>",
];

fn pseudo(doc: &mut Document) {
    let ids: Vec<(String, Vec<Inline>)> = doc
        .segments()
        .map(|(_, s)| (s.id.clone(), s.source.iter().map(|i| if let Inline::Text(t) = i { Inline::Text(format!("{t} \"<&>\"")) } else { i.clone() }).collect()))
        .collect();
    for (id, t) in ids {
        let _ = doc.set_target(&id, t);
    }
}

fn exercise(f: &HtmlFilter, bytes: &[u8]) {
    let Ok(mut d) = f.extract(bytes, &ExtractOptions::default()) else { return };
    assert_eq!(f.merge(&d).unwrap(), bytes);
    let _ = f.preview(&d);
    pseudo(&mut d);
    d.target_lang = Some("ar".into());
    if let Ok(out) = f.merge(&d) {
        let _ = f.extract(&out, &ExtractOptions::default());
    }
    let _ = f.preview(&d);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        exercise(&HtmlFilter::new(), &bytes);
    }

    #[test]
    fn mutated_pages_never_panic(page in 0..SEED_PAGES.len(), edits in proptest::collection::vec((any::<usize>(), any::<u8>(), 0u8..3), 1..8)) {
        let mut b = SEED_PAGES[page].as_bytes().to_vec();
        for (at, byte, op) in edits {
            let k = if b.is_empty() { 0 } else { at % b.len() };
            match op {
                0 => b.insert(k, byte),
                1 if !b.is_empty() => { b.remove(k); }
                _ if !b.is_empty() => b[k] = byte,
                _ => {}
            }
        }
        exercise(&HtmlFilter::new(), &b);
        exercise(&HtmlFilter::new().with_page_meta(false), &b);
        exercise(&HtmlFilter::new().with_template_pattern(regex::Regex::new(r"\{\{.*?\}\}").unwrap()), &b);
    }
}

#[test]
fn huge_and_truncated_pages_are_typed_errors_or_open() {
    let tight = ExtractOptions::new().with_limits(Limits::new().with_max_units(10));
    let many = "<p>x</p>".repeat(50);
    assert!(matches!(Html.extract(many.as_bytes(), &tight), Err(Error::Limit(_))));
    let tags = "<b>".repeat(200);
    assert!(matches!(Html.extract(tags.as_bytes(), &tight), Err(Error::Limit(_))));
    for s in SEED_PAGES {
        for end in 0..s.len() {
            if s.is_char_boundary(end) {
                exercise(&HtmlFilter::new(), &s.as_bytes()[..end]);
            }
        }
    }
}
