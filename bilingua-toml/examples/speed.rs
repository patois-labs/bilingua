//! `cargo run --release -p bilingua-toml --example speed -- OUT.toml` writes a generated 5+ MB file and times
//! extract, untouched merge and translated merge. `bilingua-toml/tools/bench_locos.py OUT.toml` times LocOS.
use bilingua_model::{merge_text, Filter, Inline};
use bilingua_toml::TomlFilter;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("output path");
    let mut s = String::from("# generated speed file\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        let n = i / 4;
        match i % 4 {
            0 => s.push_str(&format!("\n[section{n}]\ntitle = \"Welcome to the bakery number {n}, where fresh bread is ready at six\"   # note\n")),
            1 => s.push_str(&format!("body = 'Literal text {n} with {{{{ .Name }}}} and a few more words to translate here'\n")),
            2 => s.push_str(&format!("long = \"\"\"\nA multi-line value {n}.\nSecond line with \\\"escapes\\\" and \\u00e9.\"\"\"\n")),
            _ => s.push_str(&format!("tags = [\"alpha {n}\", \"beta {n}\", 3, {{ label = \"Gamma {n}\" }}]\nn = {n}\nwhen = 1979-05-27T07:32:00Z\n")),
        }
        i += 1;
    }
    std::fs::write(&path, &s).unwrap();
    let bytes = s.as_bytes();
    println!("file: {} bytes", bytes.len());
    let t = Instant::now();
    let mut d = TomlFilter.extract(bytes, &Default::default()).unwrap();
    println!("extract: {:?} ({} units)", t.elapsed(), d.units.len());
    let t = Instant::now();
    let out = TomlFilter.merge(&d).unwrap();
    assert_eq!(out, bytes);
    println!("merge untouched: {:?}", t.elapsed());
    let t = Instant::now();
    // (Document::set_target looks the segment up linearly; writing the field keeps this a parser benchmark.)
    for u in d.units.iter_mut() {
        let mut v = vec![Inline::text("DE ")];
        v.extend(u.source());
        u.segments[0].target = merge_text(v);
    }
    println!("set targets: {:?}", t.elapsed());
    let t = Instant::now();
    let out = TomlFilter.merge(&d).unwrap();
    println!("merge translated (with verify): {:?} ({} bytes)", t.elapsed(), out.len());
}
