//! Resource limit fixes from the security review (run 2): a long key over many values is refused while the units
//! are built, before every unit holds its own copy of the key.

mod peak;
use bilingua_model::*;
use bilingua_php::Php;

#[test]
fn long_key_over_many_values_is_refused() {
    let key = "k".repeat(100_000);
    let entries: String = (0..2_000).map(|i| format!("'a{i}' => 'x', ")).collect();
    let src = format!("<?php\nreturn ['{key}' => [{entries}]];\n");
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(8 << 20));
    let (r, bytes) = peak::peak(|| Php.extract(src.as_bytes(), &o));
    assert!(matches!(r, Err(Error::Limit(ref e)) if e.kind == LimitKind::Context), "{r:?}");
    assert!(bytes < 64 << 20, "{bytes} bytes held");
    // a long key over a few values still reads
    let small: String = (0..50).map(|i| format!("'a{i}' => 'x', ")).collect();
    let src = format!("<?php\nreturn ['{key}' => [{small}]];\n");
    assert_eq!(Php.extract(src.as_bytes(), &ExtractOptions::default()).unwrap().units.len(), 50);
}
