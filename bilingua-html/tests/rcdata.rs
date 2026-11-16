//! BILI-51: `<title>` and `<textarea>` are RCDATA in HTML5. A browser starts no tag, comment or CDATA section
//! inside them; only the element's own end tag ends them. The reader and the writer follow that rule, so a literal
//! `<` in title text reads as text and is written back as typed.

use bilingua_html::Html;
use bilingua_model::*;

fn page(head: &str, body: &str) -> String {
    format!("<!DOCTYPE html>\n<html><head>{head}</head><body>{body}<p>Para</p></body></html>\n")
}

fn open(s: &str) -> Document {
    Html.extract(s.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn sources(d: &Document) -> Vec<Vec<Inline>> {
    d.units.iter().map(|u| u.source()).collect()
}

/// Every unit translated as `T ` + its source, merged.
fn translated(d: &Document) -> String {
    let mut t = d.clone();
    let ids: Vec<String> = t.units.iter().flat_map(|u| u.segments.iter().map(|s| s.id.clone())).collect();
    for id in &ids {
        let src = t.find(id).unwrap().1.source.clone();
        let mut tgt = vec![Inline::text("T ")];
        tgt.extend(src);
        t.set_target(id, tgt).unwrap();
    }
    String::from_utf8(Html.merge(&t).unwrap()).unwrap()
}

fn set_first(d: &Document, text: &str) -> String {
    let mut t = d.clone();
    let id = t.units[0].segments[0].id.clone();
    t.set_target(&id, vec![Inline::text(text)]).unwrap();
    String::from_utf8(Html.merge(&t).unwrap()).unwrap()
}

#[test]
fn a_literal_less_than_in_a_title_reads_as_text_and_round_trips() {
    let src = page("<title>1 < 2 & 3</title>", "");
    let d = open(&src);
    assert_eq!(d.units[0].source(), vec![Inline::text("1 < 2 & 3")]);
    assert_eq!(d.units[0].context.kind.as_deref(), Some("title"));
    // untouched: byte for byte
    assert_eq!(Html.merge(&d).unwrap(), src.as_bytes());
    // translated: written as typed, and the reader gives back exactly what was written
    let out = translated(&d);
    assert!(out.contains("<title>T 1 < 2 & 3</title>"), "{out}");
    let back = open(&out);
    assert_eq!(back.units[0].source(), vec![Inline::text("T 1 < 2 & 3")]);
    assert_eq!(back.units.len(), d.units.len());
    // a translation that adds a literal `<` keeps it
    let out = set_first(&d, "eins < zwei");
    assert!(out.contains("<title>eins < zwei</title>"), "{out}");
    assert_eq!(open(&out).units[0].source(), vec![Inline::text("eins < zwei")]);
}

#[test]
fn markup_inside_a_title_is_its_text() {
    for (title, text) in [
        ("a<b", "a<b"),
        ("x <b>y</b> z", "x <b>y</b> z"),
        ("a</b>c", "a</b>c"),
        ("a <!-- c --> d", "a <!-- c --> d"),
        ("a <![CDATA[ c ]]> d", "a <![CDATA[ c ]]> d"),
        ("a </titlex> b", "a </titlex> b"),
    ] {
        let src = page(&format!("<title>{title}</title>"), "");
        let d = open(&src);
        assert_eq!(sources(&d), vec![vec![Inline::text(text)], vec![Inline::text("Para")]], "{title}");
        assert!(d.units[0].codes.is_empty(), "{title}");
        assert_eq!(Html.merge(&d).unwrap(), src.as_bytes(), "{title}");
        let out = translated(&d);
        assert!(out.contains(&format!("<title>T {text}</title>")), "{title}: {out}");
        assert_eq!(open(&out).units[0].source(), vec![Inline::text(format!("T {text}"))], "{title}");
    }
}

#[test]
fn the_end_tag_is_matched_without_regard_to_case_and_needs_a_delimiter() {
    let d = open(&page("<TITLE>a < b</Title >", ""));
    assert_eq!(sources(&d), vec![vec![Inline::text("a < b")], vec![Inline::text("Para")]]);
    let d = open(&page("<title>a</title/>", ""));
    assert_eq!(sources(&d)[0], vec![Inline::text("a")]);
}

#[test]
fn a_target_cannot_end_the_title_early() {
    let src = page("<title>Home</title>", "");
    let d = open(&src);
    let out = set_first(&d, "a </TITLE><script>x</script> b");
    assert!(out.contains("<title>a &lt;/TITLE><script>x</script> b</title>"), "{out}");
    // still one title, nothing new on the page
    let back = open(&out);
    assert_eq!(back.units.len(), d.units.len());
    assert_eq!(back.units[0].context.kind.as_deref(), Some("title"));
}

#[test]
fn a_textarea_is_rcdata_too() {
    let src = page("<title>t</title>", "<textarea name=t>a<b c &lt; d</textarea>");
    let d = open(&src);
    assert_eq!(d.units[1].source(), vec![Inline::text("a<b c &lt; d")]);
    assert!(d.units[1].codes.is_empty());
    let out = translated(&d);
    assert!(out.contains("<textarea name=t>T a<b c &lt; d</textarea>"), "{out}");
    assert_eq!(open(&out).units[1].source(), vec![Inline::text("T a<b c &lt; d")]);
}

#[test]
fn a_title_in_svg_is_an_ordinary_element() {
    // in foreign content a comment inside <title> is a comment: two pieces of text, as a browser reads them
    let src = page("<title>Page</title>", "<svg viewBox=\"0 0 1 1\"><title>Logo <!-- c --> mark</title><path d=\"M0 0\"/></svg>");
    let d = open(&src);
    let texts: Vec<String> = d.units.iter().map(|u| plain_text(&u.source())).collect();
    assert!(texts.contains(&"Logo".to_string()) && texts.contains(&"mark".to_string()), "{texts:?}");
    assert_eq!(Html.merge(&d).unwrap(), src.as_bytes());
}

#[test]
fn a_self_closed_title_holds_nothing() {
    let src = "<?xml version=\"1.0\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title/></head><body><p>One</p><p>Two</p></body></html>\n";
    let d = open(src);
    assert_eq!(sources(&d), vec![vec![Inline::text("One")], vec![Inline::text("Two")]]);
}

#[test]
fn xhtml_keeps_xml_text_in_a_title() {
    // a page that reads as XML stays XML: a `<` the translation adds is written as a reference
    let src = "<?xml version=\"1.0\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Home</title></head><body><p>One</p></body></html>\n";
    let d = open(src);
    let out = set_first(&d, "1 < 2");
    assert!(out.contains("<title>1 &lt; 2</title>"), "{out}");
}
