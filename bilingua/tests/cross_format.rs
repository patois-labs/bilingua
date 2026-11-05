//! Sentence segmentation applied to every format through the facade: an untouched document still merges
//! byte for byte, and translating one sentence changes only that sentence. The DOCX fixture was produced by
//! LibreOffice from page.html, so it is a real-world Word file, not one built by our own writer.

use bilingua::*;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn opts(mode: Granularity) -> ExtractOptions {
    ExtractOptions {
        source_lang: Some("en".into()),
        target_lang: Some("es-ES".into()),
        granularity: mode,
        include_locked: false,
        limits: None,
    }
}

fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("page.html", std::fs::read(format!("{FIXTURES}/page.html")).unwrap()),
        ("page.docx", std::fs::read(format!("{FIXTURES}/page.docx")).unwrap()),
        ("notes.md", b"# Title\n\nFirst sentence. Second *sentence* here.\n".to_vec()),
        ("strings.json", br#"{"a": "One. Two.", "b": {"c": "Hello {name}. Bye."}}"#.to_vec()),
        (
            "app.po",
            b"msgid \"\"\nmsgstr \"\"\n\"Language: es\\n\"\n\n#. A button\nmsgid \"Save it. Now.\"\nmsgstr \"\"\n".to_vec(),
        ),
        ("cues.srt", b"1\n00:00:01,000 --> 00:00:02,000\nHello there. How are you?\n\n".to_vec()),
        ("cues.vtt", b"WEBVTT\n\n00:01.000 --> 00:02.000\nOne. Two.\n".to_vec()),
        ("plain.txt", b"First. Second.\n\nThird paragraph.\n".to_vec()),
    ]
}

#[test]
fn untouched_documents_merge_unchanged_in_every_mode() {
    for (name, bytes) in samples() {
        for mode in [Granularity::Sentence, Granularity::Paragraph, Granularity::Document] {
            let doc = extract(name, &bytes, &opts(mode)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(doc.segment_count() > 0, "{name}: no segments");
            let out = merge(&doc).unwrap_or_else(|e| panic!("{name} {mode:?}: {e}"));
            if name.ends_with(".docx") {
                // Zip container bytes may differ; the brief requires identical XML parts.
                assert_docx_parts_equal(&bytes, &out);
            } else {
                assert_eq!(out, bytes, "{name} {mode:?} did not round-trip");
            }
        }
    }
}

#[test]
fn sentence_mode_splits_and_translating_one_sentence_changes_only_it() {
    for (name, bytes) in samples() {
        let mut doc = extract(name, &bytes, &opts(Granularity::Sentence)).unwrap();
        let multi = doc.units.iter().find(|u| u.translate && u.segments.len() > 1);
        let Some(unit) = multi else { panic!("{name}: no unit was split into sentences") };
        let seg = unit.segments[0].clone();
        let other = plain_text(&unit.segments[1].source);
        // Keep the source's tags, replace the text: a translation that only changes words.
        let target: Vec<Inline> = seg
            .source
            .iter()
            .map(|i| match i {
                Inline::Text(t) if !t.trim().is_empty() => Inline::text("TRADUCIDO"),
                x => x.clone(),
            })
            .collect();
        doc.set_target(&seg.id, target).unwrap_or_else(|e| panic!("{name}: {e}"));
        let out = merge(&doc).unwrap();
        let text = if name.ends_with(".docx") { docx_text(&out) } else { String::from_utf8_lossy(&out).into_owned() };
        assert!(text.contains("TRADUCIDO"), "{name}: translation missing");
        // Markup may sit between the words (bold runs, *emphasis*), so check word by word.
        for w in other.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
            assert!(text.contains(w), "{name}: word {w:?} of the neighbouring sentence lost: {other}");
        }
    }
}

#[test]
fn previews_wrap_every_segment_for_html_markdown_and_docx() {
    for (name, bytes) in samples().into_iter().filter(|(n, _)| n.ends_with(".html") || n.ends_with(".md") || n.ends_with(".docx")) {
        let doc = extract(name, &bytes, &opts(Granularity::Sentence)).unwrap();
        let html = preview(&doc).unwrap_or_else(|| panic!("{name}: no preview"));
        assert!(!html.to_ascii_lowercase().contains("<script"), "{name}: script in preview");
        let missing: Vec<_> = doc
            .segments()
            .filter(|(u, _)| u.translate)
            .map(|(_, s)| s.id.clone())
            .filter(|id| !html.contains(&format!("\"{id}\"")) && !html.contains(&format!("{id},")) && !html.contains(&format!(",{id}")))
            .collect();
        assert!(missing.is_empty(), "{name}: segments without a preview span: {missing:?}");
        let first = doc.segments().find(|(u, _)| u.translate).unwrap().1.id.clone();
        assert!(render_segment(&doc, &first).is_some(), "{name}: render_segment none for {first}");
    }
}

fn zip_parts(bytes: &[u8]) -> std::collections::BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = std::collections::BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = Vec::new();
        f.read_to_end(&mut v).unwrap();
        out.insert(f.name().unwrap().to_string(), v);
    }
    out
}

fn assert_docx_parts_equal(a: &[u8], b: &[u8]) {
    let (pa, pb) = (zip_parts(a), zip_parts(b));
    assert_eq!(pa.keys().collect::<Vec<_>>(), pb.keys().collect::<Vec<_>>(), "docx entry list changed");
    for (k, v) in &pa {
        assert!(v == &pb[k], "docx part {k} changed");
    }
}

/// The visible text of word/document.xml: markup removed, so text split across runs reads as one string.
fn docx_text(bytes: &[u8]) -> String {
    let xml = String::from_utf8_lossy(&zip_parts(bytes)["word/document.xml"]).into_owned();
    let mut out = String::new();
    let mut in_tag = false;
    for c in xml.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}
