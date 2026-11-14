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

use bilingua_ini::IniFilter;

/// One long section name over many keys: every unit copies the name, and the copies count against the budget.
#[test]
fn section_name_copies_are_budgeted() {
    let src = format!("[{}]\n{}", "s".repeat(2000), (0..500).map(|i| format!("k{i}=word\n")).collect::<String>());
    assert_eq!(limit_kind(IniFilter.extract(src.as_bytes(), &tight())), LimitKind::Context);
    let doc = IniFilter.extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 500);
    assert!(context_bytes(&doc) > 1_000_000);
}
