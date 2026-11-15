//! BILI-48 and BILI-49: a script that switches code pages reports its source encoding as `multiple`, and a script
//! read in a code page and written as UTF-8 has every `#pragma code_page` say 65001.

use bilingua_model::*;
use bilingua_rc::Rc;

/// A Western section in 1252 and a Russian one in 1251.
const TWO_PAGES: &[u8] = b"#pragma code_page(1252)\r\nSTRINGTABLE\r\nBEGIN\r\n  1 \"Caf\xe9 au lait\"\r\nEND\r\n#pragma code_page(1251)\r\nSTRINGTABLE\r\nBEGIN\r\n  2 \"\xcf\xf0\xe8\xe2\xe5\xf2 \xec\xe8\xf0\"\r\nEND\r\n";

fn texts(doc: &Document) -> Vec<String> {
    doc.units.iter().map(|u| plain_text(&u.source())).collect()
}

fn translated(src: &[u8]) -> Document {
    let mut doc = Rc.extract(src, &ExtractOptions::new()).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Caf\u{e9} cr\u{e8}me")]).unwrap();
    doc
}

#[test]
fn a_script_with_several_code_pages_reports_multiple() {
    let doc = Rc.extract(TWO_PAGES, &ExtractOptions::new()).unwrap();
    assert_eq!(texts(&doc), ["Caf\u{e9} au lait", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}"]);
    assert_eq!(doc.detected_source_encoding().as_deref(), Some("multiple"));
    // the merge writes each section in its own page again
    assert_eq!(Rc.merge(&doc).unwrap(), TWO_PAGES);
    let out = Rc.merge(&translated(TWO_PAGES)).unwrap();
    assert!(out.windows(9).any(|w| w == b"Caf\xe9 cr\xe8m"), "{}", String::from_utf8_lossy(&out));
    // a Document stored before the tag changed still merges
    let mut old = translated(TWO_PAGES);
    if let Skeleton::Text { encoding, .. } = &mut old.skeleton {
        *encoding = encoding.trim_start_matches("multiple;").to_string();
    }
    assert_eq!(Rc.merge(&old).unwrap(), out);
    // a single code page is still reported by name
    let one = Rc.extract(b"#pragma code_page(1252)\r\nSTRINGTABLE\r\nBEGIN\r\n  1 \"Caf\xe9\"\r\nEND\r\n", &ExtractOptions::new()).unwrap();
    assert_eq!(one.detected_source_encoding().as_deref(), Some("windows-1252"));
}

#[test]
fn utf8_output_says_65001_in_every_pragma() {
    let utf8 = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8);
    let out = String::from_utf8(Rc.merge_with(&translated(TWO_PAGES), &utf8).unwrap()).expect("UTF-8 output");
    assert_eq!(out.matches("#pragma code_page(65001)").count(), 2, "{out}");
    assert!(!out.contains("code_page(1252)") && !out.contains("code_page(1251)"), "{out}");
    let back = Rc.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(texts(&back), ["Caf\u{e9} cr\u{e8}me", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}"]);
    // a 1252 script with no pragma gets one, so rc.exe does not read the UTF-8 bytes as ANSI
    let plain: &[u8] = b"STRINGTABLE\r\nBEGIN\r\n  1 \"Caf\xe9 au lait\"\r\nEND\r\n";
    let out = String::from_utf8(Rc.merge_with(&translated(plain), &utf8).unwrap()).unwrap();
    assert!(out.starts_with("#pragma code_page(65001)\r\n"), "{out}");
    assert_eq!(texts(&Rc.extract(out.as_bytes(), &ExtractOptions::new()).unwrap()), ["Caf\u{e9} cr\u{e8}me"]);
    // a UTF-8 script is written as it was
    let u = "STRINGTABLE\r\nBEGIN\r\n  1 \"Caf\u{e9}\"\r\nEND\r\n";
    let doc = Rc.extract(u.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(Rc.merge_with(&doc, &utf8).unwrap(), u.as_bytes());
}
