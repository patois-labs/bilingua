//! Gate attacks on the zip layer and the member writer: raw-copy fidelity, scan bypasses, id escaping,
//! injection through translations, BOM stamping.

mod common;
use bilingua_model::{Error, ExtractOptions, Inline, MergeOptions, OutputEncoding, SegState};
use common::*;

/// One central-directory record, split into the bytes that must survive a raw copy and where its data lies.
struct Cd {
    /// The whole record except the local-header offset (bytes 42..46).
    record: Vec<u8>,
    name: String,
    data: Vec<u8>,
}

fn u16at(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes([b[o], b[o + 1]]) as usize
}
fn u32at(b: &[u8], o: usize) -> usize {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize
}

fn central(z: &[u8]) -> Vec<Cd> {
    let eocd = z.windows(4).rposition(|w| w == [0x50, 0x4b, 5, 6]).unwrap();
    let (count, mut off) = (u16at(z, eocd + 10), u32at(z, eocd + 16));
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(&z[off..off + 4], &[0x50, 0x4b, 1, 2]);
        let (n, x, c) = (u16at(z, off + 28), u16at(z, off + 30), u16at(z, off + 32));
        let csize = u32at(z, off + 20);
        let lho = u32at(z, off + 42);
        let end = off + 46 + n + x + c;
        let mut record = z[off..off + 42].to_vec();
        // bit 3 (sizes in a data descriptor) is cleared by the copy: the local header then carries the sizes
        record[8] &= !0x08;
        // and the writer sets its own "version made by" / "version needed" (bytes 4..8)
        record[4..8].fill(0);
        record.extend_from_slice(&z[off + 46..end]);
        let name = String::from_utf8_lossy(&z[off + 46..off + 46 + n]).into_owned();
        let start = lho + 30 + u16at(z, lho + 26) + u16at(z, lho + 28);
        out.push(Cd { record, name, data: z[start..start + csize].to_vec() });
        off = end;
    }
    out
}

const RICH: &[u8] = include_bytes!("data/gate_rich.xlz");

#[test]
fn raw_copy_keeps_every_byte_of_untouched_members() {
    // per-member comments, extra fields, DOS attributes and data descriptors (tests/tools/gen_gate_rich.py)
    let f = filter();
    let mut d = extract(&*f, RICH);
    assert_eq!(f.merge(&d).unwrap(), RICH, "untouched merge is the original bytes");
    translate(&mut d, "content.xlf|1", |t| t.replace("Open the ", "Ouvrez la page "));
    let out = f.merge(&d).unwrap();
    let (a, b) = (central(RICH), central(&out));
    assert_eq!(a.iter().map(|c| &c.name).collect::<Vec<_>>(), b.iter().map(|c| &c.name).collect::<Vec<_>>());
    for i in 1..a.len() {
        assert_eq!(a[i].data, b[i].data, "{}: compressed bytes", a[i].name);
        assert_eq!(a[i].record, b[i].record, "{}: central-directory record (flags, dates, crc, attrs, extra, comment)", a[i].name);
    }
    assert_eq!(comment(&out), b"package comment");
    // the rewritten member keeps its name, date and method
    let (ma, mb) = (members(RICH), members(&out));
    assert_eq!((&ma[0].0, ma[0].1, ma[0].2), (&mb[0].0, mb[0].1, mb[0].2));
    assert!(text(&mb[0].3).contains("Ouvrez la page "));
    // and its member comment and DOS attributes (LocOS `_copy_info`)
    let z = zip::ZipArchive::new(std::io::Cursor::new(&out[..])).unwrap();
    let mut z = z;
    let m = z.by_index_raw(0).unwrap();
    assert_eq!(m.comment(), "xliff member");
    assert_eq!(m.external_attributes(), 0x20);
}

/// Two members with one name: built under two names, then renamed in every header.
fn dup_pkg() -> Vec<u8> {
    let z = zip_of(&[("a.xlf", WS, true), ("a.xlg", WS, true)], b"");
    let mut z = z;
    let mut i = 0;
    while i + 5 <= z.len() {
        if &z[i..i + 5] == b"a.xlg" {
            z[i..i + 5].copy_from_slice(b"a.xlf");
        }
        i += 1;
    }
    z
}

#[test]
fn a_fake_end_record_in_the_comment_does_not_hide_a_duplicate() {
    // The archive comment holds an end-of-directory signature with an empty directory. The name scan must not stop
    // at that fake record while the zip reader uses the real one.
    let mut z = dup_pkg();
    let n = z.len();
    let mut fake = vec![0x50u8, 0x4b, 5, 6];
    fake.extend([0u8; 18]);
    z[n - 2..n].copy_from_slice(&(fake.len() as u16).to_le_bytes());
    z.extend(&fake);
    match f_extract(&z) {
        Err(e) => assert!(!e.to_string().is_empty()),
        Ok(d) => panic!("a package with a hidden duplicate was read: {} segments", d.segment_count()),
    }
}

fn f_extract(z: &[u8]) -> Result<bilingua_model::Document, Error> {
    filter().extract(z, &ExtractOptions::default())
}

