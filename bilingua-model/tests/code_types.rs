//! BILI-118 (unit model step 5): typed inline codes. `CodeData.ty` (JSON `type`) and `CodeData.attrs` are left
//! out of the JSON when empty, so a code without them writes exactly what it wrote before; an unknown type reads
//! as `other`; the LocOS profile clears both.

use bilingua_model::*;

#[test]
fn untyped_code_json_is_unchanged() {
    let c = CodeData::paired("<b>", "</b>").with_label("bold");
    assert_eq!(
        serde_json::to_string(&c).unwrap(),
        r#"{"open":"<b>","close":"</b>","standalone":null,"label":"bold"}"#
    );
    assert_eq!(serde_json::to_string(&CodeData::new()).unwrap(), r#"{"open":null,"close":null,"standalone":null,"label":null}"#);
    // and the old JSON reads back to the same code
    let back: CodeData = serde_json::from_str(r#"{"open":"<b>","close":"</b>","standalone":null,"label":"bold"}"#).unwrap();
    assert_eq!(back, c);
    assert_eq!((back.code_type(), back.attrs().is_empty()), (None, true));
}

#[test]
fn typed_code_round_trips() {
    let c = CodeData::paired("<a href=\"https://x.com\">", "</a>")
        .with_label("link")
        .with_code_type(Some(CodeType::Link))
        .with_attr("title", "X")
        .with_attr("href", "https://x.com");
    let j = serde_json::to_string(&c).unwrap();
    assert_eq!(
        j,
        r#"{"open":"<a href=\"https://x.com\">","close":"</a>","standalone":null,"label":"link","type":"link","attrs":{"href":"https://x.com","title":"X"}}"#
    );
    let back: CodeData = serde_json::from_str(&j).unwrap();
    assert_eq!(back, c);
    assert_eq!(back.attr("href"), Some("https://x.com"));
}

#[test]
fn unknown_type_reads_as_other() {
    let c: CodeData = serde_json::from_str(r#"{"open":null,"close":null,"standalone":"x","label":null,"type":"hologram"}"#).unwrap();
    assert_eq!(c.code_type(), Some(CodeType::Other));
    assert_eq!(CodeType::from_name("hologram"), CodeType::Other);
}

#[test]
fn every_type_has_its_lowercase_name() {
    let names = [
        "bold", "italic", "underline", "strike", "superscript", "subscript", "code", "style", "link", "image", "linebreak", "tab", "field",
        "footnoteref", "placeholder", "other",
    ];
    assert_eq!(CodeType::ALL.len(), names.len());
    for (t, n) in CodeType::ALL.iter().zip(names) {
        assert_eq!(t.as_str(), n);
        assert_eq!(serde_json::to_string(t).unwrap(), format!("\"{n}\""));
        assert_eq!(serde_json::from_str::<CodeType>(&format!("\"{n}\"")).unwrap(), *t);
        assert_eq!(CodeType::from_name(n), *t);
    }
    assert_eq!(CodeType::Link.attr_keys(), &["href", "title"]);
    assert_eq!(CodeType::Image.attr_keys(), &["src", "alt"]);
    assert_eq!(CodeType::Field.attr_keys(), &["field"]);
    assert_eq!(CodeType::Style.attr_keys(), &["style"]);
    assert_eq!(CodeType::Placeholder.attr_keys(), &["name"]);
    assert!(CodeType::Bold.attr_keys().is_empty());
}

#[test]
fn setters() {
    let mut c = CodeData::standalone("<br/>");
    c.set_code_type(Some(CodeType::Linebreak));
    c.set_attr("name", "");
    assert!(c.attrs().is_empty(), "an empty value is not stored");
    c.attrs_mut().insert("name", "n");
    assert_eq!(c.attr("name"), Some("n"));
    c.set_attr("name", "");
    assert_eq!(c.attr("name"), None, "an empty value removes the key");
    c.set_attr("name", "n");
    c.clear_typing();
    assert_eq!((c.code_type(), c.attrs().len()), (None, 0));
}

#[test]
fn locos_profile_clears_types() {
    let mut doc = Document::new("docx", vec![Unit::new("1", vec![Inline::text("x")]).with_code(
        "1",
        CodeData::paired("<b>", "</b>").with_label("bold").with_code_type(Some(CodeType::Bold)).with_attr("style", "S"),
    )], Skeleton::default());
    let mut other = doc.clone();
    profile::annotate(&mut other, None);
    assert_eq!(other.units[0].codes()["1"].code_type(), Some(CodeType::Bold), "other profiles keep them");
    profile::annotate(&mut doc, Some(profile::LOCOS));
    let c = &doc.units[0].codes()["1"];
    assert_eq!((c.code_type(), c.attrs().len(), c.label()), (None, 0, Some("bold")), "the label stays");
}
