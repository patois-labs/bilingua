//! Translation packages: Trados `.sdlppx` / `.sdlrpx` and memoQ `.mqxlz`.
//!
//! All three are zip files. Opening reads every entry into memory under the limits in
//! [`ZipLimits`] (see `zipio.rs`), opens the bilingual files inside as documents (any type that implements
//! [`PackageDoc`]; `bilingua_xliff::Doc` does), and keeps all
//! other entries as they were. Saving a Trados return package or a memoQ file writes a new zip;
//! nothing is ever extracted to disk.
//!
//! What is verified and what is inferred:
//! * The Trados layout (project file `*.sdlproj` at the root, `<language>/<path>.sdlxliff` per
//!   language, `Reports/`) and the differences between a project package and a return package were
//!   read from the two real Okapi files `test-packages.sdlppx` and `test-packages.sdlrpx`
//!   (Apache-2.0, `fixtures/real/`). The return writer applies exactly those differences.
//! * Nobody has opened a package written here in Trados Studio. "Trados would accept it" is an
//!   inference from the real structure, not a test with the product.
//! * `.mqxlz` is handled as a zip of `.mqxliff` files with any other entries kept verbatim. No real
//!   `.mqxlz` is in the fixtures; the container is inferred from memoQ documentation.

use bilingua_xliff::Error;
pub use crate::zipio::{Entry, ZipLimits};
use crate::zipio::{read_zip, safe_name, write_zip};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;

/// A bilingual file inside a package, as the package needs it: open from bytes, count, tell whether it was
/// edited, write back. `bilingua_xliff::Doc` is one; an editor can wrap it in its own type.
pub trait PackageDoc: Sized {
    fn open(bytes: Vec<u8>) -> Result<Self, Error>;
    fn segment_count(&self) -> usize;
    fn is_dirty(&self) -> bool;
    fn to_bytes(&self) -> Vec<u8>;
    /// Target language from the file header, when it has one.
    fn target_language(&self) -> Option<String>;
}

impl PackageDoc for bilingua_xliff::Doc {
    fn open(bytes: Vec<u8>) -> Result<Self, Error> {
        bilingua_xliff::Doc::from_bytes(bytes)
    }
    fn segment_count(&self) -> usize {
        self.len()
    }
    fn is_dirty(&self) -> bool {
        bilingua_xliff::Doc::is_dirty(self)
    }
    fn to_bytes(&self) -> Vec<u8> {
        bilingua_xliff::Doc::to_bytes(self)
    }
    fn target_language(&self) -> Option<String> {
        Some(self.target_lang().to_string()).filter(|s| !s.trim().is_empty())
    }
}

/// What a file inside a package is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileRole {
    /// The `.sdlproj` project description.
    Project,
    /// A bilingual file in the source language (reference copy; not edited).
    Source,
    /// A bilingual file in a target language (the work).
    Target,
    /// An analysis report or similar.
    Report,
    /// Anything else (kept verbatim).
    Other,
}

/// One entry of a package, for listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageFile {
    /// Path inside the zip, forward slashes.
    pub path: String,
    pub role: FileRole,
    /// Language code from the project file (Trados only).
    pub language: Option<String>,
    /// Uncompressed size in bytes.
    pub size: usize,
    /// Segment count for files opened as documents.
    pub segments: Option<usize>,
}

/// Project package (to the translator) or return package (back to the project manager).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SdlPackageKind {
    Project,
    Return,
}

/// Time and identity written into a return package.
#[derive(Debug, Clone)]
pub struct ReturnStamp {
    /// Seconds since 1970-01-01 UTC.
    pub unix_secs: u64,
    /// Sub-second part in nanoseconds.
    pub nanos: u32,
    /// `PackageGuid` of the new package (lower-case hyphenated GUID).
    pub package_guid: String,
}

impl ReturnStamp {
    /// Now, with a fresh random GUID.
    pub fn now() -> ReturnStamp {
        let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        ReturnStamp { unix_secs: d.as_secs(), nanos: d.subsec_nanos(), package_guid: random_guid(d.as_nanos() as u64) }
    }

    /// A fixed time and GUID (for repeatable output and tests).
    pub fn at(unix_secs: u64, package_guid: impl Into<String>) -> ReturnStamp {
        ReturnStamp { unix_secs, nanos: 0, package_guid: package_guid.into() }
    }
}

