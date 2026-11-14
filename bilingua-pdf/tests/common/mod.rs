#![allow(dead_code)]
//! A small PDF writer for the tests (independent of the crate): objects in order, a classic xref table, optional
//! Flate streams. Enough for generated hostile files and the speed file.

use bilingua_model::*;
use std::io::Write;

pub fn data(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join(name)).unwrap()
}

pub fn deflate(b: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(b).unwrap();
    e.finish().unwrap()
}

pub enum O {
    Plain(String),
    Stream(String, Vec<u8>),
}

/// Writes objects 1..=n (object 1 must be the catalog) with a correct xref table.
pub fn pdf(objs: &[O]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        match o {
            O::Plain(s) => out.extend_from_slice(s.as_bytes()),
            O::Stream(d, data) => {
                out.extend_from_slice(format!("<<{d} /Length {}>>\nstream\n", data.len()).as_bytes());
                out.extend_from_slice(data);
                out.extend_from_slice(b"\nendstream");
            }
        }
        out.extend_from_slice(b"\nendobj\n");
    }
    let at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<</Size {} /Root 1 0 R>>\nstartxref\n{at}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

/// A document whose pages are the given content streams, Helvetica as /F1 (object 3).
pub fn pages(contents: &[Vec<u8>], compress: bool) -> Vec<u8> {
    let n = contents.len();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    let mut objs = vec![
        O::Plain("<</Type /Catalog /Pages 2 0 R>>".into()),
        O::Plain(format!("<</Type /Pages /Kids [{}] /Count {n}>>", kids.join(" "))),
        O::Plain("<</Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding>>".into()),
    ];
    for (i, c) in contents.iter().enumerate() {
        objs.push(O::Plain(format!("<</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources <</Font <</F1 3 0 R>>>> /Contents {} 0 R>>", 5 + 2 * i)));
        if compress {
            objs.push(O::Stream("/Filter /FlateDecode".into(), deflate(c)));
        } else {
            objs.push(O::Stream(String::new(), c.clone()));
        }
    }
    pdf(&objs)
}

/// One page of text lines.
pub fn text_page(lines: &[&str]) -> Vec<u8> {
    let mut c = String::from("BT /F1 11 Tf 14 TL 72 740 Td\n");
    for l in lines {
        c.push_str(&format!("({}) Tj T*\n", l.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)")));
    }
    c.push_str("ET\n");
    c.into_bytes()
}

pub fn ex(bytes: &[u8]) -> Result<Document, Error> {
    bilingua_pdf::Pdf.extract(bytes, &ExtractOptions::default())
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

pub fn golden() -> serde_json::Value {
    let s = String::from_utf8(data("locos_golden.json")).unwrap();
    serde_json::from_str(&s).unwrap()
}
