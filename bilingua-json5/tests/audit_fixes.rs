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

use bilingua_json5::Json5Filter;

/// One long key over many values: every string copies the key path, and the copies count against the budget.
#[test]
fn key_path_copies_are_budgeted() {
    let src = format!("{{{}: [{}]}}", "k".repeat(2000), "'x', ".repeat(500));
    assert_eq!(limit_kind(Json5Filter::default().extract(src.as_bytes(), &tight())), LimitKind::Context);
    let doc = Json5Filter::default().extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 500);
    assert!(context_bytes(&doc) > 1_000_000);
}
