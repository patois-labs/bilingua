//! Bilingua packages: Trados project and return packages (`.sdlppx`, `.sdlrpx`) and memoQ packages (`.mqxlz`).
//!
//! All three are zip files that hold bilingual XLIFF files. [`SdlPackage`] and [`MqPackage`] open them under
//! zip safety limits ([`ZipLimits`]), read the bilingual files inside, and write a package back with the edited
//! files replaced and every other entry as it was. [`SdlPackage::to_return_package`] builds the Trados return
//! package.
//!
//! The package types are generic over the open-file type ([`PackageDoc`]); `bilingua_xliff::Doc` is the one this
//! workspace uses. [`filters`] gives the Bilingua [`Filter`]s: one document holds the segments of every target
//! file, with segment ids `<path in the package>|<segment id>` (`%` and `|` in the path escaped as `%25`, `%7C`).

/// Version of the skeleton layout this crate writes (bumped when an old skeleton can no longer be merged).
pub const SKELETON_SCHEMA: u32 = 1;

mod package;
mod zipio;

pub use bilingua_xliff::Error;
pub use package::{FileRole, MqPackage, PackageDoc, PackageFile, ReturnStamp, SdlPackage, SdlPackageKind};
pub use zipio::{Entry, ZipLimits};

use bilingua_model::{Document, ExtractOptions, Filter, Skeleton, ZipData, ZipEntry};
use bilingua_xliff::Doc;

/// Which package family a filter reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Sdl,
    Mq,
}

/// The package filters (Trados and memoQ).
pub struct PackageFilter {
    id: &'static str,
    exts: &'static [&'static str],
    family: Family,
}

/// All package filters.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![
        Box::new(PackageFilter { id: "sdlppx", exts: &["sdlppx", "sdlrpx"], family: Family::Sdl }),
        Box::new(PackageFilter { id: "mqxlz", exts: &["mqxlz"], family: Family::Mq }),
    ]
}

fn perr(id: &str, e: Error) -> bilingua_model::Error {
    bilingua_model::Error::Parse { format: id.to_string(), msg: e.to_string() }
}

fn werr(e: Error) -> bilingua_model::Error {
    bilingua_model::Error::Write(e.to_string())
}

/// `<path>|`, the start of the ids of one package file's segments. A `%` or `|` in the path is written `%25` or
/// `%7C`, so the first `|` of an id always ends the path and two files can never give the same id.
fn id_prefix(path: &str) -> String {
    format!("{}|", path.replace('%', "%25").replace('|', "%7C"))
}

/// Writes the segments of one package file back into it: only the units extracted from that file (a `|` in a
/// path cannot make one file's ids match another's). A unit that lost its location is matched by id prefix alone.
fn apply_file(doc: &mut Doc, document: &Document, path: &str) {
    let units = document.units.iter().filter(|u| u.context.location.as_deref().is_none_or(|l| l == path));
    bilingua_xliff::apply_units_prefixed(doc, units, &id_prefix(path));
}

/// The bilingual files of a package as (path, bytes), plus the whole entry list.
struct Opened {
    entries: Vec<Entry>,
    docs: Vec<String>,
}

fn open_entries(family: Family, bytes: &[u8]) -> Result<Opened, Error> {
    Ok(match family {
        Family::Sdl => {
            let p = SdlPackage::<Doc>::from_bytes(bytes)?;
            Opened { entries: p.entries().to_vec(), docs: p.document_paths() }
        }
        Family::Mq => {
            let p = MqPackage::<Doc>::from_bytes(bytes)?;
            Opened { entries: p.entries().to_vec(), docs: p.document_paths() }
        }
    })
}

impl Filter for PackageFilter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn extensions(&self) -> &'static [&'static str] {
        self.exts
    }
    fn sniff(&self, _bytes: &[u8]) -> bool {
        // A zip is a zip: packages are recognised by extension only.
        false
    }
    fn bilingual(&self) -> bool {
        true
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, bilingua_model::Error> {
        let opened = open_entries(self.family, bytes).map_err(|e| perr(self.id, e))?;
        let mut units = Vec::new();
        let mut source_lang = None;
        let mut target_lang = None;
        for path in &opened.docs {
            let data = &opened.entries.iter().find(|e| &e.name == path).expect("listed above").data;
            let doc = Doc::from_bytes(data.clone()).map_err(|e| perr(self.id, e))?;
            let one = bilingua_xliff::to_document(&doc, self.id, opts)?;
            source_lang = source_lang.or(one.source_lang.clone());
            target_lang = target_lang.or(one.target_lang.clone());
            for mut u in one.units {
                let prefix = id_prefix(path);
                u.id = format!("{prefix}{}", u.id);
                for s in &mut u.segments {
                    s.id = format!("{prefix}{}", s.id);
                }
                u.context.location = Some(path.clone());
                units.push(u);
            }
        }
        let zentries = opened
            .entries
            .iter()
            .map(|e| ZipEntry { name: e.name.clone(), data: ZipData::Raw(e.data.clone()) })
            .collect();
        Ok(Document {
            format: self.id.to_string(),
            name: None,
            source_lang,
            target_lang,
            granularity: bilingua_model::Granularity::Paragraph,
            units,
            skeleton: Skeleton::zip(zentries, bilingua_model::parser_info!()),
        })
    }

    fn merge(&self, document: &Document) -> Result<Vec<u8>, bilingua_model::Error> {
        bilingua_model::check_parser(document, &bilingua_model::parser_info!())?;
        let Skeleton::Zip { entries, .. } = &document.skeleton else {
            return Err(bilingua_model::Error::Write(format!("{} needs a zip skeleton", self.id)));
        };
        let raw: Vec<Entry> = entries
            .iter()
            .filter_map(|e| match &e.data {
                ZipData::Raw(b) => Some(Entry { name: e.name.clone(), data: b.clone(), modified: None }),
                ZipData::Parts(_) => None,
            })
            .collect();
        let zipped = zipio::write_zip(&raw, (1980, 1, 1, 0, 0, 0)).map_err(werr)?;
        match self.family {
            Family::Sdl => {
                let mut p = SdlPackage::<Doc>::from_bytes_with_limits(&zipped, &ZipLimits::default()).map_err(werr)?;
                let paths = p.document_paths();
                for path in paths {
                    if let Some(doc) = p.doc_mut(&path) {
                        apply_file(doc, document, &path);
                    }
                }
                p.to_bytes().map_err(werr)
            }
            Family::Mq => {
                let mut p = MqPackage::<Doc>::from_bytes_with_limits(&zipped, &ZipLimits::default()).map_err(werr)?;
                let paths = p.document_paths();
                for path in paths {
                    if let Some(doc) = p.doc_mut(&path) {
                        apply_file(doc, document, &path);
                    }
                }
                p.to_bytes().map_err(werr)
            }
        }
    }
}
