//! Safe zip reading and writing for the Office parsers (DOCX, XLSX, PPTX). Localization OS's zip guard: no names
//! that can escape a folder, no name listed twice (exactly, by case or by `\` against `/`, in any directory layout),
//! no encrypted entries or links, caps on entry count, total unpacked size and
//! expansion ratio (refused with a typed [`bilingua_model::LimitError`]), and the real size and checksum of every
//! entry proven against what the directory claims. Nothing is extracted to disk. Compression method and time stamp of every entry are kept so a rebuilt file looks like
//! the original.

#[doc(hidden)]
pub mod hostile;

use bilingua_model::{Error, LimitError, LimitKind, Limits};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write};

/// The zip part of [`Limits`]. `Default` is Localization OS's numbers (10 000 entries, 256 MB unpacked, 200 times).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct ZipLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
    pub max_ratio: u64,
    pub ratio_floor_bytes: u64,
}

impl Default for ZipLimits {
    fn default() -> Self {
        ZipLimits::from(&Limits::default())
    }
}

impl From<&Limits> for ZipLimits {
    fn from(l: &Limits) -> ZipLimits {
        ZipLimits {
            max_entries: l.max_zip_entries,
            // LocOS has no per-entry cap beyond the total (the default of `max_zip_entry_bytes`).
            max_entry_bytes: l.max_zip_entry_bytes.min(l.max_zip_bytes),
            max_total_bytes: l.max_zip_bytes,
            max_ratio: l.max_zip_ratio,
            ratio_floor_bytes: l.zip_ratio_floor_bytes,
        }
    }
}

fn limit(kind: LimitKind, msg: String) -> Error {
    Error::Limit(LimitError::new(kind, msg))
}

fn mb(n: u64) -> String {
    format!("{} MB", n.div_ceil(1024 * 1024))
}

fn invalid_sizes() -> Error {
    limit(LimitKind::ZipMalformed, "This file has invalid ZIP sizes or data. It was not processed.".into())
}

/// Runs the XML guard ([`bilingua_model::guard::check_xml`]) over every XML part of an unpacked package
/// (`.xml`, `.rels`, `.vml`, `.xlf`, `.xliff`, `.sdlxliff`), so a reader never parses a part with an entity bomb.
pub fn check_xml_parts(entries: &[RawEntry], limits: &Limits) -> Result<(), Error> {
    check_xml_parts_except(entries, limits, &[])
}

