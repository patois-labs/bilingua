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
