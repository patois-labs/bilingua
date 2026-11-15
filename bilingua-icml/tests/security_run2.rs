//! Regression tests for the ICML follow-up of security review run 2 (G1-F8): bare `Content` and `Br` children of a
//! paragraph range are wrapped in a character range in one pass (moving one child at a time scanned all the others
//! each time, and every move walked to the root).

mod common;

use bilingua_icml::Icml;
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

fn plain(u: &Unit) -> String {
    u.source().iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect()
}

/// One paragraph range with text and 100,000 bare `Br` children: about 10^10 element moves before the fix, at
/// extract and again at merge. The first text is one unit and its translation is written.
#[test]
fn icml_many_bare_breaks_wrap_linear() {
    let body = psr(&format!("<Content>Hello there</Content>{}<Content>The end</Content>", "<Br/>".repeat(100_000)));
    let data = icml(&body, "", "");
    let (merged, n) = within(60, move || {
        let d = extract(&data);
        let first = d.units.iter().find(|u| plain(u) == "Hello there").map(p_index).expect("first unit");
        (translate(&data, &[(first, "Hallo zusammen")]), d.units.len())
    });
    let again = extract(&merged);
    assert_eq!(again.units.len(), n);
    assert!(again.units.iter().any(|u| plain(u) == "Hallo zusammen"));
    assert!(again.units.iter().any(|u| plain(u) == "The end"));
}

/// Bare runs between other children are wrapped in their own ranges, in place, and keep the text after them.
#[test]
fn icml_bare_runs_keep_their_places() {
    let body = psr(&format!("<Content>One</Content>\n<Br/>\n{}\n<Content>Two</Content>\n", csr("<Content>Mid</Content><Br/>")));
    let data = icml(&body, "", "");
    let d = extract(&data);
    let texts: Vec<String> = d.units.iter().map(plain).collect();
    assert_eq!(texts.iter().map(|t| t.trim()).collect::<Vec<_>>(), ["One", "Mid", "Two"], "{texts:?}");
    let two = d.units.iter().find(|u| plain(u) == "Two").map(p_index).unwrap();
    let out = String::from_utf8(translate(&data, &[(two, "Zwei")])).unwrap();
    let (a, b, c) = (out.find(">One<").unwrap(), out.find(">Mid<").unwrap(), out.find(">Zwei<").unwrap());
    assert!(a < b && b < c, "{out}");
    let again: Vec<String> = Icml.extract(out.as_bytes(), &opts()).unwrap().units.iter().map(plain).collect();
    assert_eq!(again.iter().map(|t| t.trim()).collect::<Vec<_>>(), ["One", "Mid", "Zwei"], "{out}");
}
