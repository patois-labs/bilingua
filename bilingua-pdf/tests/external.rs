//! The same comparison as `locos_golden.rs` over a folder of PDFs that cannot be committed (third-party files),
//! with its own `locos_golden.json` made by `tools/gen_golden.py`. Run by hand:
//!
//!     BILINGUA_PDF_EXTERNAL=/path/to/folder cargo test --release -p bilingua-pdf --test external -- --ignored --nocapture
//!
//! It prints the share of pages whose text is exactly PDFium's, the share whose units are exactly LocOS's, and
//! the first differences, and fails only when a file LocOS reads does not open or panics.

use bilingua_model::*;
use bilingua_pdf::{page_texts, paragraphs};

#[test]
#[ignore]
fn external_folder_matches_locos() {
    let Ok(dir) = std::env::var("BILINGUA_PDF_EXTERNAL") else { return };
    let dir = std::path::PathBuf::from(dir);
    let s = std::fs::read_to_string(dir.join("locos_golden.json")).unwrap();
    let g: serde_json::Value = serde_json::from_str(&s).unwrap();
    let (mut files, mut pages, mut same_pages, mut unit_pages, mut same_units) = (0, 0, 0, 0, 0);
    let mut shown = 0;
    let mut hard = Vec::new();
    let mut per_file = Vec::new();
    for (name, want) in g.as_object().unwrap() {
        files += 1;
        let bytes = std::fs::read(dir.join(name)).unwrap();
        let t0 = std::time::Instant::now();
        let got = std::panic::catch_unwind(|| page_texts(&bytes, &ExtractOptions::default()));
        let took = t0.elapsed();
        let got = match got {
            Ok(g) => g,
            Err(_) => {
                hard.push(format!("{name}: PANIC"));
                continue;
            }
        };
        if want.get("error").is_some() {
            if got.is_ok() {
                println!("{name}: LocOS refuses, Bilingua reads");
            }
            continue;
        }
        let got = match got {
            Ok(p) => p,
            Err(e) => {
                hard.push(format!("{name}: {e}"));
                continue;
            }
        };
        let want_pages: Vec<&str> = want["pages"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let mut fsame = 0;
        for (i, w) in want_pages.iter().enumerate() {
            pages += 1;
            let Some(p) = got.get(i) else { continue };
            if p.text == *w {
                same_pages += 1;
                fsame += 1;
            } else if shown < 40 {
                shown += 1;
                let a: Vec<char> = p.text.chars().collect();
                let b: Vec<char> = w.chars().collect();
                let k = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
                let from = k.saturating_sub(40);
                let ga: String = a[from..(k + 60).min(a.len())].iter().collect();
                let gb: String = b[from..(k + 60).min(b.len())].iter().collect();
                println!("{name} p{}: at {k}\n   got  {}\n   want {}", i + 1, ga.escape_debug(), gb.escape_debug());
            }
            let wt = bilingua_model::py_strip(w).chars().count();
            if wt >= 20 {
                unit_pages += 1;
                if paragraphs(&p.text) == paragraphs(w) {
                    same_units += 1;
                }
            }
        }
        per_file.push(format!("{name}: {fsame}/{} pages, {:?}", want_pages.len(), took));
    }
    for l in &per_file {
        println!("  {l}");
    }
    println!(
        "external: {files} files, {same_pages}/{pages} pages exact ({:.1} %), units exact on {same_units}/{unit_pages} text pages ({:.1} %)",
        100.0 * same_pages as f64 / pages.max(1) as f64,
        100.0 * same_units as f64 / unit_pages.max(1) as f64
    );
    assert!(hard.is_empty(), "{}", hard.join("\n"));
}
