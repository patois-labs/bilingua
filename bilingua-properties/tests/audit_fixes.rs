//! Resource limit fixes from the security review (run 2): placeholder scanning stays linear, and the caller's
//! limits apply inside the filter.

use bilingua_model::*;
use bilingua_properties::Properties;

fn ex(text: &str, o: &ExtractOptions) -> Result<Document, Error> {
    Properties.extract(text.as_bytes(), o)
}

#[test]
fn unclosed_messageformat_args_are_linear() {
    // `{0,` many times with no closing brace: each argument scan must not run to the end of the value again
    let o = ExtractOptions::default();
    let work = |k: usize| bilingua_model::work::count(|| ex(&format!("a = x {}", "{0,".repeat(k)), &o)).1;
    let n = 4_000;
    let (small, big) = (work(n), work(2 * n));
    assert!(small > 0 && big * 10 <= small * 25 + 2_000, "{small} steps for n={n}, {big} for n={}: more than linear", 2 * n);
    // nested and closed arguments still read as one placeholder
    let d = ex("a = You have {0,choice,0#none|1<{0} items} left\n", &o).unwrap();
    assert_eq!(d.units.len(), 1);
    assert!(d.units[0].source().iter().any(|i| matches!(i, Inline::Tag { .. })));
}

#[test]
fn caller_limits_apply_inside_the_filter() {
    let long = format!("a = {}\n", "word ".repeat(400));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_unit_chars(1_000));
    assert!(matches!(ex(&long, &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
    let many: String = (0..30).map(|i| format!("k{i} = value {i}\n")).collect();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(20));
    assert!(matches!(ex(&many, &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(100));
    assert!(matches!(ex(&many, &o), Err(Error::Limit(e)) if e.kind == LimitKind::InputBytes));
    // the defaults still read both files
    assert!(ex(&long, &ExtractOptions::default()).is_ok());
    assert!(ex(&many, &ExtractOptions::default()).is_ok());
}
