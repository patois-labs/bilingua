//! Resource limit fixes from the security review (run 2): the parser bounds its own nesting, so string literals
//! that hold braces or calls nested inside calls cannot make it recurse past the stack.

use bilingua_ftl::FtlFilter;
use bilingua_model::*;

fn on_small_stack(src: String) -> Result<Document, Error> {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || FtlFilter.extract(src.as_bytes(), &ExtractOptions::default()))
        .unwrap()
        .join()
        .expect("the parse must not overflow the stack")
}

#[test]
fn braces_in_strings_do_not_bypass_nesting_limit() {
    // each level opens a placeable and a call whose first argument is the string "}"
    let src = format!("x = {}\n", "{F(\"}\", ".repeat(100_000));
    assert!(matches!(on_small_stack(src), Err(Error::Limit(e)) if e.kind == LimitKind::Depth));
    // calls nested inside calls, with no braces at all after the first
    let src = format!("x = {{ {}\n", "F(".repeat(100_000));
    assert!(matches!(on_small_stack(src), Err(Error::Limit(e)) if e.kind == LimitKind::Depth));
    // ordinary nesting still reads
    let src = "x = { F(\"}\", { G(1) }) } and { $n -> \n    [one] { NUMBER($n) } item\n   *[other] items\n}\n".to_string();
    assert!(!on_small_stack(src).unwrap().units.is_empty());
}
