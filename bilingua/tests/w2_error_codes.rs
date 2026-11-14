//! W2 B-side (11-07): through the facade, size, budget and nesting refusals are code `limit` and byte-level decode
//! refusals code `encoding`, in the wave 2 formats and the others that raised them as `unsupported` or `parse`.

use bilingua::{extract, Error, ExtractOptions, Limits};

fn small(units: usize) -> ExtractOptions {
    let mut o = ExtractOptions::default();
    o.limits = Some(Limits::new().with_max_units(units));
    o
}

fn is_limit(r: Result<bilingua::Document, Error>) -> bool {
    matches!(r, Err(Error::Limit(_)))
}

fn is_encoding(r: Result<bilingua::Document, Error>) -> bool {
    matches!(r, Err(Error::Encoding(_)))
}

#[test]
fn budgets_are_limit_errors() {
    let o = small(3);
    assert!(is_limit(extract("a.ftl", format!("x = {}\n", "{ $a } ".repeat(100)).as_bytes(), &o)));
    assert!(is_limit(extract("a.toml", format!("a = \"{}\"\n", "'".repeat(200)).as_bytes(), &o)));
    assert!(is_limit(extract("a.toml", format!("a = [{}]\n", vec!["1"; 100].join(",")).as_bytes(), &o)));
    assert!(is_limit(extract("a.arb", format!("{{{}}}", (0..200).map(|i| format!("\"k{i}\": \"\"")).collect::<Vec<_>>().join(", ")).as_bytes(), &o)));
    assert!(is_limit(extract("a.json5", format!("[{}]", "''".repeat(200)).as_bytes(), &o)));
    assert!(is_limit(extract("a.resx", format!("<root>{}</root>", "<x/>".repeat(300)).as_bytes(), &o)));
    let o5 = small(5);
    assert!(is_limit(extract("a.js", format!("export default [{}]", "'',".repeat(300)).as_bytes(), &o5)));
    assert!(is_limit(extract("a.js", format!("export default {{}}{}", ";".repeat(400)).as_bytes(), &o5)));
    let deep = format!("<?php return {}'x'{};", "['a' => ".repeat(40), "]".repeat(40));
    assert!(is_limit(extract("a.php", deep.as_bytes(), &ExtractOptions::default())));
}

#[test]
fn decode_refusals_are_encoding_errors() {
    let o = ExtractOptions::default();
    assert!(is_encoding(extract("a.properties", &b"k = v\x00\x01\x02\x03\x04\x05".repeat(50), &o)));
    assert!(is_encoding(extract("a.dtd", b"<!ENTITY a \"caf\xe9\">", &o)));
    assert!(is_encoding(extract("a.ftl", b"hello = caf\xe9\n", &o)));
    assert!(is_encoding(extract("a.toml", b"a = 'caf\xe9'", &o)));
    assert!(is_encoding(extract("a.arb", b"{\"a\": \"caf\xe9\"}", &o)));
    assert!(is_encoding(extract("a.resjson", b"{\"a\": \"caf\xe9\"}", &o)));
    assert!(is_encoding(extract("a.strings", b"\xe9=\xe9;", &o)));
}
