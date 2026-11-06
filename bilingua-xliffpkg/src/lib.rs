//! Bilingua XLIFF packages: WorldServer / Idiom `.xlz` and `.wsxz`, and (for parity with Localization OS, which
//! reads them with the same code) memoQ `.mqxlz` through [`mqxlz`].
//!
//! A package is a zip whose translatable content is one or more XLIFF 1.2 files (`.xlf`, `.xliff`, `.mqxliff`,
//! `.sdlxliff`, `.sdlxlf`, `.mxliff`). Every such member goes through `bilingua-xliff` (segments, targets, states,
//! locks, inline codes); every other member (skeleton, TMX, TBX, preview, manifest) is carried through exactly.
//!
//! * One document holds the segments of every XLIFF member. A segment id is `<path>|<segment id>` (`%` and `|` in
//!   the path written `%25`, `%7C`), so two members can never give one id. `Unit::context.location` is the path.
//! * Existing translations load as targets (a finished segment arrives with its target and state).
//! * `merge` copies the original package and replaces only the XLIFF members that changed, with the member's
//!   compression, date and name, the member order and the archive comment kept. Nothing translated: the original
//!   bytes come back. When at least one member changed and the document has a `target_lang`, a `<file>` with no
//!   `target-language` gets one (a file's own language is never changed). `merge_with` honours
//!   `stamp_target_always` (stamp even when nothing was translated) and `output_encoding` (a rewritten UTF-16
//!   member goes out as UTF-8).
//! * Safety: the central directory is scanned for duplicate names, the zip is read under the `Limits` of
//!   `bilingua-model` (bombs, forged sizes, encrypted entries, links, path escapes), each XLIFF member passes the XML
//!   guard (no entity declarations), at most 200 XLIFF members of at most 64 MB each.
//! * XLIFF 2 members and members in a code page other than UTF-8 or UTF-16 are refused, as Localization OS does.
//! * Under the Localization OS profile (Track D group B) the members go through `bilingua_xliff::locos` (LocOS's CAT
//!   XLIFF engine): LocOS's units and its package numbering (`locos.p_index` = the member's offset plus the unit's
//!   slot in the member's walk, protected slots counted), and LocOS's splice when writing. The document then keeps
//!   the original package as a `Skeleton::Binary`.

/// Version of the skeleton layout this crate writes (bumped when an old skeleton can no longer be merged).
pub const SKELETON_SCHEMA: u32 = 1;

mod member;
mod scan;

use bilingua_model::*;
use bilingua_zip::{RawEntry, ZipLimits};
use member::{decode, encode, html_lang, open, perr, stamp_language};
use std::collections::HashMap;
use std::io::{Cursor, Write};

/// The skeleton holds the original package under this name (it contains a NUL, which no real zip entry name can).
const ORIGINAL: &str = "\0bilingua-xliffpkg-original";
const XLIFF_EXTS: [&str; 6] = [".mqxliff", ".sdlxliff", ".sdlxlf", ".mxliff", ".xlf", ".xliff"];
/// Most XLIFF members in one package (Localization OS `MAX_XLIFF_MEMBERS`).
pub const MAX_XLIFF_MEMBERS: usize = 200;
/// Largest XLIFF member (Localization OS `MAX_MEMBER_BYTES`).
pub const MAX_MEMBER_BYTES: u64 = 64 * 1024 * 1024;

/// The package filter. `xlz` claims `.xlz` and `.wsxz`.
pub struct XliffPackage {
    id: &'static str,
    exts: &'static [&'static str],
}

/// The filters this crate adds to the facade: `xlz` (`xlz`, `wsxz`).
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(XliffPackage { id: "xlz", exts: &["xlz", "wsxz"] })]
}

/// The memoQ `.mqxlz` filter as Localization OS builds it, for comparison with `bilingua-sdl`'s `mqxlz` (which the
/// facade keeps). Not in [`filters`].
pub fn mqxlz() -> Box<dyn Filter> {
    Box::new(XliffPackage { id: "mqxlz", exts: &["mqxlz"] })
}

