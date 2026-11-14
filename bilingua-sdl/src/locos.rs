//! Trados packages under the Localization OS profile (Track D group B): a port of LocOS's `sdl_package.py` for a
//! package with one target SDLXLIFF (LocOS's `.sdlrpx` filter). The units are that file's, read by LocOS's CAT
//! XLIFF engine (`bilingua_xliff::locos`, keys as for a loose `.sdlxliff`); the merge writes LocOS's return package:
//! the `.sdlproj` with `PackageType="ProjectPackage"` switched to `ReturnPackage` and the rebuilt SDLXLIFF, in the
//! package's order, nothing else. A package with several target files is refused, as LocOS refuses it for one
//! document (it imports them as one document per file through its project path).
//!
//! The memoQ `.mqxlz` goes to `bilingua_xliffpkg`'s LocOS port, which reads it as LocOS does.

use crate::zipio::{read_zip, write_zip, Entry, ZipLimits};
use bilingua_model::{profile, Document, Error, ExtractOptions, Granularity, MergeOptions, Skeleton};
use quick_xml::events::Event;

fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "sdlppx".into(), msg: msg.into() }
}

fn folder(path: &str) -> &str {
    match path.split_once('/') {
        Some((f, _)) => f,
        None => "",
    }
}

/// What LocOS needs from the `.sdlproj`: its root element and language directions.
fn languages(xml: &[u8]) -> Result<(String, Vec<(String, String)>), Error> {
    let mut r = quick_xml::Reader::from_reader(xml);
    let mut root = None;
    let mut pairs = Vec::new();
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::DocType(_)) => return Err(perr("This Trados package's .sdlproj file could not be read: it declares a DOCTYPE.")),
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.local_name();
                let local = String::from_utf8_lossy(name.as_ref()).to_string();
                if root.is_none() {
                    root = Some(local.clone());
                }
                if local == "LanguageDirection" {
                    let get = |k: &str| {
                        e.attributes().flatten().find(|a| a.key.local_name().as_ref() == k.as_bytes()).and_then(|a| a.unescape_value().ok()).map(|v| v.trim().to_string()).unwrap_or_default()
                    };
                    let (s, t) = (get("SourceLanguageCode"), get("TargetLanguageCode"));
                    if !s.is_empty() && !t.is_empty() {
                        pairs.push((s, t));
                    }
                }
            }
            Ok(_) => {}
            Err(e) => return Err(perr(format!("This Trados package's .sdlproj file could not be read: {e}"))),
        }
        buf.clear();
    }
    Ok((root.unwrap_or_default(), pairs))
}

/// The `.sdlproj` index and the one target SDLXLIFF index (LocOS `inspect` and `_open_member`).
fn inspect(entries: &[Entry]) -> Result<(usize, usize), Error> {
    let projs: Vec<usize> = entries.iter().enumerate().filter(|(_, e)| !e.name.contains('/') && e.name.to_lowercase().ends_with(".sdlproj")).map(|(i, _)| i).collect();
    if projs.len() != 1 {
        return Err(perr(format!("This is not a Trados project package: it needs exactly one .sdlproj file at the top level (found {}).", projs.len())));
    }
    let p = projs[0];
    let (root, pairs) = languages(&entries[p].data)?;
    if root != "PackageProject" && root != "Project" {
        return Err(perr(format!("This Trados package's .sdlproj is not a Trados project file (root element {root:?}).")));
    }
    if pairs.is_empty() {
        return Err(perr("This Trados package's .sdlproj names no language direction (SourceLanguageCode / TargetLanguageCode)."));
    }
    let source = pairs[0].0.to_lowercase();
    if pairs.iter().any(|(s, _)| s.to_lowercase() != source) {
        return Err(perr("This Trados package has more than one source language; a project here has one."));
    }
    let targets: Vec<String> = pairs.iter().map(|(_, t)| t.to_lowercase()).collect();
    let found: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            let low = e.name.to_lowercase();
            let f = folder(&e.name).to_lowercase();
            *i != p && (low.ends_with(".sdlxliff") || low.ends_with(".sdlxlf")) && f != source && targets.contains(&f)
        })
        .map(|(i, _)| i)
        .collect();
    match found.len() {
        0 => Err(perr("This Trados package has no target-language SDLXLIFF files to translate.")),
        1 => Ok((p, found[0])),
        n => Err(perr(format!("This Trados package has {n} target files. Upload it to a project (one document per file) or create a project from it."))),
    }
}

