//! The XML guard and the reader behind each format must agree on where a DOCTYPE, a comment or a processing
//! instruction ends. When they do not, markup the guard skipped as "inside the DOCTYPE" (or a comment) is read as
//! elements by the reader, and the depth and element limits (and the entity check) are bypassed.
//!
//! Every test hides a document behind one trick and runs it under limits the plain document breaks: the guard
//! must refuse it. The inputs are hand-written, or the repository's own fixtures with their parts rewritten.

use bilingua_model::{Error, ExtractOptions, LimitKind, Limits};
use std::io::{Read, Write};
use std::path::Path;

/// (name, text before the root element, text after the document); `{n}` is the root element's name.
const TRICKS: &[(&str, &str, &str)] = &[
    // quick-xml takes `<!doctype` in any case and ends it at the first '>' (quotes are not looked at)
    ("lowercase doctype, quoted '>'", "<!doctype {n} SYSTEM '>", ""),
    ("doctype, single-quoted '>'", "<!DOCTYPE {n} SYSTEM '>", ""),
    ("doctype, double-quoted '>'", "<!DOCTYPE {n} SYSTEM \">", ""),
    // a '[' the reader does not see opens a subset the guard never leaves
    ("unclosed subset", "<!DOCTYPE {n} [>", ""),
    ("subset closed after the root", "<!DOCTYPE {n} [>", "]>"),
    ("'>' in a subset comment", "<!DOCTYPE {n} [<!-- > -->", ""),
    // `<!-->` and `<?>` end early for a scanner that searches from the '<'
    ("comment starting with '>'", "<!-->", "<!-- -->"),
    ("comment starting with '->'", "<!--->", "<!-- -->"),
    ("empty processing instruction", "<?>", "<?x?>"),
];

fn doctype_end(b: &[u8], i: usize) -> usize {
    let (mut j, mut q, mut sub) = (i + 9, 0u8, 0usize);
    while j < b.len() {
        let c = b[j];
        if q != 0 {
            if c == q {
                q = 0;
            }
        } else if c == b'"' || c == b'\'' {
            q = c;
        } else if c == b'[' {
            sub += 1;
        } else if c == b']' {
            sub = sub.saturating_sub(1);
        } else if c == b'>' && sub == 0 {
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

/// `t` with its own DOCTYPE dropped and `pre` put right before the root element, `post` after the end.
fn hide(t: &str, pre: &str, post: &str) -> Option<String> {
    let b = t.as_bytes();
    let (mut i, mut prolog) = (0usize, String::new());
    loop {
        let s = i;
        while i < b.len() && (b[i].is_ascii_whitespace() || t[i..].starts_with('\u{FEFF}')) {
            i += if b[i].is_ascii_whitespace() { 1 } else { 3 };
        }
        prolog.push_str(&t[s..i]);
        let r = &b[i..];
        if r.starts_with(b"<?") {
            let e = t[i..].find("?>")? + i + 2;
            prolog.push_str(&t[i..e]);
            i = e;
        } else if r.starts_with(b"<!--") {
            let e = t[i + 4..].find("-->")? + i + 7;
            prolog.push_str(&t[i..e]);
            i = e;
        } else if r.len() > 9 && r[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            i = doctype_end(b, i);
        } else if r.len() > 1 && r[0] == b'<' && (r[1].is_ascii_alphabetic() || r[1] == b'_') {
            break;
        } else {
            return None;
        }
    }
    let ne = b[i + 1..].iter().position(|c| c.is_ascii_whitespace() || *c == b'>' || *c == b'/').map_or(b.len(), |p| p + i + 1);
    Some(format!("{prolog}{}{}{post}", pre.replace("{n}", &t[i + 1..ne]), &t[i..]))
}

fn xml_part(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".xml", ".rels", ".xlf", ".xliff", ".sdlxliff", ".vml"].iter().any(|e| n.ends_with(e))
}

/// The package with `hide` applied to every XML part.
fn hide_in_zip(z: &[u8], pre: &str, post: &str) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(z)).unwrap();
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..src.len() {
        let mut e = src.by_index(i).unwrap();
        let n = e.name().unwrap().to_string();
        let mut data = Vec::new();
        e.read_to_end(&mut data).unwrap();
        if xml_part(&n) {
            if let Some(h) = std::str::from_utf8(&data).ok().and_then(|s| hide(s, pre, post)) {
                data = h.into_bytes();
            }
        }
        let m = if n == "mimetype" { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
        w.start_file(n, zip::write::SimpleFileOptions::default().compression_method(m)).unwrap();
        w.write_all(&data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

fn opts(l: Limits) -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("es").with_limits(l)
}

fn over_limit(r: &Result<bilingua_model::Document, Error>) -> bool {
    matches!(r, Err(Error::Limit(e)) if matches!(e.kind, LimitKind::XmlDepth | LimitKind::XmlElements | LimitKind::XmlEntities))
}

/// `bytes` reads under the default limits and is refused under `tight`; every trick must be refused under `tight`
/// too (never read, never a panic).
fn every_trick_is_refused(name: &str, bytes: &[u8], tight: Limits) {
    let ok = bilingua::extract(name, bytes, &opts(Limits::default()));
    assert!(ok.is_ok(), "{name}: the plain file must read: {:?}", ok.err());
    assert!(over_limit(&bilingua::extract(name, bytes, &opts(tight))), "{name}: the plain file must break the tight limits");
    let zipped = bytes.starts_with(b"PK");
    let mut read = Vec::new();
    for (what, pre, post) in TRICKS {
        let hostile = if zipped {
            hide_in_zip(bytes, pre, post)
        } else {
            hide(std::str::from_utf8(bytes).unwrap(), pre, post).unwrap().into_bytes()
        };
        let r = std::panic::catch_unwind(|| bilingua::extract(name, &hostile, &opts(tight)));
        match r {
            Err(_) => read.push(format!("{what}: PANIC")),
            Ok(Ok(d)) => read.push(format!("{what}: read, {} units", d.units.len())),
            Ok(Err(_)) => {}
        }
    }
    assert!(read.is_empty(), "{name}: the limits were bypassed by {read:#?}");
}

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)).unwrap()
}

