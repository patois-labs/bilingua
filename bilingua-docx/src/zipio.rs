//! Zip reading and writing for DOCX: the shared, guarded reader and writer in `bilingua-zip`, with errors that
//! name this format.

use bilingua_model::Error;
pub use bilingua_zip::{raw_body, write_zip_parts, Body, EntryMeta, Packed, RawEntry, ZipLimits};

pub fn read_zip(bytes: &[u8], limits: &ZipLimits) -> Result<Vec<RawEntry>, Error> {
    bilingua_zip::read_zip("docx", bytes, limits)
}
