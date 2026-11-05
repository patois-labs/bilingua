//! The public structs are `#[non_exhaustive]`: callers outside the model build them with `new()` and `with_*`.

use bilingua_model::*;

#[test]
fn extract_options_builders() {
    let o = ExtractOptions::new()
        .with_source_lang("en")
        .with_target_lang(String::from("de-DE"))
        .with_granularity(Granularity::Paragraph)
        .with_include_locked(true)
        .with_limits(Limits::new().with_max_units(7))
        .with_format_option("ftl.translate_terms", true);
    assert_eq!(o.source_lang.as_deref(), Some("en"));
    assert_eq!(o.target_lang.as_deref(), Some("de-DE"));
    assert_eq!(o.granularity, Granularity::Paragraph);
    assert!(o.include_locked);
    assert_eq!(o.limits().max_units, 7);
    assert_eq!(o.format_flag("ftl.translate_terms"), Some(true));
    // an Option clears or sets, so a caller can pass what it has
    let none: Option<&str> = None;
    assert_eq!(o.clone().with_target_lang(none).target_lang, None);
    assert_eq!(o.with_limits(None).limits, None);
    assert_eq!(ExtractOptions::new(), ExtractOptions::default());
}

#[test]
fn merge_options_and_limits_builders() {
    let m = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8).with_stamp_target_always(true);
    assert_eq!(m.output_encoding, OutputEncoding::Utf8);
    assert!(m.stamp_target_always);
    assert_eq!(MergeOptions::new(), MergeOptions::default());
    let l = Limits::new().with_max_input_bytes(10).with_max_zip_entries(3);
    assert_eq!((l.max_input_bytes, l.max_zip_entries, l.max_units), (10, 3, Limits::default().max_units));
}

#[test]
fn document_unit_segment_constructors() {
    let seg = Segment::new("u1", vec![Inline::text("Hi")]).with_target(vec![Inline::text("Hola")]).with_state(SegState::Translated).with_origin(Origin::new(OriginKind::Tm).with_match(100));
    assert_eq!(seg.origin.as_ref().and_then(|o| o.match_pct), Some(100));
    let u = Unit::from_segments("u1", vec![seg.clone(), Segment::new("u1#2", vec![])])
        .with_code("1", CodeData::default())
        .with_translate(false)
        .with_lock_reason("context match");
    assert_eq!(u.separators, vec![String::new()]);
    assert!(!u.translate && u.lock_reason.as_deref() == Some("context match") && u.codes.contains_key("1"));
    let opts = ExtractOptions::new().with_source_lang("en").with_target_lang("fr").with_granularity(Granularity::Document);
    let d = Document::new("text", vec![Unit::new("u2", vec![Inline::text("x")])], Skeleton::default()).with_options(&opts).with_name("a.txt");
    assert_eq!((d.format.as_str(), d.name.as_deref(), d.source_lang.as_deref(), d.target_lang.as_deref()), ("text", Some("a.txt"), Some("en"), Some("fr")));
    assert_eq!(d.granularity, Granularity::Document);
    assert_eq!(d.units[0].segments[0], Segment::new("u2", vec![Inline::text("x")]));
}
