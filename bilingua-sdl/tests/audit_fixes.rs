//! Regression tests for the 0.9.1 security audit: a Trados project file with many language directions is read in
//! linear time, the XML guard runs over the project file and the bilingual files before they are parsed, and the
//! number of target languages is capped.

use bilingua_model::{Error, ExtractOptions, Filter, LimitKind, Limits};
use bilingua_sdl::filters;
use std::io::{Cursor, Write};
use std::time::{Duration, Instant};

fn filter(id: &str) -> Box<dyn Filter> {
    filters().into_iter().find(|f| f.id() == id).unwrap()
}

fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (n, d) in files {
        w.start_file(*n, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
        w.write_all(d.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

const SDLX: &str = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\"><file original=\"a.txt\" source-language=\"en-US\" target-language=\"de-DE\" datatype=\"x\"><body><trans-unit id=\"t\"><source>Save</source><seg-source><mrk mtype=\"seg\" mid=\"1\">Save</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\">Sichern</mrk></target><sdl:seg-defs><sdl:seg id=\"1\"/></sdl:seg-defs></trans-unit></body></file></xliff>";

/// A project file with `k` language directions (distinct target codes, or one code repeated), one listed file.
fn project(k: usize, distinct: bool, doctype: &str) -> String {
    let mut s = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>{doctype}<PackageProject PackageType=\"ProjectPackage\"><GeneralProjectInfo Name=\"p\"/>");
    for i in 0..k {
        s.push_str(&format!("<LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"x{:07}\"/>", if distinct { i } else { 0 }));
    }
    s.push_str("<LanguageFile LanguageCode=\"de-DE\"><FileVersion PhysicalPath=\"de-DE\\a.sdlxliff\"/></LanguageFile></PackageProject>");
    s
}

fn package(proj: &str) -> Vec<u8> {
    zip_of(&[("p.sdlproj", proj), ("de-DE/a.sdlxliff", SDLX)])
}

fn limit_kind(r: Result<bilingua_model::Document, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => e.kind,
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("expected a limit error, the package was accepted"),
    }
}

/// 80 000 distinct target codes took 19 s (debug build): every direction walked the list of all codes before it.
/// The codes are now looked up in a set, so the package reads about as fast as one with a single repeated code.
#[test]
fn many_language_directions_are_read_in_linear_time() {
    let f = filter("sdlppx");
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_zip_entries(100_000));
    let start = Instant::now();
    let d = f.extract(&package(&project(80_000, true, "")), &opts).expect("distinct codes");
    let took = start.elapsed();
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].id, "de-DE/a.sdlxliff|t");
    let start = Instant::now();
    assert_eq!(f.extract(&package(&project(80_000, false, "")), &opts).expect("one code").units.len(), 1);
    let control = start.elapsed();
    assert!(took < control * 4 + Duration::from_secs(2), "distinct codes took {took:?}, one repeated code {control:?}");
    assert!(f.merge(&d).is_ok());
}

/// More target languages than the package may hold entries is a typed limit error in the filter, and a plain
/// package error in the open-package API (which never gives typed ones).
#[test]
fn the_target_languages_are_capped() {
    let f = filter("sdlppx");
    let pkg = package(&project(10_001, true, ""));
    assert_eq!(limit_kind(f.extract(&pkg, &ExtractOptions::new())), LimitKind::XmlElements);
    assert!(matches!(bilingua_sdl::SdlPackage::<bilingua_xliff::Doc>::from_bytes(&pkg), Err(bilingua_sdl::Error::Package(_))));
    // at the cap, and any number of repeats of one code, are fine
    assert!(f.extract(&package(&project(10_000, true, "")), &ExtractOptions::new()).is_ok());
    assert!(f.extract(&package(&project(30_000, false, "")), &ExtractOptions::new()).is_ok());
}

/// The project file goes through the XML guard before it is parsed: an entity declaration, too many elements and
/// too deep a nesting are refused with their limit kinds.
#[test]
fn the_project_file_is_guarded() {
    let f = filter("sdlppx");
    let dtd = "<!DOCTYPE PackageProject [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;\">]>";
    assert_eq!(limit_kind(f.extract(&package(&project(1, true, dtd)), &ExtractOptions::new())), LimitKind::XmlEntities);
    let few = ExtractOptions::new().with_limits(Limits::new().with_max_xml_elements(1000));
    assert_eq!(limit_kind(f.extract(&package(&project(5000, false, "")), &few)), LimitKind::XmlElements);
    let deep = format!(
        "<PackageProject PackageType=\"ProjectPackage\">{}{}<LanguageFile LanguageCode=\"de-DE\"><FileVersion PhysicalPath=\"de-DE/a.sdlxliff\"/></LanguageFile></PackageProject>",
        "<a>".repeat(400),
        "</a>".repeat(400)
    );
    assert_eq!(limit_kind(f.extract(&package(&deep), &ExtractOptions::new())), LimitKind::XmlDepth);
}

