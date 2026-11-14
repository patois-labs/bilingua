//! Regression tests for the findings of the 0.9.1 source audit.

use bilingua_model::*;
use bilingua_xslt::Xslt;
use std::time::{Duration, Instant};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en")
}

fn many_attrs(n: usize, extra: &str) -> Vec<u8> {
    let mut s = String::from("<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"");
    for i in 0..n {
        s.push_str(&format!(" a{i:07}=\"\""));
    }
    s.push_str(extra);
    s.push_str("><xsl:template match=\"/\"><p>Hello</p></xsl:template></xsl:stylesheet>");
    s.into_bytes()
}

/// `bilingua-xmb/src/scan.rs:start_tag:linear-duplicate-attribute-scan` (the same check in `lex.rs`): the
/// duplicate-attribute check compared each name with every earlier one of the tag.
#[test]
fn a_tag_with_100_000_attributes_is_read_in_linear_time() {
    let bytes = many_attrs(100_000, "");
    let t = Instant::now();
    let doc = Xslt.extract(&bytes, &opts()).expect("a well-formed stylesheet");
    assert_eq!(doc.units.len(), 1);
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
}

/// The set-based check still refuses a repeated name, early or late in a long tag, and in a short tag.
#[test]
fn a_repeated_attribute_is_still_refused() {
    for bytes in [many_attrs(5_000, " a0000000=\"\""), many_attrs(5_000, " a0004999=\"\""), many_attrs(3, " a0000001=\"\""), many_attrs(8, " a0000007=\"\"")] {
        let e = Xslt.extract(&bytes, &opts()).expect_err("a repeated attribute");
        assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("duplicate attribute")), "{e:?}");
    }
    assert!(Xslt.extract(&many_attrs(9, ""), &opts()).is_ok());
}
