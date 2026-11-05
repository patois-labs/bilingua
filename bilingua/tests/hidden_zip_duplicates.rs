//! Every zip format through the facade: a package that lists one member twice is refused however its directory is
//! laid out (plain, a zip64 end record, bytes before the archive). The zip reader keeps one entry per name, so a
//! hand scan that gives up on the last two layouts would let the second copy vanish without a word. A sound package
//! rebuilt the same way still reads in every layout, so the refusal is the duplicate's, not the layout's.

use bilingua::*;
use bilingua_zip::hostile::{layouts, stored_zip};
use std::io::Read;

fn corpus(rel: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus").join(rel)).unwrap()
}

fn members(z: &[u8]) -> Members {
    let mut a = zip::ZipArchive::new(std::io::Cursor::new(z)).unwrap();
    (0..a.len())
        .map(|i| {
            let mut f = a.by_index(i).unwrap();
            let mut d = Vec::new();
            f.read_to_end(&mut d).unwrap();
            (f.name().unwrap().to_string(), d)
        })
        .collect()
}

fn rebuild(ms: &[(String, Vec<u8>)], twice: Option<&str>) -> Vec<u8> {
    let mut v: Vec<(&str, &[u8])> = ms.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
    if let Some(name) = twice {
        let (n, d) = ms.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no member {name}"));
        v.push((n.as_str(), d.as_slice()));
    }
    stored_zip(&v)
}

const PROJ: &str = "<?xml version=\"1.0\"?><PackageProject PackageType=\"ProjectPackage\"><GeneralProjectInfo Name=\"Demo\"/><LanguageDirections><SourceLanguageCode>en-US</SourceLanguageCode><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"ar-SA\"/></LanguageDirections><Files><LanguageFile LanguageCode=\"ar-SA\"><FileVersion PhysicalPath=\"ar-SA/a.sdlxliff\"/></LanguageFile></Files></PackageProject>";
const SDLX: &str = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\"><file original=\"a.txt\" source-language=\"en-US\" target-language=\"ar-SA\" datatype=\"x\"><body><trans-unit id=\"t\"><source>Save</source><seg-source><mrk mtype=\"seg\" mid=\"1\">Save</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\"></mrk></target><sdl:seg-defs><sdl:seg id=\"1\"/></sdl:seg-defs></trans-unit></body></file></xliff>";
const MQX: &str = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:mq=\"MQXliff\"><file source-language=\"en\" target-language=\"ar\" datatype=\"x\"><body><trans-unit id=\"1\" mq:status=\"NotStarted\"><source>Hello</source></trans-unit></body></file></xliff>";

type Members = Vec<(String, Vec<u8>)>;

/// (file name for the facade, its members, the member listed twice)
fn cases() -> Vec<(&'static str, Members, &'static str)> {
    let own = |ps: &[(&str, &str)]| ps.iter().map(|(n, d)| (n.to_string(), d.as_bytes().to_vec())).collect::<Vec<_>>();
    let idml = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/idml"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "idml"))
        .min()
        .unwrap();
    let idml = members(&std::fs::read(idml).unwrap());
    let story = idml.iter().find(|(n, _)| n.starts_with("Stories/")).unwrap().0.clone();
    vec![
        ("a.docx", members(&corpus("docx/manual.docx")), "word/document.xml"),
        ("a.xlsx", members(&corpus("xlsx/menu.xlsx")), "xl/sharedStrings.xml"),
        ("a.pptx", members(&corpus("pptx/launch.pptx")), "ppt/slides/slide1.xml"),
        ("a.odt", members(&corpus("odf/guide.odt")), "content.xml"),
        ("a.ods", members(&corpus("odf/sales.ods")), "content.xml"),
        ("a.odp", members(&corpus("odf/slides.odp")), "content.xml"),
        ("a.vsdx", members(&corpus("vsdx/fixture.vsdx")), "visio/pages/page1.xml"),
        ("a.epub", members(&corpus("epub/guide.epub")), "META-INF/container.xml"),
        ("a.xcloc", members(&corpus("xcloc/lumen_fr.xcloc")), "fr.xcloc/Localized Contents/fr.xliff"),
        ("a.idml", idml, Box::leak(story.into_boxed_str())),
        ("a.sdlppx", own(&[("p.sdlproj", PROJ), ("ar-SA/a.sdlxliff", SDLX)]), "ar-SA/a.sdlxliff"),
        ("a.mqxlz", own(&[("doc.mqxliff", MQX), ("note.txt", "n")]), "doc.mqxliff"),
    ]
}

fn opts() -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: Some("fr".into()), ..Default::default() }
}

#[test]
fn a_sound_package_reads_in_every_layout() {
    for (file, ms, _) in cases() {
        for (layout, z) in layouts(&rebuild(&ms, None)) {
            if let Err(e) = extract(file, &z, &opts()) {
                panic!("{file} ({layout}): a sound package was refused: {e}");
            }
        }
    }
}

#[test]
fn a_member_listed_twice_is_refused_in_every_layout_of_every_zip_format() {
    let mut read = Vec::new();
    for (file, ms, twice) in cases() {
        for (layout, z) in layouts(&rebuild(&ms, Some(twice))) {
            match extract(file, &z, &opts()) {
                Err(e) => {
                    let m = e.to_string();
                    assert!(m.contains("twice") || m.contains("repeats") || m.contains("duplicate"), "{file} ({layout}): {m}");
                }
                Ok(d) => read.push(format!("{file} ({layout}, {} units)", d.units.len())),
            }
        }
    }
    assert!(read.is_empty(), "read with a member listed twice: {}", read.join(", "));
}
