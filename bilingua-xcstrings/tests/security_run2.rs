//! Regression test for the xcstrings fix of security review run 2 (G5-5): the merge mirrors a variation map with
//! one lookup per category, not a scan of the whole map.

use bilingua_model::*;
use bilingua_xcstrings::Xcstrings;
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

/// One string whose source has a string unit and a device variation of 300,000 categories (values that make no
/// units): translating the string unit made the merge search the map twice per category (about 10^11 compares).
#[test]
fn a_variation_with_many_categories_merges_in_linear_time() {
    let cats: String = (0..300_000).map(|i| format!("\"k{i}\":1,")).collect::<String>().trim_end_matches(',').to_string();
    let json = format!(
        "{{\"sourceLanguage\":\"en\",\"strings\":{{\"greeting\":{{\"localizations\":{{\"en\":{{\"stringUnit\":{{\"state\":\"translated\",\"value\":\"Hello there\"}},\"variations\":{{\"device\":{{{cats}}}}}}}}}}}}},\"version\":\"1.0\"}}"
    );
    let out = within(30, move || {
        let mut doc = Xcstrings.extract(json.as_bytes(), &ExtractOptions::new().with_target_lang("de")).unwrap();
        let u = doc.units.iter_mut().find(|u| plain_text(&u.segments[0].source) == "Hello there").expect("the string unit");
        u.segments[0].target = vec![Inline::text("Hallo zusammen")];
        u.segments[0].state = SegState::Translated;
        Xcstrings.merge(&doc).unwrap()
    });
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("Hallo zusammen"));
}
