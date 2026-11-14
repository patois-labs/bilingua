//! Regression tests for the findings of the 0.9.1 source audit.

use bilingua_model::*;
use bilingua_qtts::{Limits, QtTs};
use std::time::{Duration, Instant};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en")
}

const ONE_MESSAGE: &str = "<context><name>c</name><message><source>a</source><translation/></message></context></TS>";

fn many_attrs(n: usize, extra: &str) -> Vec<u8> {
    let mut s = String::from("<TS version=\"2.1\"");
    for i in 0..n {
        s.push_str(&format!(" a{i:07}=\"\""));
    }
    s.push_str(extra);
    s.push('>');
    s.push_str(ONE_MESSAGE);
    s.into_bytes()
}

/// `bilingua-xmb/src/scan.rs:start_tag:linear-duplicate-attribute-scan` (the same check in `xml.rs`): the
/// duplicate-attribute check compared each name with every earlier one of the tag.
#[test]
fn a_tag_with_100_000_attributes_is_read_in_linear_time() {
    let bytes = many_attrs(100_000, "");
    let t = Instant::now();
    let doc = QtTs.extract(&bytes, &opts()).expect("a well-formed file");
    assert_eq!(doc.units.len(), 1);
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
}

/// The set-based check still refuses a repeated name, early or late in a long tag, and in a short tag.
#[test]
fn a_repeated_attribute_is_still_refused() {
    for bytes in [many_attrs(5_000, " a0000000=\"\""), many_attrs(5_000, " a0004999=\"\""), many_attrs(3, " a0000001=\"\""), many_attrs(8, " a0000007=\"\"")] {
        let e = QtTs.extract(&bytes, &opts()).expect_err("a repeated attribute");
        assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("appears twice")), "{e:?}");
    }
    assert!(QtTs.extract(&many_attrs(9, ""), &opts()).is_ok());
}

/// Thousands of namespace declarations in scope and as many prefixed elements: the "is this prefix declared"
/// test is one lookup, not a search of every declaration.
#[test]
fn many_namespace_prefixes_in_scope_are_looked_up_in_constant_time() {
    let n = 40_000;
    let mut s = String::from("<TS version=\"2.1\" xmlns:first=\"u\"");
    for i in 0..n {
        s.push_str(&format!(" xmlns:p{i}=\"u\""));
    }
    s.push('>');
    for _ in 0..n {
        s.push_str("<first:x/>");
    }
    s.push_str(ONE_MESSAGE);
    let t = Instant::now();
    let doc = QtTs.extract(s.as_bytes(), &opts()).expect("a well-formed file");
    assert_eq!(doc.units.len(), 1);
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
    // scoping is unchanged: a prefix is gone when its element closes, and a redeclared one survives the inner close
    let gone = format!("<TS><a xmlns:p=\"u\"/><p:b/>{ONE_MESSAGE}");
    let e = QtTs.extract(gone.as_bytes(), &opts()).expect_err("p is out of scope");
    assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("namespace prefix p is not defined")), "{e:?}");
    let kept = format!("<TS xmlns:p=\"u\"><a xmlns:p=\"v\"><p:c/></a><p:b/>{ONE_MESSAGE}");
    assert_eq!(QtTs.extract(kept.as_bytes(), &opts()).expect("p is still declared").units.len(), 1);
}

fn long_name(name_len: usize, messages: usize) -> Vec<u8> {
    let mut s = String::from("<TS version=\"2.1\"><context><name>");
    s.push_str(&"N".repeat(name_len));
    s.push_str("</name>");
    for _ in 0..messages {
        s.push_str("<message><source>a</source><translation/></message>");
    }
    s.push_str("</context></TS>");
    s.into_bytes()
}

/// `bilingua-qtts/src/lib.rs:message:context-name-per-unit-copy`: a 1 MB file with a 20 000 byte context name
/// and 20 000 messages gave a Document holding 400 MB of name copies. The copies now count against
/// `max_context_bytes` before they are made.
#[test]
fn a_long_context_name_over_many_messages_is_refused() {
    let e = QtTs.extract(&long_name(20_000, 20_000), &opts()).expect_err("400 MB of name copies");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Context), "{e:?}");
    // the same shape under a small cap: 100 copies of 1 000 bytes do not fit 50 000 bytes
    let small = Limits::default().with_max_context_bytes(50_000);
    let e = QtTs.extract_with(&long_name(1_000, 100), &opts(), &small).expect_err("over the context cap");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Context), "{e:?}");
    // an ordinary file is untouched: every unit still carries the name
    let doc = QtTs.extract(&long_name(40, 50), &opts()).expect("an ordinary file");
    assert_eq!(doc.units.len(), 50);
    assert!(doc.units.iter().all(|u| u.context.extra.get("context").is_some_and(|c| c.len() == 40)));
}
