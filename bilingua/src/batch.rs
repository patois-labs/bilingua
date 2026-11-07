//! ZIP batches: one uploaded `.zip` becomes one document per supported member (Localization OS
//! `app/filters/zip_batch.py`).
//!
//! A batch is not a format: `.zip` is never claimed by [`crate::filters`], so [`crate::detect`] still refuses a
//! plain `.zip` and [`crate::extract`] never opens one. A caller that takes uploads recognises the extension with
//! [`is_batch`] and calls [`expand`]; every member it returns then goes through [`crate::extract`] as if it had
//! been uploaded on its own.
//!
//! Untrusted input, so the archive is read with the zip guard every package format uses (`bilingua-zip`: entry
//! count, unpacked size, expansion ratio, real sizes and checksums proven, names that climb out refused) before
//! any member is used. Then, as LocOS:
//! * a member path that is absolute, carries a drive letter, a `..` component, a control character, a
//!   component with surrounding spaces or a trailing dot, or a Windows device name (`CON`, `aux.md`, `COM1`)
//!   refuses the WHOLE archive;
//! * nested `.zip` members, OS junk (`__MACOSX/`, `.DS_Store`, `Thumbs.db`, `desktop.ini`), duplicate paths
//!   (ignoring case), members larger than `Limits::max_input_bytes` and unsupported formats are skipped, and
//!   every one is listed with its reason;
//! * more than [`BatchOptions::max_documents`] supported members (LocOS `ZIP_BATCH_MAX_DOCUMENTS`, 500) refuses
//!   the whole archive.
//!
//! The member's folder path is kept: [`Member::name`] is the path with `/` written as `__` (a document name may
//! not carry a path), [`Member::path`] the normalised relative path.
//!
//! Differences from LocOS, all stricter: a symbolic link, an encrypted member, a name listed twice in the central
//! directory (a copy the zip reader would hide), two names that differ only in ASCII case, and a name with a colon
//! refuse the archive (the shared zip guard refuses them for every package; LocOS skips a link and a case twin).
//! Names that differ only in non-ASCII case (`Ä.md`, `ä.md`) are skipped as duplicates, as LocOS does.
//!
//! Errors: bytes that are not a zip are [`Error::Parse`] for format `zip`; every refusal of a real archive (a guard
//! limit, an unsafe or repeated name, a link, too many documents) is an [`Error::Limit`], so a caller can show its
//! message to whoever uploaded the file. Nothing is returned in either case.
//!
//! ```
//! # fn run(upload_name: &str, bytes: &[u8]) -> Result<(), bilingua::Error> {
//! use bilingua::batch;
//! if batch::is_batch(upload_name) {
//!     let b = batch::expand_with(bytes, &batch::BatchOptions::new().with_max_documents(100))?;
//!     for m in &b.members {
//!         // each member is an upload of its own: a parse error belongs to that member, not to the batch
//!         match bilingua::extract(&m.name, &m.data, &bilingua::ExtractOptions::new()) {
//!             Ok(doc) => println!("{}: {} units", m.path, doc.units.len()),
//!             Err(e) => println!("{}: {e}", m.path),
//!         }
//!     }
//!     for s in &b.skipped {
//!         println!("{}: skipped, {}", s.path, s.reason);
//!     }
//! }
//! # Ok(()) }
//! ```

use bilingua_model::{Error, LimitError, LimitKind, Limits};

/// The extensions a batch upload has.
pub const BATCH_EXTS: &[&str] = &["zip"];

/// What joins the folders of a member path in its document name.
pub const NAME_SEPARATOR: &str = "__";

const JUNK_NAMES: &[&str] = &[".ds_store", "thumbs.db", "desktop.ini"];

/// A supported member of a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Member {
    /// The normalised relative path inside the archive (`docs/guide/intro.md`).
    pub path: String,
    /// The document name (`docs__guide__intro.md`).
    pub name: String,
    /// The member's bytes.
    pub data: Vec<u8>,
}