fn tight() -> Limits {
    Limits::new().with_max_xml_elements(3).with_max_xml_depth(2)
}

#[test]
fn xmlres_android_strings() {
    let s = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n<string name=\"a\">Open the door</string>\n<string-array name=\"b\"><item>One</item><item>Two</item></string-array>\n</resources>\n";
    every_trick_is_refused("strings.xml", s.as_bytes(), tight());
}

#[test]
fn plist_property_list() {
    let s = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n<key>Title</key>\n<array><string>Open the door</string><string>Close it</string></array>\n</dict>\n</plist>\n";
    every_trick_is_refused("Info.plist", s.as_bytes(), tight());
}

#[test]
fn image_svg() {
    let s = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><g><g><text x=\"1\" y=\"2\">Open the door</text></g></g></svg>\n";
    every_trick_is_refused("a.svg", s.as_bytes(), tight());
}

#[test]
fn wxl_localization() {
    let s = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<WixLocalization Culture=\"en-us\" xmlns=\"http://schemas.microsoft.com/wix/2006/localization\">\n<String Id=\"A\">Open the door</String>\n<String Id=\"B\">Close it</String>\n<String Id=\"C\">Again</String>\n</WixLocalization>\n";
    every_trick_is_refused("a.wxl", s.as_bytes(), tight());
}

#[test]
fn xliff_12() {
    let s = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file original=\"a.txt\" source-language=\"en\" target-language=\"es\" datatype=\"plaintext\"><body><trans-unit id=\"1\"><source>Open the door</source></trans-unit></body></file></xliff>\n";
    every_trick_is_refused("a.xlf", s.as_bytes(), tight());
}

#[test]
fn xliff_2() {
    let s = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"2.0\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" srcLang=\"en\" trgLang=\"es\"><file id=\"f\"><unit id=\"1\"><segment><source>Open the door</source></segment></unit></file></xliff>\n";
    every_trick_is_refused("a.xlf", s.as_bytes(), tight());
}

#[test]
fn resx_resources() {
    let s = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n<resheader name=\"resmimetype\"><value>text/microsoft-resx</value></resheader>\n<data name=\"A\" xml:space=\"preserve\"><value>Open the door</value></data>\n<data name=\"B\" xml:space=\"preserve\"><value>Close it</value></data>\n</root>\n";
    every_trick_is_refused("a.resx", s.as_bytes(), tight());
}

#[test]
fn docx_parts() {
    every_trick_is_refused("page.docx", &fixture("bilingua/tests/fixtures/page.docx"), tight());
}

#[test]
fn pptx_parts() {
    every_trick_is_refused("a.pptx", &fixture("bilingua/tests/locos/pptx/hostile_tails.pptx"), tight());
}

#[test]
fn xlsx_parts() {
    every_trick_is_refused("a.xlsx", &fixture("bilingua-xlsx/tests/data/modes/comments_basic.de.xlsx"), tight());
}

#[test]
fn vsdx_parts_never_panic() {
    every_trick_is_refused("a.vsdx", &fixture("oracle/corpus/vsdx/fixture.vsdx"), tight());
}

#[test]
fn xliffpkg_xlz_members() {
    every_trick_is_refused("a.xlz", &fixture("bilingua-xliffpkg/tests/data/gate_rich.xlz"), tight());
}

#[test]
fn sdl_package_members() {
    every_trick_is_refused("a.sdlppx", &fixture("oracle/corpus/sdlppx/trados_demo.sdlppx"), tight());
}

#[test]
fn xcstrings_xcloc_members() {
    every_trick_is_refused("a.xcloc", &fixture("oracle/corpus/xcloc/lumen_fr.xcloc"), tight());
}

/// The DOCTYPEs real files carry still pass, in the strict formats too.
#[test]
fn real_doctypes_still_read() {
    let plist = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>T</key><string>Open the door</string></dict></plist>\n";
    assert!(bilingua::extract("Info.plist", plist.as_bytes(), &opts(Limits::default())).is_ok());
    let svg = "<?xml version=\"1.0\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n<svg xmlns=\"http://www.w3.org/2000/svg\"><text>Open the door</text></svg>\n";
    assert!(bilingua::extract("a.svg", svg.as_bytes(), &opts(Limits::default())).is_ok());
    let xmb = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE messagebundle [\n<!ELEMENT messagebundle (msg)*>\n<!ATTLIST messagebundle class CDATA #IMPLIED>\n<!-- one message per string -->\n<!ELEMENT msg (#PCDATA|ph)*>\n<!ATTLIST msg id CDATA #IMPLIED desc CDATA #IMPLIED>\n]>\n<messagebundle><msg id=\"1\" desc=\"door\">Open the door</msg></messagebundle>\n";
    let d = bilingua::extract("a.xmb", xmb.as_bytes(), &opts(Limits::default()));
    assert!(d.is_ok(), "{:?}", d.err());
}
