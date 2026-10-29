//! Safe zip reading and writing for DOCX. The same limits as `catcore` uses for packages: no names that can
//! escape a folder, no encrypted entries or links, caps on entry count, size and expansion ratio. Nothing is
//! extracted to disk. Compression method and time stamp of every entry are kept so a rebuilt file looks like
//! the original.

use bilingua_model::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
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

/// An entry's original deflated bytes with the checksum and size of its unpacked data. Entries nobody changes
/// are kept (and written back) in this form, so a merge never recompresses them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Packed {
    pub crc32: u32,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub name: String,
    /// The unpacked bytes.
    pub data: Vec<u8>,
    pub meta: EntryMeta,
    /// The original deflated bytes and their description, for entries that were deflated.
    pub packed: Option<(Packed, Vec<u8>)>,
}

impl RawEntry {
    /// The bytes to keep in the skeleton for an entry that is not rewritten: the original deflated bytes when
    /// there are any (recorded in `packed` under the entry's name), else the plain bytes.
    pub fn keep(&self, packed: &mut BTreeMap<String, Packed>) -> Vec<u8> {
        match &self.packed {
            Some((p, bytes)) => {
                packed.insert(self.name.clone(), p.clone());
                bytes.clone()
            }
            None => self.data.clone(),
        }
    }
}

/// What `write_zip_parts` writes for one entry.
pub enum Body<'a> {
    /// Unpacked bytes, compressed on the way out as the entry's metadata says.
    Plain(&'a [u8]),
    /// Bytes built by the merge itself.
    Owned(Vec<u8>),
    /// The original deflated bytes, copied without recompression.
    Packed(&'a Packed, &'a [u8]),
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
                packed: None,
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
        let mut pk = if stored {
            None
        } else {
            let mut raw = zip
                .by_index_raw(i)
                .map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
            let crc32 = raw.crc32();
            let mut bytes = Vec::with_capacity(raw.compressed_size().min(64 * 1024 * 1024) as usize);
            (&mut raw)
                .take(limits.max_entry_bytes + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
            Some((Packed { crc32, size: claimed }, bytes))
        };
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
        if let Some((p, _)) = pk.as_mut() {
            p.size = got;
        }
        out.push(RawEntry { name, data, meta, packed: pk });
    }
    Ok(out)
}

#[cfg(test)]
pub fn write_zip(entries: &[(EntryMeta, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    let parts: Vec<(EntryMeta, Body)> = entries
        .iter()
        .map(|(m, d)| (m.clone(), Body::Plain(d)))
        .collect();
    write_zip_parts(&parts)
}

/// A one-entry zip holding already deflated bytes, so the zip writer can copy them as they are.
fn single_entry_zip(name: &str, p: &Packed, bytes: &[u8]) -> Vec<u8> {
    let nb = name.as_bytes();
    let (csize, usize_) = (bytes.len() as u32, p.size as u32);
    let mut z = Vec::with_capacity(bytes.len() + 2 * nb.len() + 128);
    let w16 = |z: &mut Vec<u8>, v: u16| z.extend_from_slice(&v.to_le_bytes());
    let w32 = |z: &mut Vec<u8>, v: u32| z.extend_from_slice(&v.to_le_bytes());
    // local file header
    w32(&mut z, 0x0403_4b50);
    w16(&mut z, 20);
    w16(&mut z, 0x0800); // names are UTF-8
    w16(&mut z, 8); // deflate
    w16(&mut z, 0);
    w16(&mut z, 0x21); // 1980-01-01; the caller sets the real time
    w32(&mut z, p.crc32);
    w32(&mut z, csize);
    w32(&mut z, usize_);
    w16(&mut z, nb.len() as u16);
    w16(&mut z, 0);
    z.extend_from_slice(nb);
    z.extend_from_slice(bytes);
    let cd_start = z.len() as u32;
    // central directory header
    w32(&mut z, 0x0201_4b50);
    w16(&mut z, 20);
    w16(&mut z, 20);
    w16(&mut z, 0x0800);
    w16(&mut z, 8);
    w16(&mut z, 0);
    w16(&mut z, 0x21);
    w32(&mut z, p.crc32);
    w32(&mut z, csize);
    w32(&mut z, usize_);
    w16(&mut z, nb.len() as u16);
    w16(&mut z, 0);
    w16(&mut z, 0);
    w16(&mut z, 0);
    w16(&mut z, 0);
    w32(&mut z, 0);
    w32(&mut z, 0);
    z.extend_from_slice(nb);
    let cd_len = z.len() as u32 - cd_start;
    // end of central directory
    w32(&mut z, 0x0605_4b50);
    w16(&mut z, 0);
    w16(&mut z, 0);
    w16(&mut z, 1);
    w16(&mut z, 1);
    w32(&mut z, cd_len);
    w32(&mut z, cd_start);
    w16(&mut z, 0);
    z
}

pub fn write_zip_parts(entries: &[(EntryMeta, Body<'_>)]) -> Result<Vec<u8>, Error> {
    let werr =
        |name: &str, e: &dyn std::fmt::Display| Error::Write(format!("cannot write {name:?}: {e}"));
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (m, body) in entries {
        let dt = m
            .modified
            .and_then(|t| {
                zip::DateTime::from_date_and_time(
                    t[0], t[1] as u8, t[2] as u8, t[3] as u8, t[4] as u8, t[5] as u8,
                )
                .ok()
            })
            .unwrap_or_default();
        let data: &[u8] = match body {
            Body::Packed(p, bytes) if !m.dir && !m.stored => {
                // Untouched entry: copy its deflated bytes, header values and time stamp as they were.
                let one = single_entry_zip(&m.name, p, bytes);
                let mut src =
                    zip::ZipArchive::new(Cursor::new(one)).map_err(|e| werr(&m.name, &e))?;
                let f = src.by_index_raw(0).map_err(|e| werr(&m.name, &e))?;
                w.raw_copy_file_touch(f, dt, None)
                    .map_err(|e| werr(&m.name, &e))?;
                continue;
            }
            Body::Plain(d) => d,
            Body::Owned(d) => d,
            Body::Packed(..) => {
                return Err(Error::Write(format!(
                    "{:?} is marked packed but stored without compression",
                    m.name
                )))
            }
        };
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
