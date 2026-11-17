//! Safety limits and the XML guard, one place for every parser (D27 step 5).
//!
//! The numbers are Localization OS's (`app/config.py`): a 100 MB input, 50 000 units, 100 000 characters in one
//! unit, twenty lines per allowed unit, a zip of at most 10 000 entries that unpacks to at most 256 MB with no
//! entry expanding more than 200 times. The caller can change any of them through `ExtractOptions::limits`.
//! Each refusal is a [`LimitError`] with a [`LimitKind`] a program can branch on and a short message that is
//! safe to show to the person who uploaded the file.
//!
//! The XML guard ([`check_xml`]) reads no DTD and expands no entity: a document that declares an entity
//! (internal or external, so the billion-laughs and XXE families) is refused before any parser sees it, and the
//! element depth and count are bounded. A plain `<!DOCTYPE x>` or `<!DOCTYPE x SYSTEM "x.dtd">` is allowed because
//! the parsers never fetch or apply it.

use crate::{Document, Error};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Which limit a refusal is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LimitKind {
    /// The file is too large.
    InputBytes,
    /// Too many lines.
    Lines,
    /// Too many units or segments.
    Units,
    /// A unit is too long.
    UnitChars,
    /// Too much key, note and other context text on the units (`Limits::max_context_bytes`).
    Context,
    /// XML nested too deeply.
    XmlDepth,
    /// Nesting deeper than allowed in a format that is not XML (HTML, JSON, YAML, ...). Uses
    /// `Limits::max_xml_depth`, which is the nesting cap for every format.
    Depth,
    /// Too many XML elements.
    XmlElements,
    /// An XML entity declaration (DTD): entity bombs and external entities.
    XmlEntities,
    /// More paragraphs in a word-processing file than allowed (empty ones count).
    Paragraphs,
    /// A spreadsheet over its cell, row, characters-per-cell or total-text limit.
    Spreadsheet,
    /// Too many zip entries.
    ZipEntries,
    /// Too many bytes once unpacked.
    ZipBytes,
    /// A zip entry expands too much (a zip bomb).
    ZipRatio,
    /// A zip whose declared sizes or names are unsafe or do not match its data.
    ZipMalformed,
    /// A merge would write more than `Limits::max_output_bytes`.
    OutputBytes,
    /// A stored document's skeleton is larger, or refers to its units more often (in all, or to any one unit), than
    /// any extraction makes it, or refers to a unit the document does not have. Merge only.
    Skeleton,
}

/// A file refused for being over a limit or unsafe.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LimitError {
    /// Which limit.
    pub kind: LimitKind,
    /// The message, safe to show the person who uploaded the file.
    pub msg: String,
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl LimitError {
    /// A limit error of this kind with this message.
    pub fn new(kind: LimitKind, msg: impl Into<String>) -> LimitError {
        LimitError { kind, msg: msg.into() }
    }
}

impl From<LimitError> for Error {
    fn from(e: LimitError) -> Error {
        Error::Limit(e)
    }
}

/// The limits one extraction runs under. `Default` is Localization OS's configuration.
/// Build them with [`Limits::new`] and the `with_*` methods (`Limits::new().with_max_units(500)`); new limits are
/// added without breaking callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct Limits {
    /// Largest input file (`MAX_UPLOAD_BYTES`).
    pub max_input_bytes: u64,
    /// Most units (and most segments) in a document (`MAX_DOCUMENT_SEGMENTS`).
    pub max_units: usize,
    /// Most characters in one unit (`MAX_SEGMENT_CHARS`).
    pub max_unit_chars: usize,
    /// Most lines in a text file: twenty per allowed unit, as LocOS's line budget.
    pub max_lines: usize,
    /// Deepest XML element nesting (libxml2's own default).
    pub max_xml_depth: usize,
    /// Most XML elements in one part.
    pub max_xml_elements: usize,
    /// Most entries in a zip (`ZIP_MAX_MEMBERS`).
    pub max_zip_entries: usize,
    /// Most bytes all entries of a zip unpack to (`ZIP_MAX_UNCOMPRESSED_BYTES`).
    pub max_zip_bytes: u64,
    /// Most bytes one zip entry may unpack to. LocOS has no cap of its own beyond the total, so the default is the
    /// same 256 MB; a caller may set it lower (or raise both for desktop use).
    pub max_zip_entry_bytes: u64,
    /// Largest unpacked/packed ratio of one entry (`ZIP_MAX_RATIO`).
    pub max_zip_ratio: u64,
    /// Entries smaller than this are not checked for ratio (`ZIP_RATIO_MIN_BYTES`).
    pub zip_ratio_floor_bytes: u64,
    /// Most paragraphs in a DOCX, empty ones included (`DOCX_MAX_PARAGRAPHS`).
    pub max_paragraphs: usize,
    /// Most cells in an XLSX (`XLSX_MAX_CELLS`).
    pub xlsx_max_cells: usize,
    /// Highest row number in an XLSX (`XLSX_MAX_ROWS`).
    pub xlsx_max_rows: u32,
    /// Most characters in one XLSX cell or shared string (`XLSX_MAX_CELL_CHARS`, Excel's own limit).
    pub xlsx_max_cell_chars: usize,
    /// Most characters of cell text in an XLSX, repeated shared strings counted every time
    /// (`XLSX_MAX_TOTAL_TEXT_CHARS`).
    pub xlsx_max_total_text_chars: usize,
    /// Most bytes of context the units of one document may carry together: keys, notes, headings, locations and
    /// the other strings of `UnitContext`. A string the file holds once and a parser copies into many units counts
    /// once per copy, so one long key or comment over many units cannot multiply the input.
    pub max_context_bytes: u64,
    /// Most bytes one merge may write (1 GB). Only a merge reads it: an ordinary file never comes near it, so it
    /// bounds what a stored Document that was changed by hand can make a merge produce.
    pub max_output_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_input_bytes: 100 * 1024 * 1024,
            max_units: 50_000,
            max_unit_chars: 100_000,
            max_lines: 20 * 50_000,
            max_xml_depth: 256,
            max_xml_elements: 4_000_000,
            max_zip_entries: 10_000,
            max_zip_bytes: 256 * 1024 * 1024,
            max_zip_entry_bytes: 256 * 1024 * 1024,
            max_zip_ratio: 200,
            zip_ratio_floor_bytes: 1024 * 1024,
            max_paragraphs: 50_000,
            xlsx_max_cells: 500_000,
            xlsx_max_rows: 100_000,
            xlsx_max_cell_chars: 32_767,
            xlsx_max_total_text_chars: 8 * 1024 * 1024,
            max_context_bytes: 128 * 1024 * 1024,
            max_output_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// The largest count or size the `unbounded` profile allows: far beyond any real file, and small enough that the
/// budgets formats derive from a limit (`40 * max_units`, `20 * max_units` lines) cannot overflow.
/// On a 32-bit target (WASM) it is about 16 million, so `200 * max_units` still fits.
const UNBOUNDED_COUNT: usize = if usize::BITS >= 64 { usize::MAX >> 12 } else { usize::MAX >> 8 };
const UNBOUNDED_BYTES: u64 = u64::MAX >> 12;

impl Limits {
    /// Localization OS's limits (the same as `Limits::default()`); change any of them with the `with_*` methods.
    /// These are the limits for a server that takes uploads from people it does not know (`hosted`).
    pub fn new() -> Limits {
        Limits::default()
    }

    /// The limits for a person's own files on their own machine (the CLI, the desktop app, a batch job): large
    /// enough for any ordinary document (a 4 GB input, 20 million units or paragraphs, every row Excel allows,
    /// 200 million cells), so big manuals, spreadsheets and pages are processed at the cost of memory. The
    /// structural guards stay: XML nesting depth, zip entry count, and a zip ratio check that still stops a
    /// zip bomb (1000:1 instead of 200:1).
    pub fn large() -> Limits {
        Limits {
            max_input_bytes: 4 << 30,
            max_units: 20_000_000,
            max_unit_chars: 10_000_000,
            max_lines: 400_000_000,
            max_xml_depth: 256,
            max_xml_elements: 400_000_000,
            max_zip_entries: 200_000,
            max_zip_bytes: 16 << 30,
            max_zip_entry_bytes: 8 << 30,
            max_zip_ratio: 1000,
            zip_ratio_floor_bytes: 1024 * 1024,
            max_paragraphs: 20_000_000,
            xlsx_max_cells: 200_000_000,
            xlsx_max_rows: 1_048_576,
            xlsx_max_cell_chars: 32_767,
            xlsx_max_total_text_chars: 2_000_000_000,
            max_context_bytes: 16 << 30,
            max_output_bytes: 32 << 30,
        }
    }

