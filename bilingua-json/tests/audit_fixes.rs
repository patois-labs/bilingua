//! Regression tests for the 0.9.1 security audit.

use bilingua_model::{Document, Error, ExtractOptions, Filter, LimitKind, Limits};

fn tight() -> ExtractOptions {
    ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(100_000))
}

fn limit_kind(r: Result<Document, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => e.kind,
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("expected a limit error, the file was accepted"),
    }
}

fn context_bytes(doc: &Document) -> usize {
    doc.units.iter().map(|u| bilingua_model::guard::unit_context_bytes(&u.context) as usize).sum()
}

use bilingua_json::JsonFilter;

/// One long key over many values: every string value copies the pointer, and the copies count against the budget.
#[test]
fn pointer_copies_are_budgeted() {
    let json = format!("{{\"{}\": [{}\"x\"]}}", "k".repeat(2000), "\"x\", ".repeat(499));
    assert_eq!(limit_kind(JsonFilter.extract(json.as_bytes(), &tight())), LimitKind::Context);
    let doc = JsonFilter.extract(json.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 500);
    assert!(context_bytes(&doc) > 1_000_000);
}

/// A message dialect: every unit of one message (an ICU message has one per branch) copies the description.
#[test]
fn message_note_copies_are_budgeted() {
    let branches: String = (0..500).map(|i| format!("b{i} {{text {i}}} ")).collect();
    let json = format!(
        "{{\"greeting\": {{\"defaultMessage\": \"{{g, select, {branches}other {{text}}}}\", \"description\": \"{}\"}}}}",
        "d".repeat(2000)
    );
    let named = |o: ExtractOptions| o.with_format_option("json.dialect", "formatjs");
    assert_eq!(limit_kind(JsonFilter.extract(json.as_bytes(), &named(tight()))), LimitKind::Context);
    let doc = JsonFilter.extract(json.as_bytes(), &named(ExtractOptions::new())).unwrap();
    assert!(doc.units.len() > 500, "{} units", doc.units.len());
    assert!(doc.units.iter().all(|u| u.context.notes == ["d".repeat(2000)]));
}

/// go-i18n: the strings of one message share its description; ordinary files keep it on every unit.
#[test]
fn go_messages_keep_their_description_on_every_unit() {
    let json = r#"{"cats": {"description": "Number of cats", "one": "{{.Count}} cat", "other": "{{.Count}} cats"}}"#;
    let doc = JsonFilter.extract(json.as_bytes(), &ExtractOptions::new().with_format_option("json.dialect", "go")).unwrap();
    assert_eq!(doc.units.len(), 2);
    assert!(doc.units.iter().all(|u| u.context.notes == ["Number of cats"]));
}
