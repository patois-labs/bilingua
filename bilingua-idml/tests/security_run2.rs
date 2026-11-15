//! Regression test for the IDML fix of security review run 2: bare `Content` and `Br` children of a story are
//! wrapped in a range in one pass, and a merge splits that range at its breaks in one pass (moving one child at a
//! time scanned all the others each time).

mod common;
use bilingua_model::*;
use common::*;
use std::sync::mpsc;
use std::time::Duration;

fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(secs)).unwrap_or_else(|_| panic!("did not finish within {secs} s"))
}

/// One story with text and 100,000 bare `Br` children: about 10^10 element moves before the fix, at extract and
/// again at merge. The text is one unit and its translation is written.
#[test]
fn idml_many_bare_breaks_wrap_linear() {
    let body = format!("<Content>Hello there</Content>{}<Content>The end</Content>", "<Br/>".repeat(100_000));
    let bytes = package(body);
    let out = within(60, move || {
        let mut doc = extract(&bytes);
        let first = doc.units.iter().position(|u| plain_text(&u.segments[0].source) == "Hello there").expect("first unit");
        doc.units[first].segments[0].target = vec![Inline::text("Hallo zusammen")];
        doc.units[first].segments[0].state = SegState::Translated;
        (merge(&doc), doc.units.len())
    });
    let (merged, n) = out;
    let again = extract(&merged);
    assert_eq!(again.units.len(), n);
    assert!(again.units.iter().any(|u| plain_text(&u.segments[0].source) == "Hallo zusammen"));
    assert!(again.units.iter().any(|u| plain_text(&u.segments[0].source) == "The end"));
}

/// Bare runs between other children are wrapped in their own ranges, in place, as before.
#[test]
fn idml_bare_runs_keep_their_places() {
    let body = format!("<Content>One</Content><Br/>{}<Content>Two</Content>", csr("<Content>Mid</Content><Br/>"));
    let texts = units_of(&package(body));
    assert_eq!(texts.iter().map(|t| t.trim()).collect::<Vec<_>>(), ["One", "Mid", "Two"], "{texts:?}");
}
