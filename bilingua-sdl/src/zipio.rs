//! Safe zip reading and plain zip writing for the package formats.
//!
//! A package comes from a customer or a vendor, so it is untrusted input. Reading refuses:
//! junk that is not a zip, encrypted entries, symbolic links, entry names that could escape a
//! folder (`..`, absolute paths, drive letters, NUL and other control characters, Windows
//! alternate-stream colons), duplicate names (compared without case), too many entries, entries
//! or totals that are too big, and entries whose real size is far larger than their compressed
//! size (zip bombs). The size checks use the bytes actually produced by the decompressor, not
//! the sizes the zip header claims.
//!
//! Nothing here touches the disk besides the caller's own `read`/`write`: entries stay in memory
//! as `(name, bytes)`, so a bad name cannot write outside anywhere.

use bilingua_model::{LimitError, LimitKind};
use bilingua_xliff::Error;
use std::collections::HashSet;
use std::io::{Cursor, Read, Write};

/// Limits applied when reading a package: `bilingua_model::Limits` as this module uses them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ZipLimits {
    /// Most entries (folders included) a package may hold.
    pub max_entries: usize,
    /// Most bytes one entry may expand to.
    pub max_entry_bytes: u64,
    /// Most bytes all entries together may expand to.
    pub max_total_bytes: u64,
    /// Largest accepted `expanded size / compressed size` for an entry bigger than
    /// `ratio_floor_bytes`. XML compresses 5 to 30 times; 200 leaves room for repetitive files.
    pub max_ratio: u64,
    /// Entries at or under this size are not ratio-checked (tiny files can compress absurdly well).
    pub ratio_floor_bytes: u64,
    /// Refuse with a typed `Error::Limit` (kind and a message safe to show an uploader), as the `Filter`s do for
    /// `ExtractOptions::limits`. False (the default, for the open-package API the editor core uses): a plain
    /// `Error::Package` message.
    pub typed_errors: bool,
}

impl ZipLimits {
    /// The zip limits of `l` (Localization OS's numbers by default: 10 000 entries, 256 MB unpacked in total and per
    /// entry, ratio 200 over 1 MB). `typed`: refuse with `Error::Limit` (the `Filter`s) rather than a plain
    /// `Error::Package` message (the open-package API).
    pub(crate) fn of(l: &bilingua_model::Limits, typed: bool) -> ZipLimits {
        ZipLimits {
            max_entries: l.max_zip_entries,
            max_entry_bytes: l.max_zip_entry_bytes.min(l.max_zip_bytes),
            max_total_bytes: l.max_zip_bytes,
            max_ratio: l.max_zip_ratio,
            ratio_floor_bytes: l.zip_ratio_floor_bytes,
            typed_errors: typed,
        }
    }

    /// No cap: for re-reading a package this crate wrote itself from entries that were already accepted.
    pub(crate) fn unlimited() -> ZipLimits {
        ZipLimits { max_entries: usize::MAX, max_entry_bytes: u64::MAX - 1, max_total_bytes: u64::MAX, max_ratio: u64::MAX, ratio_floor_bytes: u64::MAX, typed_errors: false }
    }
}

/// A limit refusal: typed for the `Filter`s, a plain package message for the open-package API.
fn refuse(l: &ZipLimits, kind: LimitKind, typed: String, plain: String) -> Error {
    if l.typed_errors {
        Error::Limit(LimitError::new(kind, typed))
    } else {
        Error::Package(plain)
    }
}

/// One file of a package, in memory.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Forward-slash path inside the zip, checked by [`safe_name`].
    pub name: String,
    pub data: Vec<u8>,
    /// Modification time as stored: (year, month, day, hour, minute, second).
    pub modified: Option<(u16, u8, u8, u8, u8, u8)>,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Package(msg.into())
}

/// Check and normalise an entry name. Backslashes become `/`. Folder names (trailing `/`) keep it.
pub fn safe_name(raw: &str) -> Result<String, Error> {
    if raw.is_empty() {
        return Err(bad("an entry has an empty name"));
    }
    if raw.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(bad(format!("entry name {raw:?} holds a control character")));
    }
    let name = raw.replace('\\', "/");
    if name.starts_with('/') {
        return Err(bad(format!("entry name {raw:?} is an absolute path")));
    }
    if name.contains(':') {
        // Drive letters (C:), alternate data streams (file:stream) and URL-like names.
        return Err(bad(format!("entry name {raw:?} contains a colon")));
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        match part {
            ".." => return Err(bad(format!("entry name {raw:?} climbs out of the package (..)"))),
            "." | "" => {}
            p => {
                // Windows trims trailing dots and spaces, which can turn "x.." into "x" or ".. " into "..".
                if p.trim_end_matches(['.', ' ']).is_empty() {
                    return Err(bad(format!("entry name {raw:?} has a component made of dots or spaces")));
                }
                parts.push(p);
            }
        }
    }
    if parts.is_empty() {
        return Err(bad(format!("entry name {raw:?} names no file")));
    }
    let mut out = parts.join("/");
    if name.ends_with('/') {
        out.push('/');
    }
    Ok(out)
}