    /// No size or count limits at all: only what the machine's memory allows. The XML depth limit stays (it
    /// protects the stack, and no real document nests 256 elements deep). Use it only for trusted files.
    pub fn unbounded() -> Limits {
        Limits {
            max_input_bytes: UNBOUNDED_BYTES,
            max_units: UNBOUNDED_COUNT,
            max_unit_chars: UNBOUNDED_COUNT,
            max_lines: UNBOUNDED_COUNT,
            max_xml_depth: 256,
            max_xml_elements: UNBOUNDED_COUNT,
            max_zip_entries: UNBOUNDED_COUNT,
            max_zip_bytes: UNBOUNDED_BYTES,
            max_zip_entry_bytes: UNBOUNDED_BYTES,
            max_zip_ratio: UNBOUNDED_BYTES,
            zip_ratio_floor_bytes: 1024 * 1024,
            max_paragraphs: UNBOUNDED_COUNT,
            xlsx_max_cells: UNBOUNDED_COUNT,
            xlsx_max_rows: u32::MAX >> 4,
            xlsx_max_cell_chars: UNBOUNDED_COUNT,
            xlsx_max_total_text_chars: UNBOUNDED_COUNT,
            max_context_bytes: UNBOUNDED_BYTES,
            max_output_bytes: UNBOUNDED_BYTES,
        }
    }

    /// A profile by name: `hosted` (the default, [`Limits::new`]), `large` ([`Limits::large`]) or `unbounded`
    /// ([`Limits::unbounded`]; `none` is accepted too). `None` for any other name.
    pub fn profile(name: &str) -> Option<Limits> {
        match name.trim().to_ascii_lowercase().as_str() {
            "hosted" | "default" | "server" => Some(Limits::new()),
            "large" | "desktop" | "local" => Some(Limits::large()),
            "unbounded" | "none" => Some(Limits::unbounded()),
            _ => None,
        }
    }
    /// Sets `max_input_bytes`.
    pub fn with_max_input_bytes(mut self, v: u64) -> Limits {
        self.max_input_bytes = v;
        self
    }
    /// Sets `max_units`.
    pub fn with_max_units(mut self, v: usize) -> Limits {
        self.max_units = v;
        self
    }
    /// Sets `max_unit_chars`.
    pub fn with_max_unit_chars(mut self, v: usize) -> Limits {
        self.max_unit_chars = v;
        self
    }
    /// Sets `max_lines`.
    pub fn with_max_lines(mut self, v: usize) -> Limits {
        self.max_lines = v;
        self
    }
    /// Sets `max_xml_depth`.
    pub fn with_max_xml_depth(mut self, v: usize) -> Limits {
        self.max_xml_depth = v;
        self
    }
    /// Sets `max_xml_elements`.
    pub fn with_max_xml_elements(mut self, v: usize) -> Limits {
        self.max_xml_elements = v;
        self
    }
    /// Sets `max_zip_entries`.
    pub fn with_max_zip_entries(mut self, v: usize) -> Limits {
        self.max_zip_entries = v;
        self
    }
    /// Sets `max_zip_bytes`.
    pub fn with_max_zip_bytes(mut self, v: u64) -> Limits {
        self.max_zip_bytes = v;
        self
    }
    /// Sets `max_zip_entry_bytes`.
    pub fn with_max_zip_entry_bytes(mut self, v: u64) -> Limits {
        self.max_zip_entry_bytes = v;
        self
    }
    /// Sets `max_zip_ratio`.
    pub fn with_max_zip_ratio(mut self, v: u64) -> Limits {
        self.max_zip_ratio = v;
        self
    }
    /// Sets `zip_ratio_floor_bytes`.
    pub fn with_zip_ratio_floor_bytes(mut self, v: u64) -> Limits {
        self.zip_ratio_floor_bytes = v;
        self
    }
    /// Sets `max_paragraphs`.
    pub fn with_max_paragraphs(mut self, v: usize) -> Limits {
        self.max_paragraphs = v;
        self
    }
    /// Sets `xlsx_max_cells`.
    pub fn with_xlsx_max_cells(mut self, v: usize) -> Limits {
        self.xlsx_max_cells = v;
        self
    }
    /// Sets `xlsx_max_rows`.
    pub fn with_xlsx_max_rows(mut self, v: u32) -> Limits {
        self.xlsx_max_rows = v;
        self
    }
    /// Sets `xlsx_max_cell_chars`.
    pub fn with_xlsx_max_cell_chars(mut self, v: usize) -> Limits {
        self.xlsx_max_cell_chars = v;
        self
    }
    /// Sets `xlsx_max_total_text_chars`.
    pub fn with_xlsx_max_total_text_chars(mut self, v: usize) -> Limits {
        self.xlsx_max_total_text_chars = v;
        self
    }
    /// Sets `max_context_bytes`.
    pub fn with_max_context_bytes(mut self, v: u64) -> Limits {
        self.max_context_bytes = v;
        self
    }
    /// Sets `max_output_bytes`.
    pub fn with_max_output_bytes(mut self, v: u64) -> Limits {
        self.max_output_bytes = v;
        self
    }
}

/// A format crate's own cap under the caller's limit for the same thing. While the caller's number is at or below
/// the hosted one (`hosted`, the field of [`Limits::new`]) the cap is the smaller of the crate's own and the
/// caller's, so a server gets exactly the crate's numbers it always had; once the caller raised it above the
/// hosted number (`Limits::large`, `Limits::unbounded`, a desktop's own choice) the caller's number decides, so a
/// raised profile raises every format.
pub fn own_cap<T: Ord + Copy>(own: T, hosted: T, caller: T) -> T {
    if caller > hosted {
        caller
    } else {
        own.min(caller)
    }
}

/// [`own_cap`] for a byte cap kept as `usize` against a `u64` limit.
pub fn own_cap_bytes(own: usize, hosted: u64, caller: u64) -> usize {
    let c = usize::try_from(caller).unwrap_or(usize::MAX);
    if caller > hosted {
        c
    } else {
        own.min(c)
    }
}

fn mb(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{} MB", n / (1024 * 1024))
    } else {
        format!("{n} bytes")
    }
}

/// Refuses an input over `max_input_bytes`. Call before any other work on the bytes.
pub fn check_input(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    if bytes.len() as u64 > l.max_input_bytes {
        return Err(LimitError::new(
            LimitKind::InputBytes,
            format!(
                "This file is {} long; the limit is {}. Split it into smaller files.",
                mb(bytes.len() as u64),
                mb(l.max_input_bytes)
            ),
        )
        .into());
    }
    Ok(())
}

/// Refuses a text with more lines than the line budget, before any per-line work (a file of 100 MB of newlines
/// would otherwise build a list of 100 million lines).
pub fn check_lines(text: &str, l: &Limits) -> Result<(), Error> {
    let n = text.bytes().filter(|b| *b == b'\n' || *b == b'\r').count();
    if n > l.max_lines {
        return Err(LimitError::new(
            LimitKind::Lines,
            format!("This file has more than {} lines. Split it into smaller files.", l.max_lines),
        )
        .into());
    }
    Ok(())
}

/// The running-total check for a parser that counts units as it goes: refuse as soon as `count` passes the cap,
/// so a hostile file stops early.
pub fn check_unit_count(count: usize, l: &Limits) -> Result<(), Error> {
    if count > l.max_units {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("This file has more than {} segments. Split it into smaller files.", l.max_units),
        )
        .into());
    }
    Ok(())
}

/// Localization OS's pre-parse budget (#571 and its gate): a text with more than `per_unit` x `max_units` of some
/// marker (`count`: JSON quotes, or commas and brackets; PO backslashes) is refused before the parser builds
/// anything, since each marker costs work or memory whether or not it ends in a unit (100 MB of `{},` holds no
/// text but millions of values). `what` names the file and the marker ("This JSON file has far more values").
pub fn check_budget(count: usize, per_unit: usize, what: &str, l: &Limits) -> Result<(), Error> {
    if count > per_unit.saturating_mul(l.max_units) {
        return Err(LimitError::new(
            LimitKind::Units,
            format!("{what} than a file of {} segments. Split it into smaller files.", l.max_units),
        )
        .into());
    }
    Ok(())
}

