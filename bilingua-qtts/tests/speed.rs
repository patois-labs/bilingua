//! One large generated file (over 5 MB), extracted and merged. Run `cargo test --release -p bilingua-qtts --test
//! speed -- --nocapture` for the timings. Set `QTTS_SPEED_OUT` to a path to also save the file, so the LocOS
//! filter can be timed on the same bytes (`tests/tools/speed_locos.py`).

mod common;
use bilingua_model::*;
use bilingua_qtts::QtTs;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut s = String::with_capacity(6_500_000);
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE TS>\n<TS version=\"2.1\" language=\"de_DE\" sourcelanguage=\"en_US\">\n");
    let mut n = 0usize;
    for c in 0..400 {
        s.push_str(&format!("<context>\n    <name>Window{c}</name>\n"));
        for _ in 0..100 {
            let w = |k: usize| words[(n * 7 + k) % words.len()];
            if n.is_multiple_of(20) {
                s.push_str(&format!(
                    "    <message numerus=\"yes\">\n        <location filename=\"../src/w{c}.cpp\" line=\"{}\"/>\n        <source>%n {} {} deleted from the {} in sentence {n}</source>\n        <translation type=\"unfinished\">\n            <numerusform></numerusform>\n            <numerusform></numerusform>\n        </translation>\n    </message>\n",
                    n % 900 + 1, w(1), w(2), w(3)
                ));
            } else if n % 50 == 1 {
                s.push_str(&format!(
                    "    <message>\n        <source>Tab<byte value=\"x9\"/>{} &amp; {} near the {} for entry {n}</source>\n        <comment>note {n}</comment>\n        <translation type=\"unfinished\"/>\n    </message>\n",
                    w(1), w(2), w(3)
                ));
            } else {
                s.push_str(&format!(
                    "    <message>\n        <location filename=\"../src/w{c}.cpp\" line=\"{}\"/>\n        <source>Open the {} {} near the {} at %1 and bring {} for \"{}\" caf\u{e9} \u{2013} sentence number {n} of the guide.</source>\n        <translation type=\"unfinished\"></translation>\n    </message>\n",
                    n % 900 + 1, w(1), w(2), w(3), w(4), w(5)
                ));
            }
            n += 1;
        }
        s.push_str("</context>\n");
    }
    s.push_str("</TS>\n");
    s.into_bytes()
}

#[test]
fn large_file_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("QTTS_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let big = Limits::new().with_max_units(1_000_000).with_max_lines(10_000_000);
    let t0 = Instant::now();
    let mut d = QtTs.extract_with(&bytes, &opts(None), &big).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(d.units.len(), 40_000);
    let t1 = Instant::now();
    let same = QtTs.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    pseudo(&mut d);
    d.target_lang = Some("pl-PL".into());
    let t2 = Instant::now();
    let out = QtTs.merge(&d).unwrap();
    let t_merge_tr = t2.elapsed();
    let back = QtTs.extract_with(&out, &opts(None), &big).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    assert!(s(&out).contains("language=\"pl_PL\""));
    println!(
        "qtts speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, translated merge (pl-PL, forms added) {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_merge_tr,
        t_extract + t_merge_tr
    );
}
