//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-php --release --test speed -- --nocapture
//! Set BILINGUA_PHP_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py reads it).
use bilingua_model::*;
use bilingua_php::Php;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("<?php\n\n// Lumen Bakery generated messages\nreturn [\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        if i % 200 == 0 {
            if i > 0 {
                s.push_str("    ],\n");
            }
            s.push_str(&format!("    'section{}' => [\n", i / 200));
        }
        match i % 6 {
            0 => s.push_str(&format!("        // note for entry {i}\n        'key{i}' => 'Click \\'Save\\' to continue with order number {i} today',\n")),
            1 => s.push_str(&format!("        'greet{i}' => \"Welcome back, :name! You have <b>:count</b> new messages ({i}).\\n\",\n")),
            2 => s.push_str(&format!("        'items{i}' => '{{0}} No items|[1,19] Some items in your basket {i}|[20,*] Many items',\n")),
            3 => s.push_str(&format!("        'jp{i}' => '\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {i} \u{3067}\u{3059}\u{3002}\u{3053}\u{308c}\u{306f}\u{9577}\u{3044}\u{6587}\u{7ae0}\u{3067}\u{3059}\u{3002}',\n")),
            4 => s.push_str(&format!("        'cat{i}' => 'Part one of the sentence ' . \"and part two {i}\" . ' and the end',\n")),
            _ => s.push_str(&format!("        'tags{i}' => ['Click <a href=\"/x\">here</a> to read %s of %1$d', 'Plain text {i}', 42, true],\n")),
        }
        i += 1;
    }
    s.push_str("    ],\n];\n");
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_PHP_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let opts = ExtractOptions { limits: Some(Limits { max_units: 10_000_000, max_input_bytes: 1 << 30, max_lines: 100_000_000, ..Limits::default() }), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = Php.extract(text.as_bytes(), &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("T{i} \"q\" translated"))];
    }
    let out = Php.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    let untouched = Php.merge(&Php.extract(text.as_bytes(), &opts).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "Bilingua php: {:.1} MB, {} units, extract {} ms, translate+merge {} ms, extract+merge {} ms",
        text.len() as f64 / 1e6,
        n,
        t_extract.as_millis(),
        t_merge.as_millis(),
        (t_extract + t_merge).as_millis()
    );
    let again = Php.extract(&out, &opts).unwrap();
    assert_eq!(again.units.len(), n);
}