/// Refuses a unit text longer than `max_unit_chars` characters.
pub fn check_unit_chars(chars: usize, l: &Limits) -> Result<(), Error> {
    if chars > l.max_unit_chars {
        return Err(LimitError::new(
            LimitKind::UnitChars,
            format!(
                "This file has a line or paragraph longer than {} characters. Break it into shorter lines.",
                l.max_unit_chars
            ),
        )
        .into());
    }
    Ok(())
}

/// Refuses a running total of context bytes over `max_context_bytes`.
pub fn check_context_bytes(total: u64, l: &Limits) -> Result<(), Error> {
    if total > l.max_context_bytes {
        return Err(LimitError::new(
            LimitKind::Context,
            format!(
                "This file carries more key, comment and note text than the limit of {} allows. Split it into smaller files.",
                mb(l.max_context_bytes)
            ),
        )
        .into());
    }
    Ok(())
}

/// The bytes of context one unit carries: every string of its `UnitContext`.
pub fn unit_context_bytes(c: &crate::UnitContext) -> u64 {
    let opt = |s: &Option<String>| s.as_ref().map_or(0, |v| v.len() as u64);
    opt(&c.kind)
        + opt(&c.style)
        + opt(&c.key)
        + opt(&c.heading)
        + opt(&c.location)
        + c.notes.iter().map(|n| n.len() as u64).sum::<u64>()
        + c.extra.iter().map(|(k, v)| (k.len() + v.len()) as u64).sum::<u64>()
}

/// The running context budget for a parser that copies a shared string (a key path, a section name, a comment)
/// into many units: charge every copy before it is made, so a hostile file is refused before the memory is
/// taken. `check_document` applies the same cap to the finished document as the safety net.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextBudget {
    used: u64,
}

impl ContextBudget {
    /// A budget with nothing charged.
    pub fn new() -> ContextBudget {
        ContextBudget::default()
    }
    /// Charges `bytes` more of context and refuses once the total passes `max_context_bytes`.
    pub fn charge(&mut self, bytes: usize, l: &Limits) -> Result<(), Error> {
        self.used = self.used.saturating_add(bytes as u64);
        check_context_bytes(self.used, l)
    }
    /// The bytes charged so far.
    pub fn used(&self) -> u64 {
        self.used
    }
}

/// The check for a finished extraction: unit and segment counts, the length of every unit, and the context
/// bytes of all units together.
pub fn check_document(doc: &Document, l: &Limits) -> Result<(), Error> {
    check_unit_count(doc.units.len(), l)?;
    check_unit_count(doc.segment_count(), l)?;
    let mut context: u64 = 0;
    for u in &doc.units {
        context = context.saturating_add(unit_context_bytes(&u.context));
        check_context_bytes(context, l)?;
        let chars: usize = u
            .segments
            .iter()
            .flat_map(|s| s.source.iter())
            .map(|i| match i {
                crate::Inline::Text(t) => t.chars().count(),
                crate::Inline::Tag { .. } => 1,
            })
            .sum();
        check_unit_chars(chars, l)?;
    }
    Ok(())
}

/// How often a skeleton may refer to units: in all, per allowed unit (`max_units`), and to any one unit.
/// Extraction writes each unit into the skeleton once or a few times; a stored Document that refers to one unit
/// many more times would make a merge write that unit over and over (a small Document, a large output).
pub const MERGE_REFS_PER_UNIT: usize = 4;

/// How long a stored target may be, as a multiple of `max_unit_chars` (a source over that one is refused at
/// extract; a translation may be longer than its source, but not without bound).
pub const MERGE_TARGET_CHARS_FACTOR: usize = 4;

/// The checks a merge runs on a stored Document before writing it (BILI-105). A Document is the caller's data, and
/// a host should keep a stored one where only it can write it, but it is plain JSON between extract and merge, so
/// these bound what one that was edited by hand can make a merge do, under the caller's merge limits
/// (`MergeOptions::limits`):
/// * the checks of an extraction ([`check_document`]): units and segments within `max_units`, every unit's source
///   within `max_unit_chars`, the context within `max_context_bytes`; and every unit's target within
///   [`MERGE_TARGET_CHARS_FACTOR`] x `max_unit_chars`;
/// * no more zip entries than `max_zip_entries`, and every zip entry name one the zip reader gives
///   ([`check_zip_name`]), none listed twice (case and `\` against `/` ignored, as the reader compares them);
/// * at most [`MERGE_REFS_PER_UNIT`] x `max_units` unit slots in all, at most [`MERGE_REFS_PER_UNIT`] to any one
///   unit, none to a unit the document does not have, and `max_lines` + the slot cap skeleton parts
///   ([`LimitKind::Skeleton`]);
/// * an estimate of the output (the skeleton's own bytes, each unit slot counted at its unit's size, and the units
///   once more for formats that rebuild from them) within `max_output_bytes`.
///
/// An ordinary extraction is far inside every one of these.
pub fn check_merge(doc: &Document, l: &Limits) -> Result<(), Error> {
    check_document(doc, l)?;
    check_target_chars(doc, l)?;
    let max_refs = MERGE_REFS_PER_UNIT.saturating_mul(l.max_units);
    let max_parts = l.max_lines.saturating_add(max_refs);
    let sizes: Vec<u64> = doc.units.iter().map(unit_output_bytes).collect();
    let mut walk = SkeletonWalk { parts: 0, refs: 0, per_unit: vec![0; sizes.len()], estimate: sizes.iter().fold(0u64, |a, b| a.saturating_add(*b)) };
    check_output_estimate(walk.estimate, l)?;
    match &doc.skeleton {
        crate::Skeleton::Text { parts, .. } => walk.parts(parts, &sizes, max_parts, max_refs, l)?,
        crate::Skeleton::Zip { entries, .. } => {
            if entries.len() > l.max_zip_entries {
                return Err(LimitError::new(
                    LimitKind::ZipEntries,
                    format!("This document lists more than {} zip entries. It was not written.", l.max_zip_entries),
                )
                .into());
            }
            let mut seen = std::collections::HashSet::with_capacity(entries.len());
            for e in entries {
                // a format's own records (`\0bilingua-docx-meta`, ...) are never written as entries: the zip
                // writers refuse a name with a control character, so one changed by hand cannot reach the output
                if e.name.starts_with(INTERNAL_ENTRY_PREFIX) {
                    continue;
                }
                check_zip_name(&e.name)?;
                if !seen.insert(e.name.to_ascii_lowercase()) {
                    return Err(zip_name_error(&e.name, "is listed twice"));
                }
            }
            for e in entries {
                match &e.data {
                    crate::ZipData::Raw(b) => {
                        walk.estimate = walk.estimate.saturating_add(b.len() as u64);
                        check_output_estimate(walk.estimate, l)?;
                    }
                    crate::ZipData::Parts(parts) => walk.parts(parts, &sizes, max_parts, max_refs, l)?,
                }
            }
        }
        crate::Skeleton::Binary { data, .. } => {
            walk.estimate = walk.estimate.saturating_add(data.len() as u64);
            check_output_estimate(walk.estimate, l)?;
        }
    }
    Ok(())
}

/// Refuses a stored target longer than [`MERGE_TARGET_CHARS_FACTOR`] x `max_unit_chars` (codes count one each,
/// as in [`check_document`]).
fn check_target_chars(doc: &Document, l: &Limits) -> Result<(), Error> {
    let max = l.max_unit_chars.saturating_mul(MERGE_TARGET_CHARS_FACTOR);
    for u in &doc.units {
        let chars: usize = u
            .segments
            .iter()
            .flat_map(|s| s.target.iter())
            .map(|i| match i {
                crate::Inline::Text(t) => t.chars().count(),
                crate::Inline::Tag { .. } => 1,
            })
            .fold(0usize, usize::saturating_add);
        if chars > max {
            return Err(LimitError::new(
                LimitKind::UnitChars,
                format!("This document has a translation longer than {max} characters. It was not written."),
            )
            .into());
        }
    }
    Ok(())
}

/// The prefix of the zip skeleton entries a format keeps for itself (its metadata, the original archive), which no
/// writer puts in the output archive.
pub const INTERNAL_ENTRY_PREFIX: &str = "\0bilingua-";

fn zip_name_error(raw: &str, what: &str) -> Error {
    // the name is the caller's own data; shown escaped and cut short
    let shown: String = raw.chars().take(80).flat_map(char::escape_debug).collect();
    LimitError::new(LimitKind::ZipMalformed, format!("This document has a zip entry name that {what} (\"{shown}\"). It was not written.")).into()
}

