//! One large generated PDF (over 5 MB of text in its content streams), extracted. Run
//! `cargo test --release -p bilingua-pdf --test speed -- --nocapture` for the timing; set `PDF_SPEED_OUT` to a path
//! to also save the file so the LocOS filter can be timed on the same bytes (`tools/speed_locos.py`).

mod common;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut contents = Vec::new();
    let mut total = 0;
    let mut n = 0usize;
    while total < 5_200_000 {
        let mut lines = Vec::new();
        for _ in 0..120 {
            let w = |k: usize| words[(n * 7 + k) % words.len()];
            lines.push(format!("Open the {} {} near the {} and bring {} for {} line {n} of the guide.", w(1), w(2), w(3), w(4), w(5)));
            n += 1;
        }
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let c = text_page(&refs);
        total += c.len();
        contents.push(c);
    }
    pages(&contents, true)
}

#[test]
fn large_file_is_timed() {
    let bytes = generate();
    if let Ok(path) = std::env::var("PDF_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let t0 = Instant::now();
    let d = ex(&bytes).unwrap();
    let took = t0.elapsed();
    assert!(d.units.len() > 300);
    let chars: usize = texts(&d).iter().map(|t| t.len()).sum();
    assert!(chars >= 4_000_000, "{chars}");
    println!("pdf speed: {} bytes on disk, {} pages, {} units, {} bytes of unit text; extract {:?}", bytes.len(), d.units.len(), d.units.len(), chars, took);
}
