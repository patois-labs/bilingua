//! Shared fixtures: the packages the Localization OS suite builds (`tests/verify_odf_filter.py`), and helpers.
#![allow(dead_code)]

use bilingua_model::locos::{from_locos_text, to_locos_text};
use bilingua_model::*;
use bilingua_odf::{Odg, Odp, Ods, Odt};
use bilingua_zip::{write_zip, EntryMeta, RawEntry, ZipLimits};

pub const NSDECL: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.3""#;

pub const MIME_TEXT: &str = "application/vnd.oasis.opendocument.text";
pub const MIME_SHEET: &str = "application/vnd.oasis.opendocument.spreadsheet";
pub const MIME_SLIDES: &str = "application/vnd.oasis.opendocument.presentation";
pub const MIME_DRAW: &str = "application/vnd.oasis.opendocument.graphics";

fn xml(body: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", body.replace("NSDECL", NSDECL))
}

pub fn odt_content() -> String {
    xml(concat!(
        "<office:document-content NSDECL><office:body><office:text>",
        r#"<text:tracked-changes><text:changed-region text:id="ct1"><text:deletion>"#,
        "<office:change-info><dc:creator>Ann</dc:creator></office:change-info>",
        "<text:p>Deleted words</text:p></text:deletion></text:changed-region>",
        r#"<text:changed-region text:id="ct2"><text:insertion><office:change-info>"#,
        "<dc:creator>Ann</dc:creator></office:change-info></text:insertion></text:changed-region>",
        "</text:tracked-changes>",
        r#"<text:h text:style-name="H1" text:outline-level="1">Getting started</text:h>"#,
        r#"<text:p text:style-name="P1">Press <text:span text:style-name="T1">Save</text:span> "#,
        r#"to keep {0} files.<text:note text:id="ftn1" text:note-class="footnote">"#,
        "<text:note-citation>1</text:note-citation><text:note-body>",
        "<text:p>A footnote about %1$s.</text:p></text:note-body></text:note></text:p>",
        r#"<text:p>Two<text:s text:c="3"/>gaps<text:tab/>and a <text:a xlink:href="https://x.example/">"#,
        r#"link</text:a><text:bookmark text:name="b1"/>.</text:p>"#,
        "<text:p>Before<office:annotation><dc:creator>Bo</dc:creator><text:p>Comment text</text:p>",
        r#"</office:annotation> after <text:change-start text:change-id="ct2"/>insert"#,
        r#"<text:change-end text:change-id="ct2"/>.<text:change text:change-id="ct1"/></text:p>"#,
        r#"<text:p>Caption<draw:frame draw:name="F1"><draw:text-box><text:p>Inside the box</text:p>"#,
        "</draw:text-box></draw:frame></text:p>",
        r#"<table:table><table:table-row><table:table-cell office:value-type="string">"#,
        "<text:p>Cell text</text:p></table:table-cell>",
        r#"<table:table-cell office:value-type="float" office:value="3"><text:p>3</text:p>"#,
        "</table:table-cell></table:table-row></table:table>",
        "<text:p>   </text:p><text:p>12345</text:p>",
        "<text:p>Unicode caf\u{e9} \u{65e5}\u{672c} \u{1F600}</text:p>",
        "</office:text></office:body></office:document-content>"
    ))
}

pub fn odt_styles() -> String {
    xml(concat!(
        "<office:document-styles NSDECL><office:styles>",
        r#"<style:default-style style:family="paragraph"><style:text-properties fo:language="en" "#,
        r#"fo:country="US"/></style:default-style>"#,
        r#"<style:style style:name="Standard" style:family="paragraph"/></office:styles>"#,
        r#"<office:master-styles><style:master-page style:name="Standard">"#,
        "<style:header><text:p>Header line</text:p></style:header>",
        "<style:footer><text:p>Page <text:page-number>1</text:page-number></text:p></style:footer>",
        "</style:master-page></office:master-styles></office:document-styles>"
    ))
}

pub fn meta_xml() -> String {
    xml("<office:document-meta NSDECL><office:meta><dc:title>T</dc:title><dc:language>en-US</dc:language></office:meta></office:document-meta>")
}