/// `<path>|`, the start of the ids of one member's segments.
fn id_prefix(path: &str) -> String {
    format!("{}|", path.replace('%', "%25").replace('|', "%7C"))
}

struct Opened {
    entries: Vec<RawEntry>,
    /// Indexes into `entries` of the XLIFF members, in package order.
    members: Vec<usize>,
}

fn is_xliff(e: &RawEntry) -> bool {
    let n = e.name.to_ascii_lowercase();
    !e.meta.dir && XLIFF_EXTS.iter().any(|x| n.ends_with(x))
}

fn limit(kind: LimitKind, msg: String) -> Error {
    LimitError::new(kind, msg).into()
}

fn open_package(bytes: &[u8], l: &Limits) -> Result<Opened, Error> {
    if !bytes.starts_with(b"PK") {
        return Err(perr("This XLIFF package is not a zip archive. Export it again from the CAT tool."));
    }
    scan::check_member_names(bytes)?;
    let entries = bilingua_zip::read_zip("xlz", bytes, &ZipLimits::from(l))?;
    let members: Vec<usize> = entries.iter().enumerate().filter(|(_, e)| is_xliff(e)).map(|(i, _)| i).collect();
    if members.is_empty() {
        return Err(perr("This XLIFF package contains no XLIFF file (.mqxliff, .xlf, .xliff or .sdlxliff) to translate."));
    }
    if members.len() > MAX_XLIFF_MEMBERS {
        return Err(limit(
            LimitKind::ZipEntries,
            format!("This XLIFF package has {} XLIFF files; the limit is {MAX_XLIFF_MEMBERS}. Ask the sender to split the package.", members.len()),
        ));
    }
    for &i in &members {
        let e = &entries[i];
        if e.data.len() as u64 > MAX_MEMBER_BYTES {
            return Err(limit(
                LimitKind::ZipBytes,
                format!("The XLIFF file {:?} in this package is larger than {} MB.", e.name, MAX_MEMBER_BYTES / (1024 * 1024)),
            ));
        }
    }
    Ok(Opened { entries, members })
}

impl XliffPackage {
    /// The package as Localization OS reads it (`xliff_package.parse_package`): every member through its CAT XLIFF
    /// engine, a unit's `locos.p_index` its member's offset plus its slot in the member's walk.
    fn extract_locos(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let l = opts.limits();
        guard::check_input(bytes, &l)?;
        let opened = open_package(bytes, &l)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut offset = 0i64;
        for &i in &opened.members {
            let e = &opened.entries[i];
            guard::check_xml(&e.data, &l)?;
            let (found, slots) = bilingua_xliff::locos::cat_units(&e.data, self.id, None)
                .map_err(|err| perr(format!("The XLIFF file {:?} in this package could not be read: {err}", e.name)))?;
            for u in found {
                let p = offset + u.pos as i64;
                units.push(bilingua_xliff::locos::document_unit(u, self.id, Some((&e.name, p))));
            }
            guard::check_unit_count(units.len(), &l)?;
            offset += slots as i64;
        }
        let skeleton = Skeleton::binary(bytes.to_vec(), parser_info!());
        let doc = Document::new(self.id.to_string(), units, skeleton).with_target_lang(opts.target_lang.clone()).with_granularity(Granularity::Paragraph);
        guard::finish(doc, vec![], &l)
    }

