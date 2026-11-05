//! Merging a big package must not recompress the parts nobody changed: they are copied with their original
//! compressed bytes. The 20 MB file is generated here (never committed).

use bilingua::*;
use std::io::{Read, Write};
use std::time::Instant;

const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
const CT: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
const RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";

/// Pseudo-random bytes (xorshift): compress poorly, like a photo.
fn noise(n: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut v = Vec::with_capacity(n);
    while v.len() < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.extend_from_slice(&x.to_le_bytes());
    }
    v.truncate(n);
    v
}

fn big_docx() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    // A level the merge writer does not use, so recompression could not reproduce the same bytes by luck.
    let best = deflate.compression_level(Some(9));
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {NS}><w:body><w:p><w:r><w:t>Hello there. Second one.</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"
    );
    for (name, data, opt) in [
        ("[Content_Types].xml", CT.as_bytes().to_vec(), deflate),
        ("_rels/.rels", RELS.as_bytes().to_vec(), deflate),
        ("word/document.xml", doc.into_bytes(), deflate),
        ("word/media/image1.png", noise(20 * 1024 * 1024), best),
        ("word/media/stored.bin", noise(64 * 1024), stored),
    ] {
        w.start_file(name, opt).unwrap();
        w.write_all(&data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

struct Raw {
    method: zip::CompressionMethod,
    crc: u32,
    size: u64,
    packed: Vec<u8>,
    time: Option<zip::DateTime>,
}

fn raw_entries(bytes: &[u8]) -> Vec<(String, Raw)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index_raw(i).unwrap();
            let name = f.name().unwrap().into_owned();
            let (method, crc, size, time) = (f.compression(), f.crc32(), f.size(), f.last_modified());
            let mut packed = Vec::new();
            f.read_to_end(&mut packed).unwrap();
            (name, Raw { method, crc, size, packed, time })
        })
        .collect()
}

fn same(a: &Raw, b: &Raw) -> bool {
    a.method == b.method && a.crc == b.crc && a.size == b.size && a.packed == b.packed && a.time == b.time
}

#[test]
fn untouched_entries_keep_their_compressed_bytes_and_merge_is_fast() {
    let bytes = big_docx();
    let opts = ExtractOptions {
        source_lang: Some("en".into()),
        target_lang: Some("es".into()),
        granularity: Granularity::Sentence,
        include_locked: false,
        limits: None,
    };
    let mut doc = extract("big.docx", &bytes, &opts).unwrap();
    let before = raw_entries(&bytes);

    let t = Instant::now();
    let out = merge(&doc).unwrap();
    let elapsed = t.elapsed();
    eprintln!("merge of a {} byte docx took {elapsed:?}", bytes.len());
    // Release builds copy the image; even a debug build must not spend seconds recompressing 20 MB.
    assert!(elapsed.as_millis() < 1500, "merge took {elapsed:?}");

    // Untouched: every entry (including the rebuilt document.xml, whose text did not change) matches.
    let after = raw_entries(&out);
    assert_eq!(before.len(), after.len());
    for ((na, a), (nb, b)) in before.iter().zip(&after) {
        assert_eq!(na, nb);
        if na != "word/document.xml" {
            assert!(same(a, b), "{na}: compressed bytes or header changed");
        }
    }

    // A translated document: only word/document.xml differs; the image is still the original compressed data.
    let first = doc.units[0].segments[0].id.clone();
    doc.set_target(&first, vec![Inline::text("Hola.")]).unwrap();
    let out = merge(&doc).unwrap();
    let after = raw_entries(&out);
    for ((na, a), (_, b)) in before.iter().zip(&after) {
        if na == "word/document.xml" {
            assert!(!same(a, b));
        } else {
            assert!(same(a, b), "{na}: changed by an unrelated edit");
        }
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&out)).unwrap();
    let mut text = String::new();
    z.by_name("word/document.xml").unwrap().read_to_string(&mut text).unwrap();
    assert!(text.contains("Hola."));
    let mut img = Vec::new();
    z.by_name("word/media/image1.png").unwrap().read_to_end(&mut img).unwrap();
    assert_eq!(img, noise(20 * 1024 * 1024));
}

