//! Resource limit fixes from the security review (run 2): object streams, streams without a length, ToUnicode
//! ranges and the text-line orientation pass cost work and memory in proportion to the file, not more.

mod common;
mod peak;
use bilingua_model::*;
use bilingua_pdf::Pdf;
use common::*;

fn ex_default(b: &[u8]) -> Result<Document, Error> {
    Pdf.extract(b, &ExtractOptions::default())
}

/// Work counted for `n` and `2n`: at most a little over twice as much (linear), not four times.
fn linear(n: usize, make: impl Fn(usize) -> Vec<u8>) {
    let (a, b) = (make(n), make(2 * n));
    let small = bilingua_model::work::count(|| ex_default(&a)).1;
    let big = bilingua_model::work::count(|| ex_default(&b)).1;
    assert!(small > 0 && big * 10 <= small * 25 + 2_000, "{small} steps for n={n}, {big} for n={}: more than linear", 2 * n);
}

#[test]
fn objstm_shared_offset_is_parsed_once() {
    // no cross reference, so the repair scan finds the object stream; its header gives 500 object numbers the
    // same offset, and the page's contents name all of them
    let m = 500;
    let ints = 20_000;
    let header: String = (0..m).map(|i| format!("{} 0 ", 10 + i)).collect();
    let body = format!("[{}]", "0 ".repeat(ints));
    let raw = format!("{header}{body}");
    let z = deflate(raw.as_bytes());
    let refs: String = (0..m).map(|i| format!("{} 0 R ", 10 + i)).collect();
    let mut b = b"%PDF-1.7\n1 0 obj <</Type /Catalog /Pages 2 0 R>> endobj\n2 0 obj <</Type /Pages /Kids [3 0 R] /Count 1>> endobj\n".to_vec();
    b.extend_from_slice(format!("3 0 obj <</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents [{refs}]>> endobj\n").as_bytes());
    b.extend_from_slice(format!("4 0 obj <</Type /ObjStm /N {m} /First {} /Length {} /Filter /FlateDecode>> stream\n", header.len(), z.len()).as_bytes());
    b.extend_from_slice(&z);
    b.extend_from_slice(b"\nendstream endobj\ntrailer <</Root 1 0 R>>\n%%EOF\n");
    let (r, bytes) = peak::peak(|| ex_default(&b));
    let _ = r;
    assert!(bytes < 48 << 20, "{bytes} bytes held for one object stream of {} bytes", raw.len());
}

#[test]
fn many_streams_without_length_scan_linear() {
    // stream objects with no /Length and no endstream of their own, one endstream after the last: finding the
    // end of each must not scan the rest of the file again
    linear(1_500, |n| {
        let first = 4;
        let refs: String = (0..n).map(|i| format!("{} 0 R ", first + i)).collect();
        let mut b = b"%PDF-1.7\n".to_vec();
        let mut offs = Vec::new();
        offs.push(b.len());
        b.extend_from_slice(b"1 0 obj <</Type /Catalog /Pages 2 0 R>> endobj\n");
        offs.push(b.len());
        b.extend_from_slice(b"2 0 obj <</Type /Pages /Kids [3 0 R] /Count 1>> endobj\n");
        offs.push(b.len());
        b.extend_from_slice(format!("3 0 obj <</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents [{refs}]>> endobj\n").as_bytes());
        for i in 0..n {
            offs.push(b.len());
            b.extend_from_slice(format!("{} 0 obj<<>>stream\n", first + i).as_bytes());
        }
        b.extend_from_slice(b"endstream\nendobj\n");
        let at = b.len();
        b.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offs.len() + 1).as_bytes());
        for o in &offs {
            b.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        b.extend_from_slice(format!("trailer\n<</Size {} /Root 1 0 R>>\nstartxref\n{at}\n%%EOF\n", offs.len() + 1).as_bytes());
        b
    });
}

/// One page drawing `(a)` with font /F1, whose ToUnicode stream is `cmap`.
fn tounicode_page(cmap: &[u8]) -> Vec<u8> {
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), b"BT /F1 12 Tf 72 700 Td (a) Tj ET".to_vec()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica /ToUnicode 6 0 R>>".into()),
        O::Stream("/Filter /FlateDecode".into(), deflate(cmap)),
    ];
    pdf(&objs)
}

#[test]
fn tounicode_bfrange_bomb_is_a_limit_error() {
    // CPU: two sections of 5,000 full ranges each write 2.56 million mappings; only 65,536 codes exist
    let mut cmap = String::new();
    for _ in 0..2 {
        cmap.push_str("5000 beginbfrange\n");
        for _ in 0..5_000 {
            cmap.push_str("<0000> <00ff> <0041>\n");
        }
        cmap.push_str("endbfrange\n");
    }
    let b = tounicode_page(cmap.as_bytes());
    let (_, work) = bilingua_model::work::count(|| ex_default(&b));
    assert!(work < 1_000_000, "{work} steps for a ToUnicode map of {} bytes", cmap.len());
    // memory: ranges whose destination is a long string are not expanded into 256 strings each before use
    let mut cmap = String::from("2000 beginbfrange\n");
    let dst = "0041".repeat(100);
    for _ in 0..2_000 {
        cmap.push_str(&format!("<0000> <00ff> <{dst}>\n"));
    }
    cmap.push_str("endbfrange\n");
    let b = tounicode_page(cmap.as_bytes());
    let (_, bytes) = peak::peak(|| ex_default(&b));
    assert!(bytes < 32 << 20, "{bytes} bytes held for a ToUnicode map of {} bytes", cmap.len());
    // an ordinary map still applies
    let b = tounicode_page(b"1 beginbfrange <0061> <0062> <0058> endbfrange");
    assert_eq!(texts(&ex_default(&b).unwrap()), vec!["X".to_string()]);
}

#[test]
fn flow_orientation_is_linear_in_objects() {
    // stroked text with a line width wider than the page: every text rectangle covers the whole page
    let n = 500;
    let mut c = String::from("BT /F1 1 Tf 1 Tr 200000 w 10 10 Td\n");
    for _ in 0..n {
        c.push_str("(a) Tj\n");
    }
    c.push_str("ET");
    let objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain("<</Type /Pages /Kids [3 0 R] /Count 1>>".into()),
        O::Plain("<</Type /Page /Parent 2 0 R /MediaBox [0 0 100000 100000] /Resources <</Font <</F1 5 0 R>>>> /Contents 4 0 R>>".into()),
        O::Stream(String::new(), c.into_bytes()),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica>>".into()),
    ];
    let b = pdf(&objs);
    let (_, work) = bilingua_model::work::count(|| ex_default(&b));
    assert!(work < 20 * (n as u64 + 200_000), "{work} steps for {n} text objects");
}

#[test]
fn text_objects_per_page_memory() {
    // lead L1: the memory one page of many small text objects holds
    let n = 50_000;
    let mut c = String::from("BT /F1 1 Tf\n");
    for _ in 0..n {
        c.push_str("(a)Tj ");
    }
    c.push_str("ET");
    let b = pages(&[c.into_bytes()], true);
    let (_, bytes) = peak::peak(|| ex_default(&b));
    assert!(bytes / n < 100, "{} bytes per text object ({bytes} for {n})", bytes / n);
}
