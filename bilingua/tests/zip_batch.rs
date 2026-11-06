//! ZIP batches (`bilingua::batch`), ported from Localization OS `tests/verify_zip_batch.py` (sections A and B; C to
//! E drive the web, API and CLI upload doors, which are LocOS's). Archives are built here.

use bilingua::batch::{self, BatchOptions};
use bilingua::{Error, LimitKind, Limits};
use proptest::prelude::*;
use std::io::Write;

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (n, d) in entries {
        if n.ends_with('/') {
            w.add_directory(*n, opt).unwrap();
        } else {
            w.start_file(*n, opt).unwrap();
            w.write_all(d).unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

const MD: &[u8] = b"# Getting started\n\nInstall the app. Then open it.\n";
const TXT: &[u8] = b"Hello world.\nSecond line here.\n";
const JSON: &[u8] = b"{\"title\": \"Save {count} files\", \"body\": \"Click %s to continue.\"}\n";

fn good_zip() -> Vec<u8> {
    let nested = zip_of(&[("inner.md", b"# inner\n")]);
    zip_of(&[
        ("docs/", b""),
        ("docs/guide/intro.md", MD),
        ("readme.txt", TXT),
        ("data/strings.json", JSON),
        ("tools/setup.exe", b"MZ\x00\x00"),
        ("archive/nested.zip", &nested),
        ("__MACOSX/docs/._intro.md", b"\x00\x05"),
        (".DS_Store", b"\x00\x00"),
        ("blank.txt", b"   \n\n"),
        ("noext", b"plain"),
        ("docs/Thumbs.db", b"\x00"),
    ])
}

fn limit_kind(r: Result<batch::Batch, Error>) -> Option<LimitKind> {
    match r {
        Err(Error::Limit(l)) => Some(l.kind),
        _ => None,
    }
}

#[test]
fn members_keep_their_folders_and_every_skip_has_a_reason() {
    let b = batch::expand(&good_zip()).unwrap();
    let paths: Vec<&str> = b.members.iter().map(|m| m.path.as_str()).collect();
    // blank.txt is a supported format (it fails later, at extract): it is a member here
    assert_eq!(paths, ["docs/guide/intro.md", "readme.txt", "data/strings.json", "blank.txt"]);
    assert_eq!(b.members[0].name, "docs__guide__intro.md");
    assert_eq!(b.members[0].data, MD);
    let why = |p: &str| b.skipped.iter().find(|s| s.path == p).map(|s| s.reason.clone()).unwrap_or_else(|| panic!("{p} not skipped"));
    assert!(why("tools/setup.exe").starts_with("unsupported format (.exe)"));
    assert!(why("archive/nested.zip").contains("nested ZIP"));
    assert!(why("__MACOSX/docs/._intro.md").contains("system file"));
    assert!(why(".DS_Store").contains("system file"));
    assert!(why("docs/Thumbs.db").contains("system file"));
    assert!(why("noext").contains("no extension"));
    assert_eq!(b.skipped.len(), 6);
    // every member opens as if it had been uploaded on its own
    for m in &b.members[..3] {
        let d = bilingua::extract(&m.name, &m.data, &bilingua::ExtractOptions::default()).unwrap();
        assert!(!d.units.is_empty(), "{}", m.name);
    }
}

#[test]
fn batch_extensions() {
    assert!(batch::is_batch("x.ZIP") && batch::is_batch("site.zip") && batch::is_batch("C:\\up\\site.zip"));
    assert!(!batch::is_batch("x.flprjzip") && !batch::is_batch("zip") && !batch::is_batch("a.zip/readme"));
    // a .zip is never a format: detect refuses it, so a plain extract cannot open a batch
    assert!(bilingua::detect("site.zip", &good_zip()).is_none());
}

#[test]
fn unsafe_paths_refuse_the_whole_archive() {
    for bad in ["../evil.md", "docs/../../evil.md", "/etc/evil.md", "C:/evil.md", "C:\\evil.md", "docs\\..\\..\\evil.md", "a\u{1}b.md", "trail./x.md", " lead/x.md"] {
        let z = zip_of(&[("ok.md", MD), (bad, MD)]);
        // every refusal of a real archive is a limit error with a message for the uploader (LocOS InputLimitError)
        match batch::expand(&z) {
            Err(Error::Limit(l)) => assert!(l.kind == LimitKind::ZipMalformed && l.msg.contains("unsafe path"), "{bad:?}: {l:?}"),
            other => panic!("{bad:?}: {other:?}"),
        }
    }
    for raw in ["../x", "/x", "c:x", "", "a/./../b", "a\u{7f}b", "x /y.md", "x./y.md"] {
        assert_eq!(batch::safe_member_path(raw), None, "{raw:?}");
    }
    assert_eq!(batch::safe_member_path("a/./b//c.md").as_deref(), Some("a/b/c.md"));
    assert_eq!(batch::safe_member_path("a\\b\\c.md").as_deref(), Some("a/b/c.md"));
    assert_eq!(batch::document_name("a/b/c.md"), "a__b__c.md");
}

#[test]
fn stricter_than_locos_duplicates_by_case_refuse() {
    // LocOS skips the second spelling; the shared zip guard refuses the archive (documented difference)
    let z = zip_of(&[("docs/guide/intro.md", MD), ("DOCS/GUIDE/INTRO.md", MD)]);
    assert_eq!(limit_kind(batch::expand(&z)), Some(LimitKind::ZipMalformed));
}

#[test]
fn hidden_duplicates_links_and_unicode_case_twins() {
    // the same name twice in the central directory: the zip reader keeps one and hides the other; in every layout
    // (plain, zip64 end record, bytes before the archive) the archive is refused
    let twice = bilingua_zip::hostile::stored_zip(&[("a.md", MD), ("b.txt", TXT), ("a.md", b"# evil\n")]);
    for (layout, z) in bilingua_zip::hostile::layouts(&twice) {
        assert_eq!(limit_kind(batch::expand(&z)), Some(LimitKind::ZipMalformed), "{layout}");
    }
    // a symbolic link refuses the archive (LocOS skips it)
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file("ok.md", zip::write::SimpleFileOptions::default()).unwrap();
    w.write_all(MD).unwrap();
    w.add_symlink("docs/link.md", "/etc/passwd", zip::write::SimpleFileOptions::default()).unwrap();
    let z = w.finish().unwrap().into_inner();
    assert_eq!(limit_kind(batch::expand(&z)), Some(LimitKind::ZipMalformed));
    // names that differ only in non-ASCII case pass the guard and are skipped as duplicates, as LocOS
    let b = batch::expand(&zip_of(&[("Ä.md", MD), ("ä.md", MD)])).unwrap();
    assert_eq!((b.members.len(), b.skipped.len()), (1, 1));
    assert!(b.skipped[0].reason.contains("duplicate path"));
    // a member comes back byte for byte (stored, and deflated above)
    let raw: Vec<u8> = b"line one\r\n\xc3\xa9t\xc3\xa9\r\nno newline at end".to_vec();
    let b = batch::expand(&bilingua_zip::hostile::stored_zip(&[("bin/data.txt", &raw)])).unwrap();
    assert_eq!(b.members[0].data, raw);
}

#[test]
fn bombs_forged_sizes_and_caps() {
    // 50 MB of zeros deflates about 1000 times
    let bomb = zip_of(&[("bomb.txt", &vec![0u8; 50 * 1024 * 1024])]);
    assert_eq!(limit_kind(batch::expand(&bomb)), Some(LimitKind::ZipRatio));
    // a central directory that understates the size
    let mut z = zip_of(&[("a.txt", &[b'A'; 5000])]);
    let cd = z.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
    z[cd + 24..cd + 28].copy_from_slice(&10u32.to_le_bytes());
    assert!(batch::expand(&z).is_err(), "forged size accepted");
    // entry count
    let four: Vec<(String, &[u8])> = (0..4).map(|i| (format!("f{i}.txt"), TXT)).collect();
    let four: Vec<(&str, &[u8])> = four.iter().map(|(n, d)| (n.as_str(), *d)).collect();
    let small = BatchOptions::new().with_limits(Limits::default().with_max_zip_entries(3));
    assert_eq!(limit_kind(batch::expand_with(&zip_of(&four), &small)), Some(LimitKind::ZipEntries));
    // document cap; skipped members do not count
    let two = BatchOptions::new().with_max_documents(2);
    let e = batch::expand_with(&zip_of(&four[..3]), &two).unwrap_err();
    assert!(e.to_string().contains("ZIP_BATCH_MAX_DOCUMENTS"), "{e}");
    let b = batch::expand_with(&zip_of(&[("a.txt", TXT), ("b.exe", b"x"), ("c.exe", b"x")]), &two).unwrap();
    assert_eq!((b.members.len(), b.skipped.len()), (1, 2));
    // a member over the upload limit is skipped, not refused
    let tiny = BatchOptions::new().with_limits(Limits::default().with_max_input_bytes(10));
    let b = batch::expand_with(&zip_of(&[("a.txt", TXT), ("b.txt", b"short")]), &tiny).unwrap();
    assert_eq!(b.members.iter().map(|m| m.path.as_str()).collect::<Vec<_>>(), ["b.txt"]);
    assert!(b.skipped[0].reason.contains("larger than the upload limit"));
}

#[test]
fn junk_is_not_a_zip() {
    for junk in [&b""[..], b"PK\x03\x04hello", b"not a zip"] {
        match batch::expand(junk) {
            Err(Error::Parse { format, msg }) => assert!(format == "zip" && msg.contains("not a readable ZIP"), "{msg}"),
            other => panic!("{junk:?}: {other:?}"),
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = batch::expand(&bytes);
    }

    #[test]
    fn mutated_archives_never_panic(at in 0usize..4096, val in any::<u8>()) {
        let mut z = good_zip();
        let i = at % z.len();
        z[i] = val;
        let _ = batch::expand(&z);
    }

    #[test]
    fn member_paths_are_never_unsafe(raw in "[a-zA-Z0-9 ./\\\\:]{0,24}") {
        if let Some(p) = batch::safe_member_path(&raw) {
            prop_assert!(!p.starts_with('/') && !p.split('/').any(|c| c == ".." || c.is_empty() || c == "."));
            prop_assert!(!p.contains('\\'));
        }
    }
}