#[test]
fn segment_ids_from_paths_and_ids_cannot_collide() {
    // path "a.xlf" + id "x|1" against path "a.xlf|x" style names: the path part is escaped
    let tu = |id: &str| format!("<?xml version=\"1.0\"?><xliff version=\"1.2\"><file source-language=\"en\" original=\"o\"><body><trans-unit id=\"{id}\"><source>Hi {id}</source></trans-unit></body></file></xliff>");
    let one = tu("x|1");
    let two = tu("1");
    let three = tu("%7C1");
    let z = zip_of(&[("d/a.xlf", one.as_bytes(), true), ("d/a.xlf|x.xlf", two.as_bytes(), true), ("d/a.xlf%7Cx.xlf", three.as_bytes(), true)], b"");
    let d = match f_extract(&z) {
        Ok(d) => d,
        Err(e) => {
            // a refused name is fine too, as long as it is a typed refusal
            assert!(!e.to_string().is_empty());
            return;
        }
    };
    let ids = ids(&d);
    let set: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(set.len(), ids.len(), "{ids:?}");
    // each translation lands in its own member only
    let mut d = d;
    for id in ids.clone() {
        let w = format!("T[{id}]");
        d.set_target(&id, vec![Inline::text(w)]).unwrap();
        d.set_state(&id, SegState::Translated).unwrap();
    }
    let out = filter().merge(&d).unwrap();
    let back = f_extract(&out).unwrap();
    for id in ids {
        assert_eq!(locos(&back.find(&id).unwrap().1.target), format!("T[{id}]"));
    }
}

#[test]
fn translations_cannot_inject_markup() {
    let f = filter();
    let pkg = xlz();
    let mut d = extract_to(&*f, &pkg, "fr-FR\"><evil a=\"");
    let evil = "</target></trans-unit><trans-unit id=\"99\"><source>x</source></trans-unit><![CDATA[ ]]> &amp; & <!ENTITY e \"x\"> \u{0}\u{1}";
    d.set_target("content.xlf|1", vec![Inline::text(evil)]).unwrap();
    d.set_state("content.xlf|1", SegState::Translated).unwrap();
    let out = match f.merge(&d) {
        Ok(o) => o,
        Err(e) => {
            assert!(!e.to_string().is_empty());
            return;
        }
    };
    let inner = text(&member(&out, "content.xlf"));
    assert!(!inner.contains("<evil"), "the target language is never written raw");
    let back = f_extract(&out).unwrap();
    assert_eq!(back.segment_count(), extract(&*f, &pkg).segment_count(), "no new trans-unit");
    assert!(back.find("content.xlf|99").is_none());
}

#[test]
fn stamping_keeps_a_bom_member_intact() {
    let x = format!("\u{feff}{}", text(WS).replace(" target-language=\"fr-FR\"", ""));
    let pkg = zip_of(&[("a.xlf", x.as_bytes(), true)], b"");
    let f = filter();
    let mut d = extract_to(&*f, &pkg, "de-DE");
    translate(&mut d, "a.xlf|1", |t| format!("DE {t}"));
    let out = text(&member(&f.merge(&d).unwrap(), "a.xlf"));
    assert!(out.starts_with('\u{feff}'));
    let file = &out[out.find("<file").unwrap()..];
    let file = &file[..file.find('>').unwrap()];
    assert!(file.contains("target-language=\"de-DE\""), "{file}");
    assert!(f_extract(&zip_of(&[("a.xlf", out.as_bytes(), true)], b"")).is_ok());
}


/// `z` with its end record turned into the zip64 form (a zip64 end record and locator, the 32-bit fields 0xFFFF..).
fn as_zip64(z: &[u8]) -> Vec<u8> {
    let eocd = z.windows(4).rposition(|w| w == [0x50, 0x4b, 5, 6]).unwrap();
    let count = u16at(z, eocd + 10) as u64;
    let (size, off) = (u32at(z, eocd + 12) as u64, u32at(z, eocd + 16) as u64);
    let mut out = z[..eocd].to_vec();
    let z64 = out.len() as u64;
    out.extend([0x50, 0x4b, 6, 6]);
    out.extend(44u64.to_le_bytes());
    out.extend([45, 0, 45, 0]);
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(off.to_le_bytes());
    out.extend([0x50, 0x4b, 6, 7]);
    out.extend(0u32.to_le_bytes());
    out.extend(z64.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0]);
    out
}

#[test]
fn a_zip64_end_record_does_not_hide_a_duplicate() {
    // the zip64 form of a sound package still reads
    let ok = as_zip64(&xlz());
    assert!(f_extract(&ok).is_ok(), "{:?}", f_extract(&ok).err());
    let z = as_zip64(&dup_pkg());
    match f_extract(&z) {
        Err(e) => assert!(e.to_string().contains("duplicate"), "{e}"),
        Ok(d) => panic!("a zip64 package with a hidden duplicate was read: {} segments", d.segment_count()),
    }
}

