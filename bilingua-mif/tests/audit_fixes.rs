//! Regression tests for the 0.9.1 security audit: pairing the font changes of one paragraph is bounded in time and
//! memory, and pairs ordinary font changes as before.
mod common;
use bilingua_mif::Mif;
use bilingua_model::Filter;
use common::*;
use std::time::{Duration, Instant};

fn para(body: &str) -> Vec<u8> {
    format!("<MIFFile 2019>\n<Para <ParaLine {body}<String `a'>>>\n").into_bytes()
}

/// Thousands of font changes in one paragraph, none restored. With a new property name in each, every change used
/// to copy the whole font state (10 s and 680 MB for 3000); with one property and a new value each, every change
/// was compared with all the earlier ones (6 s for 5000 in a debug build). The third file has one font of many
/// properties under many small changes.
#[test]
fn unrestored_font_changes_stay_cheap() {
    let new_name: String = (0..20_000).map(|i| format!("<Font <K{i} x>>")).collect();
    let same_name: String = (0..20_000).map(|i| format!("<Font <FSize {i}>>")).collect();
    let wide: String = (0..20_000).map(|i| format!("<K{i} x>")).collect();
    let one_wide = format!("<Font {wide}>{}", "<Font <FSize 1>><Font <FSize 2>>".repeat(10_000));
    for (what, body) in [("new property names", new_name), ("one property", same_name), ("one wide font", one_wide)] {
        let data = para(&body);
        let t = Instant::now();
        let doc = extract(&data).expect("extract");
        let took = t.elapsed();
        assert_eq!(doc.units.len(), 1, "{what}");
        assert!(took < Duration::from_secs(5), "{what}: {took:?}");
        assert!(Mif.merge(&doc).expect("merge") == data, "{what}: untouched merge");
    }
}

fn text_of(body: &str) -> String {
    let data = format!("<MIFFile 2019>\n<Para <ParaLine {body}>>\n");
    texts(data.as_bytes()).join("|")
}

fn g(n: u32) -> String {
    format!("\u{27E6}g{n}\u{27E7}")
}

fn end(n: u32) -> String {
    format!("\u{27E6}/g{n}\u{27E7}")
}

fn x(n: u32) -> String {
    format!("\u{27E6}x{n}\u{27E7}")
}

/// The pairing rules are unchanged: a change is paired with the statement that restores the font before it, by
/// naming that font or by resetting the changed properties to plain values; nested changes pair inside out; a
/// restore of an outer change leaves the inner one a point; a change never restored is a point.
#[test]
fn ordinary_font_changes_pair_as_before() {
    // reset to the plain value
    assert_eq!(
        text_of("<String `one '><Font <FWeight `Bold'>><String `two'><Font <FWeight `Regular'>><String ` three'>"),
        format!("one {}two{} three", g(1), end(1))
    );
    // back to the font before, by an empty Font
    assert_eq!(
        text_of("<String `one '><Font <FTag `Emphasis'>><String `two'><Font <FTag `'>><String ` three'>"),
        format!("one {}two{} three", g(1), end(1))
    );
    // nested, restored inside out
    assert_eq!(
        text_of("<String `a '><Font <FWeight `Bold'>><String `b '><Font <FAngle `Italic'>><String `c'><Font <FAngle `Regular'>><String ` d'><Font <FWeight `Regular'>><String ` e'>"),
        format!("a {}b {}c{} d{} e", g(1), g(2), end(2), end(1))
    );
    // the outer change restored while the inner one is open: the inner one is a point
    assert_eq!(
        text_of("<String `a '><Font <FWeight `Bold'>><String `b '><Font <FAngle `Italic'>><String `c'><Font <FTag `'>><String ` d'>"),
        format!("a {}b {}c{} d", g(1), x(2), end(1))
    );
    // never restored
    assert_eq!(text_of("<String `a '><Font <FSize 12 pt>><String `b'>"), format!("a {}b", x(1)));
    // a new value of the same property is a second change, and naming the first value again restores it
    assert_eq!(
        text_of("<String `a '><Font <FSize 12 pt>><String `b '><Font <FSize 14 pt>><String `c'><Font <FSize 12 pt>><String ` d'>"),
        format!("a {}b {}c{} d", x(1), g(2), end(2))
    );
}

/// More changes open at once than the pairing tracks: the first ones still pair with their restores.
#[test]
fn changes_past_the_open_limit_are_points() {
    let opens: String = (0..100).map(|i| format!("<Font <K{i} x>><String `w{i} '>")).collect();
    let body = format!("<String `a '><Font <FWeight `Bold'>><String `b '>{opens}<Font <FTag `'>><String `z'>");
    let t = text_of(&body);
    assert!(t.starts_with(&format!("a {}b ", g(1))), "{t}");
    assert!(t.ends_with(&format!("{}z", end(1))), "{t}");
}