    /// `xliff_package.build_package`: each member with something to write (or a target language to stamp) through
    /// the CAT XLIFF splice; the original package when nothing changed.
    fn merge_locos(&self, document: &Document, original: &[u8], opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        let l = Limits::default();
        let opened = open_package(original, &l)?;
        let any = document.units.iter().any(|u| u.segments.iter().any(|s| s.has_target()));
        let tag = opts.stamp_lang(document, any).and_then(profile::lang_tag);
        let mut replaced: HashMap<usize, Vec<u8>> = HashMap::new();
        for &i in &opened.members {
            let e = &opened.entries[i];
            guard::check_xml(&e.data, &l)?;
            let texts = bilingua_xliff::locos::written_texts(document.units.iter().filter(|u| u.context.location.as_deref() == Some(e.name.as_str())));
            if texts.is_empty() && tag.is_none() {
                continue;
            }
            let new = bilingua_xliff::locos::build_cat(&e.data, &texts, tag.as_deref())?;
            if new != e.data {
                replaced.insert(i, new);
            }
        }
        if replaced.is_empty() {
            return Ok(original.to_vec());
        }
        rebuild(original, &opened.entries, &replaced)
    }

    fn extract_doc(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let l = opts.limits();
        guard::check_input(bytes, &l)?;
        let opened = open_package(bytes, &l)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut source_lang = None;
        let mut target_lang = None;
        for &i in &opened.members {
            let e = &opened.entries[i];
            guard::check_xml(&e.data, &l)?;
            let (utf8, _) = decode(&e.name, &e.data)?;
            let doc = open(&e.name, utf8)?;
            let one = bilingua_xliff::to_document(&doc, self.id, opts)?;
            source_lang = source_lang.or(one.source_lang.clone());
            target_lang = target_lang.or(one.target_lang.clone());
            let prefix = id_prefix(&e.name);
            for mut u in one.units {
                u.id = format!("{prefix}{}", u.id);
                for s in &mut u.segments {
                    s.id = format!("{prefix}{}", s.id);
                }
                u.context.location = Some(e.name.clone());
                units.push(u);
            }
            guard::check_unit_count(units.len(), &l)?;
        }
        let skeleton = Skeleton::zip(vec![ZipEntry::raw(ORIGINAL, bytes.to_vec())], parser_info!());
        let doc = Document::new(self.id.to_string(), units, skeleton).with_source_lang(source_lang).with_target_lang(target_lang).with_granularity(Granularity::Paragraph);
        guard::finish(doc, vec![], &l)
    }

    fn merge_doc(&self, document: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_parser(document, &parser_info!())?;
        if let Skeleton::Binary { data, .. } = &document.skeleton {
            return self.merge_locos(document, data, opts);
        }
        let Skeleton::Zip { entries: skel, .. } = &document.skeleton else {
            return Err(Error::Write(format!("{} needs a zip skeleton", self.id)));
        };
        let original = skel
            .iter()
            .find(|e| e.name == ORIGINAL)
            .and_then(|e| if let ZipData::Raw(b) = &e.data { Some(b) } else { None })
            .ok_or_else(|| Error::Write(format!("the {} skeleton lost the original package", self.id)))?;
        let l = Limits::default();
        let opened = open_package(original, &l)?;
        struct Work {
            idx: usize,
            doc: bilingua_xliff::Doc,
            enc: member::Enc,
            input: Vec<u8>,
        }
        let mut work: Vec<Work> = Vec::new();
        for &i in &opened.members {
            let e = &opened.entries[i];
            guard::check_xml(&e.data, &l)?;
            let (utf8, enc) = decode(&e.name, &e.data)?;
            let mut doc = open(&e.name, utf8.clone())?;
            let units = document.units.iter().filter(|u| u.context.location.as_deref().is_none_or(|p| p == e.name));
            bilingua_xliff::apply_units_prefixed(&mut doc, units, &id_prefix(&e.name));
            work.push(Work { idx: i, doc, enc, input: utf8 });
        }
        let changed = work.iter().any(|w| w.doc.is_dirty());
        let tag = document.target_lang.as_deref().and_then(html_lang);
        // nothing translated: the original bytes, unless the caller wants the language stamped anyway (LocOS)
        let stamp_anyway = opts.stamp_target_always && tag.is_some();
        if !changed && !stamp_anyway {
            return Ok(original.clone());
        }
        let mut replaced: HashMap<usize, Vec<u8>> = HashMap::new();
        for w in &work {
            let mut out = w.doc.to_bytes();
            if let Some(t) = &tag {
                let text = std::str::from_utf8(&out).map_err(|_| Error::Write("the merged XLIFF member is not valid UTF-8".into()))?;
                if let Some(stamped) = stamp_language(text, t) {
                    out = stamped.into_bytes();
                }
            }
            if out != w.input {
                // UTF-8 out (the reader's text already declares UTF-8; a UTF-8 BOM stays), or the member's own encoding
                let bytes = if opts.output_encoding == OutputEncoding::Utf8 { out } else { encode(&out, &w.enc)? };
                replaced.insert(w.idx, bytes);
            }
        }
        if replaced.is_empty() {
            return Ok(original.clone());
        }
        rebuild(original, &opened.entries, &replaced)
    }
}

