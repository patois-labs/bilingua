//! The empty-translation rule: an empty target is untranslated (the source is written) unless the segment is marked
//! translated to nothing (`Document::set_target_empty`); then the writer writes an empty value.

use bilingua::*;

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("de").with_granularity(Granularity::Paragraph)
}

fn first_id(d: &Document) -> String {
    d.units[0].segments[0].id.clone()
}

#[test]
fn an_empty_translation_is_written_empty() {
    for (name, src, want, keep) in [
        ("a.json", "{\"a\": \"Hello\", \"b\": \"World\"}", "\"a\": \"\"", "World"),
        ("a.properties", "a=Hello\nb=World\n", "a=\n", "World"),
        ("a.html", "<html><body><p>Hello</p><p>World</p></body></html>", "<p></p>", "World"),
    ] {
        let mut d = extract(name, src.as_bytes(), &opts()).unwrap();
        // untranslated: the source is written
        assert_eq!(merge(&d).unwrap(), src.as_bytes(), "{name}");
        let id = first_id(&d);
        d.set_target_empty(&id).unwrap();
        let s = d.find(&id).unwrap().1;
        assert!(s.empty_target && s.has_target() && s.target.is_empty(), "{name}");
        let out = String::from_utf8(merge(&d).unwrap()).unwrap();
        assert!(out.contains(want) && out.contains(keep) && !out.contains("Hello"), "{name}: {out}");
        // the flag survives JSON, and a real target clears it
        let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d, "{name}");
        d.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
        assert!(!d.find(&id).unwrap().1.empty_target, "{name}");
    }
}

#[test]
fn a_locked_unit_cannot_be_emptied() {
    let mut d = extract("a.json", b"{\"a\": \"Hello\"}", &opts()).unwrap();
    d.units[0].translate = false;
    let id = first_id(&d);
    assert!(matches!(d.set_target_empty(&id), Err(Error::Locked(_))));
    assert!(matches!(d.set_target_empty("nope"), Err(Error::UnknownSegment(_))));
}
