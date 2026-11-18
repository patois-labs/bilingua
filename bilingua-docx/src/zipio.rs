//! Zip reading and writing for DOCX: the shared, guarded reader and writer in `bilingua-zip`, with errors that
//! name this format.

use bilingua_model::Error;
pub use bilingua_zip::{raw_body, write_zip_parts, Body, EntryMeta, Packed, RawEntry, ZipLimits};

/// Reads the package (as `bilingua_zip::read_zip`) with a watcher over each entry's bytes as they inflate.
pub fn read_zip_watched(bytes: &[u8], limits: &ZipLimits, watch: &mut dyn bilingua_zip::Watch) -> Result<Vec<RawEntry>, Error> {
    bilingua_zip::read_zip_watched("docx", bytes, limits, watch)
}