/// Refuses a zip entry name the zip reader never gives (BILI-105): a stored Document's names are written into the
/// output archive, so one that was changed by hand must not reach it. A name the reader gives is relative, uses
/// `/`, and has no empty, `.` or `..` component, no component made only of dots and spaces, no colon and no
/// control character; a folder ends in `/`. Duplicates are checked over the whole list by [`check_merge`].
pub fn check_zip_name(raw: &str) -> Result<(), Error> {
    if raw.is_empty() {
        return Err(zip_name_error(raw, "is empty"));
    }
    if raw.chars().any(char::is_control) {
        return Err(zip_name_error(raw, "holds a control character"));
    }
    if raw.starts_with('/') || raw.contains('\\') {
        return Err(zip_name_error(raw, "is an absolute path or uses a backslash"));
    }
    if raw.contains(':') {
        return Err(zip_name_error(raw, "contains a colon"));
    }
    let body = raw.strip_suffix('/').unwrap_or(raw);
    for part in body.split('/') {
        if part == ".." {
            return Err(zip_name_error(raw, "climbs out of the archive (..)"));
        }
        if part.is_empty() || part.trim_end_matches(['.', ' ']).is_empty() {
            return Err(zip_name_error(raw, "has an empty component or one made of dots or spaces"));
        }
    }
    Ok(())
}

/// The running totals of [`check_merge`] over every part list of a skeleton.
struct SkeletonWalk {
    parts: usize,
    refs: usize,
    /// References so far to each unit.
    per_unit: Vec<usize>,
    estimate: u64,
}

impl SkeletonWalk {
    fn parts(&mut self, parts: &[crate::SkelPart], sizes: &[u64], max_parts: usize, max_refs: usize, l: &Limits) -> Result<(), Error> {
        self.parts = self.parts.saturating_add(parts.len());
        if self.parts > max_parts {
            return Err(over_skeleton(l));
        }
        for p in parts {
            match p {
                crate::SkelPart::Raw(s) => self.estimate = self.estimate.saturating_add(s.len() as u64),
                crate::SkelPart::Unit(i) => {
                    self.refs += 1;
                    if self.refs > max_refs {
                        return Err(over_skeleton(l));
                    }
                    let Some(n) = self.per_unit.get_mut(*i) else {
                        return Err(LimitError::new(
                            LimitKind::Skeleton,
                            format!("This document's skeleton refers to unit {i}, which the document does not have. It was not written."),
                        )
                        .into());
                    };
                    *n += 1;
                    if *n > MERGE_REFS_PER_UNIT {
                        return Err(LimitError::new(
                            LimitKind::Skeleton,
                            format!(
                                "This document's skeleton writes one unit more than {MERGE_REFS_PER_UNIT} times, more than any extraction makes it. It was not written."
                            ),
                        )
                        .into());
                    }
                    self.estimate = self.estimate.saturating_add(sizes[*i]);
                }
            }
        }
        check_output_estimate(self.estimate, l)
    }
}

/// Refuses merge output over `max_output_bytes` (the facade calls it on what the writer returned).
pub fn check_output(len: usize, l: &Limits) -> Result<(), Error> {
    check_output_estimate(len as u64, l)
}

fn check_output_estimate(n: u64, l: &Limits) -> Result<(), Error> {
    if n > l.max_output_bytes {
        return Err(LimitError::new(
            LimitKind::OutputBytes,
            format!("This document would write more than {}, the output limit. It was not written.", mb(l.max_output_bytes)),
        )
        .into());
    }
    Ok(())
}

fn over_skeleton(l: &Limits) -> Error {
    LimitError::new(
        LimitKind::Skeleton,
        format!(
            "This document's skeleton is larger, or refers to its units more often, than a file of {} segments. It was not written.",
            l.max_units
        ),
    )
    .into()
}

/// What writing one unit costs at least: per segment the larger of its source and target, each code counted at
/// its stored markup.
fn unit_output_bytes(u: &crate::Unit) -> u64 {
    let inlines = |v: &[crate::Inline]| -> u64 {
        v.iter()
            .map(|i| match i {
                crate::Inline::Text(t) => t.len() as u64,
                crate::Inline::Tag { id, kind, display, .. } => {
                    let code = u.codes.get(id).and_then(|c| match kind {
                        crate::TagKind::Open => c.open.as_ref(),
                        crate::TagKind::Close => c.close.as_ref(),
                        crate::TagKind::Standalone => c.standalone.as_ref(),
                    });
                    code.map_or(display.len(), String::len) as u64
                }
            })
            .fold(0u64, u64::saturating_add)
    };
    u.segments.iter().map(|s| inlines(&s.source).max(inlines(&s.target))).fold(0u64, u64::saturating_add)
}

/// Checks every stored target's tags as a merge reads them (BILI-105): a tag must be one of its segment's source
/// tags (same id and kind) or have its markup in the unit's `codes`. With `counts`, a paired tag may also appear no more often than in the source, the
/// rule of [`Document::set_target_repeating`](crate::Document::set_target_repeating); without it repeats pass.
pub fn check_targets(doc: &Document, counts: bool) -> Result<(), Error> {
    for u in &doc.units {
        for s in &u.segments {
            if !s.target.is_empty() {
                crate::check_target_tags(&s.source, &s.target, &u.codes, counts)?;
            }
        }
    }
    Ok(())
}

/// What every parser ends with: the document passes `check_document`, and the warnings of the text decode go on it.
pub fn finish(mut doc: Document, warnings: Vec<crate::Warning>, l: &Limits) -> Result<Document, Error> {
    check_document(&doc, l)?;
    doc.warnings.extend(warnings);
    Ok(doc)
}

fn unsafe_xml(msg: impl Into<String>) -> Error {
    LimitError::new(LimitKind::XmlEntities, msg).into()
}

fn find(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

/// Reads XML as bytes, transcoding UTF-16 (BOM or `<` in 16-bit form) so the scan sees ASCII markup.
fn markup_bytes(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    let le = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[b'<', 0]);
    let be = bytes.starts_with(&[0xFE, 0xFF]) || bytes.starts_with(&[0, b'<']);
    if le && !bytes.starts_with(&[0xFF, 0xFE, 0, 0]) {
        let (s, _) = encoding_rs::UTF_16LE.decode_without_bom_handling(bytes);
        return std::borrow::Cow::Owned(s.into_owned().into_bytes());
    }
    if be {
        let (s, _) = encoding_rs::UTF_16BE.decode_without_bom_handling(bytes);
        return std::borrow::Cow::Owned(s.into_owned().into_bytes());
    }
    std::borrow::Cow::Borrowed(bytes)
}

/// The XML guard: scans the markup of one XML part once and refuses
/// * any `<!ENTITY` declaration (an entity bomb or an external entity; `&amp; &lt; &gt; &quot; &apos;` and
///   character references are all a document needs),
/// * nesting deeper than `max_xml_depth`,
/// * more than `max_xml_elements` elements.
///
/// Comments, CDATA sections and processing instructions are skipped, so markup inside them is not counted.
///
/// The count is only as good as the reader's agreement on where those regions end, so a part is also refused when
/// the readers would not agree: a DOCTYPE that never ends, has a `<`, `>` or bracket in a quoted string, a bracket
/// or angle bracket in a comment of its internal subset (a quote there is fine, but then no `<!ENTITY` may follow
/// the DOCTYPE anywhere), a quote in a processing instruction of the subset, or a `<`/`>` that `quick-xml` pairs
/// differently; a comment that
/// starts with `>` or `->`; `<?>`; a `<!` declaration outside the DOCTYPE. Every reading a reader may give the
/// bytes is scanned: UTF-16 and UTF-8 for `<\0` without a byte order mark, and the double-byte code pages for
/// bytes that are not UTF-8.
pub fn check_xml(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    check_xml_with(bytes, l, false, false)
}

/// Like [`check_xml`], but a DOCTYPE may declare plain internal entities: `<!ENTITY name "literal">` with a literal
/// that holds no `&` (so no entity expands into another), at most 1024 bytes long, and at most 64 of them. Adobe Illustrator, Inkscape and
/// Sodipodi SVGs declare a few of these for namespace URIs. External (`SYSTEM`, `PUBLIC`) and parameter (`%`)
/// entities are refused as before, and every other check of [`check_xml`] still applies. For a reader that never
/// expands an entity (the SVG filter keeps every `&name;` as typed).
pub fn check_xml_internal_entities(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    check_xml_with(bytes, l, false, true)
}

