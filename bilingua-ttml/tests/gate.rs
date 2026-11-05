//! Opus gate (11-05): hidden splice codes, markup injection through translations, damaged tags, namespace-heavy
//! inputs that must stay linear, legacy-encoding refusals and the target-language stamp rules.

mod common;

use bilingua_model::*;
use common::*;
use std::time::{Duration, Instant};

const TT: &[u8] = include_bytes!("data/locos_suite_tt.ttml");

fn caption_count(out: &[u8]) -> usize {
    let s = String::from_utf8_lossy(out);
    s.matches("<p ").count() + s.matches("<p>").count()
}

/// Translate caption 0 of the suite document with these inlines and return the merged text.
fn merge_target(target: Vec<Inline>) -> String {
    let mut d = extract(TT);
    d.units[0].segments[0].target = target;
    String::from_utf8(F.merge(&d).expect("merge")).expect("utf-8")
}

#[test]
fn hidden_splice_codes_never_reach_a_target() {
    // `~raw` (the caption content as written) and `~meta` (splice data) are codes of the unit but never tags a
    // translation can place: set directly on the segment, past set_target's check, they are dropped.
    for id in ["~raw", "~meta"] {
        for kind in [TagKind::Standalone, TagKind::Open] {
            let out = merge_target(vec![Inline::text("Hola "), Inline::tag(id, kind, ""), Inline::text("fin")]);
            assert!(out.contains(">Hola fin</p>"), "{id} {kind:?}: {out}");
            assert!(!out.contains("Hello &amp; welcome<br/>to the"), "{id}: source copied into the target: {out}");
            assert!(!out.contains("0|0|br"), "{id}: splice data written: {out}");
            assert_eq!(well_formed(out.as_bytes()).units.len(), extract(TT).units.len());
        }
    }
}

#[test]
fn translations_cannot_create_captions_spans_timing_styles_or_breaks() {
    let attacks = [
        "</p><p begin=\"0s\" end=\"99s\">new cue",
        "<span tts:color=\"red\">x</span>",
        "<br/>",
        "<![CDATA[x]]>",
        "]]>",
        "<!-- c --><?pi x?>",
        "&lt; &amp; &#60; &nbsp;",
        "\" begin=\"0s",
        "<style xml:id=\"s9\"/>",
        "<set begin=\"1s\" tts:color=\"red\"/>",
        "<tt:p>x</tt:p>",
    ];
    for a in attacks {
        let out = merge_target(vec![Inline::text(a)]);
        let back = well_formed(out.as_bytes());
        assert_eq!(back.units.len(), 3, "{a}: {out}");
        assert_eq!(caption_count(out.as_bytes()), caption_count(TT), "{a}: {out}");
        assert_eq!(show(&back.units[0]), a.trim(), "{a}: the attack reads back as plain text: {out}");
        assert_eq!(out.matches("<span").count(), String::from_utf8_lossy(TT).matches("<span").count() - 1, "{a}");
        assert_eq!(out.matches("<br/>").count(), 0, "{a}: {out}");
        assert_eq!(out.matches("<set ").count(), 1, "{a}");
        assert_eq!(out.matches("begin=").count(), 5 + a.matches("begin=").count(), "{a}: timing only as escaped text");
    }
    // typed newline and pasted page break: a <br/> only, never other markup
    let out = merge_target(vec![Inline::text("a\nb\u{0C}c\u{0}d\u{1F}e")]);
    assert!(out.contains(">a<br/>b<br/>cde</p>"), "{out}");
}

