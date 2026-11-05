//! One large generated file (over 5 MB), extracted and merged. Run `cargo test --release -p bilingua-resjson
//! --test speed -- --nocapture` for the timings. Set `RESJSON_SPEED_OUT` to a path to also save the file, so the
//! LocOS filter can be timed on the same bytes.

mod common;
use bilingua_model::*;
use bilingua_resjson::Resjson;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("\u{feff}{\r\n  // Generated: Larchmoor Reader strings\r\n");
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    for n in 0..40_000usize {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        s.push_str(&format!(
            "  \"page{n}.item.label\" : \"Open the {} {} near the {} at {{0}} and bring {} for {} \\\"{}\\\" caf\\u00e9 \\u2013 sentence number {n} of the guide.\",\r\n",
            w(1), w(2), w(3), w(4), w(5), w(6)
        ));
        if n % 10 == 0 {
            s.push_str(&format!("  \"_page{n}.item.label.comment\" : \"Note for entry {n}.\",\r\n"));
        }
        if n % 1000 == 0 {
            s.push_str(&format!("  /* block {n} */\r\n  \"group{n}\" : {{ \"a\": \"Group {n} first\", \"b\": 5 }},\r\n"));
        }
    }
    s.push_str("  \"last\": \"The end\"\r\n}\r\n");
    s.into_bytes()
}

#[test]
fn large_file_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("RESJSON_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let t0 = Instant::now();
    let mut d = Resjson.extract(&bytes, &opts(false)).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(d.units.len(), 40_000 + 40 + 1);
    let t1 = Instant::now();
    let same = Resjson.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    // Translate everything and merge again.
    translate_all(&mut d, |s| format!("\u{ab}{s}\u{bb}"));
    let t2 = Instant::now();
    let out = Resjson.merge(&d).unwrap();
    let t_merge_tr = t2.elapsed();
    let back = Resjson.extract(&out, &opts(false)).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    println!(
        "resjson speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, translated merge {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_merge_tr,
        t_extract + t_merge
    );
}
