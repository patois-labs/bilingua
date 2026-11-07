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
        }
    }
}

impl Limits {
    /// Localization OS's limits (the same as `Limits::default()`); change any of them with the `with_*` methods.
    pub fn new() -> Limits {
        Limits::default()
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

/// The check for a finished extraction: unit and segment counts, and the length of every unit.
pub fn check_document(doc: &Document, l: &Limits) -> Result<(), Error> {
    check_unit_count(doc.units.len(), l)?;
    check_unit_count(doc.segment_count(), l)?;
    for u in &doc.units {
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
pub fn check_xml(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    check_xml_with(bytes, l, false)
}

/// Like [`check_xml`] for document formats where a DTD with entity declarations is normal (DocBook, DITA, plain
/// XML): the declarations are allowed, and the depth and element limits still apply. This is safe only for a reader
/// that never expands or fetches an entity, as the XML filter family does: it keeps every `&name;` as an opaque
/// code and writes the DOCTYPE back byte for byte.
pub fn check_xml_keeping_dtd(bytes: &[u8], l: &Limits) -> Result<(), Error> {
    check_xml_with(bytes, l, true)
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

fn check_xml_with(bytes: &[u8], l: &Limits, keep_dtd: bool) -> Result<(), Error> {
    let data = markup_bytes(bytes);
    let b: &[u8] = &data;
    let (mut i, mut depth, mut elements) = (0usize, 0usize, 0usize);
    while i < b.len() {
        let Some(p) = b[i..].iter().position(|c| *c == b'<') else { break };
        i += p;
        let rest = &b[i..];
        if rest.starts_with(b"<!--") {
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
            match find(b, i + 2, b"?>") {
                Some(e) => i = e + 2,
                None => break,
            }
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            let end = doctype_end(b, i);
            if !keep_dtd && contains_ci(&b[i..end], b"<!ENTITY") {
                return Err(unsafe_xml(
                    "This file declares XML entities (a DTD), which could expand without limit or read other files. It was not processed.",
                ));
            }
            i = end + 1;
        } else if rest.starts_with(b"<!") {
            // another declaration outside a DOCTYPE
            if rest.len() >= 8 && rest[..8].eq_ignore_ascii_case(b"<!ENTITY") {
                return Err(unsafe_xml("This file declares XML entities, which are not allowed. It was not processed."));
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
    fn line_budget() {
        let l = Limits { max_lines: 5, ..Limits::default() };
        assert!(check_lines("a\nb\nc\n", &l).is_ok());
        assert_eq!(kind(check_lines(&"\n".repeat(6), &l)), Some(LimitKind::Lines));
    }
}
