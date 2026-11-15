//! Resource limits (security review run 2): namespace prefixes and URIs are looked up through indexes, and the
//! declarations in scope at one element are bounded.

mod common;

use bilingua_icml::Icml;
use bilingua_model::*;
use common::*;
use std::time::{Duration, Instant};

fn with_decls(k: usize, filler: usize) -> Vec<u8> {
    let decls: String = (0..k).map(|i| format!(" xmlns:p{i}=\"urn:u{i}\"")).collect();
    let head = HEAD.replacen("<Document DOMVersion=\"20.0\" Self=\"d\">", &format!("<Document DOMVersion=\"20.0\" Self=\"d\"{decls}>"), 1);
    let body = psr(&csr("<Content>Hello world</Content>"));
    format!("{head}{}{}</Document>\n", "<a/>".repeat(filler), story("u1", &body)).into_bytes()
}

/// Tens of thousands of declarations on the root: refused with an error (every element looked its prefix up
/// through all of them, and every URI was interned by a scan).
#[test]
fn too_many_namespace_declarations_are_refused() {
    let r = Icml.extract(&with_decls(20_000, 10), &opts());
    assert!(matches!(&r, Err(Error::Parse { msg, .. }) if msg.contains("namespace declarations")), "{:?}", r.map(|d| d.units.len()));
}

/// A thousand declarations above a quarter of a million elements read and write back in linear time.
#[test]
fn many_namespace_declarations_are_linear() {
    let data = with_decls(1000, 250_000);
    let start = Instant::now();
    let d = Icml.extract(&data, &opts()).expect("extract");
    assert_eq!(d.units.len(), 1);
    let out = translate(&data, &[(p_index(&d.units[0]), "Hallo Welt")]);
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("Hallo Welt") && out.contains(" xmlns:p999=\"urn:u999\""));
}