/// True when every `<!ENTITY` declaration in a DOCTYPE is `<!ENTITY name "literal">` (or single quotes) and the
/// literal has no `&`; false for an external or parameter entity, a malformed one, or more than 64.
fn only_plain_internal_entities(doctype: &[u8]) -> bool {
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(p) = doctype[from..].windows(8).position(|w| w.eq_ignore_ascii_case(b"<!ENTITY")) {
        n += 1;
        if n > 64 {
            return false;
        }
        let mut j = from + p + 8;
        let ws = |j: &mut usize| {
            let s = *j;
            while *j < doctype.len() && matches!(doctype[*j], b' ' | b'\t' | b'\r' | b'\n') {
                *j += 1;
            }
            *j > s
        };
        if !ws(&mut j) {
            return false;
        }
        let name_start = j;
        while j < doctype.len() && (doctype[j].is_ascii_alphanumeric() || matches!(doctype[j], b'_' | b'-' | b'.' | b':')) {
            j += 1;
        }
        if j == name_start || !ws(&mut j) || j >= doctype.len() {
            return false;
        }
        let q = doctype[j];
        if q != b'"' && q != b'\'' {
            return false;
        }
        let Some(len) = doctype[j + 1..].iter().position(|c| *c == q) else { return false };
        // no `&`: nothing expands into anything; at most 1024 bytes: a few references cannot blow up a reader
        // that does expand them (the file is written back as it is, for browsers and editors to read)
        if len > 1024 || doctype[j + 1..j + 1 + len].contains(&b'&') {
            return false;
        }
        from = j + 1 + len + 1;
    }
    true
}

/// Like [`check_xml`] for document formats where a DTD with entity declarations is normal (DocBook, DITA, plain
/// XML): the declarations are allowed, and the depth and element limits still apply. This is safe only for a reader
/// that never expands or fetches an entity, as the XML filter family does: it keeps every `&name;` as an opaque
/// code and writes the DOCTYPE back byte for byte.
pub fn check_xml_keeping_dtd(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    check_xml_with(bytes, l, true, false)
}

/// End of a `<!DOCTYPE ...>` that starts at `i`: the index of its closing `>` (or `b.len()`). Quoted strings, the
/// `[ ... ]` internal subset and the comments and processing instructions inside it are skipped.
fn doctype_end(b: &[u8], i: usize) -> usize {
    let (mut j, mut sq, mut subset) = (i + 9, 0u8, 0usize);
    while j < b.len() {
        let c = b[j];
        if sq != 0 {
            if c == sq {
                sq = 0;
            }
        } else if c == b'"' || c == b'\'' {
            sq = c;
        } else if subset > 0 && b[j..].starts_with(b"<!--") {
            match find(b, j + 4, b"-->") {
                Some(e) => j = e + 2,
                None => return b.len(),
            }
        } else if subset > 0 && b[j..].starts_with(b"<?") {
            match find(b, j + 2, b"?>") {
                Some(e) => j = e + 1,
                None => return b.len(),
            }
        } else if c == b'[' {
            subset += 1;
        } else if c == b']' {
            subset = subset.saturating_sub(1);
        } else if c == b'>' && subset == 0 {
            break;
        }
        j += 1;
    }
    j.min(b.len())
}

/// Where `quick-xml` ends the `<!DOCTYPE` that starts at `i`: the first `>` outside a `<` ... `>` pair, quotes,
/// brackets and comments not looked at (the index of that `>`, or `None` when there is none).
fn reader_doctype_end(b: &[u8], i: usize) -> Option<usize> {
    let mut balance = 0usize;
    for (j, c) in b.iter().enumerate().skip(i + 2) {
        match c {
            b'<' => balance += 1,
            b'>' if balance == 0 => return Some(j),
            b'>' => balance -= 1,
            _ => {}
        }
    }
    None
}

/// `Some` when every XML reader in the workspace ends the DOCTYPE at `i..=end` (`end` from [`doctype_end`]) at
/// the same `>`. The readers differ in what they skip: `quick-xml` only pairs `<` with `>` (and skips the comments
/// of the subset whole); the hand-written scanners skip quoted strings, some also the `[ ... ]` subset, some the
/// comments or processing instructions inside it. They all agree when no quoted string holds a `<`, `>`, `[` or
/// `]`, no comment or processing instruction of the subset holds a bracket or an angle bracket of its own, no
/// processing instruction a quote, and `quick-xml` stops at the same place. Real files meet that easily
/// (`<!DOCTYPE plist PUBLIC "..." "...">`, an XMB subset of `<!ELEMENT>` and `<!ATTLIST>` declarations).
///
/// A quote in a comment of the subset (`<!-- don't edit -->`, BILI-60) ends the DOCTYPE in the same place for
/// every reader that skips the comment; `Some(true)` says the DOCTYPE has one, so the caller can make sure a
/// scanner that pairs that quote instead could not find an entity declaration in what follows.
fn doctype_is_unambiguous(b: &[u8], i: usize, end: usize) -> Option<bool> {
    if end >= b.len() || reader_doctype_end(b, i) != Some(end) {
        return None;
    }
    let mut quoted_comment = false;
    let special = |s: &[u8], also_quotes: bool| s.iter().any(|c| matches!(c, b'<' | b'>' | b'[' | b']') || (also_quotes && matches!(c, b'"' | b'\'')));
    let (mut j, mut subset) = (i + 9, 0usize);
    while j < end {
        let c = b[j];
        if c == b'"' || c == b'\'' {
            let q = b[j + 1..end].iter().position(|x| *x == c)?;
            if special(&b[j + 1..j + 1 + q], false) {
                return None;
            }
            j += q + 2;
            continue;
        }
        if subset > 0 && b[j..].starts_with(b"<!--") {
            let e = find(b, j + 4, b"-->").filter(|e| *e < end)?;
            let body = &b[j + 4..e];
            if special(body, false) {
                return None;
            }
            quoted_comment |= body.iter().any(|c| matches!(c, b'"' | b'\''));
            j = e + 3;
            continue;
        }
        if subset > 0 && b[j..].starts_with(b"<?") {
            let e = find(b, j + 2, b"?>").filter(|e| *e < end)?;
            if special(&b[j + 2..e], true) {
                return None;
            }
            j = e + 2;
            continue;
        }
        if c == b'[' {
            subset += 1;
        } else if c == b']' {
            subset = subset.saturating_sub(1);
        }
        j += 1;
    }
    Some(quoted_comment)
}

fn ambiguous_markup(what: &str) -> Error {
    LimitError::new(
        LimitKind::XmlElements,
        format!("This file has {what}, which XML readers do not agree on, so its elements could not be counted safely. It was not processed."),
    )
    .into()
}

/// Every way a reader in the workspace may read `bytes` as markup. A UTF-16 byte order mark leaves one reading.
/// Without one, `<\0` or `\0<` is UTF-16 for the readers that sniff it (EPUB, IDML) but UTF-8 with a NUL for the
/// ones that decode with [`crate::enc::decode_text`], so both are scanned. Bytes that are not UTF-8 may be read
/// in a double-byte code page whose second byte can be `[` or `]` (Shift_JIS, GBK/GB18030, Big5: a CDATA section
/// then ends at another `]]>` than in the bytes), so each of those readings is scanned too.
fn markup_views(bytes: &[u8]) -> Vec<std::borrow::Cow<'_, [u8]>> {
    use std::borrow::Cow;
    let decode = |e: &'static encoding_rs::Encoding| Cow::Owned(e.decode_without_bom_handling(bytes).0.into_owned().into_bytes());
    if (bytes.starts_with(&[0xFF, 0xFE]) && !bytes.starts_with(&[0xFF, 0xFE, 0, 0])) || bytes.starts_with(&[0xFE, 0xFF]) {
        return vec![markup_bytes(bytes)];
    }
    let mut v = vec![Cow::Borrowed(bytes)];
    if bytes.starts_with(&[b'<', 0]) || bytes.starts_with(&[0, b'<']) {
        v.push(markup_bytes(bytes));
    } else if std::str::from_utf8(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).is_err() {
        for e in [encoding_rs::SHIFT_JIS, encoding_rs::GB18030, encoding_rs::BIG5] {
            v.push(decode(e));
        }
    }
    v
}

fn check_xml_with(bytes: &[u8], l: &Limits, keep_dtd: bool, internal_ok: bool) -> Result<(), Error> {
    for view in markup_views(bytes) {
        scan_xml(&view, l, keep_dtd, internal_ok)?;
    }
    Ok(())
}