/// A bilingual file is guarded and counted before and while it is opened, in both package families: an entity
/// declaration is refused as one, and a file over the unit limit is refused for its units.
#[test]
fn the_bilingual_files_are_guarded_before_parsing() {
    let bomb = SDLX.replacen("<xliff", "<!DOCTYPE xliff [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;\">]>\n<xliff", 1);
    let many = {
        let unit = "<trans-unit id=\"t\"><source>Save</source></trans-unit>";
        format!("<?xml version=\"1.0\"?><xliff version=\"1.2\" xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\"><file original=\"a\" source-language=\"en-US\" target-language=\"de-DE\" datatype=\"x\"><body>{}</body></file></xliff>", unit.repeat(300))
    };
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_units(100));
    let proj = project(1, true, "");
    let sdl = filter("sdlppx");
    assert_eq!(limit_kind(sdl.extract(&zip_of(&[("p.sdlproj", &proj), ("de-DE/a.sdlxliff", &bomb)]), &ExtractOptions::new())), LimitKind::XmlEntities);
    assert_eq!(limit_kind(sdl.extract(&zip_of(&[("p.sdlproj", &proj), ("de-DE/a.sdlxliff", &many)]), &small)), LimitKind::Units);
    assert_eq!(sdl.extract(&zip_of(&[("p.sdlproj", &proj), ("de-DE/a.sdlxliff", &many)]), &ExtractOptions::new()).unwrap().units.len(), 300);
    let mq = filter("mqxlz");
    let mq_file = |body: &str| zip_of(&[("document.mqxliff", &body.replace("xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\"", "xmlns:mq=\"MQXliff\""))]);
    assert_eq!(limit_kind(mq.extract(&mq_file(&bomb), &ExtractOptions::new())), LimitKind::XmlEntities);
    assert_eq!(limit_kind(mq.extract(&mq_file(&many), &small)), LimitKind::Units);
}

/// Roles and languages come from the project file as before: a path listed twice keeps its first language, a
/// listed report and anything under `Reports/` is a report, and only target-language files are work files.
#[test]
fn roles_and_languages_are_unchanged() {
    let proj = "<PackageProject PackageType=\"ProjectPackage\"><GeneralProjectInfo Name=\"p\"/><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"fr-FR\"/><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/><LanguageFile LanguageCode=\"en-US\"><FileVersion PhysicalPath=\"en-US\\a.sdlxliff\"/></LanguageFile><LanguageFile LanguageCode=\"de-DE\"><FileVersion PhysicalPath=\"de-DE\\a.sdlxliff\"/><FileVersion PhysicalPath=\"x\\r.sdlxliff\"/></LanguageFile><LanguageFile LanguageCode=\"en-US\"><FileVersion PhysicalPath=\"de-DE\\a.sdlxliff\"/></LanguageFile><Report PhysicalPath=\"x\\r.sdlxliff\"/></PackageProject>";
    let pkg = zip_of(&[("p.sdlproj", proj), ("en-US/a.sdlxliff", SDLX), ("de-DE/a.sdlxliff", SDLX), ("x/r.sdlxliff", SDLX), ("Reports/q.sdlxliff", SDLX), ("other.txt", "x")]);
    let p = bilingua_sdl::SdlPackage::<bilingua_xliff::Doc>::from_bytes(&pkg).unwrap();
    assert_eq!(p.target_languages(), ["de-DE", "fr-FR"]);
    assert_eq!(p.source_language(), "en-US");
    assert_eq!(p.document_paths(), ["de-DE/a.sdlxliff"]);
    let got: Vec<(String, bilingua_sdl::FileRole, Option<String>)> = p.files().into_iter().map(|f| (f.path, f.role, f.language)).collect();
    use bilingua_sdl::FileRole::*;
    let want = [
        ("p.sdlproj", Project, None),
        ("en-US/a.sdlxliff", Source, Some("en-US")),
        ("de-DE/a.sdlxliff", Target, Some("de-DE")),
        ("x/r.sdlxliff", Report, Some("de-DE")),
        ("Reports/q.sdlxliff", Report, None),
        ("other.txt", Other, None),
    ];
    assert_eq!(got, want.map(|(p, r, l)| (p.to_string(), r, l.map(String::from))));
}