#[test]
fn damaged_tags_still_give_well_formed_captions() {
    let d = extract(TT);
    let src = d.units[0].source();
    let tags: Vec<Inline> = src.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned().collect();
    // every order, repeat and drop of the source tags plus foreign ids
    let mut variants: Vec<Vec<Inline>> = vec![
        tags.iter().rev().cloned().collect(),
        tags.iter().chain(tags.iter()).cloned().collect(),
        vec![Inline::tag("2", TagKind::Close, ""), Inline::text("x"), Inline::tag("2", TagKind::Open, "")],
        vec![Inline::tag("2", TagKind::Open, ""), Inline::text("x")],
        vec![Inline::tag("1", TagKind::Open, ""), Inline::text("x"), Inline::tag("1", TagKind::Close, "")],
        vec![Inline::tag("2", TagKind::Standalone, ""), Inline::text("x")],
        vec![Inline::tag("99", TagKind::Standalone, ""), Inline::tag("99", TagKind::Open, ""), Inline::text("x")],
    ];
    for t in &tags {
        variants.push(vec![Inline::text("x"), t.clone()]);
    }
    for mut v in variants {
        v.insert(0, Inline::text("t ")); // a caption with only codes is no unit, so keep some text
        let out = merge_target(v.clone());
        let back = well_formed(out.as_bytes());
        assert_eq!(back.units.len(), 3, "{v:?}: {out}");
        assert_eq!(caption_count(out.as_bytes()), caption_count(TT), "{v:?}");
    }
}

fn timed<T>(what: &str, budget: Duration, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let r = f();
    let took = t.elapsed();
    eprintln!("{what}: {took:?}");
    assert!(took < budget, "{what} took {took:?} (budget {budget:?}): not linear");
    r
}

#[test]
fn many_namespaces_in_scope_stay_linear() {
    // 4000 declarations on <tt> (the cap is 4096), the styling prefix declared first (the deepest one for a
    // search from the innermost declaration), then many elements and captions that look it up.
    let mut s = String::from("<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\"");
    for k in 0..4000 {
        s.push_str(&format!(" xmlns:n{k}=\"urn:n{k}\""));
    }
    s.push_str(" xml:lang=\"en\"><head><metadata>");
    for _ in 0..300_000 {
        s.push_str("<tts:e/>");
    }
    s.push_str("</metadata></head><body><div>");
    for _ in 0..40_000 {
        s.push_str("<p tts:color=\"red\">x</p>");
    }
    s.push_str("</div></body></tt>");
    let d = timed("namespaces in scope", Duration::from_secs(20), || F.extract(s.as_bytes(), &opts()).expect("extract"));
    assert_eq!(d.units.len(), 40_000);
}

#[test]
fn many_prefixed_attributes_stay_linear() {
    // one start tag with 100 000 distinct prefixed attributes: the duplicate check must not compare all pairs
    let mut s = String::from("<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:a=\"urn:a\"><body><div><p");
    for k in 0..100_000 {
        s.push_str(&format!(" a:x{k}=\"1\""));
    }
    s.push_str(">hi</p></div></body></tt>");
    let d = timed("prefixed attributes", Duration::from_secs(20), || F.extract(s.as_bytes(), &opts()).expect("extract"));
    assert_eq!(d.units.len(), 1);
    // and a real duplicate behind two prefixes for the same namespace is still refused
    let dup = b"<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:a=\"urn:a\" xmlns:b=\"urn:a\"><body><p a:x=\"1\" b:x=\"2\">hi</p></body></tt>";
    assert!(F.extract(dup, &opts()).is_err());
}

