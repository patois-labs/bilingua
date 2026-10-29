//! Safe zip reading and writing for DOCX. The same limits as `catcore` uses for packages: no names that can
//! escape a folder, no encrypted entries or links, caps on entry count, size and expansion ratio. Nothing is
//! extracted to disk. Compression method and time stamp of every entry are kept so a rebuilt file looks like
//! the original.

use bilingua_model::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Cursor, Read, Write};

#[derive(Debug, Clone, Copy)]
pub struct ZipLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
    pub max_ratio: u64,
    pub ratio_floor_bytes: u64,
}

impl Default for ZipLimits {
    fn default() -> Self {
        ZipLimits {
            max_entries: 20_000,
            max_entry_bytes: 512 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_ratio: 200,
            ratio_floor_bytes: 1024 * 1024,
        }
    }
}

/// How one entry was stored, kept so the writer can repeat it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryMeta {
    pub name: String,
    pub stored: bool,
    pub dir: bool,
    pub modified: Option<[u16; 6]>,
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub meta: EntryMeta,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Parse {
        format: "xlsx".into(),
        msg: msg.into(),
    }
}

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
        return Err(bad(format!("entry name {raw:?} contains a colon")));
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        match part {
            ".." => {
                return Err(bad(format!(
                    "entry name {raw:?} climbs out of the file (..)"
                )))
            }
            "." | "" => {}
            p => {
                if p.trim_end_matches(['.', ' ']).is_empty() {
                    return Err(bad(format!(
                        "entry name {raw:?} has a component made of dots or spaces"
                    )));
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

pub fn read_zip(bytes: &[u8], limits: &ZipLimits) -> Result<Vec<RawEntry>, Error> {
    if bytes.len() < 22 || !(bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")) {
        return Err(bad("not a zip file (no PK signature)"));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| bad(format!("unreadable zip: {e}")))?;
    if zip.len() > limits.max_entries {
        return Err(bad(format!(
            "{} entries, the limit is {}",
            zip.len(),
            limits.max_entries
        )));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let (name, is_dir, claimed, packed, modified, stored) = {
            let f = zip
                .by_index_raw(i)
                .map_err(|e| bad(format!("unreadable entry {i}: {e}")))?;
            if f.encrypted() {
                return Err(bad("an entry is encrypted"));
            }
            if f.is_symlink() {
                return Err(bad("the file holds a symbolic link"));
            }
            let raw = f
                .name()
                .map_err(|e| bad(format!("entry {i} has an unreadable name: {e}")))?
                .into_owned();
            let name = safe_name(&raw)?;
            let modified = f.last_modified().map(|d| {
                [
                    d.year(),
                    d.month() as u16,
                    d.day() as u16,
                    d.hour() as u16,
                    d.minute() as u16,
                    d.second() as u16,
                ]
            });
            let stored = f.compression() == zip::CompressionMethod::Stored;
            (
                name,
                f.is_dir() || raw.ends_with('/') || raw.ends_with('\\'),
                f.size(),
                f.compressed_size(),
                modified,
                stored,
            )
        };
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(bad(format!("entry {name:?} appears twice")));
        }
        let meta = EntryMeta {
            name: name.clone(),
            stored,
            dir: is_dir,
            modified,
        };
        if is_dir {
            out.push(RawEntry {
                name,
                data: vec![],
                meta,
            });
            continue;
        }
        if claimed > limits.max_entry_bytes {
            return Err(bad(format!(
                "entry {name:?} claims {claimed} bytes, the limit is {}",
                limits.max_entry_bytes
            )));
        }
        if claimed > limits.ratio_floor_bytes && claimed / packed.max(1) > limits.max_ratio {
            return Err(bad(format!(
                "entry {name:?} expands {}x, the limit is {}x",
                claimed / packed.max(1),
                limits.max_ratio
            )));
        }
        let mut f = zip
            .by_index(i)
            .map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
        let mut data = Vec::with_capacity(claimed.min(64 * 1024 * 1024) as usize);
        let got = (&mut f)
            .take(limits.max_entry_bytes + 1)
            .read_to_end(&mut data)
            .map_err(|e| bad(format!("cannot decompress entry {name:?}: {e}")))?
            as u64;
        if got > limits.max_entry_bytes {
            return Err(bad(format!(
                "entry {name:?} expands past {} bytes",
                limits.max_entry_bytes
            )));
        }
        if got > limits.ratio_floor_bytes && got / packed.max(1) > limits.max_ratio {
            return Err(bad(format!(
                "entry {name:?} expands {}x, the limit is {}x",
                got / packed.max(1),
                limits.max_ratio
            )));
        }
        total += got;
        if total > limits.max_total_bytes {
            return Err(bad(format!(
                "the file expands past {} bytes",
                limits.max_total_bytes
            )));
        }
        out.push(RawEntry { name, data, meta });
    }
    Ok(out)
}

pub fn write_zip(entries: &[(EntryMeta, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    let werr =
        |name: &str, e: &dyn std::fmt::Display| Error::Write(format!("cannot write {name:?}: {e}"));
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (m, data) in entries {
        let dt = m
            .modified
            .and_then(|t| {
                zip::DateTime::from_date_and_time(
                    t[0], t[1] as u8, t[2] as u8, t[3] as u8, t[4] as u8, t[5] as u8,
                )
                .ok()
            })
            .unwrap_or_default();
        let method = if m.stored {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(dt);
        if m.dir {
            w.add_directory(&m.name, opts)
                .map_err(|e| werr(&m.name, &e))?;
        } else {
            w.start_file(&m.name, opts).map_err(|e| werr(&m.name, &e))?;
            w.write_all(data).map_err(|e| werr(&m.name, &e))?;
        }
    }
    let cursor = w
        .finish()
        .map_err(|e| Error::Write(format!("cannot finish the zip: {e}")))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(safe_name("xl/workbook.xml").unwrap(), "xl/workbook.xml");
        for bad in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "C:\\x",
            "a/b:stream",
            "a\0b",
            "",
            "..",
            "a/ ../b",
        ] {
            assert!(safe_name(bad).is_err(), "{bad:?} must be refused");
        }
    }
}