fn scan_xml(b: &[u8], l: &Limits, keep_dtd: bool, internal_ok: bool) -> Result<(), Error> {
    let strict = !keep_dtd;
    let (mut i, mut depth, mut elements) = (0usize, 0usize, 0usize);
    while i < b.len() {
        let Some(p) = b[i..].iter().position(|c| *c == b'<') else { break };
        i += p;
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
            // `<!-->` and `<!--->` open a comment for XML, but a scanner that looks for `-->` from the `<` reads them
            // as a whole comment and goes on with what XML still calls comment text
            if strict && (rest[4..].starts_with(b">") || rest[4..].starts_with(b"->")) {
                return Err(ambiguous_markup("a comment that starts with '>' or '->'"));
            }
            match find(b, i + 4, b"-->") {
                Some(e) => i = e + 3,
                None => break,
            }
        } else if rest.starts_with(b"<![CDATA[") {
            match find(b, i + 9, b"]]>") {
                Some(e) => i = e + 3,
                None => break,
            }
        } else if rest.starts_with(b"<?") {
            // `<?>` is no processing instruction; a scanner that looks for `?>` from the `<` reads it as one
            if strict && rest[2..].starts_with(b">") {
                return Err(ambiguous_markup("a processing instruction with no name ('<?>')"));
            }
            match find(b, i + 2, b"?>") {
                Some(e) => i = e + 2,
                None => break,
            }
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            let end = doctype_end(b, i);
            if !keep_dtd && contains_ci(&b[i..end], b"<!ENTITY") && !(internal_ok && only_plain_internal_entities(&b[i..end])) {
                return Err(unsafe_xml(
                    "This file declares XML entities (a DTD), which could expand without limit or read other files. It was not processed.",
                ));
            }
            // the readers must all end the DOCTYPE where this scan does, or elements (or an ENTITY) could hide in
            // what one of them calls the DOCTYPE. The keep-DTD readers (the XML filter family) check that themselves.
            if strict && end >= b.len() {
                return Err(unsafe_xml("This file has a DOCTYPE that is not closed. It was not processed."));
            }
            if strict {
                match doctype_is_unambiguous(b, i, end) {
                    None => {
                        return Err(unsafe_xml(
                            "This file has a DOCTYPE with a quoted '<', '>' or bracket, or a comment in its DTD with brackets, which XML readers do not agree on. It was not processed.",
                        ))
                    }
                    // a scanner that pairs the comment's quote with a later one would still be in the subset
                    // after the DOCTYPE: no `<!ENTITY` may follow anywhere, even in an attribute, comment or CDATA
                    Some(true) if contains_ci(&b[end..], b"<!ENTITY") => {
                        return Err(unsafe_xml(
                            "This file declares XML entities, which are not allowed. It was not processed.",
                        ))
                    }
                    Some(_) => {}
                }
            }
            i = end + 1;
        } else if rest.starts_with(b"<!") {
            // another declaration outside a DOCTYPE
            if rest.len() >= 8 && rest[..8].eq_ignore_ascii_case(b"<!ENTITY") {
                return Err(unsafe_xml("This file declares XML entities, which are not allowed. It was not processed."));
            }
            // `quick-xml` reads `<![` to `]]>` and `<!-` to `-->`; XML has no other declaration outside a DOCTYPE
            if strict {
                return Err(ambiguous_markup("a markup declaration outside the DOCTYPE"));
            }
            match find(b, i + 2, b">") {
                Some(e) => i = e + 1,
                None => break,
            }
        } else if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            match find(b, i + 2, b">") {
                Some(e) => i = e + 1,
                None => break,
            }
        } else {
            // a start tag: find '>' outside quotes
            let (mut j, mut sq) = (i + 1, 0u8);
            let mut end = None;
            while j < b.len() {
                let c = b[j];
                if sq != 0 {
                    if c == sq {
                        sq = 0;
                    }
                } else if c == b'"' || c == b'\'' {
                    sq = c;
                } else if c == b'>' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(e) = end else { break };
            elements += 1;
            if elements > l.max_xml_elements {
                return Err(LimitError::new(
                    LimitKind::XmlElements,
                    format!("This file has more than {} XML elements in one part. It was not processed.", l.max_xml_elements),
                )
                .into());
            }
            if b[e - 1] != b'/' {
                depth += 1;
                if depth > l.max_xml_depth {
                    return Err(LimitError::new(
                        LimitKind::XmlDepth,
                        format!("This file nests XML elements more than {} deep. It was not processed.", l.max_xml_depth),
                    )
                    .into());
                }
            }
            i = e + 1;
        }
    }
    Ok(())
}

/// Counts the start tags whose local name is `local` (`p` for a Word paragraph), comments, CDATA and processing
/// instructions skipped. Cheap enough to run over every part before a parser builds a tree of it.
pub fn count_elements(bytes: &[u8], local: &str) -> usize {
    let data = markup_bytes(bytes);
    let b: &[u8] = &data;
    let (mut i, mut n) = (0usize, 0usize);
    while let Some(p) = b.get(i..).and_then(|r| r.iter().position(|c| *c == b'<')) {
        i += p;
        let rest = &b[i + 1..];
        if rest.starts_with(b"!--") {
            i = find(b, i + 4, b"-->").map_or(b.len(), |e| e + 3);
        } else if rest.starts_with(b"![CDATA[") {
            i = find(b, i + 9, b"]]>").map_or(b.len(), |e| e + 3);
        } else if rest.starts_with(b"?") {
            i = find(b, i + 2, b"?>").map_or(b.len(), |e| e + 2);
        } else {
            let name_end = rest.iter().position(|c| c.is_ascii_whitespace() || *c == b'>' || *c == b'/').unwrap_or(rest.len());
            let name = &rest[..name_end];
            let lname = name.rsplit(|c| *c == b':').next().unwrap_or(name);
            if !name.starts_with(b"/") && !name.starts_with(b"!") && lname == local.as_bytes() {
                n += 1;
            }
            i += 1 + name_end;
        }
    }
    n
}