#[test]
fn undeclared_legacy_bytes_are_refused_and_declared_ones_read() {
    // Latin-1 bytes with no declaration: not UTF-8, refused (XML and LocOS)
    let raw = b"<tt xmlns=\"http://www.w3.org/ns/ttml\"><body><p>Caf\xe9</p></body></tt>";
    assert!(F.extract(raw, &opts()).is_err());
    let decl = [b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>".as_slice(), raw].concat();
    let d = extract(&decl);
    assert_eq!(show(&d.units[0]), "Caf\u{e9}");
    assert_eq!(F.merge(&d).unwrap(), decl);
    // a declaration that lies (says UTF-8, bytes are Latin-1) is refused too
    let lie = [b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>".as_slice(), raw].concat();
    assert!(F.extract(&lie, &opts()).is_err());
}

#[test]
fn stamp_only_when_something_is_translated_unless_asked() {
    let mut d = F.extract(TT, &opts_for(Some("ar"))).unwrap();
    // nothing translated: original bytes with the engine default, stamped with stamp_target_always
    assert_eq!(F.merge(&d).unwrap(), TT);
    let always = MergeOptions { stamp_target_always: true, ..Default::default() };
    let s = String::from_utf8(F.merge_with(&d, &always).unwrap()).unwrap();
    assert!(s.contains("xml:lang=\"ar\">\n  <head>") && s.contains("<div xml:lang=\"ar\">"), "{s}");
    assert!(!s.contains("direction"), "no caption rebuilt, so no direction: {s}");
    // an echo (target = source) counts as translated for the stamp (the model's rule), but no caption is rebuilt,
    // so the caption bytes stay and no direction is written
    for u in &mut d.units {
        u.segments[0].target = u.segments[0].source.clone();
    }
    let echo = String::from_utf8(F.merge(&d).unwrap()).unwrap();
    assert_eq!(echo, s, "an echo merge is the stamp-only merge");
    // one caption translated: stamped, and only that caption gets the right-to-left level
    set(&mut d, 1, "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}");
    let s = String::from_utf8(F.merge(&d).unwrap()).unwrap();
    assert!(s.contains("xml:lang=\"ar\">\n  <head>"), "{s}");
    assert_eq!(s.matches("tts:direction=\"rtl\"").count(), 1, "{s}");
    assert!(s.contains("<p tts:direction=\"rtl\" begin=\"5s\" end=\"7s\">"), "{s}");
    // no target language: never stamped
    d.target_lang = None;
    assert!(!String::from_utf8(F.merge(&d).unwrap()).unwrap().contains("direction"));
}

#[test]
fn default_namespace_switches_decide_what_is_a_caption() {
    let doc = br#"<tt xmlns="http://www.w3.org/ns/ttml" xml:lang="en"><body><div>
<p>one</p>
<div xmlns=""><p>no namespace</p></div>
<x:wrap xmlns:x="urn:x"><p xmlns="http://www.w3.org/ns/ttml">inside a foreign element</p></x:wrap>
<tt:p xmlns:tt="http://www.w3.org/2006/10/ttaf1">other TTML namespace</tt:p>
<p xmlns="urn:other">foreign default</p>
<q:p xmlns:q="http://www.w3.org/ns/ttml">two</q:p>
</div></body></tt>"#;
    assert_eq!(texts(doc), ["one", "other TTML namespace", "two"]);
    assert_identity(doc);
}

#[test]
fn a_shadowed_styling_prefix_is_never_used_for_the_direction() {
    // `tts` is the styling prefix on <tt> but rebound to another namespace on <div>: the caption must get a fresh
    // prefix bound to the styling namespace, never `tts:direction` (which would be urn:other's attribute)
    let doc = br#"<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tts="http://www.w3.org/ns/ttml#styling" xml:lang="en"><body><div xmlns:tts="urn:other"><p begin="1s">Hi</p></div></body></tt>"#;
    let out = build_str(doc, &[(0, "\u{05E9}\u{05DC}\u{05D5}\u{05DD}")], Some("he"));
    assert!(out.contains("xmlns:tts1=\"http://www.w3.org/ns/ttml#styling\""), "{out}");
    assert!(out.contains("<p tts1:direction=\"rtl\" begin=\"1s\">"), "{out}");
    // and a second styling prefix declared inside wins over the outer one
    let doc = br#"<tt xmlns="http://www.w3.org/ns/ttml" xmlns:tts="http://www.w3.org/ns/ttml#styling" xml:lang="en"><body><div xmlns:s="http://www.w3.org/ns/ttml#styling"><p>Hi</p></div></body></tt>"#;
    let out = build_str(doc, &[(0, "\u{05E9}")], Some("he"));
    assert!(out.contains("<p s:direction=\"rtl\">"), "{out}");
}
