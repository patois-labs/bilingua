//! LocOS server pages end in `encode_text_lossless` (no `encode_like`): a page read in a code page is written as UTF-8
//! once a unit is written, whether it declared the code page or not, and its declaration then says UTF-8. Without the
//! profile the page keeps its code page.
mod common;
use bilingua_model::*;
use common::*;

fn changed(fmt: &str, page: &[u8]) -> Document {
    let mut d = extract(fmt, page).expect("extract");
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Caf\u{e9} ok!")]).unwrap();
    d
}

#[test]
fn code_page_server_page_is_written_as_utf8_under_the_profile() {
    let locos = MergeOptions::new().with_profile(profile::LOCOS);
    let declared: &[u8] = b"<html><head><meta charset=\"windows-1252\"></head><body><p>Caf\xe9 ok</p><p>Two</p></body></html>\n";
    let bare: &[u8] = b"<html><body><p>Caf\xe9 ok</p><p>Two</p></body></html>\n";
    for fmt in ["erb", "aspx", "jsp", "phtml", "shtml"] {
        let f = filter(fmt);
        let out = f.merge_with(&changed(fmt, declared), &locos).unwrap();
        assert_eq!(
            out,
            b"<html><head><meta charset=\"UTF-8\"></head><body><p>Caf\xc3\xa9 ok!</p><p>Two</p></body></html>\n".to_vec(),
            "{fmt} declared"
        );
        let out = f.merge_with(&changed(fmt, bare), &locos).unwrap();
        assert_eq!(out, b"<html><body><p>Caf\xc3\xa9 ok!</p><p>Two</p></body></html>\n".to_vec(), "{fmt} bare");
        // native: the code page stays
        let out = f.merge(&changed(fmt, bare)).unwrap();
        assert_eq!(out, b"<html><body><p>Caf\xe9 ok!</p><p>Two</p></body></html>\n".to_vec(), "{fmt} native");
        // nothing changed: the bytes as they were
        let d = extract(fmt, bare).unwrap();
        assert_eq!(f.merge_with(&d, &locos).unwrap(), bare, "{fmt} untouched");
    }
}
