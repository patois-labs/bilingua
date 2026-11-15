//! Regression tests for the 0.9.1 security audit: the native SRT and WebVTT writers keep a translated cue one cue.

use bilingua_model::{Document, Error, ExtractOptions, Filter, Inline};
use bilingua_subtitle::{Srt, Vtt};

const SRT: &str = "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n2\n00:00:03,000 --> 00:00:04,000\nWorld\n";
const VTT: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello\n\n00:00:03.000 --> 00:00:04.000\nWorld\n";

fn with_target(f: &dyn Filter, src: &str, target: &str) -> Document {
    let mut doc = f.extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text(target)]).unwrap();
    doc
}

fn times(doc: &Document) -> Vec<(String, String)> {
    doc.units.iter().map(|u| (u.context.extra["start"].clone(), u.context.extra["end"].clone())).collect()
}

/// A WebVTT target with a blank line and a timing line stays inside its cue: the arrow is escaped and reads back.
#[test]
fn a_vtt_translation_cannot_add_a_cue() {
    let doc = with_target(&Vtt, VTT, "Hallo\n\n00:00:10.000 --> 00:59:59.000 line:0 align:start\nINJECTED CUE");
    let out = String::from_utf8(Vtt.merge(&doc).unwrap()).unwrap();
    assert_eq!(out, "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHallo\n00:00:10.000 --&gt; 00:59:59.000 line:0 align:start\nINJECTED CUE\n\n00:00:03.000 --> 00:00:04.000\nWorld\n");
    let back = Vtt.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(times(&back), [("00:00:01.000".to_string(), "00:00:02.000".to_string()), ("00:00:03.000".to_string(), "00:00:04.000".to_string())]);
    assert!(back.units[0].source().iter().any(|i| matches!(i, Inline::Text(t) if t.contains("00:00:10.000 --> 00:59:59.000"))), "the arrow reads back as text");
}

/// An SRT target with a timing line is refused; blank lines alone (also from a lone CR) are dropped.
#[test]
fn an_srt_translation_cannot_add_a_cue() {
    let doc = with_target(&Srt, SRT, "Hallo\n\n99\n00:00:10,000 --> 00:59:59,000\nINJECTED CUE");
    match Srt.merge(&doc) {
        Err(Error::Unwritable { unit, .. }) => assert_eq!(unit.as_deref(), Some("c1")),
        other => panic!("expected Unwritable, got {other:?}"),
    }
    for target in ["Hallo\n\nWelt", "Hallo\r\rWelt", "Hallo\n \t\nWelt"] {
        let out = String::from_utf8(Srt.merge(&with_target(&Srt, SRT, target)).unwrap()).unwrap();
        assert_eq!(out, SRT.replace("Hello", "Hallo\nWelt"), "{target:?}");
    }
    // an arrow that is not a timing line is ordinary text
    let out = String::from_utf8(Srt.merge(&with_target(&Srt, SRT, "A --> B")).unwrap()).unwrap();
    assert_eq!(out, SRT.replace("Hello", "A --> B"));
}

/// Ordinary translations are written as before, and a source cue that already holds a timing line (two cues run
/// together in the file) can still be translated with that line kept.
#[test]
fn ordinary_cues_are_written_as_before() {
    let out = String::from_utf8(Srt.merge(&with_target(&Srt, SRT, "Hallo\nWelt")).unwrap()).unwrap();
    assert_eq!(out, SRT.replace("Hello", "Hallo\nWelt"));
    let crlf = SRT.replace('\n', "\r\n");
    let out = String::from_utf8(Srt.merge(&with_target(&Srt, &crlf, "Hallo\nWelt")).unwrap()).unwrap();
    assert_eq!(out, crlf.replace("Hello", "Hallo\r\nWelt"));
    let out = String::from_utf8(Vtt.merge(&with_target(&Vtt, VTT, "Hallo <Welt> & mehr")).unwrap()).unwrap();
    assert_eq!(out, VTT.replace("Hello", "Hallo &lt;Welt> &amp; mehr"));

    let run_together = "1\n00:00:01,000 --> 00:00:02,000\nHello\n2\n00:00:03,000 --> 00:00:04,000\nWorld\n";
    let mut doc = Srt.extract(run_together.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 1);
    let seg = doc.units[0].segments[0].clone();
    let target: Vec<Inline> = seg.source.iter().map(|i| match i {
        Inline::Text(t) => Inline::text(t.replace("Hello", "Hallo")),
        tag => tag.clone(),
    }).collect();
    doc.set_target(&seg.id, target).unwrap();
    let out = String::from_utf8(Srt.merge(&doc).unwrap()).unwrap();
    assert_eq!(out, run_together.replace("Hello", "Hallo"));
}

/// Run 2 L1: one cue with many open tags and then many close tags of another name: each close tag used to walk the
/// stack of open tags. Open tags are now found by name.
#[test]
fn many_unmatched_close_tags_in_one_cue_are_linear() {
    let n = 40_000;
    let secs = |cue: &str| {
        let src = format!("1\n00:00:01,000 --> 00:00:02,000\n{cue}\n");
        let t = std::time::Instant::now();
        let _ = Srt.extract(src.as_bytes(), &ExtractOptions::new());
        t.elapsed().as_secs_f64()
    };
    let c = secs(&format!("Hello {}{}", "<i>".repeat(n), "<b>x</b>".repeat(n)));
    let a = secs(&format!("Hello {}{}", "<i>".repeat(n), "</b>".repeat(n)));
    assert!(a < c * 5.0 + 3.0, "close tags {a:.2} s, control {c:.2} s");
    // tags pair as before: a close tag closes the innermost open tag of its name and drops those above it
    let doc = Srt.extract(b"1\n00:00:01,000 --> 00:00:02,000\n<b>a <i>b <u>c</b> d</u></i>\n", &ExtractOptions::new()).unwrap();
    let pairs = doc.units[0].codes.values().filter(|c| c.close.is_some()).count();
    assert_eq!(pairs, 1);
}
