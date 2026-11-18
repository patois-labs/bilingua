//! bilingua#21: a line break kept as a tag (a .txt paragraph's line ending, a subtitle cue's line break, a Markdown
//! hard break) ends a sentence under Sentence granularity, and at the cut it moves into the separator, so a
//! translation without any tags still writes every line break with the file's own line ending.

use bilingua::*;

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("es")
}

fn has_tag(v: &[Inline]) -> bool {
    v.iter().any(|i| matches!(i, Inline::Tag { .. }))
}

/// Extracts at Sentence granularity, checks an untouched merge gives back the bytes, translates every segment with
/// plain text (no tags) and returns the segments' source texts and the merged output.
fn translate_plain(name: &str, src: &str) -> (Vec<String>, String) {
    let doc = extract(name, src.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(String::from_utf8(merge(&doc).unwrap()).unwrap(), src, "{name}: untouched merge");
    let mut t = doc.clone();
    let mut sources = Vec::new();
    for u in doc.units.iter().filter(|u| u.translate) {
        for s in &u.segments {
            let text = plain_text(&s.source);
            if text.trim().is_empty() {
                continue;
            }
            sources.push(text.clone());
            t.set_target(&s.id, vec![Inline::text(format!("T:{text}"))]).unwrap();
        }
    }
    (sources, String::from_utf8(merge(&t).unwrap()).unwrap())
}

#[test]
fn txt_one_sentence_per_line_lf_and_crlf() {
    for eol in ["\n", "\r\n"] {
        let src = format!("Hello world.{eol}This is a test.{eol}Bye now.{eol}");
        let doc = extract("a.txt", src.as_bytes(), &opts()).unwrap();
        let u = &doc.units[0];
        assert_eq!(u.segments.len(), 3, "{eol:?}");
        assert!(u.segments.iter().all(|s| !has_tag(&s.source)), "{eol:?}: a break tag left in a segment");
        assert_eq!(u.separators, vec![eol.to_string(), eol.to_string()]);
        let (sources, out) = translate_plain("a.txt", &src);
        assert_eq!(sources, vec!["Hello world.", "This is a test.", "Bye now."]);
        assert_eq!(out, format!("T:Hello world.{eol}T:This is a test.{eol}T:Bye now.{eol}"), "{eol:?}");
    }
    // mixed endings: each break keeps its own
    let (_, out) = translate_plain("a.txt", "One.\r\nTwo.\nThree.\n");
    assert_eq!(out, "T:One.\r\nT:Two.\nT:Three.\n");
}

#[test]
fn txt_mid_sentence_wraps_keep_their_tag() {
    let src = "This sentence is wrapped\nin the middle. We met Dr.\nSmith there.\n";
    let doc = extract("a.txt", src.as_bytes(), &opts()).unwrap();
    let u = &doc.units[0];
    let got: Vec<String> = u.segments.iter().map(|s| plain_text(&s.source)).collect();
    assert_eq!(got, vec!["This sentence is wrappedin the middle.", "We met Dr.Smith there."]);
    assert!(u.segments.iter().all(|s| has_tag(&s.source)));
    assert_eq!(String::from_utf8(merge(&doc).unwrap()).unwrap(), src);
}

#[test]
fn txt_paragraph_granularity_is_unchanged() {
    let src = "Hello world.\nThis is a test.\n";
    let doc = extract("a.txt", src.as_bytes(), &opts().with_granularity(Granularity::Paragraph)).unwrap();
    assert_eq!(doc.units[0].segments.len(), 1);
    assert!(has_tag(&doc.units[0].segments[0].source));
    assert!(doc.units[0].separators.is_empty());
    // Sentence then back to Paragraph is the Paragraph extraction
    let mut s = extract("a.txt", src.as_bytes(), &opts()).unwrap();
    assert_eq!(s.units[0].segments.len(), 2);
    resegment(&mut s, "en", Granularity::Paragraph);
    assert_eq!(s.units, doc.units);
}

#[test]
fn srt_and_vtt_cue_lines() {
    for eol in ["\n", "\r\n"] {
        let srt = format!("1{eol}00:00:01,000 --> 00:00:02,000{eol}Hello world.{eol}This is a test.{eol}{eol}2{eol}00:00:03,000 --> 00:00:04,000{eol}Bye now.{eol}");
        let (sources, out) = translate_plain("a.srt", &srt);
        assert_eq!(sources, vec!["Hello world.", "This is a test.", "Bye now."], "{eol:?}");
        assert_eq!(out, srt.replace("Hello", "T:Hello").replace("This", "T:This").replace("Bye", "T:Bye"), "{eol:?}");
        let vtt = format!("WEBVTT{eol}{eol}00:00:01.000 --> 00:00:02.000{eol}Hello world.{eol}This is a test.{eol}");
        let (sources, out) = translate_plain("a.vtt", &vtt);
        assert_eq!(sources, vec!["Hello world.", "This is a test."], "{eol:?}");
        assert_eq!(out, vtt.replace("Hello", "T:Hello").replace("This", "T:This"), "{eol:?}");
    }
}

#[test]
fn markdown_hard_break() {
    for eol in ["\n", "\r\n"] {
        let md = format!("# Title{eol}{eol}Hello world.  {eol}This is a test.{eol}");
        let doc = extract("a.md", md.as_bytes(), &opts()).unwrap();
        let u = doc.units.iter().find(|u| plain_text(&u.source()).starts_with("Hello")).unwrap();
        assert_eq!(u.segments.len(), 2, "{eol:?}: {:?}", u.segments);
        assert!(u.segments.iter().all(|s| !has_tag(&s.source)), "{eol:?}");
        assert_eq!(u.separators, vec![format!("  {eol}")]);
        let (_, out) = translate_plain("a.md", &md);
        assert_eq!(out, format!("# T:Title{eol}{eol}T:Hello world.  {eol}T:This is a test.{eol}"), "{eol:?}");
    }
}
