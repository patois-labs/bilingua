//! Regression test for lead L2 of security review run 2: a message's description is copied into each of its plural
//! units, and every copy counts against the caller's context budget.

use bilingua_arb::Arb;
use bilingua_model::*;

fn arb(branches: usize, note: usize) -> String {
    let arms: String = (0..branches).map(|i| format!(" ={i}{{item number {i}}}")).collect();
    format!("{{\"@@locale\": \"en\", \"items\": \"{{count, plural,{arms} other{{many items}}}}\", \"@items\": {{\"description\": \"{}\"}}}}", "n".repeat(note))
}

#[test]
fn plural_copies_of_a_description_are_budgeted() {
    let text = arb(300, 10_000);
    let doc = Arb.extract(text.as_bytes(), &ExtractOptions::new()).unwrap();
    let units = doc.units.len();
    assert!(units > 100, "{units} units");
    assert!(doc.units.iter().all(|u| u.context.notes.iter().any(|n| n.len() >= 10_000)));
    // 300 copies of a 10 KB description are 3 MB of notes: over a 1 MB budget
    let tight = ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(1_000_000));
    match Arb.extract(text.as_bytes(), &tight) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Context),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(d) => panic!("accepted with {} units", d.units.len()),
    }
}
