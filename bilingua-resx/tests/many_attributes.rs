//! Resource limit tests for start tags with very many attributes (RUSTSEC-2026-0194, fixed by quick-xml 0.41).
//! Every attribute is still checked (well formed, no duplicates, references resolve), and the work stays linear in
//! the number of attributes. Also pins the attribute value reading kept across the quick-xml upgrade: references
//! are resolved, whitespace is not normalized.

use bilingua_model::*;
use bilingua_resx::Resx;

const N: usize = 100_000;

fn one_entry(extra: &str) -> Vec<u8> {
    format!("<root><data name=\"Greeting\"{extra}><value>Hello</value></data></root>").into_bytes()
}

fn many(n: usize) -> String {
    let mut s = String::with_capacity(n * 12);
    for i in 0..n {
        s.push_str(&format!(" a{i}=\"v\""));
    }
    s
}

#[test]
fn a_tag_with_a_hundred_thousand_attributes_reads_in_linear_time() {
    let data = one_entry(&many(N));
    let t0 = std::time::Instant::now();
    let d = Resx.extract(&data, &ExtractOptions::new()).unwrap();
    let took = t0.elapsed().as_secs_f64();
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].context.key.as_deref(), Some("Greeting"));
    assert_eq!(Resx.merge(&d).unwrap(), data);
    eprintln!("{N} attributes on one tag: extract in {took:.2} s (this build)");
    // a quadratic duplicate check takes minutes here in a debug build; linear takes well under a second
    assert!(took < 10.0, "far too slow: {took:.1} s");
}

#[test]
fn a_duplicate_after_many_attributes_is_still_refused() {
    let data = one_entry(&format!("{} a7=\"again\"", many(N)));
    let t0 = std::time::Instant::now();
    match Resx.extract(&data, &ExtractOptions::new()) {
        Err(Error::Parse { format, msg }) => {
            assert_eq!(format, "resx");
            assert!(msg.contains("bad attribute"), "{msg}");
        }
        Err(e) => panic!("expected a parse error, got {e}"),
        Ok(_) => panic!("a duplicate attribute was accepted"),
    }
    let took = t0.elapsed().as_secs_f64();
    assert!(took < 10.0, "far too slow: {took:.1} s");
}

#[test]
fn attribute_values_resolve_references_and_keep_whitespace() {
    let d = Resx.extract(b"<root><data name=\"A&amp;B&#x9;C\nD\tE\"><value>x</value></data></root>", &ExtractOptions::new()).unwrap();
    assert_eq!(d.units[0].context.key.as_deref(), Some("A&B\tC\nD\tE"));
}