/// A member that was not taken, with the reason (shown to the user; never dropped without a word).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Skipped {
    /// The member's path.
    pub path: String,
    /// Why it was skipped.
    pub reason: String,
}

/// An expanded batch: the supported members in archive order, and the skipped ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Batch {
    /// Supported members, archive order.
    pub members: Vec<Member>,
    /// Skipped members, archive order.
    pub skipped: Vec<Skipped>,
}

/// Limits for [`expand_with`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BatchOptions {
    /// The zip guard's numbers, and `max_input_bytes`: a member larger than it is skipped.
    pub limits: Limits,
    /// More supported members than this refuse the archive (LocOS `ZIP_BATCH_MAX_DOCUMENTS`).
    pub max_documents: usize,
}

impl Default for BatchOptions {
    fn default() -> Self {
        BatchOptions { limits: Limits::default(), max_documents: 500 }
    }
}

impl BatchOptions {
    /// LocOS's numbers (500 documents, the default [`Limits`]).
    pub fn new() -> BatchOptions {
        BatchOptions::default()
    }
    /// Sets the limits.
    pub fn with_limits(mut self, limits: Limits) -> BatchOptions {
        self.limits = limits;
        self
    }
    /// Sets the document cap.
    pub fn with_max_documents(mut self, n: usize) -> BatchOptions {
        self.max_documents = n;
        self
    }
}

/// True for a file name whose extension is a batch (`.zip`), whatever the folder.
pub fn is_batch(file_name: &str) -> bool {
    let leaf = file_name.rsplit(['/', '\\']).next().unwrap_or("");
    match leaf.rsplit_once('.') {
        Some((_, ext)) => BATCH_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)),
        None => false,
    }
}

/// A Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`, and the superscript
/// `COM¹` to `LPT³`), in any case, with or without an extension: Windows opens the device, not a file of that name.
fn is_device_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or("").trim_end_matches(' ').to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let Some(n) = stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT")) else { return false };
    let mut c = n.chars();
    matches!((c.next(), c.next()), (Some('1'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None))
}

/// The normalised relative path of a member, or `None` when it is unsafe (absolute, drive-qualified, `..`, a
/// control character, empty, a component with surrounding spaces or a trailing dot, which Windows would drop
/// so that two members collide, or a component that is a Windows device name). Use it again before writing a
/// stored path anywhere.
pub fn safe_member_path(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.chars().any(|c| (c as u32) < 0x20 || c == '\u{7F}') {
        return None;
    }
    let p = raw.replace('\\', "/");
    let b = p.as_bytes();
    if p.starts_with('/') || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':') {
        return None;
    }
    let parts: Vec<&str> = p.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    if parts.is_empty() || parts.contains(&"..") {
        return None;
    }
    // Python's str.strip() (Unicode white space), as LocOS
    if parts.iter().any(|c| bilingua_model::py_strip(c) != *c || c.ends_with('.')) {
        return None;
    }
    // the members go back to the user as files (issue 812): `CON/a.md` or `aux.md` would open a device on Windows
    if parts.iter().any(|c| is_device_name(c)) {
        return None;
    }
    Some(parts.join("/"))
}

/// The document name for a member path: folders kept, `/` written as `__`.
pub fn document_name(path: &str) -> String {
    path.split('/').collect::<Vec<_>>().join(NAME_SEPARATOR)
}

fn ext_of(path: &str) -> String {
    let leaf = path.rsplit('/').next().unwrap_or("");
    leaf.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// Every extension some filter claims (built once per batch, not once per member).
fn supported_exts() -> std::collections::HashSet<&'static str> {
    crate::filters().iter().flat_map(|f| f.extensions().iter().copied()).collect()
}

fn refuse(msg: String) -> Error {
    Error::Limit(LimitError::new(LimitKind::ZipMalformed, msg))
}