/// A skeleton written before packed entries existed (schema 1: plain bytes, no `packed` map) still merges,
/// and gives the same file content back.
#[test]
fn schema_1_zip_skeletons_with_plain_bytes_still_merge() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {NS}><w:body><w:p><w:r><w:t>Old one.</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"
    );
    let blob = noise(4096).iter().map(|b| b % 7).collect::<Vec<u8>>();
    for (name, data) in [("[Content_Types].xml", CT.as_bytes()), ("_rels/.rels", RELS.as_bytes()), ("word/document.xml", doc.as_bytes()), ("word/media/a.bin", &blob[..])] {
        w.start_file(name, deflate).unwrap();
        w.write_all(data).unwrap();
    }
    let bytes = w.finish().unwrap().into_inner();
    let opts = ExtractOptions { source_lang: Some("en".into()), target_lang: None, granularity: Granularity::Sentence, include_locked: false, limits: None };
    let new = extract("a.docx", &bytes, &opts).unwrap();
    let mut json = serde_json::to_value(&new).unwrap();
    let zip_obj = json["skeleton"]["zip"].as_object_mut().unwrap();
    zip_obj["parser"].as_object_mut().unwrap().remove("schema");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    for e in zip_obj["entries"].as_array_mut().unwrap() {
        let name = e["name"].as_str().unwrap().to_string();
        if name.starts_with('\0') {
            let mut meta: serde_json::Value = {
                use base64_free::decode;
                serde_json::from_slice(&decode(e["data"]["raw"].as_str().unwrap())).unwrap()
            };
            meta.as_object_mut().unwrap().remove("packed");
            e["data"]["raw"] = serde_json::Value::String(base64_free::encode(&serde_json::to_vec(&meta).unwrap()));
        } else if e["data"].get("raw").is_some() {
            let mut plain = Vec::new();
            z.by_name(&name).unwrap().read_to_end(&mut plain).unwrap();
            e["data"]["raw"] = serde_json::Value::String(base64_free::encode(&plain));
        }
    }
    let old: Document = serde_json::from_value(json).unwrap();
    let out = merge(&old).unwrap();
    let mut z2 = zip::ZipArchive::new(std::io::Cursor::new(&out)).unwrap();
    let mut got = Vec::new();
    z2.by_name("word/media/a.bin").unwrap().read_to_end(&mut got).unwrap();
    assert_eq!(got, blob);
    let mut text = String::new();
    z2.by_name("word/document.xml").unwrap().read_to_string(&mut text).unwrap();
    assert!(text.contains("Old one."));
}

fn sentence_opts() -> ExtractOptions {
    ExtractOptions {
        source_lang: Some("en".into()),
        target_lang: Some("es".into()),
        granularity: Granularity::Sentence,
        include_locked: false,
        limits: None,
    }
}

fn raw_entry<'a>(doc: &'a mut Document, name: &str) -> &'a mut Vec<u8> {
    let Skeleton::Zip { entries, .. } = &mut doc.skeleton else { panic!("not a zip skeleton") };
    match &mut entries.iter_mut().find(|e| e.name == name).unwrap().data {
        ZipData::Raw(b) => b,
        ZipData::Parts(_) => panic!("{name} is not raw"),
    }
}

/// Every entry's name and unpacked bytes, in order.
fn contents(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut d = Vec::new();
            f.read_to_end(&mut d).unwrap();
            (f.name().unwrap().into_owned(), d)
        })
        .collect()
}

/// A small docx whose picture is deflated, so its skeleton keeps it packed.
fn small_docx() -> (Vec<u8>, Vec<u8>) {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {NS}><w:body><w:p><w:r><w:t>Small one.</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"
    );
    let pic: Vec<u8> = noise(50_000).iter().map(|b| b % 5).collect();
    for (name, data) in [("[Content_Types].xml", CT.as_bytes()), ("_rels/.rels", RELS.as_bytes()), ("word/document.xml", doc.as_bytes()), ("word/media/p.bin", &pic[..])] {
        w.start_file(name, deflate).unwrap();
        w.write_all(data).unwrap();
    }
    (w.finish().unwrap().into_inner(), pic)
}

/// Kept compressed bytes that no longer match their record (edited, cut short, or swapped for plain bytes by
/// some other tool) are refused at merge time instead of being copied into a broken file.
#[test]
fn packed_bytes_that_do_not_match_their_record_are_refused() {
    let (bytes, pic) = small_docx();
    let doc = extract("s.docx", &bytes, &sentence_opts()).unwrap();
    assert_eq!(contents(&merge(&doc).unwrap()), contents(&bytes));

    let mut flipped = doc.clone();
    let b = raw_entry(&mut flipped, "word/media/p.bin");
    let mid = b.len() / 2;
    b[mid] ^= 0x55;
    assert!(matches!(merge(&flipped), Err(Error::Write(m)) if m.contains("p.bin")));

    let mut short = doc.clone();
    raw_entry(&mut short, "word/media/p.bin").truncate(10);
    assert!(matches!(merge(&short), Err(Error::Write(_))));

    let mut plain = doc.clone();
    *raw_entry(&mut plain, "word/media/p.bin") = pic;
    assert!(matches!(merge(&plain), Err(Error::Write(_))));
}