#[test]
fn bytes_before_the_archive_do_not_hide_a_duplicate() {
    // a self-extractor style prefix: the directory offsets no longer point at the directory, the zip reader
    // finds it anyway
    let mut z = b"PK\x03\x04".to_vec();
    z.extend([0u8; 60]);
    z.extend(dup_pkg());
    match f_extract(&z) {
        Err(e) => assert!(!e.to_string().is_empty()),
        Ok(d) => panic!("a prefixed package with a hidden duplicate was read: {} segments", d.segment_count()),
    }
}


fn plain(name: &str, enc: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"{enc}\"?>\n<xliff version=\"1.2\"><file original=\"{name}\" source-language=\"en-US\"><body><trans-unit id=\"1\"><source>One.</source></trans-unit><trans-unit id=\"2\"><source>Two.</source></trans-unit></body></file></xliff>\n")
}

fn utf16le(s: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFE];
    for c in s.encode_utf16() {
        v.extend(c.to_le_bytes());
    }
    v
}

#[test]
fn merge_with_default_is_merge() {
    let f = filter();
    let pkg = zip_of(&[("a.xlf", plain("a", "UTF-8").as_bytes(), true), ("b.bin", b"zz", false)], b"");
    let mut d = extract_to(&*f, &pkg, "fr-FR");
    assert_eq!(f.merge_with(&d, &MergeOptions::default()).unwrap(), pkg);
    translate(&mut d, "a.xlf|1", |_| "Un.".into());
    assert_eq!(f.merge_with(&d, &MergeOptions::default()).unwrap(), f.merge(&d).unwrap());
}

#[test]
fn stamp_target_always_stamps_an_untranslated_package() {
    let f = filter();
    let pkg = zip_of(&[("a.xlf", plain("a", "UTF-8").as_bytes(), true), ("b.xliff", plain("b", "UTF-8").as_bytes(), true)], b"keep");
    let d = extract_to(&*f, &pkg, "ja-JP");
    assert_eq!(f.merge(&d).unwrap(), pkg, "default: nothing translated, original bytes");
    let always = MergeOptions { stamp_target_always: true, ..Default::default() };
    let out = f.merge_with(&d, &always).unwrap();
    for m in ["a.xlf", "b.xliff"] {
        assert!(text(&member(&out, m)).contains("target-language=\"ja-JP\""), "{m}");
    }
    assert_eq!(comment(&out), b"keep");
    // no target language: nothing to stamp, the original bytes
    let mut none = d.clone();
    none.target_lang = None;
    assert_eq!(f.merge_with(&none, &always).unwrap(), pkg);
    // a file that already declares its language is never changed, so the package comes back as it was
    let own = zip_of(&[("a.xlf", text(WS).as_bytes(), true)], b"");
    let d = extract_to(&*f, &own, "de-DE");
    assert_eq!(f.merge_with(&d, &always).unwrap(), own);
}

#[test]
fn output_encoding_utf8_rewrites_a_utf16_member_as_utf8() {
    let f = filter();
    let u16 = utf16le(&plain("u", "UTF-16"));
    let pkg = zip_of(&[("u.xlf", &u16, true), ("w.xlf", &utf16le(&plain("w", "UTF-16")), true)], b"");
    let mut d = extract_to(&*f, &pkg, "ko-KR");
    translate(&mut d, "u.xlf|1", |_| "\u{D558}\u{B098}.".into());
    let out = f.merge_with(&d, &MergeOptions { output_encoding: OutputEncoding::Utf8, ..Default::default() }).unwrap();
    let u = member(&out, "u.xlf");
    let s = String::from_utf8(u).expect("UTF-8 out");
    assert!(s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{s}");
    assert!(s.contains("\u{D558}\u{B098}."));
    // a member that is stamped but not otherwise translated is rewritten as well, and also goes out as UTF-8
    assert!(String::from_utf8(member(&out, "w.xlf")).unwrap().contains("target-language=\"ko-KR\""));
    let back = f.extract(&out, &ExtractOptions::default()).unwrap();
    assert_eq!(locos(&back.find("u.xlf|1").unwrap().1.target), "\u{D558}\u{B098}.");
    // keep (the default): the member stays UTF-16
    let kept = member(&f.merge(&d).unwrap(), "u.xlf");
    assert!(kept.starts_with(&[0xFF, 0xFE]));
    // LocOS's adapter options together
    let both = f.merge_with(&d, &MergeOptions::locos()).unwrap();
    assert!(String::from_utf8(member(&both, "u.xlf")).is_ok());
}

#[test]
fn encrypted_flag_on_the_local_header_only() {
    // the LocOS case: bit 0 set on the first local header, the central directory clean
    let mut z = xlz();
    z[6] |= 1;
    match f_extract(&z) {
        Err(e) => assert!(!e.to_string().is_empty()),
        Ok(mut d) => {
            // the data is not really encrypted: when it is read, a merge must still give a readable package
            assert_eq!(filter().merge(&d).unwrap(), z);
            translate(&mut d, "content.xlf|1", |_| "Un.".into());
            let out = filter().merge(&d).unwrap();
            assert!(text(&member(&out, "content.xlf")).contains("Un."));
        }
    }
}