pub(crate) fn extract(bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let l = opts.limits();
    let entries = read_zip(bytes, &ZipLimits::of(&l, true)).map_err(|e| crate::perr("sdlppx", e))?;
    let (_, member) = inspect(&entries)?;
    let e = &entries[member];
    bilingua_model::guard::check_xml(&e.data, &l)?;
    let (units, _) = bilingua_xliff::locos::cat_units(&e.data, "sdlxliff", Some("http://sdl.com/FileTypes/SdlXliff/1.0"))?;
    let units = units.into_iter().map(|u| bilingua_xliff::locos::document_unit(u, "sdlppx", None)).collect();
    let doc = Document::new("sdlppx", units, Skeleton::binary(bytes.to_vec(), bilingua_model::parser_info!()))
        .with_target_lang(opts.target_lang.clone())
        .with_granularity(Granularity::Paragraph);
    bilingua_model::guard::finish(doc, vec![], &l)
}

/// LocOS `flip_package_type`: `PackageType="ProjectPackage"` becomes `ReturnPackage` in the root's start tag.
fn flip_package_type(xml: &[u8]) -> Vec<u8> {
    let from = find(xml, b"PackageProject", 0).unwrap_or(0);
    let Some(end) = find(xml, b">", from) else { return xml.to_vec() };
    let head = &xml[..end + 1];
    let mut i = 0;
    while let Some(at) = find(head, b"PackageType", i) {
        i = at + 1;
        // `\b`: not inside a longer name
        if at > 0 && (head[at - 1].is_ascii_alphanumeric() || head[at - 1] == b'_') {
            continue;
        }
        let mut j = at + b"PackageType".len();
        while j < head.len() && head[j].is_ascii_whitespace() {
            j += 1;
        }
        if head.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while j < head.len() && head[j].is_ascii_whitespace() {
            j += 1;
        }
        let Some(&q) = head.get(j).filter(|q| **q == b'"' || **q == b'\'') else { continue };
        let v = j + 1;
        if head[v..].starts_with(b"ProjectPackage") && head.get(v + b"ProjectPackage".len()) == Some(&q) {
            return [&xml[..v], b"ReturnPackage", &xml[v + b"ProjectPackage".len()..]].concat();
        }
    }
    xml.to_vec()
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

pub(crate) fn merge(document: &Document, original: &[u8], opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let l = bilingua_model::Limits::default();
    let entries = read_zip(original, &ZipLimits::of(&l, true)).map_err(|e| crate::perr("sdlppx", e))?;
    let (p, member) = inspect(&entries)?;
    let texts = bilingua_xliff::locos::written_texts_escaped(document.units.iter());
    let tag = opts.stamp_lang(document, !texts.is_empty()).and_then(profile::lang_tag);
    let built = bilingua_xliff::locos::build_cat_escaped(&entries[member].data, &texts, tag.as_deref())?;
    let mut parts = vec![(p, flip_package_type(&entries[p].data)), (member, built)];
    parts.sort_by_key(|(i, _)| *i);
    let out: Vec<Entry> = parts.into_iter().map(|(i, data)| Entry { name: entries[i].name.clone(), data, modified: entries[i].modified }).collect();
    write_zip(&out, (1980, 1, 1, 0, 0, 0)).map_err(|e| Error::Write(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_only_the_root_package_type() {
        let x = br#"<?xml version="1.0"?><PackageProject  PackageType = 'ProjectPackage' Name="p"><X PackageType="ProjectPackage"/></PackageProject>"#;
        let out = flip_package_type(x);
        assert_eq!(out, br#"<?xml version="1.0"?><PackageProject  PackageType = 'ReturnPackage' Name="p"><X PackageType="ProjectPackage"/></PackageProject>"#.to_vec());
        assert_eq!(flip_package_type(b"<Project MyPackageType=\"ProjectPackage\"/>"), b"<Project MyPackageType=\"ProjectPackage\"/>".to_vec());
        assert_eq!(flip_package_type(b"<P PackageType=\"ProjectPackage'/>"), b"<P PackageType=\"ProjectPackage'/>".to_vec());
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let e: Vec<Entry> = entries.iter().map(|(n, d)| Entry { name: n.to_string(), data: d.to_vec(), modified: None }).collect();
        write_zip(&e, (1980, 1, 1, 0, 0, 0)).unwrap()
    }

    #[test]
    fn refuses_packages_locos_cannot_take_as_one_document() {
        let proj = b"<PackageProject PackageType=\"ProjectPackage\"><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/></PackageProject>";
        let sdl = b"<xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" version=\"1.2\"><file><body><trans-unit id=\"1\"><source>Hi</source></trans-unit></body></file></xliff>";
        let o = profile::locos().extract_options();
        for bad in [
            zip(&[("a.sdlproj", proj), ("b.sdlproj", proj), ("de-DE/a.sdlxliff", sdl)]),
            zip(&[("p.sdlproj", proj), ("de-DE/a.sdlxliff", sdl), ("de-DE/b.sdlxliff", sdl)]),
            zip(&[("p.sdlproj", proj), ("en-US/a.sdlxliff", sdl)]),
            zip(&[("p.sdlproj", b"<!DOCTYPE x><PackageProject/>"), ("de-DE/a.sdlxliff", sdl)]),
            zip(&[("p.sdlproj", b"<Other/>"), ("de-DE/a.sdlxliff", sdl)]),
            b"PK\x03\x04 not a zip".to_vec(),
        ] {
            assert!(extract(&bad, &o).is_err());
        }
        let good = zip(&[("p.sdlproj", proj), ("en-US/a.docx", b"x"), ("de-DE/a.sdlxliff", sdl)]);
        let mut doc = extract(&good, &o).unwrap();
        let seg = doc.units[0].segments[0].id.clone();
        doc.set_target(&seg, vec![bilingua_model::Inline::text("Hallo")]).unwrap();
        let out = merge(&doc, &good, &profile::locos().merge_options("sdlppx")).unwrap();
        let back = read_zip(&out, &ZipLimits::unlimited()).unwrap();
        assert_eq!(back.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["p.sdlproj", "de-DE/a.sdlxliff"]);
        assert!(String::from_utf8_lossy(&back[0].data).contains("ReturnPackage"));
        assert!(String::from_utf8_lossy(&back[1].data).contains("<target>Hallo</target>"));
    }

    /// Issue 814 item 1: token look-alike text in the member is written back as text, not read as a code.
    #[test]
    fn token_look_alikes_stay_text() {
        let proj = b"<PackageProject PackageType=\"ProjectPackage\"><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/></PackageProject>";
        let body = "Hi \u{27e6}g1\u{27e7}<g id=\"7\">b</g>\u{27e6}/g1\u{27e7} \u{e000}\u{e001}";
        let sdl = format!("<xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{body}</source></trans-unit></body></file></xliff>");
        let pkg = zip(&[("p.sdlproj", proj), ("de-DE/a.sdlxliff", sdl.as_bytes())]);
        let o = profile::locos().extract_options();
        let mut doc = extract(&pkg, &o).unwrap();
        let seg = doc.units[0].segments[0].clone();
        profile::locos_set_target(&mut doc, &seg.id, seg.source.clone()).unwrap();
        let out = merge(&doc, &pkg, &profile::locos().merge_options("sdlppx")).unwrap();
        let back = read_zip(&out, &ZipLimits::unlimited()).unwrap();
        let x = String::from_utf8_lossy(&back[1].data).to_string();
        assert!(x.contains(&format!("<target>{body}</target>")), "{x}");
    }
}
