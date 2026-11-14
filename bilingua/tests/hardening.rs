//! D27 step 5: the limits, XML and zip guards and the text-encoding rules, one hostile or edge file per rule. Every
//! file is made up here (nothing large is committed). Each test names the Localization OS guard it ports.

use bilingua::*;
use bilingua_model::{LimitKind, Limits};
use std::io::{Read, Write};

fn opts_with(l: Limits) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("es").with_limits(l)
}

fn lang(l: &str) -> ExtractOptions {
    ExtractOptions::new().with_source_lang(l).with_target_lang("en")
}

fn kind(r: Result<Document, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => {
            assert!(!e.msg.is_empty() && e.msg.len() < 400, "short message: {}", e.msg);
            e.kind
        }
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("expected a limit error, the file was accepted"),
    }
}

fn corpus(rel: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus").join(rel)).unwrap()
}

/// Rewrites one part of a zip (`f` gets its text) and keeps the rest.
fn with_part(zip_bytes: &[u8], name: &str, f: impl Fn(&str) -> String) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..src.len() {
        let mut e = src.by_index(i).unwrap();
        let n = e.name().unwrap().to_string();
        let mut data = Vec::new();
        e.read_to_end(&mut data).unwrap();
        if n == name {
            data = f(&String::from_utf8(data).unwrap()).into_bytes();
        }
        w.start_file(n, opt).unwrap();
        w.write_all(&data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn zip_of(parts: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (n, d) in parts {
        w.start_file(*n, opt).unwrap();
        w.write_all(d).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// Puts a DOCTYPE with the given internal subset right after the XML declaration.
fn doctype(subset: &'static str) -> impl Fn(&str) -> String {
    move |x: &str| match x.find("?>") {
        Some(p) => format!("{}\n<!DOCTYPE r [{subset}]>{}", &x[..p + 2], &x[p + 2..]),
        None => format!("<!DOCTYPE r [{subset}]>{x}"),
    }
}

const BOMB: &str = r#"<!ENTITY a "lol"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;"><!ENTITY c "&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;">"#;
const XXE: &str = r#"<!ENTITY x SYSTEM "file:///etc/passwd">"#;

// ---------------------------------------------------------------- limits (base.py)

#[test]
fn input_size_limit_check_upload_bytes() {
    let l = Limits::new().with_max_input_bytes(50);
    for (name, bytes) in [("a.txt", vec![b'a'; 60]), ("a.md", vec![b'a'; 60]), ("a.json", vec![b' '; 60]), ("a.po", vec![b'a'; 60])] {
        assert_eq!(kind(extract(name, &bytes, &opts_with(l))), LimitKind::InputBytes, "{name}");
    }
    assert!(extract("a.txt", &[b'a'; 50], &opts_with(l)).is_ok());
    // the default is LocOS's 100 MB and the zip formats check it too
    assert_eq!(kind(extract("a.docx", &[b'P'; 60], &opts_with(l))), LimitKind::InputBytes);
}

#[test]
fn unit_count_limit_check_segment_cap() {
    let l = Limits::new().with_max_units(10);
    let txt: String = (0..11).map(|i| format!("Paragraph number {i}.\n\n")).collect();
    assert_eq!(kind(extract("a.txt", txt.as_bytes(), &opts_with(l))), LimitKind::Units);
    let md: String = (0..11).map(|i| format!("Paragraph number {i}.\n\n")).collect();
    assert_eq!(kind(extract("a.md", md.as_bytes(), &opts_with(l))), LimitKind::Units);
    let json = format!("[{}]", (0..11).map(|i| format!("\"String number {i}\"")).collect::<Vec<_>>().join(","));
    assert_eq!(kind(extract("a.json", json.as_bytes(), &opts_with(l))), LimitKind::Units);
    let ten: String = (0..10).map(|i| format!("Paragraph number {i}.\n\n")).collect();
    assert!(extract("a.txt", ten.as_bytes(), &opts_with(l)).is_ok());
    // sentences count against the cap after splitting: 10 units, 20 segments
    let two: String = (0..10).map(|i| format!("One sentence {i}. Another sentence {i}.

")).collect();
    let sentences = opts_with(l).with_granularity(Granularity::Sentence);
    assert_eq!(kind(extract("a.txt", two.as_bytes(), &sentences)), LimitKind::Units);
    assert!(extract("a.txt", two.as_bytes(), &opts_with(l).with_granularity(Granularity::Paragraph)).is_ok());
}

#[test]
fn unit_text_length_limit_check_unit_texts() {
    let l = Limits::new().with_max_unit_chars(30);
    let long = "x".repeat(31);
    assert_eq!(kind(extract("a.txt", long.as_bytes(), &opts_with(l))), LimitKind::UnitChars);
    assert_eq!(kind(extract("a.md", long.as_bytes(), &opts_with(l))), LimitKind::UnitChars);
    assert_eq!(kind(extract("a.json", format!("[\"{long}\"]").as_bytes(), &opts_with(l))), LimitKind::UnitChars);
    assert_eq!(kind(extract("a.html", format!("<p>{long}</p>").as_bytes(), &opts_with(l))), LimitKind::UnitChars);
    // counted in characters, not bytes: 30 Arabic letters are 60 bytes
    assert!(extract("a.txt", "\u{639}".repeat(30).as_bytes(), &opts_with(l)).is_ok());
}

#[test]
fn line_budget_check_line_budget() {
    let l = Limits::new().with_max_lines(20);
    assert_eq!(kind(extract("a.txt", "\n".repeat(21).as_bytes(), &opts_with(l))), LimitKind::Lines);
    assert_eq!(kind(extract("a.srt", "\r".repeat(21).as_bytes(), &opts_with(l))), LimitKind::Lines);
    assert!(extract("a.txt", "\n".repeat(20).as_bytes(), &opts_with(l)).is_ok());
}

#[test]
fn docx_paragraph_cap_counts_empty_paragraphs() {
    let l = Limits::new().with_max_paragraphs(3);
    let many = with_part(&corpus("docx/manual.docx"), "word/document.xml", |x| x.replace("<w:body>", &format!("<w:body>{}", "<w:p/>".repeat(10))));
    assert_eq!(kind(extract("a.docx", &many, &opts_with(l))), LimitKind::Paragraphs);
    assert!(extract("a.docx", &corpus("docx/manual.docx"), &opts_with(Limits::default())).is_ok());
}

#[test]
fn xlsx_cell_row_and_text_limits_check_xlsx_guard() {
    let menu = corpus("xlsx/menu.xlsx");
    assert!(extract("a.xlsx", &menu, &opts_with(Limits::default())).is_ok());
    assert_eq!(kind(extract("a.xlsx", &menu, &opts_with(Limits::new().with_xlsx_max_cells(2)))), LimitKind::Spreadsheet);
    assert_eq!(kind(extract("a.xlsx", &menu, &opts_with(Limits::new().with_xlsx_max_rows(1)))), LimitKind::Spreadsheet);
    assert_eq!(kind(extract("a.xlsx", &menu, &opts_with(Limits::new().with_xlsx_max_cell_chars(3)))), LimitKind::Spreadsheet);
    assert_eq!(kind(extract("a.xlsx", &menu, &opts_with(Limits::new().with_xlsx_max_total_text_chars(10)))), LimitKind::Spreadsheet);
}

// ---------------------------------------------------------------- XML (xml_guard.py)

#[test]
fn entity_bomb_and_xxe_are_refused_in_every_xml_format() {
    for (rel, part) in [("docx/manual.docx", "word/document.xml"), ("xlsx/menu.xlsx", "xl/workbook.xml"), ("pptx/launch.pptx", "ppt/slides/slide1.xml")] {
        for subset in [BOMB, XXE] {
            let name = rel.rsplit('/').next().unwrap();
            let bad = with_part(&corpus(rel), part, doctype(subset));
            assert_eq!(kind(extract(name, &bad, &ExtractOptions::default())), LimitKind::XmlEntities, "{rel} {subset}");
        }
    }
    let xlf = |subset: &str| {
        format!("<?xml version=\"1.0\"?>\n<!DOCTYPE xliff [{subset}]>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file original=\"a\" source-language=\"en\" target-language=\"es\" datatype=\"plaintext\"><body><trans-unit id=\"1\"><source>&c;</source></trans-unit></body></file></xliff>")
    };
    for subset in [BOMB, XXE] {
        assert_eq!(kind(extract("a.xlf", xlf(subset).as_bytes(), &ExtractOptions::default())), LimitKind::XmlEntities);
    }
    // HTML where it is XML-like: an entity declaration is refused too
    assert_eq!(kind(extract("a.html", format!("<!DOCTYPE html [{BOMB}]><p>&c;</p>").as_bytes(), &ExtractOptions::default())), LimitKind::XmlEntities);
    // a plain DOCTYPE with no entities is fine
    assert!(extract("a.html", b"<!DOCTYPE html><p>Hello there</p>", &ExtractOptions::default()).is_ok());
}

#[test]
fn xml_depth_and_element_count_are_bounded() {
    let deep = with_part(&corpus("docx/manual.docx"), "word/document.xml", |x| x.replace("<w:body>", &format!("<w:body>{}{}", "<w:sdt>".repeat(40), "</w:sdt>".repeat(40))));
    let l = Limits::new().with_max_xml_depth(20);
    assert_eq!(kind(extract("a.docx", &deep, &opts_with(l))), LimitKind::XmlDepth);
    let wide = with_part(&corpus("docx/manual.docx"), "word/document.xml", |x| x.replace("<w:body>", &format!("<w:body>{}", "<w:bookmarkEnd w:id=\"1\"/>".repeat(100))));
    let l = Limits::new().with_max_xml_elements(50);
    assert_eq!(kind(extract("a.docx", &wide, &opts_with(l))), LimitKind::XmlElements);
    // the default depth is generous enough for real files
    assert!(extract("a.docx", &corpus("docx/manual.docx"), &ExtractOptions::default()).is_ok());
}

// ---------------------------------------------------------------- zip (zip_guard.py)

#[test]
fn zip_entry_count_total_size_and_ratio_are_bounded() {
    let docx = corpus("docx/manual.docx");
    let n = zip::ZipArchive::new(std::io::Cursor::new(&docx)).unwrap().len();
    assert_eq!(kind(extract("a.docx", &docx, &opts_with(Limits::new().with_max_zip_entries(n - 1)))), LimitKind::ZipEntries);
    assert!(extract("a.docx", &docx, &opts_with(Limits::new().with_max_zip_entries(n))).is_ok());
    // total uncompressed size
    assert_eq!(kind(extract("a.docx", &docx, &opts_with(Limits::new().with_max_zip_bytes(500)))), LimitKind::ZipBytes);
    // a part that expands 1000 times (a zip bomb in miniature: 3 MB of zeros)
    let bomb = zip_of(&[("zeros.bin", vec![0u8; 3 * 1024 * 1024]), ("[Content_Types].xml", b"<Types/>".to_vec())]);
    assert!((bomb.len() as u64) < 100_000);
    assert_eq!(kind(extract("a.docx", &bomb, &ExtractOptions::default())), LimitKind::ZipRatio);
    // below the floor a repetitive part is normal
    let small = zip_of(&[("zeros.bin", vec![0u8; 200_000]), ("[Content_Types].xml", b"<Types/>".to_vec())]);
    assert!(!matches!(extract("a.docx", &small, &ExtractOptions::default()), Err(Error::Limit(_))));
}

#[test]
fn zip_with_forged_sizes_is_refused() {
    // 100 KB of 'a' deflated, then the uncompressed size in both headers cut to 10: the real data is bigger
    let mut z = zip_of(&[("[Content_Types].xml", vec![b'a'; 100_000])]);
    let local = z.windows(4).position(|w| w == b"PK\x03\x04").unwrap();
    z[local + 22..local + 26].copy_from_slice(&10u32.to_le_bytes());
    let cd = z.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    z[cd + 24..cd + 28].copy_from_slice(&10u32.to_le_bytes());
    match extract("a.docx", &z, &ExtractOptions::default()) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::ZipMalformed),
        other => panic!("{:?}", other.map(|d| d.format)),
    }
}

#[test]
fn zip_names_that_escape_are_refused() {
    for name in ["../evil.xml", "/abs.xml", "C:/x.xml", "a/../../b.xml"] {
        let z = zip_of(&[("[Content_Types].xml", b"<Types/>".to_vec()), (name, b"x".to_vec())]);
        assert!(extract("a.docx", &z, &ExtractOptions::default()).is_err(), "{name}");
    }
}

// ---------------------------------------------------------------- encodings (base.py, code_pages.py)

const CASES: &[(&str, &str, &str)] = &[
    ("ar", "windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{645}\u{648}\u{642}\u{639}\u{646}\u{627}. \u{647}\u{630}\u{627} \u{646}\u{635} \u{62a}\u{62c}\u{631}\u{64a}\u{628}\u{64a}."),
    ("ja", "shift_jis", "\u{3053}\u{3053}\u{306f}\u{30c6}\u{30b9}\u{30c8}\u{7528}\u{306e}\u{6587}\u{7ae0}\u{3067}\u{3059}\u{3002}\u{65e5}\u{672c}\u{8a9e}\u{3067}\u{66f8}\u{304b}\u{308c}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}"),
    ("zh-CN", "gbk", "\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6d4b}\u{8bd5}\u{6587}\u{4ef6}\u{3002}\u{5b83}\u{7528}\u{7b80}\u{4f53}\u{4e2d}\u{6587}\u{5199}\u{6210}\u{3002}"),
    ("zh-TW", "big5", "\u{9019}\u{662f}\u{4e00}\u{500b}\u{6e2c}\u{8a66}\u{6a94}\u{6848}\u{3002}\u{5b83}\u{7528}\u{7e41}\u{9ad4}\u{4e2d}\u{6587}\u{5beb}\u{6210}\u{3002}"),
    ("ko", "euc-kr", "\u{c774}\u{ac83}\u{c740} \u{d14c}\u{c2a4}\u{d2b8} \u{d30c}\u{c77c}\u{c785}\u{b2c8}\u{b2e4}. \u{d55c}\u{ad6d}\u{c5b4}\u{b85c} \u{c4f0}\u{c5ec} \u{c788}\u{c2b5}\u{b2c8}\u{b2e4}."),
    ("el", "windows-1253", "Αυτό είναι ένα δοκιμαστικό κείμενο. Γράφτηκε στα ελληνικά."),
    ("ru", "windows-1251", "\u{42d}\u{442}\u{43e} \u{442}\u{435}\u{441}\u{442}\u{43e}\u{432}\u{44b}\u{439} \u{444}\u{430}\u{439}\u{43b}. \u{41e}\u{43d} \u{43d}\u{430}\u{43f}\u{438}\u{441}\u{430}\u{43d} \u{43f}\u{43e}-\u{440}\u{443}\u{441}\u{441}\u{43a}\u{438}."),
];

fn encode_as(label: &str, s: &str) -> Vec<u8> {
    let enc = bilingua_model::encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
    let (b, _, bad) = enc.encode(s);
    assert!(!bad, "{label}");
    b.into_owned()
}

fn cases() -> Vec<(&'static str, &'static str, String)> {
    CASES.iter().map(|(l, e, s)| (*l, *e, s.to_string())).collect()
}

#[test]
fn legacy_code_pages_of_the_source_language_round_trip_in_every_text_format() {
    for (lang_tag, label, text) in cases() {
        for (name, wrap) in [
            ("a.txt", format!("{text}\n")),
            ("a.md", format!("# {text}\n\n{text}\n")),
            ("a.srt", format!("1\n00:00:01,000 --> 00:00:02,000\n{text}\n\n")),
            ("a.vtt", format!("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n{text}\n\n")),
            ("a.po", format!("msgid \"{text}\"
msgstr \"\"
")),
            ("a.json", format!("{{\"k\": \"{text}\"}}")),
            ("a.html", format!("<html><body><p>{text}</p></body></html>")),
        ] {
            let bytes = encode_as(label, &wrap);
            let doc = extract(name, &bytes, &lang(lang_tag)).unwrap_or_else(|e| panic!("{name} {lang_tag}: {e}"));
            assert!(doc.warnings.is_empty(), "{name} {lang_tag}: {:?}", doc.warnings);
            assert!(
                doc.units.iter().any(|u| text_of(u).contains(text.split(['.', '\u{3002}']).next().unwrap())),
                "{name} {lang_tag}: the text was not decoded"
            );
            // untouched: byte for byte
            assert_eq!(merge(&doc).unwrap(), bytes, "{name} {lang_tag}: untouched merge changed the file");
        }
    }
}

#[test]
fn a_translation_that_fits_stays_in_the_code_page_and_one_that_does_not_becomes_utf8() {
    let (lang_tag, label, text) = cases().into_iter().find(|c| c.0 == "ja").unwrap();
    let bytes = encode_as(label, &format!("{text}\n"));
    let mut doc = extract("a.txt", &bytes, &lang(lang_tag).with_granularity(Granularity::Paragraph)).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}")]).unwrap();
    assert_eq!(merge(&doc).unwrap(), encode_as(label, "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\n"));
    doc.set_target(&id, vec![Inline::text("مرحبا")]).unwrap();
    assert_eq!(merge(&doc).unwrap(), "مرحبا\n".as_bytes());
}

#[test]
fn wrong_code_page_is_a_warning_on_the_document_not_an_error() {
    // Greek in Windows-1253 uploaded by a project with no source language (read as Windows-1252)
    let (_, label, text) = cases().into_iter().find(|c| c.0 == "el").unwrap();
    let bytes = encode_as(label, &format!("{text}\n{text}\n"));
    let doc = extract("a.txt", &bytes, &lang("en")).unwrap();
    assert_eq!(doc.warnings.len(), 1, "{:?}", doc.warnings);
    assert_eq!(doc.warnings[0].kind, "text_encoding");
    assert!(doc.warnings[0].message.contains("UTF-8"));
    // the warning travels in the JSON, and an empty list is left out
    let v = serde_json::to_value(&doc).unwrap();
    assert!(v["warnings"].is_array());
    let clean = extract("a.txt", b"Plain text.\n", &lang("en")).unwrap();
    assert!(serde_json::to_value(&clean).unwrap().get("warnings").is_none());
    // the same bytes with a Greek source language are read correctly, with no warning
    assert!(extract("a.txt", &bytes, &lang("el")).unwrap().warnings.is_empty());
    // untouched merge returns the original bytes either way
    assert_eq!(merge(&doc).unwrap(), bytes);
}

#[test]
fn bom_utf16_and_newline_style_are_kept() {
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice("Hello there.\r\nSecond line.\r\n".as_bytes());
    let doc = extract("a.txt", &bom, &ExtractOptions::default()).unwrap();
    assert_eq!(merge(&doc).unwrap(), bom);
    let mut u16le = vec![0xFF, 0xFE];
    for u in "Caf\u{e9} \u{4e2d}\u{6587}\r\nnext\r\n".encode_utf16() {
        u16le.extend_from_slice(&u.to_le_bytes());
    }
    for name in ["a.txt", "a.md", "a.srt", "a.html"] {
        let body: Vec<u8> = if name == "a.srt" {
            let mut v = vec![0xFF, 0xFE];
            for u in "1\r\n00:00:01,000 --> 00:00:02,000\r\nCaf\u{e9}\r\n\r\n".encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        } else {
            u16le.clone()
        };
        let d = extract(name, &body, &ExtractOptions::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(merge(&d).unwrap(), body, "{name}");
    }
    // new text with bare \n goes out with the file's CRLF
    let mut doc = extract("a.txt", b"One line.\r\nTwo lines.\r\n", &ExtractOptions::default()).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Uno.\nDos.")]).unwrap();
    assert_eq!(merge(&doc).unwrap(), b"Uno.\r\nDos.\r\n");
}

#[test]
fn declared_charsets_are_read_and_written_back() {
    // HTML: <meta charset> names Windows-1253; kept as written, characters it cannot hold become references
    let html = encode_as("windows-1253", "<html><head><meta charset=\"windows-1253\"></head><body><p>\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5}.</p></body></html>");
    let doc = extract("a.html", &html, &ExtractOptions::default()).unwrap();
    assert!(text_of(&doc.units[0]).starts_with('\u{39a}'));
    assert_eq!(merge(&doc).unwrap(), html);
    // PO: the Content-Type charset (ISO-8859-2 here) is read and rewritten to UTF-8 in the skeleton, restored on merge
    let po = encode_as("iso-8859-2", "msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=ISO-8859-2\\n\"\n\nmsgid \"Hello\"\nmsgstr \"Zażółć gęślą jaźń\"\n");
    let d = extract("a.po", &po, &ExtractOptions::default()).unwrap();
    assert!(d.units.iter().any(|u| text_of(u) == "Hello"));
    assert_eq!(merge(&d).unwrap(), po);
    // a pure-ASCII PO that declares ISO-8859-2: text added later is written in it
    let ascii = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=ISO-8859-2\\n\"\n\nmsgid \"Hello\"\nmsgstr \"\"\n".to_vec();
    let mut d = extract("a.po", &ascii, &lang("en")).unwrap();
    assert_eq!(merge(&d).unwrap(), ascii);
    let id = d.units.iter().find(|u| text_of(u) == "Hello").unwrap().segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Zażółć")]).unwrap();
    let out = merge(&d).unwrap();
    assert!(out.windows(6).any(|w| w == encode_as("iso-8859-2", "Zażółć").as_slice()));
    // XLIFF in a legacy encoding declared in the XML declaration
    let xlf = encode_as("windows-1251", "<?xml version=\"1.0\" encoding=\"windows-1251\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file original=\"a\" source-language=\"ru\" target-language=\"en\" datatype=\"plaintext\"><body><trans-unit id=\"1\"><source>\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{438}\u{440}</source></trans-unit></body></file></xliff>\n");
    let d = extract("a.xlf", &xlf, &lang("ru")).unwrap();
    assert!(text_of(&d.units[0]).contains('\u{41f}'));
    assert_eq!(merge(&d).unwrap(), xlf);
}

#[test]
fn binary_and_unreadable_text_is_refused_with_a_resave_message() {
    for name in ["a.txt", "a.md", "a.json", "a.po", "a.srt", "a.html"] {
        let e = extract(name, &[b'a', 0xFF, 0x00, 0x01, 0x02, b'b'], &ExtractOptions::default()).unwrap_err();
        assert!(matches!(&e, Error::Encoding(m) if m.contains("UTF-8")), "{name}: {e}");
    }
    // Windows-1252 has five undefined bytes
    let e = extract("a.txt", &[b'a', 0x81, b'b'], &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Encoding(_)));
}

fn text_of(u: &Unit) -> String {
    u.source().iter().map(|i| if let Inline::Text(t) = i { t.as_str() } else { "" }).collect()
}
