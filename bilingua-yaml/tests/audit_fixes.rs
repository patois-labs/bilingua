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

use bilingua_yaml::Yaml;
use std::time::Instant;

/// One long key over many values: every unit copies the key path, and the copies count against the budget.
#[test]
fn key_path_copies_are_budgeted() {
    let yaml = format!("{}: [{}]\n", "k".repeat(2000), "x, ".repeat(500));
    assert_eq!(limit_kind(Yaml.extract(yaml.as_bytes(), &tight())), LimitKind::Context);
    let doc = Yaml.extract(yaml.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 500);
    assert!(context_bytes(&doc) > 1_000_000);
}

fn commented_flow(k: usize, u: usize) -> String {
    format!("{}{{{}}}\n", "#a\n".repeat(k), "a: x, ".repeat(u))
}

/// Many comment lines above a one-line flow mapping: the block is read once, not once per value, and its copies
/// are charged, so a file whose notes would pass the budget is refused early.
#[test]
fn the_comment_above_a_flow_mapping_is_read_once_and_budgeted() {
    // 4000 values x 20000 comment lines: 80 million line reads before the fix (far over ten seconds in a debug build)
    let big = commented_flow(20_000, 4_000);
    let t = Instant::now();
    assert_eq!(limit_kind(Yaml.extract(big.as_bytes(), &tight())), LimitKind::Context);
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());

    // inside the default budget every unit still carries the note, as before
    let t = Instant::now();
    let doc = Yaml.extract(commented_flow(8_000, 2_000).as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
    assert_eq!(doc.units.len(), 2_000);
    assert!(doc.units.iter().all(|u| u.context.notes.len() == 1 && u.context.notes[0].len() == 2 * 8_000 - 1));
}

/// Ordinary files keep exactly the notes they had: one per unit under a commented key, the flow values of one
/// line each with the same note.
#[test]
fn notes_of_ordinary_files_are_unchanged() {
    let src = "# first\n# second\na: one\nb: two\n# flow\nc: {d: three, e: four}\n# seq\nf:\n  - five\n";
    let doc = Yaml.extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    let notes: Vec<Vec<String>> = doc.units.iter().map(|u| u.context.notes.clone()).collect();
    let n = |s: &str| vec![s.to_string()];
    // `d` and `e` sit on the line of `c`, so both read the comment above that line
    assert_eq!(notes, [n("first\nsecond"), vec![], n("flow"), n("flow"), vec![]]);
}

/// Many %TAG handles and many tagged values: each tag looks its handle up in a set, not in a list.
#[test]
fn tag_handles_are_looked_up_in_constant_time() {
    let n = 40_000;
    let mut src = String::new();
    for i in 0..n {
        src.push_str(&format!("%TAG !y{i}! x\n"));
    }
    src.push_str("%TAG !z! x\n--- [");
    src.push_str(&"!z!a,".repeat(n));
    src.push_str("]\n");
    let t = Instant::now();
    Yaml.extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    // 1.6 billion string comparisons before the fix
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

/// A handle declared twice in one document is a parse error, as in PyYAML and libyaml; a second document may
/// declare it again.
#[test]
fn a_duplicate_tag_handle_is_refused() {
    let e = Yaml.extract(b"%TAG !e! x\n%TAG !e! y\n--- !e!a b\n", &ExtractOptions::new()).unwrap_err();
    assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("duplicate %TAG")), "{e}");
    Yaml.extract(b"%TAG !e! x\n--- !e!a b\n...\n%TAG !e! y\n--- !e!a c\n", &ExtractOptions::new()).unwrap();
}