/// Read every file of a zip held in memory, applying `limits`. Folder entries are dropped.
pub(crate) fn read_zip(bytes: &[u8], limits: &ZipLimits) -> Result<Vec<Entry>, Error> {
    if bytes.len() < 22 || !(bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")) {
        return Err(bad("not a zip file (no PK signature)"));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(format!("unreadable zip: {e}")))?;
    // a name listed twice is dropped by the reader, whatever the layout: refused by the shared check
    if let Err(bilingua_model::Error::Limit(e)) = bilingua_zip::refuse_duplicate_names(&zip) {
        return Err(refuse(limits, e.kind, e.msg, "the package lists the same entry twice".into()));
    }
    if zip.len() > limits.max_entries {
        return Err(refuse(
            limits,
            LimitKind::ZipEntries,
            format!("This package has {} entries, more than the {} allowed. It was not processed.", zip.len(), limits.max_entries),
            format!("{} entries, the limit is {}", zip.len(), limits.max_entries),
        ));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        // Metadata first, without decrypting or decompressing anything.
        let (name, is_dir, claimed, packed, modified) = {
            let f = zip.by_index_raw(i).map_err(|e| bad(format!("unreadable entry {i}: {e}")))?;
            if f.encrypted() {
                return Err(bad(format!("entry {:?} is encrypted", f.name().map(|n| n.into_owned()).unwrap_or_default())));
            }
            if f.is_symlink() {
                return Err(bad("the package holds a symbolic link"));
            }
            let raw = f.name().map_err(|e| bad(format!("entry {i} has an unreadable name: {e}")))?.into_owned();
            let name = safe_name(&raw)?;
            let modified = f.last_modified().map(|d| (d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second()));
            (name, f.is_dir() || raw.ends_with('/') || raw.ends_with('\\'), f.size(), f.compressed_size(), modified)
        };
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(bad(format!("entry {name:?} appears twice")));
        }
        if is_dir {
            continue;
        }
        if claimed > limits.max_entry_bytes {
            return Err(refuse(
                limits,
                LimitKind::ZipBytes,
                format!("A file in this package is larger than {} bytes unpacked. It was not processed.", limits.max_entry_bytes),
                format!("entry {name:?} claims {claimed} bytes, the limit is {}", limits.max_entry_bytes),
            ));
        }
        if claimed > limits.ratio_floor_bytes && claimed / packed.max(1) > limits.max_ratio {
            return Err(refuse(
                limits,
                LimitKind::ZipRatio,
                format!("A file in this package expands more than {}x. It was not processed.", limits.max_ratio),
                format!("entry {name:?} expands {}x, the limit is {}x", claimed / packed.max(1), limits.max_ratio),
            ));
        }
        let mut f = zip.by_index(i).map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
        let mut data = Vec::with_capacity(claimed.min(64 * 1024 * 1024) as usize);
        // The header can lie about the size: stop one byte past the limit.
        let got = (&mut f)
            .take(limits.max_entry_bytes.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(|e| bad(format!("cannot decompress entry {name:?}: {e}")))? as u64;
        if got > limits.max_entry_bytes {
            return Err(refuse(
                limits,
                LimitKind::ZipBytes,
                format!("A file in this package unpacks to more than {} bytes. It was not processed.", limits.max_entry_bytes),
                format!("entry {name:?} expands past {} bytes", limits.max_entry_bytes),
            ));
        }
        if got > limits.ratio_floor_bytes && got / packed.max(1) > limits.max_ratio {
            return Err(refuse(
                limits,
                LimitKind::ZipRatio,
                format!("A file in this package expands more than {}x. It was not processed.", limits.max_ratio),
                format!("entry {name:?} expands {}x, the limit is {}x", got / packed.max(1), limits.max_ratio),
            ));
        }
        total += got;
        if total > limits.max_total_bytes {
            return Err(refuse(
                limits,
                LimitKind::ZipBytes,
                format!("This package unpacks to more than {} bytes. It was not processed.", limits.max_total_bytes),
                format!("the package expands past {} bytes", limits.max_total_bytes),
            ));
        }
        out.push(Entry { name, data, modified });
    }
    Ok(out)
}

/// Write entries as a zip (deflate), in the given order.
pub fn write_zip(entries: &[Entry], default_time: (u16, u8, u8, u8, u8, u8)) -> Result<Vec<u8>, Error> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for e in entries {
        let t = e.modified.unwrap_or(default_time);
        let dt = zip::DateTime::from_date_and_time(t.0, t.1, t.2, t.3, t.4, t.5)
            .unwrap_or_else(|_| zip::DateTime::default());
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(dt);
        w.start_file(&e.name, opts).map_err(|err| bad(format!("cannot write {:?}: {err}", e.name)))?;
        w.write_all(&e.data).map_err(|err| bad(format!("cannot write {:?}: {err}", e.name)))?;
    }
    let cursor = w.finish().map_err(|e| bad(format!("cannot finish the zip: {e}")))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(safe_name("en-US/dir1/a.sdlxliff").unwrap(), "en-US/dir1/a.sdlxliff");
        assert_eq!(safe_name("en-US\\dir1\\a.sdlxliff").unwrap(), "en-US/dir1/a.sdlxliff");
        for bad in ["../x", "a/../../x", "/etc/passwd", "\\x", "C:\\x", "a/b:stream", "a\0b", "", "..", "a/ ../b", "a/..\\b"] {
            assert!(safe_name(bad).is_err(), "{bad:?} must be refused");
        }
    }
}
