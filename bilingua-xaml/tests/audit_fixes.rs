//! Regression tests for the findings of the 0.9.1 source audit.

use bilingua_model::*;
use bilingua_xaml::Xaml;
use std::time::{Duration, Instant};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en")
}

/// A resource dictionary whose root declares `prefixes` extra namespace prefixes and holds `elements` empty
/// `sys:Int32` elements before its one string.
fn dictionary(prefixes: usize, elements: usize) -> Vec<u8> {
    let mut s = String::from(
        "<ResourceDictionary xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" xmlns:sys=\"clr-namespace:System;assembly=mscorlib\"",
    );
    for i in 0..prefixes {
        s.push_str(&format!(" xmlns:p{i}=\"u\""));
    }
    s.push('>');
    for _ in 0..elements {
        s.push_str("<sys:Int32/>");
    }
    s.push_str("<sys:String x:Key=\"k\">Hello</sys:String></ResourceDictionary>");
    s.into_bytes()
}

/// `bilingua-xaml/src/scan.rs:lookup:linear-namespace-scan`: every name was looked up by a search of all the
/// declarations in scope, and nothing capped them: 40 000 declarations and 40 000 elements took minutes.
#[test]
fn too_many_namespaces_in_scope_are_refused() {
    let t = Instant::now();
    let e = Xaml::default().extract(&dictionary(40_000, 40_000), &opts()).expect_err("40 000 namespaces in scope");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlElements), "{e:?}");
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
}

/// Under the cap the lookup is indexed: 4 000 declarations in scope and 60 000 elements read in linear time.
#[test]
fn many_namespaces_under_the_cap_are_looked_up_in_constant_time() {
    let t = Instant::now();
    let doc = Xaml::default().extract(&dictionary(4_000, 60_000), &opts()).expect("under the cap");
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].context.key.as_deref(), Some("k"));
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
}

/// Scoping is as before: an inner declaration hides the outer one until its element closes, and a prefix is gone
/// when the element that declared it closes.
#[test]
fn namespace_scoping_is_unchanged() {
    let head = "<ResourceDictionary xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" xmlns:sys=\"clr-namespace:System;assembly=mscorlib\">";
    // inside <Style>, sys is some other namespace: its String is not a resource string; after it, sys is System again
    let hidden = format!("{head}<Style xmlns:sys=\"urn:other\"><sys:String x:Key=\"a\">Inner</sys:String></Style><sys:String x:Key=\"b\">Outer</sys:String></ResourceDictionary>");
    let doc = Xaml::default().extract(hidden.as_bytes(), &opts()).expect("well formed");
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].context.key.as_deref(), Some("b"));
    let gone = format!("{head}<Style xmlns:q=\"urn:q\"/><q:String>Text</q:String></ResourceDictionary>");
    let e = Xaml::default().extract(gone.as_bytes(), &opts()).expect_err("q is out of scope");
    assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("namespace prefix q is not defined")), "{e:?}");
}