/// The shared reader's errors in batch terms: not a zip stays a parse error; any other refusal of the archive (an
/// unsafe name, a link, an encrypted or unreadable member) is a limit error with a message for the uploader.
fn reader_error(e: Error) -> Error {
    match e {
        Error::Parse { msg, .. } if msg.contains("not a zip") || msg.contains("unreadable zip") => {
            Error::Parse { format: "zip".into(), msg: "This .zip file is not a readable ZIP archive.".into() }
        }
        Error::Parse { msg, .. } if msg.starts_with("entry name") || msg.starts_with("an entry has an empty name") => {
            refuse(format!("This ZIP archive contains an unsafe path ({msg}). It was not processed."))
        }
        Error::Parse { msg, .. } => refuse(format!("This ZIP archive cannot be opened safely ({msg}). It was not processed.")),
        e => e,
    }
}

/// [`expand_with`] with LocOS's numbers.
pub fn expand(bytes: &[u8]) -> Result<Batch, Error> {
    expand_with(bytes, &BatchOptions::default())
}

/// Opens a batch and returns its supported members (their bytes exactly as stored) plus the skipped list. Bytes
/// that are not a zip are an [`Error::Parse`] for format `zip`; an archive over a guard limit, with an unsafe or
/// repeated path, a link, or too many supported members is an [`Error::Limit`]. Nothing is returned in either case.
pub fn expand_with(bytes: &[u8], opts: &BatchOptions) -> Result<Batch, Error> {
    let l = &opts.limits;
    let zl = bilingua_zip::ZipLimits::from(l);
    // the shared guard (bomb, entry count, forged sizes, names) over the whole archive before any member is used
    let entries = bilingua_zip::read_zip("zip", bytes, &zl).map_err(reader_error)?;
    let exts = supported_exts();
    let mut batch = Batch::default();
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        if e.meta.dir {
            continue; // a folder entry carries no content
        }
        let Some(path) = safe_member_path(&e.name) else {
            let shown: String = e.name.chars().map(|c| if (c as u32) < 0x20 || c == '\u{7F}' { '?' } else { c }).take(120).collect();
            return Err(refuse(format!(
                "This ZIP archive contains an unsafe path ({shown:?}): absolute, drive-qualified, climbing out with '..', or a Windows device name. It was not processed."
            )));
        };
        let leaf = path.rsplit('/').next().unwrap_or("");
        let ext = ext_of(&path);
        let folded = path.to_lowercase();
        let reason = if path.split('/').next() == Some("__MACOSX") || JUNK_NAMES.contains(&leaf.to_lowercase().as_str()) {
            Some("system file (not translatable)".to_string())
        } else if seen.contains(&folded) {
            Some("duplicate path (another member already uses this name)".to_string())
        } else if BATCH_EXTS.contains(&ext.as_str()) {
            Some("nested ZIP archive (not opened; upload it separately)".to_string())
        } else if ext.is_empty() || !exts.contains(ext.as_str()) {
            Some(if ext.is_empty() { "unsupported format (no extension)".to_string() } else { format!("unsupported format (.{ext})") })
        } else if e.data.len() as u64 > l.max_input_bytes {
            Some(format!("larger than the upload limit ({} bytes)", l.max_input_bytes))
        } else {
            None
        };
        seen.insert(folded);
        if let Some(reason) = reason {
            batch.skipped.push(Skipped { path, reason });
            continue;
        }
        if batch.members.len() >= opts.max_documents {
            return Err(Error::Limit(LimitError::new(
                LimitKind::Units,
                format!(
                    "This ZIP archive holds more than {} supported files (ZIP_BATCH_MAX_DOCUMENTS). Split it into smaller archives. It was not processed.",
                    opts.max_documents
                ),
            )));
        }
        let name = document_name(&path);
        batch.members.push(Member { path, name, data: e.data });
    }
    Ok(batch)
}
