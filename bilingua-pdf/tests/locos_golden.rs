//! Differential test against Localization OS: every fixture in `tests/data`, with what LocOS made of it recorded
//! in `data/locos_golden.json` (`tools/gen_golden.py`, LocOS's own `PdfFilter.parse` with OCR stubbed, and the raw
//! PDFium page text). Bilingua must give the same page text, the same units for the text pages, the same OCR
//! pages, and refuse the same files.

use bilingua_model::*;
use bilingua_pdf::{page_texts, Pdf};
use std::path::PathBuf;

fn data() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("data")
}

fn golden() -> serde_json::Map<String, serde_json::Value> {
    let s = std::fs::read_to_string(data().join("locos_golden.json")).unwrap();
    serde_json::from_str::<serde_json::Value>(&s).unwrap().as_object().unwrap().clone()
}

fn show(s: &str) -> String {
    s.escape_debug().to_string()
}

#[test]
fn every_fixture_matches_locos() {
    let g = golden();
    let mut bad: Vec<String> = Vec::new();
    let mut files = 0;
    let mut pages_same = 0;
    let mut pages_total = 0;
    for (name, want) in &g {
        files += 1;
        let bytes = std::fs::read(data().join(name)).unwrap();
        let got = page_texts(&bytes, &ExtractOptions::default());
        if want.get("error").is_some() {
            if let Ok(p) = &got {
                bad.push(format!("{name}: LocOS refuses it ({}), Bilingua read {} pages", want["error"], p.len()));
            }
            continue;
        }
        let pages = match got {
            Ok(p) => p,
            Err(e) => {
                bad.push(format!("{name}: LocOS reads it, Bilingua: {e}"));
                continue;
            }
        };
        let want_pages: Vec<String> = want["pages"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        if pages.len() != want_pages.len() {
            bad.push(format!("{name}: {} pages, LocOS {}", pages.len(), want_pages.len()));
            continue;
        }
        for (p, w) in pages.iter().zip(&want_pages) {
            pages_total += 1;
            if &p.text == w {
                pages_same += 1;
            } else {
                bad.push(format!("{name} page {}:\n    got  {}\n    want {}", p.page + 1, show(&p.text), show(w)));
            }
        }
        let want_ocr: Vec<usize> = want["ocr_pages"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as usize).collect();
        let got_ocr: Vec<usize> = pages.iter().filter(|p| p.ocr).map(|p| p.page).collect();
        if got_ocr != want_ocr {
            bad.push(format!("{name}: OCR pages {got_ocr:?}, LocOS {want_ocr:?}"));
        }
        let doc = Pdf.extract(&bytes, &ExtractOptions::default()).unwrap();
        let got_units: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
        let want_units: Vec<String> =
            want["units"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).filter(|u| !u.starts_with("<OCR page")).collect();
        if got_units != want_units {
            bad.push(format!("{name}: units differ\n    got  {got_units:?}\n    want {want_units:?}"));
        }
    }
    println!("pdf golden: {files} files, {pages_same} of {pages_total} pages exactly the LocOS page text");
    assert!(bad.is_empty(), "{} differences:\n{}", bad.len(), bad.join("\n"));
}
