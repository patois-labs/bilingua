//! Regression tests for the findings of the 0.9.1 source audit.

use bilingua_model::*;
use bilingua_xmb::Xmb;
use std::time::{Duration, Instant};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en")
}

fn many_attrs(n: usize, extra: &str) -> Vec<u8> {
    let mut s = String::from("<messagebundle");
    for i in 0..n {
        s.push_str(&format!(" a{i:07}=\"\""));
    }
    s.push_str(extra);
    s.push_str("><msg id=\"1\">Hello</msg></messagebundle>");
    s.into_bytes()
}

/// `bilingua-xmb/src/scan.rs:start_tag:linear-duplicate-attribute-scan`: the duplicate-attribute check compared
/// each name with every earlier one of the tag, so 100 000 attributes on one tag took a very long time.
#[test]
fn a_tag_with_100_000_attributes_is_read_in_linear_time() {
    let bytes = many_attrs(100_000, "");
    let t = Instant::now();
    let doc = Xmb.extract(&bytes, &opts()).expect("a well-formed bundle");
    assert_eq!(doc.units.len(), 1);
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
}

/// The set-based check still refuses a repeated name, early or late in a long tag, and in a short tag.
#[test]
fn a_repeated_attribute_is_still_refused() {
    for bytes in [many_attrs(5_000, " a0000000=\"\""), many_attrs(5_000, " a0004999=\"\""), many_attrs(3, " a0000001=\"\""), many_attrs(8, " a0000007=\"\"")] {
        let e = Xmb.extract(&bytes, &opts()).expect_err("a repeated attribute");
        assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("repeated")), "{e:?}");
    }
    assert!(Xmb.extract(&many_attrs(9, ""), &opts()).is_ok());
}