pub fn manifest(mimetype: &str, names: &[&str]) -> String {
    let mut entries = format!(r#"<manifest:file-entry manifest:full-path="/" manifest:media-type="{mimetype}"/>"#);
    for n in names {
        entries.push_str(&format!(r#"<manifest:file-entry manifest:full-path="{n}" manifest:media-type="text/xml"/>"#));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\">{entries}</manifest:manifest>"
    )
}

pub struct Pkg<'a> {
    pub content: String,
    pub styles: Option<String>,
    pub mimetype: Option<&'a str>,
    pub manifest: Option<String>,
    pub meta: Option<String>,
    pub extra: Vec<(String, Vec<u8>)>,
}

impl<'a> Pkg<'a> {
    pub fn new(content: String) -> Pkg<'a> {
        Pkg { content, styles: None, mimetype: Some(MIME_TEXT), manifest: None, meta: Some(meta_xml()), extra: vec![] }
    }
    pub fn styles(mut self, s: String) -> Self {
        self.styles = Some(s);
        self
    }
    pub fn mime(mut self, m: &'a str) -> Self {
        self.mimetype = Some(m);
        self
    }
    pub fn no_mimetype(mut self) -> Self {
        self.mimetype = None;
        self
    }
    pub fn manifest(mut self, m: String) -> Self {
        self.manifest = Some(m);
        self
    }
    pub fn no_meta(mut self) -> Self {
        self.meta = None;
        self
    }
    pub fn extra(mut self, name: &str, data: Vec<u8>) -> Self {
        self.extra.push((name.to_string(), data));
        self
    }

    /// A minimal valid package: the manifest lists every member, as LibreOffice requires.
    pub fn build(&self) -> Vec<u8> {
        let mime = self.mimetype.unwrap_or("application/zip");
        let mut names = vec!["content.xml"];
        if self.styles.is_some() {
            names.push("styles.xml");
        }
        if self.meta.is_some() {
            names.push("meta.xml");
        }
        let manifest = self.manifest.clone().unwrap_or_else(|| {
            let mut m = manifest(mime, &names);
            let mut more = String::from(r#"<manifest:file-entry manifest:full-path="Pictures/a.png" manifest:media-type="image/png"/>"#);
            for (n, _) in &self.extra {
                more.push_str(&format!(r#"<manifest:file-entry manifest:full-path="{n}" manifest:media-type=""/>"#));
            }
            m = m.replace("</manifest:manifest>", &format!("{more}</manifest:manifest>"));
            m
        });
        let em = |name: &str, stored: bool| EntryMeta { name: name.to_string(), stored, dir: false, modified: Some([2026, 11, 4, 9, 0, 0]) };
        let mut e: Vec<(EntryMeta, Vec<u8>)> = vec![];
        if let Some(m) = self.mimetype {
            e.push((em("mimetype", true), m.as_bytes().to_vec()));
        }
        e.push((em("content.xml", false), self.content.clone().into_bytes()));
        if let Some(s) = &self.styles {
            e.push((em("styles.xml", false), s.clone().into_bytes()));
        }
        if let Some(m) = &self.meta {
            e.push((em("meta.xml", false), m.clone().into_bytes()));
        }
        e.push((em("Pictures/a.png", true), b"\x89PNG fake image bytes".to_vec()));
        e.push((em("META-INF/manifest.xml", false), manifest.into_bytes()));
        for (n, d) in &self.extra {
            e.push((em(n, false), d.clone()));
        }
        write_zip(&e).expect("fixture zip")
    }
}

pub fn odt() -> Vec<u8> {
    Pkg::new(odt_content()).styles(odt_styles()).build()
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: target.map(str::to_string), ..Default::default() }
}

pub fn filter_for(ext: &str) -> Box<dyn Filter> {
    match ext {
        "odt" | "ott" => Box::new(Odt::default()),
        "ods" | "ots" => Box::new(Ods::default()),
        "odp" | "otp" => Box::new(Odp::default()),
        _ => Box::new(Odg::default()),
    }
}

pub fn p_index(u: &Unit) -> usize {
    u.context.extra["p_index"].parse().unwrap()
}

/// The unit text as Localization OS shows it: inline codes as tokens.
pub fn ltext(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn extract(ext: &str, data: &[u8], target: Option<&str>) -> Document {
    filter_for(ext).extract(data, &opts(target)).unwrap_or_else(|e| panic!("extract: {e}"))
}

/// (p_index, text) of every unit, like the Python suite's `units()`.
pub fn units(ext: &str, data: &[u8]) -> Vec<(usize, String)> {
    extract(ext, data, None).units.iter().map(|u| (p_index(u), ltext(u))).collect()
}

/// Sets the target of the unit with LocOS index `idx` from LocOS token text.
pub fn set(doc: &mut Document, idx: usize, text: &str) {
    let u = doc.units.iter_mut().find(|u| p_index(u) == idx).unwrap_or_else(|| panic!("no unit {idx}"));
    let (_, map) = to_locos_text(&u.source());
    let t = from_locos_text(text, &map);
    u.segments[0].target = t;
    u.segments[0].state = SegState::Translated;
}

/// LocOS `build(original, {idx: text})`: extract, translate the named units, merge.
pub fn build(ext: &str, data: &[u8], tr: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let f = filter_for(ext);
    let mut d = f.extract(data, &opts(target)).unwrap();
    for (i, t) in tr {
        set(&mut d, *i, t);
    }
    f.merge(&d).unwrap_or_else(|e| panic!("merge: {e}"))
}

pub fn entries(data: &[u8]) -> Vec<RawEntry> {
    bilingua_zip::read_zip("odt", data, &ZipLimits::default()).unwrap_or_else(|e| panic!("read: {e}"))
}

pub fn member(data: &[u8], name: &str) -> Vec<u8> {
    entries(data).into_iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no member {name}")).data
}

pub fn member_str(data: &[u8], name: &str) -> String {
    String::from_utf8(member(data, name)).unwrap()
}

/// The text of the first element that starts with `open` (for example `<style:default-style style:family="graphic">`)
/// up to its end tag.
pub fn element<'a>(xml: &'a str, open: &str, close: &str) -> &'a str {
    let a = xml.find(open).unwrap_or_else(|| panic!("no {open} in {xml}"));
    let b = xml[a..].find(close).unwrap_or_else(|| panic!("no {close}")) + a + close.len();
    &xml[a..b]
}
