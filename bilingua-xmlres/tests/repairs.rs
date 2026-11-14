//! Gate cases (wave 12): a translation can never write broken XML (LocOS `_rebuild_target` repair rules), a Java
//! entry with a comment is left alone (lxml counts the comment as a child), plural and array items carry the comment
//! before their group, the #706 break rewrite and the plural fill stay linear, and a UTF-8 merge of a legacy file
//! says UTF-8 in its declaration.

use bilingua_model::*;
use bilingua_xmlres::{AndroidStrings, JavaXmlProperties};

const STYLED: &[u8] = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources xmlns:xliff=\"urn:oasis:names:tc:xliff:document:1.2\">\n    <string name=\"s\">Tap <b>Save</b> or <i>Undo</i> for <xliff:g id=\"n\">%1$s</xliff:g>.</string>\n</resources>\n";

fn tags(d: &Document) -> (String, String, String) {
    let u = &d.units[0];
    let id = |k: TagKind, raw: &str| {
        u.source()
            .iter()
            .find_map(|i| match i {
                Inline::Tag { id, kind, display, .. } if *kind == k && display.starts_with(raw) => Some(id.clone()),
                _ => None,
            })
            .unwrap()
    };
    (id(TagKind::Open, "<b>"), id(TagKind::Open, "<i>"), id(TagKind::Standalone, "<xliff:g"))
}

fn merged(target: Vec<Inline>) -> String {
    let mut d = AndroidStrings.extract(STYLED, &ExtractOptions::default()).unwrap();
    // the JSON route skips set_target's tag check, as a stored document edited elsewhere would
    d.units[0].segments[0].target = target;
    let out = String::from_utf8(AndroidStrings.merge(&d).unwrap()).unwrap();
    AndroidStrings.extract(out.as_bytes(), &ExtractOptions::default()).expect("the merge reads back as XML");
    out.lines().nth(2).unwrap().trim().to_string()
}

#[test]
fn a_translation_never_writes_broken_markup() {
    let d = AndroidStrings.extract(STYLED, &ExtractOptions::default()).unwrap();
    let (b, i, g) = tags(&d);
    let t = Inline::text;
    let open = |id: &str| Inline::tag(id, TagKind::Open, "");
    let close = |id: &str| Inline::tag(id, TagKind::Close, "");
    let point = |id: &str| Inline::tag(id, TagKind::Standalone, "");
    // a close before its open: the stray close is dropped, the open is closed at the end (LocOS: `x<b> y</b>`)
    assert_eq!(merged(vec![close(&b), t("x"), open(&b), t(" y")]), r#"<string name="s">x<b> y</b></string>"#);
    // crossed pairs nest
    assert_eq!(merged(vec![open(&b), t("a"), open(&i), t("b"), close(&b), t("c"), close(&i)]), r#"<string name="s"><b>a<i>bc</i></b></string>"#);
    // a pair or a point used twice is written once; an unknown id is dropped
    assert_eq!(
        merged(vec![open(&b), t("a"), close(&b), open(&b), t("b"), close(&b), point(&g), point(&g), point("99"), t(".")]),
        r#"<string name="s"><b>a</b>b<xliff:g id="n">%1$s</xliff:g>.</string>"#
    );
}

#[test]
fn a_java_entry_with_a_comment_is_left_alone() {
    let data = b"<?xml version=\"1.0\"?>\n<properties>\n<entry key=\"a\">Hello <!-- c --> world</entry>\n<entry key=\"b\">Plain <?pi x?> value</entry>\n<entry key=\"c\">Plain value</entry>\n</properties>\n";
    let d = JavaXmlProperties.extract(data, &ExtractOptions::default()).unwrap();
    // LocOS: ['Plain value']
    let texts: Vec<String> = d.units.iter().filter(|u| u.translate).map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, ["Plain value"]);
    assert_eq!(JavaXmlProperties.merge(&d).unwrap(), data);
}

#[test]
fn plural_and_array_items_carry_the_group_comment() {
    let data = b"<resources>\n  <!-- Songs on the device -->\n  <plurals name=\"songs\"><item quantity=\"one\">%d song</item><item quantity=\"other\">%d songs</item></plurals>\n  <!-- Planets -->\n  <string-array name=\"p\"><item>Mars</item></string-array>\n  <string name=\"x\">No comment</string>\n</resources>\n";
    let d = AndroidStrings.extract(data, &ExtractOptions::default()).unwrap();
    let notes: Vec<Vec<String>> = d.units.iter().map(|u| u.context.notes.clone()).collect();
    assert_eq!(notes[0], ["Songs on the device"]);
    assert_eq!(notes[1], ["Songs on the device"]);
    assert_eq!(notes[2], ["Planets"]);
    assert!(notes[3].is_empty());
}

#[test]
fn long_runs_of_spaces_and_many_plural_groups_stay_linear() {
    // #706 rewrite over 2 MB of spaces with no break (was one scan per position)
    let data = b"<resources>\n<string name=\"s\">One\\nTwo</string>\n</resources>\n";
    let mut d = AndroidStrings.extract(data, &ExtractOptions::default()).unwrap();
    let id = d.units[0].segments[0].id.clone();
    let tr = format!("a{}b\nc", " ".repeat(2_000_000));
    d.set_target(&id, vec![Inline::text(tr)]).unwrap();
    let t = std::time::Instant::now();
    let out = String::from_utf8(AndroidStrings.merge(&d).unwrap()).unwrap();
    assert!(out.contains("b\\nc"));
    assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    // 60 000 plural groups, each gaining two forms for Polish
    let mut s = String::from("<resources>\n");
    for i in 0..60_000 {
        s.push_str(&format!("  <plurals name=\"p{i}\">\n    <item quantity=\"one\">%d x</item>\n    <item quantity=\"other\">%d xs</item>\n  </plurals>\n"));
    }
    s.push_str("</resources>\n");
    let o = ExtractOptions::new().with_target_lang("pl").with_limits(Limits::default().with_max_units(1_000_000));
    let d = AndroidStrings.extract(s.as_bytes(), &o).unwrap();
    let t = std::time::Instant::now();
    let out = AndroidStrings.merge_with(&d, &MergeOptions::new().with_stamp_target_always(true)).unwrap();
    assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    assert_eq!(String::from_utf8(out).unwrap().matches("quantity=\"many\"").count(), 60_000);
}

#[test]
fn a_utf8_merge_of_a_legacy_file_declares_utf8() {
    let data = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<resources>\n<string name=\"s\">Caf\xe9</string>\n</resources>\n";
    let mut d = AndroidStrings.extract(data, &ExtractOptions::default()).unwrap();
    assert_eq!(AndroidStrings.merge(&d).unwrap(), data, "untouched merge keeps the code page");
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Caf\u{e9} cr\u{e8}me")]).unwrap();
    let out = AndroidStrings.merge_with(&d, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)).unwrap();
    let s = String::from_utf8(out).expect("UTF-8 out");
    assert!(s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{s}");
    assert!(s.contains("Caf\u{e9} cr\u{e8}me"));
}
