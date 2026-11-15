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

/// One paragraph range with text and 100,000 bare `Br` children, wrapped in one range at extract and again at
/// merge. The first text is one unit and its translation is written.
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

/// 60,000 bare runs, each between two ranges (a break after every 40): every run was looked up among all the
/// children of the paragraph range several times over (bare runs x children), and every move walked to the root.
#[test]
fn icml_bare_runs_between_ranges_wrap_linear() {
    let n = 60_000usize;
    let body: String = (0..n)
        .map(|k| {
            let br = if k % 40 == 39 { "<Br/>" } else { "" };
            format!("<Content>a{k} </Content>\n{}\n", csr(&format!("<Content>b{k} </Content>{br}")))
        })
        .collect();
    let data = icml(&psr(&body), "", "");
    let (merged, units) = within(60, move || {
        let d = extract(&data);
        let last = p_index(d.units.last().expect("units"));
        (translate(&data, &[(last, "Ende")]), d.units.len())
    });
    let again: Vec<String> = extract(&merged).units.iter().map(plain).collect();
    assert_eq!(again.len(), units);
    assert!(again[0].starts_with("a0 b0 a1 b1 "), "{}", again[0]);
    assert_eq!(again[units - 1], "Ende");
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
