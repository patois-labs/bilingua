//! Regression tests for the PO fix of security review run 2 (G5-2): a plural entry with very many `msgstr[N]`
//! lines and no known target language is refused before its units are built, and the forms are found by index.

use bilingua_model::{Error, ExtractOptions, Filter, LimitKind, Limits};
use bilingua_po::Po;
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

fn plural_entry(forms: usize) -> String {
    let strs: String = (0..forms).map(|_| "msgstr[0] \"\"\n").collect();
    format!("msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\n\"\n\nmsgid \"a file\"\nmsgid_plural \"files\"\n{strs}")
}

/// 200,000 `msgstr[0]` lines: before the fix 200,000 units were built (each form found by a scan of all lines,
/// about 2 x 10^10 compares) before the unit cap was checked. Now the entry is refused at once.
#[test]
fn many_plural_forms_are_refused_before_building_units() {
    let po = plural_entry(200_000);
    let r = within(20, move || Po.extract(po.as_bytes(), &ExtractOptions::new()).map(|d| d.units.len()));
    match r {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units, "{e:?}"),
        other => panic!("expected a limit error, got {other:?}"),
    }
}

/// An ordinary plural entry still gives one unit per form, and the unit cap applies while forms are made.
#[test]
fn an_ordinary_plural_entry_still_opens() {
    let po = "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\n\"\n\nmsgid \"a file\"\nmsgid_plural \"files\"\nmsgstr[0] \"ein\"\nmsgstr[1] \"viele\"\nmsgstr[2] \"noch mehr\"\n";
    let doc = Po.extract(po.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(doc.units.len(), 3);
    let tight = ExtractOptions::new().with_limits(Limits::new().with_max_units(2));
    assert!(matches!(Po.extract(po.as_bytes(), &tight), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
}
