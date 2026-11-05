//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/tool.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_properties::{Limits, Properties};
use common::*;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("# Generated Lumen Bakery messages\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        match i % 4 {
            0 => s.push_str(&format!("# note {i}\nmenu.item{i} = Our {} {} is ready, {{0}} items for {{1}}\n", w(1), w(2))),
            1 => s.push_str(&format!("menu.item{i}: Caf\\u00e9 {} \\u65e5\\u672c {} don''t miss {{0}}\n", w(3), w(4))),
            2 => s.push_str(&format!("menu.item{i} Total %d {} at %s, 100%% {}\n", w(5), w(6))),
            _ => s.push_str(&format!("menu.item{i} = A long text about {} \\\n    that continues on {} and ends\n", w(1), w(2))),
        }
        i += 1;
    }
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let limits = Limits { max_units: 10_000_000, ..Limits::default() };
    let mut d = Properties.extract_with(&data, &opts(), &limits).unwrap();
    assert!(d.units.len() > 50_000);
    assert_eq!(Properties.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = Properties.merge(&d).unwrap();
    let back = Properties.extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), d.units.len());
}
