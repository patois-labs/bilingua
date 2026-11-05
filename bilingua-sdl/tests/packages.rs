//! Package filters over made-up packages built in memory: Arabic and Japanese targets, untouched and edited.

use bilingua_model::{plain_text, ExtractOptions, Filter, Inline, SegState};
use bilingua_sdl::filters;
use std::io::{Cursor, Read, Write};

fn filter(id: &str) -> Box<dyn Filter> {
    filters().into_iter().find(|f| f.id() == id).unwrap()
}

fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in files {
        w.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(d.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn read(zip_bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

const PROJ: &str = "<?xml version=\"1.0\"?><PackageProject PackageType=\"ProjectPackage\"><GeneralProjectInfo Name=\"Demo\"/><LanguageDirections><SourceLanguageCode>en-US</SourceLanguageCode><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"ar-SA\"/><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"ja-JP\"/></LanguageDirections><Files><LanguageFile LanguageCode=\"en-US\"><FileVersion PhysicalPath=\"en-US/a.sdlxliff\"/></LanguageFile><LanguageFile LanguageCode=\"ar-SA\"><FileVersion PhysicalPath=\"ar-SA/a.sdlxliff\"/></LanguageFile><LanguageFile LanguageCode=\"ja-JP\"><FileVersion PhysicalPath=\"ja-JP/a.sdlxliff\"/></LanguageFile></Files></PackageProject>";

fn sdlx(lang: &str, tgt: &str) -> String {
    format!("<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\"><file original=\"a.txt\" source-language=\"en-US\" target-language=\"{lang}\" datatype=\"x\"><body><trans-unit id=\"t\"><source>Save</source><seg-source><mrk mtype=\"seg\" mid=\"1\">Save</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\">{tgt}</mrk></target><sdl:seg-defs><sdl:seg id=\"1\"/></sdl:seg-defs></trans-unit></body></file></xliff>")
}

#[test]
fn sdl_package_reads_target_files_and_writes_edits_back() {
    let ar = sdlx("ar-SA", "\u{62D}\u{641}\u{638}");
    let ja = sdlx("ja-JP", "");
    let en = sdlx("en-US", "");
    let pkg = zip_of(&[("p.sdlproj", PROJ), ("en-US/a.sdlxliff", &en), ("ar-SA/a.sdlxliff", &ar), ("ja-JP/a.sdlxliff", &ja), ("Reports/r.txt", "report")]);
    let f = filter("sdlppx");
    let mut d = f.extract(&pkg, &ExtractOptions::default()).unwrap();
    // Two target files; the source copy is not a work file.
    assert_eq!(d.units.len(), 2);
    let ar_id = "ar-SA/a.sdlxliff|1";
    let ja_id = "ja-JP/a.sdlxliff|1";
    assert_eq!(plain_text(&d.find(ar_id).unwrap().1.target), "\u{62D}\u{641}\u{638}");
    assert_eq!(d.find(ja_id).unwrap().1.state, SegState::New);
    // Untouched: every entry comes back with the same content.
    let same = f.merge(&d).unwrap();
    for n in ["p.sdlproj", "en-US/a.sdlxliff", "ar-SA/a.sdlxliff", "ja-JP/a.sdlxliff", "Reports/r.txt"] {
        assert_eq!(read(&same, n), read(&pkg, n), "{n}");
    }
    d.set_target(ja_id, vec![Inline::text("\u{4FDD}\u{5B58}")]).unwrap();
    d.set_state(ja_id, SegState::Translated).unwrap();
    let out = f.merge(&d).unwrap();
    assert!(read(&out, "ja-JP/a.sdlxliff").contains("\u{4FDD}\u{5B58}"));
    assert!(read(&out, "ja-JP/a.sdlxliff").contains("conf=\"Translated\""));
    assert_eq!(read(&out, "ar-SA/a.sdlxliff"), ar);
}

#[test]
fn mq_package_round_trips_and_refuses_a_zip_without_mqxliff() {
    let x = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:mq=\"MQXliff\"><file source-language=\"en\" target-language=\"ar\" datatype=\"x\"><body><trans-unit id=\"1\" mq:status=\"NotStarted\"><source>Hello</source></trans-unit></body></file></xliff>";
    let pkg = zip_of(&[("doc.mqxliff", x), ("note.txt", "n")]);
    let f = filter("mqxlz");
    let mut d = f.extract(&pkg, &ExtractOptions::default()).unwrap();
    assert_eq!(d.units.len(), 1);
    d.set_target("doc.mqxliff|1", vec![Inline::text("\u{645}\u{631}\u{62D}\u{628}\u{627}")]).unwrap();
    d.set_state("doc.mqxliff|1", SegState::Translated).unwrap();
    let out = f.merge(&d).unwrap();
    let got = read(&out, "doc.mqxliff");
    assert!(got.contains("mq:status=\"ManuallyConfirmed\"") && got.contains("<target>\u{645}\u{631}\u{62D}\u{628}\u{627}</target>"), "{got}");
    assert_eq!(read(&out, "note.txt"), "n");
    assert!(f.extract(&zip_of(&[("a.txt", "x")]), &ExtractOptions::default()).is_err());
}

#[test]
fn a_bar_in_a_package_path_does_not_send_one_file_s_segments_into_another() {
    // `a.mqxliff|b.mqxliff` is a legal zip name. Written plainly, its segment `1` and the segment `b.mqxliff|1` of
    // `a.mqxliff` would both get the id `a.mqxliff|b.mqxliff|1`; the bar in the path is escaped instead.
    let mq = |id: &str| format!("<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:mq=\"MQXliff\"><file source-language=\"en\" target-language=\"ja\" datatype=\"x\"><body><trans-unit id=\"{id}\" mq:status=\"NotStarted\"><source>Hello</source></trans-unit></body></file></xliff>");
    let a = mq("b.mqxliff|1");
    let b = mq("1");
    let pkg = zip_of(&[("a.mqxliff", &a), ("a.mqxliff|b.mqxliff", &b)]);
    let f = filter("mqxlz");
    let mut d = f.extract(&pkg, &ExtractOptions::default()).unwrap();
    assert_eq!(d.units.len(), 2);
    assert!(d.find("a.mqxliff|b.mqxliff|1").is_some_and(|(u, _)| u.context.location.as_deref() == Some("a.mqxliff")));
    let id = "a.mqxliff%7Cb.mqxliff|1";
    assert_eq!(d.find(id).unwrap().0.context.location.as_deref(), Some("a.mqxliff|b.mqxliff"));
    d.set_target(id, vec![Inline::text("\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}")]).unwrap();
    let out = f.merge(&d).unwrap();
    assert_eq!(read(&out, "a.mqxliff"), a, "the first file must stay untouched");
    assert!(read(&out, "a.mqxliff|b.mqxliff").contains("\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}"));
}

fn limit_kind(r: Result<bilingua_model::Document, bilingua_model::Error>) -> Option<bilingua_model::LimitKind> {
    match r {
        Err(bilingua_model::Error::Limit(e)) => Some(e.kind),
        _ => None,
    }
}

fn sample_package() -> Vec<u8> {
    let en = sdlx("en-US", "");
    let ar = sdlx("ar-SA", "\u{62D}\u{641}\u{638}");
    zip_of(&[("p.sdlproj", PROJ), ("en-US/a.sdlxliff", &en), ("ar-SA/a.sdlxliff", &ar)])
}

// Package zips use the B6 limits of the call (Localization OS's numbers by default), as a typed Limit error.
#[test]
fn package_zip_limits_come_from_the_extract_options() {
    let pkg = sample_package();
    let f = filter("sdlppx");
    assert!(f.extract(&pkg, &ExtractOptions::default()).is_ok());
    let o = |l: bilingua_model::Limits| ExtractOptions { limits: Some(l), ..Default::default() };
    let base = bilingua_model::Limits::default();
    assert_eq!(limit_kind(f.extract(&pkg, &o(bilingua_model::Limits { max_zip_entries: 2, ..base }))), Some(bilingua_model::LimitKind::ZipEntries));
    assert_eq!(limit_kind(f.extract(&pkg, &o(bilingua_model::Limits { max_zip_bytes: 50, ..base }))), Some(bilingua_model::LimitKind::ZipBytes));
    assert_eq!(limit_kind(f.extract(&pkg, &o(bilingua_model::Limits { max_input_bytes: 100, ..base }))), Some(bilingua_model::LimitKind::InputBytes));
    // the ratio check: a large repetitive entry
    let big = "x".repeat(3 << 20);
    let bomb = {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file("p.sdlproj", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
        w.write_all(PROJ.as_bytes()).unwrap();
        w.start_file("en-US/a.sdlxliff", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
        w.write_all(big.as_bytes()).unwrap();
        w.finish().unwrap().into_inner()
    };
    assert_eq!(limit_kind(f.extract(&bomb, &ExtractOptions::default())), Some(bilingua_model::LimitKind::ZipRatio));
    // a caller can raise the ratio; the file is then refused for what it is (not an XLIFF), not for its size
    let lax = o(bilingua_model::Limits { max_zip_ratio: 1_000_000, ..base });
    assert_eq!(limit_kind(f.extract(&bomb, &lax)), None);
}

// The defaults changed from this crate's own (20 000 entries, 512 MB per entry, 2 GB) to Localization OS's.
#[test]
fn default_zip_limits_are_locos_numbers() {
    let z = bilingua_sdl::ZipLimits::default();
    let l = bilingua_model::Limits::default();
    assert_eq!((z.max_entries, z.max_total_bytes, z.max_ratio), (l.max_zip_entries, l.max_zip_bytes, l.max_zip_ratio));
    assert_eq!(z.max_entries, 10_000);
    // the open-package API keeps plain Package errors; the Filters ask for typed ones
    assert!(!z.typed_errors && bilingua_sdl::ZipLimits::from(&l).typed_errors);
}

// A merge re-reads the entries it was given; limits the caller raised for extract must not refuse it.
#[test]
fn merge_works_with_raised_limits() {
    let pkg = sample_package();
    let f = filter("sdlppx");
    let base = bilingua_model::Limits::default();
    let opts = ExtractOptions { limits: Some(bilingua_model::Limits { max_zip_bytes: 1 << 40, ..base }), ..Default::default() };
    let d = f.extract(&pkg, &opts).unwrap();
    assert!(f.merge(&d).is_ok());
}