/// The guard for markup that is not strict XML (HTML): no entity declarations, and the element count cap. Depth
/// of HTML is the parser's own business (unclosed tags make it unknowable from the text).
pub fn check_markup(text: &str, l: &Limits) -> Result<(), Error> {
    let b = text.as_bytes();
    if contains_ci(b, b"<!ENTITY") {
        return Err(unsafe_xml("This file declares markup entities, which are not allowed. It was not processed."));
    }
    let elements = b.windows(2).filter(|w| w[0] == b'<' && (w[1].is_ascii_alphabetic() || w[1] == b'/')).count();
    if elements > l.max_xml_elements {
        return Err(LimitError::new(
            LimitKind::XmlElements,
            format!("This file has more than {} markup elements. It was not processed.", l.max_xml_elements),
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(r: Result<(), Error>) -> Option<LimitKind> {
        match r {
            Err(Error::Limit(e)) => Some(e.kind),
            _ => None,
        }
    }

    #[test]
    fn defaults_are_locos_numbers() {
        let l = Limits::default();
        assert_eq!((l.max_input_bytes, l.max_units, l.max_unit_chars), (100 << 20, 50_000, 100_000));
        assert_eq!((l.max_zip_entries, l.max_zip_bytes, l.max_zip_ratio), (10_000, 256 << 20, 200));
    }

    #[test]
    fn pre_parse_budget_scales_with_the_unit_limit() {
        let l = Limits { max_units: 10, ..Limits::default() };
        assert!(check_budget(200, 20, "This JSON file has far more values", &l).is_ok());
        let e = check_budget(201, 20, "This JSON file has far more values", &l).unwrap_err();
        assert!(e.to_string().contains("far more values than a file of 10 segments. Split"), "{e}");
        assert_eq!(kind(Err(e)), Some(LimitKind::Units));
        assert!(check_budget(usize::MAX, usize::MAX, "x", &Limits { max_units: usize::MAX, ..Limits::default() }).is_ok());
    }

    #[test]
    fn input_size_is_checked_against_the_callers_limit() {
        let l = Limits { max_input_bytes: 10, ..Limits::default() };
        assert!(check_input(&[0; 10], &l).is_ok());
        assert_eq!(kind(check_input(&[0; 11], &l)), Some(LimitKind::InputBytes));
    }

    #[test]
    fn xml_entity_bomb_and_xxe_are_refused() {
        let bomb = br#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;">]><lolz>&lol2;</lolz>"#;
        assert_eq!(kind(check_xml(bomb, &Limits::default())), Some(LimitKind::XmlEntities));
        let xxe = br#"<!DOCTYPE r [ <!ENTITY x SYSTEM "file:///etc/passwd"> ]><r>&x;</r>"#;
        assert_eq!(kind(check_xml(xxe, &Limits::default())), Some(LimitKind::XmlEntities));
        let param = br#"<!doctype r [ <!entity % p SYSTEM "http://example.invalid/x.dtd"> %p; ]><r/>"#;
        assert_eq!(kind(check_xml(param, &Limits::default())), Some(LimitKind::XmlEntities));
    }

    #[test]
    fn internal_entities_mode_allows_only_plain_literals() {
        let l = Limits::default();
        let ok = br#"<?xml version="1.0"?><!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [ <!ENTITY ns_a "http://ns.example.invalid/a/1.0/"> <!ENTITY b 'x'> ]><svg xmlns:a="&ns_a;"/>"#;
        assert!(check_xml_internal_entities(ok, &l).is_ok());
        assert_eq!(kind(check_xml(ok, &l)), Some(LimitKind::XmlEntities), "the strict check still refuses it");
        for bad in [
            r#"<!DOCTYPE r [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;">]><r/>"#,
            r#"<!DOCTYPE r [<!ENTITY x SYSTEM "file:///etc/hosts">]><r/>"#,
            r#"<!DOCTYPE r [<!ENTITY x PUBLIC "a" "b">]><r/>"#,
            r#"<!DOCTYPE r [<!ENTITY % p "x">]><r/>"#,
            r#"<!DOCTYPE r [<!ENTITY x>]><r/>"#,
            r#"<!DOCTYPE r [<!ENTITY x "unclosed>]><r/>"#,
            r#"<!ENTITY x "y"><r/>"#,
        ] {
            assert_eq!(kind(check_xml_internal_entities(bad.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{bad}");
        }
        let long = format!("<!DOCTYPE r [<!ENTITY x \"{}\">]><r/>", "a".repeat(1025));
        assert_eq!(kind(check_xml_internal_entities(long.as_bytes(), &l)), Some(LimitKind::XmlEntities));
        // a literal never carries markup: a quoted '<' or '>' in the DOCTYPE is refused
        for bad in [r#"<!DOCTYPE r [<!ENTITY x "<g/>">]><r/>"#, r#"<!DOCTYPE r [<!ENTITY x "a>b">]><r/>"#] {
            assert!(check_xml_internal_entities(bad.as_bytes(), &l).is_err(), "{bad}");
        }
        // declared entities do not lift the depth and element limits
        let tight = Limits { max_xml_depth: 2, max_xml_elements: 3, ..Limits::default() };
        let deep = r#"<!DOCTYPE r [<!ENTITY x "y">]><r><a><b>&x;</b></a></r>"#;
        assert!(check_xml_internal_entities(deep.as_bytes(), &tight).is_err());
        let many: String = (0..65).map(|k| format!("<!ENTITY e{k} \"v\">")).collect();
        assert!(check_xml_internal_entities(format!("<!DOCTYPE r [{many}]><r/>").as_bytes(), &l).is_err());
    }

    #[test]
    fn keeping_dtd_allows_declarations_but_not_more_elements_or_depth() {
        let l = Limits { max_xml_depth: 8, max_xml_elements: 20, ..Limits::default() };
        let bomb = br#"<!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;"><!ENTITY % p SYSTEM "http://example.invalid/x.dtd"> %p;]><lolz>&lol2;</lolz>"#;
        assert!(check_xml_keeping_dtd(bomb, &l).is_ok());
        assert_eq!(kind(check_xml(bomb, &l)), Some(LimitKind::XmlEntities), "the strict check still refuses it");
        // a quote, a '>' or a ']' in a comment or a PI of the subset does not end the DOCTYPE early or late
        for dt in [r#"<!DOCTYPE r [ <!-- ' ]> --> <!ENTITY a "x"> ]>"#, r#"<!DOCTYPE r [ <?pi ]> " ?> <!ENTITY a "]>"> ]>"#] {
            let deep = format!("{dt}{}{}", "<a>".repeat(9), "</a>".repeat(9));
            assert_eq!(kind(check_xml_keeping_dtd(deep.as_bytes(), &l)), Some(LimitKind::XmlDepth), "{dt}");
            let wide = format!("{dt}<r>{}</r>", "<b/>".repeat(30));
            assert_eq!(kind(check_xml_keeping_dtd(wide.as_bytes(), &l)), Some(LimitKind::XmlElements), "{dt}");
            assert_eq!(kind(check_xml(deep.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{dt}");
        }
        // an ENTITY declaration outside a DOCTYPE is refused in both modes
        assert_eq!(kind(check_xml_keeping_dtd(br#"<!ENTITY a "x"><r/>"#, &l)), Some(LimitKind::XmlEntities));
    }

    #[test]
    fn a_doctype_the_readers_end_elsewhere_is_refused() {
        let l = Limits { max_xml_depth: 2, max_xml_elements: 3, ..Limits::default() };
        let body = "<r><a><b><c/></b></a></r>";
        assert_eq!(kind(check_xml(body.as_bytes(), &l)), Some(LimitKind::XmlDepth));
        for pre in [
            "<!doctype r SYSTEM '>",
            "<!DOCTYPE r SYSTEM \">",
            "<!DoCtYpE r PUBLIC 'a>b' 'c'>",
            "<!DOCTYPE r [>",
            "<!DOCTYPE r [<!-- > -->",
            "<!DOCTYPE r [<!-- ] --> <x><y>",
            "<!DOCTYPE r [<!-- ' --> <?p ' ?> ]>",
            "<!DOCTYPE r SYSTEM 'a[b'>",
            "<!DOCTYPE r < >",
            "<!DOCTYPE r [ > ]>",
            "<!DOCTYPE r",
        ] {
            let k = kind(check_xml(format!("{pre}{body}").as_bytes(), &l));
            assert!(matches!(k, Some(LimitKind::XmlEntities | LimitKind::XmlDepth)), "{pre}: {k:?}");
            assert!(kind(check_xml(format!("{pre}{body}").as_bytes(), &Limits::default())).is_some(), "{pre} refused even with room");
        }
        // what real files carry still passes
        for ok in [
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">",
            "<!doctype html>",
            "<!DOCTYPE r []>",
            "<!DOCTYPE mb [ <!ELEMENT mb (msg)*> <!ATTLIST msg id CDATA #IMPLIED d CDATA 'x'> <!-- a note --> <?pi data?> ]>",
            "<!DOCTYPE r [ <!---->]>",
        ] {
            assert!(check_xml(format!("{ok}<r/>").as_bytes(), &Limits::default()).is_ok(), "{ok}");
        }
        // the keep-DTD check leaves this to its reader (the XML filter family refuses a mismatch itself)
        assert!(check_xml_keeping_dtd(b"<!DOCTYPE r [<!-- > --> ]><r/>", &Limits::default()).is_ok());
    }

    #[test]
    fn a_quote_in_a_comment_of_the_subset_passes_but_hides_no_entity() {
        // BILI-60: every reader skips a comment of the subset whole (quick-xml, the guard, the XML family), so an
        // apostrophe in it changes no reading
        let l = Limits::default();
        for ok in [
            "<!DOCTYPE r [ <!-- don't edit this file --> <!ELEMENT r ANY> ]><r/>",
            "<!DOCTYPE r [ <!-- the \"r\" element --> <!ATTLIST r id CDATA #IMPLIED> <!-- it's 'r' --> ]><r id='a'>x</r>",
            "<!DOCTYPE r [<!--'-->]><r/>",
        ] {
            assert!(check_xml(ok.as_bytes(), &l).is_ok(), "{ok}");
            assert!(check_xml_internal_entities(ok.as_bytes(), &l).is_ok(), "{ok}");
        }
        let svg = "<!DOCTYPE svg [ <!-- Adobe's namespaces --> <!ENTITY ns_svg \"http://www.w3.org/2000/svg\"> ]><svg/>";
        assert!(check_xml_internal_entities(svg.as_bytes(), &l).is_ok());
        assert_eq!(kind(check_xml(svg.as_bytes(), &l)), Some(LimitKind::XmlEntities));
        // an entity declaration next to, inside or after such a comment is still refused
        for bad in [
            "<!DOCTYPE r [ <!-- don't --> <!ENTITY a \"x\"> ]><r>&a;</r>",
            "<!DOCTYPE r [ <!ENTITY a \"x\"> <!-- don't --> ]><r>&a;</r>",
            "<!DOCTYPE r [ <!-- ' <!ENTITY a SYSTEM \"file:///etc/passwd\"> ' --> ]><r/>",
            "<!DOCTYPE r [ <!-- ' --><!ENTITY % p SYSTEM \"http://example.invalid/x.dtd\"> %p; ]><r/>",
            "<!DOCTYPE r [ <!-- ' --> ]><!ENTITY a \"x\"><r/>",
        ] {
            assert_eq!(kind(check_xml(bad.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{bad}");
        }
        for bad in [
            "<!DOCTYPE svg [ <!-- ' --> <!ENTITY a \"&b;&b;\"> ]><svg/>",
            "<!DOCTYPE svg [ <!-- ' <!ENTITY a SYSTEM \"file:///etc/passwd\"> --> ]><svg/>",
            "<!DOCTYPE svg [ <!-- ' --> <!ENTITY % p \"x\"> ]><svg/>",
        ] {
            assert_eq!(kind(check_xml_internal_entities(bad.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{bad}");
        }
        // a reader that pairs quotes but does not skip comments would read the subset on to the next quote: an
        // ENTITY anywhere after such a DOCTYPE (an attribute, a comment, CDATA) could be a declaration to it
        for bad in [
            "<!DOCTYPE r [ <!-- ' --> ]><r a='<!ENTITY b \"x\">'/>",
            "<!DOCTYPE r [ <!-- \" --> ]><r><!-- <!entity b 'x'> --></r>",
            "<!DOCTYPE r [ <!-- ' --> ]><r><![CDATA[<!ENTITY b 'x'>]]></r>",
        ] {
            assert_eq!(kind(check_xml(bad.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{bad}");
            assert_eq!(kind(check_xml_internal_entities(bad.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{bad}");
        }
        // a bracket or an angle bracket in the comment, or a quote in a processing instruction, is still refused
        for bad in [
            "<!DOCTYPE r [ <!-- ' ] --> ]><r/>",
            "<!DOCTYPE r [ <!-- \" > --> ]><r/>",
            "<!DOCTYPE r [ <!-- ' < --> ]><r/>",
            "<!DOCTYPE r [ <!-- ' [ --> ]><r/>",
            "<!DOCTYPE r [ <!-- ok --> <?pi don't ?> ]><r/>",
            "<!DOCTYPE r [ <!-- ' --> ",
        ] {
            assert_eq!(kind(check_xml(bad.as_bytes(), &l)), Some(LimitKind::XmlEntities), "{bad}");
        }
        // and the elements after it are counted as before
        let tight = Limits { max_xml_depth: 2, max_xml_elements: 3, ..Limits::default() };
        let deep = "<!DOCTYPE r [ <!-- don't --> ]><r><a><b><c/></b></a></r>";
        assert_eq!(kind(check_xml(deep.as_bytes(), &tight)), Some(LimitKind::XmlDepth));
        let wide = "<!DOCTYPE r [ <!-- don't --> ]><r><b/><b/><b/><b/></r>";
        assert_eq!(kind(check_xml(wide.as_bytes(), &tight)), Some(LimitKind::XmlElements));
    }

    #[test]
    fn every_reading_of_the_bytes_is_scanned() {
        let l = Limits { max_xml_depth: 2, max_xml_elements: 3, ..Limits::default() };
        let body = "<a><b><c/></b></a>";
        // 0x81 0x5D is one Shift_JIS / GBK character: the bytes end the CDATA at `]]>`, the decoded text later
        let sjis = format!("<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><r><![CDATA[\u{1}]]><!--]]>{body}--></r>");
        let mut bytes = sjis.into_bytes();
        let p = bytes.iter().position(|c| *c == 1).unwrap();
        bytes[p] = 0x81;
        assert!(check_xml(&bytes, &l).is_err());
        // a valid double-byte file still passes
        let ok = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><r><![CDATA[\x93\xfa]]>\x96\x7b</r>";
        assert!(check_xml(ok, &Limits::default()).is_ok());
        // a NUL first: UTF-16BE for one reader, UTF-8 for another
        let nul = format!("\0<r>{body}</r>");
        assert!(check_xml(nul.as_bytes(), &l).is_err());
        let mut le = Vec::new();
        for u in format!("<r>{body}</r>").encode_utf16() {
            le.extend(u.to_le_bytes());
        }
        assert!(check_xml(&le, &l).is_err());
        assert!(check_xml(&le, &Limits::default()).is_ok());
    }

    #[test]
    fn a_comment_or_pi_a_scanner_ends_early_is_refused() {
        let l = Limits::default();
        for bad in ["<!--><r><a/></r><!-- -->", "<!---><r/><!-- -->", "<?><r/><?x?>", "<r><![IGNORE[ <a/> ]]></r>", "<!ELEMENT r ANY><r/>"] {
            assert_eq!(kind(check_xml(bad.as_bytes(), &l)), Some(LimitKind::XmlElements), "{bad}");
        }
        for ok in ["<r><!----></r>", "<r><!-- - --></r>", "<r><??></r>", "<r><?pi x?></r>"] {
            assert!(check_xml(ok.as_bytes(), &l).is_ok(), "{ok}");
        }
    }

    #[test]
    fn a_plain_doctype_and_predefined_entities_pass() {
        let ok = br#"<?xml version="1.0"?><!DOCTYPE xliff PUBLIC "-//OASIS//DTD XLIFF 1.2//EN" "xliff.dtd"><a>x &amp; y &#233;</a>"#;
        assert!(check_xml(ok, &Limits::default()).is_ok());
        // an ENTITY inside a comment or CDATA is only text
        assert!(check_xml(b"<a><!-- <!ENTITY x 'y'> --><![CDATA[<!ENTITY z 'w'>]]></a>", &Limits::default()).is_ok());
    }

    #[test]
    fn xml_depth_and_element_count_are_bounded() {
        let l = Limits { max_xml_depth: 8, max_xml_elements: 20, ..Limits::default() };
        let deep = format!("{}{}", "<a>".repeat(9), "</a>".repeat(9));
        assert_eq!(kind(check_xml(deep.as_bytes(), &l)), Some(LimitKind::XmlDepth));
        let ok = format!("{}{}", "<a>".repeat(8), "</a>".repeat(8));
        assert!(check_xml(ok.as_bytes(), &l).is_ok());
        let wide = format!("<r>{}</r>", "<b/>".repeat(30));
        assert_eq!(kind(check_xml(wide.as_bytes(), &l)), Some(LimitKind::XmlElements));
    }

    #[test]
    fn utf16_xml_is_scanned_too() {
        let src = "<!DOCTYPE r [<!ENTITY a 'b'>]><r/>";
        let mut v = vec![0xFF, 0xFE];
        v.extend(src.encode_utf16().flat_map(|u| u.to_le_bytes()));
        assert_eq!(kind(check_xml(&v, &Limits::default())), Some(LimitKind::XmlEntities));
    }

    #[test]
    fn counts_elements_by_local_name() {
        let x = b"<w:body><w:p/><w:p w:x='1'>a</w:p><!-- <w:p/> --><w:pPr/><p>x</p></w:body>";
        assert_eq!(count_elements(x, "p"), 3);
    }

    #[test]
    fn own_caps_follow_a_raised_limit_only() {
        let h = Limits::new();
        // hosted and lowered: the smaller of the crate's own and the caller's, as before
        assert_eq!(own_cap(500_000, h.max_units, h.max_units), 50_000);
        assert_eq!(own_cap(20_000, h.max_units, h.max_units), 20_000);
        assert_eq!(own_cap(500_000, h.max_units, 10), 10);
        assert_eq!(own_cap_bytes(64 << 20, h.max_input_bytes, h.max_input_bytes), 64 << 20);
        assert_eq!(own_cap_bytes(256 << 20, h.max_input_bytes, h.max_input_bytes), 100 << 20);
        // raised: the caller's
        let l = Limits::large();
        assert_eq!(own_cap(500_000, h.max_units, l.max_units), l.max_units);
        assert_eq!(own_cap_bytes(64 << 20, h.max_input_bytes, l.max_input_bytes) as u64, l.max_input_bytes);
    }

    #[test]
    fn line_budget() {
        let l = Limits { max_lines: 5, ..Limits::default() };
        assert!(check_lines("a\nb\nc\n", &l).is_ok());
        assert_eq!(kind(check_lines(&"\n".repeat(6), &l)), Some(LimitKind::Lines));
    }
}
