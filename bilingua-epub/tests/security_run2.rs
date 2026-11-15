//! Regression tests for the EPUB fix of security review run 2: the package XML reader resolves a namespace prefix
//! with one lookup and checks repeated attributes with a set, so many declarations and many attributes are linear.

mod common;
use bilingua_epub::Epub;
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

/// An OPF whose root declares 50,000 prefixes after the default namespace, then 50,000 unprefixed elements, and a
/// start tag of 30,000 attributes: each element scanned every binding and each attribute every earlier one (about
/// 3 x 10^9 steps). The book still opens with its usual units.
#[test]
fn epub_opf_many_namespaces_and_attributes_linear() {
    let decls: String = (0..50_000).map(|i| format!(" xmlns:p{i}=\"urn:x:{i}\"")).collect();
    let metas: String = (0..50_000).map(|i| format!("<meta property=\"x:m{i}\">v</meta>")).collect();
    let attrs: String = (0..30_000).map(|i| format!(" a{i}=\"\"")).collect();
    let opf = OPF
        .replacen("xml:lang=\"en\">", &format!("xml:lang=\"en\"{decls}>"), 1)
        .replacen("<meta property=\"dcterms:modified\">", &format!("{metas}<meta{attrs} property=\"dcterms:modified\">"), 1);
    let bytes = epub_with(&[("OEBPS/content.opf", opf.as_bytes())]);
    let plain = extract(&epub()).units.len();
    let doc = within(30, move || Epub.extract(&bytes, &ExtractOptions::new()).unwrap());
    assert_eq!(doc.units.len(), plain);
}

/// A repeated attribute is still refused among few attributes and among many.
#[test]
fn epub_opf_repeated_attribute_is_still_refused() {
    let many: String = (0..40).map(|i| format!(" a{i}=\"\"")).collect();
    for attrs in [" b=\"1\" b=\"2\"".to_string(), format!("{many} a7=\"\"")] {
        let opf = OPF.replacen("<meta property=\"dcterms:modified\">", &format!("<meta{attrs} property=\"dcterms:modified\">"), 1);
        assert!(Epub.extract(&epub_with(&[("OEBPS/content.opf", opf.as_bytes())]), &ExtractOptions::new()).is_err(), "{attrs}");
    }
}
