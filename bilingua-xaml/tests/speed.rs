//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/tool.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_xaml::{Limits, Xaml};
use common::*;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("<ResourceDictionary xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\" xmlns:sys=\"clr-namespace:System;assembly=mscorlib\">\r\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        match i % 4 {
            0 => s.push_str(&format!("  <sys:String x:Key=\"k{i}\">Our {} {} is ready, {{0}} items for {{1}}</sys:String>\r\n", w(1), w(2))),
            1 => s.push_str(&format!("  <sys:String x:Key=\"k{i}\">Caf\u{e9} {} \u{65e5}\u{672c} {} &amp; {{0}}</sys:String>\r\n", w(3), w(4))),
            2 => s.push_str(&format!("  <!-- c{i} -->\r\n  <sys:String x:Key=\"k{i}\">{{Binding {}}}</sys:String>\r\n", w(5))),
            _ => s.push_str(&format!("  <sys:String x:Key=\"k{i}\">\r\n    A long text about {}\r\n  </sys:String>\r\n", w(1))),
        }
        i += 1;
    }
    s.push_str("</ResourceDictionary>\r\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let limits = Limits::new().with_max_units(10_000_000);
    let mut d = Xaml::default().extract_with(&data, &opts(), &limits).unwrap();
    assert!(d.units.len() > 30_000);
    assert_eq!(Xaml::default().merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = Xaml::default().merge(&d).unwrap();
    let back = Xaml::default().extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), d.units.len());
}