fn random_guid(seed: u64) -> String {
    let s = std::collections::hash_map::RandomState::new();
    let mut a = s.build_hasher();
    a.write_u64(seed);
    let mut b = s.build_hasher();
    b.write_u64(!seed);
    let (x, y) = (a.finish(), b.finish());
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&x.to_be_bytes());
    bytes[8..].copy_from_slice(&y.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", h[0..4].concat(), h[4..6].concat(), h[6..8].concat(), h[8..10].concat(), h[10..16].concat())
}

/// (year, month, day, hour, minute, second) in UTC.
fn civil(unix_secs: u64) -> (u16, u8, u8, u8, u8, u8) {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u16, m as u8, d as u8, (rem / 3600) as u8, (rem % 3600 / 60) as u8, (rem % 60) as u8)
}

/// `2021-09-03T06:39:19.5493817Z`, the form Trados writes.
fn iso(stamp: &ReturnStamp) -> String {
    let (y, mo, d, h, mi, s) = civil(stamp.unix_secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{:07}Z", stamp.nanos / 100)
}

// ---------------------------------------------------------------------------------------------
// Trados
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct ProjectInfo {
    name: String,
    package_type: String,
    source: String,
    targets: Vec<String>,
    /// (language, normalised physical path) for every file version listed in the project.
    files: Vec<(String, String)>,
    reports: Vec<String>,
}

fn attr(e: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name.as_bytes()).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

fn parse_project(xml: &[u8]) -> Result<ProjectInfo, Error> {
    let mut rd = Reader::from_reader(xml);
    let mut info = ProjectInfo::default();
    let mut lang_file: Option<String> = None;
    let mut in_source_lang = false;
    let mut saw_root = false;
    loop {
        let ev = rd.read_event().map_err(|e| Error::Package(format!("the .sdlproj is not valid XML: {e}")))?;
        match ev {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                match e.name().as_ref() {
                    b"PackageProject" => {
                        saw_root = true;
                        info.package_type = attr(&e, "PackageType").unwrap_or_default();
                    }
                    b"GeneralProjectInfo" => info.name = attr(&e, "Name").unwrap_or_default(),
                    b"SourceLanguageCode" => in_source_lang = true,
                    b"LanguageDirection" => {
                        if let Some(t) = attr(&e, "TargetLanguageCode") {
                            if !info.targets.contains(&t) {
                                info.targets.push(t);
                            }
                        }
                        if info.source.is_empty() {
                            info.source = attr(&e, "SourceLanguageCode").unwrap_or_default();
                        }
                    }
                    b"LanguageFile" => lang_file = attr(&e, "LanguageCode"),
                    b"FileVersion" => {
                        if let (Some(l), Some(p)) = (lang_file.clone(), attr(&e, "PhysicalPath")) {
                            info.files.push((l, p.replace('\\', "/")));
                        }
                    }
                    b"Report" => {
                        if let Some(p) = attr(&e, "PhysicalPath") {
                            info.reports.push(p.replace('\\', "/"));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_source_lang => {
                if let Ok(s) = t.decode() {
                    if !s.trim().is_empty() {
                        info.source = s.trim().to_owned();
                    }
                }
                in_source_lang = false;
            }
            Event::End(e) if e.name().as_ref() == b"LanguageFile" => lang_file = None,
            _ => {}
        }
    }
    if !saw_root {
        return Err(Error::Package("the .sdlproj has no PackageProject element".into()));
    }
    Ok(info)
}

/// An open Trados package (`.sdlppx` project package or `.sdlrpx` return package).
pub struct SdlPackage<D: PackageDoc> {
    entries: Vec<Entry>,
    project: usize,
    info: ProjectInfo,
    /// Entry index -> opened bilingual file (target languages only).
    docs: HashMap<usize, D>,
}

impl<D: PackageDoc> SdlPackage<D> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_bytes(&std::fs::read(path)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_bytes_with_limits(bytes, &ZipLimits::default())
    }

    pub fn from_bytes_with_limits(bytes: &[u8], limits: &ZipLimits) -> Result<Self, Error> {
        let entries = read_zip(bytes, limits)?;
        let projects: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.name.contains('/') && e.name.to_ascii_lowercase().ends_with(".sdlproj"))
            .map(|(i, _)| i)
            .collect();
        let project = match projects.as_slice() {
            [one] => *one,
            [] => return Err(Error::Package("no .sdlproj at the root: not a Trados package".into())),
            _ => return Err(Error::Package("more than one .sdlproj at the root".into())),
        };
        let data = &entries[project].data;
        let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(data);
        let info = parse_project(data)?;
        let mut docs = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if role_of(&info, e) == FileRole::Target && e.name.to_ascii_lowercase().ends_with(".sdlxliff") {
                let doc = D::open(e.data.clone()).map_err(|err| Error::Package(format!("{}: {err}", e.name)))?;
                docs.insert(i, doc);
            }
        }
        Ok(SdlPackage { entries, project, info, docs })
    }

    /// Every file of the package as read (no folders), in order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The package as a zip again: edited bilingual files replaced, every other entry as it was, same order.
    /// (A return package for Trados is built by [`SdlPackage::to_return_package`].)
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let out: Vec<Entry> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut e2 = e.clone();
                if let Some(d) = self.docs.get(&i) {
                    if d.is_dirty() {
                        e2.data = d.to_bytes();
                    }
                }
                e2
            })
            .collect();
        write_zip(&out, (1980, 1, 1, 0, 0, 0))
    }

    pub fn kind(&self) -> SdlPackageKind {
        if self.info.package_type == "ReturnPackage" {
            SdlPackageKind::Return
        } else {
            SdlPackageKind::Project
        }
    }

    pub fn project_name(&self) -> &str {
        &self.info.name
    }

    pub fn source_language(&self) -> &str {
        &self.info.source
    }

    pub fn target_languages(&self) -> &[String] {
        &self.info.targets
    }

    /// Every file of the package with its role.
    pub fn files(&self) -> Vec<PackageFile> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, e)| PackageFile {
                path: e.name.clone(),
                role: if i == self.project { FileRole::Project } else { role_of(&self.info, e) },
                language: language_of(&self.info, e),
                size: e.data.len(),
                segments: self.docs.get(&i).map(D::segment_count),
            })
            .collect()
    }

    /// Paths of the editable bilingual files (target languages), in package order.
    pub fn document_paths(&self) -> Vec<String> {
        let mut idx: Vec<usize> = self.docs.keys().copied().collect();
        idx.sort_unstable();
        idx.into_iter().map(|i| self.entries[i].name.clone()).collect()
    }

    pub fn doc(&self, path: &str) -> Option<&D> {
        self.entries.iter().position(|e| e.name == path).and_then(|i| self.docs.get(&i))
    }

    pub fn doc_mut(&mut self, path: &str) -> Option<&mut D> {
        let i = self.entries.iter().position(|e| e.name == path)?;
        self.docs.get_mut(&i)
    }

    pub fn is_dirty(&self) -> bool {
        self.docs.values().any(D::is_dirty)
    }

    /// Build the return package (`.sdlrpx`) with the current state of every target file.
    ///
    /// Mirrors the differences between the real `test-packages.sdlppx` and `test-packages.sdlrpx`:
    /// the project file name gets the return time; `PackageType` becomes `ReturnPackage` with a new
    /// `PackageGuid` and time; the manual task becomes `Completed` (100 %, `CompletedAt`) with its task
    /// files `Completed="true"`; `FileVersion` size and timestamp follow files that changed; the
    /// automatic-task analysis, its report files and the termbase configuration are left out. Source
    /// and target files are all included, unchanged ones byte for byte.
    ///
    /// Not recomputed: word and segment statistics in the project file (Trados recomputes them when
    /// it imports the return package; unverified).
    pub fn to_return_package(&self, stamp: &ReturnStamp) -> Result<Vec<u8>, Error> {
        let now = iso(stamp);
        let (y, mo, d, h, mi, s) = civil(stamp.unix_secs);
        // Which files changed, by normalised path -> new size.
        let mut changed: HashMap<String, usize> = HashMap::new();
        let mut files: Vec<Entry> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if i == self.project || is_report(&self.info, e) {
                continue;
            }
            let mut e2 = e.clone();
            if let Some(doc) = self.docs.get(&i) {
                if doc.is_dirty() {
                    e2.data = doc.to_bytes();
                    e2.modified = Some((y, mo, d, h, mi, s));
                    changed.insert(e.name.clone(), e2.data.len());
                }
            }
            files.push(e2);
        }
        let raw = &self.entries[self.project].data;
        let (bom, body) = match raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
            Some(rest) => (&raw[..3], rest),
            None => (&raw[..0], &raw[..]),
        };
        let xml = std::str::from_utf8(body).map_err(|_| Error::Package("the .sdlproj is not UTF-8".into()))?;
        let xml = return_project_xml(xml, &now, &stamp.package_guid, &changed);
        let safe_proj: String =
            self.info.name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
        let proj_name = format!("{safe_proj}-{y}{mo}{d}-{h}h{mi}m{s}s.sdlproj");
        let mut out = vec![Entry {
            name: safe_name(&proj_name)?,
            data: [bom, xml.as_bytes()].concat(),
            modified: Some((y, mo, d, h, mi, s)),
        }];
        out.extend(files);
        write_zip(&out, (y, mo, d, h, mi, s))
    }

    /// `to_return_package` with the current time, written to `path`.
    pub fn write_return_package(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let bytes = self.to_return_package(&ReturnStamp::now())?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

fn language_of(info: &ProjectInfo, e: &Entry) -> Option<String> {
    info.files.iter().find(|(_, p)| *p == e.name).map(|(l, _)| l.clone())
}

fn is_report(info: &ProjectInfo, e: &Entry) -> bool {
    e.name.starts_with("Reports/") || info.reports.contains(&e.name)
}

fn role_of(info: &ProjectInfo, e: &Entry) -> FileRole {
    if !e.name.contains('/') && e.name.to_ascii_lowercase().ends_with(".sdlproj") {
        return FileRole::Project;
    }
    if is_report(info, e) {
        return FileRole::Report;
    }
    match language_of(info, e) {
        Some(l) if l == info.source => FileRole::Source,
        Some(_) => FileRole::Target,
        None => FileRole::Other,
    }
}

/// End of the tag that starts at `s` (index just after `>`), quote aware.
fn tag_end(xml: &str, s: usize) -> usize {
    let b = xml.as_bytes();
    let mut q = 0u8;
    let mut i = s;
    while i < b.len() {
        match (q, b[i]) {
            (0, b'"' | b'\'') => q = b[i],
            (c, d) if c != 0 && c == d => q = 0,
            (0, b'>') => return i + 1,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

/// Rebuild `xml`, passing every start tag named `name` through `f`.
fn rewrite_tags(xml: &str, name: &str, mut f: impl FnMut(&str) -> String) -> String {
    let pat = format!("<{name}");
    let mut out = String::with_capacity(xml.len());
    let mut i = 0;
    while let Some(p) = xml[i..].find(&pat) {
        let s = i + p;
        let after = xml.as_bytes().get(s + pat.len()).copied();
        if !matches!(after, Some(b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/')) {
            out.push_str(&xml[i..s + pat.len()]);
            i = s + pat.len();
            continue;
        }
        let e = tag_end(xml, s);
        out.push_str(&xml[i..s]);
        out.push_str(&f(&xml[s..e]));
        i = e;
    }
    out.push_str(&xml[i..]);
    out
}

/// Value of attribute `name` in a start tag.
fn get_attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!(" {name}=\"");
    let s = tag.find(&pat)? + pat.len();
    let e = tag[s..].find('"')? + s;
    Some(tag[s..e].to_owned())
}

/// Set (or add) attribute `name` in a start tag. `value` must already be XML-safe.
fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let pat = format!(" {name}=\"");
    if let Some(p) = tag.find(&pat) {
        let s = p + pat.len();
        if let Some(e) = tag[s..].find('"') {
            return format!("{}{}{}", &tag[..s], value, &tag[s + e..]);
        }
    }
    let end = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
    let keep = tag[..end].trim_end().len();
    format!("{} {name}=\"{value}\"{}", &tag[..keep], &tag[keep..])
}

/// Drop `<name ...>...</name>` blocks together with the whitespace before them.
fn remove_blocks(xml: &str, name: &str) -> String {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut out = String::with_capacity(xml.len());
    let mut i = 0;
    while let Some(p) = xml[i..].find(&open) {
        let s = i + p;
        let after = xml.as_bytes().get(s + open.len()).copied();
        if !matches!(after, Some(b' ' | b'\t' | b'\r' | b'\n' | b'>')) {
            out.push_str(&xml[i..s + open.len()]);
            i = s + open.len();
            continue;
        }
        let Some(c) = xml[s..].find(&close) else { break };
        let end = s + c + close.len();
        out.push_str(xml[i..s].trim_end_matches([' ', '\t']));
        i = end;
        // Swallow the line break after the closing tag too, so no blank line is left.
        let rest = &xml[i..];
        if rest.starts_with("\r\n") {
            i += 2;
        } else if rest.starts_with('\n') {
            i += 1;
        }
    }
    out.push_str(&xml[i..]);
    out
}

fn return_project_xml(xml: &str, now: &str, package_guid: &str, changed: &HashMap<String, usize>) -> String {
    let xml = rewrite_tags(xml, "PackageProject", |t| {
        let t = set_attr(t, "PackageType", "ReturnPackage");
        let t = set_attr(&t, "PackageGuid", package_guid);
        set_attr(&t, "PackageCreatedAt", now)
    });
    let xml = remove_blocks(&xml, "AutomaticTask");
    let xml = remove_blocks(&xml, "TermbaseConfiguration");
    let xml = rewrite_tags(&xml, "FileVersion", |t| {
        let path = get_attr(t, "PhysicalPath").unwrap_or_default().replace('\\', "/");
        match changed.get(&path) {
            Some(size) => set_attr(&set_attr(t, "Size", &size.to_string()), "FileTimeStamp", now),
            None => t.to_owned(),
        }
    });
    let xml = rewrite_tags(&xml, "ManualTask", |t| {
        // Real order: Status, CompletedAt, PercentComplete.
        let t = remove_attr(t, "CompletedAt");
        let t = set_attr(&t, "Status", &format!("Completed\" CompletedAt=\"{now}"));
        set_attr(&t, "PercentComplete", "100")
    });
    rewrite_tags(&xml, "TaskFile", |t| set_attr(t, "Completed", "true"))
}

fn remove_attr(tag: &str, name: &str) -> String {
    let pat = format!(" {name}=\"");
    if let Some(p) = tag.find(&pat) {
        let s = p + pat.len();
        if let Some(e) = tag[s..].find('"') {
            return format!("{}{}", &tag[..p], &tag[s + e + 1..]);
        }
    }
    tag.to_owned()
}

// ---------------------------------------------------------------------------------------------
// memoQ
// ---------------------------------------------------------------------------------------------

/// An open memoQ package (`.mqxlz`: a zip holding one or more `.mqxliff` files).
pub struct MqPackage<D: PackageDoc> {
    entries: Vec<Entry>,
    docs: HashMap<usize, D>,
}

impl<D: PackageDoc> MqPackage<D> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_bytes(&std::fs::read(path)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_bytes_with_limits(bytes, &ZipLimits::default())
    }

    pub fn from_bytes_with_limits(bytes: &[u8], limits: &ZipLimits) -> Result<Self, Error> {
        let entries = read_zip(bytes, limits)?;
        let mut docs = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if e.name.to_ascii_lowercase().ends_with(".mqxliff") {
                let doc = D::open(e.data.clone()).map_err(|err| Error::Package(format!("{}: {err}", e.name)))?;
                docs.insert(i, doc);
            }
        }
        if docs.is_empty() {
            return Err(Error::Package("no .mqxliff inside: not a memoQ package".into()));
        }
        Ok(MqPackage { entries, docs })
    }

    /// Every file of the package as read (no folders), in order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn files(&self) -> Vec<PackageFile> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, e)| PackageFile {
                path: e.name.clone(),
                role: if self.docs.contains_key(&i) { FileRole::Target } else { FileRole::Other },
                language: self.docs.get(&i).and_then(D::target_language),
                size: e.data.len(),
                segments: self.docs.get(&i).map(D::segment_count),
            })
            .collect()
    }

    pub fn document_paths(&self) -> Vec<String> {
        let mut idx: Vec<usize> = self.docs.keys().copied().collect();
        idx.sort_unstable();
        idx.into_iter().map(|i| self.entries[i].name.clone()).collect()
    }

    pub fn doc(&self, path: &str) -> Option<&D> {
        self.entries.iter().position(|e| e.name == path).and_then(|i| self.docs.get(&i))
    }

    pub fn doc_mut(&mut self, path: &str) -> Option<&mut D> {
        let i = self.entries.iter().position(|e| e.name == path)?;
        self.docs.get_mut(&i)
    }

    pub fn is_dirty(&self) -> bool {
        self.docs.values().any(D::is_dirty)
    }

    /// The package as a zip: edited `.mqxliff` files replaced, every other entry as it was, same order.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let out: Vec<Entry> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let mut e2 = e.clone();
                if let Some(d) = self.docs.get(&i) {
                    if d.is_dirty() {
                        e2.data = d.to_bytes();
                    }
                }
                e2
            })
            .collect();
        write_zip(&out, (1980, 1, 1, 0, 0, 0))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        std::fs::write(path, self.to_bytes()?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_630_651_159), (2021, 9, 3, 6, 39, 19));
        assert_eq!(civil(1_782_864_000), (2026, 7, 1, 0, 0, 0));
        assert_eq!(civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn attribute_helpers() {
        let t = "<A x=\"1\" y=\"2\">";
        assert_eq!(set_attr(t, "x", "9"), "<A x=\"9\" y=\"2\">");
        assert_eq!(set_attr(t, "z", "3"), "<A x=\"1\" y=\"2\" z=\"3\">");
        assert_eq!(set_attr("<A x=\"1\" />", "z", "3"), "<A x=\"1\" z=\"3\" />");
        assert_eq!(remove_attr(t, "x"), "<A y=\"2\">");
        assert!(random_guid(1).len() == 36);
    }
}
