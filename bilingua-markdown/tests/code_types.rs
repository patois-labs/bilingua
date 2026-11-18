//! BILI-118 (unit model step 5): Markdown inline codes carry their type, links their target and title, images
//! their file and alt text; the LocOS profile has its own extraction and leaves them unset.

use bilingua_markdown::MarkdownFilter;
use bilingua_model::*;

const SRC: &str = "Some **bold**, *italic*, `code`, a [link](https://x.com/ \"X\"), <https://y.com/> and ![a cat](cat.png).\n";

#[test]
fn extract_fills_types_and_attrs() {
    let doc = MarkdownFilter.extract(SRC.as_bytes(), &ExtractOptions::default()).unwrap();
    let u = doc.units.iter().find(|u| u.codes().len() >= 5).expect("the paragraph");
    let typed = |t: CodeType| u.codes().values().filter(move |c| c.code_type() == Some(t)).collect::<Vec<_>>();
    assert_eq!(typed(CodeType::Bold).len(), 1);
    assert_eq!(typed(CodeType::Italic).len(), 1);
    assert_eq!(typed(CodeType::Code).len(), 1);
    let links = typed(CodeType::Link);
    let hrefs: Vec<_> = links.iter().map(|c| (c.label(), c.attr("href"), c.attr("title"))).collect();
    assert!(hrefs.contains(&(Some("link"), Some("https://x.com/"), Some("X"))), "{hrefs:?}");
    assert!(hrefs.contains(&(Some("link"), Some("https://y.com/"), None)), "{hrefs:?}");
    let img = typed(CodeType::Image);
    assert_eq!((img[0].attr("src"), img[0].attr("alt")), (Some("cat.png"), Some("a cat")));
}

#[test]
fn locos_profile_leaves_types_unset() {
    let doc = MarkdownFilter.extract(SRC.as_bytes(), &ExtractOptions::new().with_profile("locos")).unwrap();
    assert!(doc.units.iter().all(|u| u.codes().values().all(|c| c.code_type().is_none() && c.attrs().is_empty())));
}