/// The package again: untouched members are raw-copied (their compressed bytes, CRC, sizes, dates, attributes, extra
/// fields, comments and names as they were; the writer sets its own version fields and drops the data-descriptor
/// flag, since the local header then carries the sizes), replaced members are written with the same name, date,
/// compression method, attributes and member comment (not their extra fields, as LocOS), and the archive comment
/// is kept.
fn rebuild(original: &[u8], entries: &[RawEntry], replaced: &HashMap<usize, Vec<u8>>) -> Result<Vec<u8>, Error> {
    let werr = |what: &str, e: &dyn std::fmt::Display| Error::Write(format!("cannot write {what}: {e}"));
    let mut src = zip::ZipArchive::new(Cursor::new(original)).map_err(|e| werr("the package", &e))?;
    if src.len() != entries.len() {
        return Err(Error::Write("the package changed while it was rebuilt".into()));
    }
    let comment = src.comment().to_vec();
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (i, entry) in entries.iter().enumerate() {
        let f = src.by_index_raw(i).map_err(|e| werr("a member", &e))?;
        match replaced.get(&i) {
            None => w.raw_copy_file(f).map_err(|e| werr(&entry.name, &e))?,
            Some(data) => {
                let name = f.name().map_err(|e| werr(&entry.name, &e))?.into_owned();
                // name, date, method, attributes and member comment kept, as LocOS's `_copy_info`
                let mut opts = zip::write::FullFileOptions::default()
                    .compression_method(f.compression())
                    .external_attributes(f.external_attributes());
                if !f.comment().is_empty() {
                    opts = opts.with_file_comment(f.comment());
                }
                if let Some(t) = f.last_modified() {
                    opts = opts.last_modified_time(t);
                }
                drop(f);
                w.start_file(&name, opts).map_err(|e| werr(&name, &e))?;
                w.write_all(data).map_err(|e| werr(&name, &e))?;
            }
        }
    }
    if !comment.is_empty() {
        w.set_raw_comment(comment.into_boxed_slice()).map_err(|e| werr("the comment", &e))?;
    }
    Ok(w.finish().map_err(|e| werr("the package", &e))?.into_inner())
}

impl Filter for XliffPackage {
    fn id(&self) -> &'static str {
        self.id
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.exts
    }
    fn sniff(&self, _bytes: &[u8]) -> bool {
        // A zip is a zip: packages are recognised by extension only (as in Localization OS).
        false
    }
    fn bilingual(&self) -> bool {
        true
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        if opts.is_profile(profile::LOCOS) {
            return self.extract_locos(bytes, opts);
        }
        self.extract_doc(bytes, opts)
    }
    /// `stamp_target_always`: with a `target_lang`, every XLIFF member's `<file>` without a language is stamped even
    /// when nothing was translated (LocOS `build_localized`). `output_encoding: utf-8`: a rewritten UTF-16 member is
    /// written as UTF-8 with its declaration saying so, as `bilingua-xliff` does; members that are not rewritten stay
    /// raw-copied in their own encoding (the package itself is a zip, not text).
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.merge_doc(doc, opts)
    }
}
