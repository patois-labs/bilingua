use bilingua::*;

const TXT: &str = "Welcome to Larchmoor. The tide is out.\n\nSecond paragraph, with Dr. Hale. Done!\n";
const JSON: &str = "{\n  \"title\": \"Harbor lamp. Warm light.\",\n  \"url\": \"https://example.test/lamp\",\n  \"msg\": \"Hello {name}. You have %d items.\",\n  \"ja\": \"\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3002}\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3002}\"\n}\n";
const MD: &str = "---\ntitle: Lamp guide\n---\n\n# Harbor lamp\n\nFirst sentence. **Second** sentence here. See [docs](https://x.test).\n\n- One item. Another one.\n";

fn opts(lang: &str, mode: Granularity) -> ExtractOptions {
    ExtractOptions { source_lang: Some(lang.into()), granularity: mode, ..Default::default() }
}

#[test]
fn untranslated_files_come_back_byte_identical_in_every_mode() {
    for (name, src) in [("a.txt", TXT), ("a.json", JSON), ("a.md", MD)] {
        for mode in [Granularity::Sentence, Granularity::Paragraph, Granularity::Document] {
            let d = extract(name, src.as_bytes(), &opts("en", mode)).unwrap();
            assert_eq!(d.granularity, mode);
            assert_eq!(merge(&d).unwrap(), src.as_bytes(), "{name} {mode:?}");
        }
    }
}

#[test]
fn sentence_mode_splits_and_paragraph_mode_does_not() {
    let s = extract("a.txt", TXT.as_bytes(), &opts("en", Granularity::Sentence)).unwrap();
    assert_eq!(s.segment_count(), 4);
    let ids: Vec<&str> = s.segments().map(|(_, seg)| seg.id.as_str()).collect();
    assert_eq!(ids, vec!["p1#1", "p1#2", "p2#1", "p2#2"]);
    let p = extract("a.txt", TXT.as_bytes(), &opts("en", Granularity::Paragraph)).unwrap();
    assert_eq!(p.segment_count(), 2);
}

#[test]
fn default_source_language_is_english() {
    let d = extract("a.txt", b"One. Two.\n", &ExtractOptions::default()).unwrap();
    assert_eq!(d.segment_count(), 2);
}

#[test]
fn japanese_json_splits_on_the_ideographic_stop() {
    let d = extract("a.json", JSON.as_bytes(), &opts("ja", Granularity::Sentence)).unwrap();
    let ja = d.units.iter().find(|u| u.context.key.as_deref() == Some("/ja")).unwrap();
    assert_eq!(ja.segments.len(), 2);
    assert!(ja.separators.iter().all(|s| s.is_empty()));
    // the URL is not a unit
    assert!(d.units.iter().all(|u| u.context.key.as_deref() != Some("/url")));
    // placeholders are tags: each stays whole in the sentence it belongs to
    let msg = d.units.iter().find(|u| u.context.key.as_deref() == Some("/msg")).unwrap();
    assert_eq!(msg.segments.len(), 2);
    assert!(msg.segments[0].source.iter().any(|i| matches!(i, Inline::Tag { display, .. } if display == "{name}")));
    assert!(msg.segments[1].source.iter().any(|i| matches!(i, Inline::Tag { display, .. } if display == "%d")));
}

#[test]
fn translating_through_the_facade() {
    let mut d = extract("a.txt", TXT.as_bytes(), &opts("en", Granularity::Sentence)).unwrap();
    d.set_target("p1#1", vec![Inline::text("Bienvenido a Larchmoor.")]).unwrap();
    d.set_target("p1#2", vec![Inline::text("La marea est\u{e1} baja.")]).unwrap();
    let out = String::from_utf8(merge(&d).unwrap()).unwrap();
    assert_eq!(out, "Bienvenido a Larchmoor. La marea est\u{e1} baja.\n\nSecond paragraph, with Dr. Hale. Done!\n");

    let mut d = extract("a.md", MD.as_bytes(), &opts("en", Granularity::Sentence)).unwrap();
    let (unit, seg) = d.segments().find(|(_, s)| plain_text(&s.source) == "First sentence.").map(|(u, s)| (u.id.clone(), s.id.clone())).unwrap();
    let _ = unit;
    d.set_target(&seg, vec![Inline::text("Primera frase.")]).unwrap();
    let out = String::from_utf8(merge(&d).unwrap()).unwrap();
    assert!(out.contains("Primera frase. **Second** sentence here. See [docs](https://x.test)."), "{out}");
}

#[test]
fn paired_markdown_tags_are_not_split() {
    let d = extract("a.md", b"Before. **Bold one. Bold two.** After.\n", &opts("en", Granularity::Sentence)).unwrap();
    let u = &d.units[0];
    assert_eq!(u.segments.len(), 3);
    assert_eq!(plain_text(&u.segments[1].source), "Bold one. Bold two.");
}

#[test]
fn detection() {
    assert_eq!(detect("x.json", b"[]").unwrap().id(), "json");
    assert_eq!(detect("x.md", b"hi").unwrap().id(), "markdown");
    assert_eq!(detect("x.TXT", b"hi").unwrap().id(), "txt");
    assert_eq!(detect("data", b"{\"a\": \"b\"}").unwrap().id(), "json");
    assert_eq!(detect("data", b"just words").unwrap().id(), "txt");
    assert!(detect("x.bin", &[0, 1, 2, 0xFF]).is_none());
    assert!(matches!(extract("x.bin", &[0, 1, 2, 0xFF], &ExtractOptions::default()), Err(Error::Unsupported(_))));
}

#[test]
fn preview_only_for_markdown() {
    let d = extract("a.md", MD.as_bytes(), &opts("en", Granularity::Sentence)).unwrap();
    let html = preview(&d).unwrap();
    assert!(html.contains("data-bl-seg=\"u2\""));
    let seg = &d.units[1].segments[0].id;
    assert!(render_segment(&d, seg).is_some());
    let t = extract("a.txt", TXT.as_bytes(), &ExtractOptions::default()).unwrap();
    assert!(preview(&t).is_none());
}

#[test]
fn resegment_changes_granularity_and_join_split_are_exported() {
    let mut d = extract("a.txt", TXT.as_bytes(), &opts("en", Granularity::Sentence)).unwrap();
    resegment(&mut d, "en", Granularity::Paragraph);
    assert_eq!(d.segment_count(), 2);
    resegment(&mut d, "en", Granularity::Sentence);
    assert_eq!(d.segment_count(), 4);
    join(&mut d, "p1#1", "p1#2").unwrap();
    assert_eq!(d.segment_count(), 3);
    split(&mut d, "p1", 22).unwrap();
    assert_eq!(d.segment_count(), 4);
    assert_eq!(merge(&d).unwrap(), TXT.as_bytes());
}

#[test]
fn bad_files_are_errors_not_panics() {
    assert!(extract("a.json", b"{oops", &ExtractOptions::default()).is_err());
    assert!(extract("a.txt", &[0xFF, 0xFE, 0x41, 0x00], &ExtractOptions::default()).is_err());
    assert!(extract("a.md", &[0x23, 0x20, 0xFF], &ExtractOptions::default()).is_err());
}