/// The zip record names the packed entries; if it cannot be read, guessing would write deflated bytes as plain
/// ones, so merge refuses.
#[test]
fn an_unreadable_zip_record_is_an_error_not_a_guess() {
    let corpus = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut files = vec![("s.docx".to_string(), small_docx().0)];
    for f in ["docx/manual.docx", "xlsx/menu.xlsx", "pptx/launch.pptx"] {
        files.push((f.rsplit('/').next().unwrap().to_string(), std::fs::read(corpus.join(f)).unwrap()));
    }
    for (name, bytes) in files {
        let mut doc = extract(&name, &bytes, &sentence_opts()).unwrap();
        let meta_name = {
            let Skeleton::Zip { entries, .. } = &doc.skeleton else { panic!() };
            entries.iter().find(|e| e.name.starts_with(' ')).unwrap().name.clone()
        };
        *raw_entry(&mut doc, &meta_name) = b"{not json".to_vec();
        assert!(matches!(merge(&doc), Err(Error::Write(m)) if m.contains("unreadable")), "{name}");
    }
}

/// Stamps from before schemas (none at all, or a version only) merge to exactly the same bytes as the current
/// stamp, and an older schema number does too.
#[test]
fn legacy_stamps_merge_byte_identical() {
    let (bytes, _) = small_docx();
    let doc = extract("s.docx", &bytes, &sentence_opts()).unwrap();
    let want = merge(&doc).unwrap();
    let mut v = serde_json::to_value(&doc).unwrap();
    let parser = v["skeleton"]["zip"]["parser"].as_object_mut().unwrap();
    parser.remove("schema");
    parser.insert("version".into(), "0.1.0".into());
    let version_only: Document = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(merge(&version_only).unwrap(), want);
    v["skeleton"]["zip"].as_object_mut().unwrap().remove("parser");
    let unstamped: Document = serde_json::from_value(v).unwrap();
    assert_eq!(merge(&unstamped).unwrap(), want);
}

/// Entries written by a streaming zip writer carry data descriptors (sizes after the data); their compressed
/// bytes are copied all the same and the file reads back whole.
#[test]
fn entries_with_data_descriptors_copy_cleanly() {
    let mut w = zip::ZipWriter::new_stream(Vec::new());
    let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {NS}><w:body><w:p><w:r><w:t>Streamed. Mañana 你好 مرحبا.</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"
    );
    let pic: Vec<u8> = noise(30_000).iter().map(|b| b % 3).collect();
    for (name, data) in [("[Content_Types].xml", CT.as_bytes()), ("_rels/.rels", RELS.as_bytes()), ("word/document.xml", doc.as_bytes()), ("word/media/日本.bin", &pic[..])] {
        w.start_file(name, deflate).unwrap();
        w.write_all(data).unwrap();
    }
    let bytes = w.finish().unwrap().into_inner();
    let mut d = extract("s.docx", &bytes, &sentence_opts()).unwrap();
    assert_eq!(contents(&merge(&d).unwrap()), contents(&bytes));
    let first = d.units[0].segments[0].id.clone();
    d.set_target(&first, vec![Inline::text("Transmitido.")]).unwrap();
    let out = merge(&d).unwrap();
    let back = contents(&out);
    assert_eq!(back[3], ("word/media/日本.bin".to_string(), pic));
    // The output extracts again.
    let again = extract("s.docx", &out, &sentence_opts()).unwrap();
    assert!(again.units[0].segments[0].source.iter().any(|i| matches!(i, Inline::Text(t) if t.contains("Transmitido"))));
}

/// Standard base64, enough for the test (the model crate's codec is private).
mod base64_free {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(d: &[u8]) -> String {
        let mut s = String::new();
        for c in d.chunks(3) {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            for i in 0..4 {
                if i <= c.len() {
                    s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }
    pub fn decode(t: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let (mut acc, mut bits) = (0u32, 0);
        for ch in t.bytes().filter(|b| *b != b'=') {
            acc = acc << 6 | A.iter().position(|a| *a == ch).unwrap() as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits & 0xff) as u8);
            }
        }
        out
    }
}