/// [`check_xml_parts`] for the parts whose name starts with none of `skip_prefixes` (case-sensitive, as LocOS's
/// `zip_guard` compares them): a DOCX's `customXml/` parts are never parsed into units, so their depth or size never
/// refuses the document (LocOS only scans them for external links).
pub fn check_xml_parts_except(entries: &[RawEntry], limits: &Limits, skip_prefixes: &[&str]) -> Result<(), Error> {
    for e in entries {
        if skip_prefixes.iter().any(|p| e.name.starts_with(p)) {
            continue;
        }
        let n = e.name.to_ascii_lowercase();
        if !e.meta.dir && [".xml", ".rels", ".vml", ".xlf", ".xliff", ".sdlxliff"].iter().any(|x| n.ends_with(x)) {
            bilingua_model::guard::check_xml(&e.data, limits)?;
        }
    }
    Ok(())
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

    /// [`keep`](Self::keep) without the copy: moves the bytes out and leaves the entry's `data` and `packed` empty
    /// (its name and metadata stay). For the last pass over the entries, when nothing reads them afterwards.
    pub fn take_kept(&mut self, packed: &mut BTreeMap<String, Packed>) -> Vec<u8> {
        match self.packed.take() {
            Some((p, bytes)) => {
                packed.insert(self.name.clone(), p);
                self.data = Vec::new();
                bytes
            }
            None => std::mem::take(&mut self.data),
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
        format: "zip".into(),
        msg: msg.into(),
    }
}

/// Gives a reader error the caller's format name ("docx", "xlsx", ...).
fn as_format(e: Error, format: &str) -> Error {
    match e {
        Error::Parse { msg, .. } => Error::Parse {
            format: format.into(),
            msg,
        },
        e => e,
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

fn duplicate(name: &str) -> Error {
    limit(LimitKind::ZipMalformed, format!("This file lists the part {name:?} twice (duplicate names). It was not processed."))
}

/// Refuses an archive whose central directory lists one name more than once. The zip reader keeps one entry per
/// exact name and drops the others without a word, so a second copy of a part would be invisible here while another
/// reader of the same file might use it. The reader's own count of the records it read is used, so the check holds
/// however the directory is found (a zip64 end record, bytes before the archive). Names that differ only in case or
/// in `\` against `/` both reach the reader and are refused by [`read_zip`] itself.
pub fn refuse_duplicate_names<R: Read + std::io::Seek>(zip: &zip::ZipArchive<R>) -> Result<(), Error> {
    if zip.has_duplicate_names() {
        // the reader kept one record per name, so it cannot say which name was repeated
        return Err(limit(
            LimitKind::ZipMalformed,
            "This file lists the same part twice (duplicate names). It was not processed.".into(),
        ));
    }
    Ok(())
}

/// Reads every entry of a zip file within `limits`. Errors name `format` ("docx", "xlsx", ...).
pub fn read_zip(format: &str, bytes: &[u8], limits: &ZipLimits) -> Result<Vec<RawEntry>, Error> {
    read_entries(bytes, limits, &mut NoWatch).map_err(|e| as_format(e, format))
}

/// Sees the bytes of each entry as [`read_zip_watched`] inflates them, so a format can count what its limits
/// measure while the package is read and refuse an oversized file before the rest is unpacked and before any tree
/// is built (BILI-115).
pub trait Watch {
    /// The next inflated bytes of the file entry `name` (never a directory). An error stops the read and is
    /// returned as it is.
    fn bytes(&mut self, name: &str, chunk: &[u8]) -> Result<(), Error>;
    /// The entry `name` is fully inflated and its size proven.
    fn done(&mut self, name: &str) -> Result<(), Error>;
}

struct NoWatch;

impl Watch for NoWatch {
    fn bytes(&mut self, _: &str, _: &[u8]) -> Result<(), Error> {
        Ok(())
    }
    fn done(&mut self, _: &str) -> Result<(), Error> {
        Ok(())
    }
}

/// How many inflated bytes a watcher sees at a time.
const WATCH_CHUNK: usize = 256 * 1024;

/// [`read_zip`] with a [`Watch`] over each file entry's bytes as they inflate. The directory checks (entry count,
/// unpacked total, ratio) run first, as in `read_zip`. The watcher sees an entry's bytes while it inflates, before
/// that entry's own checks (its real size against the directory and `max_entry_bytes`, its checksum) have run: the
/// entries before it have passed all of `read_zip`'s checks, the current one may not have yet, so a watcher refusal
/// can come where `read_zip` would have refused that entry for its size or checksum. An archive the watcher never
/// refuses is read exactly as `read_zip` reads it.
pub fn read_zip_watched(format: &str, bytes: &[u8], limits: &ZipLimits, watch: &mut dyn Watch) -> Result<Vec<RawEntry>, Error> {
    read_entries(bytes, limits, watch).map_err(|e| as_format(e, format))
}

fn read_entries(bytes: &[u8], limits: &ZipLimits, watch: &mut dyn Watch) -> Result<Vec<RawEntry>, Error> {
    if bytes.len() < 22 || !(bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06")) {
        return Err(bad("not a zip file (no PK signature)"));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| bad(format!("unreadable zip: {e}")))?;
    refuse_duplicate_names(&zip)?;
    if zip.len() > limits.max_entries {
        return Err(limit(
            LimitKind::ZipEntries,
            format!("This file has {} internal parts; the limit is {}. It was not processed.", zip.len(), limits.max_entries),
        ));
    }
    // Early rejection from the directory, before anything is inflated: the sum of the claimed sizes and the
    // ratio of every large entry. The real sizes are proven below.
    let mut claimed_total: u64 = 0;
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(|e| bad(format!("unreadable entry {i}: {e}")))?;
        claimed_total = claimed_total.saturating_add(f.size());
        // exact, like LocOS's float ratio: 200.5 times is over a limit of 200
        if f.size() > limits.ratio_floor_bytes && f.size() > f.compressed_size().max(1).saturating_mul(limits.max_ratio) {
            return Err(limit(
                LimitKind::ZipRatio,
                format!(
                    "This file contains a part that expands {} times (limit {} times), which is typical of a compression bomb. It was not processed.",
                    f.size() / f.compressed_size().max(1),
                    limits.max_ratio
                ),
            ));
        }
    }
    if claimed_total > limits.max_total_bytes {
        return Err(limit(
            LimitKind::ZipBytes,
            format!(
                "This file expands to {} when unpacked; the limit is {}. It was not processed.",
                mb(claimed_total),
                mb(limits.max_total_bytes)
            ),
        ));
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let (name, is_dir, claimed, _packed, modified, stored, deflated) = {
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
            let deflated = f.compression() == zip::CompressionMethod::Deflated;
            (
                name,
                f.is_dir() || raw.ends_with('/') || raw.ends_with('\\'),
                f.size(),
                f.compressed_size(),
                modified,
                stored,
                deflated,
            )
        };
        // case-insensitive, `\` read as `/` (safe_name), as Windows and the Office readers see the names
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(duplicate(&name));
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
            return Err(limit(
                LimitKind::ZipBytes,
                format!("This file has a part of {}; the limit is {}. It was not processed.", mb(claimed), mb(limits.max_entry_bytes)),
            ));
        }
        // Only plain deflate data is kept packed, and only when every compressed byte was read; anything else
        // (another method the reader would refuse below anyway, a short read) falls back to plain bytes.
        let mut pk = if !deflated {
            None
        } else {
            let mut raw = zip
                .by_index_raw(i)
                .map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
            let crc32 = raw.crc32();
            let want = raw.compressed_size();
            let mut bytes = Vec::with_capacity(want.min(64 * 1024 * 1024) as usize);
            (&mut raw)
                .take(limits.max_entry_bytes + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
            (bytes.len() as u64 == want).then_some((
                Packed {
                    crc32,
                    size: claimed,
                },
                bytes,
            ))
        };
        let mut f = zip
            .by_index(i)
            .map_err(|e| bad(format!("cannot read entry {name:?}: {e}")))?;
        let mut data = Vec::with_capacity(claimed.min(64 * 1024 * 1024) as usize);
        // never inflate past the claimed size (+1 to see a forged one): a forged entry costs no more memory than an
        // honest one, as LocOS's bounded verify. Read in pieces, each shown to the watcher as it arrives.
        let mut body = (&mut f).take(claimed.min(limits.max_entry_bytes) + 1);
        loop {
            let at = data.len();
            let room = data.capacity() - at;
            let n = if room == 0 {
                // the reserved room is full: a small probe finds the end of an honest entry (it reads nothing, so
                // `data` keeps its size); only an entry longer than its reservation grows `data` here
                let mut probe = [0u8; 4096];
                let n = read_some(&mut body, &mut probe).map_err(|_| invalid_sizes())?;
                data.extend_from_slice(&probe[..n]);
                n
            } else {
                data.resize(at + room.min(WATCH_CHUNK), 0);
                let n = read_some(&mut body, &mut data[at..]).map_err(|_| invalid_sizes())?;
                data.truncate(at + n);
                n
            };
            if n == 0 {
                break;
            }
            watch.bytes(&name, &data[at..])?;
        }
        let got = data.len() as u64;
        if got > limits.max_entry_bytes {
            return Err(limit(
                LimitKind::ZipBytes,
                format!("This file has a part that expands past {}. It was not processed.", mb(limits.max_entry_bytes)),
            ));
        }
        // A directory that understates the real size (a forged header) is a bomb in disguise.
        if got != claimed {
            return Err(invalid_sizes());
        }
        total += got;
        if total > limits.max_total_bytes {
            return Err(limit(
                LimitKind::ZipBytes,
                format!("This file expands past {} when unpacked. It was not processed.", mb(limits.max_total_bytes)),
            ));
        }
        if let Some((p, _)) = pk.as_mut() {
            p.size = got;
        }
        watch.done(&name)?;
        out.push(RawEntry {
            name,
            data,
            meta,
            packed: pk,
        });
    }
    Ok(out)
}

/// Fills `buf` as far as the reader goes (a deflate reader hands out less than asked for), retrying an
/// interrupted read as `read_to_end` does. Returns how many bytes were read; fewer than `buf.len()` only at the end.
fn read_some(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
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

/// Writes plain entries (each compressed as its metadata says). The names are written as given, unchecked: for a
/// package rebuilt from members [`read_zip`] read (and so checked), and for test fixtures that build hostile
/// archives. A merge that writes names from a stored Document uses [`write_zip_parts`], which checks them.
pub fn write_zip(entries: &[(EntryMeta, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    let parts: Vec<(EntryMeta, Body)> = entries
        .iter()
        .map(|(m, d)| (m.clone(), Body::Plain(d)))
        .collect();
    write_entries(&parts)
}

/// The body to write for a skeleton entry kept as raw bytes: the original deflated bytes when `packed` (the
/// format's record of packed entries) names the entry, else plain bytes.
pub fn raw_body<'a>(
    packed: Option<&'a BTreeMap<String, Packed>>,
    name: &str,
    bytes: &'a [u8],
) -> Body<'a> {
    match packed.and_then(|m| m.get(name)) {
        Some(p) => Body::Packed(p, bytes),
        None => Body::Plain(bytes),
    }
}

/// Proves that packed bytes are what the skeleton says they are before they are copied: they must inflate to
/// exactly the recorded size and checksum. Bytes that were edited, cut short or replaced with plain bytes are
/// refused here, so a broken entry never reaches the output file.
fn check_packed(src: &mut zip::ZipArchive<Cursor<Vec<u8>>>, p: &Packed) -> Result<(), String> {
    if p.size > u32::MAX as u64 {
        return Err("a packed entry is too large to copy".into());
    }
    let f = src.by_index(0).map_err(|e| e.to_string())?;
    // `take` stops a forged record from inflating without end; the reader checks the CRC at the end.
    let got = std::io::copy(&mut f.take(p.size + 1), &mut std::io::sink())
        .map_err(|e| format!("the kept compressed bytes do not match their record ({e})"))?;
    if got != p.size {
        return Err(format!(
            "the kept compressed bytes inflate to {got} bytes, the record says {}",
            p.size
        ));
    }
    Ok(())
}

/// Writes a zip from a skeleton's entries. The names come from a stored Document, so every one must be a name the
/// reader gives (BILI-105): it passes [`safe_name`] and is already in the form `safe_name` gives it (no `\`, no
/// `./` or empty component), and no two are the same once case is ignored, as [`read_zip`] compares them.
pub fn write_zip_parts(entries: &[(EntryMeta, Body<'_>)]) -> Result<Vec<u8>, Error> {
    check_written_names(entries.iter().map(|(m, _)| m.name.as_str()))?;
    write_entries(entries)
}

/// The name check of [`write_zip_parts`], for a writer that builds its own entry list.
pub fn check_written_names<'a>(names: impl Iterator<Item = &'a str>) -> Result<(), Error> {
    let mut seen: HashSet<String> = HashSet::new();
    for name in names {
        if safe_name(name)? != name {
            return Err(limit(
                LimitKind::ZipMalformed,
                format!("The entry name {name:?} is not in the form the zip reader gives it. It was not written."),
            ));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(limit(LimitKind::ZipMalformed, format!("The entry name {name:?} is listed twice. It was not written.")));
        }
    }
    Ok(())
}

fn write_entries(entries: &[(EntryMeta, Body<'_>)]) -> Result<Vec<u8>, Error> {
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
                check_packed(&mut src, p).map_err(|e| werr(&m.name, &e))?;
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
        assert_eq!(safe_name("word/document.xml").unwrap(), "word/document.xml");
        assert_eq!(safe_name("./xl\\workbook.xml").unwrap(), "xl/workbook.xml");
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
        // Windows device names are fine inside a package: nothing here becomes a file on disk (the zip batch, whose
        // members are handed back as files, refuses them in `bilingua::batch::safe_member_path`)
        for ok in ["OEBPS/aux.xhtml", "word/con.xml", "CON", "nul.txt", "com1.xml"] {
            assert!(safe_name(ok).is_ok(), "{ok:?} must pass");
        }
    }

    /// BILI-105: the writer checks every name it is given, since a stored Document can carry any.
    #[test]
    fn writer_refuses_an_unsafe_name() {
        let meta = |name: &str| EntryMeta { name: name.into(), stored: false, dir: false, modified: None };
        assert!(write_zip_parts(&[(meta("word/document.xml"), Body::Plain(b"x"))]).is_ok());
        for bad in ["../x.xml", "/abs.xml", "a/b:stream", "a\0b", "a\u{1b}b", "word\\document.xml", "./word/x.xml", "word//x.xml", ""] {
            assert!(write_zip_parts(&[(meta(bad), Body::Plain(b"x"))]).is_err(), "{bad:?} must be refused");
        }
        // a folder keeps its slash
        assert!(write_zip_parts(&[(meta("word/"), Body::Plain(b""))]).is_ok());
        // a name listed twice, also in another case
        for (a, b) in [("word/a.xml", "word/a.xml"), ("word/a.xml", "Word/A.xml")] {
            let e = write_zip_parts(&[(meta(a), Body::Plain(b"x")), (meta(b), Body::Plain(b"y"))]).unwrap_err();
            assert!(e.to_string().contains("twice"), "{e}");
        }
    }

    /// BILI-115: the watcher sees every byte of every file entry, in order, and its refusal stops the read.
    #[test]
    fn the_watcher_sees_each_entry_as_it_inflates() {
        struct Seen(Vec<(String, Vec<u8>)>, Option<usize>);
        impl Watch for Seen {
            fn bytes(&mut self, name: &str, chunk: &[u8]) -> Result<(), Error> {
                match self.0.last_mut() {
                    Some((n, d)) if n == name => d.extend_from_slice(chunk),
                    _ => self.0.push((name.to_string(), chunk.to_vec())),
                }
                match self.1 {
                    Some(max) if self.0.iter().map(|(_, d)| d.len()).sum::<usize>() > max => {
                        Err(limit(LimitKind::Paragraphs, "stop".into()))
                    }
                    _ => Ok(()),
                }
            }
            fn done(&mut self, _: &str) -> Result<(), Error> {
                Ok(())
            }
        }
        let meta = |name: &str, stored: bool| EntryMeta { name: name.into(), stored, dir: false, modified: None };
        // random bytes: a repeating pattern would deflate past the ratio limit
        let mut seed = 0x115u64;
        let big: Vec<u8> = (0..3_000_000)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 56) as u8
            })
            .collect();
        let zip = write_zip(&[
            (meta("a.xml", false), b"<a/>".to_vec()),
            (meta("empty.xml", false), Vec::new()),
            (meta("big.bin", false), big.clone()),
            (meta("stored.bin", true), big[..70_000].to_vec()),
        ])
        .unwrap();
        let mut seen = Seen(Vec::new(), None);
        let entries = read_zip_watched("x", &zip, &ZipLimits::default(), &mut seen).unwrap();
        assert_eq!(entries.len(), 4);
        let plain = read_zip("x", &zip, &ZipLimits::default()).unwrap();
        for (a, b) in entries.iter().zip(&plain) {
            assert_eq!((&a.name, &a.data), (&b.name, &b.data));
        }
        let with_bytes: Vec<(String, Vec<u8>)> = plain.iter().filter(|e| !e.data.is_empty()).map(|e| (e.name.clone(), e.data.clone())).collect();
        assert_eq!(seen.0, with_bytes);
        let mut stop = Seen(Vec::new(), Some(1_000_000));
        let e = read_zip_watched("x", &zip, &ZipLimits::default(), &mut stop).unwrap_err();
        assert!(matches!(e, Error::Limit(LimitError { kind: LimitKind::Paragraphs, .. })), "{e:?}");
        // it stopped inside the big entry, a piece or so past the limit
        assert!(stop.0.iter().map(|(_, d)| d.len()).sum::<usize>() <= 1_000_000 + WATCH_CHUNK);
    }
}
